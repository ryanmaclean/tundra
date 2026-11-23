//! Termination signals for the desktop app.
//!
//! Tauri's event loop only reaches `RunEvent::Exit` (where
//! `AppState::shutdown_for_exit` kills the terminal shells) when the app is
//! asked to exit through the event loop. A bare SIGINT/SIGTERM would otherwise
//! end the process with the default disposition and leave every PTY child
//! behind. `TerminationSignals` lets `main` turn those signals into an
//! `AppHandle::exit`, so the normal exit path runs.

use std::io;

/// Handlers for SIGINT and SIGTERM (Ctrl+C on non-Unix targets).
///
/// Handlers are installed by [`TerminationSignals::install`], not on the first
/// `recv`, so a signal delivered right after `install` returns is not lost.
/// Must be called inside a tokio runtime context.
pub struct TerminationSignals {
    #[cfg(unix)]
    int: tokio::signal::unix::Signal,
    #[cfg(unix)]
    term: tokio::signal::unix::Signal,
}

impl TerminationSignals {
    /// Install the signal handlers.
    #[cfg(unix)]
    pub fn install() -> io::Result<Self> {
        use tokio::signal::unix::{signal, SignalKind};
        Ok(Self {
            int: signal(SignalKind::interrupt())?,
            term: signal(SignalKind::terminate())?,
        })
    }

    /// Install the signal handlers.
    #[cfg(not(unix))]
    pub fn install() -> io::Result<Self> {
        Ok(Self {})
    }

    /// Wait for the next termination signal and return its name.
    #[cfg(unix)]
    pub async fn recv(&mut self) -> &'static str {
        tokio::select! {
            _ = self.int.recv() => "SIGINT",
            _ = self.term.recv() => "SIGTERM",
        }
    }

    /// Wait for the next termination signal and return its name.
    #[cfg(not(unix))]
    pub async fn recv(&mut self) -> &'static str {
        let _ = tokio::signal::ctrl_c().await;
        "CTRL_C"
    }
}
