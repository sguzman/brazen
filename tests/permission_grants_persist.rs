use brazen::config::BrazenConfig;
use brazen::permissions::{Capability, PermissionDecision};
use brazen::profile_db::ProfileDb;
use brazen::PlatformPaths;

#[test]
fn persisted_permission_grants_override_default_policy() {
    let dir = tempfile::tempdir().unwrap();
    let platform = PlatformPaths::from_roots(
        dir.path().join("config"),
        dir.path().join("data"),
        dir.path().join("cache"),
    );
    let config_path = platform.default_config_path();
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    brazen::write_default_config(&config_path).unwrap();

    let mut config = BrazenConfig::load_with_defaults(&config_path).unwrap();
    assert_eq!(
        config.permissions.decision_for_domain("example.com", &Capability::TerminalExec),
        PermissionDecision::Deny
    );

    let paths = platform.resolve_runtime_paths(&config, &config_path).unwrap();
    std::fs::create_dir_all(&paths.active_profile_dir).unwrap();
    let db = ProfileDb::open(paths.active_profile_dir.join("state.sqlite")).unwrap();
    db.upsert_permission_grant(
        "example.com",
        Capability::TerminalExec,
        PermissionDecision::Allow,
        "now",
    )
    .unwrap();

    // Simulate bootstrap merge.
    let grants = db.load_permission_grants().unwrap();
    for (domain, overrides) in grants {
        let entry = config.permissions.domain_overrides.entry(domain).or_default();
        for (capability, decision) in overrides {
            entry.insert(capability, decision);
        }
    }

    assert_eq!(
        config.permissions.decision_for_domain("example.com", &Capability::TerminalExec),
        PermissionDecision::Allow
    );
}

