use std::fmt;

use crate::config::BrazenConfig;
use crate::platform_paths::RuntimePaths;
#[cfg(feature = "servo")]
use crate::servo_embedder::{ServoEmbedder, ServoEmbedderConfig};

pub type EngineInstanceId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusState {
    Focused,
    Unfocused,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    PointerMove { x: f32, y: f32 },
    PointerDown { button: u8 },
    PointerUp { button: u8 },
    Scroll { delta_x: f32, delta_y: f32 },
    KeyDown { key: String },
    KeyUp { key: String },
    TextInput { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImeEvent {
    CompositionStart,
    CompositionUpdate { text: String },
    CompositionEnd { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardRequest {
    Read,
    Write(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSurfaceHandle {
    pub id: u64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    NoEngine,
    Initializing,
    Ready,
    Error(String),
}

impl fmt::Display for EngineStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEngine => write!(f, "No engine"),
            Self::Initializing => write!(f, "Initializing"),
            Self::Ready => write!(f, "Ready"),
            Self::Error(message) => write!(f, "Error: {message}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSurfaceMetadata {
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub scale_factor_basis_points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSurface {
    pub handle: RenderSurfaceHandle,
    pub metadata: RenderSurfaceMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NavigationState {
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub load_progress: f32,
    pub document_ready: bool,
    pub title: String,
    pub url: String,
    pub favicon_url: Option<String>,
    pub metadata_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    StatusChanged(EngineStatus),
    NavigationRequested(String),
    NavigationStateUpdated(NavigationState),
    ClipboardRequested(ClipboardRequest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserTab {
    pub id: u64,
    pub title: String,
    pub current_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    Unsupported(&'static str),
    Startup(String),
}

pub trait BrowserEngine: Send {
    fn backend_name(&self) -> &'static str;
    fn instance_id(&self) -> EngineInstanceId;
    fn status(&self) -> EngineStatus;
    fn active_tab(&self) -> &BrowserTab;
    fn navigate(&mut self, url: &str);
    fn reload(&mut self);
    fn go_back(&mut self);
    fn go_forward(&mut self);
    fn attach_surface(&mut self, surface: RenderSurfaceHandle);
    fn set_render_surface(&mut self, metadata: RenderSurfaceMetadata);
    fn set_focus(&mut self, focus: FocusState);
    fn handle_input(&mut self, event: InputEvent);
    fn handle_ime(&mut self, event: ImeEvent);
    fn handle_clipboard(&mut self, request: ClipboardRequest);
    fn suspend(&mut self);
    fn resume(&mut self);
    fn shutdown(&mut self);
    fn take_events(&mut self) -> Vec<EngineEvent>;
}

pub trait EngineFactory {
    fn create(&self, config: &BrazenConfig, paths: &RuntimePaths) -> Box<dyn BrowserEngine>;
}

pub struct NullEngine {
    instance_id: EngineInstanceId,
    status: EngineStatus,
    active_tab: BrowserTab,
    events: Vec<EngineEvent>,
    surface: Option<RenderSurface>,
    navigation_state: NavigationState,
    focus: FocusState,
}

impl NullEngine {
    pub fn new() -> Self {
        let navigation_state = NavigationState {
            can_go_back: false,
            can_go_forward: false,
            load_progress: 0.0,
            document_ready: false,
            title: "Platform Skeleton".to_string(),
            url: "about:blank".to_string(),
            favicon_url: None,
            metadata_summary: None,
        };
        Self {
            instance_id: 1,
            status: EngineStatus::NoEngine,
            active_tab: BrowserTab {
                id: 1,
                title: "Platform Skeleton".to_string(),
                current_url: "about:blank".to_string(),
            },
            events: Vec::new(),
            surface: None,
            navigation_state,
            focus: FocusState::Unfocused,
        }
    }
}

impl Default for NullEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserEngine for NullEngine {
    fn backend_name(&self) -> &'static str {
        "null"
    }

    fn instance_id(&self) -> EngineInstanceId {
        self.instance_id
    }

    fn status(&self) -> EngineStatus {
        self.status.clone()
    }

    fn active_tab(&self) -> &BrowserTab {
        &self.active_tab
    }

    fn navigate(&mut self, url: &str) {
        self.active_tab.current_url = url.to_string();
        self.navigation_state.url = url.to_string();
        self.navigation_state.title = "Loading...".to_string();
        self.navigation_state.load_progress = 0.1;
        self.navigation_state.document_ready = false;
        if self.active_tab.current_url != "about:blank" {
            self.navigation_state.can_go_back = true;
        }
        self.events
            .push(EngineEvent::NavigationRequested(url.to_string()));
        self.events.push(EngineEvent::NavigationStateUpdated(
            self.navigation_state.clone(),
        ));
    }

    fn reload(&mut self) {
        self.navigation_state.load_progress = 1.0;
        self.navigation_state.document_ready = true;
        self.events.push(EngineEvent::NavigationStateUpdated(
            self.navigation_state.clone(),
        ));
    }

    fn go_back(&mut self) {
        if self.navigation_state.can_go_back {
            self.navigation_state.can_go_forward = true;
            self.navigation_state.load_progress = 0.2;
            self.navigation_state.document_ready = false;
            self.events.push(EngineEvent::NavigationStateUpdated(
                self.navigation_state.clone(),
            ));
        }
    }

    fn go_forward(&mut self) {
        if self.navigation_state.can_go_forward {
            self.navigation_state.load_progress = 0.2;
            self.navigation_state.document_ready = false;
            self.events.push(EngineEvent::NavigationStateUpdated(
                self.navigation_state.clone(),
            ));
        }
    }

    fn attach_surface(&mut self, surface: RenderSurfaceHandle) {
        let metadata = self
            .surface
            .as_ref()
            .map(|surface| surface.metadata.clone())
            .unwrap_or(RenderSurfaceMetadata {
                viewport_width: 0,
                viewport_height: 0,
                scale_factor_basis_points: 100,
            });
        self.surface = Some(RenderSurface {
            handle: surface,
            metadata,
        });
    }

    fn set_render_surface(&mut self, metadata: RenderSurfaceMetadata) {
        if let Some(surface) = self.surface.as_mut() {
            surface.metadata = metadata;
        } else {
            self.surface = Some(RenderSurface {
                handle: RenderSurfaceHandle {
                    id: 0,
                    label: "unbound".to_string(),
                },
                metadata,
            });
        }
    }

    fn set_focus(&mut self, focus: FocusState) {
        self.focus = focus;
    }

    fn handle_input(&mut self, _event: InputEvent) {}

    fn handle_ime(&mut self, _event: ImeEvent) {}

    fn handle_clipboard(&mut self, request: ClipboardRequest) {
        self.events.push(EngineEvent::ClipboardRequested(request));
    }

    fn suspend(&mut self) {
        self.status = EngineStatus::Initializing;
        self.events.push(EngineEvent::StatusChanged(self.status()));
    }

    fn resume(&mut self) {
        self.status = EngineStatus::Ready;
        self.events.push(EngineEvent::StatusChanged(self.status()));
    }

    fn shutdown(&mut self) {
        self.status = EngineStatus::NoEngine;
        self.events.push(EngineEvent::StatusChanged(self.status()));
    }

    fn take_events(&mut self) -> Vec<EngineEvent> {
        std::mem::take(&mut self.events)
    }
}

pub struct ServoEngineFactory;

impl EngineFactory for ServoEngineFactory {
    fn create(&self, _config: &BrazenConfig, _paths: &RuntimePaths) -> Box<dyn BrowserEngine> {
        #[cfg(feature = "servo")]
        {
            Box::new(ServoEngine::new(_config))
        }

        #[cfg(not(feature = "servo"))]
        {
            Box::new(NullEngine::new())
        }
    }
}

#[cfg(feature = "servo")]
pub struct ServoEngine {
    instance_id: EngineInstanceId,
    status: EngineStatus,
    active_tab: BrowserTab,
    events: Vec<EngineEvent>,
    surface: Option<RenderSurfaceMetadata>,
    navigation_state: NavigationState,
    focus: FocusState,
    surface_handle: Option<RenderSurfaceHandle>,
    embedder: ServoEmbedder,
}

#[cfg(feature = "servo")]
impl ServoEngine {
    pub fn new(config: &BrazenConfig) -> Self {
        tracing::info!("servo feature enabled with scaffold backend");
        let events = vec![
            EngineEvent::StatusChanged(EngineStatus::Initializing),
            EngineEvent::StatusChanged(EngineStatus::Ready),
        ];
        let navigation_state = NavigationState {
            can_go_back: false,
            can_go_forward: false,
            load_progress: 0.0,
            document_ready: false,
            title: "Servo Scaffold".to_string(),
            url: "about:blank".to_string(),
            favicon_url: None,
            metadata_summary: None,
        };

        let embedder_config = ServoEmbedderConfig::from_engine_config(&config.engine);
        let embedder = ServoEmbedder::new(embedder_config);

        Self {
            instance_id: 1,
            status: EngineStatus::Ready,
            active_tab: BrowserTab {
                id: 1,
                title: "Servo Scaffold".to_string(),
                current_url: "about:blank".to_string(),
            },
            events,
            surface: None,
            navigation_state,
            focus: FocusState::Unfocused,
            surface_handle: None,
            embedder,
        }
    }
}

#[cfg(feature = "servo")]
impl Default for ServoEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "servo")]
impl BrowserEngine for ServoEngine {
    fn backend_name(&self) -> &'static str {
        "servo-scaffold"
    }

    fn instance_id(&self) -> EngineInstanceId {
        self.instance_id
    }

    fn status(&self) -> EngineStatus {
        self.status.clone()
    }

    fn active_tab(&self) -> &BrowserTab {
        &self.active_tab
    }

    fn navigate(&mut self, url: &str) {
        tracing::info!(target: "brazen::engine::servo", %url, "servo scaffold navigate");
        self.active_tab.current_url = url.to_string();
        self.navigation_state.url = url.to_string();
        self.navigation_state.title = "Loading...".to_string();
        self.navigation_state.load_progress = 0.1;
        self.navigation_state.document_ready = false;
        if self.active_tab.current_url != "about:blank" {
            self.navigation_state.can_go_back = true;
        }
        self.events
            .push(EngineEvent::NavigationRequested(url.to_string()));
        self.events.push(EngineEvent::NavigationStateUpdated(
            self.navigation_state.clone(),
        ));
    }

    fn reload(&mut self) {
        tracing::info!(target: "brazen::engine::servo", "servo scaffold reload");
        self.navigation_state.load_progress = 1.0;
        self.navigation_state.document_ready = true;
        self.events.push(EngineEvent::NavigationStateUpdated(
            self.navigation_state.clone(),
        ));
    }

    fn go_back(&mut self) {
        tracing::info!(target: "brazen::engine::servo", "servo scaffold go back");
        if self.navigation_state.can_go_back {
            self.navigation_state.can_go_forward = true;
            self.navigation_state.document_ready = false;
            self.events.push(EngineEvent::NavigationStateUpdated(
                self.navigation_state.clone(),
            ));
        }
    }

    fn go_forward(&mut self) {
        tracing::info!(target: "brazen::engine::servo", "servo scaffold go forward");
        if self.navigation_state.can_go_forward {
            self.navigation_state.document_ready = false;
            self.events.push(EngineEvent::NavigationStateUpdated(
                self.navigation_state.clone(),
            ));
        }
    }

    fn attach_surface(&mut self, surface: RenderSurfaceHandle) {
        self.surface_handle = Some(surface);
    }

    fn set_render_surface(&mut self, metadata: RenderSurfaceMetadata) {
        tracing::debug!(
            target: "brazen::engine::servo",
            width = metadata.viewport_width,
            height = metadata.viewport_height,
            scale = metadata.scale_factor_basis_points,
            "updated render surface metadata"
        );
        self.surface = Some(metadata);
    }

    fn set_focus(&mut self, focus: FocusState) {
        self.focus = focus;
        tracing::debug!(target: "brazen::engine::servo", ?focus, "focus updated");
    }

    fn handle_input(&mut self, event: InputEvent) {
        tracing::debug!(target: "brazen::engine::servo", ?event, "input event");
    }

    fn handle_ime(&mut self, event: ImeEvent) {
        tracing::debug!(target: "brazen::engine::servo", ?event, "ime event");
    }

    fn handle_clipboard(&mut self, request: ClipboardRequest) {
        tracing::debug!(target: "brazen::engine::servo", ?request, "clipboard request");
        self.events.push(EngineEvent::ClipboardRequested(request));
    }

    fn suspend(&mut self) {
        self.status = EngineStatus::Initializing;
        self.events.push(EngineEvent::StatusChanged(self.status()));
    }

    fn resume(&mut self) {
        self.status = EngineStatus::Ready;
        self.events.push(EngineEvent::StatusChanged(self.status()));
    }

    fn shutdown(&mut self) {
        self.status = EngineStatus::NoEngine;
        self.events.push(EngineEvent::StatusChanged(self.status()));
    }

    fn take_events(&mut self) -> Vec<EngineEvent> {
        std::mem::take(&mut self.events)
    }
}
