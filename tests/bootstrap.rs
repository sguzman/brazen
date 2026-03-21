use brazen::commands::{AppCommand, CommandOutcome, dispatch_command};
use brazen::config::{BrazenConfig, LoggingConfig, default_config_toml};
use brazen::engine::{BrowserEngine, BrowserTab, EngineEvent, EngineStatus, NullEngine};
use brazen::logging::{LoggingPlan, init_tracing};
use brazen::permissions::{Capability, PermissionDecision};
use brazen::platform_paths::PlatformPaths;
use brazen::{BootstrapOptions, EngineFactory, bootstrap};
use tempfile::tempdir;

struct TestFactory;

impl EngineFactory for TestFactory {
    fn create(
        &self,
        _config: &BrazenConfig,
        _paths: &brazen::RuntimePaths,
    ) -> Box<dyn BrowserEngine> {
        Box::new(NullEngine::new())
    }
}

#[test]
fn default_config_contains_capability_sections() {
    let config = default_config_toml();
    assert!(config.contains("[permissions.capabilities]"));
    assert!(config.contains("[automation]"));
}

#[test]
fn config_defaults_merge_with_partial_overrides() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brazen.toml");
    std::fs::write(
        &path,
        r#"
[window]
initial_width = 1600.0

[permissions.capabilities]
terminal-exec = "ask"
"#,
    )
    .unwrap();

    let config = BrazenConfig::load_with_defaults(&path).unwrap();
    assert_eq!(config.window.initial_width, 1600.0);
    assert_eq!(config.window.initial_height, 920.0);
    assert_eq!(
        config.permissions.decision_for(&Capability::TerminalExec),
        PermissionDecision::Ask
    );
}

#[test]
fn invalid_config_is_rejected() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("brazen.toml");
    std::fs::write(
        &path,
        r#"
[window]
initial_width = 320.0
"#,
    )
    .unwrap();

    let error = BrazenConfig::load_with_defaults(&path).unwrap_err();
    assert!(error.to_string().contains("initial_width"));
}

#[test]
fn runtime_paths_resolve_relative_to_config_directory() {
    let roots = PlatformPaths::from_roots(
        "/tmp/brazen-config",
        "/tmp/brazen-data",
        "/tmp/brazen-cache",
    );
    let config = BrazenConfig::default();
    let config_path = std::path::PathBuf::from("/workspace/settings/brazen.toml");

    let runtime = roots.resolve_runtime_paths(&config, &config_path).unwrap();

    assert_eq!(
        runtime.data_dir,
        std::path::PathBuf::from("/tmp/brazen-data")
    );
    assert_eq!(
        runtime.logs_dir,
        std::path::PathBuf::from("/tmp/brazen-data")
    );
    assert_eq!(
        runtime.cache_dir,
        std::path::PathBuf::from("/tmp/brazen-cache")
    );
}

#[test]
fn logging_plan_is_derived_from_config() {
    let config = LoggingConfig {
        console_filter: "warn,brazen=trace".to_string(),
        file_filter: "debug".to_string(),
        file_name_prefix: "custom.log".to_string(),
        ansi: false,
    };

    let plan = LoggingPlan::from_config(&config);
    assert_eq!(plan.console_filter, "warn,brazen=trace");
    assert_eq!(plan.file_filter, "debug");
    assert_eq!(plan.file_name_prefix, "custom.log");
}

#[test]
fn tracing_init_is_idempotent() {
    let dir = tempdir().unwrap();
    let config = LoggingConfig::default();

    init_tracing(&config, dir.path()).unwrap();
    init_tracing(&config, dir.path()).unwrap();
}

#[test]
fn command_dispatch_routes_navigation_and_panel_state() {
    let dir = tempdir().unwrap();
    let runtime_paths = brazen::RuntimePaths {
        config_path: dir.path().join("brazen.toml"),
        data_dir: dir.path().join("data"),
        logs_dir: dir.path().join("logs"),
        profiles_dir: dir.path().join("profiles"),
        cache_dir: dir.path().join("cache"),
    };
    let mut shell = brazen::ShellState {
        app_name: "Brazen".to_string(),
        backend_name: "null".to_string(),
        engine_instance_id: 1,
        engine_status: EngineStatus::NoEngine,
        active_tab: BrowserTab {
            id: 1,
            title: "Platform Skeleton".to_string(),
            current_url: "about:blank".to_string(),
        },
        address_bar_input: String::new(),
        page_title: "Platform Skeleton".to_string(),
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
        runtime_paths,
    };
    let mut engine = NullEngine::new();

    let outcome = dispatch_command(
        &mut shell,
        &mut engine,
        AppCommand::NavigateTo("https://example.com".to_string()),
    );
    assert_eq!(outcome, CommandOutcome::NavigationScheduled);
    assert_eq!(engine.active_tab().current_url, "https://example.com");

    let outcome = dispatch_command(&mut shell, &mut engine, AppCommand::GoBack);
    assert_eq!(outcome, CommandOutcome::BackScheduled);

    let outcome = dispatch_command(&mut shell, &mut engine, AppCommand::GoForward);
    assert_eq!(outcome, CommandOutcome::ForwardScheduled);

    let outcome = dispatch_command(&mut shell, &mut engine, AppCommand::ToggleLogPanel);
    assert_eq!(outcome, CommandOutcome::LogPanelVisibility(false));

    let outcome = dispatch_command(&mut shell, &mut engine, AppCommand::OpenPermissionPanel);
    assert_eq!(outcome, CommandOutcome::PermissionPanelVisibility(true));
}

#[test]
fn bootstrap_starts_with_default_config_and_custom_path() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("brazen.toml");
    std::fs::write(
        &config_path,
        r#"
[app]
name = "Brazen Test"
"#,
    )
    .unwrap();

    let bootstrap = bootstrap(BootstrapOptions::from_path(&config_path), &TestFactory).unwrap();
    assert_eq!(bootstrap.config.app.name, "Brazen Test");
    assert_eq!(bootstrap.paths.config_path, config_path);
    assert!(
        bootstrap
            .shell_state
            .event_log
            .iter()
            .any(|line| line.contains("loaded config"))
    );
}

#[test]
fn bootstrap_surfaces_invalid_configuration_errors() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("brazen.toml");
    std::fs::write(
        &config_path,
        r#"
[automation]
enabled = true
bind = "http://127.0.0.1:9999"
"#,
    )
    .unwrap();

    let error = bootstrap(BootstrapOptions::from_path(&config_path), &TestFactory).unwrap_err();
    assert!(error.to_string().contains("automation.bind"));
}

#[test]
fn null_engine_emits_navigation_event() {
    let mut engine = NullEngine::new();
    engine.navigate("https://example.com");
    let events = engine.take_events();
    assert!(events.iter().any(|event| {
        matches!(
            event,
            EngineEvent::NavigationRequested(url) if url == "https://example.com"
        )
    }));
    assert!(
        events
            .iter()
            .any(|event| { matches!(event, EngineEvent::NavigationStateUpdated(_)) })
    );
}
