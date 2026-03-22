use chrono::Utc;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::cache::{AssetQuery, AssetStore};
use crate::commands::{AppCommand, dispatch_command};
use crate::config::BrazenConfig;
use crate::engine::{
    AlphaMode, BrowserEngine, BrowserTab, DialogKind, EngineEvent, EngineFactory, EngineStatus,
    FocusState, InputEvent, RenderSurfaceHandle, RenderSurfaceMetadata, SecurityWarningKind,
    WindowDisposition,
};
use crate::navigation::{normalize_url_input, resolve_startup_url};
use crate::permissions::Capability;
use crate::platform_paths::RuntimePaths;
use crate::rendering::normalize_pixels;
use crate::session::{NavigationEntry, SessionSnapshot, load_session, save_session};

#[derive(Debug, Clone)]
pub struct ShellState {
    pub app_name: String,
    pub backend_name: String,
    pub engine_instance_id: u64,
    pub engine_status: EngineStatus,
    pub active_tab: BrowserTab,
    pub address_bar_input: String,
    pub page_title: String,
    pub load_progress: f32,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub document_ready: bool,
    pub favicon_url: Option<String>,
    pub metadata_summary: Option<String>,
    pub history: Vec<String>,
    pub last_committed_url: Option<String>,
    pub was_minimized: bool,
    pub pending_popup: Option<(String, WindowDisposition)>,
    pub pending_dialog: Option<(DialogKind, String)>,
    pub pending_context_menu: Option<(f32, f32)>,
    pub pending_new_window: Option<(String, WindowDisposition)>,
    pub last_download: Option<String>,
    pub last_security_warning: Option<(SecurityWarningKind, String)>,
    pub last_crash: Option<String>,
    pub last_crash_dump: Option<String>,
    pub devtools_endpoint: Option<String>,
    pub engine_verbose_logging: bool,
    pub session: SessionSnapshot,
    pub event_log: Vec<String>,
    pub log_panel_open: bool,
    pub permission_panel_open: bool,
    pub capabilities_snapshot: Vec<(String, String)>,
    pub runtime_paths: RuntimePaths,
}

impl ShellState {
    pub fn record_event(&mut self, event: impl Into<String>) {
        self.event_log.push(event.into());
    }

    pub fn sync_from_engine(&mut self, engine: &mut dyn BrowserEngine) {
        self.engine_instance_id = engine.instance_id();
        self.backend_name = engine.backend_name().to_string();
        self.engine_status = engine.status();
        self.active_tab = engine.active_tab().clone();
        for event in engine.take_events() {
            match event {
                EngineEvent::StatusChanged(status) => {
                    self.engine_status = status.clone();
                    self.record_event(format!("status: {status}"));
                }
                EngineEvent::NavigationStateUpdated(state) => {
                    self.page_title = state.title.clone();
                    self.load_progress = state.load_progress;
                    self.can_go_back = state.can_go_back;
                    self.can_go_forward = state.can_go_forward;
                    self.document_ready = state.document_ready;
                    self.favicon_url = state.favicon_url.clone();
                    self.metadata_summary = state.metadata_summary.clone();
                    if self
                        .last_committed_url
                        .as_ref()
                        .map(|value| value != &state.url)
                        .unwrap_or(true)
                    {
                        self.history.push(state.url.clone());
                        self.last_committed_url = Some(state.url.clone());
                        let entry = NavigationEntry {
                            url: state.url.clone(),
                            title: state.title.clone(),
                            timestamp: Utc::now().to_rfc3339(),
                            redirect_chain: state.redirect_chain.clone(),
                        };
                        self.session.commit_navigation(entry);
                    }
                    self.record_event(format!(
                        "nav: {} ({:.0}%)",
                        state.url,
                        state.load_progress * 100.0
                    ));
                }
                EngineEvent::ClipboardRequested(request) => {
                    self.record_event(format!("clipboard request: {request:?}"));
                }
                EngineEvent::NavigationFailed { input, reason } => {
                    self.record_event(format!("navigation failed: {input} ({reason})"));
                }
                EngineEvent::DevtoolsReady { endpoint } => {
                    self.devtools_endpoint = Some(endpoint.clone());
                    self.record_event(format!("devtools ready: {endpoint}"));
                }
                EngineEvent::PopupRequested { url, disposition } => {
                    self.pending_popup = Some((url.clone(), disposition.clone()));
                    self.record_event(format!("popup requested: {url} ({disposition:?})"));
                }
                EngineEvent::DialogRequested { kind, message } => {
                    self.pending_dialog = Some((kind.clone(), message.clone()));
                    self.record_event(format!("dialog requested: {kind:?}"));
                }
                EngineEvent::ContextMenuRequested { x, y } => {
                    self.pending_context_menu = Some((x, y));
                    self.record_event(format!("context menu requested: {x:.0},{y:.0}"));
                }
                EngineEvent::NewWindowRequested { url, disposition } => {
                    self.pending_new_window = Some((url.clone(), disposition.clone()));
                    self.record_event(format!("new window requested: {url} ({disposition:?})"));
                }
                EngineEvent::DownloadRequested {
                    url,
                    suggested_path,
                } => {
                    let message = suggested_path
                        .as_ref()
                        .map(|path| format!("{url} -> {path}"))
                        .unwrap_or_else(|| url.clone());
                    self.last_download = Some(message.clone());
                    self.session
                        .active_tab_mut()
                        .downloads
                        .push(message.clone());
                    self.record_event(format!("download requested: {message}"));
                }
                EngineEvent::SecurityWarning { kind, url } => {
                    self.last_security_warning = Some((kind.clone(), url.clone()));
                    self.record_event(format!("security warning: {kind:?} {url}"));
                }
                EngineEvent::Crashed { reason } => {
                    self.last_crash = Some(reason.clone());
                    self.session.crash_recovery_pending = true;
                    self.record_event(format!("engine crashed: {reason}"));
                }
                other => {
                    self.record_event(format!("engine event: {other:?}"));
                }
            }
        }
    }
}

