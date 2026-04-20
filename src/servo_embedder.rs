use std::net::TcpListener;
use std::path::PathBuf;

use crate::config::EngineConfig;
use crate::engine::{
    AlphaMode, ColorSpace, EngineFrame, FocusState, InputEvent, PixelFormat, RenderSurfaceHandle,
    RenderSurfaceMetadata,
};
#[cfg(feature = "servo-upstream")]
use crate::rendering::frame_checksum;
use crate::servo_runtime::{
    FramePacing, FrameScheduler, RenderMode, ServoRuntimeConfig, ServoWindowAdapter,
};
#[cfg(feature = "servo-upstream")]
use crate::servo_upstream::{ServoUpstreamConfig, ServoUpstreamRuntime, UpstreamSnapshot};
#[cfg(feature = "servo-upstream")]
use libservo::CompositionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServoProcessModel {
    SingleProcess,
    MultiProcess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServoEmbedderState {
    Uninitialized,
    Initializing,
    Running,
    Suspended,
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct ServoEmbedderConfig {
    pub process_model: ServoProcessModel,
    pub gfx_backend: String,
    pub source_path: Option<PathBuf>,
    pub resources_dir: Option<PathBuf>,
    pub certificate_path: Option<PathBuf>,
    pub ignore_certificate_errors: bool,
    pub verbose_logging: bool,
    pub pixel_format: PixelFormat,
    pub alpha_mode: AlphaMode,
    pub color_space: ColorSpace,
    pub enable_pixel_probe: bool,
    pub runtime: ServoRuntimeConfig,
}

impl ServoEmbedderConfig {
    pub fn from_engine_config(config: &EngineConfig) -> Self {
        Self {
            process_model: match config.process_model.as_str() {
                "multi-process" => ServoProcessModel::MultiProcess,
                _ => ServoProcessModel::SingleProcess,
            },
            gfx_backend: config.gfx_backend.clone(),
            source_path: config
                .servo_source
                .as_ref()
                .and_then(|value| {
                    if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    }
                })
                .map(PathBuf::from),
            resources_dir: config
                .servo_resources_dir
                .as_ref()
                .and_then(|value| {
                    if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    }
                })
                .map(PathBuf::from),
            certificate_path: config
                .certificate_path
                .as_ref()
                .and_then(|value| {
                    if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    }
                })
                .map(PathBuf::from),
            ignore_certificate_errors: config.ignore_certificate_errors,
            verbose_logging: config.verbose_logging,
            pixel_format: PixelFormat::from_value(&config.pixel_format),
            alpha_mode: AlphaMode::from_value(&config.alpha_mode),
            color_space: ColorSpace::from_value(&config.color_space),
            enable_pixel_probe: config.debug_pixel_probe,
            runtime: ServoRuntimeConfig::from_engine_config(config),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SurfaceSwapChain {
    pub surface: RenderSurfaceHandle,
    pub metadata: RenderSurfaceMetadata,
    pub pixels: Vec<u8>,
}

impl SurfaceSwapChain {
    pub fn new(surface: RenderSurfaceHandle, metadata: RenderSurfaceMetadata) -> Self {
        let size = (metadata.viewport_width as usize)
            .saturating_mul(metadata.viewport_height as usize)
            .saturating_mul(4);
        Self {
            surface,
            metadata,
            pixels: vec![0; size],
        }
    }

    pub fn resize(&mut self, metadata: &RenderSurfaceMetadata) {
        if self.metadata.viewport_width == metadata.viewport_width
            && self.metadata.viewport_height == metadata.viewport_height
        {
            self.metadata = metadata.clone();
            return;
        }
        let size = (metadata.viewport_width as usize)
            .saturating_mul(metadata.viewport_height as usize)
            .saturating_mul(4);
        self.metadata = metadata.clone();
        self.pixels.resize(size, 0);
    }

    pub fn present(&self) {}
}

#[derive(Debug)]
pub struct DevtoolsState {
    pub enabled: bool,
    pub transport: String,
    pub endpoint: Option<String>,
    pub listener: Option<TcpListener>,
}

#[derive(Clone)]
pub struct ServoBrowserState {
    pub history: Vec<String>,
    pub history_index: usize,
    pub last_committed_url: String,
    pub title: String,
    pub favicon_url: Option<String>,
}

impl ServoBrowserState {
    pub fn new() -> Self {
        Self {
            history: vec!["about:blank".to_string()],
            history_index: 0,
            last_committed_url: "about:blank".to_string(),
            title: "Servo".to_string(),
            favicon_url: None,
        }
    }

    pub fn navigate(&mut self, url: &str) {
        if self.history_index + 1 < self.history.len() {
            self.history.truncate(self.history_index + 1);
        }
        self.history.push(url.to_string());
        self.history_index = self.history.len().saturating_sub(1);
        self.last_committed_url = url.to_string();
        self.title = format!("Loading {url}");
        self.favicon_url = if url.starts_with("http") {
            Some(format!("{url}/favicon.ico"))
        } else {
            None
        };
    }

    pub fn reload(&mut self) {
        self.title = format!("Reloading {}", self.last_committed_url);
    }

    pub fn stop(&mut self) {
        self.title = format!("Stopped {}", self.last_committed_url);
    }
}

impl Default for ServoBrowserState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ServoEmbedder {
    pub state: ServoEmbedderState,
    pub config: ServoEmbedderConfig,
    pub surface: Option<SurfaceSwapChain>,
    pub frame_counter: u64,
    pub verbose_logging: bool,
    pub render_mode: RenderMode,
    pub frame_pacing: FramePacing,
    pub window: Option<ServoWindowAdapter>,
    pub frame_scheduler: FrameScheduler,
    pub devtools: Option<DevtoolsState>,
    pub renderer_ready: bool,
    pub compositor_ready: bool,
    pub browser_state: ServoBrowserState,
    pub last_focus: FocusState,
    #[cfg(feature = "servo-upstream")]
    pub upstream: Option<ServoUpstreamRuntime>,
    #[cfg(feature = "servo-upstream")]
    pub last_snapshot: Option<UpstreamSnapshot>,
    pub upstream_active: bool,
    pub upstream_error: Option<String>,
    pub resource_reader_ready: Option<bool>,
    pub resource_reader_path: Option<PathBuf>,
    pub last_pointer: (f32, f32),
    pub input_scale: f32,
    pub last_frame_checksum: Option<u64>,
    pub pending_navigation: Option<String>,
    pub logged_first_frame: bool,
    pub page_zoom: f32,
    #[cfg(feature = "servo-upstream")]
    pub upstream_event_tx: Option<std::sync::mpsc::Sender<crate::engine::EngineEvent>>,
    #[cfg(feature = "servo-upstream")]
    pub upstream_event_rx: Option<std::sync::mpsc::Receiver<crate::engine::EngineEvent>>,
}

impl std::fmt::Debug for ServoEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServoEmbedder")
            .field("state", &self.state)
            .field("frame_counter", &self.frame_counter)
            .field("upstream_active", &self.upstream_active)
            .finish()
    }
}

