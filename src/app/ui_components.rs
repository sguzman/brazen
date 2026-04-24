use super::state::*;
use crate::app::PaletteCommand;
use crate::engine::EngineStatus;
use crate::commands::{AppCommand, dispatch_command};
use crate::engine::{RenderSurfaceMetadata};
use crate::navigation::{normalize_url_input};

impl super::BrazenApp {
    pub fn render_browser_view(&mut self, ctx: &eframe::egui::Context) {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            // Determine sizing and get a response object
            let (rect, _response_opt) = if let Some(texture) = &self.render_texture {
                let response = ui.add(eframe::egui::Image::from_texture(texture).shrink_to_fit());
                (response.rect, Some(response))
            } else {
                // Initial placeholder to get a rect and trigger first frame
                let rect = ui.available_rect_before_wrap();
                let response = ui.allocate_rect(rect, eframe::egui::Sense::click_and_drag());
                ui.painter().rect_filled(rect, 0.0, eframe::egui::Color32::BLACK);
                (rect, Some(response))
            };

            // Update render surface based on actual widget size
            let pixels_per_point = ctx.pixels_per_point();
            let metadata = RenderSurfaceMetadata {
                viewport_width: (rect.width() * pixels_per_point) as u32,
                viewport_height: (rect.height() * pixels_per_point) as u32,
                scale_factor_basis_points: (pixels_per_point * 100.0) as u32,
            };

            if self.last_surface.as_ref() != Some(&metadata) {
                self.engine.attach_surface(self.surface_handle.clone());
                self.engine.set_render_surface(metadata.clone());
                self.last_surface = Some(metadata);
                
                // Handle startup URL on first surface attachment
                if let Some(startup_url) = self.pending_startup_url.take() {
                    if let Ok(normalized) = normalize_url_input(&startup_url) {
                        self.shell_state.record_event(format!("startup navigation: {normalized}"));
                        self.engine.navigate(&normalized);
                    }
                }
            }

            self.render_viewport_rect = Some(rect);
                
            // Scaffold Mode Overlay
            if self.shell_state.backend_name == "scaffold" {
                let painter = ui.painter().with_clip_rect(rect);
                painter.rect_filled(
                    rect,
                    0.0,
                    eframe::egui::Color32::from_black_alpha(180),
                );
                painter.text(
                    rect.center() - eframe::egui::vec2(0.0, 60.0),
                    eframe::egui::Align2::CENTER_CENTER,
                    "SCAFFOLD MODE ACTIVE",
                    eframe::egui::FontId::proportional(28.0),
                    eframe::egui::Color32::YELLOW,
                );
                painter.text(
                    rect.center() - eframe::egui::vec2(0.0, 20.0),
                    eframe::egui::Align2::CENTER_CENTER,
                    "To enable full rendering, recompile with:",
                    eframe::egui::FontId::proportional(16.0),
                    eframe::egui::Color32::WHITE,
                );
                painter.text(
                    rect.center() + eframe::egui::vec2(0.0, 10.0),
                    eframe::egui::Align2::CENTER_CENTER,
                    "cargo run --features servo",
                    eframe::egui::FontId::monospace(14.0),
                    eframe::egui::Color32::GREEN,
                );
                painter.text(
                    rect.center() + eframe::egui::vec2(0.0, 50.0),
                    eframe::egui::Align2::CENTER_CENTER,
                    format!("Viewing: {}", self.shell_state.active_tab.current_url),
                    eframe::egui::FontId::monospace(12.0),
                    eframe::egui::Color32::LIGHT_GRAY,
                );
            }

            if self.config.engine.debug_pointer_overlay
                && let Some(pos) = self.last_pointer_pos
                && rect.contains(pos)
            {
                let painter = ui.painter().with_clip_rect(rect);
                let stroke = eframe::egui::Stroke::new(1.0, eframe::egui::Color32::YELLOW);
                let offset = eframe::egui::vec2(8.0, 0.0);
                painter.line_segment([pos - offset, pos + offset], stroke);
                let offset = eframe::egui::vec2(0.0, 8.0);
                painter.line_segment([pos - offset, pos + offset], stroke);
            }
            
            // Floating overlays or info could go here
            if let Some(warning) = &self.shell_state.render_warning {
                ui.colored_label(eframe::egui::Color32::YELLOW, warning);
            }
        });
    }

    pub fn render_header(&mut self, ctx: &eframe::egui::Context) {
        eframe::egui::TopBottomPanel::top("header")
            .frame(eframe::egui::Frame::NONE
                .fill(ctx.style().visuals.panel_fill)
                .inner_margin(eframe::egui::Margin::symmetric(12, 8))
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.heading(eframe::egui::RichText::new("Brazen").strong().color(eframe::egui::Color32::from_rgb(0, 150, 255)));
                    ui.add_space(12.0);
                    
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if ui.button("⏴").on_hover_text("Back").clicked() {
                            let _ = dispatch_command(&mut self.shell_state, self.engine.as_mut(), AppCommand::GoBack);
                        }
                        if ui.button("⏵").on_hover_text("Forward").clicked() {
                            let _ = dispatch_command(&mut self.shell_state, self.engine.as_mut(), AppCommand::GoForward);
                        }
                        if ui.button("⟳").on_hover_text("Reload").clicked() {
                            let _ = dispatch_command(&mut self.shell_state, self.engine.as_mut(), AppCommand::ReloadActiveTab);
                        }
                    });
                    
                    ui.add_space(8.0);
                    
                    let address_bar = eframe::egui::TextEdit::singleline(&mut self.shell_state.address_bar_input)
                        .hint_text("Search or enter address")
                        .desired_width(f32::INFINITY)
                        .margin(eframe::egui::Margin::symmetric(12, 6));
                        
                    let response = ui.add(address_bar);
                    if self.address_bar_focus_pending {
                        response.request_focus();
                        self.address_bar_focus_pending = false;
                    }
                    if response.lost_focus() && ui.input(|i| i.key_pressed(eframe::egui::Key::Enter)) {
                        self.handle_navigation();
                    }
                    
                    ui.add_space(8.0);
                    
                    ui.horizontal(|ui| {
                        if ui.button("🏠").on_hover_text("Dashboard").clicked() {
                            self.panels.dashboard = !self.panels.dashboard;
                        }
                        if ui.button("🔍").on_hover_text("Find").clicked() {
                            self.shell_state.find_panel_open = !self.shell_state.find_panel_open;
                        }
                        if ui.button("👤").on_hover_text("Profile").clicked() {
                            self.panels.workspace_settings = !self.panels.workspace_settings;
                        }
                        if ui.button("⚙").on_hover_text("Settings").clicked() {
                            self.panels.workspace_settings = !self.panels.workspace_settings;
                        }
                    });
                });
                
                if self.shell_state.load_progress > 0.0 && self.shell_state.load_progress < 1.0 {
                    ui.add(eframe::egui::ProgressBar::new(self.shell_state.load_progress).show_percentage());
                }
            });
    }

    pub fn render_left_sidebar(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.sidebar_visible {
            return;
        }
        eframe::egui::SidePanel::left("left_sidebar")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.left_panel_tab, LeftPanelTab::Workspace, "📁 Workspace");
                    ui.selectable_value(&mut self.left_panel_tab, LeftPanelTab::Assets, "📦 Assets");
                });
                ui.separator();
                ui.add_space(4.0);

                match self.left_panel_tab {
                    LeftPanelTab::Workspace => {
                        ui.horizontal(|ui| {
                            if ui.button("New Tab").clicked() {
                                {
                                    let mut session = self.shell_state.session.write().unwrap();
                                    session.open_new_tab("about:blank", "New Tab");
                                    session.active_tab_mut().zoom_level = self.config.engine.zoom_default;
                                }
                                self.shell_state.active_tab_zoom = self.config.engine.zoom_default;
                            }
                            if ui.button("Duplicate").clicked() {
                                self.shell_state.session.write().unwrap().duplicate_active_tab();
                            }
                        });
                        ui.separator();
                        let active_window = self.shell_state.session.read().unwrap().active_window;
                        let (tabs, active_index) = {
                            let session = self.shell_state.session.read().unwrap();
                            if let Some(window) = session.windows.get(active_window) {
                                (window.tabs.clone(), window.active_tab)
                            } else {
                                (Vec::new(), 0)
                            }
                        };
                        eframe::egui::ScrollArea::vertical().id_salt("left_workspace_scroll").show(ui, |ui| {
                            for (index, tab) in tabs.iter().enumerate() {
                                let label = format!(
                                    "{}{} {}",
                                    if index == active_index { ">" } else { " " },
                                    if tab.pinned { "📌" } else { "  " },
                                    tab.title
                                );
                                if ui.selectable_label(index == active_index, label).clicked() {
                                    self.shell_state.session.write().unwrap().set_active_tab(index);
                                    self.shell_state.address_bar_input = tab.url.clone();
                                }
                            }
                        });
                    }
                    LeftPanelTab::Assets => {
                        eframe::egui::ScrollArea::vertical().id_salt("left_assets_scroll").show(ui, |ui| {
                            self.render_resources_content(ui);
                        });
                    }
                }
            });
    }

    pub fn render_right_sidebar(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.terminal {
            return;
        }
        eframe::egui::SidePanel::right("right_panel")
            .resizable(true)
            .width_range(240.0..=600.0)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.heading("💻 Terminal");
                ui.separator();
                ui.add_space(4.0);
                
                self.render_terminal_content(ui);
            });
    }

    pub fn render_dashboard(&mut self, ctx: &eframe::egui::Context) {
        if !self.panels.dashboard {
            return;
        }
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.heading(eframe::egui::RichText::new("Brazen Command Center").size(32.0).strong());
                ui.with_layout(eframe::egui::Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                    if ui.button("Exit Dashboard").clicked() {
                        self.panels.dashboard = false;
                    }
                    if ui.button("Settings").clicked() {
                        self.panels.workspace_settings = true;
                    }
                });
            });
            ui.separator();
            ui.add_space(20.0);

            ui.columns(2, |cols| {
                // Col 1: Files & Assets
                cols[0].vertical(|ui| {
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.heading("Project Assets");
                        ui.separator();
                        eframe::egui::ScrollArea::vertical().id_salt("dash_assets").show(ui, |ui| {
                            self.render_resources_content(ui);
                        });
                    });
                });

                // Col 2: Terminal & Status
                cols[1].vertical(|ui| {
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.heading("System Telemetry");
                        ui.separator();
                        self.render_telemetry_content(ui);
                    });
                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.heading("Terminal");
                        ui.separator();
                        self.render_terminal_content(ui);
                    });
                });
            });
        });
    }

    pub fn render_top_menu(&mut self, ctx: &eframe::egui::Context) {
        eframe::egui::TopBottomPanel::top("top_menu_bar").show(ctx, |ui| {
            eframe::egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Tab").clicked() {
                        self.apply_palette_command(PaletteCommand::NewTab);
                        ui.close();
                    }
                    if ui.button("Close Tab").clicked() {
                        self.apply_palette_command(PaletteCommand::CloseTab);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Copy URL").clicked() {
                        if let Some(url) = self.shell_state.last_committed_url.as_deref() {
                            ctx.copy_text(url.to_string());
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Find...").clicked() {
                        self.shell_state.find_panel_open = true;
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.panels.dashboard, "Dashboard");
                    ui.checkbox(&mut self.panels.sidebar_visible, "Left Sidebar");
                    ui.checkbox(&mut self.panels.terminal, "Right Sidebar (Terminal)");
                    ui.separator();
                    ui.checkbox(&mut self.panels.bottom_panel_visible, "Bottom Panel");
                    ui.menu_button("Diagnostic Tabs", |ui| {
                        if ui.selectable_label(self.panels.active_diagnostic_tab == DiagnosticTab::Logs, "Logs").clicked() {
                            self.panels.active_diagnostic_tab = DiagnosticTab::Logs;
                            self.panels.bottom_panel_visible = true;
                            ui.close();
                        }
                        if ui.selectable_label(self.panels.active_diagnostic_tab == DiagnosticTab::Network, "Network").clicked() {
                            self.panels.active_diagnostic_tab = DiagnosticTab::Network;
                            self.panels.bottom_panel_visible = true;
                            ui.close();
                        }
                        if ui.selectable_label(self.panels.active_diagnostic_tab == DiagnosticTab::Dom, "DOM Inspector").clicked() {
                            self.panels.active_diagnostic_tab = DiagnosticTab::Dom;
                            self.panels.bottom_panel_visible = true;
                            ui.close();
                        }
                        if ui.selectable_label(self.panels.active_diagnostic_tab == DiagnosticTab::Health, "Engine Health").clicked() {
                            self.panels.active_diagnostic_tab = DiagnosticTab::Health;
                            self.panels.bottom_panel_visible = true;
                            ui.close();
                        }
                        if ui.selectable_label(self.panels.active_diagnostic_tab == DiagnosticTab::Downloads, "Downloads").clicked() {
                            self.panels.active_diagnostic_tab = DiagnosticTab::Downloads;
                            self.panels.bottom_panel_visible = true;
                            ui.close();
                        }
                        if ui.selectable_label(self.panels.active_diagnostic_tab == DiagnosticTab::Automation, "Automation").clicked() {
                            self.panels.active_diagnostic_tab = DiagnosticTab::Automation;
                            self.panels.bottom_panel_visible = true;
                            ui.close();
                        }
                        if ui.selectable_label(self.panels.active_diagnostic_tab == DiagnosticTab::Cache, "Cache Explorer").clicked() {
                            self.panels.active_diagnostic_tab = DiagnosticTab::Cache;
                            self.panels.bottom_panel_visible = true;
                            ui.close();
                        }
                        if ui.selectable_label(self.panels.active_diagnostic_tab == DiagnosticTab::Capabilities, "Capabilities").clicked() {
                            self.panels.active_diagnostic_tab = DiagnosticTab::Capabilities;
                            self.panels.bottom_panel_visible = true;
                            ui.close();
                        }
                    });
                    ui.separator();
                    ui.menu_button("Floating Windows", |ui| {
                        ui.checkbox(&mut self.panels.bookmarks, "Bookmarks");
                        ui.checkbox(&mut self.panels.history, "History");
                        ui.checkbox(&mut self.panels.reading_queue, "Reading Queue");
                        ui.checkbox(&mut self.panels.reader_mode, "Reader Mode");
                        ui.checkbox(&mut self.panels.tts_controls, "TTS Controls");
                    });
                    ui.separator();
                    if ui.button("Reload").clicked() {
                        self.apply_palette_command(PaletteCommand::Reload);
                        ui.close();
                    }
                });
                ui.menu_button("Tools", |ui| {
                    ui.checkbox(&mut self.shell_state.observe_dom, "Observe DOM");
                    ui.checkbox(&mut self.shell_state.control_terminal, "Control Terminal");
                    ui.checkbox(&mut self.shell_state.use_mcp_tools, "Use MCP Tools");
                    ui.separator();
                    if ui.button("Settings").clicked() {
                        self.panels.workspace_settings = true;
                        ui.close();
                    }
                });
                
                ui.with_layout(eframe::egui::Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                    if let Some(status) = &self.shell_state.load_status {
                         ui.label(eframe::egui::RichText::new(status.as_str()).small());
                    }
                    if self.shell_state.engine_status != EngineStatus::Ready {
                         ui.spinner();
                    }
                });
            });
        });
    }

    pub fn render_command_palette(&mut self, ctx: &eframe::egui::Context) {
        if !self.command_palette_open {
            return;
        }
        let mut open = true;
        let mut close_requested = false;
        eframe::egui::Window::new("Command Palette")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(
                eframe::egui::Align2::CENTER_TOP,
                eframe::egui::vec2(0.0, 24.0),
            )
            .show(ctx, |ui| {
                let response = ui.add(
                    eframe::egui::TextEdit::singleline(&mut self.command_palette_query)
                        .hint_text("Type a command"),
                );
                if self.command_palette_focus_pending {
                    response.request_focus();
                    self.command_palette_focus_pending = false;
                }
                let query = self.command_palette_query.trim().to_lowercase();
                let entries = Self::palette_entries()
                    .iter()
                    .filter(|entry| entry.label.to_lowercase().contains(&query))
                    .collect::<Vec<_>>();
                ui.separator();
                for entry in entries.iter().take(8) {
                    if ui.button(entry.label).clicked() {
                        self.apply_palette_command(entry.action);
                        close_requested = true;
                    }
                }
                if ui.input(|input| input.key_pressed(eframe::egui::Key::Enter)) {
                    if let Some(entry) = entries.first() {
                        self.apply_palette_command(entry.action);
                    }
                    close_requested = true;
                }
                if ui.input(|input| input.key_pressed(eframe::egui::Key::Escape)) {
                    close_requested = true;
                }
            });
        if close_requested {
            open = false;
        }
        if !open {
            self.command_palette_open = false;
        }
    }

    pub fn render_context_menu(&mut self, ctx: &eframe::egui::Context) {
        let Some((x, y)) = self.shell_state.pending_context_menu else {
            return;
        };
        let mut close_menu = false;
        let screen = ctx.viewport_rect();
        let mut pos = eframe::egui::pos2(x, y);
        let max_x = (screen.right() - 200.0).max(screen.left());
        let max_y = (screen.bottom() - 200.0).max(screen.top());
        pos.x = pos.x.clamp(screen.left(), max_x);
        pos.y = pos.y.clamp(screen.top(), max_y);

        let response = eframe::egui::Area::new(eframe::egui::Id::new("context_menu"))
            .order(eframe::egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let frame = eframe::egui::Frame::popup(ui.style());
                frame.show(ui, |ui| {
                    ui.set_min_width(180.0);
                    let current_url = self.shell_state.active_tab.current_url.clone();
                    if ui.button("Copy URL").clicked() {
                        ctx.copy_text(current_url.clone());
                        self.shell_state.record_event("context menu: copy url");
                        close_menu = true;
                    }
                    if ui.button("Open In New Tab").clicked() {
                        {
                            let mut session = self.shell_state.session.write().unwrap();
                            session.open_new_tab(&current_url, "New Tab");
                            session.active_tab_mut().zoom_level = self.config.engine.zoom_default;
                        }
                        self.shell_state.active_tab_zoom = self.config.engine.zoom_default;
                        self.shell_state.address_bar_input = current_url.clone();
                        let _ = dispatch_command(
                            &mut self.shell_state,
                            self.engine.as_mut(),
                            AppCommand::NavigateTo(current_url),
                        );
                        self.shell_state
                            .record_event("context menu: open in new tab");
                        close_menu = true;
                    }
                    if ui.button("Reload").clicked() {
                        let _ = dispatch_command(
                            &mut self.shell_state,
                            self.engine.as_mut(),
                            AppCommand::ReloadActiveTab,
                        );
                        close_menu = true;
                    }
                    if ui.button("Save Snapshot").clicked() {
                        self.save_snapshot_to_disk();
                        close_menu = true;
                    }
                    ui.separator();
                    if ui.button("Zoom In").clicked() {
                        self.apply_zoom_steps(1, "context menu");
                        close_menu = true;
                    }
                    if ui.button("Zoom Out").clicked() {
                        self.apply_zoom_steps(-1, "context menu");
                        close_menu = true;
                    }
                    if ui.button("Reset Zoom").clicked() {
                        self.set_active_tab_zoom(self.config.engine.zoom_default, "reset");
                        close_menu = true;
                    }
                });
            });

        if ctx.input(|input| input.pointer.any_pressed())
            && let Some(pos) = ctx.input(|input| input.pointer.latest_pos())
            && !response.response.rect.contains(pos)
        {
            close_menu = true;
        }

        if close_menu {
            self.shell_state.pending_context_menu = None;
        }
    }
}