pub fn build_shell_state(
    config: &BrazenConfig,
    paths: &RuntimePaths,
    engine_factory: &dyn EngineFactory,
) -> ShellState {
    let mut engine = engine_factory.create(config, paths);
    engine.set_render_surface(RenderSurfaceMetadata {
        viewport_width: config.window.initial_width as u32,
        viewport_height: config.window.initial_height as u32,
        scale_factor_basis_points: 100,
    });

    let capabilities_snapshot = vec![
        (
            Capability::TerminalExec.label().to_string(),
            format!(
                "{:?}",
                config.permissions.decision_for(&Capability::TerminalExec)
            ),
        ),
        (
            Capability::DomRead.label().to_string(),
            format!(
                "{:?}",
                config.permissions.decision_for(&Capability::DomRead)
            ),
        ),
        (
            Capability::CacheRead.label().to_string(),
            format!(
                "{:?}",
                config.permissions.decision_for(&Capability::CacheRead)
            ),
        ),
        (
            Capability::TabInspect.label().to_string(),
            format!(
                "{:?}",
                config.permissions.decision_for(&Capability::TabInspect)
            ),
        ),
        (
            Capability::AiToolUse.label().to_string(),
            format!(
                "{:?}",
                config.permissions.decision_for(&Capability::AiToolUse)
            ),
        ),
    ];

    let session = load_session(&paths.session_path).unwrap_or_else(|_| {
        SessionSnapshot::new(
            config.profiles.active_profile.clone(),
            Utc::now().to_rfc3339(),
        )
    });
    let startup_url = resolve_startup_url(&config.engine.startup_url)
        .ok()
        .flatten();

    let mut shell_state = ShellState {
        app_name: config.app.name.clone(),
        backend_name: engine.backend_name().to_string(),
        engine_instance_id: engine.instance_id(),
        engine_status: engine.status(),
        active_tab: engine.active_tab().clone(),
        address_bar_input: startup_url
            .clone()
            .unwrap_or_else(|| config.app.homepage.clone()),
        page_title: engine.active_tab().title.clone(),
        load_progress: 0.0,
        can_go_back: false,
        can_go_forward: false,
        document_ready: false,
        favicon_url: None,
        metadata_summary: None,
        history: Vec::new(),
        last_committed_url: None,
        was_minimized: false,
        pending_popup: None,
        pending_dialog: None,
        pending_context_menu: None,
        pending_new_window: None,
        last_download: None,
        last_security_warning: None,
        last_crash: None,
        last_crash_dump: None,
        devtools_endpoint: None,
        engine_verbose_logging: config.engine.verbose_logging,
        session,
        event_log: vec![
            format!("loaded config for {}", config.app.name),
            format!("data dir: {}", paths.data_dir.display()),
            format!("logs dir: {}", paths.logs_dir.display()),
            format!("session path: {}", paths.session_path.display()),
            format!(
                "engine policy: new-window={}, devtools={}, transport={}",
                config.engine.new_window_policy,
                config.engine.devtools_enabled,
                config.engine.devtools_transport
            ),
            format!(
                "engine limits: memory={}mb cpu={}ms max_tabs={}",
                config.engine.resource_limits.memory_mb,
                config.engine.resource_limits.cpu_ms,
                config.engine.resource_limits.max_tabs
            ),
            format!(
                "startup url: {}",
                startup_url.as_deref().unwrap_or("about:blank (disabled)")
            ),
        ],
        log_panel_open: config.window.show_log_panel_on_startup,
        permission_panel_open: config.window.show_permission_panel_on_startup,
        capabilities_snapshot,
        runtime_paths: paths.clone(),
    };

    shell_state.sync_from_engine(engine.as_mut());
    shell_state
}

pub struct BrazenApp {
    config: BrazenConfig,
    shell_state: ShellState,
    engine: Box<dyn BrowserEngine>,
    engine_factory: crate::engine::ServoEngineFactory,
    surface_handle: RenderSurfaceHandle,
    last_surface: Option<RenderSurfaceMetadata>,
    render_texture: Option<eframe::egui::TextureHandle>,
    render_frame_number: Option<u64>,
    render_frame_size: Option<(u32, u32)>,
    render_frame_format: Option<(
        crate::engine::PixelFormat,
        AlphaMode,
        crate::engine::ColorSpace,
    )>,
    frame_times: VecDeque<f32>,
    last_frame_instant: Option<Instant>,
    last_frame_ms: Option<f32>,
    upload_times: VecDeque<f32>,
    last_upload_ms: Option<f32>,
    last_pointer_pos: Option<eframe::egui::Pos2>,
    capture_next_frame: bool,
    pending_restart_at: Option<chrono::DateTime<Utc>>,
    crash_count: u32,
    cache_store: AssetStore,
    cache_query_url: String,
    cache_query_mime: String,
    cache_query_hash: String,
    cache_query_session: String,
    cache_export_path: String,
    cache_import_path: String,
    cache_manifest_path: String,
    pending_startup_url: Option<String>,
}

impl BrazenApp {
    pub fn new(config: BrazenConfig, shell_state: ShellState) -> Self {
        let engine_factory = crate::engine::ServoEngineFactory;
        let mut engine = engine_factory.create(&config, &shell_state.runtime_paths);
        engine.set_verbose_logging(config.engine.verbose_logging);
        engine.configure_devtools(
            config.engine.devtools_enabled,
            &config.engine.devtools_transport,
        );
        let surface_handle = RenderSurfaceHandle {
            id: 1,
            label: "primary-surface".to_string(),
        };
        let cache_store = AssetStore::load(
            config.cache.clone(),
            &shell_state.runtime_paths,
            config.profiles.active_profile.clone(),
        );
        let capture_next_frame = config.engine.debug_capture_next_frame;
        let pending_startup_url = resolve_startup_url(&config.engine.startup_url)
            .ok()
            .flatten();

        Self {
            config,
            shell_state,
            engine,
            engine_factory,
            surface_handle,
            last_surface: None,
            render_texture: None,
            render_frame_number: None,
            render_frame_size: None,
            render_frame_format: None,
            frame_times: VecDeque::with_capacity(120),
            last_frame_instant: None,
            last_frame_ms: None,
            upload_times: VecDeque::with_capacity(120),
            last_upload_ms: None,
            last_pointer_pos: None,
            capture_next_frame,
            pending_restart_at: None,
            crash_count: 0,
            cache_store,
            cache_query_url: String::new(),
            cache_query_mime: String::new(),
            cache_query_hash: String::new(),
            cache_query_session: String::new(),
            cache_export_path: "cache-export.json".to_string(),
            cache_import_path: "cache-import.json".to_string(),
            cache_manifest_path: "cache-manifest.json".to_string(),
            pending_startup_url,
        }
    }

