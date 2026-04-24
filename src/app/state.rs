use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use super::DiagnosticTab;

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
