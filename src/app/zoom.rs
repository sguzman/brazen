use super::*;

impl BrazenApp {
    pub(super) fn clamp_zoom(&self, zoom: f32) -> f32 {
        zoom.clamp(self.config.engine.zoom_min, self.config.engine.zoom_max)
    }

    pub(super) fn set_active_tab_zoom(&mut self, zoom: f32, reason: &str) {
        let clamped = self.clamp_zoom(zoom);
        {
            let mut session = self.shell_state.session.write().unwrap();
            session.active_tab_mut().zoom_level = clamped;
        }
        self.shell_state.active_tab_zoom = clamped;
        self.engine.set_page_zoom(clamped);
        self.shell_state
            .record_event(format!("zoom {reason}: {clamped:.2}x"));
    }

    pub(super) fn apply_zoom_steps(&mut self, steps: i32, reason: &str) {
        let current = self.shell_state.active_tab_zoom;
        let step = self.config.engine.zoom_step;
        let next = current + step * steps as f32;
        self.set_active_tab_zoom(next, reason);
    }

    pub(super) fn apply_zoom_factor(&mut self, factor: f32, reason: &str) {
        let current = self.shell_state.active_tab_zoom;
        self.set_active_tab_zoom(current * factor, reason);
    }

    pub(super) fn update_click_count(
        &mut self,
        button: eframe::egui::PointerButton,
        pos: eframe::egui::Pos2,
    ) -> u8 {
        let now = Instant::now();
        let within_time = self
            .last_click_at
            .map(|at| now.duration_since(at) <= Duration::from_millis(500))
            .unwrap_or(false);
        let within_distance = self
            .last_click_pos
            .map(|last| last.distance(pos) <= 4.0)
            .unwrap_or(false);
        if within_time && within_distance && self.last_click_button == Some(button) {
            self.click_count = (self.click_count + 1).min(3);
        } else {
            self.click_count = 1;
        }
        self.last_click_at = Some(now);
        self.last_click_pos = Some(pos);
        self.last_click_button = Some(button);
        self.click_count
    }
}