impl ServoEmbedder {
    pub fn new(config: ServoEmbedderConfig) -> Self {
        Self {
            state: ServoEmbedderState::Uninitialized,
            verbose_logging: config.verbose_logging,
            render_mode: config.runtime.render_mode,
            frame_pacing: config.runtime.frame_pacing,
            window: None,
            frame_scheduler: FrameScheduler::new(config.runtime.frame_pacing),
            config,
            surface: None,
            frame_counter: 0,
            devtools: None,
            renderer_ready: false,
            compositor_ready: false,
            browser_state: ServoBrowserState::new(),
            last_focus: FocusState::Unfocused,
            #[cfg(feature = "servo-upstream")]
            upstream: None,
            #[cfg(feature = "servo-upstream")]
            last_snapshot: None,
            upstream_active: false,
            upstream_error: None,
            resource_reader_ready: None,
            resource_reader_path: None,
            last_pointer: (0.0, 0.0),
            input_scale: 1.0,
            last_frame_checksum: None,
            pending_navigation: None,
            logged_first_frame: false,
            page_zoom: 1.0,
            #[cfg(feature = "servo-upstream")]
            upstream_event_tx: None,
            #[cfg(feature = "servo-upstream")]
            upstream_event_rx: None,
        }
    }

    pub fn init(&mut self) -> Result<(), String> {
        tracing::info!(target: "brazen::servo", "initializing servo embedder");
        tracing::info!(
            target: "brazen::servo",
            render_mode = ?self.render_mode,
            webrender_backend = %self.config.runtime.webrender_backend,
            frame_pacing = ?self.frame_pacing,
            "servo runtime config"
        );
        self.state = ServoEmbedderState::Initializing;
        self.init_renderer()?;
        self.init_compositor()?;
        self.init_webrender()?;
        self.state = ServoEmbedderState::Running;
        Ok(())
    }

