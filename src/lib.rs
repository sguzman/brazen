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
pub mod profile_db;
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
pub mod mcp;
pub mod mcp_stdio;
pub mod extraction;
pub mod ui_theme;

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
    let mut config = BrazenConfig::load_with_defaults(&config_path)?;
    let runtime_paths = platform_paths.resolve_runtime_paths(&config, &config_path)?;

    apply_profile_overrides(&mut config, &runtime_paths);

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

fn apply_profile_overrides(config: &mut BrazenConfig, runtime_paths: &RuntimePaths) {
    let Ok(db) = profile_db::ProfileDb::open(runtime_paths.active_profile_dir.join("state.sqlite"))
    else {
        return;
    };

    if let Ok(grants) = db.load_permission_grants() {
        for (domain, overrides) in grants {
            let entry = config.permissions.domain_overrides.entry(domain).or_default();
            for (capability, decision) in overrides {
                entry.insert(capability, decision);
            }
        }
    }

    if let Ok(settings) = db.load_automation_settings() {
        // Overlay profile-specific automation settings while preserving enabled flag and bind from config.
        let enabled = config.automation.enabled;
        let bind = config.automation.bind.clone();
        config.automation = settings;
        config.automation.enabled = enabled;
        config.automation.bind = bind;
    }
}

#[cfg(test)]
mod profile_override_tests {
    use super::*;

    #[test]
    fn profile_db_overrides_automation_config() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = BrazenConfig::default();
        config.automation.enabled = true;
        let runtime_paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };
        std::fs::create_dir_all(&runtime_paths.active_profile_dir).unwrap();
        let db = profile_db::ProfileDb::open(runtime_paths.active_profile_dir.join("state.sqlite"))
            .unwrap();
        let mut desired = config::AutomationConfig::default();
        desired.bind = "ws://127.0.0.1:0/ws".to_string();
        desired.require_auth = false;
        desired.expose_cache_api = true;
        db.save_automation_settings(&desired).unwrap();

        apply_profile_overrides(&mut config, &runtime_paths);
        assert_eq!(config.automation.bind, "ws://127.0.0.1:7942".to_string());
        assert_eq!(config.automation.require_auth, false);
        assert!(config.automation.expose_cache_api);
        assert!(config.automation.enabled);
    }
}
