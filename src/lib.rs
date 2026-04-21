pub mod app;
pub mod automation;
pub mod cache;
pub mod cli_cache;
pub mod cli_introspect;
pub mod commands;
pub mod config;
pub mod engine;
pub mod logging;
pub mod navigation;
pub mod permissions;
pub mod platform_paths;
pub mod rendering;
pub mod servo_embedder;
pub mod servo_resources;
pub mod servo_runtime;
#[cfg(feature = "servo-upstream")]
pub mod servo_upstream;
pub mod mounts;
pub mod session;
pub mod terminal;
pub mod virtual_protocol;
pub mod virtual_router;
pub mod tls;
pub mod audit_log;

use std::path::{Path, PathBuf};

use thiserror::Error;

pub use app::{BrazenApp, ShellState};
pub use config::BrazenConfig;
pub use engine::{BrowserEngine, EngineFactory, EngineStatus, ServoEngineFactory};
pub use logging::{LoggingPlan, init_tracing};
pub use platform_paths::{PlatformPaths, RuntimePaths};

#[derive(Debug, Clone)]
pub struct BootstrapOptions {
    pub config_path: Option<PathBuf>,
}

impl BootstrapOptions {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self {
            config_path: Some(path.into()),
        }
    }
}

#[derive(Debug)]
pub struct BootstrapResult {
    pub config: BrazenConfig,
    pub paths: RuntimePaths,
    pub shell_state: ShellState,
}

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error(transparent)]
    Paths(#[from] platform_paths::PathsError),
    #[error(transparent)]
    Logging(#[from] logging::LoggingError),
}

pub fn default_config_path() -> Result<PathBuf, platform_paths::PathsError> {
    Ok(PlatformPaths::detect()?.default_config_path())
}

pub fn bootstrap(
    options: BootstrapOptions,
    engine_factory: &dyn EngineFactory,
) -> Result<BootstrapResult, BootstrapError> {
    tls::install_crypto_provider();
    let platform_paths = PlatformPaths::detect()?;
    let config_path = options
        .config_path
        .unwrap_or_else(|| platform_paths.default_config_path());
    let config = BrazenConfig::load_with_defaults(&config_path)?;
    let runtime_paths = platform_paths.resolve_runtime_paths(&config, &config_path)?;

    init_tracing(&config.logging, &runtime_paths.logs_dir)?;

    let shell_state = app::build_shell_state(&config, &runtime_paths, engine_factory);

    Ok(BootstrapResult {
        config,
        paths: runtime_paths,
        shell_state,
    })
}

pub fn write_default_config(path: &Path) -> Result<(), config::ConfigError> {
    config::write_default_config(path)
}
