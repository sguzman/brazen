use std::net::TcpListener;
use std::path::PathBuf;

use crate::config::EngineConfig;
use crate::engine::{
    EngineFrame, FocusState, InputEvent, RenderSurfaceHandle, RenderSurfaceMetadata,
};
use crate::servo_runtime::{FramePacing, RenderMode, ServoRuntimeConfig};

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
    pub verbose_logging: bool,
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
            verbose_logging: config.verbose_logging,
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

    pub fn resize(&mut self, metadata: RenderSurfaceMetadata) {
        if self.metadata.viewport_width == metadata.viewport_width
            && self.metadata.viewport_height == metadata.viewport_height
        {
            self.metadata = metadata;
            return;
        }
        let size = (metadata.viewport_width as usize)
            .saturating_mul(metadata.viewport_height as usize)
            .saturating_mul(4);
        self.metadata = metadata;
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

#[derive(Debug)]
pub struct ServoEmbedder {
    pub state: ServoEmbedderState,
    pub config: ServoEmbedderConfig,
    pub surface: Option<SurfaceSwapChain>,
    pub frame_counter: u64,
    pub verbose_logging: bool,
    pub render_mode: RenderMode,
    pub frame_pacing: FramePacing,
    pub devtools: Option<DevtoolsState>,
    pub renderer_ready: bool,
    pub compositor_ready: bool,
    pub current_url: String,
    pub last_focus: FocusState,
}

impl ServoEmbedder {
    pub fn new(config: ServoEmbedderConfig) -> Self {
        Self {
            state: ServoEmbedderState::Uninitialized,
            verbose_logging: config.verbose_logging,
            render_mode: config.runtime.render_mode,
            frame_pacing: config.runtime.frame_pacing,
            config,
            surface: None,
            frame_counter: 0,
            devtools: None,
            renderer_ready: false,
            compositor_ready: false,
            current_url: "about:blank".to_string(),
            last_focus: FocusState::Unfocused,
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
        self.surface = Some(SurfaceSwapChain::new(surface, metadata));
    }

    pub fn update_surface(&mut self, metadata: RenderSurfaceMetadata) {
        if let Some(surface) = self.surface.as_mut() {
            surface.resize(metadata);
        }
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
    }

    pub fn tick(&mut self) {
        if self.verbose_logging {
            tracing::trace!(target: "brazen::servo", "tick");
        }
    }

    pub fn render_frame(&mut self) -> Option<EngineFrame> {
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
            pixels,
        })
    }

    pub fn handle_input(&mut self, event: &InputEvent) {
        if self.verbose_logging {
            tracing::trace!(target: "brazen::servo", ?event, "input forwarded");
        }
    }

    pub fn handle_ime(&mut self, event: &crate::engine::ImeEvent) {
        if self.verbose_logging {
            tracing::trace!(target: "brazen::servo", ?event, "ime forwarded");
        }
    }

    pub fn handle_clipboard(&mut self, request: &crate::engine::ClipboardRequest) {
        if self.verbose_logging {
            tracing::trace!(target: "brazen::servo", ?request, "clipboard forwarded");
        }
    }

    pub fn set_focus(&mut self, focus: FocusState) {
        self.last_focus = focus;
        if self.verbose_logging {
            tracing::trace!(target: "brazen::servo", ?focus, "focus updated");
        }
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
        if transport == "tcp" {
            if let Ok(listener) = TcpListener::bind("127.0.0.1:0") {
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
}
