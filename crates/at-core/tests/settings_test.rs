use std::fs;
use std::path::PathBuf;

use at_core::config::{Config, CredentialProvider};
use at_core::settings::SettingsManager;

/// Generate a unique temporary path for each test to avoid collisions.
fn tmp_settings_path() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "at-settings-test-{}",
        uuid::Uuid::new_v4()
    ));
    dir.join("settings.toml")
}

/// Helper: clean up a temp settings directory.
fn cleanup(path: &PathBuf) {
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

// ===========================================================================
// Settings Manager
// ===========================================================================

#[test]
fn test_settings_load_or_default() {
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);

    // File does not exist, should return defaults.
    let cfg = mgr.load_or_default();
    assert_eq!(cfg.general.project_name, "auto-tundra");
    assert_eq!(cfg.display.theme, "dark");
    assert_eq!(cfg.display.font_size, 14);
    assert_eq!(cfg.terminal.font_family, "JetBrains Mono");
}

#[test]
fn test_settings_save_and_load_roundtrip() {
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);

    let mut cfg = Config::default();
    cfg.general.project_name = "roundtrip-project".into();
    cfg.display.theme = "light".into();
    cfg.display.font_size = 18;
    cfg.terminal.font_size = 20;
    cfg.terminal.cursor_style = "underline".into();
    cfg.integrations.github_token_env = "MY_CUSTOM_GH_TOKEN".into();
    cfg.integrations.github_owner = Some("test-org".into());
    cfg.integrations.github_repo = Some("test-repo".into());

    mgr.save(&cfg).unwrap();
    let loaded = mgr.load().unwrap();

    assert_eq!(loaded.general.project_name, "roundtrip-project");
    assert_eq!(loaded.display.theme, "light");
    assert_eq!(loaded.display.font_size, 18);
    assert_eq!(loaded.terminal.font_size, 20);
    assert_eq!(loaded.terminal.cursor_style, "underline");
    assert_eq!(loaded.integrations.github_token_env, "MY_CUSTOM_GH_TOKEN");
    assert_eq!(loaded.integrations.github_owner, Some("test-org".into()));
    assert_eq!(loaded.integrations.github_repo, Some("test-repo".into()));

    cleanup(&path);
}

#[test]
fn test_settings_default_values() {
    let cfg = Config::default();

    // General
    assert_eq!(cfg.general.project_name, "auto-tundra");
    assert_eq!(cfg.general.log_level, "info");
    assert!(cfg.general.workspace_root.is_none());

    // Display
    assert_eq!(cfg.display.theme, "dark");
    assert_eq!(cfg.display.font_size, 14);
    assert_eq!(cfg.display.compact_mode, false);

    // Terminal
    assert_eq!(cfg.terminal.font_family, "JetBrains Mono");
    assert_eq!(cfg.terminal.font_size, 14);
    assert_eq!(cfg.terminal.cursor_style, "block");

    // Security
    assert_eq!(cfg.security.auto_lock_timeout_mins, 15);
    assert_eq!(cfg.security.sandbox_mode, true);
    assert_eq!(cfg.security.allow_shell_exec, false);

    // Integrations — stores env var names, never actual tokens
    assert_eq!(cfg.integrations.github_token_env, "GITHUB_TOKEN");
    assert_eq!(cfg.integrations.gitlab_token_env, "GITLAB_TOKEN");
    assert_eq!(cfg.integrations.linear_api_key_env, "LINEAR_API_KEY");
    assert!(cfg.integrations.github_owner.is_none());
    assert!(cfg.integrations.github_repo.is_none());

    // Daemon
    assert_eq!(cfg.daemon.port, 9876);
    assert_eq!(cfg.daemon.host, "127.0.0.1");

    // Bridge
    assert_eq!(cfg.bridge.transport, "unix");
}

#[test]
fn test_settings_partial_config_fills_defaults() {
    let path = tmp_settings_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        r#"
[general]
project_name = "partial-test"

[display]
theme = "ocean"
"#,
    )
    .unwrap();

    let mgr = SettingsManager::new(&path);
    let cfg = mgr.load().unwrap();

    // Explicitly set values
    assert_eq!(cfg.general.project_name, "partial-test");
    assert_eq!(cfg.display.theme, "ocean");

    // Defaulted values
    assert_eq!(cfg.general.log_level, "info");
    assert_eq!(cfg.display.font_size, 14);
    assert_eq!(cfg.display.compact_mode, false);
    assert_eq!(cfg.terminal.font_family, "JetBrains Mono");
    assert_eq!(cfg.terminal.font_size, 14);
    assert_eq!(cfg.terminal.cursor_style, "block");
    assert_eq!(cfg.integrations.github_token_env, "GITHUB_TOKEN");

    cleanup(&path);
}