    pub fn shell_state(&self) -> &ShellState {
        &self.shell_state
    }

    fn handle_navigation(&mut self) {
        let input = self.shell_state.address_bar_input.trim().to_string();
        self.shell_state
            .session
            .mark_pending_navigation(&input, Utc::now().to_rfc3339());
        let _ = dispatch_command(
            &mut self.shell_state,
            self.engine.as_mut(),
            AppCommand::NavigateTo(input),
        );
        self.shell_state.sync_from_engine(self.engine.as_mut());
    }

    fn update_render_surface(&mut self, ctx: &eframe::egui::Context) {
        let screen_rect = ctx.content_rect();
        let pixels_per_point = ctx.pixels_per_point();
        let metadata = RenderSurfaceMetadata {
            viewport_width: (screen_rect.width() * pixels_per_point) as u32,
            viewport_height: (screen_rect.height() * pixels_per_point) as u32,
            scale_factor_basis_points: (pixels_per_point * 100.0) as u32,
        };

        if self.last_surface.as_ref() != Some(&metadata) {
            tracing::info!(
                target: "brazen::render",
                viewport_width = metadata.viewport_width,
                viewport_height = metadata.viewport_height,
                pixels_per_point,
                scale_factor = metadata.scale_factor_basis_points,
                "render surface updated"
            );
            self.engine.attach_surface(self.surface_handle.clone());
            self.engine.set_render_surface(metadata.clone());
            self.last_surface = Some(metadata);
            if let Some(startup_url) = self.pending_startup_url.take() {
                if let Ok(normalized) = normalize_url_input(&startup_url) {
                    self.shell_state
                        .record_event(format!("startup navigation: {normalized}"));
                    self.engine.navigate(&normalized);
                } else {
                    self.shell_state
                        .record_event(format!("startup navigation failed: {startup_url}"));
                }
            }
        }
    }

    fn update_render_frame(&mut self, ctx: &eframe::egui::Context) {
        let Some(frame) = self.engine.render_frame() else {
            return;
        };
        self.render_frame_format = Some((frame.pixel_format, frame.alpha_mode, frame.color_space));
        if let Some(surface) = &self.last_surface {
            if surface.viewport_width != frame.width || surface.viewport_height != frame.height {
                tracing::warn!(
                    target: "brazen::render",
                    expected_width = surface.viewport_width,
                    expected_height = surface.viewport_height,
                    frame_width = frame.width,
                    frame_height = frame.height,
                    "frame size differs from render surface"
                );
            }
        }
        let pixels = normalize_pixels(&frame, self.config.engine.debug_bypass_swizzle);
        if pixels.is_empty() {
            return;
        }
        let size = [frame.width as usize, frame.height as usize];
        let image = match frame.alpha_mode {
            AlphaMode::Premultiplied => {
                eframe::egui::ColorImage::from_rgba_premultiplied(size, &pixels)
            }
            AlphaMode::Straight => eframe::egui::ColorImage::from_rgba_unmultiplied(size, &pixels),
        };
        let options = eframe::egui::TextureOptions::LINEAR;
        let upload_start = Instant::now();
        tracing::trace!(
            target: "brazen::render",
            frame_number = frame.frame_number,
            width = frame.width,
            height = frame.height,
            bytes = pixels.len(),
            alpha_mode = frame.alpha_mode.as_str(),
            pixel_format = frame.pixel_format.as_str(),
            color_space = frame.color_space.as_str(),
            "uploading frame to egui"
        );
        match self.render_texture.as_mut() {
            Some(texture) => {
                if texture.size() != size {
                    *texture = ctx.load_texture("brazen-render", image, options);
                } else {
                    texture.set(image, options);
                }
            }
            None => {
                self.render_texture = Some(ctx.load_texture("brazen-render", image, options));
            }
        }
        self.render_frame_number = Some(frame.frame_number);
        self.render_frame_size = Some((frame.width, frame.height));
        let upload_ms = upload_start.elapsed().as_secs_f32() * 1000.0;
        self.last_upload_ms = Some(upload_ms);
        if self.upload_times.len() == 120 {
            self.upload_times.pop_front();
        }
        self.upload_times.push_back(upload_ms);
        let now = Instant::now();
        if let Some(previous) = self.last_frame_instant {
            let ms = (now - previous).as_secs_f32() * 1000.0;
            self.last_frame_ms = Some(ms);
            if self.frame_times.len() == 120 {
                self.frame_times.pop_front();
            }
            self.frame_times.push_back(ms);
        }
        self.last_frame_instant = Some(now);
        if self.capture_next_frame {
            self.capture_next_frame = false;
            self.capture_frame_to_disk(&frame, &pixels);
        }
        match self.config.engine.frame_pacing.as_str() {
            "manual" => ctx.request_repaint_after(Duration::from_millis(16)),
            "on-demand" => {}
            _ => ctx.request_repaint(),
        }
    }

    fn capture_frame_to_disk(&self, frame: &crate::engine::EngineFrame, pixels: &[u8]) {
        #[cfg(not(feature = "servo-upstream"))]
        let _ = pixels;
        let dir = self.resolve_capture_dir();
        if let Err(error) = std::fs::create_dir_all(&dir) {
            tracing::warn!(
                target: "brazen::render",
                path = %dir.display(),
                %error,
                "failed to create capture directory"
            );
            return;
        }
        let filename = format!(
            "brazen-frame-{}-{}x{}.png",
            frame.frame_number, frame.width, frame.height
        );
        let path = dir.join(filename);
        #[cfg(feature = "servo-upstream")]
        {
            let image = libservo::RgbaImage::from_raw(frame.width, frame.height, pixels.to_vec());
            match image {
                Some(image) => {
                    if let Err(error) = image.save(&path) {
                        tracing::warn!(
                            target: "brazen::render",
                            path = %path.display(),
                            %error,
                            "failed to write frame capture"
                        );
                    } else {
                        tracing::info!(
                            target: "brazen::render",
                            path = %path.display(),
                            "saved frame capture"
                        );
                    }
                }
                None => tracing::warn!(
                    target: "brazen::render",
                    path = %path.display(),
                    "failed to build capture image"
                ),
            }
        }
        #[cfg(not(feature = "servo-upstream"))]
        {
            tracing::warn!(
                target: "brazen::render",
                path = %path.display(),
                "frame capture requires the servo-upstream feature"
            );
        }
    }

