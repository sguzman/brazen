use brazen::ShellState;
use brazen::engine::{BrowserEngine, EngineStatus, EngineFrame, EngineEvent, RenderHealth, BrowserTab, RenderSurfaceMetadata, EngineInstanceId, FocusState, InputEvent, ImeEvent, ClipboardRequest, RenderSurfaceHandle};
use brazen::platform_paths::RuntimePaths;
use brazen::session::SessionSnapshot;
use brazen::mounts::MountManager;
use std::sync::{Arc, RwLock};

struct MockEngine {
    status: EngineStatus,
    health: RenderHealth,
}

impl BrowserEngine for MockEngine {
    fn instance_id(&self) -> EngineInstanceId { 1 }
    fn backend_name(&self) -> &'static str { "mock" }
    fn status(&self) -> EngineStatus { self.status.clone() }
    fn health(&self) -> RenderHealth { self.health.clone() }
    fn active_tab(&self) -> &BrowserTab {
        static TAB: BrowserTab = BrowserTab { id: 0, title: String::new(), current_url: String::new() };
        &TAB
    }
    fn navigate(&mut self, _url: &str) {}
    fn render_frame(&mut self) -> Option<EngineFrame> { None }
    fn handle_input(&mut self, _event: InputEvent) {}
    fn take_events(&mut self) -> Vec<EngineEvent> { Vec::new() }
    fn set_render_surface(&mut self, _meta: RenderSurfaceMetadata) {}
    fn shutdown(&mut self) {}
    fn inject_event(&mut self, _event: EngineEvent) {}
    fn evaluate_javascript(&mut self, _script: String, _callback: Box<dyn FnOnce(Result<serde_json::Value, String>) + Send + 'static>) {
        _callback(Ok(serde_json::json!(null)));
    }
    fn interact_dom(&mut self, _selector: String, _event: String, _value: Option<String>, _callback: Box<dyn FnOnce(Result<(), String>) + Send + 'static>) {
        _callback(Ok(()));
    }
    fn take_screenshot(&mut self) -> Result<EngineFrame, String> {
        Err("not implemented".to_string())
    }
    fn set_focus(&mut self, _focus: FocusState) {}
    fn handle_ime(&mut self, _event: ImeEvent) {}
    fn handle_clipboard(&mut self, _request: ClipboardRequest) {}
    fn set_page_zoom(&mut self, _zoom: f32) {}
    fn page_zoom(&self) -> f32 { 1.0 }
    fn set_verbose_logging(&mut self, _enabled: bool) {}
    fn configure_devtools(&mut self, _enabled: bool, _transport: &str) {}
    fn suspend(&mut self) {}
    fn resume(&mut self) {}
    fn reload(&mut self) {}
    fn stop(&mut self) {}
    fn go_back(&mut self) {}
    fn go_forward(&mut self) {}
    fn attach_surface(&mut self, _handle: RenderSurfaceHandle) {}
}

fn create_mock_shell_state() -> ShellState {
    let root = std::path::PathBuf::from("/tmp/brazen");
    ShellState {
        app_name: "test".to_string(),
        backend_name: "mock".to_string(),
        engine_instance_id: 1,
        engine_status: EngineStatus::Ready,
        active_tab: BrowserTab { id: 0, title: String::new(), current_url: String::new() },
        address_bar_input: String::new(),
        page_title: String::new(),
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
        session: Arc::new(RwLock::new(SessionSnapshot::new("test".to_string(), "now".to_string()))),
        event_log: Vec::new(),
        log_panel_open: false,
        permission_panel_open: false,
        find_panel_open: false,
        find_query: String::new(),
        capabilities_snapshot: Vec::new(),
        automation_activities: Vec::new(),
        tts_queue: std::collections::VecDeque::new(),
        tts_playing: false,
        reading_queue: std::collections::VecDeque::new(),
        mount_manager: MountManager::new(),
        runtime_paths: RuntimePaths {
            config_path: root.join("brazen.toml"),
            data_dir: root.join("data"),
            logs_dir: root.join("logs"),
            profiles_dir: root.join("profiles"),
            cache_dir: root.join("cache"),
            downloads_dir: root.join("downloads"),
            crash_dumps_dir: root.join("crashes"),
            active_profile_dir: root.join("profiles/default"),
            session_path: root.join("session.json"),
            audit_log_path: root.join("audit.log"),
        },
        pending_window_screenshot: Arc::new(std::sync::Mutex::new(None)),
    }
}

#[test]
fn test_engine_health_synchronization() {
    let mut state = create_mock_shell_state();
    let health = RenderHealth {
        resource_reader_ready: Some(true),
        resource_reader_path: Some("/tmp/mock".to_string()),
        upstream_active: true,
        last_error: Some("test error".to_string()),
    };
    let mut engine = MockEngine {
        status: EngineStatus::Ready,
        health: health.clone(),
    };

    state.sync_from_engine(&mut engine);

    assert_eq!(state.engine_status, EngineStatus::Ready);
    assert_eq!(state.upstream_active, true);
    assert_eq!(state.resource_reader_ready, Some(true));
    assert_eq!(state.resource_reader_path, Some("/tmp/mock".to_string()));
    assert_eq!(state.upstream_last_error, Some("test error".to_string()));
}

#[test]
fn test_event_log_retention() {
    let mut state = create_mock_shell_state();
    state.record_event("event 1");
    state.record_event("event 2");
    
    assert_eq!(state.event_log.len(), 2);
    assert_eq!(state.event_log[0], "event 1");
}
