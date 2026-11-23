//! The single owner of terminal PTY lifecycle: admission, retirement,
//! orphan reaping and shutdown.
//!
//! Every path that creates or ends a terminal's child process goes through
//! this module: `POST /api/terminals` ([`admit`]), `DELETE` and the
//! WebSocket reconnect-grace expiry ([`retire`]), the daemon patrol tick
//! ([`reap_orphans`]) and daemon shutdown ([`shutdown`]). The rules it keeps:
//!
//! - **Atomic publication.** A terminal's registry entry and its
//!   [`PtyHandle`] are inserted under both write locks at once (lock order:
//!   `terminal_registry`, then `pty_handles`), so no scan ever sees a
//!   half-created terminal.
//! - **Ownership until confirmed exit.** A handle leaves `pty_handles` only
//!   to be terminated. If the kill cannot be confirmed, the handle goes back
//!   (status `Dead`) so the next patrol tick retries; it is never dropped
//!   on the floor while its child may still run.
//! - **Pool capacity follows the child.** The pool slot is freed by the
//!   handle's `Drop` once the exit is confirmed, exactly once; this module
//!   never calls `PtyPool::release` itself.
//! - **Unknown is not dead.** A failed `try_wait` leaves the terminal alone.
//! - **One retirement at a time per terminal**, tracked in
//!   [`TerminalLifecycle`].

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use at_session::pty_pool::{ChildStatus, PtyHandle};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::http_api::ApiState;
use crate::terminal::{TerminalInfo, TerminalStatus};

/// Lifecycle bookkeeping shared by every terminal path (held in
/// [`ApiState::terminal_lifecycle`]).
#[derive(Debug)]
pub struct TerminalLifecycle {
    accepting: AtomicBool,
    retiring: Mutex<HashSet<Uuid>>,
}

impl Default for TerminalLifecycle {
    fn default() -> Self {
        Self {
            accepting: AtomicBool::new(true),
            retiring: Mutex::new(HashSet::new()),
        }
    }
}

impl TerminalLifecycle {
    /// `false` once [`shutdown`] has started; new terminals are refused.
    pub fn accepting(&self) -> bool {
        self.accepting.load(Ordering::Acquire)
    }

    /// Whether a retirement of `id` is in flight.
    pub fn is_retiring(&self, id: &Uuid) -> bool {
        self.retiring
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(id)
    }

    fn claim(&self, id: Uuid) -> Option<RetireClaim<'_>> {
        let inserted = self
            .retiring
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id);
        inserted.then_some(RetireClaim { owner: self, id })
    }
}

/// Releases a terminal's retirement claim when the retirement ends (or its
/// future is dropped).
struct RetireClaim<'a> {
    owner: &'a TerminalLifecycle,
    id: Uuid,
}

impl Drop for RetireClaim<'_> {
    fn drop(&mut self) {
        self.owner
            .retiring
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

/// What happens to the registry entry once the child is confirmed gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetireMode {
    /// Remove the terminal entirely (`DELETE`, patrol reaping).
    Remove,
    /// Keep the entry with status `Dead` so a reconnect gets `410 Gone`
    /// (reconnect-grace expiry, shutdown).
    MarkDead,
}

/// Result of [`retire`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetireOutcome {
    /// The child (if any) is confirmed exited and all state was cleaned up.
    Retired,
    /// Neither a registry entry nor a handle exists for this id.
    NotFound,
    /// Another retirement of this terminal is in flight.
    InProgress,
    /// The kill could not be confirmed. The handle is still owned (status
    /// `Dead`, retried by the next patrol tick) and its pool slot is held.
    Failed(String),
}

/// Publish a freshly spawned terminal: registry entry and handle together.
///
/// Returns the handle back if the server is shutting down; the caller must
/// then terminate it.
pub async fn admit(
    state: &ApiState,
    info: TerminalInfo,
    handle: PtyHandle,
) -> Result<(), PtyHandle> {
    let mut registry = state.terminal_registry.write().await;
    let mut handles = state.pty_handles.write().await;
    // Checked under both locks: `shutdown` flips the flag under the same
    // locks, so a terminal is either refused here or seen by shutdown.
    if !state.terminal_lifecycle.accepting() {
        return Err(handle);
    }
    let id = handle.id;
    registry.register(info);
    handles.insert(id, handle);
    Ok(())
}

