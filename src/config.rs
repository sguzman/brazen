use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use url::Url;

use crate::navigation::resolve_startup_url;
use crate::permissions::PermissionPolicy;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BrazenConfig {
    pub app: AppConfig,
    pub window: WindowConfig,
    pub logging: LoggingConfig,
    pub engine: EngineConfig,
    pub directories: DirectoryRootsConfig,
    pub profiles: ProfileConfig,
    pub cache: CacheConfig,
    pub permissions: PermissionPolicy,
    pub automation: AutomationConfig,
    pub extraction: ExtractionConfig,
    pub media: MediaConfig,
    pub features: FeatureFlags,
}

impl BrazenConfig {
    pub fn load_with_defaults(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|source| ConfigError::CreateConfigDir {
                    path: parent.display().to_string(),
                    source,
                })?;
            }
            write_default_config(path)?;
        }

        let raw = fs::read_to_string(path).map_err(|source| ConfigError::ReadConfig {
            path: path.display().to_string(),
            source,
        })?;
        let merged = merge_defaults(&raw)?;
        let config: BrazenConfig = merged.try_into().map_err(ConfigError::Deserialize)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.window.initial_width < 640.0 {
            return Err(ConfigError::Validation(
                "window.initial_width must be at least 640".to_string(),
            ));
        }
        if self.window.initial_height < 480.0 {
            return Err(ConfigError::Validation(
                "window.initial_height must be at least 480".to_string(),
            ));
        }
        if self.cache.max_entry_bytes == 0 {
            return Err(ConfigError::Validation(
                "cache.max_entry_bytes must be greater than zero".to_string(),
            ));
        }
        match self.cache.storage_mode.as_str() {
            "memory" | "disk" | "archive" => {}
            _ => {
                return Err(ConfigError::Validation(
                    "cache.storage_mode must be memory, disk, or archive".to_string(),
                ));
            }
        }
        if self.profiles.active_profile.trim().is_empty() {
            return Err(ConfigError::Validation(
                "profiles.active_profile must be non-empty".to_string(),
            ));
        }
        match self.app.mode.as_str() {
            "dev" | "prod" => {}
            _ => {
                return Err(ConfigError::Validation(
                    "app.mode must be dev or prod".to_string(),
                ));
            }
        }
        if self.engine.resource_limits.memory_mb == 0 {
            return Err(ConfigError::Validation(
                "engine.resource_limits.memory_mb must be greater than zero".to_string(),
            ));
        }
        if let Err(reason) = resolve_startup_url(&self.engine.startup_url) {
            return Err(ConfigError::Validation(format!(
                "engine.startup_url is invalid: {reason}"
            )));
        }
        if self.engine.resource_limits.max_tabs == 0 {
            return Err(ConfigError::Validation(
                "engine.resource_limits.max_tabs must be greater than zero".to_string(),
            ));
        }
        match self.engine.new_window_policy.as_str() {
            "same-tab" | "new-tab" | "block" => {}
            _ => {
                return Err(ConfigError::Validation(
                    "engine.new_window_policy must be same-tab, new-tab, or block".to_string(),
                ));
            }
        }
        if self.engine.devtools_enabled {
            match self.engine.devtools_transport.as_str() {
                "local-socket" | "tcp" => {}
                _ => {
                    return Err(ConfigError::Validation(
                        "engine.devtools_transport must be local-socket or tcp when devtools are enabled".to_string(),
                    ))
                }
            }
        }
        match self.engine.render_mode.as_str() {
            "cpu-readback" | "gpu-texture" => {}
            _ => {
                return Err(ConfigError::Validation(
                    "engine.render_mode must be cpu-readback or gpu-texture".to_string(),
                ));
            }
        }
        match self.engine.pixel_format.as_str() {
            "rgba8" | "bgra8" => {}
            _ => {
                return Err(ConfigError::Validation(
                    "engine.pixel_format must be rgba8 or bgra8".to_string(),
                ));
            }
        }
        match self.engine.alpha_mode.as_str() {
            "straight" | "premultiplied" => {}
            _ => {
                return Err(ConfigError::Validation(
                    "engine.alpha_mode must be straight or premultiplied".to_string(),
                ));
            }
        }
        match self.engine.color_space.as_str() {
            "srgb" | "linear" => {}
            _ => {
                return Err(ConfigError::Validation(
                    "engine.color_space must be srgb or linear".to_string(),
                ));
            }
        }
        match self.engine.frame_pacing.as_str() {
            "vsync" | "manual" | "on-demand" => {}
            _ => {
                return Err(ConfigError::Validation(
                    "engine.frame_pacing must be vsync, manual, or on-demand".to_string(),
                ));
            }
        }
        if self.automation.enabled {
            let url = Url::parse(&self.automation.bind).map_err(|error| {
                ConfigError::Validation(format!("automation.bind must be a valid URL: {error}"))
            })?;
            if url.scheme() != "ws" && url.scheme() != "wss" {
                return Err(ConfigError::Validation(
                    "automation.bind must use ws or wss".to_string(),
                ));
            }
        }

        Ok(())
    }
}

