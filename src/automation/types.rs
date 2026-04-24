use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use crate::cache::{AssetMetadata, AssetQuery, CacheStats};
use crate::engine::EngineFrame;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationActivity {
    pub id: String,
    pub command: String,
    pub status: AutomationActivityStatus,
    pub timestamp: String,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub enum AutomationActivityStatus {
    Pending,
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationTab {
    pub tab_id: String,
    pub index: usize,
    pub title: String,
    pub url: String,
    pub zoom: f32,
    pub pinned: bool,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationNavigationEvent {
    pub url: String,
    pub title: String,
    pub load_status: Option<String>,
    pub load_progress: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationCapabilityEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "topic", rename_all = "kebab-case")]
pub enum AutomationEvent {
    Navigation(AutomationNavigationEvent),
    Capability(AutomationCapabilityEvent),
    TerminalOutput(AutomationTerminalOutputEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationTerminalOutputEvent {
    pub session_id: String,
    pub stream: String,
    pub chunk: String,
    pub done: bool,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationEnvelope<T> {
    pub id: Option<String>,
    #[serde(flatten)]
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    ConnectorList,
    ConnectorSet {
        connector: String,
        enabled: bool,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
