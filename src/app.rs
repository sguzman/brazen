use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::automation::{
    AutomationCapabilityEvent, AutomationCommand, AutomationHandle, AutomationNavigationEvent,
    drain_automation_commands,
};
use crate::cache::{AssetQuery, AssetStore};
use crate::commands::{AppCommand, dispatch_command};
use crate::config::BrazenConfig;
use crate::engine::{
    AlphaMode, BrowserEngine, BrowserTab, DialogKind, EngineEvent, EngineFactory, EngineLoadStatus,
    EngineStatus, FocusState, InputEvent, RenderSurfaceHandle, RenderSurfaceMetadata,
    SecurityWarningKind, WindowDisposition,
};
use crate::navigation::{normalize_url_input, resolve_startup_url};
use crate::permissions::Capability;
use crate::platform_paths::RuntimePaths;
use crate::rendering::{normalize_pixels, probe_frame_stats};
use crate::session::{NavigationEntry, SessionSnapshot, load_session, save_session};

const INPUT_TEST_URL: &str = "data:text/html;charset=utf-8,<html><body%20style=\"font-family:sans-serif;background:%23f8f8f8;\"><h1>Input%20Test</h1><p>Click%20buttons,%20type%20in%20the%20field,%20and%20use%20right-click.</p><input%20placeholder=\"type%20here\"><button%20onclick=\"document.body.style.background='%23dff'\">Change%20Color</button></body></html>";

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
    pub load_status: Option<EngineLoadStatus>,
    pub favicon_url: Option<String>,
    pub metadata_summary: Option<String>,
    pub history: Vec<String>,
    pub last_committed_url: Option<String>,
    pub active_tab_zoom: f32,
    pub cursor_icon: Option<String>,
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
    pub resource_reader_ready: Option<bool>,
    pub resource_reader_path: Option<String>,
    pub upstream_active: bool,
    pub upstream_last_error: Option<String>,
    pub render_warning: Option<String>,
    pub session: SessionSnapshot,
    pub event_log: Vec<String>,
    pub log_panel_open: bool,
    pub permission_panel_open: bool,
    pub find_panel_open: bool,
    pub find_query: String,
    pub capabilities_snapshot: Vec<(String, String)>,
    pub mount_manager: crate::mounts::MountManager,
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
                    self.load_status = state.load_status;
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
                EngineEvent::RenderHealthUpdated(health) => {
                    self.resource_reader_ready = health.resource_reader_ready;
                    self.resource_reader_path = health.resource_reader_path;
                    self.upstream_active = health.upstream_active;
                    self.upstream_last_error = health.last_error;
                }
                EngineEvent::CursorChanged { cursor } => {
                    self.cursor_icon = Some(cursor.clone());
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
    let mount_manager = crate::mounts::MountManager::new();
    let mut engine = engine_factory.create(config, paths, mount_manager.clone());
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
        load_status: None,
        favicon_url: None,
        metadata_summary: None,
        history: Vec::new(),
        last_committed_url: None,
        active_tab_zoom: session
            .active_tab()
            .map(|tab| tab.zoom_level)
            .unwrap_or(config.engine.zoom_default),
        cursor_icon: None,
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
        resource_reader_ready: None,
        resource_reader_path: None,
        upstream_active: false,
        upstream_last_error: None,
        render_warning: None,
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
        find_panel_open: false,
        find_query: String::new(),
        capabilities_snapshot,
        mount_manager,
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
    last_frame_pixels: Option<Vec<u8>>,
    render_viewport_rect: Option<eframe::egui::Rect>,
    frame_probe: Option<crate::rendering::FrameProbeStats>,
    blank_frame_streak: u32,
    blank_frame_warned: bool,
    render_capture_logged: bool,
    load_status_started_at: Option<Instant>,
    load_status_warned: bool,
    last_nav_url: Option<String>,
    frame_times: VecDeque<f32>,
    last_frame_instant: Option<Instant>,
    last_frame_ms: Option<f32>,
    upload_times: VecDeque<f32>,
    last_upload_ms: Option<f32>,
    last_pointer_pos: Option<eframe::egui::Pos2>,
    last_pointer_local: Option<eframe::egui::Pos2>,
    pointer_captured: bool,
    pointer_inside: bool,
    last_click_at: Option<Instant>,
    last_click_pos: Option<eframe::egui::Pos2>,
    last_click_button: Option<eframe::egui::PointerButton>,
    click_count: u8,
    capture_next_frame: bool,
    pending_restart_at: Option<chrono::DateTime<Utc>>,
    crash_count: u32,
    cache_store: AssetStore,
    cache_query_url: String,
    cache_query_mime: String,
    cache_query_hash: String,
    cache_query_session: String,
    cache_query_tab: String,
    cache_query_status: String,
    cache_selected_asset: Option<String>,
    cache_export_path: String,
    cache_import_path: String,
    cache_manifest_path: String,
    panels: WorkspacePanels,
    workspace_layout_path: PathBuf,
    bookmarks: Vec<String>,
    downloads: Vec<String>,
    ui_theme: UiTheme,
    ui_density: UiDensity,
    command_palette_open: bool,
    command_palette_query: String,
    command_palette_focus_pending: bool,
    address_bar_focus_pending: bool,
    pending_startup_url: Option<String>,
    automation_handle: Option<AutomationHandle>,
    automation_rx: Option<tokio::sync::mpsc::UnboundedReceiver<AutomationCommand>>,
    last_nav_event: Option<(String, Option<EngineLoadStatus>, f32)>,
    last_event_log_len: usize,
}

#[derive(Debug, Clone, Copy)]
enum PaletteCommand {
    NewTab,
    CloseTab,
    Reload,
    StopLoading,
    GoBack,
    GoForward,
    FocusAddressBar,
    ToggleLogs,
    TogglePermissions,
}

struct PaletteEntry {
    label: &'static str,
    action: PaletteCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkspaceLayout {
    panels: WorkspacePanels,
    theme: UiTheme,
    density: UiDensity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum UiTheme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum UiDensity {
    Compact,
    Comfortable,
}

#[derive(Debug, Clone, Copy)]
enum LayoutPreset {
    Focus,
    Inspector,
    Archive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct WorkspacePanels {
    sidebar_visible: bool,
    bookmarks: bool,
    history: bool,
    downloads: bool,
    dom_inspector: bool,
    network_inspector: bool,
    cache_explorer: bool,
    capability_inspector: bool,
    automation_console: bool,
    knowledge_graph: bool,
    reading_queue: bool,
    tts_controls: bool,
    workspace_settings: bool,
}

impl Default for WorkspacePanels {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            bookmarks: false,
            history: false,
            downloads: false,
            dom_inspector: false,
            network_inspector: false,
            cache_explorer: false,
            capability_inspector: false,
            automation_console: false,
            knowledge_graph: false,
            reading_queue: false,
            tts_controls: false,
            workspace_settings: false,
        }
    }
}

impl BrazenApp {
    pub fn new(
        config: BrazenConfig,
        shell_state: ShellState,
        automation: Option<crate::automation::AutomationRuntime>,
    ) -> Self {
        let engine_factory = crate::engine::ServoEngineFactory;
        let mut engine = engine_factory.create(&config, &shell_state.runtime_paths, shell_state.mount_manager.clone());
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
        let workspace_layout_path = shell_state
            .runtime_paths
            .data_dir
            .join("workspace-layout.json");
        let mut panels = WorkspacePanels::default();
        let mut ui_theme = UiTheme::System;
        let mut ui_density = UiDensity::Comfortable;
        if let Some(layout) = Self::load_workspace_layout(&workspace_layout_path) {
            panels = layout.panels;
            ui_theme = layout.theme;
            ui_density = layout.density;
        }

        let (automation_handle, automation_rx) = automation
            .map(|runtime| (Some(runtime.handle), Some(runtime.command_rx)))
            .unwrap_or((None, None));

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
            last_frame_pixels: None,
            render_viewport_rect: None,
            frame_probe: None,
            blank_frame_streak: 0,
            blank_frame_warned: false,
            render_capture_logged: false,
            load_status_started_at: None,
            load_status_warned: false,
            last_nav_url: None,
            frame_times: VecDeque::with_capacity(120),
            last_frame_instant: None,
            last_frame_ms: None,
            upload_times: VecDeque::with_capacity(120),
            last_upload_ms: None,
            last_pointer_pos: None,
            last_pointer_local: None,
            pointer_captured: false,
            pointer_inside: false,
            last_click_at: None,
            last_click_pos: None,
            last_click_button: None,
            click_count: 0,
            capture_next_frame,
            pending_restart_at: None,
            crash_count: 0,
            cache_store,
            cache_query_url: String::new(),
            cache_query_mime: String::new(),
            cache_query_hash: String::new(),
            cache_query_session: String::new(),
            cache_query_tab: String::new(),
            cache_query_status: String::new(),
            cache_selected_asset: None,
            cache_export_path: "cache-export.json".to_string(),
            cache_import_path: "cache-import.json".to_string(),
            cache_manifest_path: "cache-manifest.json".to_string(),
            panels,
            workspace_layout_path,
            bookmarks: Vec::new(),
            downloads: Vec::new(),
            ui_theme,
            ui_density,
            command_palette_open: false,
            command_palette_query: String::new(),
            command_palette_focus_pending: false,
            address_bar_focus_pending: false,
            pending_startup_url,
            automation_handle,
            automation_rx,
            last_nav_event: None,
            last_event_log_len: 0,
        }
    }

