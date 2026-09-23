#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! auto-tundra desktop application.
//!
//! Embeds the full daemon (API server, patrol loops, KPI, heartbeat)
//! in-process. The Leptos WASM frontend runs in the Tauri webview and
//! discovers the API port and key via `window.__TUNDRA_API_PORT__` and
//! `window.__TUNDRA_API_KEY__`, set by a webview initialization script that
//! runs on every page load (reloads included).

use at_core::config::Config;
use at_daemon::daemon::Daemon;
use at_tauri::bridge::ipc_handler_from_daemon;
use at_tauri::sounds::SoundEngine;
use at_tauri::state::AppState;
use tracing::info;

fn main() {
    at_telemetry::logging::init_logging("auto-tundra", "info");
    info!("auto-tundra desktop app starting");

    let start_time = std::time::Instant::now();

    let runtime = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");

    // Boot daemon inside the tokio runtime.
    let (daemon, api_port) = runtime.block_on(async {
        let config = load_config();
        let daemon = Daemon::new(config).await.expect("failed to create daemon");
        let port = daemon
            .start_embedded()
            .await
            .expect("failed to start embedded API server");
        (daemon, port)
    });

    info!(api_port, "daemon started, launching UI");

    // Build a fully-wired IPC handler that shares the daemon's bead/agent
    // vectors and event bus, replacing the previous stub.
    let ipc = ipc_handler_from_daemon(&daemon, start_time);

    let state = AppState {
        daemon,
        api_port,
        ipc,
    };

    // Initialize sound engine (returns None if no audio device available).
    let sound_engine: Option<SoundEngine> = SoundEngine::try_new();
    if sound_engine.is_some() {
        info!("sound engine initialized");
    } else {
        info!("no audio device — sound effects disabled");
    }

    // Inject runtime flags/config into the webview before any JS runs.
    // Registered as an *initialization script*, which the webview re-runs on
    // every navigation. (A one-shot `eval` in setup was lost on reload, e.g.
    // after switching projects, sending the UI to 127.0.0.1:9090.)
    // Only trusted values are injected: the bound port, the daemon API key
    // (JSON/HTML-escaped by at-api-types) and static mode flags.
    //
    // Native-shell prototype mode (macOS only) can be enabled via:
    //   AT_NATIVE_SHELL_MACOS=1
    let native_shell = cfg!(target_os = "macos") && env_flag("AT_NATIVE_SHELL_MACOS");
    // Traffic lights/top inset: larger in native-shell prototype mode.
    let titlebar_inset = if cfg!(target_os = "macos") {
        if native_shell {
            36
        } else {
            28
        }
    } else {
        0
    };

    // Same key the embedded daemon enforces (env var or ~/.auto-tundra/daemon.key).
    let api_key = at_core::config::CredentialProvider::ensure_daemon_api_key();
    let init_script = build_init_script(api_port, &api_key, native_shell, titlebar_inset);

    tauri::Builder::default()
        .manage(state)
        .manage(sound_engine)
        .invoke_handler(tauri::generate_handler![
            at_tauri::commands::cmd_get_api_port,
            at_tauri::commands::cmd_play_sound,
            at_tauri::commands::cmd_set_sound_enabled,
            at_tauri::commands::cmd_set_sound_volume,
            at_tauri::commands::cmd_get_sound_settings,
        ])
        .setup(move |app| {
            // The "main" window is declared with `create: false` in
            // tauri.conf.json so we can attach the initialization script.
            let window_config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .cloned()
                .ok_or("tauri.conf.json has no window labelled \"main\"")?;
            tauri::WebviewWindowBuilder::from_config(app.handle(), &window_config)?
                .initialization_script(init_script.as_str())
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running auto-tundra");

    info!("UI closed, shutting down daemon");
}

/// JavaScript run in the webview before page scripts on every navigation.
fn build_init_script(
    api_port: u16,
    api_key: &str,
    native_shell: bool,
    titlebar_inset: u16,
) -> String {
    format!(
        "{connection}\
         window.__TUNDRA_NATIVE_SHELL__ = {native_shell};\
         document.documentElement.style.setProperty('--titlebar-inset', '{titlebar_inset}px');\
         document.documentElement.dataset.nativeShell = {native_shell_data};",
        connection = at_bridge::http_api::at_api_types::auth::browser_bootstrap_script(
            api_port,
            Some(api_key)
        ),
        native_shell = native_shell,
        titlebar_inset = titlebar_inset,
        native_shell_data = if native_shell { "\"1\"" } else { "\"0\"" }
    )
}

fn env_flag(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

fn load_config() -> Config {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let data_dir = std::path::Path::new(&home).join(".auto-tundra");
    std::fs::create_dir_all(&data_dir).ok();

    let config_path = data_dir.join("config.toml");
    let mut config = if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "bad config.toml, using defaults");
                Config::default()
            }),
            Err(e) => {
                tracing::warn!(error = %e, "cannot read config.toml, using defaults");
                Config::default()
            }
        }
    } else {
        Config::default()
    };

    // Expand ~ in cache path
    if config.cache.path.starts_with("~/") {
        config.cache.path = config.cache.path.replacen("~", &home, 1);
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_script_carries_port_key_and_flags() {
        let s = build_init_script(51234, "key-1", false, 28);
        assert!(s.starts_with("window.__TUNDRA_API_PORT__=51234;window.__TUNDRA_API_KEY__=\"key-1\";"));
        assert!(s.contains("window.__TUNDRA_NATIVE_SHELL__ = false;"));
        assert!(s.contains("'--titlebar-inset', '28px'"));
        assert!(s.contains("dataset.nativeShell = \"0\";"));
    }

    #[test]
    fn main_window_is_created_in_code() {
        // setup() builds the window itself so the init script is attached;
        // if tauri.conf.json created it too we would get two windows.
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let main = conf["app"]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["label"] == "main")
            .expect("main window config");
        assert_eq!(main["create"], false);
    }
}
