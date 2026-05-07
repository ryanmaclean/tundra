use std::sync::Arc;

use anyhow::Result;
use at_bridge::http_api::ApiState;
use at_core::cache::{CacheDb, CacheError};
use at_core::types::BeadStatus;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Result of a single patrol sweep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolReport {
    /// Number of stale agents discovered (no heartbeat in 2x interval).
    pub stale_agents: usize,
    /// Number of beads stuck in `Slung` status past the timeout.
    pub stuck_beads: usize,
    /// Number of orphan PTYs detected.
    pub orphan_ptys: usize,
    /// IDs of stuck beads found.
    pub stuck_bead_ids: Vec<uuid::Uuid>,
    /// Timestamp of this patrol run.
    pub timestamp: DateTime<Utc>,
}

/// Runs periodic patrol sweeps over the cache to detect anomalies.
pub struct PatrolRunner {
    /// Heartbeat interval in seconds; agents missing for 2x this are stale.
    _heartbeat_interval_secs: u64,
    /// Maximum duration a bead may remain in `Slung` before it is considered stuck.
    slung_timeout: ChronoDuration,
}

impl PatrolRunner {
    /// Create a new patrol runner.
    ///
    /// `heartbeat_interval_secs` is used to compute the staleness threshold
    /// (2x the heartbeat interval). The default slung timeout is 30 minutes.
    pub fn new(heartbeat_interval_secs: u64) -> Self {
        Self {
            _heartbeat_interval_secs: heartbeat_interval_secs,
            slung_timeout: ChronoDuration::minutes(30),
        }
    }

    /// Override the slung timeout.
    pub fn with_slung_timeout(mut self, timeout: ChronoDuration) -> Self {
        self.slung_timeout = timeout;
        self
    }

    /// Execute a full patrol sweep.
    ///
    /// Checks:
    /// - Stuck beads: beads in `Slung` status longer than the timeout.
    /// - Stale agents: detected via the heartbeat monitor (count reported but
    ///   agent enumeration requires external tracking since CacheDb does not
    ///   expose a list-all-agents API).
    /// - Orphan PTYs: placeholder for future PTY session tracking.
    pub async fn run_patrol(&self, cache: &CacheDb) -> Result<PatrolReport> {
        let now = Utc::now();
        debug!("patrol sweep starting");

        // --- Check for stuck beads (slung longer than timeout) ---
        let slung_beads = cache
            .list_beads_by_status(BeadStatus::Slung)
            .await
            .map_err(|e| match e {
                CacheError::InvalidRow { ref context, .. } => {
                    // Persistent schema corruption — log at error; operator action required.
                    tracing::error!(
                        context = %context,
                        "slung bead row has corrupt data (schema drift?); \
                         patrol cannot complete stuck-bead check"
                    );
                    anyhow::anyhow!("slung beads contain corrupt row data — {}: {}", context, e)
                }
                CacheError::Db(ref db_err) => {
                    // Transient DB error — patrol will retry on the next cycle.
                    tracing::warn!(
                        error = %db_err,
                        "transient DB error querying slung beads; patrol will retry next cycle"
                    );
                    anyhow::anyhow!("transient DB error querying slung beads: {}", db_err)
                }
            })?;

        let mut stuck_bead_ids = Vec::new();
        for bead in &slung_beads {
            if let Some(slung_at) = bead.slung_at {
                let elapsed = now.signed_duration_since(slung_at);
                if elapsed > self.slung_timeout {
                    stuck_bead_ids.push(bead.id);
                    info!(
                        bead_id = %bead.id,
                        slung_at = %slung_at,
                        elapsed_mins = elapsed.num_minutes(),
                        "stuck bead detected"
                    );
                }
            }
        }

        // Stale agent detection is handled by HeartbeatMonitor; patrol
        // reports a zero count here since we cannot enumerate all agents
        // without a list_agents API on CacheDb.
        let stale_agents = 0;

        // Orphan PTY detection is a placeholder for future implementation.
        let orphan_ptys = 0;

        let report = PatrolReport {
            stale_agents,
            stuck_beads: stuck_bead_ids.len(),
            orphan_ptys,
            stuck_bead_ids,
            timestamp: now,
        };

        debug!(stuck_beads = report.stuck_beads, "patrol sweep completed");

        Ok(report)
    }
}