    fn resolve_capture_dir(&self) -> std::path::PathBuf {
        let choice = self.config.engine.debug_capture_dir.trim();
        match choice {
            "" | "logs" => self.shell_state.runtime_paths.logs_dir.clone(),
            "data" => self.shell_state.runtime_paths.data_dir.clone(),
            "profiles" => self.shell_state.runtime_paths.profiles_dir.clone(),
            "cache" => self.shell_state.runtime_paths.cache_dir.clone(),
            "downloads" => self.shell_state.runtime_paths.downloads_dir.clone(),
            "crash_dumps" => self.shell_state.runtime_paths.crash_dumps_dir.clone(),
            value => std::path::PathBuf::from(value),
        }
    }

    fn forward_input_events(&mut self, ctx: &eframe::egui::Context) {
        let input = ctx.input(|input| input.clone());
        let focused = if input.raw.focused {
            FocusState::Focused
        } else {
            FocusState::Unfocused
        };
        self.engine.set_focus(focused);

        if let Some(minimized) = input.raw.viewport().minimized {
            if minimized && !self.shell_state.was_minimized {
                self.engine.suspend();
                self.shell_state.was_minimized = true;
            } else if !minimized && self.shell_state.was_minimized {
                self.engine.resume();
                self.shell_state.was_minimized = false;
            }
        }

        for event in input.raw.events {
            match event {
                eframe::egui::Event::PointerMoved(pos) => {
                    self.last_pointer_pos = Some(pos);
                    self.engine
                        .handle_input(InputEvent::PointerMove { x: pos.x, y: pos.y });
                }
                eframe::egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    ..
                } => {
                    self.last_pointer_pos = Some(pos);
                    let button_id = match button {
                        eframe::egui::PointerButton::Primary => 0,
                        eframe::egui::PointerButton::Secondary => 1,
                        eframe::egui::PointerButton::Middle => 2,
                        eframe::egui::PointerButton::Extra1 => 3,
                        eframe::egui::PointerButton::Extra2 => 4,
                    };
                    if pressed {
                        self.engine
                            .handle_input(InputEvent::PointerDown { button: button_id });
                    } else {
                        self.engine
                            .handle_input(InputEvent::PointerUp { button: button_id });
                    }
                }
                eframe::egui::Event::MouseWheel { delta, .. } => {
                    self.engine.handle_input(InputEvent::Scroll {
                        delta_x: delta.x,
                        delta_y: delta.y,
                    });
                }
                eframe::egui::Event::Zoom(delta) => {
                    self.engine.handle_input(InputEvent::Zoom { delta });
                }
                eframe::egui::Event::Key { key, pressed, .. } => {
                    let key_name = format!("{key:?}");
                    let modifiers = crate::engine::KeyModifiers {
                        alt: input.modifiers.alt,
                        ctrl: input.modifiers.ctrl,
                        shift: input.modifiers.shift,
                        command: input.modifiers.command,
                    };
                    if pressed {
                        self.engine.handle_input(InputEvent::KeyDown {
                            key: key_name,
                            modifiers,
                        });
                    } else {
                        self.engine.handle_input(InputEvent::KeyUp {
                            key: key_name,
                            modifiers,
                        });
                    }
                }
                eframe::egui::Event::Text(text) => {
                    self.engine.handle_input(InputEvent::TextInput { text });
                }
                eframe::egui::Event::Ime(ime) => match ime {
                    eframe::egui::ImeEvent::Enabled => {
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionStart);
                    }
                    eframe::egui::ImeEvent::Preedit(text) => {
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionUpdate { text });
                    }
                    eframe::egui::ImeEvent::Commit(text) => {
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionEnd { text });
                    }
                    eframe::egui::ImeEvent::Disabled => {
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionEnd {
                                text: String::new(),
                            });
                    }
                },
                eframe::egui::Event::Copy | eframe::egui::Event::Cut => {
                    self.engine
                        .handle_clipboard(crate::engine::ClipboardRequest::Read);
                }
                eframe::egui::Event::Paste(text) => {
                    self.engine
                        .handle_clipboard(crate::engine::ClipboardRequest::Write(text));
                }
                _ => {}
            }
        }
    }

    fn sync_active_tab_from_session(&mut self) {
        let tab = self.shell_state.session.active_tab_mut().clone();
        if self.shell_state.active_tab.current_url != tab.url
            || self.shell_state.active_tab.title != tab.title
        {
            self.shell_state.active_tab.title = tab.title;
            self.shell_state.active_tab.current_url = tab.url;
        }
    }

    fn apply_new_window_policy(&mut self) {
        let Some((url, disposition)) = self.shell_state.pending_new_window.take() else {
            return;
        };
        let policy = self.config.engine.new_window_policy.as_str();
        let decision = match policy {
            "new-tab" => WindowDisposition::BackgroundTab,
            "same-tab" => WindowDisposition::ForegroundTab,
            "block" => WindowDisposition::Blocked,
            _ => disposition.clone(),
        };

        match decision {
            WindowDisposition::ForegroundTab => {
                if let Ok(normalized) = normalize_url_input(&url) {
                    self.engine.navigate(&normalized);
                    self.shell_state
                        .record_event(format!("new window routed to current tab: {normalized}"));
                } else {
                    self.shell_state
                        .record_event(format!("new window navigation failed: {url}"));
                }
            }
            WindowDisposition::BackgroundTab | WindowDisposition::NewWindow => {
                if let Ok(normalized) = normalize_url_input(&url) {
                    self.shell_state
                        .session
                        .open_new_tab(&normalized, "New Tab");
                    self.shell_state
                        .record_event(format!("new window opened as tab: {normalized}"));
                } else {
                    self.shell_state
                        .record_event(format!("new window tab open failed: {url}"));
                }
            }
            WindowDisposition::Blocked => {
                self.shell_state
                    .record_event(format!("new window blocked: {url}"));
            }
        }
    }

    fn write_crash_dump(&mut self, reason: &str) {
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let filename = format!("crash-{timestamp}.log");
        let path = self
            .shell_state
            .runtime_paths
            .crash_dumps_dir
            .join(filename);
        let _ = std::fs::create_dir_all(&self.shell_state.runtime_paths.crash_dumps_dir);
        let payload = format!(
            "timestamp={}\nreason={}\nsession_id={}\nprofile={}\nactive_url={}\n",
            timestamp,
            reason,
            self.shell_state.session.session_id.0,
            self.shell_state.session.profile_id,
            self.shell_state.active_tab.current_url
        );
        let _ = std::fs::write(&path, payload.as_bytes());
        self.shell_state.last_crash_dump = Some(path.display().to_string());
    }

    fn restart_engine(&mut self) {
        self.engine.shutdown();
        self.engine = self
            .engine_factory
            .create(&self.config, &self.shell_state.runtime_paths);
        self.engine
            .set_verbose_logging(self.shell_state.engine_verbose_logging);
        self.engine.configure_devtools(
            self.config.engine.devtools_enabled,
            &self.config.engine.devtools_transport,
        );
        self.last_surface = None;
        self.render_texture = None;
        self.shell_state.record_event("engine restarted");
    }

    fn schedule_restart(&mut self) {
        if self.pending_restart_at.is_some() {
            return;
        }
        self.crash_count = self.crash_count.saturating_add(1);
        let exponent = self.crash_count.min(5);
        let backoff = 2u64.pow(exponent);
        let delay = chrono::Duration::seconds(backoff as i64);
        let scheduled = Utc::now() + delay;
        self.pending_restart_at = Some(scheduled);
        self.shell_state
            .record_event(format!("engine restart scheduled in {backoff}s"));
    }

    fn handle_crash_recovery(&mut self) {
        if self.shell_state.last_crash.is_some() {
            self.schedule_restart();
        }
        if let Some(scheduled) = self.pending_restart_at {
            if Utc::now() >= scheduled {
                self.restart_engine();
                self.shell_state.last_crash = None;
                self.shell_state.session.crash_recovery_pending = false;
                self.pending_restart_at = None;
            }
        }
    }

    fn render_cache_panel(&mut self, ui: &mut eframe::egui::Ui) {
        ui.separator();
        ui.heading("Cache");
        ui.horizontal(|ui| {
            if ui.button("Sim Capture").clicked() {
                let mut headers = std::collections::BTreeMap::new();
                headers.insert("content-type".to_string(), "text/html".to_string());
                let session_id = Some(self.shell_state.session.session_id.0.to_string());
                let tab_id = Some(self.shell_state.session.active_tab_mut().id.0.to_string());
                let _ = self.cache_store.record_asset(
                    &self.shell_state.active_tab.current_url,
                    "text/html",
                    Some(b"<html><body>Brazen</body></html>"),
                    headers,
                    false,
                    false,
                    session_id,
                    tab_id,
                    Some("request-1".to_string()),
                );
                self.shell_state.record_event("cache capture simulated");
            }
            if ui.button("Export").clicked() {
                if self
                    .cache_store
                    .export_json(self.cache_export_path.as_ref())
                    .is_ok()
                {
                    self.shell_state.record_event("cache export complete");
                }
            }
            if ui.button("Import").clicked() {
                if self
                    .cache_store
                    .import_json(self.cache_import_path.as_ref())
                    .is_ok()
                {
                    self.shell_state.record_event("cache import complete");
                }
            }
            if ui.button("Manifest").clicked() {
                if self
                    .cache_store
                    .build_replay_manifest(self.cache_manifest_path.as_ref())
                    .is_ok()
                {
                    self.shell_state.record_event("cache manifest written");
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("URL");
            ui.text_edit_singleline(&mut self.cache_query_url);
        });
        ui.horizontal(|ui| {
            ui.label("MIME");
            ui.text_edit_singleline(&mut self.cache_query_mime);
        });
        ui.horizontal(|ui| {
            ui.label("Hash");
            ui.text_edit_singleline(&mut self.cache_query_hash);
        });
        ui.horizontal(|ui| {
            ui.label("Session");
            ui.text_edit_singleline(&mut self.cache_query_session);
        });

        let query = AssetQuery {
            url: empty_to_none(&self.cache_query_url),
            mime: empty_to_none(&self.cache_query_mime),
            hash: empty_to_none(&self.cache_query_hash),
            session_id: empty_to_none(&self.cache_query_session),
        };
        let results = self.cache_store.query(query);
        ui.label(format!(
            "Assets: {} (storage: {:?})",
            self.cache_store.entries().len(),
            self.cache_store.storage_mode()
        ));
        ui.label(format!("Matches: {}", results.len()));
        for entry in results.iter().rev().take(5) {
            ui.horizontal(|ui| {
                ui.label(format!("{} {}", entry.mime, entry.url));
                if let Some(hash) = &entry.hash {
                    if entry.pinned {
                        if ui.button("Unpin").clicked() {
                            let _ = self.cache_store.unpin_asset(hash);
                            self.shell_state.record_event("asset unpinned");
                        }
                    } else if ui.button("Pin").clicked() {
                        let _ = self.cache_store.pin_asset(hash);
                        self.shell_state.record_event("asset pinned");
                    }
                }
            });
        }
    }
}

fn empty_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

impl eframe::App for BrazenApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.update_render_surface(ctx);
        self.forward_input_events(ctx);
        self.update_render_frame(ctx);
        self.shell_state.sync_from_engine(self.engine.as_mut());
        self.apply_new_window_policy();
        if let Some(reason) = self.shell_state.last_crash.clone() {
            if self.shell_state.last_crash_dump.is_none() {
                self.write_crash_dump(&reason);
            }
        }
        self.handle_crash_recovery();

        eframe::egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&self.config.app.name);
                ui.label(format!("backend: {}", self.shell_state.backend_name));
                ui.label(format!("engine: {}", self.shell_state.engine_instance_id));
                ui.separator();
                ui.label(format!("status: {}", self.shell_state.engine_status));
                ui.separator();
                ui.label(format!("title: {}", self.shell_state.page_title));
                ui.separator();
                ui.label(format!(
                    "ready: {}",
                    if self.shell_state.document_ready {
                        "yes"
                    } else {
                        "no"
                    }
                ));
            });

            ui.horizontal(|ui| {
                ui.add_enabled_ui(self.shell_state.can_go_back, |ui| {
                    if ui.button("Back").clicked() {
                        let _ = dispatch_command(
                            &mut self.shell_state,
                            self.engine.as_mut(),
                            AppCommand::GoBack,
                        );
                        self.shell_state.session.go_back(Utc::now().to_rfc3339());
                    }
                });
                ui.add_enabled_ui(self.shell_state.can_go_forward, |ui| {
                    if ui.button("Forward").clicked() {
                        let _ = dispatch_command(
                            &mut self.shell_state,
                            self.engine.as_mut(),
                            AppCommand::GoForward,
                        );
                        self.shell_state.session.go_forward(Utc::now().to_rfc3339());
                    }
                });
                let response = ui.text_edit_singleline(&mut self.shell_state.address_bar_input);
                let enter_pressed = ui.input(|input| input.key_pressed(eframe::egui::Key::Enter));
                if response.lost_focus() && enter_pressed {
                    self.handle_navigation();
                }
                if ui.button("Go").clicked() {
                    self.handle_navigation();
                }
                if ui.button("Reload").clicked() {
                    let _ = dispatch_command(
                        &mut self.shell_state,
                        self.engine.as_mut(),
                        AppCommand::ReloadActiveTab,
                    );
                }
                if ui.button("Stop").clicked() {
                    let _ = dispatch_command(
                        &mut self.shell_state,
                        self.engine.as_mut(),
                        AppCommand::StopLoading,
                    );
                }
                if ui.button("Restart Engine").clicked() {
                    self.restart_engine();
                }
                if ui.button("Logs").clicked() {
                    let _ = dispatch_command(
                        &mut self.shell_state,
                        self.engine.as_mut(),
                        AppCommand::ToggleLogPanel,
                    );
                }
                if ui.button("Permissions").clicked() {
                    let _ = dispatch_command(
                        &mut self.shell_state,
                        self.engine.as_mut(),
                        AppCommand::OpenPermissionPanel,
                    );
                }
            });
            ui.add(
                eframe::egui::ProgressBar::new(self.shell_state.load_progress)
                    .show_percentage()
                    .desired_width(f32::INFINITY),
            );
        });

        eframe::egui::SidePanel::left("tab_sidebar")
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.heading("Workspace");
                ui.horizontal(|ui| {
                    if ui.button("New Tab").clicked() {
                        self.shell_state
                            .session
                            .open_new_tab("about:blank", "New Tab");
                    }
                    if ui.button("Duplicate").clicked() {
                        self.shell_state.session.duplicate_active_tab();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        self.shell_state.session.close_active_tab();
                    }
                    if ui.button("Pin").clicked() {
                        self.shell_state.session.toggle_pin_active_tab();
                    }
                    if ui.button("Mute").clicked() {
                        self.shell_state.session.toggle_mute_active_tab();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Save Session").clicked() {
                        if save_session(
                            &self.shell_state.runtime_paths.session_path,
                            &self.shell_state.session,
                        )
                        .is_ok()
                        {
                            self.shell_state.record_event("session saved");
                        }
                    }
                    if ui.button("Load Session").clicked() {
                        if let Ok(session) =
                            load_session(&self.shell_state.runtime_paths.session_path)
                        {
                            self.shell_state.session = session;
                            let tab = self.shell_state.session.active_tab_mut().clone();
                            self.shell_state.address_bar_input = tab.url.clone();
                            self.shell_state.record_event("session loaded");
                        }
                    }
                });
                ui.label(format!(
                    "Session: {}",
                    self.shell_state.session.session_id.0
                ));
                ui.label(format!("Profile: {}", self.shell_state.session.profile_id));
                ui.label(format!(
                    "Crash recovery: {}",
                    if self.shell_state.session.crash_recovery_pending {
                        "pending"
                    } else {
                        "clear"
                    }
                ));
                ui.separator();
                let active_window = self.shell_state.session.active_window;
                if let Some(window) = self.shell_state.session.windows.get(active_window) {
                    let active_index = window.active_tab;
                    let tabs = window.tabs.clone();
                    for (index, tab) in tabs.iter().enumerate() {
                        let label = format!(
                            "{}{} {}",
                            if index == active_index { ">" } else { " " },
                            if tab.pinned { "P" } else { " " },
                            tab.title
                        );
                        if ui.selectable_label(index == active_index, label).clicked() {
                            self.shell_state.session.set_active_tab(index);
                            self.shell_state.address_bar_input = tab.url.clone();
                            self.shell_state
                                .record_event(format!("active tab: {}", tab.url));
                        }
                    }
                }
                ui.label(format!("Title: {}", self.shell_state.active_tab.title));
                ui.label(format!("URL: {}", self.shell_state.active_tab.current_url));
                ui.label(format!("History: {}", self.shell_state.history.len()));
                if let Some(favicon) = &self.shell_state.favicon_url {
                    ui.label(format!("Favicon: {favicon}"));
                }
                if let Some(metadata) = &self.shell_state.metadata_summary {
                    ui.label(format!("Metadata: {metadata}"));
                }
                ui.label(format!(
                    "Profiles: {}",
                    self.shell_state.runtime_paths.profiles_dir.display()
                ));
                ui.label(format!(
                    "Cache: {}",
                    self.shell_state.runtime_paths.cache_dir.display()
                ));
                ui.label(format!(
                    "Crash dumps: {}",
                    self.shell_state.runtime_paths.crash_dumps_dir.display()
                ));
                ui.label(format!(
                    "Downloads: {}",
                    self.shell_state.runtime_paths.downloads_dir.display()
                ));
                if let Some(last_download) = &self.shell_state.last_download {
                    ui.label(format!("Last download: {last_download}"));
                }
                if let Some((kind, url)) = &self.shell_state.last_security_warning {
                    ui.label(format!("Security: {kind:?} {url}"));
                }
                if let Some(reason) = &self.shell_state.last_crash {
                    ui.label(format!("Crash: {reason}"));
                }
                if let Some(path) = &self.shell_state.last_crash_dump {
                    ui.label(format!("Crash dump: {path}"));
                }
                if let Some(endpoint) = &self.shell_state.devtools_endpoint {
                    ui.label(format!("Devtools: {endpoint}"));
                }
                if ui
                    .checkbox(
                        &mut self.shell_state.engine_verbose_logging,
                        "Verbose Servo logging",
                    )
                    .changed()
                {
                    self.engine
                        .set_verbose_logging(self.shell_state.engine_verbose_logging);
                    self.shell_state.record_event(format!(
                        "servo verbose logging {}",
                        if self.shell_state.engine_verbose_logging {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ));
                }
                ui.collapsing("Debug events", |ui| {
                    if ui.button("Sim Popup").clicked() {
                        self.engine.inject_event(EngineEvent::PopupRequested {
                            url: "https://example.invalid/popup".to_string(),
                            disposition: WindowDisposition::NewWindow,
                        });
                    }
                    if ui.button("Sim Dialog").clicked() {
                        self.engine.inject_event(EngineEvent::DialogRequested {
                            kind: DialogKind::Alert,
                            message: "Simulated alert".to_string(),
                        });
                    }
                    if ui.button("Sim Context Menu").clicked() {
                        self.engine
                            .inject_event(EngineEvent::ContextMenuRequested { x: 120.0, y: 88.0 });
                    }
                    if ui.button("Sim New Window").clicked() {
                        self.engine.inject_event(EngineEvent::NewWindowRequested {
                            url: "https://example.invalid/new".to_string(),
                            disposition: WindowDisposition::ForegroundTab,
                        });
                    }
                    if ui.button("Sim Download").clicked() {
                        self.engine.inject_event(EngineEvent::DownloadRequested {
                            url: "https://example.invalid/file.zip".to_string(),
                            suggested_path: Some("downloads/file.zip".to_string()),
                        });
                    }
                    if ui.button("Sim TLS Warning").clicked() {
                        self.engine.inject_event(EngineEvent::SecurityWarning {
                            kind: SecurityWarningKind::TlsError,
                            url: "https://badssl.example.invalid".to_string(),
                        });
                    }
                    if ui.button("Sim Crash").clicked() {
                        self.engine.inject_event(EngineEvent::Crashed {
                            reason: "simulated crash".to_string(),
                        });
                    }
                });
                self.render_cache_panel(ui);
            });

        if self.shell_state.permission_panel_open {
            eframe::egui::SidePanel::right("permissions")
                .default_width(260.0)
                .show(ctx, |ui| {
                    ui.heading("Capability Grants");
                    for (capability, decision) in &self.shell_state.capabilities_snapshot {
                        ui.horizontal(|ui| {
                            ui.monospace(capability);
                            ui.label(decision);
                        });
                    }
                });
        }

        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Browser Backend View");
            ui.separator();
            ui.label(format!("Engine state: {}", self.shell_state.engine_status));
            ui.label("This viewport is reserved for Servo-backed rendering surfaces.");
            if let Some(frame_number) = self.render_frame_number {
                let size = self
                    .render_frame_size
                    .map(|(w, h)| format!("{w}x{h}"))
                    .unwrap_or_else(|| "unknown".to_string());
                ui.label(format!("Frame: {frame_number} ({size})"));
            }
            if let Some((format, alpha, color_space)) = self.render_frame_format {
                ui.label(format!(
                    "Format: {} / {} / {}",
                    format.as_str(),
                    alpha.as_str(),
                    color_space.as_str()
                ));
            }
            if let Some(avg) = frame_average_ms(&self.frame_times) {
                let last = self
                    .last_frame_ms
                    .map(|ms| format!("{ms:.1}ms"))
                    .unwrap_or_else(|| "n/a".to_string());
                ui.label(format!("Frame timing: avg {avg:.1}ms (last {last})"));
            }
            if let Some(avg) = frame_average_ms(&self.upload_times) {
                let last = self
                    .last_upload_ms
                    .map(|ms| format!("{ms:.1}ms"))
                    .unwrap_or_else(|| "n/a".to_string());
                ui.label(format!("Frame upload: avg {avg:.1}ms (last {last})"));
            }
            ui.label(format!(
                "Render mode: {} (pacing: {})",
                self.config.engine.render_mode, self.config.engine.frame_pacing
            ));
            if let Some(texture) = &self.render_texture {
                let response =
                    ui.add(eframe::egui::Image::from_texture(texture).shrink_to_fit());
                if self.config.engine.debug_pointer_overlay {
                    if let Some(pos) = self.last_pointer_pos {
                        if response.rect.contains(pos) {
                            let painter = ui.painter().with_clip_rect(response.rect);
                            let stroke =
                                eframe::egui::Stroke::new(1.0, eframe::egui::Color32::YELLOW);
                            let offset = eframe::egui::vec2(8.0, 0.0);
                            painter.line_segment([pos - offset, pos + offset], stroke);
                            let offset = eframe::egui::vec2(0.0, 8.0);
                            painter.line_segment([pos - offset, pos + offset], stroke);
                        }
                    }
                }
            }
            ui.add_space(12.0);
            ui.group(|ui| {
                ui.label("Current target");
                ui.monospace(&self.shell_state.active_tab.current_url);
            });
            ui.add_space(12.0);
            ui.group(|ui| {
                ui.label("Pending dialogs");
                if let Some((kind, message)) = &self.shell_state.pending_dialog {
                    ui.label(format!("{kind:?}: {message}"));
                } else {
                    ui.label("none");
                }
                if let Some((url, disposition)) = &self.shell_state.pending_popup {
                    ui.label(format!("popup: {url} ({disposition:?})"));
                }
                if let Some((x, y)) = &self.shell_state.pending_context_menu {
                    ui.label(format!("context menu: {x:.0},{y:.0}"));
                }
            });
            ui.add_space(12.0);
            ui.group(|ui| {
                ui.label("Future dimensions");
                ui.label("Permissions, automation, cache introspection, article workflows, and local-tool routing hang off this shell.");
            });
        });

        self.sync_active_tab_from_session();

        if self.shell_state.log_panel_open {
            eframe::egui::TopBottomPanel::bottom("log_panel")
                .resizable(true)
                .default_height(180.0)
                .show(ctx, |ui| {
                    ui.heading("Startup and Command Log");
                    if let Some(avg) = frame_average_ms(&self.frame_times) {
                        let last = self
                            .last_frame_ms
                            .map(|ms| format!("{ms:.1}ms"))
                            .unwrap_or_else(|| "n/a".to_string());
                        ui.label(format!("Frame timing: avg {avg:.1}ms (last {last})"));
                    }
                    if let Some(avg) = frame_average_ms(&self.upload_times) {
                        let last = self
                            .last_upload_ms
                            .map(|ms| format!("{ms:.1}ms"))
                            .unwrap_or_else(|| "n/a".to_string());
                        ui.label(format!("Frame upload: avg {avg:.1}ms (last {last})"));
                    }
                    eframe::egui::ScrollArea::vertical().show(ui, |ui| {
                        for event in self.shell_state.event_log.iter().rev().take(128) {
                            ui.monospace(event);
                        }
                    });
                });
        }
    }
}