    pub fn init_renderer(&mut self) -> Result<(), String> {
        tracing::debug!(target: "brazen::servo", "renderer initialized (stub)");
        self.renderer_ready = true;
        Ok(())
    }

    pub fn init_compositor(&mut self) -> Result<(), String> {
        tracing::debug!(target: "brazen::servo", "compositor initialized (stub)");
        self.compositor_ready = true;
        Ok(())
    }

    pub fn init_webrender(&mut self) -> Result<(), String> {
        tracing::info!(
            target: "brazen::servo",
            backend = %self.config.runtime.webrender_backend,
            "webrender initialized (stub)"
        );
        Ok(())
    }

    pub fn attach_surface(
        &mut self,
        surface: RenderSurfaceHandle,
        metadata: RenderSurfaceMetadata,
    ) {
        tracing::debug!(
            target: "brazen::servo",
            width = metadata.viewport_width,
            height = metadata.viewport_height,
            "attach surface"
        );
        self.input_scale = metadata.scale_factor_basis_points as f32 / 100.0;
        self.window = Some(ServoWindowAdapter::from_metadata(&metadata));
        self.surface = Some(SurfaceSwapChain::new(surface, metadata));
        self.frame_scheduler.request_frame();
        #[cfg(feature = "servo-upstream")]
        self.ensure_upstream();
    }

    pub fn update_surface(&mut self, metadata: RenderSurfaceMetadata) {
        self.input_scale = metadata.scale_factor_basis_points as f32 / 100.0;
        if let Some(surface) = self.surface.as_mut() {
            surface.resize(&metadata);
        }
        if let Some(window) = self.window.as_mut() {
            if window.resize(&metadata) {
                self.frame_scheduler.request_frame();
            }
        } else {
            self.window = Some(ServoWindowAdapter::from_metadata(&metadata));
        }
        #[cfg(feature = "servo-upstream")]
        self.resize_upstream(metadata.viewport_width, metadata.viewport_height);
    }

    pub fn suspend(&mut self) {
        tracing::info!(target: "brazen::servo", "embedder suspended");
        self.state = ServoEmbedderState::Suspended;
    }

    pub fn resume(&mut self) {
        tracing::info!(target: "brazen::servo", "embedder resumed");
        self.state = ServoEmbedderState::Running;
    }

    pub fn shutdown(&mut self) {
        tracing::info!(target: "brazen::servo", "embedder shutdown");
        self.state = ServoEmbedderState::Shutdown;
        self.surface = None;
        self.upstream_active = false;
        self.resource_reader_ready = None;
        self.resource_reader_path = None;
        #[cfg(feature = "servo-upstream")]
        {
            self.upstream = None;
        }
    }

    pub fn tick(&mut self) {
        if self.verbose_logging {
            tracing::trace!(target: "brazen::servo", "tick");
        }
    }

