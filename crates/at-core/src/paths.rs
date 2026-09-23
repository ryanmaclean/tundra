//! Per-user directory resolution without the `dirs` crate.
//!
//! `dirs` 6 pulls `dirs-sys` -> `option-ext` (MPL-2.0), which is outside the
//! project license policy. These helpers reproduce the subset of `dirs`
//! behaviour auto-tundra relies on using only `std`:
//!
//! | helper         | macOS                                | Linux/BSD                               | Windows      |
//! |----------------|--------------------------------------|-----------------------------------------|--------------|
//! | [`home_dir`]   | `$HOME` / passwd entry               | `$HOME` / passwd entry                  | `USERPROFILE`|
//! | [`config_dir`] | `$HOME/Library/Application Support`  | `$XDG_CONFIG_HOME` (absolute) or `$HOME/.config` | `%APPDATA%` |
//!
//! The resolution logic lives in pure functions ([`config_dir_for`]) so it can
//! be tested without mutating the process environment.

use std::path::{Path, PathBuf};

/// The current user's home directory.
///
/// Uses [`std::env::home_dir`] (`$HOME` on Unix, falling back to the passwd
/// entry; `USERPROFILE` on Windows). An empty value is treated as unset.
pub fn home_dir() -> Option<PathBuf> {
    std::env::home_dir().filter(|p| !p.as_os_str().is_empty())
}

/// Target platform family used by [`config_dir_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    /// Linux, the BSDs and every other Unix: XDG base directory spec.
    Xdg,
}

impl Platform {
    /// Platform of the running binary.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Xdg
        }
    }
}

/// The per-user configuration directory, matching `dirs::config_dir()`.
pub fn config_dir() -> Option<PathBuf> {
    config_dir_for(
        Platform::current(),
        home_dir().as_deref(),
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).as_deref(),
        std::env::var_os("APPDATA").map(PathBuf::from).as_deref(),
    )
}

/// Pure resolution of the configuration directory.
///
/// * `xdg_config_home` is honoured on [`Platform::Xdg`] only when absolute
///   (per the XDG spec; relative values are ignored).
/// * `appdata` is used on [`Platform::Windows`] only when absolute.
pub fn config_dir_for(
    platform: Platform,
    home: Option<&Path>,
    xdg_config_home: Option<&Path>,
    appdata: Option<&Path>,
) -> Option<PathBuf> {
    match platform {
        Platform::MacOs => home.map(|h| h.join("Library").join("Application Support")),
        Platform::Windows => appdata.filter(|p| p.is_absolute()).map(Path::to_path_buf),
        Platform::Xdg => xdg_config_home
            .filter(|p| p.is_absolute())
            .map(Path::to_path_buf)
            .or_else(|| home.map(|h| h.join(".config"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Users\u")
        } else {
            PathBuf::from("/home/u")
        }
    }

    fn abs(p: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!(r"C:\{p}"))
        } else {
            PathBuf::from(format!("/{p}"))
        }
    }

    #[test]
    fn macos_config_dir_is_application_support() {
        let h = home();
        let got = config_dir_for(Platform::MacOs, Some(&h), Some(&abs("xdg")), None);
        assert_eq!(got, Some(h.join("Library").join("Application Support")));
    }

    #[test]
    fn xdg_config_home_wins_when_absolute() {
        let h = home();
        let x = abs("xdg");
        assert_eq!(
            config_dir_for(Platform::Xdg, Some(&h), Some(&x), None),
            Some(x)
        );
    }

    #[test]
    fn xdg_relative_config_home_is_ignored() {
        let h = home();
        let got = config_dir_for(Platform::Xdg, Some(&h), Some(Path::new("rel/cfg")), None);
        assert_eq!(got, Some(h.join(".config")));
    }

    #[test]
    fn xdg_falls_back_to_dot_config() {
        let h = home();
        assert_eq!(
            config_dir_for(Platform::Xdg, Some(&h), None, None),
            Some(h.join(".config"))
        );
    }

    #[test]
    fn windows_uses_absolute_appdata_only() {
        let a = abs("AppData");
        assert_eq!(
            config_dir_for(Platform::Windows, Some(&home()), None, Some(&a)),
            Some(a)
        );
        assert_eq!(
            config_dir_for(Platform::Windows, Some(&home()), None, Some(Path::new("rel"))),
            None
        );
    }

    #[test]
    fn no_home_means_no_config_dir_on_unix_like() {
        assert_eq!(config_dir_for(Platform::MacOs, None, None, None), None);
        assert_eq!(config_dir_for(Platform::Xdg, None, None, None), None);
    }

    #[test]
    fn current_platform_matches_cfg() {
        let p = Platform::current();
        if cfg!(target_os = "macos") {
            assert_eq!(p, Platform::MacOs);
        } else if cfg!(windows) {
            assert_eq!(p, Platform::Windows);
        } else {
            assert_eq!(p, Platform::Xdg);
        }
    }

    #[test]
    fn home_dir_is_non_empty_when_present() {
        if let Some(h) = home_dir() {
            assert!(!h.as_os_str().is_empty());
        }
    }

    #[test]
    fn config_dir_matches_pure_resolution() {
        let expected = config_dir_for(
            Platform::current(),
            home_dir().as_deref(),
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).as_deref(),
            std::env::var_os("APPDATA").map(PathBuf::from).as_deref(),
        );
        assert_eq!(config_dir(), expected);
    }
}
