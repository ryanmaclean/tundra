//! Constructs a fully-wired [`IpcHandler`] from the daemon's shared state.
//!
//! This replaces the previous `IpcHandler::new_stub()` bootstrap path so
//! that Tauri IPC commands operate on the same bead/agent vectors that the
//! HTTP API and patrol loops use.

use at_bridge::ipc::IpcHandler;
use at_daemon::daemon::Daemon;

/// Build an [`IpcHandler`] that shares the daemon's event bus, bead list,
/// and agent list.  `start_time` is the instant the application started —
/// it drives the `uptime_seconds` field returned by `GetStatus`.
pub fn ipc_handler_from_daemon(daemon: &Daemon, start_time: std::time::Instant) -> IpcHandler {
    let api_state = daemon.api_state();
    IpcHandler::new(
        api_state.event_bus.clone(),
        api_state.beads.clone(),
        api_state.agents.clone(),
        start_time,
    )
}