    fn frame_probe_enabled(&self) -> bool {
        self.config.engine.debug_frame_probe
    }

    fn update_render_health(&mut self) {
        if self.shell_state.last_committed_url != self.last_nav_url {
            self.last_nav_url = self.shell_state.last_committed_url.clone();
            self.blank_frame_streak = 0;
            self.blank_frame_warned = false;
            self.render_capture_logged = false;
            self.load_status_started_at = None;
            self.load_status_warned = false;
            self.shell_state.render_warning = None;
        }

        match self.shell_state.load_status {
            Some(EngineLoadStatus::Started) => {
                if self.load_status_started_at.is_none() {
                    self.load_status_started_at = Some(Instant::now());
                }
                if let Some(started_at) = self.load_status_started_at
                    && started_at.elapsed() >= Duration::from_secs(10)
                    && !self.load_status_warned
                {
                    self.load_status_warned = true;
                    let warning = "load status stuck at Started for 10s".to_string();
                    tracing::warn!(target: "brazen::render", "{warning}");
                    self.shell_state.record_event(warning.clone());
                    self.shell_state.render_warning = Some(warning);
                }
            }
            Some(_) | None => {
                self.load_status_started_at = None;
                self.load_status_warned = false;
                self.shell_state.render_warning = None;
            }
        }
    }

    fn update_automation(&mut self) {
        if let Some(receiver) = &mut self.automation_rx {
            drain_automation_commands(receiver, &mut self.shell_state, self.engine.as_mut());
        }
        if let Some(handle) = &self.automation_handle {
            handle.update_snapshot(&self.shell_state, &self.cache_store);
            let nav_key = (
                self.shell_state.active_tab.current_url.clone(),
                self.shell_state.load_status,
                (self.shell_state.load_progress * 100.0).round() / 100.0,
            );
            if self.last_nav_event.as_ref() != Some(&nav_key) {
                let event = AutomationNavigationEvent {
                    url: self.shell_state.active_tab.current_url.clone(),
                    title: self.shell_state.page_title.clone(),
                    load_status: self
                        .shell_state
                        .load_status
                        .map(|status| status.as_str().to_string()),
                    load_progress: self.shell_state.load_progress,
                };
                handle.publish_navigation(event);
                self.last_nav_event = Some(nav_key);
            }
            if self.shell_state.event_log.len() > self.last_event_log_len {
                for line in self.shell_state.event_log[self.last_event_log_len..].iter() {
                    let lower = line.to_lowercase();
                    if lower.contains("permission")
                        || lower.contains("capability")
                        || lower.contains("security warning")
                    {
                        handle.publish_capability(AutomationCapabilityEvent {
                            message: line.clone(),
                        });
                    }
                }
                self.last_event_log_len = self.shell_state.event_log.len();
            }
        }
    }

    fn apply_cursor_icon(&self, ctx: &eframe::egui::Context) {
        let Some(cursor) = self.shell_state.cursor_icon.as_deref() else {
            return;
        };
        let inside = self
            .last_pointer_pos
            .and_then(|pos| self.render_viewport_rect.map(|rect| rect.contains(pos)))
            .unwrap_or(false);
        if !inside {
            return;
        }
        let icon = match cursor {
            "Pointer" => eframe::egui::CursorIcon::PointingHand,
            "Text" | "VerticalText" => eframe::egui::CursorIcon::Text,
            "Crosshair" => eframe::egui::CursorIcon::Crosshair,
            "Move" | "AllScroll" => eframe::egui::CursorIcon::Move,
            "Grab" => eframe::egui::CursorIcon::Grab,
            "Grabbing" => eframe::egui::CursorIcon::Grabbing,
            "NotAllowed" | "NoDrop" => eframe::egui::CursorIcon::NotAllowed,
            "EResize" | "EwResize" => eframe::egui::CursorIcon::ResizeHorizontal,
            "NResize" | "SResize" | "NsResize" => eframe::egui::CursorIcon::ResizeVertical,
            "NeResize" | "SwResize" | "NeswResize" => eframe::egui::CursorIcon::ResizeNeSw,
            "NwResize" | "SeResize" | "NwseResize" => eframe::egui::CursorIcon::ResizeNwSe,
            "RowResize" => eframe::egui::CursorIcon::ResizeRow,
            "ColResize" => eframe::egui::CursorIcon::ResizeColumn,
            "ZoomIn" => eframe::egui::CursorIcon::ZoomIn,
            "ZoomOut" => eframe::egui::CursorIcon::ZoomOut,
            "Wait" | "Progress" => eframe::egui::CursorIcon::Wait,
            _ => eframe::egui::CursorIcon::Default,
        };
        ctx.output_mut(|output| output.cursor_icon = icon);
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

    fn open_input_test_page(&mut self) {
        self.shell_state.address_bar_input = INPUT_TEST_URL.to_string();
        let _ = dispatch_command(
            &mut self.shell_state,
            self.engine.as_mut(),
            AppCommand::NavigateTo(INPUT_TEST_URL.to_string()),
        );
        self.shell_state.record_event("navigation: input test page");
    }

    fn render_context_menu(&mut self, ctx: &eframe::egui::Context) {
        let Some((x, y)) = self.shell_state.pending_context_menu else {
            return;
        };
        let mut close_menu = false;
        let screen = ctx.viewport_rect();
        let mut pos = eframe::egui::pos2(x, y);
        let max_x = (screen.right() - 200.0).max(screen.left());
        let max_y = (screen.bottom() - 200.0).max(screen.top());
        pos.x = pos.x.clamp(screen.left(), max_x);
        pos.y = pos.y.clamp(screen.top(), max_y);

        let response = eframe::egui::Area::new(eframe::egui::Id::new("context_menu"))
            .order(eframe::egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let frame = eframe::egui::Frame::popup(ui.style());
                frame.show(ui, |ui| {
                    ui.set_min_width(180.0);
                    let current_url = self.shell_state.active_tab.current_url.clone();
                    if ui.button("Copy URL").clicked() {
                        ctx.copy_text(current_url.clone());
                        self.shell_state.record_event("context menu: copy url");
                        close_menu = true;
                    }
                    if ui.button("Open In New Tab").clicked() {
                        self.shell_state
                            .session
                            .open_new_tab(&current_url, "New Tab");
                        self.shell_state.session.active_tab_mut().zoom_level =
                            self.config.engine.zoom_default;
                        self.shell_state.active_tab_zoom = self.config.engine.zoom_default;
                        self.shell_state.address_bar_input = current_url.clone();
                        let _ = dispatch_command(
                            &mut self.shell_state,
                            self.engine.as_mut(),
                            AppCommand::NavigateTo(current_url),
                        );
                        self.shell_state
                            .record_event("context menu: open in new tab");
                        close_menu = true;
                    }
                    if ui.button("Reload").clicked() {
                        let _ = dispatch_command(
                            &mut self.shell_state,
                            self.engine.as_mut(),
                            AppCommand::ReloadActiveTab,
                        );
                        close_menu = true;
                    }
                    if ui.button("Save Snapshot").clicked() {
                        self.save_snapshot_to_disk();
                        close_menu = true;
                    }
                    ui.separator();
                    if ui.button("Zoom In").clicked() {
                        self.apply_zoom_steps(1, "context menu");
                        close_menu = true;
                    }
                    if ui.button("Zoom Out").clicked() {
                        self.apply_zoom_steps(-1, "context menu");
                        close_menu = true;
                    }
                    if ui.button("Reset Zoom").clicked() {
                        self.set_active_tab_zoom(self.config.engine.zoom_default, "reset");
                        close_menu = true;
                    }
                });
            });

        if ctx.input(|input| input.pointer.any_pressed())
            && let Some(pos) = ctx.input(|input| input.pointer.latest_pos())
            && !response.response.rect.contains(pos)
        {
            close_menu = true;
        }

        if close_menu {
            self.shell_state.pending_context_menu = None;
        }
    }

