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
    assert_eq!(cfg.ui.theme, "dark");
    assert_eq!(cfg.bridge.transport, "unix");
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
}
