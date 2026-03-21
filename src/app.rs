use crate::commands::{AppCommand, dispatch_command};
use crate::config::BrazenConfig;
use crate::engine::{
    BrowserEngine, BrowserTab, EngineEvent, EngineFactory, EngineStatus, FocusState, InputEvent,
    RenderSurfaceHandle, RenderSurfaceMetadata,
};
use crate::permissions::Capability;
use crate::platform_paths::RuntimePaths;

#[derive(Debug, Clone)]
pub struct ShellState {
    pub app_name: String,
    pub backend_name: String,
    pub engine_instance_id: u64,
    pub engine_status: EngineStatus,
    pub active_tab: BrowserTab,
    pub address_bar_input: String,
    pub page_title: String,
    pub load_progress: f32,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub document_ready: bool,
    pub favicon_url: Option<String>,
    pub metadata_summary: Option<String>,
    pub history: Vec<String>,
    pub last_committed_url: Option<String>,
    pub was_minimized: bool,
    pub event_log: Vec<String>,
    pub log_panel_open: bool,
    pub permission_panel_open: bool,
    pub capabilities_snapshot: Vec<(String, String)>,
    pub runtime_paths: RuntimePaths,
}

impl ShellState {
    pub fn record_event(&mut self, event: impl Into<String>) {
        self.event_log.push(event.into());
    }

    pub fn sync_from_engine(&mut self, engine: &mut dyn BrowserEngine) {
        self.engine_instance_id = engine.instance_id();
        self.engine_status = engine.status();
        self.active_tab = engine.active_tab().clone();
        for event in engine.take_events() {
            match event {
                EngineEvent::NavigationStateUpdated(state) => {
                    self.page_title = state.title.clone();
                    self.load_progress = state.load_progress;
                    self.can_go_back = state.can_go_back;
                    self.can_go_forward = state.can_go_forward;
                    self.document_ready = state.document_ready;
                    self.favicon_url = state.favicon_url.clone();
                    self.metadata_summary = state.metadata_summary.clone();
                    if self
                        .last_committed_url
                        .as_ref()
                        .map(|value| value != &state.url)
                        .unwrap_or(true)
                    {
                        self.history.push(state.url.clone());
                        self.last_committed_url = Some(state.url.clone());
                    }
                    self.record_event(format!(
                        "nav: {} ({:.0}%)",
                        state.url,
                        state.load_progress * 100.0
                    ));
                }
                EngineEvent::ClipboardRequested(request) => {
                    self.record_event(format!("clipboard request: {request:?}"));
                }
                other => {
                    self.record_event(format!("engine event: {other:?}"));
                }
            }
        }
    }
}

pub fn build_shell_state(
    config: &BrazenConfig,
    paths: &RuntimePaths,
    engine_factory: &dyn EngineFactory,
) -> ShellState {
    let mut engine = engine_factory.create(config, paths);
    engine.set_render_surface(RenderSurfaceMetadata {
        viewport_width: config.window.initial_width as u32,
        viewport_height: config.window.initial_height as u32,
        scale_factor_basis_points: 100,
    });

    let capabilities_snapshot = vec![
        (
            Capability::TerminalExec.label().to_string(),
            format!(
                "{:?}",
                config.permissions.decision_for(&Capability::TerminalExec)
            ),
        ),
        (
            Capability::DomRead.label().to_string(),
            format!(
                "{:?}",
                config.permissions.decision_for(&Capability::DomRead)
            ),
        ),
        (
            Capability::CacheRead.label().to_string(),
            format!(
                "{:?}",
                config.permissions.decision_for(&Capability::CacheRead)
            ),
        ),
        (
            Capability::TabInspect.label().to_string(),
            format!(
                "{:?}",
                config.permissions.decision_for(&Capability::TabInspect)
            ),
        ),
        (
            Capability::AiToolUse.label().to_string(),
            format!(
                "{:?}",
                config.permissions.decision_for(&Capability::AiToolUse)
            ),
        ),
    ];

    let mut shell_state = ShellState {
        app_name: config.app.name.clone(),
        backend_name: engine.backend_name().to_string(),
        engine_instance_id: engine.instance_id(),
        engine_status: engine.status(),
        active_tab: engine.active_tab().clone(),
        address_bar_input: config.app.homepage.clone(),
        page_title: engine.active_tab().title.clone(),
        load_progress: 0.0,
        can_go_back: false,
        can_go_forward: false,
        document_ready: false,
        favicon_url: None,
        metadata_summary: None,
        history: Vec::new(),
        last_committed_url: None,
        was_minimized: false,
        event_log: vec![
            format!("loaded config for {}", config.app.name),
            format!("data dir: {}", paths.data_dir.display()),
            format!("logs dir: {}", paths.logs_dir.display()),
        ],
        log_panel_open: config.window.show_log_panel_on_startup,
        permission_panel_open: config.window.show_permission_panel_on_startup,
        capabilities_snapshot,
        runtime_paths: paths.clone(),
    };

    shell_state.sync_from_engine(engine.as_mut());
    shell_state
}

