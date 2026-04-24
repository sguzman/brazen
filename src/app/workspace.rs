use super::*;

impl BrazenApp {

    pub(super) fn save_workspace_layout(&self) {
        let Some(db) = &self.profile_db else {
            return;
        };
        let payload = WorkspaceLayout {
            panels: self.panels,
            theme: self.ui_theme,
            density: self.ui_density,
        };
        if let Ok(data) = serde_json::to_string_pretty(&payload) {
            let _ = db.save_workspace_layout(&data);
        }
    }

    pub(super) fn apply_layout_preset(&mut self, preset: LayoutPreset) {
        self.panels = match preset {
            LayoutPreset::Default => WorkspacePanels {
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
            },
            LayoutPreset::Developer => WorkspacePanels {
                sidebar_visible: true,
                bookmarks: false,
                history: false,
                reading_queue: false,
                reader_mode: false,
                tts_controls: false,
                workspace_settings: false,
                terminal: true,
                dashboard: false,
                find_panel_open: false,
                bottom_panel_visible: true,
                active_diagnostic_tab: DiagnosticTab::Network,
                bottom_panel_height: 300.0,
            },
            LayoutPreset::Archive => WorkspacePanels {
                sidebar_visible: true,
                terminal: false,
                dashboard: false,
                reading_queue: true,
                reader_mode: true,
                tts_controls: true,
                bookmarks: true,
                history: true,
                workspace_settings: true,
                find_panel_open: false,
                bottom_panel_visible: true,
                active_diagnostic_tab: DiagnosticTab::KnowledgeGraph,
                bottom_panel_height: 350.0,
            },
        };
        self.shell_state
            .record_event(format!("layout preset applied: {preset:?}"));
        self.save_workspace_layout();
    }

    pub(super) fn palette_entries() -> &'static [PaletteEntry] {
        &[
            PaletteEntry {
                label: "New Tab",
                action: PaletteCommand::NewTab,
            },
            PaletteEntry {
                label: "Close Tab",
                action: PaletteCommand::CloseTab,
            },
            PaletteEntry {
                label: "Reload",
                action: PaletteCommand::Reload,
            },
            PaletteEntry {
                label: "Stop Loading",
                action: PaletteCommand::StopLoading,
            },
            PaletteEntry {
                label: "Go Back",
                action: PaletteCommand::GoBack,
            },
            PaletteEntry {
                label: "Go Forward",
                action: PaletteCommand::GoForward,
            },
            PaletteEntry {
                label: "Focus Address Bar",
                action: PaletteCommand::FocusAddressBar,
            },
            PaletteEntry {
                label: "Toggle Logs Panel",
                action: PaletteCommand::ToggleLogs,
            },
            PaletteEntry {
                label: "Toggle Permissions Panel",
                action: PaletteCommand::TogglePermissions,
            },
        ]
    }

    pub(super) fn open_command_palette(&mut self) {
        self.command_palette_open = true;
        self.command_palette_focus_pending = true;
        self.command_palette_query.clear();
    }

    pub(super) fn apply_palette_command(&mut self, action: PaletteCommand) {
        match action {
            PaletteCommand::NewTab => {
                {
                    let mut session = self.shell_state.session.write().unwrap();
                    session.open_new_tab("about:blank", "New Tab");
                    session.active_tab_mut().zoom_level = self.config.engine.zoom_default;
                }
                self.shell_state.active_tab_zoom = self.config.engine.zoom_default;
                self.sync_active_tab_from_session();
                self.shell_state.address_bar_input =
                    self.shell_state.active_tab.current_url.clone();
                self.shell_state.record_event("palette: new tab");
            }
            PaletteCommand::CloseTab => {
                self.shell_state.session.write().unwrap().close_active_tab();
                self.sync_active_tab_from_session();
                self.shell_state.address_bar_input =
                    self.shell_state.active_tab.current_url.clone();
                self.shell_state.record_event("palette: close tab");
            }
            PaletteCommand::Reload => {
                let _ = dispatch_command(
                    &mut self.shell_state,
                    self.engine.as_mut(),
                    AppCommand::ReloadActiveTab,
                );
                self.shell_state.record_event("palette: reload");
            }
            PaletteCommand::StopLoading => {
                let _ = dispatch_command(
                    &mut self.shell_state,
                    self.engine.as_mut(),
                    AppCommand::StopLoading,
                );
                self.shell_state.record_event("palette: stop loading");
            }
            PaletteCommand::GoBack => {
                let _ = dispatch_command(
                    &mut self.shell_state,
                    self.engine.as_mut(),
                    AppCommand::GoBack,
                );
                self.shell_state.session.write().unwrap().go_back(Utc::now().to_rfc3339());
                self.shell_state.record_event("palette: go back");
            }
            PaletteCommand::GoForward => {
                let _ = dispatch_command(
                    &mut self.shell_state,
                    self.engine.as_mut(),
                    AppCommand::GoForward,
                );
                self.shell_state.session.write().unwrap().go_forward(Utc::now().to_rfc3339());
                self.shell_state.record_event("palette: go forward");
            }
            PaletteCommand::FocusAddressBar => {
                self.address_bar_focus_pending = true;
                self.shell_state.record_event("palette: focus address bar");
            }
            PaletteCommand::ToggleLogs => {
                let _ = dispatch_command(
                    &mut self.shell_state,
                    self.engine.as_mut(),
                    AppCommand::ToggleLogPanel,
                );
            }
            PaletteCommand::TogglePermissions => {
                self.shell_state.permission_panel_open = !self.shell_state.permission_panel_open;
                self.shell_state.record_event(format!(
                    "permission panel {}",
                    if self.shell_state.permission_panel_open {
                        "opened"
                    } else {
                        "closed"
                    }
                ));
            }
        }
    }

    pub(super) fn apply_ui_settings(&self, ctx: &eframe::egui::Context) {
        match self.ui_theme {
            UiTheme::System => {}
            UiTheme::Light => ctx.set_visuals(eframe::egui::Visuals::light()),
            UiTheme::Dark => ctx.set_visuals(eframe::egui::Visuals::dark()),
            UiTheme::Brazen => crate::ui_theme::apply_brazen_style(ctx),
        }
        let mut style = (*ctx.style()).clone();
        match self.ui_density {
            UiDensity::Compact => {
                style.spacing.item_spacing = eframe::egui::vec2(6.0, 4.0);
                style.spacing.button_padding = eframe::egui::vec2(6.0, 4.0);
            }
            UiDensity::Comfortable => {
                style.spacing.item_spacing = eframe::egui::vec2(10.0, 8.0);
                style.spacing.button_padding = eframe::egui::vec2(10.0, 6.0);
            }
        }
        ctx.set_style(style);
    }
}
