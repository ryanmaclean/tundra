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

impl Drop for AppState {
    fn drop(&mut self) {
        // Ensure clean shutdown of daemon background loops when app exits.
        // This sends a shutdown signal to all patrol/heartbeat/KPI tasks,
        // allowing them to complete gracefully before the process exits.
        tracing::info!("AppState dropping, triggering daemon shutdown");
        self.daemon.shutdown();
    }
}
