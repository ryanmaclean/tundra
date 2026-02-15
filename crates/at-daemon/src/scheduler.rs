use anyhow::Result;
use at_core::cache::CacheDb;
use at_core::types::{Bead, BeadStatus, Lane};
use chrono::Utc;
use tracing::{debug, info};
use uuid::Uuid;

/// Assigns beads from the backlog to agents based on priority ordering.
///
/// Priority rules (highest to lowest):
/// 1. Critical lane first, then Standard, then Experimental.
/// 2. Within the same lane, higher `priority` field wins.
/// 3. Ties broken by `created_at` (oldest first).
pub struct TaskScheduler;

impl TaskScheduler {
    /// Create a new task scheduler.
    pub fn new() -> Self {
        Self
    }

    /// Pick the highest-priority backlog bead.
    ///
    /// Returns `None` when the backlog is empty.
    pub async fn next_bead(&self, cache: &CacheDb) -> Option<Bead> {
        let mut backlog = cache
            .list_beads_by_status(BeadStatus::Backlog)
            .await
            .ok()?;

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
    pub async fn assign_bead(
        &self,
        cache: &CacheDb,
        bead_id: Uuid,
        agent_id: Uuid,
    ) -> Result<()> {
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
        Self::new()
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