pub struct BrazenApp {
    config: BrazenConfig,
    shell_state: ShellState,
    engine: Box<dyn BrowserEngine>,
    surface_handle: RenderSurfaceHandle,
    last_surface: Option<RenderSurfaceMetadata>,
}

impl BrazenApp {
    pub fn new(config: BrazenConfig, shell_state: ShellState) -> Self {
        let factory = crate::engine::ServoEngineFactory;
        let engine = factory.create(&config, &shell_state.runtime_paths);
        let surface_handle = RenderSurfaceHandle {
            id: 1,
            label: "primary-surface".to_string(),
        };

        Self {
            config,
            shell_state,
            engine,
            surface_handle,
            last_surface: None,
        }
    }

    pub fn shell_state(&self) -> &ShellState {
        &self.shell_state
    }

    fn handle_navigation(&mut self) {
        let input = self.shell_state.address_bar_input.trim().to_string();
        let _ = dispatch_command(
            &mut self.shell_state,
            self.engine.as_mut(),
            AppCommand::NavigateTo(input),
        );
        self.shell_state.sync_from_engine(self.engine.as_mut());
    }

    fn update_render_surface(&mut self, ctx: &eframe::egui::Context) {
        let screen_rect = ctx.content_rect();
        let pixels_per_point = ctx.pixels_per_point();
        let metadata = RenderSurfaceMetadata {
            viewport_width: (screen_rect.width() * pixels_per_point) as u32,
            viewport_height: (screen_rect.height() * pixels_per_point) as u32,
            scale_factor_basis_points: (pixels_per_point * 100.0) as u32,
        };

        if self.last_surface.as_ref() != Some(&metadata) {
            self.engine.attach_surface(self.surface_handle.clone());
            self.engine.set_render_surface(metadata.clone());
            self.last_surface = Some(metadata);
        }
    }

    fn forward_input_events(&mut self, ctx: &eframe::egui::Context) {
        let input = ctx.input(|input| input.clone());
        let focused = if input.raw.focused {
            FocusState::Focused
        } else {
            FocusState::Unfocused
        };
        self.engine.set_focus(focused);

        if let Some(minimized) = input.raw.viewport().minimized {
            if minimized && !self.shell_state.was_minimized {
                self.engine.suspend();
                self.shell_state.was_minimized = true;
            } else if !minimized && self.shell_state.was_minimized {
                self.engine.resume();
                self.shell_state.was_minimized = false;
            }
        }

        for event in input.raw.events {
            match event {
                eframe::egui::Event::PointerMoved(pos) => {
                    self.engine
                        .handle_input(InputEvent::PointerMove { x: pos.x, y: pos.y });
                }
                eframe::egui::Event::PointerButton {
                    button, pressed, ..
                } => {
                    let button_id = match button {
                        eframe::egui::PointerButton::Primary => 0,
                        eframe::egui::PointerButton::Secondary => 1,
                        eframe::egui::PointerButton::Middle => 2,
                        eframe::egui::PointerButton::Extra1 => 3,
                        eframe::egui::PointerButton::Extra2 => 4,
                    };
                    if pressed {
                        self.engine
                            .handle_input(InputEvent::PointerDown { button: button_id });
                    } else {
                        self.engine
                            .handle_input(InputEvent::PointerUp { button: button_id });
                    }
                }
                eframe::egui::Event::MouseWheel { delta, .. } => {
                    self.engine.handle_input(InputEvent::Scroll {
                        delta_x: delta.x,
                        delta_y: delta.y,
                    });
                }
                eframe::egui::Event::Key { key, pressed, .. } => {
                    let key_name = format!("{key:?}");
                    if pressed {
                        self.engine
                            .handle_input(InputEvent::KeyDown { key: key_name });
                    } else {
                        self.engine
                            .handle_input(InputEvent::KeyUp { key: key_name });
                    }
                }
                eframe::egui::Event::Text(text) => {
                    self.engine.handle_input(InputEvent::TextInput { text });
                }
                eframe::egui::Event::Ime(ime) => match ime {
                    eframe::egui::ImeEvent::Enabled => {
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionStart);
                    }
                    eframe::egui::ImeEvent::Preedit(text) => {
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionUpdate { text });
                    }
                    eframe::egui::ImeEvent::Commit(text) => {
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionEnd { text });
                    }
                    eframe::egui::ImeEvent::Disabled => {
                        self.engine
                            .handle_ime(crate::engine::ImeEvent::CompositionEnd {
                                text: String::new(),
                            });
                    }
                },
                eframe::egui::Event::Copy | eframe::egui::Event::Cut => {
                    self.engine
                        .handle_clipboard(crate::engine::ClipboardRequest::Read);
                }
                eframe::egui::Event::Paste(text) => {
                    self.engine
                        .handle_clipboard(crate::engine::ClipboardRequest::Write(text));
                }
                _ => {}
            }
        }
    }
}

