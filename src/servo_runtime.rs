use crate::config::EngineConfig;

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
