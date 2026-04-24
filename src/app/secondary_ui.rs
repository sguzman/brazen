use super::*;
use crate::cache::AssetQuery;

impl super::BrazenApp {
    pub fn render_resources_content(&mut self, ui: &mut eframe::egui::Ui) {
        ui.collapsing("📁 Mounts", |ui| {
            for mount in self.shell_state.mount_manager.list_mounts() {
                ui.label(format!("• {}", mount.name));
            }
        });
        ui.collapsing("🔌 MCP Servers", |ui| {
            for server in crate::mcp::McpBroker::list_servers() {
                ui.horizontal(|ui| {
                    ui.label(format!("• {}", server));
                    ui.with_layout(eframe::egui::Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                        ui.label(eframe::egui::RichText::new("ONLINE").color(eframe::egui::Color32::GREEN).small());
                    });
                });
            }
        });
    }

    pub fn render_telemetry_content(&mut self, ui: &mut eframe::egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Engine:");
            ui.strong(&self.shell_state.backend_name);
        });
        ui.horizontal(|ui| {
            ui.label("Status:");
            ui.strong(self.shell_state.engine_status.to_string());
        });
        ui.horizontal(|ui| {
            ui.label("Visits:");
            ui.strong(self.shell_state.visit_total.to_string());
        });
        
        ui.add_space(4.0);
        ui.collapsing("Capabilities", |ui| {
            ui.checkbox(&mut self.shell_state.observe_dom, "Observe DOM");
            ui.checkbox(&mut self.shell_state.control_terminal, "Control Terminal");
            ui.checkbox(&mut self.shell_state.use_mcp_tools, "Use MCP Tools");
        });
    }

    pub fn render_terminal_content(&mut self, ui: &mut eframe::egui::Ui) {
        eframe::egui::ScrollArea::vertical()
            .id_salt("dash_term_scroll")
            .max_height(200.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.shell_state.terminal_history {
                    ui.monospace(line);
                }
            });
        ui.horizontal(|ui| {
            ui.label("$");
            let response = ui.add_enabled(
                !self.shell_state.terminal_busy && self.shell_state.control_terminal,
                eframe::egui::TextEdit::singleline(&mut self.shell_state.terminal_input)
                    .desired_width(f32::INFINITY)
            );
            if response.lost_focus() && ui.input(|i| i.key_pressed(eframe::egui::Key::Enter)) {
                let cmd = std::mem::take(&mut self.shell_state.terminal_input);
                if !cmd.is_empty() {
                    self.shell_state.terminal_history.push(format!("$ {}", cmd));
                    if let Some(tx) = &self.terminal_tx {
                        let _ = tx.send(cmd);
                    }
                }
            }
        });
    }

    pub fn render_cache_panel(&mut self, ui: &mut eframe::egui::Ui) {
        ui.separator();
        ui.heading("Cache");
        let stats = self.cache_store.stats();
        ui.label(format!(
            "Entries: {} | Bodies: {} | Blobs: {} | Bytes: {} | Ratio: {:.2}",
            stats.entries,
            stats.captured_with_body,
            stats.unique_blobs,
            stats.total_bytes,
            stats.capture_ratio
        ));
        if let Some(last) = self.cache_store.latest_entry() {
            ui.label(format!("Last capture: {} {}", last.created_at, last.url));
        }
        ui.horizontal(|ui| {
            if ui.button("Sim Capture").clicked() {
                let mut headers = std::collections::BTreeMap::new();
                headers.insert("content-type".to_string(), "text/html".to_string());
                let session_id = Some(self.shell_state.session.read().unwrap().session_id.0.to_string());
                let tab_id = Some(self.shell_state.session.read().unwrap().active_tab().unwrap().id.0.to_string());
                let _ = self.cache_store.record_asset(
                    &self.shell_state.active_tab.current_url,
                    None,
                    Some("GET".to_string()),
                    Some(200),
                    "text/html",
                    Some(b"<html><body>Brazen</body></html>"),
                    headers,
                    false,
                    false,
                    session_id,
                    tab_id,
                    Some("request-1".to_string()),
                );
                self.shell_state.record_event("cache capture simulated");
            }
            if ui.button("Export").clicked()
                && self
                    .cache_store
                    .export_json(self.cache_export_path.as_ref())
                    .is_ok()
            {
                self.shell_state.record_event("cache export complete");
            }
            if ui.button("Import").clicked()
                && self
                    .cache_store
                    .import_json(self.cache_import_path.as_ref())
                    .is_ok()
            {
                self.shell_state.record_event("cache import complete");
            }
            if ui.button("Manifest").clicked()
                && self
                    .cache_store
                    .build_replay_manifest(self.cache_manifest_path.as_ref())
                    .is_ok()
            {
                self.shell_state.record_event("cache manifest written");
            }
        });
        ui.horizontal(|ui| {
            ui.label("URL");
            ui.text_edit_singleline(&mut self.cache_query_url);
        });
        ui.horizontal(|ui| {
            ui.label("MIME");
            ui.text_edit_singleline(&mut self.cache_query_mime);
        });
        ui.horizontal(|ui| {
            ui.label("Hash");
            ui.text_edit_singleline(&mut self.cache_query_hash);
        });
        ui.horizontal(|ui| {
            ui.label("Session");
            ui.text_edit_singleline(&mut self.cache_query_session);
        });
        ui.horizontal(|ui| {
            ui.label("Tab");
            ui.text_edit_singleline(&mut self.cache_query_tab);
        });
        ui.horizontal(|ui| {
            ui.label("Status");
            ui.text_edit_singleline(&mut self.cache_query_status);
        });

        let query = AssetQuery {
            url: empty_to_none(&self.cache_query_url),
            mime: empty_to_none(&self.cache_query_mime),
            hash: empty_to_none(&self.cache_query_hash),
            session_id: empty_to_none(&self.cache_query_session),
            tab_id: empty_to_none(&self.cache_query_tab),
            status_code: self.cache_query_status.trim().parse::<u16>().ok(),
        };
        let results = self.cache_store.query(query);
        ui.label(format!(
            "Assets: {} (storage: {:?})",
            self.cache_store.entries().len(),
            self.cache_store.storage_mode()
        ));
        ui.label(format!("Matches: {}", results.len()));
        ui.separator();
        ui.label("Recent");
        for entry in self.cache_store.entries().iter().rev().take(5) {
            ui.label(format!("{} {}", entry.created_at, entry.url));
        }
        ui.separator();
        ui.label("Matches (latest)");
        for entry in results.iter().rev().take(5) {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} {} {}",
                    entry
                        .status_code
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    entry.mime,
                    entry.url
                ));
                if ui.button("Details").clicked() {
                    self.cache_selected_asset = Some(entry.asset_id.clone());
                }
                if let Some(hash) = &entry.hash {
                    if entry.pinned {
                        if ui.button("Unpin").clicked() {
                            let _ = self.cache_store.unpin_asset(hash);
                            self.shell_state.record_event("asset unpinned");
                        }
                    } else if ui.button("Pin").clicked() {
                        let _ = self.cache_store.pin_asset(hash);
                        self.shell_state.record_event("asset pinned");
                    }
                }
            });
        }
        if let Some(selected) = self.cache_selected_asset.clone()
            && let Some(entry) = self.cache_store.find_by_id_or_hash(&selected)
        {
            ui.separator();
            ui.label(format!("Asset: {}", entry.asset_id));
            ui.label(format!("URL: {}", entry.url));
            if let Some(final_url) = &entry.final_url {
                ui.label(format!("Final URL: {}", final_url));
            }
            ui.label(format!(
                "Method/Status: {} {}",
                entry.method.as_deref().unwrap_or("-"),
                entry
                    .status_code
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ));
            ui.label(format!("MIME: {}", entry.mime));
            ui.label(format!(
                "Hash: {}",
                entry.hash.clone().unwrap_or_else(|| "-".to_string())
            ));
            if let Some(body_key) = &entry.body_key {
                ui.label(format!(
                    "Body key: {} ({})",
                    body_key,
                    self.cache_store.blob_path(body_key).display()
                ));
            }
            ui.label(format!(
                "Timing: start={:?} finish={:?} duration_ms={:?}",
                entry.request_started_at, entry.response_finished_at, entry.duration_ms
            ));
            ui.label(format!("Storage: {:?}", entry.storage_mode));
            ui.label(format!("Headers: {}", entry.response_headers.len()));
            if ui.button("Clear Details").clicked() {
                self.cache_selected_asset = None;
            }
        }
    }
}

fn empty_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
