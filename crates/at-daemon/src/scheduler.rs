use std::sync::Arc;

use anyhow::Result;
use at_core::cache::CacheDb;
use at_core::types::{Bead, BeadStatus, Lane};
use chrono::Utc;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Default maximum number of concurrent agents when none is specified.
const DEFAULT_MAX_CONCURRENT: u32 = 10;

/// Assigns beads from the backlog to agents based on priority ordering.
///
/// Priority rules (highest to lowest):
/// 1. Critical lane first, then Standard, then Experimental.
/// 2. Within the same lane, higher `priority` field wins.
/// 3. Ties broken by `created_at` (oldest first).
///
/// Enforces a concurrency limit via a [`Semaphore`]. Callers must acquire a
/// permit from [`concurrency_gate`](Self::concurrency_gate) before spawning an
/// agent, and drop the permit when the agent reaches a terminal state.
pub struct TaskScheduler {
    concurrency_gate: Arc<Semaphore>,
    max_concurrent: u32,
}

impl TaskScheduler {
    /// Create a new task scheduler with the given concurrency limit.
    pub fn new(max_concurrent: u32) -> Self {
        let limit = if max_concurrent == 0 {
            warn!("max_concurrent was 0, defaulting to {DEFAULT_MAX_CONCURRENT}");
            DEFAULT_MAX_CONCURRENT
        } else {
            max_concurrent
        };
        Self {
            concurrency_gate: Arc::new(Semaphore::new(limit as usize)),
            max_concurrent: limit,
        }
    }

    /// Returns a clone of the concurrency semaphore.
    ///
    /// External code (e.g. the orchestrator) should call
    /// `semaphore.acquire_owned().await` before spawning an agent and hold the
    /// resulting `OwnedSemaphorePermit` until the agent finishes.
    pub fn concurrency_gate(&self) -> Arc<Semaphore> {
        Arc::clone(&self.concurrency_gate)
    }

    /// Returns the number of agent slots currently available.
    pub fn available_slots(&self) -> usize {
        self.concurrency_gate.available_permits()
    }

    /// Returns the configured maximum concurrency.
    pub fn max_concurrent(&self) -> u32 {
        self.max_concurrent
    }

    /// Pick the highest-priority backlog bead.
    ///
    /// Returns `None` when the backlog is empty.
    pub async fn next_bead(&self, cache: &CacheDb) -> Option<Bead> {
        let mut backlog = cache.list_beads_by_status(BeadStatus::Backlog).await.ok()?;

        if backlog.is_empty() {
            return None;
        }

        // Sort: Critical > Standard > Experimental, then priority desc, then created_at asc.
        backlog.sort_by(|a, b| {
            let lane_ord = lane_rank(&b.lane).cmp(&lane_rank(&a.lane));
            if lane_ord != std::cmp::Ordering::Equal {
                return lane_ord;
            }
            let prio_ord = b.priority.cmp(&a.priority);
            if prio_ord != std::cmp::Ordering::Equal {
                return prio_ord;
            }
            a.created_at.cmp(&b.created_at)
        });

        debug!(
            bead_id = %backlog[0].id,
            lane = ?backlog[0].lane,
            priority = backlog[0].priority,
            "next bead selected"
        );

        Some(backlog.remove(0))
    }

    /// Assign a bead to an agent by transitioning it to `Hooked` status.
    ///
    /// Updates the bead's `agent_id`, `status`, `hooked_at`, and `updated_at`
    /// fields, then persists the change via `cache.upsert_bead`.
    pub async fn assign_bead(&self, cache: &CacheDb, bead_id: Uuid, agent_id: Uuid) -> Result<()> {
        let bead = cache
            .get_bead(bead_id)
            .await
            .map_err(|e| anyhow::anyhow!("failed to fetch bead {}: {}", bead_id, e))?
            .ok_or_else(|| anyhow::anyhow!("bead {} not found", bead_id))?;

        if !bead.status.can_transition_to(&BeadStatus::Hooked) {
            anyhow::bail!(
                "bead {} cannot transition from {:?} to Hooked",
                bead_id,
                bead.status
            );
        }

        let now = Utc::now();
        let mut updated = bead;
        updated.status = BeadStatus::Hooked;
        updated.agent_id = Some(agent_id);
        updated.hooked_at = Some(now);
        updated.updated_at = now;

        cache
            .upsert_bead(&updated)
            .await
            .map_err(|e| anyhow::anyhow!("failed to upsert bead {}: {}", bead_id, e))?;

        info!(
            bead_id = %bead_id,
            agent_id = %agent_id,
            "bead assigned to agent"
        );

        Ok(())
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CONCURRENT)
    }
}

/// Map lane variants to a numeric rank for sorting (higher = more important).
fn lane_rank(lane: &Lane) -> u8 {
    match lane {
        Lane::Critical => 2,
        Lane::Standard => 1,
        Lane::Experimental => 0,
    }
}