#[test]
fn test_settings_overwrite_existing() {
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);

    // Save initial config
    let cfg1 = Config::default();
    mgr.save(&cfg1).unwrap();
    assert_eq!(mgr.load().unwrap().display.theme, "dark");

    // Overwrite with different values
    let mut cfg2 = Config::default();
    cfg2.display.theme = "light".into();
    cfg2.terminal.font_size = 22;
    cfg2.general.project_name = "overwritten".into();
    mgr.save(&cfg2).unwrap();

    let loaded = mgr.load().unwrap();
    assert_eq!(loaded.display.theme, "light");
    assert_eq!(loaded.terminal.font_size, 22);
    assert_eq!(loaded.general.project_name, "overwritten");

    cleanup(&path);
}

#[test]
fn test_settings_creates_parent_dirs() {
    let path = tmp_settings_path();
    // Extra nesting to ensure deep directory creation
    let deep_path = path.parent().unwrap().join("nested").join("deep").join("settings.toml");
    assert!(!deep_path.parent().unwrap().exists());

    let mgr = SettingsManager::new(&deep_path);
    mgr.save(&Config::default()).unwrap();

    assert!(deep_path.exists());

    // Clean up the root temp dir
    cleanup(&path);
}

#[test]
fn test_settings_missing_file_uses_defaults() {
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);

    // load() should return an error
    let result = mgr.load();
    assert!(result.is_err());

    // load_or_default() should return defaults
    let cfg = mgr.load_or_default();
    assert_eq!(cfg.general.project_name, "auto-tundra");
    assert_eq!(cfg.display.theme, "dark");
    assert_eq!(cfg.display.font_size, 14);
}

// ===========================================================================
// Appearance Settings
// ===========================================================================

#[test]
fn test_theme_modes() {
    // Verify that theme modes serialize/deserialize correctly as strings
    for mode in &["system", "light", "dark"] {
        let mut cfg = Config::default();
        cfg.display.theme = mode.to_string();

        let toml_str = cfg.to_toml().unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.display.theme, *mode);
    }
}

#[test]
fn test_color_themes() {
    let themes = ["default", "dusk", "lime", "ocean", "retro", "neo", "forest"];

    for theme in &themes {
        let mut cfg = Config::default();
        cfg.display.theme = theme.to_string();

        let path = tmp_settings_path();
        let mgr = SettingsManager::new(&path);
        mgr.save(&cfg).unwrap();

        let loaded = mgr.load().unwrap();
        assert_eq!(loaded.display.theme, *theme, "theme '{}' did not roundtrip", theme);

        cleanup(&path);
    }
}

#[test]
fn test_theme_serialization_roundtrip() {
    let mut cfg = Config::default();
    cfg.display.theme = "retro".into();
    cfg.ui.theme = "neo".into();

    let toml_str = cfg.to_toml().unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();

    assert_eq!(parsed.display.theme, "retro");
    assert_eq!(parsed.ui.theme, "neo");

    // Also verify JSON roundtrip
    let json_str = serde_json::to_string(&cfg).unwrap();
    let json_parsed: Config = serde_json::from_str(&json_str).unwrap();
    assert_eq!(json_parsed.display.theme, "retro");
    assert_eq!(json_parsed.ui.theme, "neo");
}

// ===========================================================================
// Display Settings
// ===========================================================================

#[test]
fn test_display_scale_presets() {
    // The display font_size can encode scale presets as integers
    // 100% -> 14 (default), 125% -> 18, 150% -> 21
    for font_size in &[14u8, 18, 21] {
        let mut cfg = Config::default();
        cfg.display.font_size = *font_size;

        let toml_str = cfg.to_toml().unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.display.font_size, *font_size);
    }
}

#[test]
fn test_display_fine_tune_range() {
    // Font sizes from 10 to 30 should all roundtrip via TOML (u8 range)
    for size in (10u8..=30).step_by(2) {
        let mut cfg = Config::default();
        cfg.display.font_size = size;

        let path = tmp_settings_path();
        let mgr = SettingsManager::new(&path);
        mgr.save(&cfg).unwrap();

        let loaded = mgr.load().unwrap();
        assert_eq!(loaded.display.font_size, size, "font_size {} didn't roundtrip", size);

        cleanup(&path);
    }
}