    pub fn render_frame(&mut self) -> Option<EngineFrame> {
        #[cfg(feature = "servo-upstream")]
        if self.upstream.is_some() {
            return self.render_upstream_frame();
        }
        if !self.frame_scheduler.should_render() {
            return None;
        }
        let surface = self.surface.as_mut()?;
        let width = surface.metadata.viewport_width;
        let height = surface.metadata.viewport_height;
        if width == 0 || height == 0 {
            return None;
        }
        self.frame_counter = self.frame_counter.wrapping_add(1);
        let counter = self.frame_counter;
        let w = width as usize;
        let h = height as usize;
        let pixels = {
            let pixels = &mut surface.pixels;
            for y in 0..h {
                for x in 0..w {
                    let base = (y * w + x) * 4;
                    let r = ((x as u64 + counter) % 255) as u8;
                    let g = ((y as u64 + counter) % 255) as u8;
                    let b = ((x as u64 + y as u64 + counter) % 255) as u8;
                    pixels[base] = r;
                    pixels[base + 1] = g;
                    pixels[base + 2] = b;
                    pixels[base + 3] = 255;
                }
            }
            pixels.clone()
        };
        surface.present();
        Some(EngineFrame {
            width,
            height,
            frame_number: self.frame_counter,
            stride_bytes: (width as usize).saturating_mul(4),
            pixel_format: PixelFormat::Rgba8,
            alpha_mode: AlphaMode::Straight,
            color_space: ColorSpace::Srgb,
            pixels,
        })
    }

    pub fn navigate(&mut self, url: &str) {
        self.browser_state.navigate(url);
        self.frame_scheduler.request_frame();
        #[cfg(feature = "servo-upstream")]
        if let Some(upstream) = &self.upstream {
            if let Err(error) = upstream.navigate(url) {
                self.upstream_error = Some(error.clone());
                tracing::error!(target: "brazen::servo", %error, "upstream navigation failed");
            }
        } else {
            self.pending_navigation = Some(url.to_string());
        }
    }

    pub fn reload(&mut self) {
        self.browser_state.reload();
        self.frame_scheduler.request_frame();
        #[cfg(feature = "servo-upstream")]
        if let Some(upstream) = &self.upstream {
            upstream.reload();
        }
    }

    pub fn stop(&mut self) {
        self.browser_state.stop();
        self.frame_scheduler.request_frame();
        #[cfg(feature = "servo-upstream")]
        if let Some(upstream) = &self.upstream {
            upstream.stop();
        }
    }

