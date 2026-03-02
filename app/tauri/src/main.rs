#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! auto-tundra desktop application.
//!
//! Embeds the full daemon (API server, patrol loops, KPI, heartbeat)
//! in-process. The Leptos WASM frontend runs in the Tauri webview and
//! discovers the API port via `window.__TUNDRA_API_PORT__`.

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
    // NOTE: Tauri's WebviewWindow::eval is the standard Tauri API for
    // injecting controlled configuration into the webview — we only pass
    // trusted values (bound port + static mode flags), not user-supplied content.
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

    let init_script = format!(
        "window.__TUNDRA_API_PORT__ = {api_port};\
         window.__TUNDRA_NATIVE_SHELL__ = {native_shell};\
         document.documentElement.style.setProperty('--titlebar-inset', '{titlebar_inset}px');\
         document.documentElement.dataset.nativeShell = {native_shell_data};",
        api_port = api_port,
        native_shell = native_shell,
        titlebar_inset = titlebar_inset,
        native_shell_data = if native_shell { "\"1\"" } else { "\"0\"" }
    );

    tauri::Builder::default()
        .manage(state)
        .manage(sound_engine)
        .invoke_handler(tauri::generate_handler![
            at_tauri::commands::cmd_get_api_port,
            at_tauri::commands::cmd_play_sound,
            at_tauri::commands::cmd_set_sound_enabled,
            at_tauri::commands::cmd_set_sound_volume,
            at_tauri::commands::cmd_get_sound_settings,
            at_tauri::commands::cmd_list_beads,
            at_tauri::commands::cmd_create_bead,
            at_tauri::commands::cmd_update_bead_status,
            at_tauri::commands::cmd_update_bead,
            at_tauri::commands::cmd_delete_bead,
            at_tauri::commands::cmd_list_agents,
            at_tauri::commands::cmd_get_agent,
            at_tauri::commands::cmd_list_worktrees,
            at_tauri::commands::cmd_create_worktree,
            at_tauri::commands::cmd_delete_worktree,
            at_tauri::commands::cmd_list_github_issues,
            at_tauri::commands::cmd_list_github_prs,
            at_tauri::commands::cmd_sync_github_issues,
            at_tauri::commands::cmd_import_github_issue,
            // Intelligence: Insights
            at_tauri::commands::cmd_insights_list_sessions,
            at_tauri::commands::cmd_insights_create_session,
            at_tauri::commands::cmd_insights_delete_session,
            at_tauri::commands::cmd_insights_get_messages,
            at_tauri::commands::cmd_insights_add_message,
            // Intelligence: Ideation
            at_tauri::commands::cmd_ideation_list_ideas,
            at_tauri::commands::cmd_ideation_generate,
            at_tauri::commands::cmd_ideation_convert,
            // Intelligence: Roadmap
            at_tauri::commands::cmd_roadmap_list,
            at_tauri::commands::cmd_roadmap_create,
            at_tauri::commands::cmd_roadmap_generate,
            at_tauri::commands::cmd_roadmap_add_feature,
            at_tauri::commands::cmd_roadmap_add_feature_to_latest,
            at_tauri::commands::cmd_roadmap_update_feature_status,
            // Intelligence: Changelog
            at_tauri::commands::cmd_changelog_get,
            at_tauri::commands::cmd_changelog_generate,
            // Intelligence: Memory
            at_tauri::commands::cmd_memory_list,
            at_tauri::commands::cmd_memory_add,
            at_tauri::commands::cmd_memory_search,
            at_tauri::commands::cmd_memory_delete,
            // Settings/Configuration
            at_tauri::commands::cmd_get_settings,
            at_tauri::commands::cmd_put_settings,
            at_tauri::commands::cmd_patch_settings,
        ])
        .setup(move |app| {
            use tauri::Manager;
            if let Some(webview) = app.get_webview_window("main") {
                // Safe: init_script is a trusted constant (port integer).
                let _ = webview.eval(&init_script); // tauri::WebviewWindow::eval
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running auto-tundra");

    info!("UI closed, shutting down daemon");
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