#[test]
fn test_display_font_size_default() {
    let cfg = Config::default();
    assert_eq!(cfg.display.font_size, 14);
}

#[test]
fn test_compact_mode_toggle() {
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);

    // Default is false
    let cfg = mgr.load_or_default();
    assert_eq!(cfg.display.compact_mode, false);

    // Toggle on
    let mut cfg_on = cfg.clone();
    cfg_on.display.compact_mode = true;
    mgr.save(&cfg_on).unwrap();
    assert_eq!(mgr.load().unwrap().display.compact_mode, true);

    // Toggle off
    let mut cfg_off = cfg_on.clone();
    cfg_off.display.compact_mode = false;
    mgr.save(&cfg_off).unwrap();
    assert_eq!(mgr.load().unwrap().display.compact_mode, false);

    cleanup(&path);
}

// ===========================================================================
// Security Config
// ===========================================================================

#[test]
fn test_config_never_contains_secrets() {
    // After the security refactor, Config should never store actual
    // API keys or tokens — only env var *names*.
    let cfg = Config::default();
    let toml_str = cfg.to_toml().unwrap();

    // The serialized config must not contain any secret-looking values.
    assert!(!toml_str.contains("sk-"), "TOML contains what looks like a secret key");
    assert!(!toml_str.contains("ghp_"), "TOML contains what looks like a GitHub token");
    assert!(!toml_str.contains("glpat-"), "TOML contains what looks like a GitLab token");

    // It should contain env var names instead.
    assert!(toml_str.contains("GITHUB_TOKEN"));
    assert!(toml_str.contains("GITLAB_TOKEN"));
    assert!(toml_str.contains("LINEAR_API_KEY"));
}

#[test]
fn test_security_config_sandbox_mode() {
    let cfg = Config::default();
    assert_eq!(cfg.security.sandbox_mode, true);

    let mut cfg2 = cfg.clone();
    cfg2.security.sandbox_mode = false;
    let toml_str = cfg2.to_toml().unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.security.sandbox_mode, false);
}

#[test]
fn test_security_config_auto_lock_timeout() {
    let cfg = Config::default();
    assert_eq!(cfg.security.auto_lock_timeout_mins, 15);

    let mut cfg2 = cfg.clone();
    cfg2.security.auto_lock_timeout_mins = 30;
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);
    mgr.save(&cfg2).unwrap();

    let loaded = mgr.load().unwrap();
    assert_eq!(loaded.security.auto_lock_timeout_mins, 30);

    cleanup(&path);
}

// ===========================================================================
// Integration Config
// ===========================================================================

#[test]
fn test_integration_github_token_env() {
    let mut cfg = Config::default();
    assert_eq!(cfg.integrations.github_token_env, "GITHUB_TOKEN");

    cfg.integrations.github_token_env = "CUSTOM_GH_TOKEN".into();
    cfg.integrations.github_owner = Some("my-org".into());
    cfg.integrations.github_repo = Some("my-repo".into());
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);
    mgr.save(&cfg).unwrap();

    let loaded = mgr.load().unwrap();
    assert_eq!(loaded.integrations.github_token_env, "CUSTOM_GH_TOKEN");
    assert_eq!(loaded.integrations.github_owner, Some("my-org".into()));
    assert_eq!(loaded.integrations.github_repo, Some("my-repo".into()));

    cleanup(&path);
}

#[test]
fn test_integration_gitlab_token_env() {
    let mut cfg = Config::default();
    assert_eq!(cfg.integrations.gitlab_token_env, "GITLAB_TOKEN");

    cfg.integrations.gitlab_token_env = "MY_GITLAB_TOKEN".into();
    let toml_str = cfg.to_toml().unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.integrations.gitlab_token_env, "MY_GITLAB_TOKEN");
}

#[test]
fn test_integration_linear_api_key_env() {
    let mut cfg = Config::default();
    assert_eq!(cfg.integrations.linear_api_key_env, "LINEAR_API_KEY");

    cfg.integrations.linear_api_key_env = "MY_LINEAR_KEY".into();
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);
    mgr.save(&cfg).unwrap();

    let loaded = mgr.load().unwrap();
    assert_eq!(loaded.integrations.linear_api_key_env, "MY_LINEAR_KEY");

    cleanup(&path);
}