    pub fn handle_input(&mut self, event: &InputEvent) {
        if self.verbose_logging {
            tracing::trace!(target: "brazen::servo", ?event, "input forwarded");
        }
        match event {
            InputEvent::PointerEnter { x, y } | InputEvent::PointerMove { x, y } => {
                let scaled = (*x * self.input_scale, *y * self.input_scale);
                self.last_pointer = scaled;
                #[cfg(feature = "servo-upstream")]
                if let Some(upstream) = &self.upstream {
                    upstream.handle_mouse_move(scaled.0, scaled.1);
                }
                #[cfg(not(feature = "servo-upstream"))]
                let _ = (x, y);
            }
            InputEvent::PointerDown {
                button,
                click_count,
            } => {
                #[cfg(feature = "servo-upstream")]
                if let Some(upstream) = &self.upstream {
                    upstream.handle_mouse_button(
                        *button,
                        true,
                        self.last_pointer.0,
                        self.last_pointer.1,
                    );
                    if *click_count >= 2 {
                        tracing::trace!(
                            target: "brazen::servo",
                            button = *button,
                            click_count = *click_count,
                            "double click detected"
                        );
                    }
                }
                #[cfg(not(feature = "servo-upstream"))]
                let _ = (button, click_count);
            }
            InputEvent::PointerUp { button } => {
                #[cfg(feature = "servo-upstream")]
                if let Some(upstream) = &self.upstream {
                    upstream.handle_mouse_button(
                        *button,
                        false,
                        self.last_pointer.0,
                        self.last_pointer.1,
                    );
                }
                #[cfg(not(feature = "servo-upstream"))]
                let _ = button;
            }
            InputEvent::PointerLeave =>
            {
                #[cfg(feature = "servo-upstream")]
                if let Some(upstream) = &self.upstream {
                    upstream.handle_mouse_leave();
                }
            }
            InputEvent::Scroll { delta_x, delta_y } => {
                #[cfg(feature = "servo-upstream")]
                if let Some(upstream) = &self.upstream {
                    let scaled_x = *delta_x * self.input_scale;
                    let scaled_y = *delta_y * self.input_scale;
                    tracing::trace!(
                        target: "brazen::servo",
                        delta_x = scaled_x,
                        delta_y = scaled_y,
                        scale = self.input_scale,
                        "forwarding scroll delta"
                    );
                    upstream.handle_wheel(
                        scaled_x,
                        scaled_y,
                        self.last_pointer.0,
                        self.last_pointer.1,
                    );
                }
                #[cfg(not(feature = "servo-upstream"))]
                let _ = (delta_x, delta_y);
            }
            InputEvent::Zoom { delta } => {
                #[cfg(feature = "servo-upstream")]
                if let Some(upstream) = &self.upstream {
                    upstream.handle_pinch_zoom(*delta, self.last_pointer.0, self.last_pointer.1);
                }
                #[cfg(not(feature = "servo-upstream"))]
                let _ = delta;
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } => {
                #[cfg(feature = "servo-upstream")]
                if let Some(upstream) = &self.upstream {
                    upstream.handle_keyboard(key, true, *modifiers, *repeat);
                }
                #[cfg(not(feature = "servo-upstream"))]
                let _ = (key, modifiers, repeat);
            }
            InputEvent::KeyUp { key, modifiers } => {
                #[cfg(feature = "servo-upstream")]
                if let Some(upstream) = &self.upstream {
                    upstream.handle_keyboard(key, false, *modifiers, false);
                }
                #[cfg(not(feature = "servo-upstream"))]
                let _ = (key, modifiers);
            }
            InputEvent::TextInput { text } => {
                #[cfg(feature = "servo-upstream")]
                if let Some(upstream) = &self.upstream {
                    upstream.handle_ime_composition(CompositionState::End, text.clone());
                }
                #[cfg(not(feature = "servo-upstream"))]
                let _ = text;
            }
        }
        self.frame_scheduler.request_frame();
    }

    pub fn handle_ime(&mut self, event: &crate::engine::ImeEvent) {
        if self.verbose_logging {
            tracing::trace!(target: "brazen::servo", ?event, "ime forwarded");
        }
        #[cfg(feature = "servo-upstream")]
        if let Some(upstream) = &self.upstream {
            match event {
                crate::engine::ImeEvent::CompositionStart => {
                    upstream.handle_ime_composition(CompositionState::Start, String::new());
                }
                crate::engine::ImeEvent::CompositionUpdate { text }
                | crate::engine::ImeEvent::CompositionEnd { text } => {
                    let state = if matches!(event, crate::engine::ImeEvent::CompositionEnd { .. }) {
                        CompositionState::End
                    } else {
                        CompositionState::Update
                    };
                    upstream.handle_ime_composition(state, text.clone());
                }
                crate::engine::ImeEvent::Dismissed => {
                    upstream.handle_ime_dismissed();
                }
            }
        }
        self.frame_scheduler.request_frame();
    }

    pub fn handle_clipboard(&mut self, request: &crate::engine::ClipboardRequest) {
        if self.verbose_logging {
            tracing::trace!(target: "brazen::servo", ?request, "clipboard forwarded");
        }
        #[cfg(feature = "servo-upstream")]
        if let Some(upstream) = &self.upstream {
            upstream.handle_clipboard(request);
        }
        self.frame_scheduler.request_frame();
    }