/// Terminate terminal `id`'s child (if any), then clean up its state.
///
/// The handle is taken out of `pty_handles` only for the duration of the
/// kill. If the exit cannot be confirmed it is put back (with status `Dead`)
/// so ownership, and the pool slot, survive for a retry.
pub async fn retire(state: &ApiState, id: Uuid, mode: RetireMode) -> RetireOutcome {
    let Some(_claim) = state.terminal_lifecycle.claim(id) else {
        return RetireOutcome::InProgress;
    };

    let handle = {
        let registry = state.terminal_registry.write().await;
        let mut handles = state.pty_handles.write().await;
        let handle = handles.remove(&id);
        if handle.is_none() && registry.get(&id).is_none() {
            return RetireOutcome::NotFound;
        }
        handle
    };

    if let Some(handle) = handle {
        // If this future is dropped mid-kill, `handle` drops with it and its
        // Drop still kills the child and frees the slot after the exit.
        match handle.terminate_async().await {
            Ok(code) => {
                debug!(terminal_id = %id, exit_code = code, "terminal child exited");
                // Exit confirmed: dropping the handle frees its pool slot.
                drop(handle);
            }
            Err(e) => {
                warn!(terminal_id = %id, error = %e, "terminal kill not confirmed; keeping ownership for retry");
                let mut registry = state.terminal_registry.write().await;
                let mut handles = state.pty_handles.write().await;
                registry.update_status(&id, TerminalStatus::Dead);
                handles.insert(id, handle);
                return RetireOutcome::Failed(e.to_string());
            }
        }
    }

    {
        let mut registry = state.terminal_registry.write().await;
        match mode {
            RetireMode::Remove => {
                registry.unregister(&id);
            }
            RetireMode::MarkDead => {
                registry.update_status(&id, TerminalStatus::Dead);
            }
        }
    }
    state.disconnect_buffers.write().await.remove(&id);
    state.terminal_conns.lock().await.remove(&id);
    RetireOutcome::Retired
}

/// What one reaper pass decided for a terminal.
enum Verdict {
    Keep,
    Retire,
    /// Registry entry with no handle and no retirement in flight.
    Unregister,
}

fn verdict(
    handle_status: Option<ChildStatus>,
    registry_status: Option<&TerminalStatus>,
) -> Verdict {
    match (handle_status, registry_status) {
        // The child exited by itself.
        (Some(ChildStatus::Exited(_)), _) => Verdict::Retire,
        // A previous kill was not confirmed: retry it.
        (Some(ChildStatus::Running), Some(TerminalStatus::Dead)) => Verdict::Retire,
        // A handle nobody can reach through the API.
        (Some(ChildStatus::Running), None) => Verdict::Retire,
        (Some(ChildStatus::Running), Some(_)) => Verdict::Keep,
        // A failed status query is not an exit.
        (Some(ChildStatus::Unknown), _) => Verdict::Keep,
        (None, Some(_)) => Verdict::Unregister,
        (None, None) => Verdict::Keep,
    }
}

/// One patrol pass: retire terminals whose child exited, retry kills that
/// were not confirmed, retire handles with no registry entry and drop
/// registry entries with no handle. Scans the union of both maps. Returns
/// how many terminals were cleaned up.
pub async fn reap_orphans(state: &ApiState) -> usize {
    let decisions: Vec<(Uuid, Verdict)> = {
        let registry = state.terminal_registry.read().await;
        let handles = state.pty_handles.read().await;
        let mut statuses: HashMap<Uuid, (Option<ChildStatus>, Option<TerminalStatus>)> =
            HashMap::new();
        for (id, handle) in handles.iter() {
            statuses.entry(*id).or_default().0 = Some(handle.status());
        }
        for info in registry.list() {
            statuses.entry(info.id).or_default().1 = Some(info.status.clone());
        }
        statuses
            .into_iter()
            .map(|(id, (child, reg))| (id, verdict(child, reg.as_ref())))
            .collect()
    };

    let mut reaped = 0;
    for (id, verdict) in decisions {
        match verdict {
            Verdict::Keep => {}
            Verdict::Retire => {
                warn!(terminal_id = %id, "orphaned PTY detected, reaping");
                match retire(state, id, RetireMode::Remove).await {
                    RetireOutcome::Retired => reaped += 1,
                    RetireOutcome::Failed(e) => {
                        warn!(terminal_id = %id, error = %e, "orphan PTY kill failed; will retry")
                    }
                    RetireOutcome::NotFound | RetireOutcome::InProgress => {}
                }
            }
            Verdict::Unregister => {
                if state.terminal_lifecycle.is_retiring(&id) {
                    continue;
                }
                let removed = {
                    let mut registry = state.terminal_registry.write().await;
                    let handles = state.pty_handles.read().await;
                    // Re-check under the locks: a handle may have come back
                    // from a failed retirement since the scan.
                    if handles.contains_key(&id) {
                        false
                    } else {
                        registry.unregister(&id).is_some()
                    }
                };
                if removed {
                    warn!(terminal_id = %id, "terminal entry without a PTY removed");
                    state.disconnect_buffers.write().await.remove(&id);
                    state.terminal_conns.lock().await.remove(&id);
                    reaped += 1;
                }
            }
        }
    }

    if reaped > 0 {
        info!(orphan_count = reaped, "orphan PTY reaping complete");
    }
    reaped
}

