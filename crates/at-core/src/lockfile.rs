//! Daemon lockfile for dynamic port discovery.
//!
//! When the standalone daemon starts, it binds to OS-assigned ephemeral ports
//! and writes a JSON lockfile to `~/.auto-tundra/daemon.lock`. Consumers (CLI,
//! TUI, tests) read this file to discover the running daemon's address.
//!
//! ## Race safety
//!
//! `acquire()` uses `O_CREAT | O_EXCL` to atomically create the lockfile.
//! If two daemons race, exactly one wins the create — the loser gets
//! `AlreadyExists` and can check whether the winner is still alive.
//!
//! ## Stale lockfile recovery
//!
//! `read_valid()` checks if the PID in the lockfile is still alive via
//! `kill(pid, 0)`. If the process is dead (crash, SIGKILL), the stale
//! lockfile is removed automatically and the next daemon can start.

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// Runtime state written by the daemon after binding its ports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonLockfile {
    pub pid: u32,
    pub api_port: u16,
    pub frontend_port: u16,
    pub host: String,
    pub started_at: String,
    /// Workspace root (enables future multi-instance keying).
    pub project_path: Option<String>,
    pub version: String,
}

/// Result of trying to acquire the lockfile.
pub enum AcquireResult {
    /// We created the lockfile — we own it.
    Acquired,
    /// Another live daemon holds the lockfile.
    AlreadyRunning(DaemonLockfile),
    /// Stale lockfile was cleaned up — retry.
    StaleRemoved,
}

impl DaemonLockfile {
    /// Canonical lockfile path: `~/.auto-tundra/daemon.lock`.
    pub fn path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(".auto-tundra").join("daemon.lock")
    }

    /// Try to exclusively create and write the lockfile.
    ///
    /// Uses `O_CREAT | O_EXCL` so two daemons racing will have exactly one
    /// winner. The loser gets `AlreadyRunning` or `StaleRemoved`.
    pub fn acquire(&self) -> std::io::Result<AcquireResult> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match OpenOptions::new()
            .write(true)
            .create_new(true) // O_CREAT | O_EXCL — fails if file exists
            .open(&path)
        {
            Ok(mut file) => {
                let json = serde_json::to_string_pretty(self)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                file.write_all(json.as_bytes())?;
                file.sync_all()?;
                Ok(AcquireResult::Acquired)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // File exists — check if the holder is alive.
                match Self::read() {
                    Some(existing) if existing.is_alive() => {
                        Ok(AcquireResult::AlreadyRunning(existing))
                    }
                    _ => {
                        // Stale or corrupt — remove and let caller retry.
                        tracing::info!("removing stale daemon lockfile");
                        Self::remove();
                        Ok(AcquireResult::StaleRemoved)
                    }
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Acquire with automatic retry after stale cleanup.
    ///
    /// Returns `Ok(())` if we own the lockfile, `Err` if another daemon is
    /// running or an I/O error occurred.
    pub fn acquire_or_fail(&self) -> Result<(), String> {
        for attempt in 0..2 {
            match self.acquire() {
                Ok(AcquireResult::Acquired) => return Ok(()),
                Ok(AcquireResult::AlreadyRunning(existing)) => {
                    return Err(format!(
                        "daemon already running (pid={}, api={}, frontend={})",
                        existing.pid,
                        existing.api_url(),
                        existing.frontend_url(),
                    ));
                }
                Ok(AcquireResult::StaleRemoved) if attempt == 0 => {
                    tracing::info!("stale lockfile removed, retrying acquire");
                    continue;
                }
                Ok(AcquireResult::StaleRemoved) => {
                    return Err("failed to acquire lockfile after stale cleanup".into());
                }
                Err(e) => return Err(format!("lockfile I/O error: {e}")),
            }
        }
        Err("lockfile acquire failed".into())
    }

    /// Read the lockfile. Returns `None` if missing or unparseable.
    pub fn read() -> Option<Self> {
        let path = Self::path();
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Remove the lockfile.
    pub fn remove() {
        let _ = std::fs::remove_file(Self::path());
    }

    /// Check if the PID in this lockfile is still alive.
    pub fn is_alive(&self) -> bool {
        pid_alive(self.pid)
    }

    /// Read the lockfile, validate the PID is alive, and auto-remove stale entries.
    ///
    /// Returns `Some(lockfile)` only if the file exists AND the PID is alive.
    pub fn read_valid() -> Option<Self> {
        let lock = Self::read()?;
        if lock.is_alive() {
            Some(lock)
        } else {
            tracing::info!(
                pid = lock.pid,
                "removing stale daemon lockfile (process not running)"
            );
            Self::remove();
            None
        }
    }

    /// Build the API base URL from this lockfile.
    pub fn api_url(&self) -> String {
        format!("http://{}:{}", self.host, self.api_port)
    }

    /// Build the frontend URL from this lockfile.
    pub fn frontend_url(&self) -> String {
        format!("http://{}:{}", self.host, self.frontend_port)
    }
}

/// Check if a process with the given PID is alive.
#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    // SAFETY: kill with signal 0 checks existence without sending a signal.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn pid_alive(_pid: u32) -> bool {
    // On non-Unix platforms, assume alive (conservative — avoids accidental cleanup).
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_pid_is_alive() {
        assert!(pid_alive(std::process::id()));
    }

    #[test]
    fn bogus_pid_is_dead() {
        // PID 4_000_000 is extremely unlikely to exist.
        assert!(!pid_alive(4_000_000));
    }

    #[test]
    fn lockfile_roundtrip() {
        let lock = DaemonLockfile {
            pid: std::process::id(),
            api_port: 12345,
            frontend_port: 54321,
            host: "127.0.0.1".into(),
            started_at: "2026-02-22T00:00:00Z".into(),
            project_path: Some("/tmp/test-project".into()),
            version: "0.1.0".into(),
        };

        let json = serde_json::to_string_pretty(&lock).unwrap();
        let parsed: DaemonLockfile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.api_port, 12345);
        assert_eq!(parsed.frontend_port, 54321);
        assert_eq!(parsed.api_url(), "http://127.0.0.1:12345");
        assert_eq!(parsed.frontend_url(), "http://127.0.0.1:54321");
    }

    #[test]
    fn is_alive_for_current_process() {
        let lock = DaemonLockfile {
            pid: std::process::id(),
            api_port: 0,
            frontend_port: 0,
            host: "127.0.0.1".into(),
            started_at: String::new(),
            project_path: None,
            version: String::new(),
        };
        assert!(lock.is_alive());
    }
}