    pub fn evaluate_javascript(
        &mut self,
        script: String,
        callback: Box<dyn FnOnce(Result<serde_json::Value, String>) + Send + 'static>,
    ) {
        #[cfg(feature = "servo-upstream")]
        if let Some(upstream) = &mut self.upstream {
            upstream.evaluate_javascript(script, callback);
            return;
        }
        callback(Err("Embedder has no active upstream".to_string()));
    }

    pub fn set_page_zoom(&mut self, zoom: f32) {
        let clamped = zoom.clamp(0.1, 10.0);
        self.page_zoom = clamped;
        #[cfg(feature = "servo-upstream")]
        if let Some(upstream) = &self.upstream {
            upstream.set_page_zoom(clamped);
        }
        self.frame_scheduler.request_frame();
    }

    pub fn page_zoom(&self) -> f32 {
        self.page_zoom
    }

    pub fn set_focus(&mut self, focus: FocusState) {
        self.last_focus = focus;
        if self.verbose_logging {
            tracing::trace!(target: "brazen::servo", ?focus, "focus updated");
        }
        #[cfg(feature = "servo-upstream")]
        if let Some(upstream) = &self.upstream {
            let _ = upstream.snapshot();
        }
        self.frame_scheduler.request_frame();
    }

    pub fn set_verbose_logging(&mut self, enabled: bool) {
        self.verbose_logging = enabled;
        tracing::info!(
            target: "brazen::servo",
            enabled,
            "verbose logging toggled"
        );
    }

    pub fn configure_devtools(&mut self, enabled: bool, transport: &str) -> Option<String> {
        if !enabled {
            self.devtools = None;
            return None;
        }
        let transport = transport.to_string();
        if transport == "tcp"
            && let Ok(listener) = TcpListener::bind("127.0.0.1:0")
        {
            let addr = listener.local_addr().ok();
            let endpoint = addr.map(|addr| format!("tcp://{addr}"));
            self.devtools = Some(DevtoolsState {
                enabled,
                transport,
                endpoint: endpoint.clone(),
                listener: Some(listener),
            });
            tracing::info!(
                target: "brazen::servo",
                endpoint = ?endpoint,
                "devtools tcp listener ready"
            );
            return endpoint;
        }

        let endpoint = Some("local-socket://brazen-devtools".to_string());
        self.devtools = Some(DevtoolsState {
            enabled,
            transport,
            endpoint: endpoint.clone(),
            listener: None,
        });
        tracing::info!(
            target: "brazen::servo",
            endpoint = ?endpoint,
            "devtools transport configured"
        );
        endpoint
    }

