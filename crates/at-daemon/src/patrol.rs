use std::sync::Arc;

use anyhow::Result;
use at_bridge::http_api::ApiState;
use at_core::cache::CacheDb;
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
            .map_err(|e| anyhow::anyhow!("failed to query slung beads: {}", e))?;

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
}