pub fn write_default_config(path: &Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::CreateConfigDir {
            path: parent.display().to_string(),
            source,
        })?;
    }
    fs::write(path, default_config_toml()).map_err(|source| ConfigError::WriteConfig {
        path: path.display().to_string(),
        source,
    })
}

fn merge_defaults(raw: &str) -> Result<toml::Value, ConfigError> {
    let mut base =
        toml::Value::try_from(BrazenConfig::default()).map_err(ConfigError::Serialize)?;
    let overlay = toml::from_str::<toml::Value>(raw).map_err(ConfigError::Parse)?;
    merge_value(&mut base, overlay);
    Ok(base)
}

fn merge_value(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, value) in overlay_table {
                match base_table.get_mut(&key) {
                    Some(existing) => merge_value(existing, value),
                    None => {
                        base_table.insert(key, value);
                    }
                }
            }
        }
        (base_slot, overlay_value) => *base_slot = overlay_value,
    }
}

pub fn default_config_toml() -> &'static str {
    include_str!("../config/brazen.toml")
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to create config directory {path}: {source}")]
    CreateConfigDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read config {path}: {source}")]
    ReadConfig {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write config {path}: {source}")]
    WriteConfig {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize default config: {0}")]
    Serialize(#[source] toml::ser::Error),
    #[error("failed to deserialize merged config: {0}")]
    Deserialize(#[source] toml::de::Error),
    #[error("invalid config: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub name: String,
    pub tagline: String,
    pub homepage: String,
    pub mode: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "Brazen".to_string(),
            tagline: "Capability Browser Platform".to_string(),
            homepage: "https://example.invalid/brazen".to_string(),
            mode: "dev".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub initial_width: f32,
    pub initial_height: f32,
    pub min_width: f32,
    pub min_height: f32,
    pub show_log_panel_on_startup: bool,
    pub show_permission_panel_on_startup: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            initial_width: 1440.0,
            initial_height: 920.0,
            min_width: 960.0,
            min_height: 640.0,
            show_log_panel_on_startup: true,
            show_permission_panel_on_startup: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub console_filter: String,
    pub file_filter: String,
    pub file_name_prefix: String,
    pub ansi: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            console_filter: "info,brazen=debug".to_string(),
            file_filter: "debug,brazen=trace".to_string(),
            file_name_prefix: "brazen.log".to_string(),
            ansi: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub process_model: String,
    pub gfx_backend: String,
    pub servo_source: Option<String>,
    pub servo_source_tag: String,
    pub servo_source_rev: String,
    pub startup_url: String,
    pub enable_multiprocess: bool,
    pub new_window_policy: String,
    pub verbose_logging: bool,
    pub render_mode: String,
    pub webrender_backend: String,
    pub frame_pacing: String,
    pub pixel_format: String,
    pub alpha_mode: String,
    pub color_space: String,
    pub debug_bypass_swizzle: bool,
    pub debug_capture_next_frame: bool,
    pub debug_capture_dir: String,
    pub debug_pixel_probe: bool,
    pub debug_pointer_overlay: bool,
    pub devtools_enabled: bool,
    pub devtools_transport: String,
    pub resource_limits: ResourceLimits,
    pub security_warnings: bool,
    pub profile_isolation: bool,
    pub storage_policy: String,
    pub service_worker_policy: String,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            process_model: "single-process".to_string(),
            gfx_backend: "gl".to_string(),
            servo_source: None,
            servo_source_tag: "v0.0.4".to_string(),
            servo_source_rev: "b73ae02".to_string(),
            startup_url: "https://example.com".to_string(),
            enable_multiprocess: false,
            new_window_policy: "same-tab".to_string(),
            verbose_logging: false,
            render_mode: "cpu-readback".to_string(),
            webrender_backend: "gl".to_string(),
            frame_pacing: "vsync".to_string(),
            pixel_format: "rgba8".to_string(),
            alpha_mode: "straight".to_string(),
            color_space: "srgb".to_string(),
            debug_bypass_swizzle: false,
            debug_capture_next_frame: false,
            debug_capture_dir: "logs".to_string(),
            debug_pixel_probe: false,
            debug_pointer_overlay: false,
            devtools_enabled: false,
            devtools_transport: "none".to_string(),
            resource_limits: ResourceLimits::default(),
            security_warnings: true,
            profile_isolation: false,
            storage_policy: "profile-scoped".to_string(),
            service_worker_policy: "default".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ResourceLimits {
    pub memory_mb: u32,
    pub cpu_ms: u32,
    pub max_tabs: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_mb: 2048,
            cpu_ms: 200,
            max_tabs: 32,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DirectoryConfig {
    #[default]
    Default,
    Path(PathBuf),
}

impl Serialize for DirectoryConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Default => serializer.serialize_str("default"),
            Self::Path(path) => serializer.serialize_str(&path.display().to_string()),
        }
    }
}

impl<'de> Deserialize<'de> for DirectoryConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value == "default" {
            Ok(Self::Default)
        } else {
            Ok(Self::Path(PathBuf::from(value)))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DirectoryRootsConfig {
    pub data_dir: DirectoryConfig,
    pub logs_dir: DirectoryConfig,
    pub profiles_dir: DirectoryConfig,
    pub cache_dir: DirectoryConfig,
    pub downloads_dir: DirectoryConfig,
    pub crash_dumps_dir: DirectoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileConfig {
    pub active_profile: String,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            active_profile: "default".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    pub metadata_capture: bool,
    pub selective_body_capture: bool,
    pub archive_replay_mode: bool,
    pub max_entry_bytes: u64,
    pub third_party_mode: String,
    pub mime_allowlist: Vec<String>,
    pub host_allowlist: Vec<String>,
    pub host_denylist: Vec<String>,
    pub authenticated_only: bool,
    pub capture_html_json_css_js: bool,
    pub capture_media: bool,
    pub gc_max_entries: u32,
    pub storage_mode: String,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            metadata_capture: true,
            selective_body_capture: true,
            archive_replay_mode: false,
            max_entry_bytes: 10 * 1024 * 1024,
            third_party_mode: "metadata-only".to_string(),
            mime_allowlist: vec![
                "text/html".to_string(),
                "application/json".to_string(),
                "text/css".to_string(),
                "application/javascript".to_string(),
                "image/png".to_string(),
            ],
            host_allowlist: Vec::new(),
            host_denylist: Vec::new(),
            authenticated_only: false,
            capture_html_json_css_js: true,
            capture_media: true,
            gc_max_entries: 5000,
            storage_mode: "disk".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationConfig {
    pub enabled: bool,
    pub bind: String,
    pub expose_tab_api: bool,
    pub expose_cache_api: bool,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: "ws://127.0.0.1:7942".to_string(),
            expose_tab_api: true,
            expose_cache_api: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtractionConfig {
    pub article_processing_enabled: bool,
    pub ontology_capture_enabled: bool,
    pub rss_rehydration_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaConfig {
    pub default_tts_provider: String,
    pub auto_queue_reader_mode: bool,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            default_tts_provider: "none".to_string(),
            auto_queue_reader_mode: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FeatureFlags {
    pub shell_status_panel: bool,
    pub cache_inspector: bool,
    pub automation_server: bool,
    pub servo_backend: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            shell_status_panel: true,
            cache_inspector: true,
            automation_server: false,
            servo_backend: cfg!(feature = "servo"),
        }
    }
}
