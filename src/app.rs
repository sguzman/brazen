use chrono::Utc;

use crate::cache::{AssetQuery, AssetStore};
use crate::commands::{AppCommand, dispatch_command};
use crate::config::BrazenConfig;
use crate::engine::{
    BrowserEngine, BrowserTab, DialogKind, EngineEvent, EngineFactory, EngineStatus, FocusState,
    InputEvent, RenderSurfaceHandle, RenderSurfaceMetadata, SecurityWarningKind, WindowDisposition,
};
use crate::permissions::Capability;
use crate::platform_paths::RuntimePaths;
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
        self.engine_status = engine.status();
        self.active_tab = engine.active_tab().clone();
        for event in engine.take_events() {
            match event {
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

    let mut shell_state = ShellState {
        app_name: config.app.name.clone(),
        backend_name: engine.backend_name().to_string(),
        engine_instance_id: engine.instance_id(),
        engine_status: engine.status(),
        active_tab: engine.active_tab().clone(),
        address_bar_input: config.app.homepage.clone(),
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
    cache_store: AssetStore,
    cache_query_url: String,
    cache_query_mime: String,
    cache_query_hash: String,
    cache_query_session: String,
    cache_export_path: String,
    cache_import_path: String,
    cache_manifest_path: String,
}

impl BrazenApp {
    pub fn new(config: BrazenConfig, shell_state: ShellState) -> Self {
        let engine_factory = crate::engine::ServoEngineFactory;
        let engine = engine_factory.create(&config, &shell_state.runtime_paths);
        let surface_handle = RenderSurfaceHandle {
            id: 1,
            label: "primary-surface".to_string(),
        };
        let cache_store = AssetStore::load(
            config.cache.clone(),
            &shell_state.runtime_paths,
            config.profiles.active_profile.clone(),
        );

        Self {
            config,
            shell_state,
            engine,
            engine_factory,
            surface_handle,
            last_surface: None,
            cache_store,
            cache_query_url: String::new(),
            cache_query_mime: String::new(),
            cache_query_hash: String::new(),
            cache_query_session: String::new(),
            cache_export_path: "cache-export.json".to_string(),
            cache_import_path: "cache-import.json".to_string(),
            cache_manifest_path: "cache-manifest.json".to_string(),
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
            self.engine.attach_surface(self.surface_handle.clone());
            self.engine.set_render_surface(metadata.clone());
            self.last_surface = Some(metadata);
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
                    self.engine
                        .handle_input(InputEvent::PointerMove { x: pos.x, y: pos.y });
                }
                eframe::egui::Event::PointerButton {
                    button, pressed, ..
                } => {
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
                eframe::egui::Event::Key { key, pressed, .. } => {
                    let key_name = format!("{key:?}");
                    if pressed {
                        self.engine
                            .handle_input(InputEvent::KeyDown { key: key_name });
                    } else {
                        self.engine
                            .handle_input(InputEvent::KeyUp { key: key_name });
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
                self.engine.navigate(&url);
                self.shell_state
                    .record_event(format!("new window routed to current tab: {url}"));
            }
            WindowDisposition::BackgroundTab | WindowDisposition::NewWindow => {
                self.shell_state.session.open_new_tab(&url, "New Tab");
                self.shell_state
                    .record_event(format!("new window opened as tab: {url}"));
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
        let _ = std::fs::write(&path, reason.as_bytes());
        self.shell_state.last_crash_dump = Some(path.display().to_string());
    }

    fn restart_engine(&mut self) {
        self.engine.shutdown();
        self.engine = self
            .engine_factory
            .create(&self.config, &self.shell_state.runtime_paths);
        self.last_surface = None;
        self.shell_state.record_event("engine restarted");
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
        self.shell_state.sync_from_engine(self.engine.as_mut());
        self.apply_new_window_policy();
        if let Some(reason) = self.shell_state.last_crash.clone() {
            if self.shell_state.last_crash_dump.is_none() {
                self.write_crash_dump(&reason);
            }
        }

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
                    eframe::egui::ScrollArea::vertical().show(ui, |ui| {
                        for event in self.shell_state.event_log.iter().rev().take(128) {
                            ui.monospace(event);
                        }
                    });
                });
        }
    }
}

impl Drop for BrazenApp {
    fn drop(&mut self) {
        self.engine.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{BrowserEngine, BrowserTab, EngineEvent, EngineStatus};

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

        fn go_back(&mut self) {}

        fn go_forward(&mut self) {}

        fn attach_surface(&mut self, _surface: crate::engine::RenderSurfaceHandle) {}

        fn set_render_surface(&mut self, _metadata: RenderSurfaceMetadata) {}

        fn set_focus(&mut self, _focus: crate::engine::FocusState) {}

        fn handle_input(&mut self, _event: crate::engine::InputEvent) {}

        fn handle_ime(&mut self, _event: crate::engine::ImeEvent) {}

        fn handle_clipboard(&mut self, _request: crate::engine::ClipboardRequest) {}

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
        assert!(
            shell
                .event_log
                .iter()
                .any(|line| line.contains("StatusChanged"))
        );

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