impl eframe::App for BrazenApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.update_render_surface(ctx);
        self.forward_input_events(ctx);
        self.shell_state.sync_from_engine(self.engine.as_mut());

        eframe::egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&self.config.app.name);
                ui.label(format!("backend: {}", self.shell_state.backend_name));
                ui.label(format!("engine: {}", self.shell_state.engine_instance_id));
                ui.separator();
                ui.label(format!("status: {}", self.shell_state.engine_status));
                ui.separator();
                ui.label(format!("title: {}", self.shell_state.page_title));
                ui.separator();
                ui.label(format!(
                    "ready: {}",
                    if self.shell_state.document_ready {
                        "yes"
                    } else {
                        "no"
                    }
                ));
            });

            ui.horizontal(|ui| {
                ui.add_enabled_ui(self.shell_state.can_go_back, |ui| {
                    if ui.button("Back").clicked() {
                        let _ = dispatch_command(
                            &mut self.shell_state,
                            self.engine.as_mut(),
                            AppCommand::GoBack,
                        );
                    }
                });
                ui.add_enabled_ui(self.shell_state.can_go_forward, |ui| {
                    if ui.button("Forward").clicked() {
                        let _ = dispatch_command(
                            &mut self.shell_state,
                            self.engine.as_mut(),
                            AppCommand::GoForward,
                        );
                    }
                });
                let response = ui.text_edit_singleline(&mut self.shell_state.address_bar_input);
                let enter_pressed = ui.input(|input| input.key_pressed(eframe::egui::Key::Enter));
                if response.lost_focus() && enter_pressed {
                    self.handle_navigation();
                }
                if ui.button("Go").clicked() {
                    self.handle_navigation();
                }
                if ui.button("Reload").clicked() {
                    let _ = dispatch_command(
                        &mut self.shell_state,
                        self.engine.as_mut(),
                        AppCommand::ReloadActiveTab,
                    );
                }
                if ui.button("Logs").clicked() {
                    let _ = dispatch_command(
                        &mut self.shell_state,
                        self.engine.as_mut(),
                        AppCommand::ToggleLogPanel,
                    );
                }
                if ui.button("Permissions").clicked() {
                    let _ = dispatch_command(
                        &mut self.shell_state,
                        self.engine.as_mut(),
                        AppCommand::OpenPermissionPanel,
                    );
                }
            });
            ui.add(
                eframe::egui::ProgressBar::new(self.shell_state.load_progress)
                    .show_percentage()
                    .desired_width(f32::INFINITY),
            );
        });

        eframe::egui::SidePanel::left("tab_sidebar")
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.heading("Workspace");
                ui.label("Tab 1");
                ui.separator();
                ui.label(format!("Title: {}", self.shell_state.active_tab.title));
                ui.label(format!("URL: {}", self.shell_state.active_tab.current_url));
                ui.label(format!("History: {}", self.shell_state.history.len()));
                if let Some(favicon) = &self.shell_state.favicon_url {
                    ui.label(format!("Favicon: {favicon}"));
                }
                ui.label(format!(
                    "Profiles: {}",
                    self.shell_state.runtime_paths.profiles_dir.display()
                ));
                ui.label(format!(
                    "Cache: {}",
                    self.shell_state.runtime_paths.cache_dir.display()
                ));
            });

        if self.shell_state.permission_panel_open {
            eframe::egui::SidePanel::right("permissions")
                .default_width(260.0)
                .show(ctx, |ui| {
                    ui.heading("Capability Grants");
                    for (capability, decision) in &self.shell_state.capabilities_snapshot {
                        ui.horizontal(|ui| {
                            ui.monospace(capability);
                            ui.label(decision);
                        });
                    }
                });
        }

        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Browser Backend View");
            ui.separator();
            ui.label(format!("Engine state: {}", self.shell_state.engine_status));
            ui.label("This viewport is reserved for Servo-backed rendering surfaces.");
            ui.add_space(12.0);
            ui.group(|ui| {
                ui.label("Current target");
                ui.monospace(&self.shell_state.active_tab.current_url);
            });
            ui.add_space(12.0);
            ui.group(|ui| {
                ui.label("Future dimensions");
                ui.label("Permissions, automation, cache introspection, article workflows, and local-tool routing hang off this shell.");
            });
        });

        if self.shell_state.log_panel_open {
            eframe::egui::TopBottomPanel::bottom("log_panel")
                .resizable(true)
                .default_height(180.0)
                .show(ctx, |ui| {
                    ui.heading("Startup and Command Log");
                    eframe::egui::ScrollArea::vertical().show(ui, |ui| {
                        for event in self.shell_state.event_log.iter().rev().take(128) {
                            ui.monospace(event);
                        }
                    });
                });
        }
    }
}

