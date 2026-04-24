use eframe::egui::Color32;
use std::collections::HashMap;
use super::state::*;
use crate::automation::AutomationActivityStatus;
use crate::commands::{AppCommand, dispatch_command};

impl super::BrazenApp {
    pub fn render_workspace_settings(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.workspace_settings {
            return;
        }
        let mut open = true;
        let mut changed = false;
        eframe::egui::Window::new("Workspace Settings")
            .open(&mut open)
            .default_width(500.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::Layout, "Layout");
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::Features, "Features");
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::DevTools, "DevTools");
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::Automation, "Automation");
                    ui.selectable_value(&mut self.settings_tab, SettingsTab::Appearance, "Appearance");
                });
                ui.separator();

                eframe::egui::ScrollArea::vertical().show(ui, |ui| {
                    match self.settings_tab {
                        SettingsTab::Layout => {
                            ui.label("Layout Presets");
                            ui.horizontal(|ui| {
                                if ui.button("Default").clicked() { self.apply_layout_preset(LayoutPreset::Default); }
                                if ui.button("Developer").clicked() { self.apply_layout_preset(LayoutPreset::Developer); }
                                if ui.button("Archive").clicked() { self.apply_layout_preset(LayoutPreset::Archive); }
                            });
                            ui.separator();
                            changed |= ui.checkbox(&mut self.panels.sidebar_visible, "Show Sidebar").changed();
                            changed |= ui.checkbox(&mut self.panels.terminal, "Terminal Panel").changed();
                            changed |= ui.checkbox(&mut self.panels.dashboard, "Command Center Dashboard").changed();
                        }
                        SettingsTab::Features => {
                            changed |= ui.checkbox(&mut self.panels.bookmarks, "Bookmarks").changed();
                            changed |= ui.checkbox(&mut self.panels.history, "History").changed();
                            changed |= ui.checkbox(&mut self.panels.reading_queue, "Reading Queue").changed();
                            changed |= ui.checkbox(&mut self.panels.reader_mode, "Reader Mode").changed();
                            changed |= ui.checkbox(&mut self.panels.tts_controls, "TTS Controls").changed();
                        }
                        SettingsTab::DevTools => {
                            changed |= ui.checkbox(&mut self.panels.bottom_panel_visible, "Bottom Diagnostic Panel").changed();
                            ui.separator();
                            ui.label("Active Tab");
                            changed |= ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Network, "Network").changed();
                            changed |= ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Dom, "DOM Inspector").changed();
                            changed |= ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Health, "Engine Health").changed();
                            changed |= ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Cache, "Cache Explorer").changed();
                            changed |= ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::KnowledgeGraph, "Knowledge Graph").changed();
                        }
                        SettingsTab::Automation => {
                            changed |= ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Automation, "Automation Console").changed();
                            changed |= ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Capabilities, "Capability Inspector").changed();
                        }
                        SettingsTab::Appearance => {
                            ui.label("Theme");
                            changed |= ui.radio_value(&mut self.ui_theme, UiTheme::System, "System").clicked();
                            changed |= ui.radio_value(&mut self.ui_theme, UiTheme::Light, "Light").clicked();
                            changed |= ui.radio_value(&mut self.ui_theme, UiTheme::Dark, "Dark").clicked();
                            changed |= ui.radio_value(&mut self.ui_theme, UiTheme::Brazen, "Brazen").clicked();
                            ui.separator();
                            ui.label("Density");
                            changed |= ui.radio_value(&mut self.ui_density, UiDensity::Comfortable, "Comfortable").clicked();
                            changed |= ui.radio_value(&mut self.ui_density, UiDensity::Compact, "Compact").clicked();
                        }
                    }
                });
            });
        if !open {
            self.panels.workspace_settings = false;
            changed = true;
        }
        if changed {
            self.save_workspace_layout();
        }
    }

    pub fn render_bookmarks_panel(&mut self, ctx: &eframe::egui::Context) {
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

    pub fn render_history_panel(&mut self, ctx: &eframe::egui::Context) {
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

    pub fn render_bottom_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.bottom_panel_visible {
            return;
        }

        eframe::egui::TopBottomPanel::bottom("unified_bottom_panel")
            .resizable(true)
            .default_height(self.panels.bottom_panel_height)
            .show(ctx, |ui| {
                // Update persisted height
                self.panels.bottom_panel_height = ui.available_height().max(100.0);

                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Logs, "Logs");
                    ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Network, "Network");
                    ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Dom, "DOM");
                    ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Health, "Health");
                    ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Downloads, "Downloads");
                    ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Automation, "Automation");
                    ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Cache, "Cache");
                    ui.selectable_value(&mut self.panels.active_diagnostic_tab, DiagnosticTab::Capabilities, "Capabilities");

                    ui.with_layout(eframe::egui::Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                        if ui.button("❌").clicked() {
                            self.panels.bottom_panel_visible = false;
                        }
                    });
                });

                ui.separator();

                eframe::egui::ScrollArea::vertical()
                    .id_salt("bottom_panel_scroll")
                    .show(ui, |ui| {
                        match self.panels.active_diagnostic_tab {
                            DiagnosticTab::Logs => self.render_log_tab(ui),
                            DiagnosticTab::Network => self.render_network_tab(ui),
                            DiagnosticTab::Dom => self.render_dom_tab(ui),
                            DiagnosticTab::Health => self.render_health_tab(ui),
                            DiagnosticTab::Downloads => self.render_downloads_tab(ui),
                            DiagnosticTab::Automation => self.render_automation_tab(ui),
                            DiagnosticTab::Cache => self.render_cache_panel(ui),
                            DiagnosticTab::Capabilities => self.render_capabilities_tab(ui),
                            DiagnosticTab::KnowledgeGraph => self.render_knowledge_graph_tab(ui),
                        }
                    });
            });
    }

    fn render_log_tab(&mut self, ui: &mut eframe::egui::Ui) {
        ui.heading("Startup and Command Log");
        if let Some(avg) = crate::app::frame_average_ms(&self.frame_times) {
            let last = self.last_frame_ms.map(|ms| format!("{ms:.1}ms")).unwrap_or_else(|| "n/a".to_string());
            ui.label(format!("Frame timing: avg {avg:.1}ms (last {last})"));
        }
        for event in self.shell_state.event_log.iter().rev().take(128) {
            ui.monospace(event);
        }
    }

    fn render_health_tab(&mut self, ui: &mut eframe::egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Status:");
            ui.monospace(self.shell_state.engine_status.to_string());
        });

        ui.horizontal(|ui| {
            ui.label("Upstream Active:");
            let color = if self.shell_state.upstream_active { Color32::from_rgb(0, 200, 0) } else { Color32::from_rgb(255, 50, 50) };
            ui.label(eframe::egui::RichText::new(if self.shell_state.upstream_active { "YES" } else { "NO" }).color(color));
        });

        if let Some(ready) = self.shell_state.resource_reader_ready {
            ui.horizontal(|ui| {
                ui.label("Resource Reader:");
                let color = if ready { Color32::from_rgb(0, 200, 0) } else { Color32::from_rgb(255, 165, 0) };
                ui.label(eframe::egui::RichText::new(if ready { "READY" } else { "PENDING" }).color(color));
            });
        }

        if let Some(path) = &self.shell_state.resource_reader_path {
            ui.horizontal(|ui| { ui.label("Resource Path:"); ui.monospace(path); });
        }

        if let Some(error) = &self.shell_state.upstream_last_error {
            ui.separator();
            ui.label(eframe::egui::RichText::new("Last Error:").color(Color32::from_rgb(255, 50, 50)));
            ui.monospace(error);
        }
    }

    fn render_downloads_tab(&mut self, ui: &mut eframe::egui::Ui) {
        if let Some(last) = &self.shell_state.last_download {
            ui.label(format!("Last: {last}"));
        } else {
            ui.label("No downloads yet.");
        }
        ui.separator();
        for item in &self.downloads {
            ui.monospace(item);
        }
    }

    fn render_dom_tab(&mut self, ui: &mut eframe::egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(format!("URL: {}", self.shell_state.active_tab.current_url));
            if ui.button("Refresh Snapshot").clicked() {
                self.engine.evaluate_javascript("document.documentElement.outerHTML".to_string(), Box::new(|_| {}));
            }
        });
        ui.separator();
        if let Some(dom) = &self.shell_state.dom_snapshot {
            ui.add(eframe::egui::TextEdit::multiline(&mut dom.clone())
                .font(eframe::egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY));
        } else {
            ui.label("No DOM snapshot available.");
        }
    }

    fn render_network_tab(&mut self, ui: &mut eframe::egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(format!("Logged Requests: {}", self.shell_state.network_log.len()));
            if ui.button("Clear").clicked() { self.shell_state.network_log.clear(); }
        });
        ui.separator();
        eframe::egui::Grid::new("network_grid_bottom").striped(true).show(ui, |ui| {
            ui.label("Method"); ui.label("Status"); ui.label("Mime"); ui.label("URL"); ui.end_row();
            for req in self.shell_state.network_log.iter().rev() {
                ui.label(&req.method);
                ui.label(req.status.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string()));
                ui.label(req.mime_type.as_deref().unwrap_or("-"));
                ui.label(&req.url);
                ui.end_row();
            }
        });
    }

    fn render_capabilities_tab(&mut self, ui: &mut eframe::egui::Ui) {
        for (cap, decision) in &self.shell_state.capabilities_snapshot {
            ui.horizontal(|ui| { ui.label(cap); ui.monospace(decision); });
        }
    }

    fn render_automation_tab(&mut self, ui: &mut eframe::egui::Ui) {
        ui.heading("Activity Queue");
        for activity in self.shell_state.automation_activities.iter().rev() {
            ui.horizontal(|ui| {
                ui.label(format!("[{}]", activity.timestamp));
                ui.monospace(&activity.command);
                let color = match activity.status {
                    AutomationActivityStatus::Pending => Color32::GRAY,
                    AutomationActivityStatus::Running => Color32::from_rgb(0, 150, 255),
                    AutomationActivityStatus::Success => Color32::from_rgb(0, 200, 0),
                    AutomationActivityStatus::Failed => Color32::from_rgb(255, 50, 50),
                };
                ui.label(eframe::egui::RichText::new(format!("{:?}", activity.status)).color(color));
            });
        }
    }

    fn render_knowledge_graph_tab(&mut self, ui: &mut eframe::egui::Ui) {
        ui.heading("Virtual Mounts");
        let mounts = self.shell_state.mount_manager.list_mounts();
        if mounts.is_empty() {
            ui.label("No active virtual mounts.");
        } else {
            eframe::egui::Grid::new("mounts_grid")
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.label("Type");
                    ui.label("Access");
                    ui.end_row();
                    for mount in mounts {
                        ui.label(&mount.name);
                        ui.label(format!("{:?}", mount.mount_type));
                        ui.label(if mount.read_only { "RO" } else { "RW" });
                        ui.end_row();
                    }
                });
        }

        ui.separator();
        ui.heading("MCP Tools");
        let tools = crate::mcp::McpBroker::list_tools();
        if tools.is_empty() {
            ui.label("No MCP tools registered.");
        } else {
            for tool in tools {
                ui.collapsing(&tool.name, |ui| {
                    ui.label(format!("Description: {}", tool.description));
                    ui.label(format!("Schema: {}", serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default()));
                });
            }
        }

        ui.separator();
        ui.heading("Active Entities");
        if self.shell_state.extracted_entities.is_empty() {
            ui.label("No entities extracted from current page.");
        } else {
            let mut grouped: HashMap<String, Vec<&ExtractedEntity>> = HashMap::new();
            for entity in &self.shell_state.extracted_entities {
                grouped.entry(entity.kind.clone()).or_default().push(entity);
            }

            for (kind, entities) in grouped {
                ui.collapsing(format!("{} ({})", kind, entities.len()), |ui| {
                    for entity in entities {
                        ui.horizontal(|ui| {
                            ui.label(&entity.label);
                            if !entity.metadata.is_empty() {
                                let meta_str = entity.metadata.iter()
                                    .map(|(k, v)| format!("{}: {}", k, v))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                ui.weak(format!("({})", meta_str));
                            }
                            if ui.button("📋").on_hover_text("Copy to clipboard").clicked() {
                                ui.ctx().copy_text(entity.value.clone());
                            }
                        });
                    }
                });
            }
        }
    }

    pub fn render_reading_queue_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.reading_queue {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("Reading Queue")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Items: {}", self.shell_state.reading_queue.len()));
                    if ui.button("Save Current Tab").clicked() {
                        let url = self.shell_state.active_tab.current_url.clone();
                        let title = if self.shell_state.page_title.trim().is_empty() {
                            None
                        } else {
                            Some(self.shell_state.page_title.clone())
                        };
                        self.shell_state.reading_queue.push_back(ReadingQueueItem {
                            url,
                            title,
                            kind: "link".to_string(),
                            saved_at: chrono::Utc::now().to_rfc3339(),
                            progress: 0.0,
                            article_text: None,
                        });
                        self.shell_state.record_event("reading: enqueue link");
                    }
                    if ui.button("Clear").clicked() {
                        self.shell_state.reading_queue.clear();
                        self.shell_state.record_event("reading: clear");
                    }
                });
                ui.separator();
                eframe::egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut remove_index: Option<usize> = None;
                    let mut open_reader: Option<(String, String)> = None;
                    for (idx, item) in self.shell_state.reading_queue.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(item.kind.clone());
                                ui.monospace(&item.url);
                                if ui.button("Open Reader").clicked() {
                                    open_reader = Some((
                                        item.url.clone(),
                                        item.article_text
                                            .clone()
                                            .unwrap_or_else(|| "No article text available.".to_string()),
                                    ));
                                }
                                if ui.button("Remove").clicked() {
                                    remove_index = Some(idx);
                                }
                            });
                            if let Some(title) = &item.title {
                                if !title.trim().is_empty() {
                                    ui.label(title);
                                }
                            }
                            ui.add(
                                eframe::egui::Slider::new(&mut item.progress, 0.0..=1.0)
                                    .text("progress"),
                            );
                            if let Some(text) = &item.article_text {
                                ui.label(format!("article chars: {}", text.len()));
                            }
                        });
                    }
                    if let Some(idx) = remove_index {
                        let _ = self.shell_state.reading_queue.remove(idx);
                        self.shell_state.record_event("reading: remove");
                    }
                    if let Some((url, text)) = open_reader {
                        self.shell_state.reader_mode_open = true;
                        self.shell_state.reader_mode_source_url = Some(url);
                        self.shell_state.reader_mode_text = text;
                        self.shell_state.record_event("reader: open");
                    }
                });
            });
        if !open {
            self.panels.reading_queue = false;
        }
    }

    pub fn render_reader_mode_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.reader_mode {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("Reader Mode")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Source: {}",
                        self.shell_state
                            .reader_mode_source_url
                            .as_deref()
                            .unwrap_or("(none)")
                    ));
                    if ui.button("Close").clicked() {
                        self.shell_state.reader_mode_open = false;
                        self.shell_state.reader_mode_source_url = None;
                        self.shell_state.reader_mode_text.clear();
                        self.shell_state.record_event("reader: close");
                    }
                });
                ui.separator();
                if !self.shell_state.reader_mode_open {
                    ui.label("Reader mode is not open.");
                    return;
                }
                ui.add(
                    eframe::egui::TextEdit::multiline(&mut self.shell_state.reader_mode_text)
                        .desired_rows(24),
                );
            });
        if !open {
            self.panels.reader_mode = false;
        }
    }

    pub fn render_tts_controls_panel(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.tts_controls {
            return;
        }
        let mut open = true;
        eframe::egui::Window::new("TTS Controls")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!(
                    "State: {} | Queue: {}",
                    if self.shell_state.tts_playing { "playing" } else { "paused" },
                    self.shell_state.tts_queue.len()
                ));
                ui.horizontal(|ui| {
                    if ui.button("Play").clicked() {
                        self.shell_state.tts_playing = true;
                        self.shell_state.record_event("tts: play");
                    }
                    if ui.button("Pause").clicked() {
                        self.shell_state.tts_playing = false;
                        self.shell_state.record_event("tts: pause");
                    }
                    if ui.button("Stop").clicked() {
                        self.shell_state.tts_playing = false;
                        self.shell_state.tts_queue.clear();
                        self.shell_state.record_event("tts: stop");
                    }
                    if ui.button("Clear Queue").clicked() {
                        self.shell_state.tts_queue.clear();
                        self.shell_state.record_event("tts: clear");
                    }
                });
            });
        if !open {
            self.panels.tts_controls = false;
        }
    }
}
