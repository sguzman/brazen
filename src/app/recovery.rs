use super::*;

impl BrazenApp {
    pub(super) fn write_crash_dump(&mut self, reason: &str) {
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let filename = format!("crash-{timestamp}.log");
        let path = self
            .shell_state
            .runtime_paths
            .crash_dumps_dir
            .join(filename);
        let _ = std::fs::create_dir_all(&self.shell_state.runtime_paths.crash_dumps_dir);
        let payload = format!(
            "timestamp={}\nreason={}\nsession_id={}\nprofile={}\nactive_url={}\n",
            timestamp,
            reason,
            self.shell_state.session.read().unwrap().session_id.0,
            self.shell_state.session.read().unwrap().profile_id.clone(),
            self.shell_state.active_tab.current_url
        );
        let _ = std::fs::write(&path, payload.as_bytes());
        self.shell_state.last_crash_dump = Some(path.display().to_string());
    }

    pub(super) fn restart_engine(&mut self) {
        self.engine.shutdown();
        self.engine = self.engine_factory.create(
            &self.config,
            &self.shell_state.runtime_paths,
            self.shell_state.mount_manager.clone(),
            self.shell_state.session.clone(),
        );
        self.engine
            .set_verbose_logging(self.shell_state.engine_verbose_logging);
        self.engine.configure_devtools(
            self.config.engine.devtools_enabled,
            &self.config.engine.devtools_transport,
        );
        self.last_surface = None;
        self.render_texture = None;
        self.shell_state.record_event("engine restarted");
    }

    pub(super) fn schedule_restart(&mut self) {
        if self.pending_restart_at.is_some() {
            return;
        }
        self.crash_count = self.crash_count.saturating_add(1);
        let exponent = self.crash_count.min(5);
        let backoff = 2u64.pow(exponent);
        let delay = chrono::Duration::seconds(backoff as i64);
        let scheduled = Utc::now() + delay;
        self.pending_restart_at = Some(scheduled);
        self.shell_state
            .record_event(format!("engine restart scheduled in {backoff}s"));
    }

    pub(super) fn handle_crash_recovery(&mut self) {
        if self.shell_state.last_crash.is_some() {
            self.schedule_restart();
        }
        if let Some(scheduled) = self.pending_restart_at
            && Utc::now() >= scheduled
        {
            self.restart_engine();
            self.shell_state.last_crash = None;
            self.shell_state.session.write().unwrap().crash_recovery_pending = false;
            self.pending_restart_at = None;
        }
    }
}
