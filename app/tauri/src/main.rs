#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! auto-tundra desktop application.
//!
//! Embeds the full daemon (API server, patrol loops, KPI, heartbeat)
//! in-process. The Leptos WASM frontend runs in the Tauri webview and
//! discovers the API port and key via `window.__TUNDRA_API_PORT__` and
//! `window.__TUNDRA_API_KEY__`, set by a webview initialization script that
//! runs on every page load (reloads included).

mod tray;

use at_core::config::Config;
use at_daemon::daemon::Daemon;
use at_tauri::bridge::ipc_handler_from_daemon;
use at_tauri::sounds::SoundEngine;
use at_tauri::state::AppState;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use tracing::info;

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let proof_flag = std::ffi::OsStr::new("--verify-embedded-assets");
    let proof_requested = args.iter().any(|arg| arg.as_os_str() == proof_flag);
    if proof_requested && (args.len() != 2 || args[0].as_os_str() != proof_flag) {
        eprintln!("usage: auto-tundra --verify-embedded-assets <generated-dist-directory>");
        std::process::exit(2);
    }

    // This is the same context passed to the running app. The package proof
    // reads it before logging, credentials, the daemon, or a webview starts.
    let context = tauri::generate_context!();
    if proof_requested {
        let dist = Path::new(&args[1]);
        verify_embedded_assets(&context, dist).expect("packaged frontend asset proof failed");
        return;
    }

    // Before any TLS client is built: aws-lc-rs provider, X25519MLKEM768 first.
    at_core::tls::install_default_crypto_provider();
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
        .plugin(tauri_plugin_notification::init())
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

            // The "main" window is declared with `create: false` in
            // tauri.conf.json so we can attach the initialization script
            // before any page script runs (webview.eval() after the window
            // already exists is too late for scripts that ran on load).
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

            // Initialize system tray.
            if let Err(e) = tray::init_tray(&app.handle()) {
                tracing::warn!(error = %e, "failed to initialize system tray");
            }

            // Start notification listener for bead status changes.
            let state = app.state::<AppState>();
            let event_bus = state.daemon.event_bus();
            at_tauri::notifications::start_notification_listener(&app.handle(), event_bus);

            Ok(())
        })
        .on_window_event(|window, event| {
            // Keep the app running in system tray when window is closed.
            // Instead of exiting, hide the window and let tray icon restore it.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                info!("window close requested, hiding instead (tray icon will restore)");
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .run(context)
        .expect("error while running auto-tundra");

    info!("UI closed, shutting down daemon");
}

#[derive(serde::Serialize)]
struct AssetProofRow {
    key: String,
    dist_bytes: usize,
    dist_sha256: String,
    embedded_bytes: usize,
    embedded_sha256: String,
    transform: &'static str,
}

#[derive(serde::Serialize)]
struct AssetProof {
    schema: &'static str,
    asset_count: usize,
    assets: Vec<AssetProofRow>,
}

fn asset_proof_error(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn collect_dist_files(
    root: &Path,
    dir: &Path,
    files: &mut BTreeMap<String, PathBuf>,
) -> io::Result<()> {
    let mut entries = std::fs::read_dir(dir)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(asset_proof_error(format!(
                "symlink in generated frontend: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_dist_files(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| asset_proof_error("frontend path escaped dist"))?;
            let key = relative
                .to_str()
                .ok_or_else(|| asset_proof_error("non-UTF-8 frontend asset path"))?
                .replace(std::path::MAIN_SEPARATOR, "/");
            if files.insert(key.clone(), path).is_some() {
                return Err(asset_proof_error(format!(
                    "duplicate generated frontend key: {key}"
                )));
            }
        } else {
            return Err(asset_proof_error(format!(
                "non-file frontend entry: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn verify_embedded_assets(context: &tauri::Context, dist: &Path) -> io::Result<()> {
    if !dist.is_dir() {
        return Err(asset_proof_error(format!(
            "missing generated frontend: {}",
            dist.display()
        )));
    }
    let mut files = BTreeMap::new();
    collect_dist_files(dist, dist, &mut files)?;
    if !files.contains_key("index.html") || !files.keys().any(|key| key.ends_with(".wasm")) {
        return Err(asset_proof_error(
            "generated frontend is missing index.html or WASM",
        ));
    }

    let mut embedded = BTreeMap::new();
    for (asset_key, _) in context.assets().iter() {
        let key = asset_key.trim_start_matches('/').to_owned();
        if key.is_empty() || key.split('/').any(|segment| segment == "..") {
            return Err(asset_proof_error(format!(
                "unexpected embedded asset key: {key}"
            )));
        }
        // The iterator exposes Tauri's Brotli-compressed storage bytes.
        // Fetching by key decodes what the application serves at runtime.
        let decoded = context
            .assets()
            .get(&tauri::utils::assets::AssetKey::from(asset_key.as_ref()))
            .ok_or_else(|| asset_proof_error(format!("embedded asset cannot be fetched: {key}")))?;
        if embedded.insert(key.clone(), decoded.into_owned()).is_some() {
            return Err(asset_proof_error(format!(
                "duplicate embedded asset key: {key}"
            )));
        }
    }
    if files.keys().collect::<Vec<_>>() != embedded.keys().collect::<Vec<_>>() {
        return Err(asset_proof_error(
            "generated and embedded frontend asset key sets differ",
        ));
    }

    let mut rows = Vec::with_capacity(files.len());
    for (key, source_path) in files {
        let source = std::fs::read(&source_path)?;
        let mut expected = source.clone();
        let transform = if source_path.extension().is_some_and(|ext| ext == "html")
            && context.config().app.security.csp.is_some()
        {
            // tauri-codegen 2.6.3 map_core_assets: parse, inject CSP nonce
            // tokens, then serialize. It only changes HTML assets.
            let document =
                tauri_utils::html2::parse_doc(String::from_utf8_lossy(&source).into_owned());
            tauri_utils::html2::inject_nonce_token(
                &document,
                &context
                    .config()
                    .app
                    .security
                    .dangerous_disable_asset_csp_modification,
            );
            expected = tauri_utils::html2::serialize_doc(&document);
            "tauri-csp-html"
        } else {
            "identity"
        };
        let actual = embedded
            .get(&key)
            .ok_or_else(|| asset_proof_error(format!("missing embedded asset: {key}")))?;
        if expected != *actual {
            return Err(asset_proof_error(format!(
                "generated and embedded bytes differ: {key}"
            )));
        }
        rows.push(AssetProofRow {
            key,
            dist_bytes: source.len(),
            dist_sha256: format!("{:x}", Sha256::digest(&source)),
            embedded_bytes: actual.len(),
            embedded_sha256: format!("{:x}", Sha256::digest(actual)),
            transform,
        });
    }
    let proof = AssetProof {
        schema: "tundra-embedded-assets/v1",
        asset_count: rows.len(),
        assets: rows,
    };
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &proof).map_err(io::Error::other)?;
    use io::Write;
    writeln!(stdout)?;
    Ok(())
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
    // Same canonical file the daemon and the settings API use; going through
    // SettingsManager also migrates a legacy ~/.config/auto-tundra/settings.toml.
    let config_path = at_core::settings::SettingsManager::default_path()
        .path()
        .clone();
    if let Some(dir) = config_path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
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
        assert!(
            s.starts_with("window.__TUNDRA_API_PORT__=51234;window.__TUNDRA_API_KEY__=\"key-1\";")
        );
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