/// Stop admitting terminals and terminate every owned child, waiting up to
/// `timeout` in total. Returns the ids whose exit could not be confirmed
/// (their handles stay owned).
pub async fn shutdown(state: &ApiState, timeout: Duration) -> Vec<Uuid> {
    let ids: Vec<Uuid> = {
        let _registry = state.terminal_registry.write().await;
        let handles = state.pty_handles.write().await;
        state
            .terminal_lifecycle
            .accepting
            .store(false, Ordering::Release);
        handles.keys().copied().collect()
    };
    if ids.is_empty() {
        return Vec::new();
    }
    info!(
        count = ids.len(),
        "terminating terminal children for shutdown"
    );

    let retirements = ids.iter().map(|&id| async move {
        let outcome =
            tokio::time::timeout(timeout, retire(state, id, RetireMode::MarkDead)).await;
        match outcome {
            Ok(RetireOutcome::Retired | RetireOutcome::NotFound) => None,
            Ok(other) => {
                warn!(terminal_id = %id, outcome = ?other, "terminal child not confirmed exited at shutdown");
                Some(id)
            }
            Err(_) => {
                warn!(terminal_id = %id, "timed out terminating terminal child at shutdown");
                Some(id)
            }
        }
    });
    futures_util::future::join_all(retirements)
        .await
        .into_iter()
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_child_status_is_never_reaped() {
        assert!(matches!(
            verdict(Some(ChildStatus::Unknown), Some(&TerminalStatus::Active)),
            Verdict::Keep
        ));
        assert!(matches!(
            verdict(Some(ChildStatus::Unknown), Some(&TerminalStatus::Dead)),
            Verdict::Keep
        ));
        assert!(matches!(
            verdict(Some(ChildStatus::Unknown), None),
            Verdict::Keep
        ));
    }

    #[test]
    fn running_child_is_kept_unless_dead_or_unregistered() {
        assert!(matches!(
            verdict(Some(ChildStatus::Running), Some(&TerminalStatus::Active)),
            Verdict::Keep
        ));
        assert!(matches!(
            verdict(Some(ChildStatus::Running), Some(&TerminalStatus::Dead)),
            Verdict::Retire
        ));
        assert!(matches!(
            verdict(Some(ChildStatus::Running), None),
            Verdict::Retire
        ));
    }

    #[test]
    fn exited_child_and_handleless_entry_are_reaped() {
        assert!(matches!(
            verdict(Some(ChildStatus::Exited(0)), Some(&TerminalStatus::Active)),
            Verdict::Retire
        ));
        assert!(matches!(
            verdict(None, Some(&TerminalStatus::Active)),
            Verdict::Unregister
        ));
    }

    #[tokio::test]
    async fn shutdown_closes_admission_and_unknown_retire_is_not_found() {
        let state = ApiState::new(crate::event_bus::EventBus::new());
        assert!(state.terminal_lifecycle.accepting());
        assert!(shutdown(&state, Duration::from_secs(1)).await.is_empty());
        assert!(!state.terminal_lifecycle.accepting());
        assert_eq!(
            retire(&state, Uuid::new_v4(), RetireMode::Remove).await,
            RetireOutcome::NotFound
        );
    }
}
