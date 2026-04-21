use chrono::{Local, Utc};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tokio::sync::{Semaphore, broadcast, mpsc};
use std::collections::VecDeque;
use url::Url;

use crate::audit_log::{AuditLogger, AuditEntry};
use crate::cache::{AssetMetadata, AssetQuery, AssetStore, CacheStats};
use crate::config::{AutomationConfig, BrazenConfig, CacheConfig};
use crate::permissions::{Capability, PermissionDecision, PermissionPolicy};
use crate::platform_paths::RuntimePaths;
use crate::session::SessionSnapshot;
use crate::{ShellState, commands};
use base64::Engine;
use crate::engine::{EngineFrame, PixelFormat};
use image::ImageFormat;
use std::io::Cursor;
use std::path::Path;
use std::collections::HashMap;
use uuid::Uuid;
use tokio::io::{AsyncBufReadExt, BufReader};
use flate2::write::GzEncoder;
use flate2::Compression;
use tar::{Archive, Builder};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationSnapshot {
    pub tabs: Vec<AutomationTab>,
    pub active_tab_index: usize,
    pub active_tab_id: Option<String>,
    pub address_bar: String,
    pub page_title: String,
    pub last_committed_url: Option<String>,
    pub load_status: Option<String>,
    pub load_progress: f32,
    pub document_ready: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub engine_status: String,
    pub upstream_active: bool,
    pub upstream_last_error: Option<String>,
    pub last_security_warning: Option<String>,
    pub last_crash: Option<String>,
    pub log_panel_open: bool,
    pub permission_panel_open: bool,
    pub find_panel_open: bool,
    pub cache_stats: CacheStats,
    pub cache_entries: Vec<AutomationAssetSummary>,
    pub activities: VecDeque<AutomationActivity>,
    pub last_event_log_len: usize,
    pub tts_queue_len: usize,
    pub tts_playing: bool,
    pub reading_queue_len: usize,
    pub reading_queue_urls: Vec<String>,
    pub reader_mode_open: bool,
    pub reader_mode_source_url: Option<String>,
    pub reader_mode_text_len: usize,
    pub visit_total: u64,
    pub revisit_total: u64,
    pub unique_visit_urls: usize,
}

