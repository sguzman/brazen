use std::path::PathBuf;

use crate::config::EngineConfig;
use crate::engine::{RenderSurfaceHandle, RenderSurfaceMetadata};

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
        }
    }
}

#[derive(Debug, Clone)]
pub struct SurfaceSwapChain {
    pub surface: RenderSurfaceHandle,
    pub metadata: RenderSurfaceMetadata,
}

impl SurfaceSwapChain {
    pub fn present(&self) {}
}

#[derive(Debug)]
pub struct ServoEmbedder {
    pub state: ServoEmbedderState,
    pub config: ServoEmbedderConfig,
    pub surface: Option<SurfaceSwapChain>,
}

impl ServoEmbedder {
    pub fn new(config: ServoEmbedderConfig) -> Self {
        Self {
            state: ServoEmbedderState::Uninitialized,
            config,
            surface: None,
        }
    }

    pub fn init(&mut self) {
        self.state = ServoEmbedderState::Initializing;
        self.state = ServoEmbedderState::Running;
    }

    pub fn attach_surface(
        &mut self,
        surface: RenderSurfaceHandle,
        metadata: RenderSurfaceMetadata,
    ) {
        self.surface = Some(SurfaceSwapChain { surface, metadata });
    }

    pub fn suspend(&mut self) {
        self.state = ServoEmbedderState::Suspended;
    }

    pub fn resume(&mut self) {
        self.state = ServoEmbedderState::Running;
    }

    pub fn shutdown(&mut self) {
        self.state = ServoEmbedderState::Shutdown;
        self.surface = None;
    }
}
