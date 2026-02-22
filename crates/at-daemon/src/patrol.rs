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

        debug!(
            stuck_beads = report.stuck_beads,
            "patrol sweep completed"
        );

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
