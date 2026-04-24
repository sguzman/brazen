use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use axum::{Router, routing::get, extract::{State, WebSocketUpgrade, Query}, http::{HeaderMap, StatusCode}, response::IntoResponse};
use axum::extract::ws::{WebSocket, Message};
use tokio::sync::{Semaphore, Mutex};
use url::Url;
use crate::audit_log::AuditLogger;
use crate::config::AutomationConfig;
use crate::permissions::Capability;
use super::types::*;
use super::handle::AutomationHandle;
use super::handlers::handle_request;

#[derive(Clone)]
pub struct AutomationServerState {
    pub(crate) handle: AutomationHandle,
    pub(crate) auth_token: Option<String>,
    pub(crate) require_auth: bool,
    pub(crate) max_messages_per_minute: u32,
    pub(crate) max_subscriptions: usize,
    pub(crate) connection_semaphore: Arc<Semaphore>,
    pub(crate) audit_logger: Arc<AuditLogger>,
    pub(crate) pending_approvals: Arc<RwLock<HashMap<String, PendingApproval>>>,
    pub(crate) rate_limit: Arc<RwLock<HashMap<String, (Instant, u32)>>>,
    pub(crate) terminal_sessions: Arc<RwLock<HashMap<String, Arc<Mutex<tokio::process::Child>>>>>,
}

#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub(crate) capability: Capability,
    pub(crate) request: AutomationRequest,
    pub(crate) user_agent: Option<String>,
    pub(crate) client_ip: Option<String>,
}

impl AutomationServerState {
    pub fn new(config: AutomationConfig, handle: AutomationHandle, audit_logger: Arc<AuditLogger>) -> Self {
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

    pub(crate) fn check_permission(&self, capability: Capability) -> Result<(), String> {
        match self.handle.permissions.decision_for(&capability) {
            crate::permissions::PermissionDecision::Allow => Ok(()),
            crate::permissions::PermissionDecision::Ask => Err("approval-required".to_string()),
            crate::permissions::PermissionDecision::Deny => Err("capability denied".to_string()),
        }
    }

    pub(crate) fn check_rate_limit(&self, key: &str, max_per_minute: u32) -> Result<(), String> {
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

    pub(crate) fn check_connector_enabled(&self, connector: &str) -> Result<(), String> {
        let db_path = self
            .handle
            .runtime_paths
            .active_profile_dir
            .join("state.sqlite");
        let Ok(db) = crate::profile_db::ProfileDb::open(db_path) else {
            return Ok(());
        };
        match db.get_connector_enabled(connector) {
            Ok(true) => Ok(()),
            Ok(false) => Err(format!("connector disabled: {connector}")),
            Err(_) => Ok(()),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct AuthQuery {
    token: Option<String>,
}

pub fn run_automation_server(bind: &str, state: AutomationServerState) -> Result<(), String> {
    tracing::info!(target: "brazen::automation", %bind, "parsing automation bind address");
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

                let listener = tokio::net::TcpListener::bind(addr)
                    .await
                    .map_err(|error| format!("automation bind failed: {error}"))?;

                let actual_addr = listener.local_addr().unwrap_or(addr);
                tracing::info!(
                    target: "brazen::automation",
                    addr = %actual_addr,
                    path,
                    "automation server listening"
                );

                if let Ok(endpoint_file) = std::env::var("BRAZEN_AUTOMATION_ENDPOINT_FILE") {
                    let endpoint_url = format!("{}://{actual_addr}{path}", if scheme == "wss" { "wss" } else { "ws" });
                    let path = Path::new(&endpoint_file);
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(error) = std::fs::write(path, endpoint_url.as_bytes()) {
                        tracing::error!(target: "brazen::automation", %error, "failed to write endpoint file");
                    } else {
                        tracing::info!(target: "brazen::automation", path = %endpoint_file, url = %endpoint_url, "wrote endpoint file");
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

async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(query): Query<AuthQuery>,
    State(state): State<AutomationServerState>,
) -> impl IntoResponse {
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());
    let user_agent = headers.get("user-agent").and_then(|v| v.to_str().ok()).map(|s| s.to_string());

    if state.require_auth {
        let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok());
        let token = auth_header
            .and_then(|v| v.strip_prefix("Bearer "))
            .or(query.token.as_deref());

        match (token, state.auth_token.as_deref()) {
            (Some(t), Some(expected)) if t == expected => (),
            _ => {
                tracing::warn!(target: "brazen::automation", ?client_ip, "unauthorized automation attempt");
                return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
            }
        }
    }

    let permit = match state.connection_semaphore.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!(target: "brazen::automation", ?client_ip, "automation connection limit reached");
            return (StatusCode::TOO_MANY_REQUESTS, "Too many connections").into_response();
        }
    };

    ws.on_upgrade(move |socket| handle_socket(socket, state, permit, user_agent, client_ip))
}

async fn handle_socket(
    mut socket: WebSocket,
    state: AutomationServerState,
    _permit: tokio::sync::OwnedSemaphorePermit,
    user_agent: Option<String>,
    client_ip: Option<String>,
) {
    let mut receiver = state.handle.event_tx.subscribe();
    let mut subscribed_topics = Vec::new();

    loop {
        tokio::select! {
            msg = socket.recv() => {
                let Some(msg) = msg else { break; };
                let Ok(msg) = msg else { break; };
                if let Message::Text(text) = msg {
                    if let Some(response) = handle_request(&state, &text, &mut subscribed_topics, user_agent.clone(), client_ip.clone()).await {
                        if socket.send(Message::Text(response.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
            event = receiver.recv() => {
                let Ok(event) = event else { continue; };
                let topic = match &event {
                    AutomationEvent::Navigation(_) => "navigation",
                    AutomationEvent::Capability(_) => "capability",
                    AutomationEvent::TerminalOutput(_) => "terminal",
                };
                if subscribed_topics.contains(&topic.to_string()) {
                    if let Ok(json) = serde_json::to_string(&event) {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
}
