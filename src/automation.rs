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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationSnapshot {
    pub tabs: Vec<AutomationTab>,
    pub active_tab_index: usize,
    pub active_tab_id: Option<String>,
    pub address_bar: String,
    pub load_status: Option<String>,
    pub load_progress: f32,
    pub engine_status: String,
    pub cache_stats: CacheStats,
    pub cache_entries: Vec<AutomationAssetSummary>,
    pub activities: VecDeque<AutomationActivity>,
    pub last_event_log_len: usize,
}

impl Default for AutomationSnapshot {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_index: 0,
            active_tab_id: None,
            address_bar: String::new(),
            load_status: None,
            load_progress: 0.0,
            engine_status: "unknown".to_string(),
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
}

#[derive(Clone)]
pub struct AutomationHandle {
    snapshot: Arc<RwLock<AutomationSnapshot>>,
    command_tx: mpsc::UnboundedSender<AutomationCommand>,
    event_tx: broadcast::Sender<AutomationEvent>,
    cache_config: CacheConfig,
    runtime_paths: RuntimePaths,
    profile_id: String,
    permissions: PermissionPolicy,
    expose_tab_api: bool,
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
        snapshot.load_status = shell_state
            .load_status
            .map(|status| status.as_str().to_string());
        snapshot.load_progress = shell_state.load_progress;
        snapshot.engine_status = shell_state.engine_status.to_string();
        snapshot.cache_stats = cache.stats();
        snapshot.cache_entries = cache
            .entries()
            .iter()
            .take(512)
            .map(asset_summary_from_metadata)
            .collect();
        snapshot.last_event_log_len = shell_state.event_log.len();
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
        AutomationRequest::RenderedText => {
            Some(error_response(id, "rendered text not implemented"))
        }
        AutomationRequest::ArticleText => Some(error_response(id, "article text not implemented")),
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
                _ => Some(error_response(id, "unsupported pending request")),
            }
        }
        AutomationRequest::Shutdown => {
            let result = state.handle.request_shutdown();
            match result {
                Ok(()) => Some(ok_response(id)),
                Err(error) => Some(error_response(id, &error)),
            }
        }
        AutomationRequest::CacheStats => {
            if let Err(error) = ensure_cache_api(state) {
                return Some(error_response(id, &error));
            }
            let snapshot = state.handle.snapshot.read().expect("snapshot");
            let response = AutomationResponse {
                id,
                ok: true,
                result: Some(snapshot.cache_stats.clone()),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::CacheQuery { query, limit } => {
            if let Err(error) = ensure_cache_api(state) {
                return Some(error_response(id, &error));
            }
            let snapshot = state.handle.snapshot.read().expect("snapshot");
            let limit = limit.unwrap_or(100).min(500);
            let mut entries: Vec<AutomationAssetSummary> = snapshot.cache_entries.clone();
            if let Some(query) = query {
                entries.retain(|entry| {
                    query
                        .url
                        .as_ref()
                        .map(|q| entry.url.contains(q))
                        .unwrap_or(true)
                        && query
                            .mime
                            .as_ref()
                            .map(|q| entry.mime.contains(q))
                            .unwrap_or(true)
                        && query
                            .hash
                            .as_ref()
                            .map(|q| entry.hash.as_deref() == Some(q))
                            .unwrap_or(true)
                        && query
                            .session_id
                            .as_ref()
                            .map(|q| entry.session_id.as_deref() == Some(q))
                            .unwrap_or(true)
                        && query
                            .tab_id
                            .as_ref()
                            .map(|q| entry.tab_id.as_deref() == Some(q))
                            .unwrap_or(true)
                        && query
                            .status_code
                            .map(|q| entry.status_code == Some(q))
                            .unwrap_or(true)
                });
            }
            entries.truncate(limit);
            let response = AutomationResponse {
                id,
                ok: true,
                result: Some(entries),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::CacheBody { asset_id } => {
            if let Err(error) = ensure_cache_api(state) {
                return Some(error_response(id, &error));
            }
            match load_cache_body(&state.handle, &asset_id) {
                Ok(body) => {
                    let response = AutomationResponse {
                        id,
                        ok: true,
                        result: Some(body),
                        error: None,
                    };
                    Some(serde_json::to_string(&response).unwrap())
                }
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
        AutomationRequest::TtsControl { .. } => {
            Some(error_response(id, "tts control not implemented"))
        }
        AutomationRequest::TtsEnqueue { .. } => {
            Some(error_response(id, "tts enqueue not implemented"))
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform_paths::RuntimePaths;
    use tempfile::tempdir;

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
}
