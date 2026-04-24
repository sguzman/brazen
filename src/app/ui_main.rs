use super::*;
use crate::navigation::normalize_url_input;

impl super::BrazenApp {


    fn ensure_engine_initialized(&mut self, ctx: &eframe::egui::Context) {
        if self.last_surface.is_none() {
            let pixels_per_point = ctx.pixels_per_point();
            let metadata = RenderSurfaceMetadata {
                viewport_width: 1024,
                viewport_height: 768,
                scale_factor_basis_points: (pixels_per_point * 100.0) as u32,
            };
            self.engine.attach_surface(self.surface_handle.clone());
            self.engine.set_render_surface(metadata.clone());
            self.last_surface = Some(metadata);

            if let Some(startup_url) = self.pending_startup_url.take() {
                if let Ok(normalized) = normalize_url_input(&startup_url) {
                    self.shell_state
                        .record_event(format!("background startup navigation: {normalized}"));
                    self.engine.navigate(&normalized);
                }
            }
        }
    }
}






pub(super) fn frame_average_ms(times: &VecDeque<f32>) -> Option<f32> {
    if times.is_empty() {
        return None;
    }
    let sum: f32 = times.iter().copied().sum();
    Some(sum / times.len() as f32)
}

impl super::BrazenApp {
    pub(super) fn monitor_chatgpt_mcp(&mut self) {
        if !self.shell_state.observe_dom || !self.shell_state.control_terminal {
            return;
        }
        
        let Some(snapshot) = &self.shell_state.dom_snapshot else { return; };
        
        // Use scraper to find <client mcp="terminal">...</client>
        let fragment = scraper::Html::parse_fragment(snapshot);
        let selector = scraper::Selector::parse("client[mcp=\"terminal\"]").unwrap();
        
        for element in fragment.select(&selector) {
            let command = element.text().collect::<String>();
            let command = command.trim();
            if !command.is_empty() && !self.processed_mcp_commands.contains(command) {
                tracing::info!(target: "brazen::automation", command = %command, "Found new MCP terminal command in DOM");
                
                // Run command
                if let Some(tx) = &self.terminal_tx {
                    let _ = tx.send(command.to_string());
                }
                
                // Mark as processed
                self.processed_mcp_commands.insert(command.to_string());
            }
        }
    }
}

impl eframe::App for super::BrazenApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.ensure_engine_initialized(ctx);
        self.forward_input_events(ctx);
        self.update_render_frame(ctx);
        self.shell_state.sync_from_engine(self.engine.as_mut());
        if let Some(handle) = &self.automation_handle {
            handle.set_egui_context(ctx.clone());
        }
        self.update_automation(ctx);
        self.update_terminal(ctx);
        self.update_render_health();
        self.apply_ui_settings(ctx);
        self.apply_cursor_icon(ctx);
        self.apply_new_window_policy();
        if let Some(reason) = self.shell_state.last_crash.clone()
            && self.shell_state.last_crash_dump.is_none()
        {
            self.write_crash_dump(&reason);
        }
        self.handle_crash_recovery();
        self.monitor_chatgpt_mcp();

        // --- New Modular Layout ---
        self.render_top_menu(ctx);
        self.render_header(ctx);
        
        if self.panels.dashboard {
            self.render_dashboard(ctx);
        } else {
            self.render_left_sidebar(ctx);
            self.render_right_sidebar(ctx);
            self.render_browser_view(ctx);
        }

        self.sync_active_tab_from_session();

        // --- Supplemental Windows ---
        self.render_workspace_settings(ctx);
        self.render_reader_mode_panel(ctx);
        self.render_tts_controls_panel(ctx);

        self.render_bottom_panel(ctx);

        if self.shell_state.find_panel_open {
            eframe::egui::TopBottomPanel::bottom("find_panel")
                .resizable(false)
                .default_height(64.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Find");
                        let response = ui.text_edit_singleline(&mut self.shell_state.find_query);
                        let enter_pressed =
                            ui.input(|input| input.key_pressed(eframe::egui::Key::Enter));
                        if response.changed() && !self.shell_state.find_query.is_empty() {
                            self.shell_state.record_event(format!(
                                "find query: {}",
                                self.shell_state.find_query
                            ));
                        }
                        if (response.lost_focus() && enter_pressed)
                            || ui.button("Find Next").clicked()
                        {
                            self.shell_state.record_event(format!(
                                "find next: {}",
                                self.shell_state.find_query
                            ));
                        }
                        if ui.button("Close").clicked() {
                            self.shell_state.find_panel_open = false;
                        }
                    });
                });
        }

        self.render_command_palette(ctx);
        self.render_context_menu(ctx);
    }
}

impl Drop for super::BrazenApp {
    fn drop(&mut self) {
        self.engine.shutdown();
    }
}
