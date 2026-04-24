use crate::engine::{BrowserEngine, EngineEvent, EngineStatus, SecurityWarningKind, WindowDisposition, BrowserTab, DialogKind};
use crate::extraction::extract_entities;
use std::collections::VecDeque;
use crate::platform_paths::RuntimePaths;
use std::sync::{Arc, RwLock};

use chrono::Utc;
use crate::session::{NavigationEntry, SessionSnapshot};
use crate::engine::EngineLoadStatus;
use crate::profile_db::ProfileDb;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceLayout {
    pub panels: WorkspacePanels,
    pub theme: UiTheme,
    pub density: UiDensity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiTheme {
    System,
    Light,
    Dark,
    Brazen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiDensity {
    Compact,
    Comfortable,
}

#[derive(Debug, Clone, Copy)]
pub enum LayoutPreset {
    Default,
    Developer,
    Archive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsTab {
    Layout,
    Features,
    DevTools,
    Automation,
    Appearance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeftPanelTab {
    Workspace,
    Assets,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WorkspacePanels {
    pub sidebar_visible: bool,
    pub bookmarks: bool,
    pub history: bool,
    pub reading_queue: bool,
    pub reader_mode: bool,
    pub tts_controls: bool,
    pub workspace_settings: bool,
    pub terminal: bool,
    pub dashboard: bool,
    pub find_panel_open: bool,
    pub bottom_panel_visible: bool,
    pub active_diagnostic_tab: DiagnosticTab,
    pub bottom_panel_height: f32,
}

impl Default for WorkspacePanels {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            bookmarks: false,
            history: false,
            reading_queue: false,
            reader_mode: false,
            tts_controls: false,
            workspace_settings: false,
            terminal: false,
            dashboard: true,
            find_panel_open: false,
            bottom_panel_visible: false,
            active_diagnostic_tab: DiagnosticTab::Logs,
            bottom_panel_height: 250.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingQueueItem {
    pub url: String,
    pub title: Option<String>,
    pub kind: String,
    pub saved_at: String,
    pub progress: f32,
    pub article_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub kind: String,
    pub value: String,
    pub label: String,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticTab {
    Logs,
    Network,
    Dom,
    Health,
    Downloads,
    Automation,
    Cache,
    Capabilities,
    KnowledgeGraph,
}


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
    pub session: Arc<RwLock<SessionSnapshot>>,
    pub event_log: Vec<String>,
    pub log_panel_open: bool,
    pub permission_panel_open: bool,
    pub find_panel_open: bool,
    pub find_query: String,
    pub capabilities_snapshot: Vec<(String, String)>,
    pub automation_activities: Vec<crate::automation::AutomationActivity>,
    pub tts_queue: VecDeque<String>,
    pub tts_playing: bool,
    pub reading_queue: VecDeque<ReadingQueueItem>,
    pub reader_mode_open: bool,
    pub reader_mode_source_url: Option<String>,
    pub reader_mode_text: String,
    pub visit_counts: HashMap<String, u32>,
    pub visit_total: u64,
    pub revisit_total: u64,
    pub mount_manager: crate::mounts::MountManager,
    pub runtime_paths: RuntimePaths,
    pub pending_window_screenshot: Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<Result<crate::engine::EngineFrame, String>>>>>,
    pub dom_snapshot: Option<String>,
    pub network_log: VecDeque<crate::engine::NetworkRequest>,
    pub extracted_entities: Vec<ExtractedEntity>,
    pub terminal_history: Vec<String>,
    pub terminal_input: String,
    pub terminal_busy: bool,
    pub observe_dom: bool,
    pub control_terminal: bool,
    pub use_mcp_tools: bool,
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

        let health = engine.health();
        self.resource_reader_ready = health.resource_reader_ready;
        self.resource_reader_path = health.resource_reader_path;
        self.upstream_active = health.upstream_active;
        self.upstream_last_error = health.last_error;
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
                        self.visit_total += 1;
                        let entry = self.visit_counts.entry(state.url.clone()).or_insert(0);
                        *entry += 1;
                        if *entry > 1 {
                            self.revisit_total += 1;
                        }
                        self.history.push(state.url.clone());
                        self.last_committed_url = Some(state.url.clone());
                        if let Ok(db) = ProfileDb::open(self.runtime_paths.active_profile_dir.join("state.sqlite")) {
                            let _ = db.append_history(&state.url, Some(&state.title), &Utc::now().to_rfc3339());
                            let _ = db.save_tts_state(self.tts_playing, &self.tts_queue.iter().cloned().collect::<Vec<_>>());
                            let _ = db.save_visit_stats(self.visit_total, self.revisit_total, &self.visit_counts);
                            for item in self.reading_queue.iter() {
                                let _ = db.upsert_reading_item(item);
                            }
                        }
                        let entry = NavigationEntry {
                            url: state.url.clone(),
                            title: state.title.clone(),
                            timestamp: Utc::now().to_rfc3339(),
                            redirect_chain: state.redirect_chain.clone(),
                        };
                        self.session.write().unwrap().commit_navigation(entry);
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
                    {
                        let mut session = self.session.write().unwrap();
                        session.active_tab_mut().downloads.push(message.clone());
                    }
                    self.record_event(format!("download requested: {message}"));
                }
                EngineEvent::SecurityWarning { kind, url } => {
                    self.last_security_warning = Some((kind.clone(), url.clone()));
                    self.record_event(format!("security warning: {kind:?} {url}"));
                }
                EngineEvent::Crashed { reason } => {
                    self.last_crash = Some(reason.clone());
                    self.session.write().unwrap().crash_recovery_pending = true;
                    self.record_event(format!("engine crashed: {reason}"));
                }
                EngineEvent::DomSnapshotUpdated(snapshot) => {
                    self.extracted_entities = extract_entities(&snapshot);
                    self.dom_snapshot = Some(snapshot);
                }
                EngineEvent::NetworkRequestLogged(request) => {
                    if self.network_log.len() >= 500 {
                        self.network_log.pop_front();
                    }
                    self.network_log.push_back(request);
                }
                other => {
                    self.record_event(format!("engine event: {other:?}"));
                }
            }
        }
    }
}