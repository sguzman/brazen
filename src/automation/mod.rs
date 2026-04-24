use std::sync::{Arc, RwLock};
use std::sync::atomic::AtomicU64;
use tokio::sync::{broadcast, mpsc};
use crate::config::BrazenConfig;
use crate::platform_paths::RuntimePaths;
use crate::audit_log::AuditLogger;
use crate::{ShellState, commands};
use base64::Engine;

pub mod types;
pub mod handle;
pub mod server;
pub mod handlers;

pub use types::*;
pub use handle::AutomationHandle;
pub use server::{AutomationServerState, run_automation_server};

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
            AutomationCommand::Stop => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::StopLoading,
                );
            }
            AutomationCommand::GoBack => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::GoBack,
                );
            }
            AutomationCommand::GoForward => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::GoForward,
                );
            }
            AutomationCommand::DomQuery { selector, response_tx } => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::DomQuery { selector, response_tx },
                );
            }
            AutomationCommand::Screenshot { response_tx } => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::ScreenshotTab { response_tx },
                );
            }
            AutomationCommand::EvaluateJavascript { script, response_tx } => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::EvaluateJavascript { script, response_tx },
                );
            }
            AutomationCommand::AddMount { name, local_path, read_only, allowed_domains } => {
                shell_state.mount_manager.add_mount(crate::mounts::Mount {
                    name,
                    mount_type: crate::mounts::MountType::FileSystem(local_path),
                    read_only,
                    allowed_domains: allowed_domains,
                });
            }
            AutomationCommand::RemoveMount { name } => {
                shell_state.mount_manager.remove_mount(&name);
            }
            AutomationCommand::RenderedText { response_tx } => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::GetRenderedText { response_tx },
                );
            }
            AutomationCommand::ArticleText { response_tx } => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::GetArticleText { response_tx },
                );
            }
            AutomationCommand::CacheStats { response_tx } => {
                let stats = cache.stats();
                let _ = response_tx.send(Ok(serde_json::to_value(stats).unwrap()));
            }
            AutomationCommand::CacheQuery { query, limit, response_tx } => {
                let query = query.unwrap_or_default();
                let mut assets = cache.query(query);
                if let Some(limit) = limit {
                    assets.truncate(limit);
                }
                let summaries = assets.iter().map(handle::asset_summary_from_metadata).collect();
                let _ = response_tx.send(Ok(summaries));
            }
            AutomationCommand::CacheBody { asset_id, response_tx } => {
                if let Some(entry) = cache.find_by_id_or_hash(&asset_id) {
                    if let Some(body_key) = &entry.body_key {
                        let path = cache.blob_path(body_key);
                        match std::fs::read(path) {
                            Ok(body) => {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(&body);
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
                let cmd = match action.as_str() {
                    "pause" => commands::AppCommand::TtsPause,
                    "resume" => commands::AppCommand::TtsResume,
                    "stop" => commands::AppCommand::TtsStop,
                    _ => {
                        let _ = response_tx.send(Err("invalid tts action".to_string()));
                        continue;
                    }
                };
                let _ = commands::dispatch_command(shell_state, engine, cmd);
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::TtsEnqueue { text, response_tx } => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::TtsEnqueue(text),
                );
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::ReadingEnqueue { url, title, kind, article_text, response_tx } => {
                let item = crate::app::ReadingQueueItem {
                    url,
                    title,
                    kind,
                    saved_at: chrono::Utc::now().to_rfc3339(),
                    progress: 0.0,
                    article_text,
                };
                shell_state.reading_queue.push_back(item);
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::ReadingSetProgress { url, progress, response_tx } => {
                if let Some(item) = shell_state.reading_queue.iter_mut().find(|i| i.url == url) {
                    item.progress = progress;
                    let _ = response_tx.send(Ok(()));
                } else {
                    let _ = response_tx.send(Err("item not found".to_string()));
                }
            }
            AutomationCommand::ReadingRemove { url, response_tx } => {
                shell_state.reading_queue.retain(|i| i.url != url);
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::ReadingClear { response_tx } => {
                shell_state.reading_queue.clear();
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::ReaderModeOpen { url, response_tx } => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::OpenReaderMode(url),
                );
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::ReaderModeClose { response_tx } => {
                shell_state.reader_mode_open = false;
                let _ = response_tx.send(Ok(()));
            }
            AutomationCommand::InteractDom { selector, event, value, response_tx } => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::InteractDom { selector, event, value, response_tx },
                );
            }
            AutomationCommand::ScreenshotWindow { response_tx } => {
                let _ = commands::dispatch_command(
                    shell_state,
                    engine,
                    commands::AppCommand::ScreenshotWindow { response_tx },
                );
            }
        }
    }
}