impl Drop for BrazenApp {
    fn drop(&mut self) {
        self.engine.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{BrowserEngine, BrowserTab, EngineEvent, EngineStatus};

    struct MockEngine {
        status: EngineStatus,
        tab: BrowserTab,
        events: Vec<EngineEvent>,
    }

    impl BrowserEngine for MockEngine {
        fn backend_name(&self) -> &'static str {
            "mock"
        }

        fn instance_id(&self) -> crate::engine::EngineInstanceId {
            1
        }

        fn status(&self) -> EngineStatus {
            self.status.clone()
        }

        fn active_tab(&self) -> &BrowserTab {
            &self.tab
        }

        fn navigate(&mut self, url: &str) {
            self.tab.current_url = url.to_string();
            self.events
                .push(EngineEvent::NavigationRequested(url.to_string()));
        }

        fn reload(&mut self) {}

        fn go_back(&mut self) {}

        fn go_forward(&mut self) {}

        fn attach_surface(&mut self, _surface: crate::engine::RenderSurfaceHandle) {}

        fn set_render_surface(&mut self, _metadata: RenderSurfaceMetadata) {}

        fn set_focus(&mut self, _focus: crate::engine::FocusState) {}

        fn handle_input(&mut self, _event: crate::engine::InputEvent) {}

        fn handle_ime(&mut self, _event: crate::engine::ImeEvent) {}

        fn handle_clipboard(&mut self, _request: crate::engine::ClipboardRequest) {}

        fn suspend(&mut self) {}

        fn resume(&mut self) {}

        fn shutdown(&mut self) {}

        fn take_events(&mut self) -> Vec<EngineEvent> {
            std::mem::take(&mut self.events)
        }
    }

    #[test]
    fn shell_state_sync_handles_ready_and_error_statuses() {
        let paths = RuntimePaths {
            config_path: "brazen.toml".into(),
            data_dir: "data".into(),
            logs_dir: "logs".into(),
            profiles_dir: "profiles".into(),
            cache_dir: "cache".into(),
        };
        let mut shell = ShellState {
            app_name: "Brazen".to_string(),
            backend_name: "mock".to_string(),
            engine_instance_id: 1,
            engine_status: EngineStatus::Initializing,
            active_tab: BrowserTab {
                id: 1,
                title: "Loading".to_string(),
                current_url: "about:blank".to_string(),
            },
            address_bar_input: "https://example.com".to_string(),
            page_title: "Loading".to_string(),
            load_progress: 0.0,
            can_go_back: false,
            can_go_forward: false,
            document_ready: false,
            favicon_url: None,
            metadata_summary: None,
            history: Vec::new(),
            last_committed_url: None,
            was_minimized: false,
            event_log: Vec::new(),
            log_panel_open: true,
            permission_panel_open: false,
            capabilities_snapshot: Vec::new(),
            runtime_paths: paths,
        };

        let mut ready_engine = MockEngine {
            status: EngineStatus::Ready,
            tab: BrowserTab {
                id: 1,
                title: "Example".to_string(),
                current_url: "https://example.com".to_string(),
            },
            events: vec![EngineEvent::StatusChanged(EngineStatus::Ready)],
        };
        shell.sync_from_engine(&mut ready_engine);
        assert_eq!(shell.engine_status, EngineStatus::Ready);
        assert!(
            shell
                .event_log
                .iter()
                .any(|line| line.contains("StatusChanged"))
        );

        let mut failing_engine = MockEngine {
            status: EngineStatus::Error("boot failed".to_string()),
            tab: shell.active_tab.clone(),
            events: vec![EngineEvent::StatusChanged(EngineStatus::Error(
                "boot failed".to_string(),
            ))],
        };
        shell.sync_from_engine(&mut failing_engine);
        assert_eq!(
            shell.engine_status,
            EngineStatus::Error("boot failed".to_string())
        );
    }
}