/// Reap orphaned PTY processes whose child has exited but remain in the
/// terminal registry.
///
/// Iterates every entry in the `terminal_registry`, checks the corresponding
/// `pty_handles` entry for liveness via `PtyHandle::is_alive()`, and removes
/// dead entries from both maps. Returns the number of orphans reaped.
pub async fn reap_orphan_ptys(state: &Arc<ApiState>) -> usize {
    // Collect terminal IDs that have a registered PTY handle.
    let terminal_ids: Vec<uuid::Uuid> = {
        let registry = state.terminal_registry.read().await;
        registry.list().iter().map(|t| t.id).collect()
    };

    let mut orphan_count = 0;

    for tid in &terminal_ids {
        let is_dead = {
            let handles = state.pty_handles.read().await;
            match handles.get(tid) {
                Some(handle) => !handle.is_alive(),
                // Terminal registered but no PTY handle at all — also an orphan.
                None => true,
            }
        };

        if is_dead {
            orphan_count += 1;
            warn!(terminal_id = %tid, "orphaned PTY detected — reaping");

            // Remove from pty_handles (and kill if still present).
            {
                let mut handles = state.pty_handles.write().await;
                if let Some(handle) = handles.remove(tid) {
                    let _ = handle.kill();
                }
            }

            // Mark as closed in the registry and remove.
            {
                let mut registry = state.terminal_registry.write().await;
                registry.unregister(tid);
            }
        }
    }

    if orphan_count > 0 {
        info!(orphan_count, "orphan PTY reaping complete");
    }

    orphan_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::types::{Bead, Lane};

    // -----------------------------------------------------------------------
    // Mock plumbing for reap_orphan_ptys tests
    // -----------------------------------------------------------------------

    use at_bridge::event_bus::EventBus;
    use at_bridge::http_api::ApiState;
    use at_bridge::terminal::{TerminalInfo, TerminalRegistry, TerminalStatus};
    use at_session::pty_pool::PtyHandle;
    use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};
    use std::io;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    // ----- MockChildState ---------------------------------------------------

    struct MockChildState {
        /// When `true`, `try_wait` returns `Ok(Some(exited))`, making
        /// `PtyHandle::is_alive()` return `false`.
        is_dead: bool,
        /// When `true`, `kill()` returns an `Err`.
        should_fail_kill: bool,
        /// Records whether `kill()` was called.
        kill_called: bool,
    }

    impl std::fmt::Debug for MockChildState {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MockChildState")
                .field("is_dead", &self.is_dead)
                .field("kill_called", &self.kill_called)
                .finish()
        }
    }

    // ----- MockChild (shared via Arc<Mutex<_>>) ----------------------------

    #[derive(Debug, Clone)]
    struct MockChild(Arc<Mutex<MockChildState>>);

    impl MockChild {
        fn new(is_dead: bool, should_fail_kill: bool) -> Self {
            Self(Arc::new(Mutex::new(MockChildState {
                is_dead,
                should_fail_kill,
                kill_called: false,
            })))
        }

        fn kill_was_called(&self) -> bool {
            self.0.lock().unwrap().kill_called
        }
    }

    impl ChildKiller for MockChild {
        fn kill(&mut self) -> io::Result<()> {
            let mut state = self.0.lock().unwrap();
            state.kill_called = true;
            if state.should_fail_kill {
                Err(io::Error::new(io::ErrorKind::Other, "mock kill failure"))
            } else {
                Ok(())
            }
        }

        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(self.clone())
        }
    }

    impl Child for MockChild {
        fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            let state = self.0.lock().unwrap();
            if state.is_dead {
                Ok(Some(ExitStatus::with_exit_code(0)))
            } else {
                Ok(None)
            }
        }

        fn wait(&mut self) -> io::Result<ExitStatus> {
            Ok(ExitStatus::with_exit_code(0))
        }

        fn process_id(&self) -> Option<u32> {
            None
        }
    }

    // ----- MockMasterPty ---------------------------------------------------

    #[derive(Debug)]
    struct MockMasterPty;

    impl MasterPty for MockMasterPty {
        fn resize(&self, _size: PtySize) -> Result<(), anyhow::Error> {
            Ok(())
        }

        fn get_size(&self) -> Result<PtySize, anyhow::Error> {
            Ok(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
        }

        fn try_clone_reader(&self) -> Result<Box<dyn io::Read + Send>, anyhow::Error> {
            // Return a reader that immediately yields EOF.
            Ok(Box::new(io::empty()))
        }

        fn take_writer(&self) -> Result<Box<dyn io::Write + Send>, anyhow::Error> {
            Ok(Box::new(io::sink()))
        }

        #[cfg(unix)]
        fn process_group_leader(&self) -> Option<libc::pid_t> {
            None
        }

        #[cfg(unix)]
        fn as_raw_fd(&self) -> Option<std::os::unix::io::RawFd> {
            None
        }
    }

    // ----- builder helpers -------------------------------------------------

    /// Build a `PtyHandle` backed entirely by in-memory mocks.
    ///
    /// Returns `(handle, mock_child)` so callers can inspect `kill_was_called`
    /// after the function under test has run.  Requires the `test-helpers`
    /// feature on `at-session` (declared in `[dev-dependencies]`).
    fn make_mock_handle(id: Uuid, is_dead: bool, should_fail_kill: bool) -> (PtyHandle, MockChild) {
        let child = MockChild::new(is_dead, should_fail_kill);
        let (_, rx) = flume::bounded::<Vec<u8>>(1);
        let (tx, _) = flume::bounded::<Vec<u8>>(1);
        let handle = PtyHandle::from_parts(
            id,
            rx,
            tx,
            Arc::new(Mutex::new(
                Box::new(child.clone()) as Box<dyn portable_pty::Child + Send + Sync>,
            )),
            Arc::new(Mutex::new(
                Box::new(MockMasterPty) as Box<dyn MasterPty + Send>,
            )),
        );
        (handle, child)
    }

    /// Build a minimal `TerminalInfo` with the given `id`.
    fn make_terminal_info(id: Uuid) -> TerminalInfo {
        TerminalInfo {
            id,
            agent_id: Uuid::new_v4(),
            title: "test".into(),
            status: TerminalStatus::Active,
            cols: 80,
            rows: 24,
            font_size: 14,
            font_family: "monospace".into(),
            line_height: 1.2,
            letter_spacing: 0.0,
            profile: "default".into(),
            cursor_style: "block".into(),
            cursor_blink: false,
            auto_name: None,
            persistent: false,
        }
    }

    /// Create a fresh `ApiState` seeded with the given registries.
    ///
    /// The seam is implemented via direct field writes to the `pub` fields
    /// `terminal_registry` and `pty_handles` on `ApiState` — no new constructor
    /// is required.  This keeps the seam minimal and avoids gating issues with
    /// `#[cfg(test)]` across crate boundaries.
    async fn make_state_with(
        registry: TerminalRegistry,
        handles: std::collections::HashMap<Uuid, PtyHandle>,
    ) -> Arc<ApiState> {
        let state = Arc::new(ApiState::new(EventBus::new()));
        *state.terminal_registry.write().await = registry;
        *state.pty_handles.write().await = handles;
        state
    }

    // -----------------------------------------------------------------------
    // Tests for reap_orphan_ptys
    // -----------------------------------------------------------------------

    /// Both registries are fully consistent (live PTY for each terminal entry).
    /// `reap_orphan_ptys` must return 0, call no kills, and leave both maps intact.
    #[tokio::test]
    async fn reap_orphan_ptys_no_orphans_no_kills() {
        let tid = Uuid::new_v4();

        let mut registry = TerminalRegistry::new();
        registry.register(make_terminal_info(tid));

        // is_dead=false => PtyHandle::is_alive() returns true
        let (handle, mock_child) = make_mock_handle(tid, false, false);
        let mut handles = std::collections::HashMap::new();
        handles.insert(tid, handle);

        let state = make_state_with(registry, handles).await;

        let reaped = reap_orphan_ptys(&state).await;

        assert_eq!(reaped, 0, "no orphans expected when both maps are consistent");
        assert!(
            !mock_child.kill_was_called(),
            "kill() must not be called when the PTY is alive"
        );
        assert_eq!(
            state.terminal_registry.read().await.list().len(),
            1,
            "terminal_registry must be unchanged"
        );
        assert_eq!(
            state.pty_handles.read().await.len(),
            1,
            "pty_handles must be unchanged"
        );
    }

    /// A terminal-registry entry whose backing PTY has died (is_alive() == false)
    /// is an orphan.  The function must call kill() on the handle and remove it
    /// from both maps.
    #[tokio::test]
    async fn reap_orphan_ptys_kills_orphan_pty_with_no_terminal() {
        let tid = Uuid::new_v4();

        let mut registry = TerminalRegistry::new();
        registry.register(make_terminal_info(tid));

        // is_dead=true => PtyHandle::is_alive() returns false
        let (handle, mock_child) = make_mock_handle(tid, true, false);
        let mut handles = std::collections::HashMap::new();
        handles.insert(tid, handle);

        let state = make_state_with(registry, handles).await;

        let reaped = reap_orphan_ptys(&state).await;

        assert_eq!(reaped, 1, "exactly one orphan expected");
        assert!(
            mock_child.kill_was_called(),
            "kill() must be called on the dead PTY handle"
        );
        assert!(
            state.pty_handles.read().await.is_empty(),
            "dead handle must be removed from pty_handles"
        );
        assert!(
            state.terminal_registry.read().await.list().is_empty(),
            "dead terminal entry must be removed from terminal_registry"
        );
    }

    /// A terminal-registry entry with no matching PTY handle at all is an orphan.
    /// The function must remove the terminal entry and must NOT panic (there is
    /// nothing to kill since no handle exists).
    #[tokio::test]
    async fn reap_orphan_ptys_removes_terminal_entry_with_no_pty() {
        let tid = Uuid::new_v4();

        let mut registry = TerminalRegistry::new();
        registry.register(make_terminal_info(tid));

        // pty_handles is empty — no handle for this terminal at all.
        let state = make_state_with(registry, std::collections::HashMap::new()).await;

        let reaped = reap_orphan_ptys(&state).await;

        assert_eq!(reaped, 1, "terminal with no PTY handle is an orphan");
        assert!(
            state.terminal_registry.read().await.list().is_empty(),
            "orphan terminal entry must be removed from terminal_registry"
        );
        // pty_handles was empty and must remain empty — no crash.
        assert!(state.pty_handles.read().await.is_empty());
    }

    /// When `kill()` fails on the underlying child, `reap_orphan_ptys` must NOT
    /// panic.  The production code uses `let _ = handle.kill()` which discards
    /// the error, so the entry is removed regardless of kill outcome.
    #[tokio::test]
    async fn reap_orphan_ptys_handles_kill_failure_gracefully() {
        let tid = Uuid::new_v4();

        let mut registry = TerminalRegistry::new();
        registry.register(make_terminal_info(tid));

        // is_dead=true, should_fail_kill=true => kill() returns Err.
        let (handle, mock_child) = make_mock_handle(tid, true, true);
        let mut handles = std::collections::HashMap::new();
        handles.insert(tid, handle);

        let state = make_state_with(registry, handles).await;

        // Must NOT panic even though kill() returns Err.
        let reaped = reap_orphan_ptys(&state).await;

        assert_eq!(reaped, 1, "orphan count must be 1 even after kill failure");
        assert!(
            mock_child.kill_was_called(),
            "kill() must still be attempted on the orphaned handle"
        );
        // Graceful degradation: entry is removed regardless of kill outcome.
        assert!(
            state.pty_handles.read().await.is_empty(),
            "handle must be removed from pty_handles even when kill() fails"
        );
        assert!(
            state.terminal_registry.read().await.list().is_empty(),
            "terminal must be removed from terminal_registry even when kill() fails"
        );
    }

    fn make_slung_bead(slung_at: Option<DateTime<Utc>>) -> Bead {
        let mut bead = Bead::new("test bead", Lane::Standard);
        bead.status = BeadStatus::Slung;
        bead.slung_at = slung_at;
        bead
    }

    async fn insert_beads(cache: &CacheDb, beads: &[Bead]) {
        for b in beads {
            cache.upsert_bead(b).await.expect("upsert bead");
        }
    }

    #[test]
    fn new_uses_default_thirty_minute_slung_timeout() {
        let runner = PatrolRunner::new(60);
        assert_eq!(runner.slung_timeout, ChronoDuration::minutes(30));
    }

    #[test]
    fn new_records_heartbeat_interval() {
        let runner = PatrolRunner::new(123);
        assert_eq!(runner._heartbeat_interval_secs, 123);
    }

    #[test]
    fn with_slung_timeout_overrides_default() {
        let runner = PatrolRunner::new(60).with_slung_timeout(ChronoDuration::minutes(5));
        assert_eq!(runner.slung_timeout, ChronoDuration::minutes(5));
    }

    #[test]
    fn with_slung_timeout_accepts_zero() {
        let runner = PatrolRunner::new(60).with_slung_timeout(ChronoDuration::zero());
        assert_eq!(runner.slung_timeout, ChronoDuration::zero());
    }

    #[test]
    fn patrol_report_serde_round_trip() {
        let report = PatrolReport {
            stale_agents: 1,
            stuck_beads: 2,
            orphan_ptys: 3,
            stuck_bead_ids: vec![uuid::Uuid::new_v4(), uuid::Uuid::new_v4()],
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&report).expect("serialize");
        let back: PatrolReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.stuck_beads, report.stuck_beads);
        assert_eq!(back.stale_agents, report.stale_agents);
        assert_eq!(back.orphan_ptys, report.orphan_ptys);
        assert_eq!(back.stuck_bead_ids, report.stuck_bead_ids);
    }

    #[tokio::test]
    async fn run_patrol_on_empty_cache_yields_zero_counts() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 0);
        assert_eq!(report.stale_agents, 0);
        assert_eq!(report.orphan_ptys, 0);
        assert!(report.stuck_bead_ids.is_empty());
    }

    #[tokio::test]
    async fn run_patrol_skips_beads_without_slung_at() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Slung beads with no slung_at timestamp must not be flagged.
        let bead = make_slung_bead(None);
        insert_beads(&cache, &[bead]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 0);
        assert!(report.stuck_bead_ids.is_empty());
    }

    #[tokio::test]
    async fn run_patrol_does_not_flag_recently_slung_beads() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Slung 5 minutes ago, default timeout 30 minutes — not stuck.
        let recent = make_slung_bead(Some(Utc::now() - ChronoDuration::minutes(5)));
        insert_beads(&cache, &[recent]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 0);
    }

    #[tokio::test]
    async fn run_patrol_flags_stuck_beads_past_timeout() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Slung 45 minutes ago, default timeout 30 minutes — stuck.
        let stuck = make_slung_bead(Some(Utc::now() - ChronoDuration::minutes(45)));
        let stuck_id = stuck.id;
        insert_beads(&cache, &[stuck]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 1);
        assert_eq!(report.stuck_bead_ids, vec![stuck_id]);
    }

    #[tokio::test]
    async fn run_patrol_respects_custom_slung_timeout() {
        let runner = PatrolRunner::new(60).with_slung_timeout(ChronoDuration::seconds(10));
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Slung 30 seconds ago — exceeds the custom 10-second timeout.
        let stuck = make_slung_bead(Some(Utc::now() - ChronoDuration::seconds(30)));
        let stuck_id = stuck.id;
        insert_beads(&cache, &[stuck]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 1);
        assert_eq!(report.stuck_bead_ids, vec![stuck_id]);
    }

    #[tokio::test]
    async fn run_patrol_ignores_non_slung_beads() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Backlog/Done beads must not be considered stuck regardless of
        // slung_at, because list_beads_by_status(Slung) excludes them.
        let mut backlog = Bead::new("backlog", Lane::Standard);
        backlog.slung_at = Some(Utc::now() - ChronoDuration::hours(99));
        let mut done = Bead::new("done", Lane::Standard);
        done.status = BeadStatus::Done;
        done.slung_at = Some(Utc::now() - ChronoDuration::hours(99));

        insert_beads(&cache, &[backlog, done]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stuck_beads, 0);
        assert!(report.stuck_bead_ids.is_empty());
    }

    #[tokio::test]
    async fn run_patrol_separates_stuck_from_fresh_in_mixed_population() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        let fresh = make_slung_bead(Some(Utc::now() - ChronoDuration::minutes(1)));
        let stuck1 = make_slung_bead(Some(Utc::now() - ChronoDuration::hours(1)));
        let stuck2 = make_slung_bead(Some(Utc::now() - ChronoDuration::hours(3)));
        let no_slung = make_slung_bead(None);

        let stuck_ids = [stuck1.id, stuck2.id];
        insert_beads(&cache, &[fresh, stuck1, stuck2, no_slung]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol");

        assert_eq!(report.stuck_beads, 2);
        for id in stuck_ids {
            assert!(
                report.stuck_bead_ids.contains(&id),
                "expected stuck id {id} in report",
            );
        }
    }

    #[tokio::test]
    async fn run_patrol_timestamp_is_recent() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        let before = Utc::now();
        let report = runner.run_patrol(&cache).await.expect("patrol");
        let after = Utc::now();

        assert!(report.timestamp >= before);
        assert!(report.timestamp <= after);
    }

    #[tokio::test]
    async fn run_patrol_stale_agents_count_is_zero_placeholder() {
        // Per the impl, run_patrol always reports 0 stale agents because
        // CacheDb has no list-all-agents API. This test pins that contract.
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");
        let report = runner.run_patrol(&cache).await.expect("patrol");
        assert_eq!(report.stale_agents, 0);
        assert_eq!(report.orphan_ptys, 0);
    }

    // ----- CacheError::InvalidRow regression tests -----

    /// When `list_beads_by_status` returns `CacheError::InvalidRow` (because a
    /// slung row has a corrupt lane value), `run_patrol` must return an `Err`
    /// rather than panicking.  The error message must mention the corruption so
    /// operators can identify the source.
    #[tokio::test]
    async fn run_patrol_returns_err_on_invalid_row_in_slung_beads() {
        let runner = PatrolRunner::new(60);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Insert a slung bead with a bogus lane — row_to_bead will return
        // CacheError::InvalidRow when the patrol queries slung beads.
        cache
            .insert_raw_bead_for_test(
                "550e8400-e29b-41d4-a716-446655440002",
                "slung",
                "GALAXY_BRAIN_LANE",
            )
            .await
            .expect("raw insert");

        let result = runner.run_patrol(&cache).await;
        assert!(
            result.is_err(),
            "run_patrol must propagate CacheError::InvalidRow as Err"
        );
        let msg = result.unwrap_err().to_string();
        // The anyhow message must mention the corrupt data context.
        assert!(
            msg.contains("corrupt") || msg.contains("invalid"),
            "error message should describe the corruption, got: {msg}"
        );
    }

    /// When the slung bead query succeeds (no corruption), `run_patrol` must
    /// return `Ok` even if there are stuck beads mixed with fresh ones — confirming
    /// the baseline happy-path is unaffected by the error-handling changes.
    #[tokio::test]
    async fn run_patrol_ok_with_valid_slung_beads_after_error_handling_change() {
        let runner = PatrolRunner::new(60).with_slung_timeout(ChronoDuration::minutes(10));
        let cache = CacheDb::new_in_memory().await.expect("cache");

        let fresh = make_slung_bead(Some(Utc::now() - ChronoDuration::minutes(1)));
        let stuck = make_slung_bead(Some(Utc::now() - ChronoDuration::hours(1)));
        insert_beads(&cache, &[fresh, stuck]).await;

        let report = runner.run_patrol(&cache).await.expect("patrol must succeed");
        assert_eq!(report.stuck_beads, 1);
    }
}
