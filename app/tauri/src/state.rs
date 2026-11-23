use at_bridge::ipc::IpcHandler;
use at_daemon::daemon::Daemon;

/// Shared application state for the Tauri desktop app.
///
/// Embeds the full daemon — API server, patrol loops, heartbeat,
/// KPI collection — all running in-process.
///
/// The `ipc` field holds a fully-wired `IpcHandler` backed by the
/// daemon's shared bead/agent vectors, so Tauri commands can route
/// IPC messages without going through HTTP.
pub struct AppState {
    pub daemon: Daemon,
    pub api_port: u16,
    pub ipc: IpcHandler,
}

/// Upper bound on terminating terminal children when the app exits.
pub const EXIT_TERMINAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

impl AppState {
    /// Stop admitting terminals and terminate every shell the daemon owns,
    /// then signal the daemon's background loops to stop.
    ///
    /// The Tauri event loop ends in `process::exit`, so the daemon loop's own
    /// terminal shutdown (run after its shutdown signal) never gets a chance
    /// to finish; the `RunEvent::Exit` handler awaits this instead. It is
    /// idempotent. Returns the ids whose exit could not be confirmed.
    pub async fn shutdown_for_exit(&self) -> Vec<uuid::Uuid> {
        let pending =
            at_bridge::terminal_lifecycle::shutdown(self.daemon.api_state(), EXIT_TERMINAL_TIMEOUT)
                .await;
        self.daemon.shutdown();
        pending
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        // Ensure clean shutdown of daemon background loops when app exits.
        // This sends a shutdown signal to all patrol/heartbeat/KPI tasks,
        // allowing them to complete gracefully before the process exits.
        tracing::info!("AppState dropping, triggering daemon shutdown");
        self.daemon.shutdown();
    }
}