impl Default for AutomationSnapshot {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_index: 0,
            active_tab_id: None,
            address_bar: String::new(),
            page_title: String::new(),
            last_committed_url: None,
            load_status: None,
            load_progress: 0.0,
            document_ready: false,
            can_go_back: false,
            can_go_forward: false,
            engine_status: "unknown".to_string(),
            upstream_active: false,
            upstream_last_error: None,
            last_security_warning: None,
            last_crash: None,
            log_panel_open: false,
            permission_panel_open: false,
            find_panel_open: false,
            cache_stats: CacheStats {
                entries: 0,
                total_bytes: 0,
                captured_with_body: 0,
                unique_blobs: 0,
                capture_ratio: 0.0,
            },
            cache_entries: Vec::new(),
            activities: VecDeque::with_capacity(128),
            last_event_log_len: 0,
            tts_queue_len: 0,
            tts_playing: false,
            reading_queue_len: 0,
            reading_queue_urls: Vec::new(),
            reader_mode_open: false,
            reader_mode_source_url: None,
            reader_mode_text_len: 0,
            visit_total: 0,
            revisit_total: 0,
            unique_visit_urls: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationActivity {
    pub id: String,
    pub command: String,
    pub status: AutomationActivityStatus,
    pub timestamp: String,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AutomationActivityStatus {
    Pending,
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationTab {
    pub tab_id: String,
    pub index: usize,
    pub title: String,
    pub url: String,
    pub zoom: f32,
    pub pinned: bool,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationAssetSummary {
    pub asset_id: String,
    pub url: String,
    pub status_code: Option<u16>,
    pub mime: String,
    pub size_bytes: u64,
    pub hash: Option<String>,
    pub created_at: String,
    pub session_id: Option<String>,
    pub tab_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationNavigationEvent {
    pub url: String,
    pub title: String,
    pub load_status: Option<String>,
    pub load_progress: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationCapabilityEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "topic", rename_all = "kebab-case")]
pub enum AutomationEvent {
    Navigation(AutomationNavigationEvent),
    Capability(AutomationCapabilityEvent),
    TerminalOutput(AutomationTerminalOutputEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationTerminalOutputEvent {
    pub session_id: String,
    pub stream: String,
    pub chunk: String,
    pub done: bool,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationEnvelope<T> {
    pub id: Option<String>,
    #[serde(flatten)]
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AutomationRequest {
    WindowList,
    LogSubscribe,
    TabList,
    Snapshot,
    TabActivate {
        index: Option<usize>,
        tab_id: Option<String>,
    },
    TabNew {
        url: Option<String>,
    },
    TabClose {
        index: Option<usize>,
        tab_id: Option<String>,
    },
    TabNavigate {
        url: String,
    },
    TabReload,
    TabStop,
    TabBack,
    TabForward,
    DomQuery {
        selector: String,
    },
    Screenshot,
    ScreenshotMeta,
    RenderedText,
    ArticleText,
    CacheStats,
    CacheQuery {
        query: Option<AssetQuery>,
        limit: Option<usize>,
    },
    CacheBody {
        asset_id: String,
    },
    Subscribe {
        topics: Vec<String>,
    },
    TtsControl {
        action: String,
    },
    TtsEnqueue {
        text: String,
    },
    ReadingEnqueue {
        url: String,
        title: Option<String>,
        kind: Option<String>,
        article_text: Option<String>,
    },
    ReadingSetProgress {
        url: String,
        progress: f32,
    },
    ReadingRemove {
        url: String,
    },
    ReadingClear,
    ReaderModeOpen {
        url: String,
    },
    ReaderModeClose,
    MountAdd {
        name: String,
        local_path: String,
        read_only: Option<bool>,
        allowed_domains: Option<Vec<String>>,
    },
    MountRemove {
        name: String,
    },
    MountList,
    EvaluateJavascript {
        script: String,
    },
    TerminalExec {
        cmd: String,
        args: Vec<String>,
        cwd: Option<String>,
    },
    TerminalExecStream {
        cmd: String,
        args: Vec<String>,
        cwd: Option<String>,
    },
    TerminalCancel {
        session_id: String,
    },
    FsList {
        url: String,
    },
    FsRead {
        url: String,
    },
    FsWrite {
        url: String,
        body_base64: String,
    },
    ApprovalRespond {
        approval_id: String,
        decision: String,
    },
    InteractDom {
        selector: String,
        event: String,
        value: Option<String>,
    },
    ScreenshotWindow,
    ProfileCreate {
        profile_id: String,
    },
    ProfileSwitch {
        profile_id: String,
    },
    ProfileExport {
        profile_id: String,
        output_path: String,
        include_cache_blobs: Option<bool>,
    },
    ProfileImport {
        profile_id: String,
        input_path: String,
        overwrite: Option<bool>,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationResponse<T> {
    pub id: Option<String>,
    pub ok: bool,
    pub result: Option<T>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum AutomationCommand {
    ActivateTab { index: usize },
    CloseTab { index: usize },
    NewTab { url: Option<String> },
    Navigate { url: String },
    Reload,
    Stop,
    GoBack,
    GoForward,
    DomQuery {
        selector: String,
        response_tx: tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>,
    },
    Screenshot {
        response_tx: tokio::sync::oneshot::Sender<Result<EngineFrame, String>>,
    },
    EvaluateJavascript {
        script: String,
        response_tx: tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>,
    },
    AddMount {
        name: String,
        local_path: std::path::PathBuf,
        read_only: bool,
        allowed_domains: Vec<String>,
    },
    RemoveMount {
        name: String,
    },
    RenderedText {
        response_tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    ArticleText {
        response_tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    CacheStats {
        response_tx: tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>,
    },
    CacheQuery {
        query: Option<AssetQuery>,
        limit: Option<usize>,
        response_tx: tokio::sync::oneshot::Sender<Result<Vec<AutomationAssetSummary>, String>>,
    },
    CacheBody {
        asset_id: String,
        response_tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    TtsControl {
        action: String,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    TtsEnqueue {
        text: String,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ReadingEnqueue {
        url: String,
        title: Option<String>,
        kind: String,
        article_text: Option<String>,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ReadingSetProgress {
        url: String,
        progress: f32,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ReadingRemove {
        url: String,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ReadingClear {
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ReaderModeOpen {
        url: String,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ReaderModeClose {
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    InteractDom {
        selector: String,
        event: String,
        value: Option<String>,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ScreenshotWindow {
        response_tx: tokio::sync::oneshot::Sender<Result<EngineFrame, String>>,
    },
}

#[derive(Clone)]
pub struct AutomationHandle {
    snapshot: Arc<RwLock<AutomationSnapshot>>,
    command_tx: mpsc::UnboundedSender<AutomationCommand>,
    event_tx: broadcast::Sender<AutomationEvent>,
    #[allow(dead_code)]
    cache_config: CacheConfig,
    #[allow(dead_code)]
    runtime_paths: RuntimePaths,
    #[allow(dead_code)]
    profile_id: String,
    permissions: PermissionPolicy,
    expose_tab_api: bool,
    #[allow(dead_code)]
    expose_cache_api: bool,
    terminal_config: crate::config::TerminalConfig,
    pub mount_manager: crate::mounts::MountManager,
    activity_counter: Arc<AtomicU64>,
    egui_ctx: Arc<RwLock<Option<eframe::egui::Context>>>,
}

impl AutomationHandle {
    pub fn set_egui_context(&self, ctx: eframe::egui::Context) {
        let mut lock = self.egui_ctx.write().expect("egui ctx lock");
        *lock = Some(ctx);
    }

    pub fn request_repaint(&self) {
        if let Some(ctx) = self.egui_ctx.read().expect("egui ctx lock").as_ref() {
            ctx.request_repaint();
        }
    }

    pub fn request_shutdown(&self) -> Result<(), String> {
        let ctx = self
            .egui_ctx
            .read()
            .expect("egui ctx lock")
            .as_ref()
            .cloned()
            .ok_or_else(|| "egui context not available".to_string())?;
        ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
        Ok(())
    }
    pub fn snapshot(&self) -> AutomationSnapshot {
        self.snapshot.read().expect("automation snapshot lock").clone()
    }

    pub fn update_snapshot(&self, shell_state: &ShellState, cache: &AssetStore) {
        let mut snapshot = self.snapshot.write().expect("automation snapshot lock");
        let session = shell_state.session.read().expect("session lock");
        snapshot.tabs = build_tab_list(&session);
        snapshot.active_tab_index = session
            .active_tab()
            .and_then(|tab| {
                session
                    .windows
                    .get(session.active_window)
                    .and_then(|window| window.tabs.iter().position(|t| t.id == tab.id))
            })
            .unwrap_or(0);
        snapshot.active_tab_id = session
            .active_tab()
            .map(|tab| tab.id.0.to_string());
        snapshot.address_bar = shell_state.address_bar_input.clone();
        snapshot.page_title = shell_state.page_title.clone();
        snapshot.last_committed_url = shell_state.last_committed_url.clone();
        snapshot.load_status = shell_state
            .load_status
            .map(|status| status.as_str().to_string());
        snapshot.load_progress = shell_state.load_progress;
        snapshot.document_ready = shell_state.document_ready;
        snapshot.can_go_back = shell_state.can_go_back;
        snapshot.can_go_forward = shell_state.can_go_forward;
        snapshot.engine_status = shell_state.engine_status.to_string();
        snapshot.upstream_active = shell_state.upstream_active;
        snapshot.upstream_last_error = shell_state.upstream_last_error.clone();
        snapshot.last_security_warning = shell_state
            .last_security_warning
            .as_ref()
            .map(|(kind, message)| format!("{kind:?}: {message}"));
        snapshot.last_crash = shell_state.last_crash.clone();
        snapshot.log_panel_open = shell_state.log_panel_open;
        snapshot.permission_panel_open = shell_state.permission_panel_open;
        snapshot.find_panel_open = shell_state.find_panel_open;
        snapshot.cache_stats = cache.stats();
        snapshot.cache_entries = cache
            .entries()
            .iter()
            .take(512)
            .map(asset_summary_from_metadata)
            .collect();
        snapshot.last_event_log_len = shell_state.event_log.len();
        snapshot.tts_queue_len = shell_state.tts_queue.len();
        snapshot.tts_playing = shell_state.tts_playing;
        snapshot.reading_queue_len = shell_state.reading_queue.len();
        snapshot.reading_queue_urls = shell_state
            .reading_queue
            .iter()
            .take(16)
            .map(|item| item.url.clone())
            .collect();
        snapshot.reader_mode_open = shell_state.reader_mode_open;
        snapshot.reader_mode_source_url = shell_state.reader_mode_source_url.clone();
        snapshot.reader_mode_text_len = shell_state.reader_mode_text.len();
        snapshot.visit_total = shell_state.visit_total;
        snapshot.revisit_total = shell_state.revisit_total;
        snapshot.unique_visit_urls = shell_state.visit_counts.len();
    }

    pub fn publish_navigation(&self, event: AutomationNavigationEvent) {
        let _ = self.event_tx.send(AutomationEvent::Navigation(event));
    }

    pub fn publish_capability(&self, event: AutomationCapabilityEvent) {
        let _ = self.event_tx.send(AutomationEvent::Capability(event));
    }

    pub fn record_activity(&self, activity: AutomationActivity) {
        let mut snapshot = self.snapshot.write().expect("automation snapshot lock");
        if snapshot.activities.len() >= 128 {
            snapshot.activities.pop_front();
        }
        snapshot.activities.push_back(activity);
    }

    pub fn take_command_sender(&self) -> mpsc::UnboundedSender<AutomationCommand> {
        self.command_tx.clone()
    }
}

pub struct AutomationRuntime {
    pub handle: AutomationHandle,
    pub command_rx: mpsc::UnboundedReceiver<AutomationCommand>,
}

pub fn start_automation_runtime(
    config: &BrazenConfig,
    paths: &RuntimePaths,
    mount_manager: crate::mounts::MountManager,
) -> Option<AutomationRuntime> {
    if !config.automation.enabled || !config.features.automation_server {
        return None;
    }
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    let (event_tx, _) = broadcast::channel(config.automation.max_subscriptions.max(16) as usize);
    let handle = AutomationHandle {
        snapshot: Arc::new(RwLock::new(AutomationSnapshot::default())),
        command_tx,
        event_tx,
        cache_config: config.cache.clone(),
        runtime_paths: paths.clone(),
        profile_id: config.profiles.active_profile.clone(),
        permissions: config.permissions.clone(),
        expose_tab_api: config.automation.expose_tab_api,
        expose_cache_api: config.automation.expose_cache_api,
        terminal_config: config.terminal.clone(),
        mount_manager,
        activity_counter: Arc::new(AtomicU64::new(0)),
        egui_ctx: Arc::new(RwLock::new(None)),
    };
    let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
    tracing::info!(target: "brazen::automation", path = %paths.audit_log_path.display(), "audit logging initialized");
    let server_state = AutomationServerState::new(config.automation.clone(), handle.clone(), audit_logger);
    let bind = config.automation.bind.clone();
    std::thread::spawn(move || {
        if let Err(error) = run_automation_server(&bind, server_state) {
            tracing::error!(target: "brazen::automation", %error, "automation server failed");
        }
    });

    Some(AutomationRuntime { handle, command_rx })
}

#[derive(Clone)]
struct AutomationServerState {
    handle: AutomationHandle,
    auth_token: Option<String>,
    require_auth: bool,
    max_messages_per_minute: u32,
    max_subscriptions: usize,
    connection_semaphore: Arc<Semaphore>,
    audit_logger: Arc<AuditLogger>,
    pending_approvals: Arc<RwLock<HashMap<String, PendingApproval>>>,
    rate_limit: Arc<RwLock<HashMap<String, (Instant, u32)>>>,
    terminal_sessions: Arc<RwLock<HashMap<String, Arc<tokio::sync::Mutex<tokio::process::Child>>>>>,
}

#[derive(Debug, Clone)]
struct PendingApproval {
    capability: Capability,
    request: AutomationRequest,
    user_agent: Option<String>,
    client_ip: Option<String>,
}

impl AutomationServerState {
    fn new(config: AutomationConfig, handle: AutomationHandle, audit_logger: Arc<AuditLogger>) -> Self {
        let max_connections = config.max_connections.max(1) as usize;
        Self {
            handle,
            auth_token: config.auth_token,
            require_auth: config.require_auth,
            max_messages_per_minute: config.max_messages_per_minute.max(1),
            max_subscriptions: config.max_subscriptions.max(1) as usize,
            connection_semaphore: Arc::new(Semaphore::new(max_connections)),
            audit_logger,
            pending_approvals: Arc::new(RwLock::new(HashMap::new())),
            rate_limit: Arc::new(RwLock::new(HashMap::new())),
            terminal_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn check_permission(&self, capability: Capability) -> Result<(), String> {
        match self.handle.permissions.decision_for(&capability) {
            PermissionDecision::Allow => Ok(()),
            PermissionDecision::Ask => Err("approval-required".to_string()),
            PermissionDecision::Deny => Err("capability denied".to_string()),
        }
    }

    fn check_rate_limit(&self, key: &str, max_per_minute: u32) -> Result<(), String> {
        let mut map = self.rate_limit.write().expect("rate limit lock");
        let now = Instant::now();
        let entry = map.entry(key.to_string()).or_insert((now, 0));
        if entry.0.elapsed() > Duration::from_secs(60) {
            *entry = (now, 0);
        }
        entry.1 += 1;
        if entry.1 > max_per_minute.max(1) {
            return Err("rate-limit".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct AuthQuery {
    token: Option<String>,
}

fn run_automation_server(bind: &str, state: AutomationServerState) -> Result<(), String> {
    let url = Url::parse(bind).map_err(|error| format!("automation.bind invalid: {error}"))?;
    let scheme = url.scheme().to_string();
    let path = if url.path().is_empty() || url.path() == "/" {
        "/ws"
    } else {
        url.path()
    };
    let host = url.host_str().unwrap_or("127.0.0.1").to_string();
    let port = url.port().unwrap_or(7942);
    let router = Router::new()
        .route(path, get(ws_handler))
        .with_state(state.clone());

    let unix_socket_path = if scheme == "ws+unix" {
        #[cfg(unix)]
        {
            Some(url.to_file_path().map_err(|_| "ws+unix bind must be a file path".to_string())?)
        }
        #[cfg(not(unix))]
        {
            return Err("ws+unix is only supported on unix platforms".to_string());
        }
    } else {
        None
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start automation runtime: {error}"))?;
    runtime.block_on(async move {
        match scheme.as_str() {
            "ws" | "wss" => {
                let addr: SocketAddr = format!("{host}:{port}")
                    .parse()
                    .map_err(|error| format!("invalid socket address: {error}"))?;
                tracing::info!(
                    target: "brazen::automation",
                    %addr,
                    path,
                    "automation server listening"
                );
                let listener = tokio::net::TcpListener::bind(addr)
                    .await
                    .map_err(|error| format!("automation bind failed: {error}"))?;
                if let Ok(endpoint_file) = std::env::var("BRAZEN_AUTOMATION_ENDPOINT_FILE") {
                    if !endpoint_file.trim().is_empty() {
                        let local_addr = listener
                            .local_addr()
                            .map_err(|error| format!("automation local_addr failed: {error}"))?;
                        let endpoint = format!(
                            "ws://{}:{}{}",
                            local_addr.ip(),
                            local_addr.port(),
                            path
                        );
                        let path = Path::new(&endpoint_file);
                        if let Some(parent) = path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::write(path, endpoint.as_bytes()).map_err(|error| {
                            format!("failed to write automation endpoint file: {error}")
                        })?;
                    }
                }
                axum::serve(listener, router)
                    .await
                    .map_err(|error| format!("automation server error: {error}"))?;
            }
            "ws+unix" => {
                #[cfg(not(unix))]
                {
                    return Err("ws+unix is only supported on unix platforms".to_string());
                }
                #[cfg(unix)]
                {
                    let socket_path = unix_socket_path.expect("unix_socket_path must be set for ws+unix");
                    if let Some(parent) = socket_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if socket_path.exists() {
                        let _ = std::fs::remove_file(&socket_path);
                    }
                    let listener = tokio::net::UnixListener::bind(&socket_path)
                        .map_err(|error| format!("failed to bind unix socket: {error}"))?;
                    tracing::info!(
                        target: "brazen::automation",
                        path = %socket_path.display(),
                        "automation unix socket listening"
                    );
                    let listener = listener;
                    axum::serve(listener, router)
                        .await
                        .map_err(|error| format!("automation server error: {error}"))?;
                }
            }
            _ => {
                return Err("automation.bind must use ws, wss, or ws+unix".to_string());
            }
        }
        Ok(())
    })
}

fn is_safe_relpath(path: &std::path::Path) -> bool {
    !path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

fn add_dir_to_tar(
    builder: &mut Builder<GzEncoder<std::fs::File>>,
    src_dir: &std::path::Path,
    tar_prefix: &str,
) -> Result<(), String> {
    if !src_dir.exists() {
        return Ok(());
    }
    let walk = walkdir::WalkDir::new(src_dir).into_iter();
    for entry in walk {
        let entry = entry.map_err(|e| format!("walk error: {e}"))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(src_dir)
            .map_err(|e| format!("strip prefix: {e}"))?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        if !is_safe_relpath(rel) {
            return Err("unsafe path in export".to_string());
        }
        let tar_path = std::path::Path::new(tar_prefix).join(rel);
        if entry.file_type().is_dir() {
            builder
                .append_dir(&tar_path, path)
                .map_err(|e| format!("append_dir failed: {e}"))?;
        } else if entry.file_type().is_file() {
            let mut file =
                std::fs::File::open(path).map_err(|e| format!("open file failed: {e}"))?;
            builder
                .append_file(&tar_path, &mut file)
                .map_err(|e| format!("append_file failed: {e}"))?;
        }
    }
    Ok(())
}

fn unpack_tar_to_dir(
    input_path: &std::path::Path,
    dest_dir: &std::path::Path,
    expected_prefix: &str,
    overwrite: bool,
) -> Result<(), String> {
    let file =
        std::fs::File::open(input_path).map_err(|e| format!("open bundle failed: {e}"))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    for entry in archive.entries().map_err(|e| format!("entries failed: {e}"))? {
        let mut entry = entry.map_err(|e| format!("entry failed: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("entry path failed: {e}"))?
            .into_owned();
        if !is_safe_relpath(&path) {
            return Err("unsafe path in bundle".to_string());
        }
        let mut comps = path.components();
        let Some(first) = comps.next() else {
            continue;
        };
        let first = first.as_os_str().to_string_lossy().to_string();
        if first != expected_prefix {
            continue;
        }
        let rel: std::path::PathBuf = comps.collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        if !is_safe_relpath(&rel) {
            return Err("unsafe rel path in bundle".to_string());
        }
        let out_path = dest_dir.join(rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir failed: {e}"))?;
        }
        if out_path.exists() && !overwrite {
            continue;
        }
        entry
            .unpack(&out_path)
            .map_err(|e| format!("unpack failed: {e}"))?;
    }
    Ok(())
}

fn ensure_mount_api(state: &AutomationServerState) -> Result<(), String> {
    if state
        .handle
        .permissions
        .decision_for(&Capability::VirtualResourceMount)
        == PermissionDecision::Deny
    {
        return Err("permission denied: virtual-resource-mount".to_string());
    }
    Ok(())
}

async fn ws_handler(
    State(state): State<AutomationServerState>,
    headers: HeaderMap,
    Query(query): Query<AuthQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if state.connection_semaphore.available_permits() == 0 {
        return (StatusCode::SERVICE_UNAVAILABLE, "connection limit reached").into_response();
    }

    if state.require_auth {
        let header_token = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(|value| value.to_string());
        let token = header_token.or(query.token);
        if token.as_deref() != state.auth_token.as_deref() {
            return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
        }
    }

    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());

    let permit = state
        .connection_semaphore
        .clone()
        .acquire_owned()
        .await
        .expect("semaphore");
    ws.on_upgrade(move |socket| handle_socket(socket, state, permit, user_agent))
}

async fn handle_socket(
    mut socket: WebSocket,
    state: AutomationServerState,
    _permit: tokio::sync::OwnedSemaphorePermit,
    user_agent: Option<String>,
) {
    let mut receiver = state.handle.event_tx.subscribe();
    let mut log_receiver = crate::logging::get_log_receiver();
    let mut subscribed_topics: Vec<String> = Vec::new();
    let mut message_count = 0u32;
    let mut window_start = Instant::now();

    loop {
        tokio::select! {
            biased;
            event = receiver.recv() => {
                if let Ok(event) = event {
                    let topic = match &event {
                        AutomationEvent::Navigation(_) => "navigation",
                        AutomationEvent::Capability(_) => "capability",
                        AutomationEvent::TerminalOutput(_) => "terminal",
                    };
                    if subscribed_topics.iter().any(|value| value == topic) {
                        let payload = serde_json::to_string(&AutomationEnvelope{ id: None, payload: event })
                            .unwrap_or_else(|_| "{\"type\":\"error\",\"error\":\"encode\"}".to_string());
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
            log_msg = log_receiver.recv() => {
                if let Ok(msg) = log_msg {
                    if subscribed_topics.iter().any(|value| value == "logs") {
                        let payload = serde_json::to_string(&AutomationEnvelope{ 
                            id: None, 
                            payload: serde_json::json!({
                                "type": "log-entry",
                                "message": msg
                            }) 
                        }).unwrap_or_else(|_| "{\"type\":\"error\",\"error\":\"encode\"}".to_string());
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
            inbound = socket.recv() => {
                let Some(Ok(message)) = inbound else { break; };
                if window_start.elapsed() > Duration::from_secs(60) {
                    window_start = Instant::now();
                    message_count = 0;
                }
                message_count += 1;
                if message_count > state.max_messages_per_minute {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
                if let Message::Text(text) = message {
                    let response = handle_request(&state, &text, &mut subscribed_topics, user_agent.clone(), None).await;
                    if let Some(response) = response
                        && socket.send(Message::Text(response.into())).await.is_err()
                    {
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_request(
    state: &AutomationServerState,
    raw: &str,
    subscribed_topics: &mut Vec<String>,
    user_agent: Option<String>,
    client_ip: Option<String>,
) -> Option<String> {
    let parsed: Result<AutomationEnvelope<AutomationRequest>, _> = serde_json::from_str(raw);
    let Ok(envelope) = parsed else {
        return Some(
            serde_json::to_string(&AutomationResponse::<serde_json::Value> {
                id: None,
                ok: false,
                result: None,
                error: Some("invalid request".to_string()),
            })
            .unwrap(),
        );
    };
    let id = envelope.id.clone();
    let command_name = format!("{:?}", envelope.payload);
    let activity_id = id.clone().unwrap_or_else(|| {
        let count = state.handle.activity_counter.fetch_add(1, Ordering::SeqCst);
        format!("auto-{}", count)
    });

    tracing::info!(target: "brazen::automation", id, "handling automation request: {}", command_name);

    let audit_entry = AuditEntry {
        timestamp: Utc::now(),
        command: command_name.clone(),
        user_agent: user_agent.clone(),
        client_ip: client_ip.clone(),
        outcome: "pending".to_string(),
    };

    state.handle.record_activity(AutomationActivity {
        id: activity_id.clone(),
        command: command_name.clone(),
        status: AutomationActivityStatus::Running,
        timestamp: Local::now().format("%H:%M:%S").to_string(),
        output: None,
    });

    state.handle.request_repaint();

    let response = match envelope.payload {
        AutomationRequest::WindowList => {
            // Placeholder: Brazen core needs to expose windows in its state.
            let response = AutomationResponse {
                id,
                ok: true,
                result: Some(serde_json::json!({
                    "active_window": 0,
                    "window_count": 1
                })),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::LogSubscribe => {
            if !subscribed_topics.iter().any(|t| t == "logs") {
                subscribed_topics.push("logs".to_string());
            }
            Some(serde_json::to_string(&AutomationResponse::<serde_json::Value> {
                id,
                ok: true,
                result: Some(serde_json::json!({"status": "subscribed"})),
                error: None,
            }).unwrap())
        }
        AutomationRequest::TabList => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let snapshot = state.handle.snapshot.read().expect("snapshot");
            let response = AutomationResponse {
                id,
                ok: true,
                result: Some(&snapshot.tabs),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::Snapshot => {
            let snapshot = state.handle.snapshot();
            let response = AutomationResponse {
                id,
                ok: true,
                result: Some(snapshot),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::TabActivate { index, tab_id } => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let result = resolve_tab_index(&state.handle.snapshot, index, tab_id.as_deref());
            match result {
                Ok(index) => {
                    let _ = state
                        .handle
                        .command_tx
                        .send(AutomationCommand::ActivateTab { index });
                    Some(ok_response(id))
                }
                Err(error) => Some(error_response(id, &error)),
            }
        }
        AutomationRequest::TabNew { url } => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state
                .handle
                .command_tx
                .send(AutomationCommand::NewTab { url });
            Some(ok_response(id))
        }
        AutomationRequest::TabClose { index, tab_id } => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let result = resolve_tab_index(&state.handle.snapshot, index, tab_id.as_deref());
            match result {
                Ok(index) => {
                    let _ = state
                        .handle
                        .command_tx
                        .send(AutomationCommand::CloseTab { index });
                    Some(ok_response(id))
                }
                Err(error) => Some(error_response(id, &error)),
            }
        }
        AutomationRequest::TabNavigate { url } => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state
                .handle
                .command_tx
                .send(AutomationCommand::Navigate { url });
            Some(ok_response(id))
        }
        AutomationRequest::TabReload => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::Reload);
            Some(ok_response(id))
        }
        AutomationRequest::TabStop => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::Stop);
            Some(ok_response(id))
        }
        AutomationRequest::TabBack => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::GoBack);
            Some(ok_response(id))
        }
        AutomationRequest::TabForward => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::GoForward);
            Some(ok_response(id))
        }
        AutomationRequest::DomQuery { selector } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state
                .handle
                .command_tx
                .send(AutomationCommand::DomQuery { selector, response_tx: tx });
            match rx.await {
                Ok(Ok(result)) => {
                    let stable = match result {
                        serde_json::Value::Null => serde_json::Value::String(String::new()),
                        serde_json::Value::String(s) => serde_json::Value::String(s),
                        other => serde_json::Value::String(other.to_string()),
                    };
                    let response = AutomationResponse {
                        id,
                        ok: true,
                        result: Some(stable),
                        error: None,
                    };
                    Some(serde_json::to_string(&response).unwrap())
                }
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::Screenshot => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state
                .handle
                .command_tx
                .send(AutomationCommand::Screenshot { response_tx: tx });
            match rx.await {
                Ok(Ok(frame)) => {
                    let mut png_data = Vec::new();
                    let mut cursor = Cursor::new(&mut png_data);
                    
                    let result: Result<(), String> = match frame.pixel_format {
                        PixelFormat::Rgba8 => {
                            let img_opt = image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels);
                            if let Some(img) = img_opt {
                                match img.write_to(&mut cursor, ImageFormat::Png) {
                                    Ok(_) => Ok(()),
                                    Err(e) => Err(e.to_string()),
                                }
                            } else {
                                Err("Failed to create image from raw pixels".to_string())
                            }
                        }
                        PixelFormat::Bgra8 => {
                            let mut rgba_pixels = Vec::with_capacity(frame.pixels.len());
                            for chunk in frame.pixels.chunks_exact(4) {
                                rgba_pixels.push(chunk[2]); // R
                                rgba_pixels.push(chunk[1]); // G
                                rgba_pixels.push(chunk[0]); // B
                                rgba_pixels.push(chunk[3]); // A
                            }
                            let img_opt = image::RgbaImage::from_raw(frame.width, frame.height, rgba_pixels);
                            if let Some(img) = img_opt {
                                match img.write_to(&mut cursor, ImageFormat::Png) {
                                    Ok(_) => Ok(()),
                                    Err(e) => Err(e.to_string()),
                                }
                            } else {
                                Err("Failed to create image from raw pixels".to_string())
                            }
                        }
                    };

                    match result {
                        Ok(_) => {
                            let encoded = base64::engine::general_purpose::STANDARD.encode(&png_data);
                            let response = AutomationResponse {
                                id,
                                ok: true,
                                result: Some(encoded),
                                error: None,
                            };
                            Some(serde_json::to_string(&response).unwrap())
                        }
                        Err(error) => Some(error_response(id, &error)),
                    }
                }
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ScreenshotMeta => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state
                .handle
                .command_tx
                .send(AutomationCommand::Screenshot { response_tx: tx });
            match rx.await {
                Ok(Ok(frame)) => {
                    let mut png_data = Vec::new();
                    let mut cursor = Cursor::new(&mut png_data);

                    let result: Result<(), String> = match frame.pixel_format {
                        PixelFormat::Rgba8 => {
                            let img_opt = image::RgbaImage::from_raw(
                                frame.width,
                                frame.height,
                                frame.pixels,
                            );
                            if let Some(img) = img_opt {
                                img.write_to(&mut cursor, ImageFormat::Png)
                                    .map_err(|e| e.to_string())
                            } else {
                                Err("Failed to create image from raw pixels".to_string())
                            }
                        }
                        PixelFormat::Bgra8 => {
                            let mut rgba_pixels = Vec::with_capacity(frame.pixels.len());
                            for chunk in frame.pixels.chunks_exact(4) {
                                rgba_pixels.push(chunk[2]); // R
                                rgba_pixels.push(chunk[1]); // G
                                rgba_pixels.push(chunk[0]); // B
                                rgba_pixels.push(chunk[3]); // A
                            }
                            let img_opt = image::RgbaImage::from_raw(
                                frame.width,
                                frame.height,
                                rgba_pixels,
                            );
                            if let Some(img) = img_opt {
                                img.write_to(&mut cursor, ImageFormat::Png)
                                    .map_err(|e| e.to_string())
                            } else {
                                Err("Failed to create image from raw pixels".to_string())
                            }
                        }
                    };

                    match result {
                        Ok(_) => {
                            let encoded =
                                base64::engine::general_purpose::STANDARD.encode(&png_data);
                            let response = AutomationResponse {
                                id,
                                ok: true,
                                result: Some(serde_json::json!({
                                    "png_base64": encoded,
                                    "width": frame.width,
                                    "height": frame.height,
                                    "pixel_format": format!("{:?}", frame.pixel_format),
                                })),
                                error: None,
                            };
                            Some(serde_json::to_string(&response).unwrap())
                        }
                        Err(error) => Some(error_response(id, &error)),
                    }
                }
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::RenderedText => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::RenderedText { response_tx: tx });
            match rx.await {
                Ok(Ok(text)) => Some(serde_json::to_string(&AutomationResponse { id, ok: true, result: Some(text), error: None }).unwrap()),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ArticleText => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::ArticleText { response_tx: tx });
            match rx.await {
                Ok(Ok(text)) => Some(serde_json::to_string(&AutomationResponse { id, ok: true, result: Some(text), error: None }).unwrap()),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::CacheStats => {
            if let Err(error) = ensure_cache_api(state) {
                return Some(error_response(id, &error));
            }
            let store = AssetStore::load(
                state.handle.cache_config.clone(),
                &state.handle.runtime_paths,
                state.handle.profile_id.clone(),
            );
            let stats = store.stats();
            Some(
                serde_json::to_string(&AutomationResponse {
                    id,
                    ok: true,
                    result: Some(stats),
                    error: None,
                })
                .unwrap(),
            )
        }
        AutomationRequest::CacheQuery { query, limit } => {
            if let Err(error) = ensure_cache_api(state) {
                return Some(error_response(id, &error));
            }
            let store = AssetStore::load(
                state.handle.cache_config.clone(),
                &state.handle.runtime_paths,
                state.handle.profile_id.clone(),
            );
            let query = query.unwrap_or(AssetQuery {
                url: None,
                mime: None,
                hash: None,
                session_id: None,
                tab_id: None,
                status_code: None,
            });
            let mut assets = store.query(query);
            if let Some(limit) = limit {
                assets.truncate(limit.max(0));
            }
            Some(
                serde_json::to_string(&AutomationResponse {
                    id,
                    ok: true,
                    result: Some(assets),
                    error: None,
                })
                .unwrap(),
            )
        }
        AutomationRequest::CacheBody { asset_id } => {
            if let Err(error) = ensure_cache_api(state) {
                return Some(error_response(id, &error));
            }
            let body = load_cache_body(&state.handle, &asset_id);
            match body {
                Ok(body) => Some(
                    serde_json::to_string(&AutomationResponse {
                        id,
                        ok: true,
                        result: Some(body),
                        error: None,
                    })
                    .unwrap(),
                ),
                Err(error) => Some(error_response(id, &error)),
            }
        }
        AutomationRequest::TtsControl { action } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::TtsControl { action, response_tx: tx });
            match rx.await {
                Ok(Ok(_)) => Some(ok_response(id)),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::TtsEnqueue { text } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::TtsEnqueue { text, response_tx: tx });
            match rx.await {
                Ok(Ok(_)) => Some(ok_response(id)),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ReadingEnqueue { url, title, kind, article_text } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let kind = kind.unwrap_or_else(|| "link".to_string());
            let _ = state.handle.command_tx.send(AutomationCommand::ReadingEnqueue {
                url,
                title,
                kind,
                article_text,
                response_tx: tx,
            });
            match rx.await {
                Ok(Ok(_)) => Some(ok_response(id)),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ReadingSetProgress { url, progress } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::ReadingSetProgress {
                url,
                progress,
                response_tx: tx,
            });
            match rx.await {
                Ok(Ok(_)) => Some(ok_response(id)),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ReadingRemove { url } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state
                .handle
                .command_tx
                .send(AutomationCommand::ReadingRemove { url, response_tx: tx });
            match rx.await {
                Ok(Ok(_)) => Some(ok_response(id)),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ReadingClear => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state
                .handle
                .command_tx
                .send(AutomationCommand::ReadingClear { response_tx: tx });
            match rx.await {
                Ok(Ok(_)) => Some(ok_response(id)),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ReaderModeOpen { url } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state
                .handle
                .command_tx
                .send(AutomationCommand::ReaderModeOpen { url, response_tx: tx });
            match rx.await {
                Ok(Ok(_)) => Some(ok_response(id)),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ReaderModeClose => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state
                .handle
                .command_tx
                .send(AutomationCommand::ReaderModeClose { response_tx: tx });
            match rx.await {
                Ok(Ok(_)) => Some(ok_response(id)),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::MountAdd { name, local_path, read_only, allowed_domains } => {
            if let Err(error) = ensure_mount_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::AddMount {
                name,
                local_path: std::path::PathBuf::from(local_path),
                read_only: read_only.unwrap_or(true),
                allowed_domains: allowed_domains.unwrap_or_default(),
            });
            Some(ok_response(id))
        }
        AutomationRequest::MountRemove { name } => {
            if let Err(error) = ensure_mount_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::RemoveMount { name });
            Some(ok_response(id))
        }
        AutomationRequest::MountList => {
            if let Err(error) = ensure_mount_api(state) {
                return Some(error_response(id, &error));
            }
            let mounts = state.handle.mount_manager.list_mounts();
            let response = AutomationResponse {
                id,
                ok: true,
                result: Some(mounts),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::EvaluateJavascript { script } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state
                .handle
                .command_tx
                .send(AutomationCommand::EvaluateJavascript { script, response_tx: tx });
            match rx.await {
                Ok(Ok(result)) => {
                    let response = AutomationResponse {
                        id,
                        ok: true,
                        result: Some(result),
                        error: None,
                    };
                    Some(serde_json::to_string(&response).unwrap())
                }
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::TerminalExec { cmd, args, cwd } => {
            if let Err(error) = state.check_rate_limit("terminal-exec", 30) {
                return Some(error_response(id, &error));
            }
            if let Err(error) = state.check_permission(Capability::TerminalExec) {
                if error == "approval-required" {
                    let approval_id = Uuid::new_v4().to_string();
                    let now = Utc::now().to_rfc3339();
                    let pending = PendingApproval {
                        capability: Capability::TerminalExec,
                        request: AutomationRequest::TerminalExec {
                            cmd: cmd.clone(),
                            args: args.clone(),
                            cwd: cwd.clone(),
                        },
                        user_agent: user_agent.clone(),
                        client_ip: client_ip.clone(),
                    };
                    state
                        .pending_approvals
                        .write()
                        .expect("pending approvals lock")
                        .insert(approval_id.clone(), pending);
                    return Some(
                        serde_json::to_string(&AutomationResponse {
                            id,
                            ok: false,
                            result: Some(serde_json::json!({
                                "approval_id": approval_id,
                                "capability": Capability::TerminalExec.label(),
                                "created_at": now,
                                "summary": {
                                    "cmd": cmd,
                                    "args": args,
                                    "cwd": cwd,
                                }
                            })),
                            error: Some("approval-required".to_string()),
                        })
                        .unwrap(),
                    );
                }
                return Some(error_response(id, &error));
            }
            let request = crate::terminal::TerminalRequest { cmd, args, cwd };
            let response = crate::terminal::TerminalBroker::execute(&state.handle.terminal_config, request).await;
            Some(
                serde_json::to_string(&AutomationResponse {
                    id,
                    ok: response.success,
                    result: Some(response),
                    error: None,
                })
                .unwrap(),
            )
        }
        AutomationRequest::TerminalExecStream { cmd, args, cwd } => {
            if let Err(error) = state.check_rate_limit("terminal-exec-stream", 30) {
                return Some(error_response(id, &error));
            }
            if let Err(error) = state.check_permission(Capability::TerminalExec) {
                if error == "approval-required" {
                    let approval_id = Uuid::new_v4().to_string();
                    let now = Utc::now().to_rfc3339();
                    let pending = PendingApproval {
                        capability: Capability::TerminalExec,
                        request: AutomationRequest::TerminalExecStream {
                            cmd: cmd.clone(),
                            args: args.clone(),
                            cwd: cwd.clone(),
                        },
                        user_agent: user_agent.clone(),
                        client_ip: client_ip.clone(),
                    };
                    state
                        .pending_approvals
                        .write()
                        .expect("pending approvals lock")
                        .insert(approval_id.clone(), pending);
                    return Some(
                        serde_json::to_string(&AutomationResponse {
                            id,
                            ok: false,
                            result: Some(serde_json::json!({
                                "approval_id": approval_id,
                                "capability": Capability::TerminalExec.label(),
                                "created_at": now,
                                "summary": {
                                    "cmd": cmd,
                                    "args": args,
                                    "cwd": cwd,
                                }
                            })),
                            error: Some("approval-required".to_string()),
                        })
                        .unwrap(),
                    );
                }
                return Some(error_response(id, &error));
            }

            let started = std::time::Instant::now();
            let config = state.handle.terminal_config.clone();
            if !config.allowlist.is_empty()
                && !config.allowlist.iter().any(|allowed| allowed == &cmd)
            {
                return Some(error_response(id, "command not allowlisted"));
            }
            if args.len() > config.max_args {
                return Some(error_response(id, "too many args"));
            }

            let session_id = Uuid::new_v4().to_string();
            let session_id_for_task = session_id.clone();
            let mut command = tokio::process::Command::new(&cmd);
            command.args(&args);
            if let Some(cwd) = &cwd {
                command.current_dir(cwd);
            }
            command.stdout(std::process::Stdio::piped());
            command.stderr(std::process::Stdio::piped());

            let child = match command.spawn() {
                Ok(child) => child,
                Err(error) => return Some(error_response(id, &format!("Failed to spawn: {error}"))),
            };
            let child = Arc::new(tokio::sync::Mutex::new(child));
            state
                .terminal_sessions
                .write()
                .expect("terminal sessions lock")
                .insert(session_id.clone(), child.clone());

            let event_tx = state.handle.event_tx.clone();
            let terminal_sessions = state.terminal_sessions.clone();
            tokio::spawn(async move {
                let timeout = Duration::from_millis(config.timeout_ms);
                let deadline = tokio::time::sleep(timeout);
                tokio::pin!(deadline);

                let (stdout, stderr) = {
                    let mut lock = child.lock().await;
                    (lock.stdout.take(), lock.stderr.take())
                };

                let mut stdout_task = None;
                if let Some(stdout) = stdout {
                    let tx = event_tx.clone();
                    let sid = session_id_for_task.clone();
                    let max = config.max_stdout_bytes;
                    stdout_task = Some(tokio::spawn(async move {
                        let mut reader = BufReader::new(stdout);
                        let mut buf = String::new();
                        let mut sent = 0usize;
                        loop {
                            buf.clear();
                            let read = reader.read_line(&mut buf).await.unwrap_or(0);
                            if read == 0 {
                                break;
                            }
                            if sent >= max {
                                break;
                            }
                            let remaining = max.saturating_sub(sent);
                            let chunk = if buf.len() > remaining {
                                buf[..remaining].to_string()
                            } else {
                                buf.clone()
                            };
                            sent += chunk.len();
                            let _ = tx.send(AutomationEvent::TerminalOutput(
                                AutomationTerminalOutputEvent {
                                    session_id: sid.clone(),
                                    stream: "stdout".to_string(),
                                    chunk,
                                    done: false,
                                    exit_code: None,
                                    error: None,
                                },
                            ));
                        }
                    }));
                }

                let mut stderr_task = None;
                if let Some(stderr) = stderr {
                    let tx = event_tx.clone();
                    let sid = session_id_for_task.clone();
                    let max = config.max_stderr_bytes;
                    stderr_task = Some(tokio::spawn(async move {
                        let mut reader = BufReader::new(stderr);
                        let mut buf = String::new();
                        let mut sent = 0usize;
                        loop {
                            buf.clear();
                            let read = reader.read_line(&mut buf).await.unwrap_or(0);
                            if read == 0 {
                                break;
                            }
                            if sent >= max {
                                break;
                            }
                            let remaining = max.saturating_sub(sent);
                            let chunk = if buf.len() > remaining {
                                buf[..remaining].to_string()
                            } else {
                                buf.clone()
                            };
                            sent += chunk.len();
                            let _ = tx.send(AutomationEvent::TerminalOutput(
                                AutomationTerminalOutputEvent {
                                    session_id: sid.clone(),
                                    stream: "stderr".to_string(),
                                    chunk,
                                    done: false,
                                    exit_code: None,
                                    error: None,
                                },
                            ));
                        }
                    }));
                }

                let mut exit_code: Option<i32> = None;
                let mut error: Option<String> = None;
                tokio::select! {
                    _ = &mut deadline => {
                        error = Some("timeout".to_string());
                        let mut lock = child.lock().await;
                        let _ = lock.kill().await;
                    }
                    status = async {
                        let mut lock = child.lock().await;
                        lock.wait().await
                    } => {
                        match status {
                            Ok(status) => { exit_code = status.code(); }
                            Err(e) => { error = Some(format!("wait failed: {e}")); }
                        }
                    }
                }

                if let Some(task) = stdout_task { let _ = task.await; }
                if let Some(task) = stderr_task { let _ = task.await; }

                terminal_sessions
                    .write()
                    .expect("terminal sessions lock")
                    .remove(&session_id_for_task);

                let _ = event_tx.send(AutomationEvent::TerminalOutput(
                    AutomationTerminalOutputEvent {
                        session_id: session_id_for_task.clone(),
                        stream: "exit".to_string(),
                        chunk: format!("duration_ms={}", started.elapsed().as_millis()),
                        done: true,
                        exit_code,
                        error,
                    },
                ));
            });

            Some(
                serde_json::to_string(&AutomationResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::json!({
                        "session_id": session_id,
                        "cmd": cmd,
                        "args": args,
                        "cwd": cwd,
                    })),
                    error: None,
                })
                .unwrap(),
            )
        }
        AutomationRequest::TerminalCancel { session_id } => {
            if let Err(error) = state.check_rate_limit("terminal-cancel", 60) {
                return Some(error_response(id, &error));
            }
            if let Err(error) = state.check_permission(Capability::TerminalExec) {
                return Some(error_response(id, &error));
            }
            let session = state
                .terminal_sessions
                .read()
                .expect("terminal sessions lock")
                .get(&session_id)
                .cloned();
            let Some(session) = session else {
                return Some(error_response(id, "session not found"));
            };
            tokio::spawn(async move {
                let mut lock = session.lock().await;
                let _ = lock.kill().await;
            });
            Some(ok_response(id))
        }
        AutomationRequest::FsList { url } => {
            if let Err(error) = state.check_permission(Capability::FsRead) {
                if error == "approval-required" {
                    let approval_id = Uuid::new_v4().to_string();
                    let now = Utc::now().to_rfc3339();
                    let pending = PendingApproval {
                        capability: Capability::FsRead,
                        request: AutomationRequest::FsList { url: url.clone() },
                        user_agent: user_agent.clone(),
                        client_ip: client_ip.clone(),
                    };
                    state
                        .pending_approvals
                        .write()
                        .expect("pending approvals lock")
                        .insert(approval_id.clone(), pending);
                    return Some(
                        serde_json::to_string(&AutomationResponse {
                            id,
                            ok: false,
                            result: Some(serde_json::json!({
                                "approval_id": approval_id,
                                "capability": Capability::FsRead.label(),
                                "created_at": now,
                                "summary": { "url": url }
                            })),
                            error: Some("approval-required".to_string()),
                        })
                        .unwrap(),
                    );
                }
                return Some(error_response(id, &error));
            }

            let parsed = Url::parse(&url).map_err(|e| e.to_string());
            let Ok(parsed) = parsed else {
                return Some(error_response(id, "invalid url"));
            };
            let Some((data, _mime)) = state.handle.mount_manager.list_directory_json(&parsed) else {
                return Some(error_response(id, "not a directory or mount not found"));
            };
            let json: serde_json::Value = serde_json::from_slice(&data).unwrap_or(serde_json::Value::Null);
            Some(
                serde_json::to_string(&AutomationResponse {
                    id,
                    ok: true,
                    result: Some(json),
                    error: None,
                })
                .unwrap(),
            )
        }
        AutomationRequest::FsRead { url } => {
            if let Err(error) = state.check_permission(Capability::FsRead) {
                if error == "approval-required" {
                    let approval_id = Uuid::new_v4().to_string();
                    let now = Utc::now().to_rfc3339();
                    let pending = PendingApproval {
                        capability: Capability::FsRead,
                        request: AutomationRequest::FsRead { url: url.clone() },
                        user_agent: user_agent.clone(),
                        client_ip: client_ip.clone(),
                    };
                    state
                        .pending_approvals
                        .write()
                        .expect("pending approvals lock")
                        .insert(approval_id.clone(), pending);
                    return Some(
                        serde_json::to_string(&AutomationResponse {
                            id,
                            ok: false,
                            result: Some(serde_json::json!({
                                "approval_id": approval_id,
                                "capability": Capability::FsRead.label(),
                                "created_at": now,
                                "summary": { "url": url }
                            })),
                            error: Some("approval-required".to_string()),
                        })
                        .unwrap(),
                    );
                }
                return Some(error_response(id, &error));
            }

            let parsed = Url::parse(&url).map_err(|e| e.to_string());
            let Ok(parsed) = parsed else {
                return Some(error_response(id, "invalid url"));
            };
            let Some((_mount, path)) = state.handle.mount_manager.resolve_fs_target(&parsed) else {
                return Some(error_response(id, "mount not found"));
            };
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(_) => return Some(error_response(id, "read failed")),
            };
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Some(
                serde_json::to_string(&AutomationResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::json!({
                        "url": url,
                        "size_bytes": bytes.len(),
                        "body_base64": encoded
                    })),
                    error: None,
                })
                .unwrap(),
            )
        }
        AutomationRequest::FsWrite { url, body_base64 } => {
            if let Err(error) = state.check_rate_limit("fs-write", 60) {
                return Some(error_response(id, &error));
            }
            if let Err(error) = state.check_permission(Capability::FsWrite) {
                if error == "approval-required" {
                    let approval_id = Uuid::new_v4().to_string();
                    let now = Utc::now().to_rfc3339();
                    let pending = PendingApproval {
                        capability: Capability::FsWrite,
                        request: AutomationRequest::FsWrite {
                            url: url.clone(),
                            body_base64: body_base64.clone(),
                        },
                        user_agent: user_agent.clone(),
                        client_ip: client_ip.clone(),
                    };
                    state
                        .pending_approvals
                        .write()
                        .expect("pending approvals lock")
                        .insert(approval_id.clone(), pending);
                    return Some(
                        serde_json::to_string(&AutomationResponse {
                            id,
                            ok: false,
                            result: Some(serde_json::json!({
                                "approval_id": approval_id,
                                "capability": Capability::FsWrite.label(),
                                "created_at": now,
                                "summary": { "url": url, "size_bytes": body_base64.len() }
                            })),
                            error: Some("approval-required".to_string()),
                        })
                        .unwrap(),
                    );
                }
                return Some(error_response(id, &error));
            }

            let parsed = Url::parse(&url).map_err(|e| e.to_string());
            let Ok(parsed) = parsed else {
                return Some(error_response(id, "invalid url"));
            };
            let Some((mount, path)) = state.handle.mount_manager.resolve_fs_target(&parsed) else {
                return Some(error_response(id, "mount not found"));
            };
            if mount.read_only {
                return Some(error_response(id, "mount is read-only"));
            }
            let bytes = match base64::engine::general_purpose::STANDARD.decode(body_base64.as_bytes()) {
                Ok(b) => b,
                Err(_) => return Some(error_response(id, "invalid base64")),
            };
            if let Err(_) = std::fs::write(&path, bytes) {
                return Some(error_response(id, "write failed"));
            }
            Some(
                serde_json::to_string(&AutomationResponse::<serde_json::Value> {
                    id,
                    ok: true,
                    result: Some(serde_json::json!({"ok": true})),
                    error: None,
                })
                .unwrap(),
            )
        }
        AutomationRequest::ApprovalRespond { approval_id, decision } => {
            let decision_lower = decision.to_lowercase();
            let allow = matches!(decision_lower.as_str(), "allow" | "approve");
            let pending = state
                .pending_approvals
                .write()
                .expect("pending approvals lock")
                .remove(&approval_id);
            let Some(pending) = pending else {
                return Some(error_response(id, "unknown approval id"));
            };

            let _ = state.audit_logger.log(AuditEntry {
                timestamp: Utc::now(),
                command: format!("approval: {} {}", pending.capability.label(), decision_lower),
                user_agent: pending.user_agent.clone(),
                client_ip: pending.client_ip.clone(),
                outcome: if allow { "allowed".to_string() } else { "denied".to_string() },
            });

            if !allow {
                return Some(error_response(id, "denied"));
            }

            match pending.request {
                AutomationRequest::TerminalExec { cmd, args, cwd } => {
                    let request = crate::terminal::TerminalRequest { cmd, args, cwd };
                    let response =
                        crate::terminal::TerminalBroker::execute(&state.handle.terminal_config, request)
                            .await;
                    Some(
                        serde_json::to_string(&AutomationResponse {
                            id,
                            ok: response.success,
                            result: Some(response),
                            error: None,
                        })
                        .unwrap(),
                    )
                }
                AutomationRequest::FsList { url } => {
                    let parsed = Url::parse(&url).map_err(|e| e.to_string());
                    let Ok(parsed) = parsed else {
                        return Some(error_response(id, "invalid url"));
                    };
                    let Some((data, _mime)) = state.handle.mount_manager.list_directory_json(&parsed) else {
                        return Some(error_response(id, "not a directory or mount not found"));
                    };
                    let json: serde_json::Value =
                        serde_json::from_slice(&data).unwrap_or(serde_json::Value::Null);
                    Some(
                        serde_json::to_string(&AutomationResponse {
                            id,
                            ok: true,
                            result: Some(json),
                            error: None,
                        })
                        .unwrap(),
                    )
                }
                AutomationRequest::FsRead { url } => {
                    let parsed = Url::parse(&url).map_err(|e| e.to_string());
                    let Ok(parsed) = parsed else {
                        return Some(error_response(id, "invalid url"));
                    };
                    let Some((_mount, path)) = state.handle.mount_manager.resolve_fs_target(&parsed) else {
                        return Some(error_response(id, "mount not found"));
                    };
                    let bytes = match std::fs::read(&path) {
                        Ok(b) => b,
                        Err(_) => return Some(error_response(id, "read failed")),
                    };
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    Some(
                        serde_json::to_string(&AutomationResponse {
                            id,
                            ok: true,
                            result: Some(serde_json::json!({
                                "url": url,
                                "size_bytes": bytes.len(),
                                "body_base64": encoded
                            })),
                            error: None,
                        })
                        .unwrap(),
                    )
                }
                AutomationRequest::FsWrite { url, body_base64 } => {
                    let parsed = Url::parse(&url).map_err(|e| e.to_string());
                    let Ok(parsed) = parsed else {
                        return Some(error_response(id, "invalid url"));
                    };
                    let Some((mount, path)) = state.handle.mount_manager.resolve_fs_target(&parsed) else {
                        return Some(error_response(id, "mount not found"));
                    };
                    if mount.read_only {
                        return Some(error_response(id, "mount is read-only"));
                    }
                    let bytes = match base64::engine::general_purpose::STANDARD.decode(body_base64.as_bytes()) {
                        Ok(b) => b,
                        Err(_) => return Some(error_response(id, "invalid base64")),
                    };
                    if let Err(_) = std::fs::write(&path, bytes) {
                        return Some(error_response(id, "write failed"));
                    }
                    Some(
                        serde_json::to_string(&AutomationResponse::<serde_json::Value> {
                            id,
                            ok: true,
                            result: Some(serde_json::json!({"ok": true})),
                            error: None,
                        })
                        .unwrap(),
                    )
                }
                AutomationRequest::InteractDom { selector, event, value } => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    let _ = state.handle.command_tx.send(AutomationCommand::InteractDom { selector, event, value, response_tx: tx });
                    match rx.await {
                        Ok(Ok(_)) => Some(ok_response(id)),
                        Ok(Err(error)) => Some(error_response(id, &error)),
                        Err(_) => Some(error_response(id, "internal error")),
                    }
                }
                AutomationRequest::ScreenshotWindow => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    let _ = state.handle.command_tx.send(AutomationCommand::ScreenshotWindow { response_tx: tx });
                    match rx.await {
                        Ok(Ok(frame)) => {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&frame.pixels);
                            let json = serde_json::json!({
                                "width": frame.width,
                                "height": frame.height,
                                "png_base64": b64
                            });
                            Some(serde_json::to_string(&AutomationResponse { id, ok: true, result: Some(json), error: None }).unwrap())
                        },
                        Ok(Err(error)) => Some(error_response(id, &error)),
                        Err(_) => Some(error_response(id, "internal error")),
                    }
                }
                _ => Some(error_response(id, "unsupported pending request")),
            }
        }
        AutomationRequest::InteractDom { selector, event, value } => {
            if let Err(error) = state.check_permission(Capability::DomWrite) {
                if error == "approval-required" {
                    let approval_id = Uuid::new_v4().to_string();
                    let now = Utc::now().to_rfc3339();
                    let pending = PendingApproval {
                        capability: Capability::DomWrite,
                        request: AutomationRequest::InteractDom { selector: selector.clone(), event: event.clone(), value: value.clone() },
                        user_agent: user_agent.clone(),
                        client_ip: client_ip.clone(),
                    };
                    state.pending_approvals.write().expect("pending approvals lock").insert(approval_id.clone(), pending);
                    return Some(serde_json::to_string(&AutomationResponse {
                        id,
                        ok: false,
                        result: Some(serde_json::json!({
                            "approval_id": approval_id,
                            "capability": Capability::DomWrite.label(),
                            "created_at": now,
                            "summary": { "selector": selector, "event": event }
                        })),
                        error: Some("approval-required".to_string()),
                    }).unwrap());
                }
                return Some(error_response(id, &error));
            }
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::InteractDom { selector, event, value, response_tx: tx });
            match rx.await {
                Ok(Ok(_)) => Some(ok_response(id)),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ScreenshotWindow => {
            if let Err(error) = state.check_permission(Capability::ScreenshotWindow) {
                if error == "approval-required" {
                    let approval_id = Uuid::new_v4().to_string();
                    let now = Utc::now().to_rfc3339();
                    let pending = PendingApproval {
                        capability: Capability::ScreenshotWindow,
                        request: AutomationRequest::ScreenshotWindow,
                        user_agent: user_agent.clone(),
                        client_ip: client_ip.clone(),
                    };
                    state.pending_approvals.write().expect("pending approvals lock").insert(approval_id.clone(), pending);
                    return Some(serde_json::to_string(&AutomationResponse {
                        id,
                        ok: false,
                        result: Some(serde_json::json!({
                            "approval_id": approval_id,
                            "capability": Capability::ScreenshotWindow.label(),
                            "created_at": now,
                            "summary": {}
                        })),
                        error: Some("approval-required".to_string()),
                    }).unwrap());
                }
                return Some(error_response(id, &error));
            }
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::ScreenshotWindow { response_tx: tx });
            match rx.await {
                Ok(Ok(frame)) => {
                    let mut png_data = Vec::new();
                    let mut cursor = Cursor::new(&mut png_data);

                    let result: Result<(), String> = match frame.pixel_format {
                        PixelFormat::Rgba8 => {
                            let img_opt =
                                image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels);
                            if let Some(img) = img_opt {
                                img.write_to(&mut cursor, ImageFormat::Png)
                                    .map_err(|e| e.to_string())
                            } else {
                                Err("Failed to create image from raw pixels".to_string())
                            }
                        }
                        PixelFormat::Bgra8 => {
                            let mut rgba_pixels = Vec::with_capacity(frame.pixels.len());
                            for chunk in frame.pixels.chunks_exact(4) {
                                rgba_pixels.push(chunk[2]); // R
                                rgba_pixels.push(chunk[1]); // G
                                rgba_pixels.push(chunk[0]); // B
                                rgba_pixels.push(chunk[3]); // A
                            }
                            let img_opt =
                                image::RgbaImage::from_raw(frame.width, frame.height, rgba_pixels);
                            if let Some(img) = img_opt {
                                img.write_to(&mut cursor, ImageFormat::Png)
                                    .map_err(|e| e.to_string())
                            } else {
                                Err("Failed to create image from raw pixels".to_string())
                            }
                        }
                    };

                    match result {
                        Ok(()) => {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_data);
                            let json = serde_json::json!({
                                "width": frame.width,
                                "height": frame.height,
                                "png_base64": b64,
                            });
                            Some(serde_json::to_string(&AutomationResponse { id, ok: true, result: Some(json), error: None }).unwrap())
                        }
                        Err(error) => Some(error_response(id, &error)),
                    }
                },
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ProfileCreate { profile_id } => {
            let profile_id = profile_id.trim().to_string();
            if profile_id.is_empty() {
                return Some(error_response(id, "profile id required"));
            }
            if profile_id.contains("..") || profile_id.contains('/') || profile_id.contains('\\') {
                return Some(error_response(id, "invalid profile id"));
            }
            let dir = state.handle.runtime_paths.profiles_dir.join(&profile_id);
            if let Err(error) = std::fs::create_dir_all(&dir) {
                return Some(error_response(id, &format!("create profile dir failed: {error}")));
            }
            // Ensure state db exists.
            let _ = crate::profile_db::ProfileDb::open(dir.join("state.sqlite"));
            let response = AutomationResponse {
                id,
                ok: true,
                result: Some(serde_json::json!({
                    "profile_id": profile_id,
                    "path": dir.display().to_string()
                })),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::ProfileSwitch { profile_id } => {
            let profile_id = profile_id.trim().to_string();
            if profile_id.is_empty() {
                return Some(error_response(id, "profile id required"));
            }
            if profile_id.contains("..") || profile_id.contains('/') || profile_id.contains('\\') {
                return Some(error_response(id, "invalid profile id"));
            }
            let dir = state.handle.runtime_paths.profiles_dir.join(&profile_id);
            if !dir.exists() {
                return Some(error_response(id, "profile does not exist"));
            }

            // Persist to config TOML by editing only profiles.active_profile.
            let config_path = state.handle.runtime_paths.config_path.clone();
            let text = std::fs::read_to_string(&config_path)
                .map_err(|e| format!("read config failed: {e}"))
                .unwrap_or_default();
            let mut value: toml::Value = text.parse().unwrap_or(toml::Value::Table(toml::map::Map::new()));
            let table = value.as_table_mut().unwrap();
            let profiles = table
                .entry("profiles")
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
            let profiles_table = profiles.as_table_mut().unwrap();
            profiles_table.insert(
                "active_profile".to_string(),
                toml::Value::String(profile_id.clone()),
            );
            let out = toml::to_string_pretty(&value).unwrap_or_else(|_| text.clone());
            if let Err(error) = std::fs::write(&config_path, out) {
                return Some(error_response(id, &format!("write config failed: {error}")));
            }

            // Request restart so the running instance re-bootstrap with the new profile.
            let _ = state.handle.request_shutdown();

            let response = AutomationResponse {
                id,
                ok: true,
                result: Some(serde_json::json!({
                    "active_profile": profile_id,
                    "note": "profile switched; instance shutting down to restart with new profile"
                })),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::ProfileExport {
            profile_id,
            output_path,
            include_cache_blobs,
        } => {
            let profile_id = profile_id.trim().to_string();
            if profile_id.is_empty() {
                return Some(error_response(id, "profile id required"));
            }
            if profile_id.contains("..") || profile_id.contains('/') || profile_id.contains('\\') {
                return Some(error_response(id, "invalid profile id"));
            }
            let include_cache_blobs = include_cache_blobs.unwrap_or(false);
            let out_path = std::path::PathBuf::from(output_path);
            if let Some(parent) = out_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let file = match std::fs::File::create(&out_path) {
                Ok(f) => f,
                Err(e) => return Some(error_response(id, &format!("create bundle failed: {e}"))),
            };
            let encoder = GzEncoder::new(file, Compression::default());
            let mut builder = Builder::new(encoder);

            let profile_dir = state.handle.runtime_paths.profiles_dir.join(&profile_id);
            let cache_dir = state.handle.runtime_paths.cache_dir.join(&profile_id);
            let mut items = Vec::new();
            items.push(("profile", profile_dir.clone()));
            items.push(("cache", cache_dir.clone()));

            // Always include: profile/* and cache/{index,metadata,headers,pinned}. Blobs optional.
            if let Err(error) = add_dir_to_tar(&mut builder, &profile_dir, "profile") {
                return Some(error_response(id, &error));
            }
            if cache_dir.exists() {
                let wanted = ["index.jsonl", "metadata.jsonl", "headers.jsonl", "pinned.json"];
                for name in wanted {
                    let p = cache_dir.join(name);
                    if p.exists() {
                        let rel = std::path::Path::new("cache").join(name);
                        let mut f = match std::fs::File::open(&p) {
                            Ok(f) => f,
                            Err(e) => return Some(error_response(id, &format!("open cache file failed: {e}"))),
                        };
                        if let Err(e) = builder.append_file(rel, &mut f) {
                            return Some(error_response(id, &format!("append cache file failed: {e}")));
                        }
                    }
                }
                if include_cache_blobs {
                    let blobs = cache_dir.join("blobs");
                    if let Err(error) = add_dir_to_tar(&mut builder, &blobs, "cache/blobs") {
                        return Some(error_response(id, &error));
                    }
                }
            }
            if let Err(e) = builder.finish() {
                return Some(error_response(id, &format!("finish bundle failed: {e}")));
            }

            let response = AutomationResponse {
                id,
                ok: true,
                result: Some(serde_json::json!({
                    "profile_id": profile_id,
                    "output_path": out_path.display().to_string(),
                    "include_cache_blobs": include_cache_blobs,
                    "included": items.iter().map(|(k,p)| serde_json::json!({"kind":k,"path":p.display().to_string()})).collect::<Vec<_>>(),
                })),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::ProfileImport {
            profile_id,
            input_path,
            overwrite,
        } => {
            let profile_id = profile_id.trim().to_string();
            if profile_id.is_empty() {
                return Some(error_response(id, "profile id required"));
            }
            if profile_id.contains("..") || profile_id.contains('/') || profile_id.contains('\\') {
                return Some(error_response(id, "invalid profile id"));
            }
            let input_path = std::path::PathBuf::from(input_path);
            let overwrite = overwrite.unwrap_or(false);

            let profile_dir = state.handle.runtime_paths.profiles_dir.join(&profile_id);
            let cache_dir = state.handle.runtime_paths.cache_dir.join(&profile_id);
            let _ = std::fs::create_dir_all(&profile_dir);
            let _ = std::fs::create_dir_all(&cache_dir);

            if let Err(error) = unpack_tar_to_dir(&input_path, &profile_dir, "profile", overwrite) {
                return Some(error_response(id, &error));
            }
            if let Err(error) = unpack_tar_to_dir(&input_path, &cache_dir, "cache", overwrite) {
                return Some(error_response(id, &error));
            }

            // Ensure schema exists if state.sqlite missing.
            let _ = crate::profile_db::ProfileDb::open(profile_dir.join("state.sqlite"));

            let response = AutomationResponse {
                id,
                ok: true,
                result: Some(serde_json::json!({
                    "profile_id": profile_id,
                    "input_path": input_path.display().to_string(),
                    "overwrite": overwrite,
                })),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::Shutdown => {
            let result = state.handle.request_shutdown();
            match result {
                Ok(()) => Some(ok_response(id)),
                Err(error) => Some(error_response(id, &error)),
            }
        }
        AutomationRequest::Subscribe { topics } => {
            if topics.len() > state.max_subscriptions {
                return Some(error_response(id, "subscription limit exceeded"));
            }
            subscribed_topics.clear();
            subscribed_topics.extend(
                topics
                    .into_iter()
                    .map(|topic| topic.to_lowercase())
                    .collect::<Vec<_>>(),
            );
            Some(ok_response(id))
        }
    };

    if let Some(ref res_json) = response {
        if let Ok(res_parsed) = serde_json::from_str::<AutomationResponse<serde_json::Value>>(res_json) {
            let mut snapshot = state.handle.snapshot.write().expect("automation snapshot lock");
            if let Some(activity) = snapshot.activities.iter_mut().find(|a| a.id == activity_id) {
                activity.status = if res_parsed.ok { AutomationActivityStatus::Success } else { AutomationActivityStatus::Failed };
                activity.output = res_parsed.error.clone();
            }

            let mut final_audit = audit_entry.clone();
            final_audit.outcome = if res_parsed.ok { "success".to_string() } else { res_parsed.error.clone().unwrap_or_else(|| "failed".to_string()) };
            let _ = state.audit_logger.log(final_audit);
        }
    }

    response
}

fn ok_response(id: Option<String>) -> String {
    serde_json::to_string(&AutomationResponse::<serde_json::Value> {
        id,
        ok: true,
        result: None,
        error: None,
    })
    .unwrap()
}

fn error_response(id: Option<String>, message: &str) -> String {
    serde_json::to_string(&AutomationResponse::<serde_json::Value> {
        id,
        ok: false,
        result: None,
        error: Some(message.to_string()),
    })
    .unwrap()
}

fn ensure_tab_api(state: &AutomationServerState) -> Result<(), String> {
    if !state.handle.expose_tab_api {
        return Err("tab api disabled".to_string());
    }
    state.check_permission(Capability::TabInspect)
}

#[allow(dead_code)]
fn ensure_cache_api(state: &AutomationServerState) -> Result<(), String> {
    if !state.handle.expose_cache_api {
        return Err("cache api disabled".to_string());
    }
    state.check_permission(Capability::CacheRead)
}

fn build_tab_list(session: &SessionSnapshot) -> Vec<AutomationTab> {
    let mut tabs = Vec::new();
    if let Some(window) = session.windows.get(session.active_window) {
        for (index, tab) in window.tabs.iter().enumerate() {
            tabs.push(AutomationTab {
                tab_id: tab.id.0.to_string(),
                index,
                title: tab.title.clone(),
                url: tab.url.clone(),
                zoom: tab.zoom_level,
                pinned: tab.pinned,
                muted: tab.muted,
            });
        }
    }
    tabs
}

fn asset_summary_from_metadata(entry: &AssetMetadata) -> AutomationAssetSummary {
    AutomationAssetSummary {
        asset_id: entry.asset_id.clone(),
        url: entry.url.clone(),
        status_code: entry.status_code,
        mime: entry.mime.clone(),
        size_bytes: entry.size_bytes,
        hash: entry.hash.clone(),
        created_at: entry.created_at.clone(),
        session_id: entry.session_id.clone(),
        tab_id: entry.tab_id.clone(),
    }
}

fn resolve_tab_index(
    snapshot: &RwLock<AutomationSnapshot>,
    index: Option<usize>,
    tab_id: Option<&str>,
) -> Result<usize, String> {
    let snapshot = snapshot.read().map_err(|_| "snapshot lock".to_string())?;
    if let Some(index) = index {
        if index < snapshot.tabs.len() {
            return Ok(index);
        } else {
            return Err("tab index out of range".to_string());
        }
    }
    if let Some(tab_id) = tab_id
        && let Some(tab) = snapshot.tabs.iter().find(|tab| tab.tab_id == tab_id)
    {
        return Ok(tab.index);
    }
    Err("tab not found".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AutomationCacheBody {
    pub asset_id: String,
    pub mime: String,
    pub size_bytes: u64,
    pub body_base64: String,
}

fn load_cache_body(
    handle: &AutomationHandle,
    asset_id: &str,
) -> Result<AutomationCacheBody, String> {
    let store = AssetStore::load(
        handle.cache_config.clone(),
        &handle.runtime_paths,
        handle.profile_id.clone(),
    );
    let entry = store
        .entries()
        .iter()
        .find(|entry| entry.asset_id == asset_id)
        .ok_or_else(|| "asset not found".to_string())?;
    let body_key = entry
        .body_key
        .clone()
        .or_else(|| entry.hash.clone())
        .ok_or_else(|| "asset has no body".to_string())?;
    let path = store.blob_path(&body_key);
    let bytes =
        std::fs::read(&path).map_err(|error| format!("failed to read asset body: {error}"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(AutomationCacheBody {
        asset_id: entry.asset_id.clone(),
        mime: entry.mime.clone(),
        size_bytes: entry.size_bytes,
        body_base64: encoded,
    })
}

pub fn drain_automation_commands(
    receiver: &mut mpsc::UnboundedReceiver<AutomationCommand>,
    shell_state: &mut ShellState,
    engine: &mut dyn crate::engine::BrowserEngine,
    cache: &mut crate::cache::AssetStore,
) {
    while let Ok(command) = receiver.try_recv() {
        match command {
            AutomationCommand::ActivateTab { index } => {
                let url = {
                    let mut session = shell_state.session.write().unwrap();
                    session.set_active_tab(index);
                    session.active_tab().map(|tab| tab.url.clone())
                };
                if let Some(url) = url {
                    shell_state.address_bar_input = url.clone();
                    shell_state.record_event(format!("automation activate tab: {}", url));
                }
            }
            AutomationCommand::CloseTab { index } => {
                let mut closed = false;
                {
                    let mut session = shell_state.session.write().unwrap();
                    let active_window = session.active_window;
                    if active_window < session.windows.len() {
                        let window = &mut session.windows[active_window];
                        if index < window.tabs.len() {
                            window.tabs.remove(index);
                            if window.active_tab >= window.tabs.len() {
                                window.active_tab = window.tabs.len().saturating_sub(1);
                            }
                            closed = true;
                        }
                    }
                }
                if closed {
                    shell_state.record_event("automation close tab");
                }
            }
            AutomationCommand::NewTab { url } => {
                let target = url.unwrap_or_else(|| "about:blank".to_string());
                shell_state.session.write().unwrap().open_new_tab(&target, "New Tab");
                shell_state.address_bar_input = target.clone();
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::NavigateTo(target),
                );
            }
            AutomationCommand::Navigate { url } => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::NavigateTo(url),
                );
            }
            AutomationCommand::Reload => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::ReloadActiveTab,
                );
            }
            AutomationCommand::AddMount { name, local_path, read_only, allowed_domains } => {
                shell_state.mount_manager.add_mount(crate::mounts::Mount {
                    name,
                    mount_type: crate::mounts::MountType::FileSystem(local_path),
                    read_only,
                    allowed_domains,
                });
                shell_state.record_event("automation add mount");
            }
            AutomationCommand::RemoveMount { name } => {
                shell_state.mount_manager.remove_mount(&name);
                shell_state.record_event("automation remove mount");
            }
            AutomationCommand::Stop => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::StopLoading,
                );
            }
            AutomationCommand::GoBack => {
                let _ =
                    commands::dispatch_command(shell_state, engine, commands::AppCommand::GoBack);
            }
            AutomationCommand::GoForward => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::GoForward,
                );
            }
            AutomationCommand::DomQuery {
                selector,
                response_tx,
            } => {
                engine.evaluate_javascript(
                    format!(
                        "document.querySelector('{}') ? document.querySelector('{}').outerHTML : null",
                        selector, selector
                    ),
                    Box::new(|result| {
                        let _ = response_tx.send(result);
                    }),
                );
            }
            AutomationCommand::Screenshot { response_tx } => {
                let _ = response_tx.send(engine.take_screenshot());
            }
            AutomationCommand::EvaluateJavascript { script, response_tx } => {
                engine.evaluate_javascript(
                    script,
                    Box::new(|result| {
                        let _ = response_tx.send(result);
                    }),
                );
            }
            AutomationCommand::RenderedText { response_tx } => {
                engine.evaluate_javascript(
                    "document.body.innerText".to_string(),
                    Box::new(|result| {
                        let _ = response_tx.send(result.map(|v| {
                            if let serde_json::Value::String(s) = v {
                                s
                            } else {
                                v.to_string()
                            }
                        }));
                    }),
                );
            }
            AutomationCommand::ArticleText { response_tx } => {
                engine.evaluate_javascript(
                    "(document.querySelector('article') || document.querySelector('main') || document.body).innerText".to_string(),
                    Box::new(|result| {
                        let _ = response_tx.send(result.map(|v| {
                            if let serde_json::Value::String(s) = v {
                                s
                            } else {
                                v.to_string()
                            }
                        }));
                    }),
                );
            }
            AutomationCommand::CacheStats { response_tx } => {
                let stats = cache.stats();
                let _ = response_tx.send(Ok(serde_json::to_value(stats).unwrap()));
            }
            AutomationCommand::CacheQuery { query, limit, response_tx } => {
                let entries = if let Some(q) = query {
                    cache.query(q)
                } else {
                    cache.entries().to_vec()
                };
                let limit_val = limit.unwrap_or(entries.len());
                let result = entries.into_iter().take(limit_val).map(|e| AutomationAssetSummary {
                    asset_id: e.asset_id,
                    url: e.url,
                    status_code: e.status_code,
                    mime: e.mime,
                    size_bytes: e.size_bytes,
                    hash: e.hash,
                    created_at: e.created_at,
                    session_id: e.session_id,
                    tab_id: e.tab_id,
                }).collect();
                let _ = response_tx.send(Ok(result));
            }
            AutomationCommand::CacheBody { asset_id, response_tx } => {
                 if let Some(entry) = cache.find_by_id_or_hash(&asset_id) {
                     if let Some(body_key) = &entry.body_key {
                         let path = cache.blob_path(body_key);
                         match std::fs::read(path) {
                             Ok(bytes) => {
                                 use base64::Engine;
                                 let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                 let _ = response_tx.send(Ok(encoded));
                             }
                             Err(e) => {
                                 let _ = response_tx.send(Err(e.to_string()));
                             }
                         }
                     } else {
                         let _ = response_tx.send(Err("no body captured for this asset".to_string()));
                     }
                 } else {
                     let _ = response_tx.send(Err("asset not found".to_string()));
                 }
            }
            AutomationCommand::TtsControl { action, response_tx } => {
                let action_lc = action.to_lowercase();
                match action_lc.as_str() {
                    "play" => {
                        shell_state.tts_playing = true;
                        shell_state.record_event("tts: play");
                    }
                    "pause" => {
                        shell_state.tts_playing = false;
                        shell_state.record_event("tts: pause");
                    }
                    "stop" => {
                        shell_state.tts_playing = false;
                        shell_state.tts_queue.clear();
                        shell_state.record_event("tts: stop");
                    }
                    "clear" => {
                        shell_state.tts_queue.clear();
                        shell_state.record_event("tts: clear");
                    }
                    _ => {
                        shell_state.record_event(format!("tts: unknown control action {action}"));
                    }
                }
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::TtsEnqueue { text, response_tx } => {
                shell_state.tts_queue.push_back(text);
                shell_state.record_event("tts: enqueue");
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::ReadingEnqueue { url, title, kind, article_text, response_tx } => {
                // Ensure uniqueness by URL (latest wins).
                if let Some(pos) = shell_state.reading_queue.iter().position(|item| item.url == url) {
                    let _ = shell_state.reading_queue.remove(pos);
                }
                shell_state.reading_queue.push_back(crate::app::ReadingQueueItem {
                    url,
                    title,
                    kind,
                    saved_at: Utc::now().to_rfc3339(),
                    progress: 0.0,
                    article_text,
                });
                shell_state.record_event("reading: enqueue");
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::ReadingSetProgress { url, progress, response_tx } => {
                let progress = progress.clamp(0.0, 1.0);
                if let Some(item) = shell_state.reading_queue.iter_mut().find(|item| item.url == url) {
                    item.progress = progress;
                    shell_state.record_event("reading: progress");
                    let _ = response_tx.send(Ok(()));
                } else {
                    let _ = response_tx.send(Err("not found".to_string()));
                }
            }
            AutomationCommand::ReadingRemove { url, response_tx } => {
                if let Some(pos) = shell_state.reading_queue.iter().position(|item| item.url == url) {
                    let _ = shell_state.reading_queue.remove(pos);
                    shell_state.record_event("reading: remove");
                    let _ = response_tx.send(Ok(()));
                } else {
                    let _ = response_tx.send(Err("not found".to_string()));
                }
            }
            AutomationCommand::ReadingClear { response_tx } => {
                shell_state.reading_queue.clear();
                shell_state.record_event("reading: clear");
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::ReaderModeOpen { url, response_tx } => {
                if let Some(item) = shell_state.reading_queue.iter().find(|item| item.url == url) {
                    shell_state.reader_mode_open = true;
                    shell_state.reader_mode_source_url = Some(item.url.clone());
                    shell_state.reader_mode_text = item
                        .article_text
                        .clone()
                        .unwrap_or_else(|| "No article text available.".to_string());
                    shell_state.record_event("reader: open");
                    let _ = response_tx.send(Ok(()));
                } else {
                    let _ = response_tx.send(Err("not found".to_string()));
                }
            }
            AutomationCommand::ReaderModeClose { response_tx } => {
                shell_state.reader_mode_open = false;
                shell_state.reader_mode_source_url = None;
                shell_state.reader_mode_text.clear();
                shell_state.record_event("reader: close");
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::InteractDom { selector, event, value, response_tx } => {
                engine.interact_dom(selector, event, value, Box::new(|res| {
                    let _ = response_tx.send(res);
                }));
            }
            AutomationCommand::ScreenshotWindow { response_tx } => {
                *shell_state.pending_window_screenshot.lock().unwrap() = Some(response_tx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform_paths::RuntimePaths;
    use tempfile::tempdir;
    use tokio::time::{timeout, Duration};

    #[test]
    fn automation_request_parses_tab_list() {
        let input = r#"{"id":"1","type":"tab-list"}"#;
        let parsed: AutomationEnvelope<AutomationRequest> = serde_json::from_str(input).unwrap();
        assert!(matches!(parsed.payload, AutomationRequest::TabList));
    }

    #[tokio::test]
    async fn terminal_exec_requires_approval_when_ask() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            permissions: PermissionPolicy {
                capabilities: {
                    let mut map = PermissionPolicy::default().capabilities;
                    map.insert(Capability::TerminalExec, PermissionDecision::Ask);
                    map
                },
                ..PermissionPolicy::default()
            },
            terminal: crate::config::TerminalConfig {
                allowlist: vec!["echo".to_string()],
                ..crate::config::TerminalConfig::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let raw = r#"{"id":"1","type":"terminal-exec","cmd":"echo","args":["hi"],"cwd":null}"#;
        let response = handle_request(&state, raw, &mut Vec::new(), None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(!parsed.ok);
        assert_eq!(parsed.error.as_deref(), Some("approval-required"));
        let approval_id = parsed
            .result
            .as_ref()
            .and_then(|v| v.get("approval_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(!approval_id.is_empty());

        let approve = serde_json::json!({
            "id": "2",
            "type": "approval-respond",
            "approval_id": approval_id,
            "decision": "allow"
        })
        .to_string();
        let response = handle_request(&state, &approve, &mut Vec::new(), None, None)
            .await
            .expect("approve response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);
    }

    #[tokio::test]
    async fn approval_required_contains_action_summary() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            permissions: PermissionPolicy {
                capabilities: {
                    let mut map = PermissionPolicy::default().capabilities;
                    map.insert(Capability::TerminalExec, PermissionDecision::Ask);
                    map
                },
                ..PermissionPolicy::default()
            },
            terminal: crate::config::TerminalConfig {
                allowlist: vec!["echo".to_string()],
                ..crate::config::TerminalConfig::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let raw = serde_json::json!({
            "id":"1",
            "type":"terminal-exec",
            "cmd":"echo",
            "args":["hi"],
            "cwd": null
        })
        .to_string();
        let response = handle_request(&state, &raw, &mut Vec::new(), Some("ua".to_string()), None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed.error.as_deref(), Some("approval-required"));
        let summary = parsed
            .result
            .as_ref()
            .and_then(|v| v.get("summary"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        assert_eq!(summary["cmd"].as_str().unwrap_or(""), "echo");
    }

    #[test]
    fn audit_logger_writes_jsonl_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let logger = AuditLogger::new(path.clone());
        logger
            .log(AuditEntry {
                timestamp: Utc::now(),
                command: "test".to_string(),
                user_agent: Some("ua".to_string()),
                client_ip: None,
                outcome: "ok".to_string(),
            })
            .unwrap();
        let data = std::fs::read_to_string(&path).unwrap();
        let line = data.lines().next().unwrap_or("");
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(parsed["command"].as_str().unwrap_or(""), "test");
    }

    #[tokio::test]
    async fn fs_write_requires_approval_when_ask() {
        let dir = tempdir().unwrap();
        let mount_dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            permissions: PermissionPolicy {
                capabilities: {
                    let mut map = PermissionPolicy::default().capabilities;
                    map.insert(Capability::FsWrite, PermissionDecision::Ask);
                    map.insert(Capability::FsRead, PermissionDecision::Allow);
                    map
                },
                ..PermissionPolicy::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        mount_manager.add_mount(crate::mounts::Mount {
            name: "m".to_string(),
            mount_type: crate::mounts::MountType::FileSystem(mount_dir.path().to_path_buf()),
            read_only: false,
            allowed_domains: Vec::new(),
        });
        let runtime = start_automation_runtime(&config, &paths, mount_manager.clone()).expect("runtime");
        // Ensure the runtime handle uses the same mount manager.
        runtime.handle.mount_manager.add_mount(crate::mounts::Mount {
            name: "m".to_string(),
            mount_type: crate::mounts::MountType::FileSystem(mount_dir.path().to_path_buf()),
            read_only: false,
            allowed_domains: Vec::new(),
        });

        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let target_url = "brazen://fs/m/test.txt";
        let body = base64::engine::general_purpose::STANDARD.encode(b"hello");
        let raw = serde_json::json!({
            "id": "1",
            "type": "fs-write",
            "url": target_url,
            "body_base64": body,
        })
        .to_string();
        let response = handle_request(&state, &raw, &mut Vec::new(), None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(!parsed.ok);
        assert_eq!(parsed.error.as_deref(), Some("approval-required"));
        let approval_id = parsed
            .result
            .as_ref()
            .and_then(|v| v.get("approval_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(!approval_id.is_empty());

        let approve = serde_json::json!({
            "id": "2",
            "type": "approval-respond",
            "approval_id": approval_id,
            "decision": "allow"
        })
        .to_string();
        let response = handle_request(&state, &approve, &mut Vec::new(), None, None)
            .await
            .expect("approve response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok, "expected ok after approval: {parsed:?}");
        assert_eq!(std::fs::read(mount_dir.path().join("test.txt")).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn rate_limit_blocks_excess_terminal_exec() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            permissions: PermissionPolicy {
                capabilities: {
                    let mut map = PermissionPolicy::default().capabilities;
                    map.insert(Capability::TerminalExec, PermissionDecision::Allow);
                    map
                },
                ..PermissionPolicy::default()
            },
            terminal: crate::config::TerminalConfig {
                allowlist: vec!["echo".to_string()],
                ..crate::config::TerminalConfig::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let raw = serde_json::json!({
            "id":"x",
            "type":"terminal-exec",
            "cmd":"echo",
            "args":["hi"],
            "cwd": null
        })
        .to_string();

        // Allow 30/min; the 31st should fail.
        for i in 0..30 {
            let response = handle_request(&state, &raw, &mut Vec::new(), None, None)
                .await
                .expect("response");
            let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
            assert!(parsed.ok, "expected ok on iteration {i}: {parsed:?}");
        }
        let response = handle_request(&state, &raw, &mut Vec::new(), None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(!parsed.ok);
        assert_eq!(parsed.error.as_deref(), Some("rate-limit"));
    }

    #[tokio::test]
    async fn log_subscribe_is_ok() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let raw = r#"{"id":"1","type":"log-subscribe"}"#;
        let response = handle_request(&state, raw, &mut Vec::new(), None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);
    }

    #[tokio::test]
    async fn tab_list_and_tab_new_are_ok_when_enabled() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                expose_tab_api: true,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            permissions: PermissionPolicy {
                capabilities: {
                    let mut map = PermissionPolicy::default().capabilities;
                    map.insert(Capability::TabInspect, PermissionDecision::Allow);
                    map
                },
                ..PermissionPolicy::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let response = handle_request(&state, r#"{"id":"1","type":"tab-list"}"#, &mut Vec::new(), None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);

        let response = handle_request(&state, r#"{"id":"2","type":"tab-new","url":"about:blank"}"#, &mut Vec::new(), None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);
    }

    #[tokio::test]
    async fn mount_list_and_add_are_ok_when_enabled() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            permissions: PermissionPolicy {
                capabilities: {
                    let mut map = PermissionPolicy::default().capabilities;
                    map.insert(Capability::VirtualResourceMount, PermissionDecision::Ask);
                    map
                },
                ..PermissionPolicy::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let response = handle_request(&state, r#"{"id":"1","type":"mount-list"}"#, &mut Vec::new(), None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);

        let response = handle_request(
            &state,
            &serde_json::json!({
                "id": "2",
                "type": "mount-add",
                "name": "m",
                "local_path": "/tmp",
                "read_only": true,
                "allowed_domains": ["example.com"]
            })
            .to_string(),
            &mut Vec::new(),
            None,
            None,
        )
        .await
        .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);
    }

    #[tokio::test]
    async fn subscribe_is_ok_and_enforces_subscription_limit() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                max_subscriptions: 1,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let ok = serde_json::json!({
            "id": "1",
            "type": "subscribe",
            "topics": ["navigation"]
        })
        .to_string();
        let response = handle_request(&state, &ok, &mut Vec::new(), None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);

        let too_many = serde_json::json!({
            "id": "2",
            "type": "subscribe",
            "topics": ["navigation", "capability"]
        })
        .to_string();
        let response = handle_request(&state, &too_many, &mut Vec::new(), None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(!parsed.ok);
    }

    #[tokio::test]
    async fn cache_stats_is_ok_when_enabled() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                expose_cache_api: true,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            permissions: PermissionPolicy {
                capabilities: {
                    let mut map = PermissionPolicy::default().capabilities;
                    map.insert(Capability::CacheRead, PermissionDecision::Allow);
                    map
                },
                ..PermissionPolicy::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let response =
            handle_request(&state, r#"{"id":"1","type":"cache-stats"}"#, &mut Vec::new(), None, None)
                .await
                .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);
    }

    #[tokio::test]
    async fn terminal_exec_stream_emits_output_and_done() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            permissions: PermissionPolicy {
                capabilities: {
                    let mut map = PermissionPolicy::default().capabilities;
                    map.insert(Capability::TerminalExec, PermissionDecision::Allow);
                    map
                },
                ..PermissionPolicy::default()
            },
            terminal: crate::config::TerminalConfig {
                allowlist: vec!["echo".to_string()],
                timeout_ms: 2000,
                ..crate::config::TerminalConfig::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state =
            AutomationServerState::new(config.automation.clone(), runtime.handle.clone(), audit_logger);

        let mut subscribed_topics = vec!["terminal".to_string()];
        let mut receiver = runtime.handle.event_tx.subscribe();

        let raw =
            r#"{"id":"1","type":"terminal-exec-stream","cmd":"echo","args":["hi"],"cwd":null}"#;
        let response = handle_request(&state, raw, &mut subscribed_topics, None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);
        let session_id = parsed
            .result
            .as_ref()
            .and_then(|v| v.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();

        let mut saw_hi = false;
        let mut saw_done = false;
        for _ in 0..40 {
            if let Ok(Ok(AutomationEvent::TerminalOutput(ev))) =
                timeout(Duration::from_millis(200), receiver.recv()).await
            {
                if ev.session_id != session_id {
                    continue;
                }
                if ev.stream == "stdout" && ev.chunk.contains("hi") {
                    saw_hi = true;
                }
                if ev.done {
                    saw_done = true;
                    break;
                }
            }
        }
        assert!(saw_hi, "expected stdout chunk to include hi");
        assert!(saw_done, "expected done event");
    }

    #[tokio::test]
    async fn terminal_cancel_kills_running_session() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            permissions: PermissionPolicy {
                capabilities: {
                    let mut map = PermissionPolicy::default().capabilities;
                    map.insert(Capability::TerminalExec, PermissionDecision::Allow);
                    map
                },
                ..PermissionPolicy::default()
            },
            terminal: crate::config::TerminalConfig {
                allowlist: vec!["sh".to_string()],
                timeout_ms: 10_000,
                ..crate::config::TerminalConfig::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state =
            AutomationServerState::new(config.automation.clone(), runtime.handle.clone(), audit_logger);

        let mut subscribed_topics = vec!["terminal".to_string()];
        let mut receiver = runtime.handle.event_tx.subscribe();

        let raw = r#"{"id":"1","type":"terminal-exec-stream","cmd":"sh","args":["-c","echo start; sleep 5; echo end"],"cwd":null}"#;
        let response = handle_request(&state, raw, &mut subscribed_topics, None, None)
            .await
            .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);
        let session_id = parsed
            .result
            .as_ref()
            .and_then(|v| v.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();

        // Wait for "start", then cancel.
        for _ in 0..50 {
            if let Ok(Ok(AutomationEvent::TerminalOutput(ev))) =
                timeout(Duration::from_millis(200), receiver.recv()).await
            {
                if ev.session_id == session_id && ev.stream == "stdout" && ev.chunk.contains("start")
                {
                    break;
                }
            }
        }

        let cancel = format!(
            r#"{{"id":"2","type":"terminal-cancel","session_id":"{}"}}"#,
            session_id
        );
        let cancel_resp = handle_request(&state, &cancel, &mut subscribed_topics, None, None)
            .await
            .expect("cancel response");
        let cancel_parsed: AutomationResponse<serde_json::Value> =
            serde_json::from_str(&cancel_resp).unwrap();
        assert!(cancel_parsed.ok);

        // Ensure we eventually see the done event.
        let mut done = false;
        for _ in 0..80 {
            if let Ok(Ok(AutomationEvent::TerminalOutput(ev))) =
                timeout(Duration::from_millis(200), receiver.recv()).await
            {
                if ev.session_id == session_id && ev.done {
                    done = true;
                    break;
                }
            }
        }
        assert!(done, "expected done after cancel");
    }

    #[tokio::test]
    async fn cache_body_returns_base64_bytes_when_present() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                expose_cache_api: true,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            permissions: PermissionPolicy {
                capabilities: {
                    let mut map = PermissionPolicy::default().capabilities;
                    map.insert(Capability::CacheRead, PermissionDecision::Allow);
                    map
                },
                ..PermissionPolicy::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        // Create a minimal cache entry on disk.
        let profile_id = "default".to_string();
        let cache_root = paths.cache_dir.join(&profile_id);
        let blobs_dir = cache_root.join("blobs");
        std::fs::create_dir_all(&blobs_dir).unwrap();
        let body_key = "blob1";
        let payload = b"hello-cache";
        std::fs::write(blobs_dir.join(body_key), payload).unwrap();
        let entry = crate::cache::AssetMetadata {
            asset_id: "asset1".to_string(),
            url: "https://example.com/asset".to_string(),
            mime: "text/plain".to_string(),
            size_bytes: payload.len() as u64,
            body_key: Some(body_key.to_string()),
            profile_id: profile_id.clone(),
            created_at: Utc::now().to_rfc3339(),
            ..crate::cache::AssetMetadata::default()
        };
        let index_path = cache_root.join("index.jsonl");
        std::fs::write(&index_path, format!("{}\n", serde_json::to_string(&entry).unwrap()))
            .unwrap();

        let mount_manager = crate::mounts::MountManager::new();
        let runtime = start_automation_runtime(&config, &paths, mount_manager).expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let response = handle_request(
            &state,
            r#"{"id":"1","type":"cache-body","asset_id":"asset1"}"#,
            &mut Vec::new(),
            None,
            None,
        )
        .await
        .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);
        let b64 = parsed
            .result
            .as_ref()
            .and_then(|v| v.get("body_base64"))
            .and_then(|v| v.as_str())
            .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("decode b64");
        assert_eq!(bytes, payload);
    }

    #[tokio::test]
    async fn profile_create_creates_profile_dir_and_state_db() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            ..BrazenConfig::default()
        };

        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let runtime = start_automation_runtime(&config, &paths, crate::mounts::MountManager::new())
            .expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let response = handle_request(
            &state,
            r#"{"id":"1","type":"profile-create","profile_id":"p2"}"#,
            &mut Vec::new(),
            None,
            None,
        )
        .await
        .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);
        assert!(paths.profiles_dir.join("p2").exists());
        assert!(paths.profiles_dir.join("p2/state.sqlite").exists());
    }

    #[tokio::test]
    async fn profile_switch_updates_config_active_profile() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            ..BrazenConfig::default()
        };

        let config_path = dir.path().join("brazen.toml");
        std::fs::write(&config_path, " \n").unwrap();

        let paths = RuntimePaths {
            config_path: config_path.clone(),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        std::fs::create_dir_all(paths.profiles_dir.join("p2")).unwrap();

        let runtime = start_automation_runtime(&config, &paths, crate::mounts::MountManager::new())
            .expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        let response = handle_request(
            &state,
            r#"{"id":"1","type":"profile-switch","profile_id":"p2"}"#,
            &mut Vec::new(),
            None,
            None,
        )
        .await
        .expect("response");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);

        let updated = std::fs::read_to_string(&config_path).unwrap();
        assert!(
            updated.contains("active_profile") && updated.contains("p2"),
            "expected config to include active_profile=p2: {updated}"
        );
    }

    #[tokio::test]
    async fn profile_export_and_import_round_trip_state() {
        let dir = tempdir().unwrap();
        let config = BrazenConfig {
            automation: AutomationConfig {
                enabled: true,
                require_auth: false,
                ..AutomationConfig::default()
            },
            features: crate::config::FeatureFlags {
                automation_server: true,
                ..crate::config::FeatureFlags::default()
            },
            ..BrazenConfig::default()
        };

        let config_path = dir.path().join("brazen.toml");
        std::fs::write(&config_path, " \n").unwrap();
        let paths = RuntimePaths {
            config_path,
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };

        let runtime = start_automation_runtime(&config, &paths, crate::mounts::MountManager::new())
            .expect("runtime");
        let audit_logger = Arc::new(AuditLogger::new(paths.audit_log_path.clone()));
        let state = AutomationServerState::new(config.automation.clone(), runtime.handle, audit_logger);

        // Create profile with some state.
        let profile_id = "p_src";
        let profile_dir = paths.profiles_dir.join(profile_id);
        std::fs::create_dir_all(&profile_dir).unwrap();
        let db = crate::profile_db::ProfileDb::open(profile_dir.join("state.sqlite")).unwrap();
        db.save_tts_state(true, &["hi".to_string()]).unwrap();

        let cache_dir = paths.cache_dir.join(profile_id);
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("index.jsonl"), "[]\n").unwrap();

        // Export.
        let bundle = dir.path().join("bundle.tar.gz");
        let raw = serde_json::json!({
            "id": "1",
            "type": "profile-export",
            "profile_id": profile_id,
            "output_path": bundle.display().to_string(),
            "include_cache_blobs": false
        })
        .to_string();
        let response = handle_request(&state, &raw, &mut Vec::new(), None, None)
            .await
            .expect("export resp");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);
        assert!(bundle.exists());

        // Import into another profile id.
        let raw = serde_json::json!({
            "id": "2",
            "type": "profile-import",
            "profile_id": "p_dst",
            "input_path": bundle.display().to_string(),
            "overwrite": true
        })
        .to_string();
        let response = handle_request(&state, &raw, &mut Vec::new(), None, None)
            .await
            .expect("import resp");
        let parsed: AutomationResponse<serde_json::Value> = serde_json::from_str(&response).unwrap();
        assert!(parsed.ok);

        let dst_db = crate::profile_db::ProfileDb::open(paths.profiles_dir.join("p_dst/state.sqlite")).unwrap();
        let (playing, queue) = dst_db.load_tts_state().unwrap();
        assert!(playing);
        assert_eq!(queue.first().map(|s| s.as_str()), Some("hi"));
    }
}

// (no additional helpers)
