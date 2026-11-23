//! SIGINT/SIGTERM must be observable by the desktop app so `main` can route
//! them into the Tauri exit path (`AppHandle::exit` -> `RunEvent::Exit` ->
//! `AppState::shutdown_for_exit`, covered by `exit_shutdown_test`).
#![cfg(unix)]

use std::time::Duration;

use at_tauri::signals::TerminationSignals;

fn send(signal: &str) {
    let status = std::process::Command::new("kill")
        .args([&format!("-{signal}"), &std::process::id().to_string()])
        .status()
        .expect("run kill");
    assert!(status.success(), "kill -{signal} failed");
}

/// One test: handlers are process-wide, so SIGINT and SIGTERM are driven in
/// sequence from a single `TerminationSignals`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigint_and_sigterm_are_delivered_not_fatal() {
    let mut signals = TerminationSignals::install().expect("install handlers");

    // The handlers are installed before `recv`, so signals sent now are held.
    send("TERM");
    let name = tokio::time::timeout(Duration::from_secs(10), signals.recv())
        .await
        .expect("SIGTERM was not delivered");
    assert_eq!(name, "SIGTERM");

    send("INT");
    let name = tokio::time::timeout(Duration::from_secs(10), signals.recv())
        .await
        .expect("SIGINT was not delivered");
    assert_eq!(name, "SIGINT");
}
