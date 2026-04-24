use super::state::*;
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
}
