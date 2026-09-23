use std::path::{Path, PathBuf};

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

    /// Create a `SettingsManager` using the canonical config location
    /// ([`Config::default_path`], `~/.auto-tundra/config.toml`), the same
    /// file the daemon reads at startup.
    ///
    /// If that file does not exist yet but the legacy settings file
    /// (`~/.config/auto-tundra/settings.toml`) does, the legacy file is
    /// copied over once so settings saved by older builds are not lost.
    pub fn default_path() -> Self {
        let path = Self::canonical_path();
        if let Some(legacy) = Self::legacy_path() {
            Self::migrate_legacy(&legacy, &path);
        }
        Self { path }
    }

    /// The file [`default_path`](Self::default_path) manages, without the
    /// legacy-migration side effect.
    fn canonical_path() -> PathBuf {
        Config::default_path()
    }

    /// Location used by older builds for settings saved through the API.
    fn legacy_path() -> Option<PathBuf> {
        crate::paths::home_dir().map(|h| h.join(".config").join("auto-tundra").join("settings.toml"))
    }

    /// Copy `legacy` to `target` when `target` is missing and `legacy`
    /// exists. Returns `true` if a migration happened. Failures are logged
    /// and never fatal: the caller then simply sees no file at `target`.
    fn migrate_legacy(legacy: &Path, target: &Path) -> bool {
        if target.exists() || !legacy.is_file() {
            return false;
        }
        let result = target
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::copy(legacy, target).map(|_| ()));
        match result {
            Ok(()) => {
                tracing::warn!(
                    from = %legacy.display(),
                    to = %target.display(),
                    "migrated legacy settings file to canonical config path; \
                     the legacy file is no longer read"
                );
                true
            }
            Err(e) => {
                tracing::warn!(
                    from = %legacy.display(),
                    to = %target.display(),
                    error = %e,
                    "failed to migrate legacy settings file"
                );
                false
            }
        }
    }

    /// Load config from the TOML file on disk.
    ///
    /// Errors if the file is missing, unreadable, unparseable or invalid.
    pub fn load(&self) -> Result<Config, ConfigError> {
        let text =
            std::fs::read_to_string(&self.path).map_err(|e| ConfigError::Io(e.to_string()))?;
        Self::parse(&text)
    }

    /// Load config for a read-modify-write update.
    ///
    /// Returns `Config::default()` **only** when the file does not exist.
    /// An unreadable, unparseable or semantically invalid file is an error:
    /// callers must not merge into defaults and save, because that would
    /// overwrite the user's settings with defaults.
    pub fn load_for_update(&self) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ConfigError::Io(format!("{}: {e}", self.path.display()))),
        }
    }

    fn parse(text: &str) -> Result<Config, ConfigError> {
        let cfg: Config = toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Save config to the TOML file on disk, creating parent directories if
    /// they don't exist.
    ///
    /// The write is atomic: the TOML is written to a temporary file in the
    /// same directory, fsynced, then renamed over the target, so a crash or
    /// full disk never leaves a truncated settings file. If the settings path
    /// is a symlink, the link's target is replaced and the link is kept.
    pub fn save(&self, config: &Config) -> Result<(), ConfigError> {
        use std::io::Write as _;

        config.validate()?;
        let text = config.to_toml()?;
        let io_err = |what: &str, e: std::io::Error| ConfigError::Io(format!("{what}: {e}"));

        // Write through a symlink (e.g. dotfile-managed settings) instead of
        // replacing the link itself.
        let target = match std::fs::symlink_metadata(&self.path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                std::fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone())
            }
            _ => self.path.clone(),
        };
        let dir = target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&dir).map_err(|e| io_err("create settings dir", e))?;

        let file_name = target
            .file_name()
            .ok_or_else(|| ConfigError::Io(format!("invalid settings path: {}", target.display())))?
            .to_string_lossy()
            .into_owned();
        let tmp = dir.join(format!(
            ".{file_name}.tmp-{}",
            uuid::Uuid::new_v4().simple()
        ));

        let write_tmp = || -> std::io::Result<()> {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
            // Keep the existing file's permissions (e.g. 0600).
            if let Ok(meta) = std::fs::metadata(&target) {
                std::fs::set_permissions(&tmp, meta.permissions())?;
            }
            Ok(())
        };
        if let Err(e) = write_tmp() {
            let _ = std::fs::remove_file(&tmp);
            return Err(io_err("write temp settings file", e));
        }
        if let Err(e) = std::fs::rename(&tmp, &target) {
            let _ = std::fs::remove_file(&tmp);
            return Err(io_err("replace settings file", e));
        }
        // Persist the rename itself (best effort).
        #[cfg(unix)]
        if let Ok(d) = std::fs::File::open(&dir) {
            let _ = d.sync_all();
        }
        Ok(())
    }

    /// Load config from disk, falling back to `Config::default()` when the
    /// file is missing, unreadable or invalid.
    ///
    /// For **read-only** callers only: never merge into and `save()` the
    /// result (use [`load_for_update`](Self::load_for_update)), or an invalid
    /// file would be replaced with defaults. A present-but-invalid file is
    /// logged as a warning.
    pub fn load_or_default(&self) -> Config {
        match self.load_for_update() {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!(
                    path = %self.path.display(),
                    error = %e,
                    "settings file is invalid; using defaults for this read"
                );
                Config::default()
            }
        }
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

    #[test]
    fn default_path_is_home_dot_config_settings_toml() {
        let mgr = SettingsManager::default_path();
        let home = crate::paths::home_dir().unwrap_or_else(|| PathBuf::from("."));
        assert_eq!(
            mgr.path(),
            &home.join(".config").join("auto-tundra").join("settings.toml")
        );
    }

    fn tmp_settings_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("at-settings-test-{}", uuid::Uuid::new_v4()));
        dir.join("settings.toml")
    }

    #[test]
    fn default_path_is_the_daemon_config_file() {
        // The settings API and the daemon must share one file.
        // (`canonical_path` avoids touching the real home directory in tests.)
        assert_eq!(SettingsManager::canonical_path(), Config::default_path());
        assert!(Config::default_path().ends_with(".auto-tundra/config.toml"));
    }

    #[test]
    fn legacy_settings_are_migrated_once() {
        let legacy = tmp_settings_path();
        let target = tmp_settings_path().with_file_name("config.toml");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, "[general]\nproject_name = \"legacy\"\n").unwrap();

        assert!(SettingsManager::migrate_legacy(&legacy, &target));
        let cfg = SettingsManager::new(&target).load().unwrap();
        assert_eq!(cfg.general.project_name, "legacy");

        // An existing canonical file is never overwritten.
        fs::write(&legacy, "[general]\nproject_name = \"newer-legacy\"\n").unwrap();
        assert!(!SettingsManager::migrate_legacy(&legacy, &target));
        let cfg = SettingsManager::new(&target).load().unwrap();
        assert_eq!(cfg.general.project_name, "legacy");

        let _ = fs::remove_dir_all(legacy.parent().unwrap());
        let _ = fs::remove_dir_all(target.parent().unwrap());
    }

    #[test]
    fn migrate_legacy_is_noop_without_legacy_file() {
        let legacy = tmp_settings_path();
        let target = tmp_settings_path();
        assert!(!SettingsManager::migrate_legacy(&legacy, &target));
        assert!(!target.exists());
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

    #[test]
    fn load_for_update_defaults_only_when_missing() {
        let path = tmp_settings_path();
        let mgr = SettingsManager::new(&path);
        let cfg = mgr.load_for_update().unwrap();
        assert_eq!(cfg.general.project_name, "auto-tundra");
    }

    #[test]
    fn invalid_file_is_an_error_and_is_never_overwritten_with_defaults() {
        let path = tmp_settings_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Type error: font_size must be an integer.
        let original = "[general]\nproject_name = \"mine\"\n\n[display]\nfont_size = \"14\"\n";
        fs::write(&path, original).unwrap();
        let mgr = SettingsManager::new(&path);

        assert!(matches!(mgr.load_for_update(), Err(ConfigError::Parse(_))));

        // Semantic error: unsupported deck.
        let invalid_deck = "[general]\nproject_name = \"mine\"\n\n[kanban.planning_poker]\ndefault_deck = \"fib\"\n";
        fs::write(&path, invalid_deck).unwrap();
        assert!(matches!(
            mgr.load_for_update(),
            Err(ConfigError::Validation(_))
        ));

        // The file on disk is untouched by the failed loads.
        assert_eq!(fs::read_to_string(&path).unwrap(), invalid_deck);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temp_files() {
        let path = tmp_settings_path();
        let mgr = SettingsManager::new(&path);
        let mut cfg = Config::default();
        cfg.general.project_name = "atomic".into();
        mgr.save(&cfg).unwrap();
        cfg.general.project_name = "atomic-2".into();
        mgr.save(&cfg).unwrap();

        assert_eq!(mgr.load().unwrap().general.project_name, "atomic-2");
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["settings.toml".to_string()]);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn save_writes_through_symlink_and_keeps_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let path = tmp_settings_path();
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir).unwrap();
        let real = dir.join("real.toml");
        let mgr_real = SettingsManager::new(&real);
        mgr_real.save(&Config::default()).unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&real, &path).unwrap();

        let mgr = SettingsManager::new(&path);
        let mut cfg = Config::default();
        cfg.general.project_name = "via-link".into();
        mgr.save(&cfg).unwrap();

        assert!(fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(mgr_real.load().unwrap().general.project_name, "via-link");
        assert_eq!(
            fs::metadata(&real).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(dir);
    }
}
