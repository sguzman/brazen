use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{mpsc, broadcast};
use crate::ShellState;
use crate::cache::{AssetMetadata, AssetStore};
use crate::config::{CacheConfig, TerminalConfig};
use crate::permissions::PermissionPolicy;
use crate::platform_paths::RuntimePaths;
use crate::session::SessionSnapshot;
use super::types::*;

#[derive(Clone)]
pub struct AutomationHandle {
    pub(crate) snapshot: Arc<RwLock<AutomationSnapshot>>,
    pub(crate) command_tx: mpsc::UnboundedSender<AutomationCommand>,
    pub(crate) event_tx: broadcast::Sender<AutomationEvent>,
    #[allow(dead_code)]
    pub(crate) cache_config: CacheConfig,
    pub(crate) runtime_paths: RuntimePaths,
    pub(crate) profile_id: String,
    pub(crate) permissions: PermissionPolicy,
    pub(crate) expose_tab_api: bool,
    #[allow(dead_code)]
    pub(crate) expose_cache_api: bool,
    pub(crate) terminal_config: TerminalConfig,
    pub mount_manager: crate::mounts::MountManager,
    pub(crate) activity_counter: Arc<AtomicU64>,
    pub(crate) egui_ctx: Arc<RwLock<Option<eframe::egui::Context>>>,
}

impl AutomationHandle {
    pub fn new(
        snapshot: Arc<RwLock<AutomationSnapshot>>,
        command_tx: mpsc::UnboundedSender<AutomationCommand>,
        event_tx: broadcast::Sender<AutomationEvent>,
        config: &crate::config::BrazenConfig,
        paths: &RuntimePaths,
        mount_manager: crate::mounts::MountManager,
    ) -> Self {
        Self {
            snapshot,
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
        }
    }

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

    pub fn next_activity_id(&self) -> String {
        self.activity_counter.fetch_add(1, Ordering::SeqCst).to_string()
    }
}

pub(crate) fn build_tab_list(session: &SessionSnapshot) -> Vec<AutomationTab> {
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

pub(crate) fn asset_summary_from_metadata(entry: &AssetMetadata) -> AutomationAssetSummary {
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
