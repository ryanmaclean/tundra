use std::path::PathBuf;

use crate::config::{Config, ConfigError};

/// Manages loading and saving settings to a TOML file on disk.
pub struct SettingsManager {
    path: PathBuf,
}

impl SettingsManager {
    /// Create a new `SettingsManager` that reads/writes the given file path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Create a `SettingsManager` using the default config location
    /// (`~/.config/auto-tundra/settings.toml`).
    pub fn default_path() -> Self {
        let path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("auto-tundra")
            .join("settings.toml");
        Self { path }
    }

    /// Load config from the TOML file on disk.
    pub fn load(&self) -> Result<Config, ConfigError> {
        let text =
            std::fs::read_to_string(&self.path).map_err(|e| ConfigError::Io(e.to_string()))?;
        let cfg: Config = toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Save config to the TOML file on disk, creating parent directories if
    /// they don't exist.
    pub fn save(&self, config: &Config) -> Result<(), ConfigError> {
        config.validate()?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ConfigError::Io(e.to_string()))?;
        }
        let text = config.to_toml()?;
        std::fs::write(&self.path, text).map_err(|e| ConfigError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load config from disk, falling back to `Config::default()` when the
    /// file is missing or unparseable.
    pub fn load_or_default(&self) -> Config {
        self.load().unwrap_or_default()
    }

    /// Return the file path this manager reads/writes.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_settings_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("at-settings-test-{}", uuid::Uuid::new_v4()));
        dir.join("settings.toml")
    }

    #[test]
    fn save_and_load_roundtrip() {
        let path = tmp_settings_path();
        let mgr = SettingsManager::new(&path);

        let mut cfg = Config::default();
        cfg.general.project_name = "roundtrip-test".into();
        cfg.display.theme = "light".into();
        cfg.terminal.font_size = 18;
        cfg.integrations.github_token_env = "MY_GH_TOKEN".into();
        cfg.integrations.github_owner = Some("my-org".into());
        cfg.integrations.github_repo = Some("my-repo".into());

        mgr.save(&cfg).unwrap();
        let loaded = mgr.load().unwrap();

        assert_eq!(loaded.general.project_name, "roundtrip-test");
        assert_eq!(loaded.display.theme, "light");
        assert_eq!(loaded.terminal.font_size, 18);
        assert_eq!(loaded.integrations.github_token_env, "MY_GH_TOKEN");
        assert_eq!(loaded.integrations.github_owner, Some("my-org".into()));
        assert_eq!(loaded.integrations.github_repo, Some("my-repo".into()));

        // cleanup
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn load_or_default_returns_default_on_missing_file() {
        let path = tmp_settings_path();
        let mgr = SettingsManager::new(&path);

        let cfg = mgr.load_or_default();
        assert_eq!(cfg.general.project_name, "auto-tundra");
        assert_eq!(cfg.display.font_size, 14);
    }

    #[test]
    fn load_missing_file_returns_error() {
        let path = tmp_settings_path();
        let mgr = SettingsManager::new(&path);

        let result = mgr.load();
        assert!(result.is_err());
    }

    #[test]
    fn partial_config_fills_defaults() {
        let path = tmp_settings_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"
[general]
project_name = "partial"
"#,
        )
        .unwrap();

        let mgr = SettingsManager::new(&path);
        let cfg = mgr.load().unwrap();

        assert_eq!(cfg.general.project_name, "partial");
        // All other fields should be defaults
        assert_eq!(cfg.display.theme, "dark");
        assert_eq!(cfg.terminal.font_family, "JetBrains Mono");
        assert_eq!(cfg.integrations.github_token_env, "GITHUB_TOKEN");

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_creates_parent_directories() {
        let path = tmp_settings_path();
        assert!(!path.parent().unwrap().exists());

        let mgr = SettingsManager::new(&path);
        mgr.save(&Config::default()).unwrap();

        assert!(path.exists());

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn defaults_are_correct() {
        let cfg = Config::default();
        assert_eq!(cfg.display.theme, "dark");
        assert_eq!(cfg.display.font_size, 14);
        assert!(!cfg.display.compact_mode);
        assert_eq!(cfg.terminal.font_family, "JetBrains Mono");
        assert_eq!(cfg.terminal.font_size, 14);
        assert_eq!(cfg.terminal.cursor_style, "block");
        assert_eq!(cfg.security.auto_lock_timeout_mins, 15);
        assert!(cfg.security.sandbox_mode);
        assert_eq!(cfg.security.active_execution_profile, "balanced");
        assert!(!cfg.security.execution_profiles.is_empty());
        assert_eq!(cfg.kanban.column_mode, "classic_8");
        assert_eq!(cfg.kanban.planning_poker.default_deck, "fibonacci");
        assert_eq!(cfg.integrations.github_token_env, "GITHUB_TOKEN");
        assert_eq!(cfg.integrations.gitlab_token_env, "GITLAB_TOKEN");
        assert_eq!(cfg.integrations.linear_api_key_env, "LINEAR_API_KEY");
        assert!(cfg.integrations.github_owner.is_none());
        assert!(cfg.integrations.github_repo.is_none());
    }

    #[test]
    fn overwrite_existing_settings() {
        let path = tmp_settings_path();
        let mgr = SettingsManager::new(&path);

        let cfg1 = Config::default();
        mgr.save(&cfg1).unwrap();

        let mut cfg2 = Config::default();
        cfg2.display.theme = "light".into();
        mgr.save(&cfg2).unwrap();

        let loaded = mgr.load().unwrap();
        assert_eq!(loaded.display.theme, "light");

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