fn frame_average_ms(times: &VecDeque<f32>) -> Option<f32> {
    if times.is_empty() {
        return None;
    }
    let sum: f32 = times.iter().copied().sum();
    Some(sum / times.len() as f32)
}

impl Drop for BrazenApp {
    fn drop(&mut self) {
        self.engine.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{BrowserEngine, BrowserTab, EngineEvent, EngineFrame, EngineStatus};

    struct MockEngine {
        status: EngineStatus,
        tab: BrowserTab,
        events: Vec<EngineEvent>,
    }

    impl BrowserEngine for MockEngine {
        fn backend_name(&self) -> &'static str {
            "mock"
        }

        fn instance_id(&self) -> crate::engine::EngineInstanceId {
            1
        }

        fn status(&self) -> EngineStatus {
            self.status.clone()
        }

        fn active_tab(&self) -> &BrowserTab {
            &self.tab
        }

        fn navigate(&mut self, url: &str) {
            self.tab.current_url = url.to_string();
            self.events
                .push(EngineEvent::NavigationRequested(url.to_string()));
        }

        fn reload(&mut self) {}

        fn stop(&mut self) {}

        fn go_back(&mut self) {}

        fn go_forward(&mut self) {}

        fn attach_surface(&mut self, _surface: crate::engine::RenderSurfaceHandle) {}

        fn set_render_surface(&mut self, _metadata: RenderSurfaceMetadata) {}

        fn render_frame(&mut self) -> Option<EngineFrame> {
            None
        }

        fn set_focus(&mut self, _focus: crate::engine::FocusState) {}

        fn handle_input(&mut self, _event: crate::engine::InputEvent) {}

        fn handle_ime(&mut self, _event: crate::engine::ImeEvent) {}

        fn handle_clipboard(&mut self, _request: crate::engine::ClipboardRequest) {}

        fn set_verbose_logging(&mut self, _enabled: bool) {}

        fn configure_devtools(&mut self, _enabled: bool, _transport: &str) {}

        fn suspend(&mut self) {}

        fn resume(&mut self) {}

        fn shutdown(&mut self) {}

        fn inject_event(&mut self, event: EngineEvent) {
            self.events.push(event);
        }

        fn take_events(&mut self) -> Vec<EngineEvent> {
            std::mem::take(&mut self.events)
        }
    }

