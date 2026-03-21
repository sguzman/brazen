use std::time::{Duration, Instant};

use crate::config::EngineConfig;
use crate::engine::RenderSurfaceMetadata;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    CpuReadback,
    GpuTexture,
}

impl RenderMode {
    pub fn from_str(value: &str) -> Self {
        match value {
            "gpu-texture" => Self::GpuTexture,
            _ => Self::CpuReadback,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePacing {
    Vsync,
    Manual,
    OnDemand,
}

impl FramePacing {
    pub fn from_str(value: &str) -> Self {
        match value {
            "manual" => Self::Manual,
            "on-demand" => Self::OnDemand,
            _ => Self::Vsync,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServoRuntimeConfig {
    pub render_mode: RenderMode,
    pub webrender_backend: String,
    pub frame_pacing: FramePacing,
}

impl ServoRuntimeConfig {
    pub fn from_engine_config(config: &EngineConfig) -> Self {
        Self {
            render_mode: RenderMode::from_str(&config.render_mode),
            webrender_backend: config.webrender_backend.clone(),
            frame_pacing: FramePacing::from_str(&config.frame_pacing),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServoWindowAdapter {
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub scale_factor: f32,
}

impl ServoWindowAdapter {
    pub fn from_metadata(metadata: &RenderSurfaceMetadata) -> Self {
        Self {
            viewport_width: metadata.viewport_width,
            viewport_height: metadata.viewport_height,
            scale_factor: metadata.scale_factor_basis_points as f32 / 100.0,
        }
    }

    pub fn resize(&mut self, metadata: &RenderSurfaceMetadata) -> bool {
        let changed = self.viewport_width != metadata.viewport_width
            || self.viewport_height != metadata.viewport_height
            || (self.scale_factor - metadata.scale_factor_basis_points as f32 / 100.0).abs()
                > f32::EPSILON;
        self.viewport_width = metadata.viewport_width;
        self.viewport_height = metadata.viewport_height;
        self.scale_factor = metadata.scale_factor_basis_points as f32 / 100.0;
        changed
    }
}

#[derive(Debug, Clone)]
pub struct FrameScheduler {
    pacing: FramePacing,
    last_frame: Option<Instant>,
    pending_frame: bool,
}

impl FrameScheduler {
    pub fn new(pacing: FramePacing) -> Self {
        Self {
            pacing,
            last_frame: None,
            pending_frame: true,
        }
    }

    pub fn request_frame(&mut self) {
        self.pending_frame = true;
    }

    pub fn should_render(&mut self) -> bool {
        match self.pacing {
            FramePacing::OnDemand => {
                if self.pending_frame {
                    self.pending_frame = false;
                    self.last_frame = Some(Instant::now());
                    true
                } else {
                    false
                }
            }
            FramePacing::Manual => {
                let now = Instant::now();
                let ready = self
                    .last_frame
                    .map(|last| now.duration_since(last) >= Duration::from_millis(16))
                    .unwrap_or(true);
                if ready {
                    self.last_frame = Some(now);
                }
                ready
            }
            FramePacing::Vsync => {
                self.last_frame = Some(Instant::now());
                true
            }
        }
    }
}