    fn map_pointer_to_viewport(
        &self,
        ctx: &eframe::egui::Context,
        pos: eframe::egui::Pos2,
        allow_outside: bool,
    ) -> Option<eframe::egui::Pos2> {
        let rect = self.render_viewport_rect?;
        if !allow_outside && !rect.contains(pos) {
            return None;
        }
        let mut local = pos - rect.min;
        if let Some(surface) = &self.last_surface {
            let pixels_per_point = ctx.pixels_per_point();
            let max_x = surface.viewport_width as f32 / pixels_per_point;
            let max_y = surface.viewport_height as f32 / pixels_per_point;
            local.x = local.x.clamp(0.0, max_x);
            local.y = local.y.clamp(0.0, max_y);
        }
        Some(eframe::egui::pos2(local.x, local.y))
    }

    fn clamp_zoom(&self, zoom: f32) -> f32 {
        zoom.clamp(self.config.engine.zoom_min, self.config.engine.zoom_max)
    }

    fn set_active_tab_zoom(&mut self, zoom: f32, reason: &str) {
        let clamped = self.clamp_zoom(zoom);
        let tab = self.shell_state.session.active_tab_mut();
        tab.zoom_level = clamped;
        self.shell_state.active_tab_zoom = clamped;
        self.engine.set_page_zoom(clamped);
        self.shell_state
            .record_event(format!("zoom {reason}: {clamped:.2}x"));
    }

    fn apply_zoom_steps(&mut self, steps: i32, reason: &str) {
        let current = self.shell_state.active_tab_zoom;
        let step = self.config.engine.zoom_step;
        let next = current + step * steps as f32;
        self.set_active_tab_zoom(next, reason);
    }

    fn apply_zoom_factor(&mut self, factor: f32, reason: &str) {
        let current = self.shell_state.active_tab_zoom;
        self.set_active_tab_zoom(current * factor, reason);
    }

