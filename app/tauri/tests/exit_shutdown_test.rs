//! The desktop app's exit path must terminate the shells the daemon owns.
//!
//! Tauri's event loop ends in `process::exit`, so `main` awaits
//! `AppState::shutdown_for_exit` from the `RunEvent::Exit` handler. This test
//! drives that same method against a real shell in a real PTY.
#![cfg(unix)]

use std::time::{Duration, Instant};

use at_bridge::terminal_ws::create_terminal;
use at_core::config::Config;
use at_daemon::daemon::Daemon;
use at_tauri::bridge::ipc_handler_from_daemon;
use at_tauri::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

/// `kill -0`: true while `pid` is in the process table (zombies included).
fn pid_exists(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exit_shutdown_kills_terminal_shells_and_refuses_new_ones() {
    let mut config = Config::default();
    config.cache.path = ":memory:".to_string();
    let daemon = Daemon::new(config).await.expect("create daemon");
    let ipc = ipc_handler_from_daemon(&daemon, Instant::now());
    let state = AppState {
        daemon,
        api_port: 0,
        ipc,
    };
    let api = state.daemon.api_state().clone();

    let created = create_terminal(State(api.clone()))
        .await
        .into_response()
        .status();
    assert_eq!(created, StatusCode::CREATED);
    let pid = {
        let handles = api.pty_handles.read().await;
        assert_eq!(handles.len(), 1);
        handles
            .values()
            .next()
            .unwrap()
            .process_id()
            .expect("shell pid")
    };
    assert!(pid_exists(pid), "shell {pid} should be running");

    let pending = state.shutdown_for_exit().await;
    assert!(pending.is_empty(), "unconfirmed children: {pending:?}");

    let deadline = Instant::now() + Duration::from_secs(10);
    while pid_exists(pid) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(!pid_exists(pid), "shell {pid} survived the exit shutdown");
    assert!(api.pty_handles.read().await.is_empty());
    assert!(!api.terminal_lifecycle.accepting());

    // A second call (the daemon loop's own shutdown, or Drop) is a no-op.
    assert!(state.shutdown_for_exit().await.is_empty());
}