    #[cfg(feature = "servo-upstream")]
    fn ensure_upstream(&mut self) {
        if self.upstream.is_some() {
            return;
        }
        let Some(surface) = &self.surface else {
            return;
        };
        let upstream_config = ServoUpstreamConfig {
            pixel_format: self.config.pixel_format,
            alpha_mode: self.config.alpha_mode,
            color_space: self.config.color_space,
            enable_pixel_probe: self.config.enable_pixel_probe,
            resources_dir: self.config.resources_dir.clone(),
            certificate_path: self.config.certificate_path.clone(),
            ignore_certificate_errors: self.config.ignore_certificate_errors,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.upstream_event_tx = Some(tx.clone());
        self.upstream_event_rx = Some(rx);

        match ServoUpstreamRuntime::new(
            surface.metadata.viewport_width,
            surface.metadata.viewport_height,
            upstream_config,
            tx,
        ) {
            Ok(runtime) => {
                tracing::info!(
                    target: "brazen::servo",
                    width = surface.metadata.viewport_width,
                    height = surface.metadata.viewport_height,
                    "upstream runtime initialized"
                );
                runtime.set_page_zoom(self.page_zoom);
                self.resource_reader_ready = Some(true);
                self.resource_reader_path = Some(runtime.resources_dir().to_path_buf());
                self.upstream = Some(runtime);
                self.upstream_active = true;
                if let Some(url) = self.pending_navigation.take()
                    && let Some(upstream) = &self.upstream
                    && let Err(error) = upstream.navigate(&url)
                {
                    self.upstream_error = Some(error.clone());
                    tracing::error!(
                        target: "brazen::servo",
                        %error,
                        "pending navigation failed"
                    );
                }
            }
            Err(error) => {
                self.upstream_active = false;
                self.upstream_error = Some(error.clone());
                self.resource_reader_ready = Some(false);
                tracing::error!(target: "brazen::servo", %error, "failed to init upstream servo");
            }
        }
    }

    #[cfg(feature = "servo-upstream")]
    fn resize_upstream(&mut self, width: u32, height: u32) {
        if let Some(upstream) = &self.upstream {
            upstream.resize(width, height);
        }
    }

    #[cfg(feature = "servo-upstream")]
    fn render_upstream_frame(&mut self) -> Option<EngineFrame> {
        let upstream = self.upstream.as_mut()?;
        upstream.spin();
        let frame = upstream.render_frame()?;
        if frame.width == 0 || frame.height == 0 {
            return None;
        }
        if !self.logged_first_frame {
            self.logged_first_frame = true;
            tracing::info!(
                target: "brazen::servo",
                width = frame.width,
                height = frame.height,
                format = frame.pixel_format.as_str(),
                "first upstream frame captured"
            );
        }
        let expected_width = self
            .surface
            .as_ref()
            .map(|surface| surface.metadata.viewport_width)
            .unwrap_or(frame.width);
        let expected_height = self
            .surface
            .as_ref()
            .map(|surface| surface.metadata.viewport_height)
            .unwrap_or(frame.height);
        if frame.width != expected_width || frame.height != expected_height {
            tracing::warn!(
                target: "brazen::servo",
                expected_width,
                expected_height,
                frame_width = frame.width,
                frame_height = frame.height,
                "upstream frame size mismatch"
            );
        }
        let checksum = frame_checksum(&frame.pixels);
        if self.last_frame_checksum == Some(checksum) {
            tracing::debug!(
                target: "brazen::servo",
                checksum,
                "duplicate upstream frame detected"
            );
        }
        self.last_frame_checksum = Some(checksum);
        self.last_snapshot = Some(upstream.snapshot());
        self.frame_counter = self.frame_counter.wrapping_add(1);
        Some(EngineFrame {
            width: frame.width,
            height: frame.height,
            frame_number: self.frame_counter,
            stride_bytes: frame.stride_bytes,
            pixel_format: frame.pixel_format,
            alpha_mode: frame.alpha_mode,
            color_space: frame.color_space,
            pixels: frame.pixels,
        })
    }

    #[cfg(feature = "servo-upstream")]
    pub fn upstream_snapshot(&self) -> Option<UpstreamSnapshot> {
        self.last_snapshot
            .clone()
            .or_else(|| self.upstream.as_ref().map(|runtime| runtime.snapshot()))
    }

    pub fn upstream_active(&self) -> bool {
        #[cfg(feature = "servo-upstream")]
        {
            self.upstream_active && self.upstream.is_some()
        }
        #[cfg(not(feature = "servo-upstream"))]
        {
            false
        }
    }

    pub fn resource_reader_ready(&self) -> Option<bool> {
        self.resource_reader_ready
    }

    pub fn resource_reader_path(&self) -> Option<String> {
        self.resource_reader_path
            .as_ref()
            .map(|path| path.display().to_string())
    }

    #[cfg(feature = "servo-upstream")]
    pub fn take_devtools_endpoint(&self) -> Option<String> {
        self.upstream.as_ref()?.take_devtools_endpoint()
    }

    #[cfg(feature = "servo-upstream")]
    pub fn upstream_error(&self) -> Option<String> {
        self.upstream_error
            .clone()
            .or_else(|| self.upstream.as_ref()?.last_error())
    }
}