    fn update_click_count(
        &mut self,
        button: eframe::egui::PointerButton,
        pos: eframe::egui::Pos2,
    ) -> u8 {
        let now = Instant::now();
        let within_time = self
            .last_click_at
            .map(|at| now.duration_since(at) <= Duration::from_millis(500))
            .unwrap_or(false);
        let within_distance = self
            .last_click_pos
            .map(|last| last.distance(pos) <= 4.0)
            .unwrap_or(false);
        if within_time && within_distance && self.last_click_button == Some(button) {
            self.click_count = (self.click_count + 1).min(3);
        } else {
            self.click_count = 1;
        }
        self.last_click_at = Some(now);
        self.last_click_pos = Some(pos);
        self.last_click_button = Some(button);
        self.click_count
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
        if let Some(surface) = &self.last_surface
            && (surface.viewport_width != frame.width || surface.viewport_height != frame.height)
        {
            tracing::warn!(
                target: "brazen::render",
                expected_width = surface.viewport_width,
                expected_height = surface.viewport_height,
                frame_width = frame.width,
                frame_height = frame.height,
                "frame size differs from render surface"
            );
        }
        let pixels = normalize_pixels(&frame, self.config.engine.debug_bypass_swizzle);
        if pixels.is_empty() {
            return;
        }
        self.last_frame_pixels = Some(pixels.clone());
        if self.frame_probe_enabled() {
            self.frame_probe = probe_frame_stats(&pixels, frame.width, frame.height, 256);
            if let Some(stats) = self.frame_probe {
                if stats.non_white_ratio < 0.01 {
                    self.blank_frame_streak = self.blank_frame_streak.saturating_add(1);
                    if self.blank_frame_streak >= 30
                        && !self.blank_frame_warned
                        && self.shell_state.load_progress > 0.0
                    {
                        self.blank_frame_warned = true;
                        if !self.render_capture_logged {
                            let (r, g, b, a) =
                                Self::sample_pixel_rgba(&pixels, frame.width, frame.height);
                            tracing::warn!(
                                target: "brazen::render",
                                ratio = stats.non_white_ratio,
                                samples = stats.sample_count,
                                alpha_min = stats.alpha_min,
                                alpha_avg = stats.alpha_avg,
                                sample = format!("{r},{g},{b},{a}"),
                                "render capture still blank after navigation"
                            );
                            self.render_capture_logged = true;
                        }
                        tracing::warn!(
                            target: "brazen::render",
                            ratio = stats.non_white_ratio,
                            samples = stats.sample_count,
                            "render probe detected mostly white frames after navigation"
                        );
                        self.shell_state
                            .record_event("render probe: mostly white frames after navigation");
                    }
                } else {
                    self.blank_frame_streak = 0;
                    self.blank_frame_warned = false;
                    if !self.render_capture_logged {
                        tracing::info!(
                            target: "brazen::render",
                            ratio = stats.non_white_ratio,
                            samples = stats.sample_count,
                            alpha_min = stats.alpha_min,
                            alpha_avg = stats.alpha_avg,
                            "render capture detected non-white content"
                        );
                        self.render_capture_logged = true;
                    }
                }
            }
        } else {
            self.frame_probe = None;
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

    fn save_snapshot_to_disk(&mut self) {
        let Some(pixels) = self.last_frame_pixels.as_ref() else {
            self.shell_state
                .record_event("snapshot save skipped: no frame");
            return;
        };
        let Some((width, height)) = self.render_frame_size else {
            self.shell_state
                .record_event("snapshot save skipped: no frame size");
            return;
        };
        let dir = &self.shell_state.runtime_paths.downloads_dir;
        if let Err(error) = std::fs::create_dir_all(dir) {
            tracing::warn!(
                target: "brazen::render",
                path = %dir.display(),
                %error,
                "failed to create downloads directory"
            );
            return;
        }
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let filename = format!("brazen-snapshot-{timestamp}.ppm");
        let path = dir.join(filename);
        let mut buffer = Vec::with_capacity(pixels.len() + 64);
        buffer.extend_from_slice(format!("P6\n{} {}\n255\n", width, height).as_bytes());
        for chunk in pixels.chunks_exact(4) {
            buffer.push(chunk[0]);
            buffer.push(chunk[1]);
            buffer.push(chunk[2]);
        }
        if let Err(error) = std::fs::write(&path, buffer) {
            tracing::warn!(
                target: "brazen::render",
                path = %path.display(),
                %error,
                "failed to write snapshot"
            );
            self.shell_state
                .record_event(format!("snapshot save failed: {}", path.display()));
        } else {
            self.shell_state
                .record_event(format!("snapshot saved: {}", path.display()));
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

    fn sample_pixel_rgba(pixels: &[u8], width: u32, height: u32) -> (u8, u8, u8, u8) {
        let width = width as usize;
        let height = height as usize;
        if width == 0 || height == 0 {
            return (0, 0, 0, 0);
        }
        let x = width / 2;
        let y = height / 2;
        let idx = (y.saturating_mul(width) + x).saturating_mul(4);
        if idx + 3 >= pixels.len() {
            return (0, 0, 0, 0);
        }
        (
            pixels[idx],
            pixels[idx + 1],
            pixels[idx + 2],
            pixels[idx + 3],
        )
    }

    fn forward_input_events(&mut self, ctx: &eframe::egui::Context) {
        let input = ctx.input(|input| input.clone());
        let focused = if input.raw.focused {
            FocusState::Focused
        } else {
            FocusState::Unfocused
        };
        self.engine.set_focus(focused);
        let input_logging = self.config.engine.input_logging;
        let suppress_engine_input = self.command_palette_open;

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
                    if let Some(local) =
                        self.map_pointer_to_viewport(ctx, pos, self.pointer_captured)
                    {
                        self.last_pointer_local = Some(local);
                        if !self.pointer_inside {
                            self.pointer_inside = true;
                            self.engine.handle_input(InputEvent::PointerEnter {
                                x: local.x,
                                y: local.y,
                            });
                        }
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                x = local.x,
                                y = local.y,
                                "pointer moved"
                            );
                        }
                        self.engine.handle_input(InputEvent::PointerMove {
                            x: local.x,
                            y: local.y,
                        });
                    } else {
                        self.last_pointer_local = None;
                        if self.pointer_inside && !self.pointer_captured {
                            self.pointer_inside = false;
                            self.engine.handle_input(InputEvent::PointerLeave);
                        }
                    }
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
                    let allow_outside = self.pointer_captured || pressed;
                    if let Some(local) = self.map_pointer_to_viewport(ctx, pos, allow_outside) {
                        self.last_pointer_local = Some(local);
                        if !self.pointer_inside {
                            self.pointer_inside = true;
                            self.engine.handle_input(InputEvent::PointerEnter {
                                x: local.x,
                                y: local.y,
                            });
                        }
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                button = button_id,
                                pressed,
                                x = local.x,
                                y = local.y,
                                "pointer button"
                            );
                        }
                        if pressed {
                            let click_count = self.update_click_count(button, pos);
                            if button == eframe::egui::PointerButton::Secondary {
                                self.shell_state.pending_context_menu = Some((pos.x, pos.y));
                                self.shell_state.record_event(format!(
                                    "context menu requested: {:.0},{:.0}",
                                    pos.x, pos.y
                                ));
                            } else {
                                self.shell_state.pending_context_menu = None;
                            }
                            self.pointer_captured = matches!(
                                button,
                                eframe::egui::PointerButton::Primary
                                    | eframe::egui::PointerButton::Middle
                            );
                            self.engine.handle_input(InputEvent::PointerDown {
                                button: button_id,
                                click_count,
                            });
                        } else {
                            self.pointer_captured = false;
                            self.engine
                                .handle_input(InputEvent::PointerUp { button: button_id });
                        }
                    } else if !pressed {
                        self.pointer_captured = false;
                        if self.pointer_inside {
                            self.pointer_inside = false;
                            self.engine.handle_input(InputEvent::PointerLeave);
                        }
                    }
                }
                eframe::egui::Event::MouseWheel { delta, unit, .. } => {
                    if let Some(pos) = input.pointer.latest_pos().or(self.last_pointer_pos)
                        && let Some(local) =
                            self.map_pointer_to_viewport(ctx, pos, self.pointer_captured)
                    {
                        self.last_pointer_local = Some(local);
                        self.engine.handle_input(InputEvent::PointerMove {
                            x: local.x,
                            y: local.y,
                        });
                    }
                    let modifiers = input.modifiers;
                    let axis = if delta.y.abs() >= delta.x.abs() {
                        delta.y
                    } else {
                        delta.x
                    };
                    if modifiers.ctrl || modifiers.command {
                        let steps = if axis.abs() < 0.1 {
                            0
                        } else {
                            axis.signum() as i32
                        };
                        if steps != 0 {
                            self.apply_zoom_steps(steps, "wheel");
                        }
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                axis,
                                steps,
                                "ctrl wheel zoom"
                            );
                        }
                        continue;
                    }
                    let mut delta_x = delta.x;
                    let mut delta_y = delta.y;
                    if modifiers.shift {
                        delta_x = if delta.x.abs() > 0.0 {
                            delta.x
                        } else {
                            delta.y
                        };
                        delta_y = 0.0;
                    }
                    let scale = match unit {
                        eframe::egui::MouseWheelUnit::Line => 24.0,
                        eframe::egui::MouseWheelUnit::Point => 1.0,
                        eframe::egui::MouseWheelUnit::Page => 240.0,
                    };
                    delta_x *= scale;
                    delta_y *= scale;
                    if input_logging {
                        tracing::trace!(
                            target: "brazen::input",
                            delta_x,
                            delta_y,
                            unit = ?unit,
                            "scroll wheel"
                        );
                    }
                    self.engine
                        .handle_input(InputEvent::Scroll { delta_x, delta_y });
                }
                eframe::egui::Event::Zoom(delta) => {
                    if (delta - 1.0).abs() > f32::EPSILON {
                        self.apply_zoom_factor(delta, "pinch");
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                delta,
                                "pinch zoom"
                            );
                        }
                    }
                }
                eframe::egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    repeat,
                    ..
                } => {
                    let is_command = modifiers.ctrl || modifiers.command;
                    let mut handled_shortcut = false;
                    if pressed && is_command {
                        match key {
                            eframe::egui::Key::C | eframe::egui::Key::X => {
                                self.engine
                                    .handle_clipboard(crate::engine::ClipboardRequest::Read);
                                self.shell_state
                                    .record_event(format!("shortcut {:?} => copy", key));
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::A => {
                                self.shell_state.record_event("shortcut: select all");
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::F => {
                                self.shell_state.find_panel_open = true;
                                self.shell_state.record_event("shortcut: find");
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::K => {
                                self.open_command_palette();
                                self.shell_state.record_event("shortcut: command palette");
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::L => {
                                self.address_bar_focus_pending = true;
                                self.shell_state.record_event("shortcut: focus address bar");
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::T => {
                                self.apply_palette_command(PaletteCommand::NewTab);
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::W => {
                                self.apply_palette_command(PaletteCommand::CloseTab);
                                handled_shortcut = true;
                            }
                            eframe::egui::Key::R => {
                                self.apply_palette_command(PaletteCommand::Reload);
                                handled_shortcut = true;
                            }
                            _ => {}
                        }
                    }
                    let zoom_shortcut = pressed
                        && is_command
                        && matches!(
                            key,
                            eframe::egui::Key::Plus
                                | eframe::egui::Key::Equals
                                | eframe::egui::Key::Minus
                                | eframe::egui::Key::Num0
                        );
                    if zoom_shortcut {
                        match key {
                            eframe::egui::Key::Plus | eframe::egui::Key::Equals => {
                                self.apply_zoom_steps(1, "shortcut");
                            }
                            eframe::egui::Key::Minus => {
                                self.apply_zoom_steps(-1, "shortcut");
                            }
                            eframe::egui::Key::Num0 => {
                                self.set_active_tab_zoom(self.config.engine.zoom_default, "reset");
                            }
                            _ => {}
                        }
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                key = ?key,
                                "zoom shortcut"
                            );
                        }
                        continue;
                    }
                    if handled_shortcut {
                        continue;
                    }
                    let key_name = format!("{key:?}");
                    let modifiers = crate::engine::KeyModifiers {
                        alt: modifiers.alt,
                        ctrl: modifiers.ctrl,
                        shift: modifiers.shift,
                        command: modifiers.command,
                    };
                    if !suppress_engine_input {
                        if pressed {
                            self.engine.handle_input(InputEvent::KeyDown {
                                key: key_name,
                                modifiers,
                                repeat,
                            });
                        } else {
                            self.engine.handle_input(InputEvent::KeyUp {
                                key: key_name,
                                modifiers,
                            });
                        }
                    }
                }
                eframe::egui::Event::Text(text) => {
                    if suppress_engine_input {
                        continue;
                    }
                    if input_logging {
                        tracing::trace!(
                            target: "brazen::input",
                            text = %text,
                            "text input"
                        );
                    }
                    self.engine.handle_input(InputEvent::TextInput { text });
                }
                eframe::egui::Event::Ime(ime) => match ime {
                    eframe::egui::ImeEvent::Enabled => {
                        if input_logging {
                            tracing::trace!(target: "brazen::input", "ime enabled");
                        }
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionStart);
                    }
                    eframe::egui::ImeEvent::Preedit(text) => {
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                text = %text,
                                "ime preedit"
                            );
                        }
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionUpdate { text });
                    }
                    eframe::egui::ImeEvent::Commit(text) => {
                        if input_logging {
                            tracing::trace!(
                                target: "brazen::input",
                                text = %text,
                                "ime commit"
                            );
                        }
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionEnd { text });
                    }
                    eframe::egui::ImeEvent::Disabled => {
                        if input_logging {
                            tracing::trace!(target: "brazen::input", "ime disabled");
                        }
                        self.engine.handle_ime(crate::engine::ImeEvent::Dismissed);
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

        if !input.raw.dropped_files.is_empty() {
            for file in input.raw.dropped_files {
                let mut target = None;
                if let Some(path) = file.path {
                    if let Ok(url) = url::Url::from_file_path(&path) {
                        target = Some(url.to_string());
                    } else {
                        target = Some(path.to_string_lossy().to_string());
                    }
                } else if file.name.starts_with("http://") || file.name.starts_with("https://") {
                    target = Some(file.name.clone());
                }
                if let Some(target) = target {
                    self.shell_state.address_bar_input = target.clone();
                    let _ = dispatch_command(
                        &mut self.shell_state,
                        self.engine.as_mut(),
                        AppCommand::NavigateTo(target.clone()),
                    );
                    self.shell_state
                        .record_event(format!("dropped file/url: {target}"));
                    break;
                }
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
        if (self.shell_state.active_tab_zoom - tab.zoom_level).abs() > f32::EPSILON {
            self.shell_state.active_tab_zoom = tab.zoom_level;
            self.engine.set_page_zoom(tab.zoom_level);
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
                    self.shell_state.session.active_tab_mut().zoom_level =
                        self.config.engine.zoom_default;
                    self.shell_state.active_tab_zoom = self.config.engine.zoom_default;
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
            .create(&self.config, &self.shell_state.runtime_paths, self.shell_state.mount_manager.clone());
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
        if let Some(scheduled) = self.pending_restart_at
            && Utc::now() >= scheduled
        {
            self.restart_engine();
            self.shell_state.last_crash = None;
            self.shell_state.session.crash_recovery_pending = false;
            self.pending_restart_at = None;
        }
    }

    fn palette_entries() -> &'static [PaletteEntry] {
        &[
            PaletteEntry {
                label: "New Tab",
                action: PaletteCommand::NewTab,
            },
            PaletteEntry {
                label: "Close Tab",
                action: PaletteCommand::CloseTab,
            },
            PaletteEntry {
                label: "Reload",
                action: PaletteCommand::Reload,
            },
            PaletteEntry {
                label: "Stop Loading",
                action: PaletteCommand::StopLoading,
            },
            PaletteEntry {
                label: "Go Back",
                action: PaletteCommand::GoBack,
            },
            PaletteEntry {
                label: "Go Forward",
                action: PaletteCommand::GoForward,
            },
            PaletteEntry {
                label: "Focus Address Bar",
                action: PaletteCommand::FocusAddressBar,
            },
            PaletteEntry {
                label: "Toggle Logs Panel",
                action: PaletteCommand::ToggleLogs,
            },
            PaletteEntry {
                label: "Toggle Permissions Panel",
                action: PaletteCommand::TogglePermissions,
            },
        ]
    }

    fn open_command_palette(&mut self) {
        self.command_palette_open = true;
        self.command_palette_focus_pending = true;
        self.command_palette_query.clear();
    }

    fn apply_palette_command(&mut self, action: PaletteCommand) {
        match action {
            PaletteCommand::NewTab => {
                self.shell_state
                    .session
                    .open_new_tab("about:blank", "New Tab");
                self.shell_state.session.active_tab_mut().zoom_level =
                    self.config.engine.zoom_default;
                self.shell_state.active_tab_zoom = self.config.engine.zoom_default;
                self.sync_active_tab_from_session();
                self.shell_state.address_bar_input =
                    self.shell_state.active_tab.current_url.clone();
                self.shell_state.record_event("palette: new tab");
            }
            PaletteCommand::CloseTab => {
                self.shell_state.session.close_active_tab();
                self.sync_active_tab_from_session();
                self.shell_state.address_bar_input =
                    self.shell_state.active_tab.current_url.clone();
                self.shell_state.record_event("palette: close tab");
            }
            PaletteCommand::Reload => {
                let _ = dispatch_command(
                    &mut self.shell_state,
                    self.engine.as_mut(),
                    AppCommand::ReloadActiveTab,
                );
                self.shell_state.record_event("palette: reload");
            }
            PaletteCommand::StopLoading => {
                let _ = dispatch_command(
                    &mut self.shell_state,
                    self.engine.as_mut(),
                    AppCommand::StopLoading,
                );
                self.shell_state.record_event("palette: stop loading");
            }
            PaletteCommand::GoBack => {
                let _ = dispatch_command(
                    &mut self.shell_state,
                    self.engine.as_mut(),
                    AppCommand::GoBack,
                );
                self.shell_state.session.go_back(Utc::now().to_rfc3339());
                self.shell_state.record_event("palette: go back");
            }
            PaletteCommand::GoForward => {
                let _ = dispatch_command(
                    &mut self.shell_state,
                    self.engine.as_mut(),
                    AppCommand::GoForward,
                );
                self.shell_state.session.go_forward(Utc::now().to_rfc3339());
                self.shell_state.record_event("palette: go forward");
            }
            PaletteCommand::FocusAddressBar => {
                self.address_bar_focus_pending = true;
                self.shell_state.record_event("palette: focus address bar");
            }
            PaletteCommand::ToggleLogs => {
                let _ = dispatch_command(
                    &mut self.shell_state,
                    self.engine.as_mut(),
                    AppCommand::ToggleLogPanel,
                );
            }
            PaletteCommand::TogglePermissions => {
                self.shell_state.permission_panel_open = !self.shell_state.permission_panel_open;
                self.shell_state.record_event(format!(
                    "permission panel {}",
                    if self.shell_state.permission_panel_open {
                        "opened"
                    } else {
                        "closed"
                    }
                ));
            }
        }
    }

    fn load_workspace_layout(path: &PathBuf) -> Option<WorkspaceLayout> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    fn save_workspace_layout(&self) {
        let Some(parent) = self.workspace_layout_path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let payload = WorkspaceLayout {
            panels: self.panels,
            theme: self.ui_theme,
            density: self.ui_density,
        };
        if let Ok(data) = serde_json::to_vec_pretty(&payload) {
            let _ = std::fs::write(&self.workspace_layout_path, data);
        }
    }

    fn apply_layout_preset(&mut self, preset: LayoutPreset) {
        self.panels = match preset {
            LayoutPreset::Focus => WorkspacePanels {
                sidebar_visible: true,
                cache_explorer: false,
                capability_inspector: false,
                automation_console: false,
                dom_inspector: false,
                network_inspector: false,
                knowledge_graph: false,
                reading_queue: false,
                tts_controls: false,
                bookmarks: false,
                history: false,
                downloads: false,
                workspace_settings: true,
            },
            LayoutPreset::Inspector => WorkspacePanels {
                sidebar_visible: true,
                cache_explorer: true,
                capability_inspector: true,
                automation_console: true,
                dom_inspector: true,
                network_inspector: true,
                knowledge_graph: false,
                reading_queue: false,
                tts_controls: false,
                bookmarks: false,
                history: true,
                downloads: true,
                workspace_settings: true,
            },
            LayoutPreset::Archive => WorkspacePanels {
                sidebar_visible: true,
                cache_explorer: true,
                capability_inspector: false,
                automation_console: false,
                dom_inspector: false,
                network_inspector: false,
                knowledge_graph: true,
                reading_queue: true,
                tts_controls: true,
                bookmarks: true,
                history: true,
                downloads: true,
                workspace_settings: true,
            },
        };
        self.shell_state
            .record_event(format!("layout preset applied: {preset:?}"));
        self.save_workspace_layout();
    }

    fn apply_ui_settings(&self, ctx: &eframe::egui::Context) {
        match self.ui_theme {
            UiTheme::System => {}
            UiTheme::Light => ctx.set_visuals(eframe::egui::Visuals::light()),
            UiTheme::Dark => ctx.set_visuals(eframe::egui::Visuals::dark()),
        }
        let mut style = (*ctx.style()).clone();
        match self.ui_density {
            UiDensity::Compact => {
                style.spacing.item_spacing = eframe::egui::vec2(6.0, 4.0);
                style.spacing.button_padding = eframe::egui::vec2(6.0, 4.0);
            }
            UiDensity::Comfortable => {
                style.spacing.item_spacing = eframe::egui::vec2(10.0, 8.0);
                style.spacing.button_padding = eframe::egui::vec2(10.0, 6.0);
            }
        }
        ctx.set_style(style);
    }

    fn render_workspace_settings(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.workspace_settings {
            return;
        }
        let mut open = true;
        let mut changed = false;
        eframe::egui::Window::new("Workspace Settings")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Layout presets");
                ui.horizontal(|ui| {
                    if ui.button("Focus").clicked() {
                        self.apply_layout_preset(LayoutPreset::Focus);
                    }
                    if ui.button("Inspector").clicked() {
                        self.apply_layout_preset(LayoutPreset::Inspector);
                    }
                    if ui.button("Archive").clicked() {
                        self.apply_layout_preset(LayoutPreset::Archive);
                    }
                });
                ui.separator();
                changed |= ui
                    .checkbox(&mut self.panels.sidebar_visible, "Show sidebar")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.panels.bookmarks, "Bookmarks panel")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.panels.history, "History panel")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.panels.downloads, "Downloads panel")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.panels.dom_inspector, "DOM inspector")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.panels.network_inspector, "Network inspector")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.panels.cache_explorer, "Cache explorer")
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.panels.capability_inspector,
                        "Capability inspector",
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.panels.automation_console, "Automation console")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.panels.knowledge_graph, "Knowledge graph")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.panels.reading_queue, "Reading queue")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.panels.tts_controls, "TTS controls")
                    .changed();
                ui.separator();
                ui.label("Theme");
                changed |= ui
                    .radio_value(&mut self.ui_theme, UiTheme::System, "System")
                    .clicked();
                changed |= ui
                    .radio_value(&mut self.ui_theme, UiTheme::Light, "Light")
                    .clicked();
                changed |= ui
                    .radio_value(&mut self.ui_theme, UiTheme::Dark, "Dark")
                    .clicked();
                ui.separator();
                ui.label("Density");
                changed |= ui
                    .radio_value(&mut self.ui_density, UiDensity::Comfortable, "Comfortable")
                    .clicked();
                changed |= ui
                    .radio_value(&mut self.ui_density, UiDensity::Compact, "Compact")
                    .clicked();
            });
        if !open {
            self.panels.workspace_settings = false;
            changed = true;
        }
        if changed {
            self.save_workspace_layout();
        }
    }

    fn render_bookmarks_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.bookmarks {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("Bookmarks")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Add Current").clicked() {
                        self.bookmarks
                            .push(self.shell_state.active_tab.current_url.clone());
                        self.shell_state.record_event("bookmark added");
                    }
                    if ui.button("Clear All").clicked() {
                        self.bookmarks.clear();
                    }
                });
                ui.separator();
                for (index, entry) in self.bookmarks.clone().iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.monospace(entry);
                        if ui.button("Open").clicked() {
                            let _ = dispatch_command(
                                &mut self.shell_state,
                                self.engine.as_mut(),
                                AppCommand::NavigateTo(entry.to_string()),
                            );
                        }
                        if ui.button("Remove").clicked() {
                            self.bookmarks.remove(index);
                        }
                    });
                }
            });
        if !open {
            self.panels.bookmarks = false;
        }
    }

    fn render_history_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.history {
            return;
        }
        let mut open = true;
        let history = self.shell_state.history.clone();
        eframe::egui::Window::new("History")
            .open(&mut open)
            .show(ctx, |ui| {
                for url in history.iter().rev().take(50) {
                    ui.horizontal(|ui| {
                        ui.monospace(url);
                        if ui.button("Open").clicked() {
                            let _ = dispatch_command(
                                &mut self.shell_state,
                                self.engine.as_mut(),
                                AppCommand::NavigateTo(url.to_string()),
                            );
                        }
                    });
                }
            });
        if !open {
            self.panels.history = false;
        }
    }

    fn render_downloads_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.downloads {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("Downloads")
            .open(&mut open)
            .show(ctx, |ui| {
                if let Some(last) = &self.shell_state.last_download {
                    ui.label(format!("Last: {last}"));
                } else {
                    ui.label("No downloads yet.");
                }
                ui.separator();
                for item in &self.downloads {
                    ui.monospace(item);
                }
                if ui.button("Add Sample Download").clicked() {
                    self.downloads
                        .push(format!("sample-{}.bin", self.downloads.len() + 1));
                }
            });
        if !open {
            self.panels.downloads = false;
        }
    }

    fn render_dom_inspector_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.dom_inspector {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("DOM Inspector")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("DOM inspector not yet wired to Servo.");
                ui.label(format!(
                    "Current URL: {}",
                    self.shell_state.active_tab.current_url
                ));
            });
        if !open {
            self.panels.dom_inspector = false;
        }
    }

    fn render_network_inspector_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.network_inspector {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("Network Inspector")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Network inspector is not yet wired.");
                for event in self.shell_state.event_log.iter().rev().take(20) {
                    if event.starts_with("nav:") {
                        ui.monospace(event);
                    }
                }
            });
        if !open {
            self.panels.network_inspector = false;
        }
    }

    fn render_cache_explorer_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.cache_explorer {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("Cache Explorer")
            .open(&mut open)
            .show(ctx, |ui| {
                self.render_cache_panel(ui);
            });
        if !open {
            self.panels.cache_explorer = false;
        }
    }

    fn render_capability_inspector_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.capability_inspector {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("Capability Inspector")
            .open(&mut open)
            .show(ctx, |ui| {
                for (cap, decision) in &self.shell_state.capabilities_snapshot {
                    ui.horizontal(|ui| {
                        ui.label(cap);
                        ui.monospace(decision);
                    });
                }
            });
        if !open {
            self.panels.capability_inspector = false;
        }
    }

    fn render_automation_console_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.automation_console {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("Automation Console")
            .open(&mut open)
            .show(ctx, |ui| {
                for event in self.shell_state.event_log.iter().rev().take(50) {
                    if event.contains("automation") {
                        ui.monospace(event);
                    }
                }
            });
        if !open {
            self.panels.automation_console = false;
        }
    }

    fn render_knowledge_graph_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.knowledge_graph {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("Knowledge Graph")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Knowledge graph inspector not yet wired.");
            });
        if !open {
            self.panels.knowledge_graph = false;
        }
    }

    fn render_reading_queue_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.reading_queue {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("Reading Queue")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Reading queue surface not wired.");
            });
        if !open {
            self.panels.reading_queue = false;
        }
    }

    fn render_tts_controls_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.tts_controls {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("TTS Controls")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("TTS controls not wired.");
                ui.horizontal(|ui| {
                    let _ = ui.button("Play");
                    let _ = ui.button("Pause");
                    let _ = ui.button("Stop");
                });
            });
        if !open {
            self.panels.tts_controls = false;
        }
    }

    fn render_command_palette(&mut self, ctx: &eframe::egui::Context) {
        if !self.command_palette_open {
            return;
        }
        let mut open = true;
        let mut close_requested = false;
        eframe::egui::Window::new("Command Palette")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(
                eframe::egui::Align2::CENTER_TOP,
                eframe::egui::vec2(0.0, 24.0),
            )
            .show(ctx, |ui| {
                let response = ui.add(
                    eframe::egui::TextEdit::singleline(&mut self.command_palette_query)
                        .hint_text("Type a command"),
                );
                if self.command_palette_focus_pending {
                    response.request_focus();
                    self.command_palette_focus_pending = false;
                }
                let query = self.command_palette_query.trim().to_lowercase();
                let entries = Self::palette_entries()
                    .iter()
                    .filter(|entry| entry.label.to_lowercase().contains(&query))
                    .collect::<Vec<_>>();
                ui.separator();
                for entry in entries.iter().take(8) {
                    if ui.button(entry.label).clicked() {
                        self.apply_palette_command(entry.action);
                        close_requested = true;
                    }
                }
                if ui.input(|input| input.key_pressed(eframe::egui::Key::Enter)) {
                    if let Some(entry) = entries.first() {
                        self.apply_palette_command(entry.action);
                    }
                    close_requested = true;
                }
                if ui.input(|input| input.key_pressed(eframe::egui::Key::Escape)) {
                    close_requested = true;
                }
            });
        if close_requested {
            open = false;
        }
        if !open {
            self.command_palette_open = false;
        }
    }

    fn render_tab_strip(&mut self, ctx: &eframe::egui::Context) {
        eframe::egui::TopBottomPanel::top("tab_strip").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let active_window = self.shell_state.session.active_window;
                let Some(window) = self.shell_state.session.windows.get(active_window) else {
                    return;
                };
                let active_index = window.active_tab;
                let tabs = window.tabs.clone();
                for (index, tab) in tabs.iter().enumerate() {
                    let is_active = index == active_index;
                    let label = if tab.title.is_empty() {
                        tab.url.clone()
                    } else {
                        tab.title.clone()
                    };
                    let response = ui.selectable_label(is_active, label);
                    if response.clicked() {
                        self.shell_state.session.set_active_tab(index);
                        self.sync_active_tab_from_session();
                        self.shell_state.address_bar_input = tab.url.clone();
                    }
                    if tabs.len() > 1 {
                        if ui.small_button("x").clicked() && index == active_index {
                            self.shell_state.session.close_active_tab();
                            self.sync_active_tab_from_session();
                            self.shell_state.address_bar_input =
                                self.shell_state.active_tab.current_url.clone();
                        }
                    }
                }
                if ui.small_button("+").clicked() {
                    self.shell_state
                        .session
                        .open_new_tab("about:blank", "New Tab");
                    self.shell_state.session.active_tab_mut().zoom_level =
                        self.config.engine.zoom_default;
                    self.shell_state.active_tab_zoom = self.config.engine.zoom_default;
                    self.sync_active_tab_from_session();
                    self.shell_state.address_bar_input =
                        self.shell_state.active_tab.current_url.clone();
                }
            });
        });
    }

    fn render_cache_panel(&mut self, ui: &mut eframe::egui::Ui) {
        ui.separator();
        ui.heading("Cache");
        let stats = self.cache_store.stats();
        ui.label(format!(
            "Entries: {} | Bodies: {} | Blobs: {} | Bytes: {} | Ratio: {:.2}",
            stats.entries,
            stats.captured_with_body,
            stats.unique_blobs,
            stats.total_bytes,
            stats.capture_ratio
        ));
        if let Some(last) = self.cache_store.latest_entry() {
            ui.label(format!("Last capture: {} {}", last.created_at, last.url));
        }
        ui.horizontal(|ui| {
            if ui.button("Sim Capture").clicked() {
                let mut headers = std::collections::BTreeMap::new();
                headers.insert("content-type".to_string(), "text/html".to_string());
                let session_id = Some(self.shell_state.session.session_id.0.to_string());
                let tab_id = Some(self.shell_state.session.active_tab_mut().id.0.to_string());
                let _ = self.cache_store.record_asset(
                    &self.shell_state.active_tab.current_url,
                    None,
                    Some("GET".to_string()),
                    Some(200),
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
            if ui.button("Export").clicked()
                && self
                    .cache_store
                    .export_json(self.cache_export_path.as_ref())
                    .is_ok()
            {
                self.shell_state.record_event("cache export complete");
            }
            if ui.button("Import").clicked()
                && self
                    .cache_store
                    .import_json(self.cache_import_path.as_ref())
                    .is_ok()
            {
                self.shell_state.record_event("cache import complete");
            }
            if ui.button("Manifest").clicked()
                && self
                    .cache_store
                    .build_replay_manifest(self.cache_manifest_path.as_ref())
                    .is_ok()
            {
                self.shell_state.record_event("cache manifest written");
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
        ui.horizontal(|ui| {
            ui.label("Tab");
            ui.text_edit_singleline(&mut self.cache_query_tab);
        });
        ui.horizontal(|ui| {
            ui.label("Status");
            ui.text_edit_singleline(&mut self.cache_query_status);
        });

        let query = AssetQuery {
            url: empty_to_none(&self.cache_query_url),
            mime: empty_to_none(&self.cache_query_mime),
            hash: empty_to_none(&self.cache_query_hash),
            session_id: empty_to_none(&self.cache_query_session),
            tab_id: empty_to_none(&self.cache_query_tab),
            status_code: self.cache_query_status.trim().parse::<u16>().ok(),
        };
        let results = self.cache_store.query(query);
        ui.label(format!(
            "Assets: {} (storage: {:?})",
            self.cache_store.entries().len(),
            self.cache_store.storage_mode()
        ));
        ui.label(format!("Matches: {}", results.len()));
        ui.separator();
        ui.label("Recent");
        for entry in self.cache_store.entries().iter().rev().take(5) {
            ui.label(format!("{} {}", entry.created_at, entry.url));
        }
        ui.separator();
        ui.label("Matches (latest)");
        for entry in results.iter().rev().take(5) {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} {} {}",
                    entry
                        .status_code
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    entry.mime,
                    entry.url
                ));
                if ui.button("Details").clicked() {
                    self.cache_selected_asset = Some(entry.asset_id.clone());
                }
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
        if let Some(selected) = self.cache_selected_asset.clone()
            && let Some(entry) = self.cache_store.find_by_id_or_hash(&selected)
        {
            ui.separator();
            ui.label(format!("Asset: {}", entry.asset_id));
            ui.label(format!("URL: {}", entry.url));
            if let Some(final_url) = &entry.final_url {
                ui.label(format!("Final URL: {}", final_url));
            }
            ui.label(format!(
                "Method/Status: {} {}",
                entry.method.as_deref().unwrap_or("-"),
                entry
                    .status_code
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ));
            ui.label(format!("MIME: {}", entry.mime));
            ui.label(format!(
                "Hash: {}",
                entry.hash.clone().unwrap_or_else(|| "-".to_string())
            ));
            if let Some(body_key) = &entry.body_key {
                ui.label(format!(
                    "Body key: {} ({})",
                    body_key,
                    self.cache_store.blob_path(body_key).display()
                ));
            }
            ui.label(format!(
                "Timing: start={:?} finish={:?} duration_ms={:?}",
                entry.request_started_at, entry.response_finished_at, entry.duration_ms
            ));
            ui.label(format!("Storage: {:?}", entry.storage_mode));
            ui.label(format!("Headers: {}", entry.response_headers.len()));
            if ui.button("Clear Details").clicked() {
                self.cache_selected_asset = None;
            }
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
        self.update_automation();
        self.update_render_health();
        self.apply_ui_settings(ctx);
        self.apply_cursor_icon(ctx);
        self.apply_new_window_policy();
        if let Some(reason) = self.shell_state.last_crash.clone()
            && self.shell_state.last_crash_dump.is_none()
        {
            self.write_crash_dump(&reason);
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
                let cache_entries = self.cache_store.entries().len();
                let cache_last = self
                    .cache_store
                    .latest_entry()
                    .map(|entry| entry.created_at.clone())
                    .unwrap_or_else(|| "-".to_string());
                ui.separator();
                ui.label(format!("cache: {} last: {}", cache_entries, cache_last));
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
                if self.address_bar_focus_pending {
                    response.request_focus();
                    self.address_bar_focus_pending = false;
                }
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
                if ui.button("Input Test").clicked() {
                    self.open_input_test_page();
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
                if ui.button("Workspace").clicked() {
                    self.panels.workspace_settings = !self.panels.workspace_settings;
                    self.save_workspace_layout();
                }
                if ui.button("Bookmarks").clicked() {
                    self.panels.bookmarks = !self.panels.bookmarks;
                    self.save_workspace_layout();
                }
                if ui.button("History").clicked() {
                    self.panels.history = !self.panels.history;
                    self.save_workspace_layout();
                }
                if ui.button("Downloads").clicked() {
                    self.panels.downloads = !self.panels.downloads;
                    self.save_workspace_layout();
                }
                if ui.button("New Window").clicked() {
                    self.shell_state
                        .record_event("window management: new window requested");
                }
                if ui.button("Close Window").clicked() {
                    self.shell_state
                        .record_event("window management: close window requested");
                }
                ui.separator();
                ui.label(format!(
                    "Zoom: {:.0}%",
                    self.shell_state.active_tab_zoom * 100.0
                ));
                if ui.button("Reset Zoom").clicked() {
                    self.set_active_tab_zoom(self.config.engine.zoom_default, "reset");
                }
            });
            ui.add(
                eframe::egui::ProgressBar::new(self.shell_state.load_progress)
                    .show_percentage()
                    .desired_width(f32::INFINITY),
            );
        });

        self.render_tab_strip(ctx);

        if self.panels.sidebar_visible {
            eframe::egui::SidePanel::left("tab_sidebar")
                .default_width(240.0)
                .show(ctx, |ui| {
                    ui.heading("Workspace");
                    ui.horizontal(|ui| {
                        if ui.button("New Tab").clicked() {
                            self.shell_state
                                .session
                                .open_new_tab("about:blank", "New Tab");
                            self.shell_state.session.active_tab_mut().zoom_level =
                                self.config.engine.zoom_default;
                            self.shell_state.active_tab_zoom = self.config.engine.zoom_default;
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
                        if ui.button("Save Session").clicked()
                            && save_session(
                                &self.shell_state.runtime_paths.session_path,
                                &self.shell_state.session,
                            )
                            .is_ok()
                        {
                            self.shell_state.record_event("session saved");
                        }
                        if ui.button("Load Session").clicked()
                            && let Ok(session) =
                                load_session(&self.shell_state.runtime_paths.session_path)
                        {
                            self.shell_state.session = session;
                            let tab = self.shell_state.session.active_tab_mut().clone();
                            self.shell_state.address_bar_input = tab.url.clone();
                            self.shell_state.record_event("session loaded");
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
                            self.engine.inject_event(EngineEvent::ContextMenuRequested {
                                x: 120.0,
                                y: 88.0,
                            });
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
        }

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
            if let Some(stats) = self.frame_probe {
                ui.label(format!(
                    "Probe: non-white {:.1}% avg rgb {} {} {} alpha min {} avg {:.0}",
                    stats.non_white_ratio * 100.0,
                    stats.avg_r,
                    stats.avg_g,
                    stats.avg_b,
                    stats.alpha_min,
                    stats.alpha_avg
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
            let resource_status = match self.shell_state.resource_reader_ready {
                Some(true) => "ok",
                Some(false) => "missing",
                None => "unknown",
            };
            let load_status = self
                .shell_state
                .load_status
                .map(|status| status.as_str())
                .unwrap_or("n/a");
            let last_error = self
                .shell_state
                .upstream_last_error
                .as_deref()
                .unwrap_or("none");
            ui.label(format!(
                "Render health: resource_reader={} upstream_active={} load_status={} last_error={}",
                resource_status, self.shell_state.upstream_active, load_status, last_error
            ));
            if let Some(warning) = &self.shell_state.render_warning {
                ui.colored_label(eframe::egui::Color32::YELLOW, warning);
            }
            if let Some(texture) = &self.render_texture {
                let response =
                    ui.add(eframe::egui::Image::from_texture(texture).shrink_to_fit());
                self.render_viewport_rect = Some(response.rect);
                if self.config.engine.debug_pointer_overlay
                    && let Some(pos) = self.last_pointer_pos
                    && response.rect.contains(pos)
                {
                    let painter = ui.painter().with_clip_rect(response.rect);
                    let stroke =
                        eframe::egui::Stroke::new(1.0, eframe::egui::Color32::YELLOW);
                    let offset = eframe::egui::vec2(8.0, 0.0);
                    painter.line_segment([pos - offset, pos + offset], stroke);
                    let offset = eframe::egui::vec2(0.0, 8.0);
                    painter.line_segment([pos - offset, pos + offset], stroke);
                }
            } else {
                self.render_viewport_rect = None;
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

        self.render_workspace_settings(ctx);
        self.render_bookmarks_panel(ctx);
        self.render_history_panel(ctx);
        self.render_downloads_panel(ctx);
        self.render_dom_inspector_panel(ctx);
        self.render_network_inspector_panel(ctx);
        self.render_cache_explorer_panel(ctx);
        self.render_capability_inspector_panel(ctx);
        self.render_automation_console_panel(ctx);
        self.render_knowledge_graph_panel(ctx);
        self.render_reading_queue_panel(ctx);
        self.render_tts_controls_panel(ctx);

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

        if self.shell_state.find_panel_open {
            eframe::egui::TopBottomPanel::bottom("find_panel")
                .resizable(false)
                .default_height(64.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Find");
                        let response = ui.text_edit_singleline(&mut self.shell_state.find_query);
                        let enter_pressed =
                            ui.input(|input| input.key_pressed(eframe::egui::Key::Enter));
                        if response.changed() && !self.shell_state.find_query.is_empty() {
                            self.shell_state.record_event(format!(
                                "find query: {}",
                                self.shell_state.find_query
                            ));
                        }
                        if (response.lost_focus() && enter_pressed)
                            || ui.button("Find Next").clicked()
                        {
                            self.shell_state.record_event(format!(
                                "find next: {}",
                                self.shell_state.find_query
                            ));
                        }
                        if ui.button("Close").clicked() {
                            self.shell_state.find_panel_open = false;
                        }
                    });
                });
        }

        self.render_command_palette(ctx);
        self.render_context_menu(ctx);
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

    fn test_paths() -> RuntimePaths {
        RuntimePaths {
            config_path: "brazen.toml".into(),
            data_dir: "data".into(),
            logs_dir: "logs".into(),
            profiles_dir: "profiles".into(),
            cache_dir: "cache".into(),
            downloads_dir: "downloads".into(),
            crash_dumps_dir: "crash-dumps".into(),
            active_profile_dir: "profiles/default".into(),
            session_path: "profiles/default/session.json".into(),
        }
    }

    fn build_test_app() -> BrazenApp {
        let config = BrazenConfig::default();
        let paths = test_paths();
        let engine_factory = crate::engine::ServoEngineFactory;
        let shell_state = build_shell_state(&config, &paths, &engine_factory);
        BrazenApp::new(config, shell_state, None)
    }

    struct MockEngine {
        status: EngineStatus,
        tab: BrowserTab,
        events: Vec<EngineEvent>,
        zoom: f32,
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

        fn set_page_zoom(&mut self, zoom: f32) {
            self.zoom = zoom;
        }

        fn page_zoom(&self) -> f32 {
            self.zoom
        }

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

        fn evaluate_javascript(&mut self, _script: String, callback: Box<dyn FnOnce(Result<serde_json::Value, String>) + Send + 'static>) {
            callback(Ok(serde_json::Value::Null));
        }

        fn take_screenshot(&mut self) -> Result<Vec<u8>, String> {
            Err("MockEngine does not support screenshots".to_string())
        }
    }

    #[test]
    fn shell_state_sync_handles_ready_and_error_statuses() {
        let paths = test_paths();
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
            load_status: None,
            favicon_url: None,
            metadata_summary: None,
            history: Vec::new(),
            last_committed_url: None,
            active_tab_zoom: 1.0,
            cursor_icon: None,
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
            resource_reader_ready: None,
            resource_reader_path: None,
            upstream_active: false,
            upstream_last_error: None,
            render_warning: None,
            session: SessionSnapshot::new("default".to_string(), "now".to_string()),
            event_log: Vec::new(),
            log_panel_open: true,
            permission_panel_open: false,
            find_panel_open: false,
            find_query: String::new(),
            capabilities_snapshot: Vec::new(),
            runtime_paths: paths, mount_manager: crate::mounts::MountManager::new(),
        };

        let mut ready_engine = MockEngine {
            status: EngineStatus::Ready,
            tab: BrowserTab {
                id: 1,
                title: "Example".to_string(),
                current_url: "https://example.com".to_string(),
            },
            events: vec![EngineEvent::StatusChanged(EngineStatus::Ready)],
            zoom: 1.0,
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
            zoom: 1.0,
        };
        shell.sync_from_engine(&mut failing_engine);
        assert_eq!(
            shell.engine_status,
            EngineStatus::Error("boot failed".to_string())
        );
    }

    #[test]
    fn zoom_steps_clamp_to_config_bounds() {
        let mut app = build_test_app();
        app.apply_zoom_steps(10, "test");
        assert!((app.shell_state.active_tab_zoom - 2.0).abs() < f32::EPSILON);
        app.apply_zoom_steps(200, "test");
        assert!((app.shell_state.active_tab_zoom - app.config.engine.zoom_max).abs() < 0.001);
        app.apply_zoom_steps(-200, "test");
        assert!((app.shell_state.active_tab_zoom - app.config.engine.zoom_min).abs() < 0.001);
    }

    #[test]
    fn context_menu_event_sets_pending_state() {
        let paths = test_paths();
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
            load_status: None,
            favicon_url: None,
            metadata_summary: None,
            history: Vec::new(),
            last_committed_url: None,
            active_tab_zoom: 1.0,
            cursor_icon: None,
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
            resource_reader_ready: None,
            resource_reader_path: None,
            upstream_active: false,
            upstream_last_error: None,
            render_warning: None,
            session: SessionSnapshot::new("default".to_string(), "now".to_string()),
            event_log: Vec::new(),
            log_panel_open: true,
            permission_panel_open: false,
            find_panel_open: false,
            find_query: String::new(),
            capabilities_snapshot: Vec::new(),
            runtime_paths: paths, mount_manager: crate::mounts::MountManager::new(),
        };

        let mut engine = MockEngine {
            status: EngineStatus::Ready,
            tab: shell.active_tab.clone(),
            events: vec![EngineEvent::ContextMenuRequested { x: 120.0, y: 88.0 }],
            zoom: 1.0,
        };

        shell.sync_from_engine(&mut engine);
        assert!(shell.pending_context_menu.is_some());
    }
}