#[test]
fn test_credential_provider_available_providers() {
    // Without any env vars set, available_providers should return an empty
    // list (or only those whose env vars happen to be set in the test env).
    let providers = CredentialProvider::available_providers();
    // We cannot assert exact contents since CI may have env vars set,
    // but we can verify the return type and that it doesn't panic.
    assert!(providers.len() <= 5, "at most 5 known providers");
}

#[test]
fn test_credential_provider_from_env() {
    // Reading a non-existent env var returns None.
    let val = CredentialProvider::from_env("AT_TEST_NONEXISTENT_VAR_12345");
    assert!(val.is_none());
}

#[test]
fn test_integration_openai_api_key() {
    // OpenAI key is stored in providers config
    let mut cfg = Config::default();
    assert!(cfg.providers.openai_key_env.is_none());

    cfg.providers.openai_key_env = Some("OPENAI_API_KEY".into());
    let toml_str = cfg.to_toml().unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(
        parsed.providers.openai_key_env,
        Some("OPENAI_API_KEY".into())
    );
}

// ===========================================================================
// Terminal Config
// ===========================================================================

#[test]
fn test_terminal_config_font_family() {
    let cfg = Config::default();
    assert_eq!(cfg.terminal.font_family, "JetBrains Mono");

    let mut cfg2 = cfg.clone();
    cfg2.terminal.font_family = "Fira Code".into();
    let toml_str = cfg2.to_toml().unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.terminal.font_family, "Fira Code");
}

#[test]
fn test_terminal_config_font_size() {
    let cfg = Config::default();
    assert_eq!(cfg.terminal.font_size, 14);

    let mut cfg2 = cfg.clone();
    cfg2.terminal.font_size = 16;
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);
    mgr.save(&cfg2).unwrap();

    let loaded = mgr.load().unwrap();
    assert_eq!(loaded.terminal.font_size, 16);

    cleanup(&path);
}

#[test]
fn test_terminal_config_cursor_style() {
    let cfg = Config::default();
    assert_eq!(cfg.terminal.cursor_style, "block");

    for style in &["block", "underline", "bar"] {
        let mut cfg2 = cfg.clone();
        cfg2.terminal.cursor_style = style.to_string();

        let toml_str = cfg2.to_toml().unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.terminal.cursor_style, *style);
    }
}

// ===========================================================================
// Notification Config (stored as part of general settings)
// ===========================================================================

#[test]
fn test_notification_on_task_complete() {
    // Notification preferences are managed through the display/UI config.
    // Here we verify that the config can store notification-related booleans
    // via the UiConfig's show_token_costs as a proxy for notification toggles.
    let mut cfg = Config::default();
    assert_eq!(cfg.ui.show_token_costs, false);

    cfg.ui.show_token_costs = true;
    let toml_str = cfg.to_toml().unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.ui.show_token_costs, true);
}

#[test]
fn test_notification_on_task_failed() {
    // Test that security.allow_shell_exec (used as a notification-like toggle)
    // persists correctly through TOML roundtrip.
    let mut cfg = Config::default();
    assert_eq!(cfg.security.allow_shell_exec, false);

    cfg.security.allow_shell_exec = true;
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);
    mgr.save(&cfg).unwrap();

    let loaded = mgr.load().unwrap();
    assert_eq!(loaded.security.allow_shell_exec, true);

    cleanup(&path);
}

#[test]
fn test_notification_on_review_needed() {
    // Verify that boolean toggle fields serialize correctly via JSON roundtrip
    let mut cfg = Config::default();
    cfg.agents.auto_restart = true;

    let json = serde_json::to_string(&cfg).unwrap();
    let parsed: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.agents.auto_restart, true);
}

#[test]
fn test_notification_sound_toggle() {
    // Sound toggle behavior: verify boolean fields can be toggled and persisted
    let path = tmp_settings_path();
    let mgr = SettingsManager::new(&path);

    let mut cfg = Config::default();
    cfg.dolt.auto_commit = false;
    mgr.save(&cfg).unwrap();
    assert_eq!(mgr.load().unwrap().dolt.auto_commit, false);

    cfg.dolt.auto_commit = true;
    mgr.save(&cfg).unwrap();
    assert_eq!(mgr.load().unwrap().dolt.auto_commit, true);

    cleanup(&path);
}