    #[test]
    fn shell_state_sync_handles_ready_and_error_statuses() {
        let paths = RuntimePaths {
            config_path: "brazen.toml".into(),
            data_dir: "data".into(),
            logs_dir: "logs".into(),
            profiles_dir: "profiles".into(),
            cache_dir: "cache".into(),
            downloads_dir: "downloads".into(),
            crash_dumps_dir: "crash-dumps".into(),
            active_profile_dir: "profiles/default".into(),
            session_path: "profiles/default/session.json".into(),
        };
        let mut shell = ShellState {
            app_name: "Brazen".to_string(),
            backend_name: "mock".to_string(),
            engine_instance_id: 1,
            engine_status: EngineStatus::Initializing,
            active_tab: BrowserTab {
                id: 1,
                title: "Loading".to_string(),
                current_url: "about:blank".to_string(),
            },
            address_bar_input: "https://example.com".to_string(),
            page_title: "Loading".to_string(),
            load_progress: 0.0,
            can_go_back: false,
            can_go_forward: false,
            document_ready: false,
            favicon_url: None,
            metadata_summary: None,
            history: Vec::new(),
            last_committed_url: None,
            was_minimized: false,
            pending_popup: None,
            pending_dialog: None,
            pending_context_menu: None,
            pending_new_window: None,
            last_download: None,
            last_security_warning: None,
            last_crash: None,
            last_crash_dump: None,
            devtools_endpoint: None,
            engine_verbose_logging: false,
            session: SessionSnapshot::new("default".to_string(), "now".to_string()),
            event_log: Vec::new(),
            log_panel_open: true,
            permission_panel_open: false,
            capabilities_snapshot: Vec::new(),
            runtime_paths: paths,
        };

        let mut ready_engine = MockEngine {
            status: EngineStatus::Ready,
            tab: BrowserTab {
                id: 1,
                title: "Example".to_string(),
                current_url: "https://example.com".to_string(),
            },
            events: vec![EngineEvent::StatusChanged(EngineStatus::Ready)],
        };
        shell.sync_from_engine(&mut ready_engine);
        assert_eq!(shell.engine_status, EngineStatus::Ready);
        assert!(shell.event_log.iter().any(|line| line.contains("status:")));

        let mut failing_engine = MockEngine {
            status: EngineStatus::Error("boot failed".to_string()),
            tab: shell.active_tab.clone(),
            events: vec![EngineEvent::StatusChanged(EngineStatus::Error(
                "boot failed".to_string(),
            ))],
        };
        shell.sync_from_engine(&mut failing_engine);
        assert_eq!(
            shell.engine_status,
            EngineStatus::Error("boot failed".to_string())
        );
    }
}
