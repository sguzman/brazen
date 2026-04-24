use super::*;

impl BrazenApp {
    pub(super) fn handle_navigation(&mut self) {
        let input = self.shell_state.address_bar_input.trim().to_string();
        self.shell_state
            .session
            .write()
            .unwrap()
            .mark_pending_navigation(&input, Utc::now().to_rfc3339());
        let _ = dispatch_command(
            &mut self.shell_state,
            self.engine.as_mut(),
            AppCommand::NavigateTo(input),
        );
        self.shell_state.sync_from_engine(self.engine.as_mut());
    }

    pub(super) fn map_pointer_to_viewport(
        &self,
        ctx: &eframe::egui::Context,
        pos: eframe::egui::Pos2,
        allow_outside: bool,
    ) -> Option<eframe::egui::Pos2> {
        let rect = self.render_viewport_rect?;
        if !allow_outside && !rect.contains(pos) {
            return None;
        }
        let mut local = pos - rect.min;
        if let Some(surface) = &self.last_surface {
            let pixels_per_point = ctx.pixels_per_point();
            let max_x = surface.viewport_width as f32 / pixels_per_point;
            let max_y = surface.viewport_height as f32 / pixels_per_point;
            local.x = local.x.clamp(0.0, max_x);
            local.y = local.y.clamp(0.0, max_y);
        }
        Some(eframe::egui::pos2(local.x, local.y))
    }

    pub(super) fn sync_active_tab_from_session(&mut self) {
        let tab = self.shell_state.session.read().unwrap().active_tab().unwrap().clone();
        if self.shell_state.active_tab.current_url != tab.url
            || self.shell_state.active_tab.title != tab.title
        {
            self.shell_state.active_tab.title = tab.title;
            self.shell_state.active_tab.current_url = tab.url;
        }
        if (self.shell_state.active_tab_zoom - tab.zoom_level).abs() > f32::EPSILON {
            self.shell_state.active_tab_zoom = tab.zoom_level;
            self.engine.set_page_zoom(tab.zoom_level);
        }
    }

    pub(super) fn apply_new_window_policy(&mut self) {
        let Some((url, disposition)) = self.shell_state.pending_new_window.take() else {
            return;
        };
        let policy = self.config.engine.new_window_policy.as_str();
        let decision = match policy {
            "new-tab" => WindowDisposition::BackgroundTab,
            "same-tab" => WindowDisposition::ForegroundTab,
            "block" => WindowDisposition::Blocked,
            _ => disposition.clone(),
        };

        match decision {
            WindowDisposition::ForegroundTab => {
                if let Ok(normalized) = normalize_url_input(&url) {
                    self.engine.navigate(&normalized);
                    self.shell_state
                        .record_event(format!("new window routed to current tab: {normalized}"));
                } else {
                    self.shell_state
                        .record_event(format!("new window navigation failed: {url}"));
                }
            }
            WindowDisposition::BackgroundTab | WindowDisposition::NewWindow => {
                if let Ok(normalized) = normalize_url_input(&url) {
                    {
                        let mut session = self.shell_state.session.write().unwrap();
                        session.open_new_tab(&normalized, "New Tab");
                        session.active_tab_mut().zoom_level = self.config.engine.zoom_default;
                    }
                    self.shell_state.active_tab_zoom = self.config.engine.zoom_default;
                    self.shell_state
                        .record_event(format!("new window opened as tab: {normalized}"));
                } else {
                    self.shell_state
                        .record_event(format!("new window tab open failed: {url}"));
                }
            }
            WindowDisposition::Blocked => {
                self.shell_state
                    .record_event(format!("new window blocked: {url}"));
            }
        }
    }

    pub(super) fn apply_cursor_icon(&self, ctx: &eframe::egui::Context) {
        let Some(cursor) = self.shell_state.cursor_icon.as_deref() else {
            return;
        };
        let inside = self
            .last_pointer_pos
            .and_then(|pos| self.render_viewport_rect.map(|rect| rect.contains(pos)))
            .unwrap_or(false);
        if !inside {
            return;
        }
        let icon = match cursor {
            "Pointer" => eframe::egui::CursorIcon::PointingHand,
            "Text" | "VerticalText" => eframe::egui::CursorIcon::Text,
            "Crosshair" => eframe::egui::CursorIcon::Crosshair,
            "Move" | "AllScroll" => eframe::egui::CursorIcon::Move,
            "Grab" => eframe::egui::CursorIcon::Grab,
            "Grabbing" => eframe::egui::CursorIcon::Grabbing,
            "NotAllowed" | "NoDrop" => eframe::egui::CursorIcon::NotAllowed,
            "EResize" | "EwResize" => eframe::egui::CursorIcon::ResizeHorizontal,
            "NResize" | "SResize" | "NsResize" => eframe::egui::CursorIcon::ResizeVertical,
            "NeResize" | "SwResize" | "NeswResize" => eframe::egui::CursorIcon::ResizeNeSw,
            "NwResize" | "SeResize" | "NwseResize" => eframe::egui::CursorIcon::ResizeNwSe,
            "RowResize" => eframe::egui::CursorIcon::ResizeRow,
            "ColResize" => eframe::egui::CursorIcon::ResizeColumn,
            "ZoomIn" => eframe::egui::CursorIcon::ZoomIn,
            "ZoomOut" => eframe::egui::CursorIcon::ZoomOut,
            "Wait" | "Progress" => eframe::egui::CursorIcon::Wait,
            _ => eframe::egui::CursorIcon::Default,
        };
        ctx.output_mut(|output| output.cursor_icon = icon);
    }
}
