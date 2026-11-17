use at_core::config::Config;

#[test]
fn default_config() {
    let cfg = Config::default();
    assert_eq!(cfg.general.project_name, "auto-tundra");
    assert_eq!(cfg.general.log_level, "info");
    assert_eq!(cfg.dolt.port, 3306);
    assert_eq!(cfg.cache.max_size_mb, 256);
    assert_eq!(cfg.agents.max_concurrent, 8);
    assert_eq!(cfg.daemon.port, 9876);
    assert_eq!(cfg.daemon.host, "127.0.0.1");
    assert!(cfg.security.sandbox);
    assert!(!cfg.security.allow_shell_exec);
    assert_eq!(cfg.security.active_execution_profile, "balanced");
    assert!(!cfg.security.execution_profiles.is_empty());
    assert_eq!(cfg.ui.theme, "dark");
    assert_eq!(cfg.bridge.transport, "unix");
    assert_eq!(cfg.kanban.column_mode, "classic_8");
    assert_eq!(cfg.kanban.planning_poker.default_deck, "fibonacci");
}

#[test]
fn config_roundtrip() {
    let cfg = Config::default();
    let toml_str = cfg.to_toml().expect("serialize to toml");
    assert!(toml_str.contains("auto-tundra"));

    let parsed: Config = toml::from_str(&toml_str).expect("parse toml back");
    assert_eq!(parsed.general.project_name, cfg.general.project_name);
    assert_eq!(parsed.daemon.port, cfg.daemon.port);
    assert_eq!(parsed.cache.max_size_mb, cfg.cache.max_size_mb);
    assert_eq!(parsed.bridge.buffer_size, cfg.bridge.buffer_size);
    parsed.validate().expect("config validates");
}

#[test]
fn historical_documented_config_roundtrips() {
    // The complete example published in docs at commit b88bb7b (2026-03-01).
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/config-reference-2026-03-01.toml");
    let original = std::fs::read_to_string(&path).expect("read historical configuration");
    let legacy: Config = toml_legacy::from_str(&original).expect("parse with TOML 0.8");
    let cfg = Config::load_from(path).expect("load historical configuration with TOML 0.9");
    assert_eq!(
        serde_json::to_value(&legacy).unwrap(),
        serde_json::to_value(&cfg).unwrap()
    );
    assert_eq!(cfg.general.project_name, "auto-tundra");
    assert_eq!(cfg.daemon.port, 9090);
    assert_eq!(cfg.cache.max_size_mb, 512);
    assert_eq!(cfg.integrations.github_owner.as_deref(), Some("your-org"));

    let saved = cfg.to_toml().expect("serialize historical configuration");
    let restored: Config = toml::from_str(&saved).expect("reload serialized configuration");
    assert_eq!(
        serde_json::to_value(&cfg).unwrap(),
        serde_json::to_value(&restored).unwrap()
    );
}

#[test]
fn historical_environment_configs_match_legacy_parser() {
    // Development, staging, and production examples from docs at b88bb7b.
    for (text, name) in [
        (
            include_str!("fixtures/config-development-2026-03-01.toml"),
            "auto-tundra-dev",
        ),
        (
            include_str!("fixtures/config-staging-2026-03-01.toml"),
            "auto-tundra-staging",
        ),
        (
            include_str!("fixtures/config-production-2026-03-01.toml"),
            "auto-tundra-prod",
        ),
    ] {
        let legacy: Config = toml_legacy::from_str(text).expect("parse with TOML 0.8");
        let current: Config = toml::from_str(text).expect("parse with TOML 0.9");
        assert_eq!(current.general.project_name, name);
        assert_eq!(
            serde_json::to_value(&legacy).unwrap(),
            serde_json::to_value(&current).unwrap()
        );
        current.validate().expect("validate historical configuration");
        let saved = current.to_toml().expect("serialize historical configuration");
        let restored: Config = toml::from_str(&saved).expect("reload serialized configuration");
        assert_eq!(
            serde_json::to_value(&current).unwrap(),
            serde_json::to_value(&restored).unwrap()
        );
    }
}

#[test]
fn config_partial_toml() {
    let partial = r#"
[general]
project_name = "my-project"

[daemon]
port = 1234
"#;
    let cfg: Config = toml::from_str(partial).expect("parse partial");
    assert_eq!(cfg.general.project_name, "my-project");
    assert_eq!(cfg.daemon.port, 1234);
    // defaults should fill in the rest
    assert_eq!(cfg.general.log_level, "info");
    assert_eq!(cfg.cache.max_size_mb, 256);
    cfg.validate().expect("config validates");
}

#[test]
fn invalid_planning_poker_deck_fails_validation() {
    let mut cfg = Config::default();
    cfg.kanban.planning_poker.default_deck = "invalid".to_string();
    let err = cfg.validate().expect_err("validation should fail");
    assert!(err.to_string().contains("default_deck"));
}

#[test]
fn invalid_security_profile_fails_validation() {
    let mut cfg = Config::default();
    cfg.security.active_execution_profile = "does-not-exist".to_string();
    let err = cfg.validate().expect_err("validation should fail");
    assert!(err.to_string().contains("active_execution_profile"));
}
