use std::collections::VecDeque;
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
        let backlog = cache.list_beads_by_status(BeadStatus::Backlog).await.ok()?;
        let mut backlog = VecDeque::from(backlog);

        if backlog.is_empty() {
            return None;
        }

        // Sort: Critical > Standard > Experimental, then priority desc, then created_at asc.
        backlog.make_contiguous().sort_by(|a, b| {
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

        backlog.pop_front()
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_bead(lane: Lane, priority: i32, status: BeadStatus) -> Bead {
        let mut b = Bead::new("test", lane);
        b.priority = priority;
        b.status = status;
        b
    }

    // ----- lane_rank -----

    #[test]
    fn lane_rank_critical_is_highest() {
        assert!(lane_rank(&Lane::Critical) > lane_rank(&Lane::Standard));
        assert!(lane_rank(&Lane::Standard) > lane_rank(&Lane::Experimental));
    }

    #[test]
    fn lane_rank_exact_values() {
        assert_eq!(lane_rank(&Lane::Critical), 2);
        assert_eq!(lane_rank(&Lane::Standard), 1);
        assert_eq!(lane_rank(&Lane::Experimental), 0);
    }

    // ----- TaskScheduler constructor -----

    #[test]
    fn new_with_zero_falls_back_to_default() {
        let s = TaskScheduler::new(0);
        assert_eq!(s.max_concurrent(), DEFAULT_MAX_CONCURRENT);
        assert_eq!(s.available_slots(), DEFAULT_MAX_CONCURRENT as usize);
    }

    #[test]
    fn new_respects_positive_max() {
        let s = TaskScheduler::new(5);
        assert_eq!(s.max_concurrent(), 5);
        assert_eq!(s.available_slots(), 5);
    }

    #[test]
    fn default_uses_default_max_concurrent() {
        let s = TaskScheduler::default();
        assert_eq!(s.max_concurrent(), DEFAULT_MAX_CONCURRENT);
        assert_eq!(s.available_slots(), DEFAULT_MAX_CONCURRENT as usize);
    }

    #[test]
    fn new_with_one_allows_single_slot() {
        let s = TaskScheduler::new(1);
        assert_eq!(s.max_concurrent(), 1);
        assert_eq!(s.available_slots(), 1);
    }

    // ----- concurrency_gate semantics -----

    #[tokio::test]
    async fn concurrency_gate_returns_arc_clone() {
        let s = TaskScheduler::new(3);
        let g1 = s.concurrency_gate();
        let g2 = s.concurrency_gate();
        // Both clones share state — acquiring from one should reduce slots seen by the other.
        let _permit = g1.acquire().await.unwrap();
        assert_eq!(g2.available_permits(), 2);
        assert_eq!(s.available_slots(), 2);
    }

    #[tokio::test]
    async fn concurrency_gate_release_restores_slots() {
        let s = TaskScheduler::new(2);
        {
            let _permit = s.concurrency_gate().acquire_owned().await.unwrap();
            assert_eq!(s.available_slots(), 1);
        }
        // Permit dropped — slot should be back.
        assert_eq!(s.available_slots(), 2);
    }

    #[tokio::test]
    async fn concurrency_gate_blocks_when_exhausted() {
        let s = TaskScheduler::new(1);
        let gate = s.concurrency_gate();
        let _permit = gate.clone().acquire_owned().await.unwrap();
        assert_eq!(s.available_slots(), 0);

        // try_acquire must fail since the only permit is held.
        assert!(gate.try_acquire().is_err());
    }

    // ----- next_bead: priority / lane / tie-break ordering -----

    #[tokio::test]
    async fn next_bead_returns_none_when_backlog_empty() {
        let cache = CacheDb::new_in_memory().await.unwrap();
        let s = TaskScheduler::new(4);
        assert!(s.next_bead(&cache).await.is_none());
    }

    #[tokio::test]
    async fn next_bead_picks_critical_lane_first() {
        let cache = CacheDb::new_in_memory().await.unwrap();
        let s = TaskScheduler::new(4);

        let std_bead = make_bead(Lane::Standard, 100, BeadStatus::Backlog);
        let crit_bead = make_bead(Lane::Critical, 0, BeadStatus::Backlog);
        let exp_bead = make_bead(Lane::Experimental, 200, BeadStatus::Backlog);
        cache.upsert_bead(&std_bead).await.unwrap();
        cache.upsert_bead(&crit_bead).await.unwrap();
        cache.upsert_bead(&exp_bead).await.unwrap();

        let picked = s.next_bead(&cache).await.expect("expected a bead");
        assert_eq!(picked.id, crit_bead.id);
        assert_eq!(picked.lane, Lane::Critical);
    }

    #[tokio::test]
    async fn next_bead_picks_higher_priority_within_same_lane() {
        let cache = CacheDb::new_in_memory().await.unwrap();
        let s = TaskScheduler::new(4);

        let low = make_bead(Lane::Standard, 1, BeadStatus::Backlog);
        let high = make_bead(Lane::Standard, 99, BeadStatus::Backlog);
        cache.upsert_bead(&low).await.unwrap();
        cache.upsert_bead(&high).await.unwrap();

        let picked = s.next_bead(&cache).await.expect("expected a bead");
        assert_eq!(picked.id, high.id);
        assert_eq!(picked.priority, 99);
    }

    #[tokio::test]
    async fn next_bead_breaks_ties_with_oldest_created_at() {
        let cache = CacheDb::new_in_memory().await.unwrap();
        let s = TaskScheduler::new(4);

        let now = Utc::now();
        let mut older = make_bead(Lane::Standard, 5, BeadStatus::Backlog);
        older.created_at = now - Duration::hours(2);
        let mut newer = make_bead(Lane::Standard, 5, BeadStatus::Backlog);
        newer.created_at = now;

        // Insert in "newer first" order to ensure ordering isn't accidental.
        cache.upsert_bead(&newer).await.unwrap();
        cache.upsert_bead(&older).await.unwrap();

        let picked = s.next_bead(&cache).await.expect("expected a bead");
        assert_eq!(picked.id, older.id);
    }

    #[tokio::test]
    async fn next_bead_ignores_non_backlog_status() {
        let cache = CacheDb::new_in_memory().await.unwrap();
        let s = TaskScheduler::new(4);

        // Only non-backlog beads exist — picker should return None.
        let hooked = make_bead(Lane::Critical, 50, BeadStatus::Hooked);
        let done = make_bead(Lane::Critical, 99, BeadStatus::Done);
        cache.upsert_bead(&hooked).await.unwrap();
        cache.upsert_bead(&done).await.unwrap();

        assert!(s.next_bead(&cache).await.is_none());
    }

    // ----- assign_bead: state transitions -----

    #[tokio::test]
    async fn assign_bead_transitions_backlog_to_hooked() {
        let cache = CacheDb::new_in_memory().await.unwrap();
        let s = TaskScheduler::new(4);

        let bead = make_bead(Lane::Standard, 0, BeadStatus::Backlog);
        cache.upsert_bead(&bead).await.unwrap();

        let agent_id = Uuid::new_v4();
        s.assign_bead(&cache, bead.id, agent_id).await.unwrap();

        let stored = cache.get_bead(bead.id).await.unwrap().unwrap();
        assert_eq!(stored.status, BeadStatus::Hooked);
        assert_eq!(stored.agent_id, Some(agent_id));
        assert!(stored.hooked_at.is_some());
    }

    #[tokio::test]
    async fn assign_bead_returns_error_when_bead_missing() {
        let cache = CacheDb::new_in_memory().await.unwrap();
        let s = TaskScheduler::new(4);

        let result = s.assign_bead(&cache, Uuid::new_v4(), Uuid::new_v4()).await;
        assert!(result.is_err(), "expected error for missing bead");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not found"), "unexpected error: {msg}");
    }

    #[tokio::test]
    async fn assign_bead_rejects_invalid_transition_from_done() {
        let cache = CacheDb::new_in_memory().await.unwrap();
        let s = TaskScheduler::new(4);

        let bead = make_bead(Lane::Standard, 0, BeadStatus::Done);
        cache.upsert_bead(&bead).await.unwrap();

        let result = s.assign_bead(&cache, bead.id, Uuid::new_v4()).await;
        assert!(result.is_err(), "Done -> Hooked must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("cannot transition"), "unexpected error: {msg}");
    }

    #[tokio::test]
    async fn assign_bead_rejects_invalid_transition_from_slung() {
        let cache = CacheDb::new_in_memory().await.unwrap();
        let s = TaskScheduler::new(4);

        let bead = make_bead(Lane::Standard, 0, BeadStatus::Slung);
        cache.upsert_bead(&bead).await.unwrap();

        let result = s.assign_bead(&cache, bead.id, Uuid::new_v4()).await;
        assert!(result.is_err(), "Slung -> Hooked must be rejected");
    }

    #[tokio::test]
    async fn assign_bead_does_not_mutate_on_failed_transition() {
        let cache = CacheDb::new_in_memory().await.unwrap();
        let s = TaskScheduler::new(4);

        let bead = make_bead(Lane::Standard, 0, BeadStatus::Done);
        let original_updated_at = bead.updated_at;
        cache.upsert_bead(&bead).await.unwrap();

        let _ = s.assign_bead(&cache, bead.id, Uuid::new_v4()).await;

        let stored = cache.get_bead(bead.id).await.unwrap().unwrap();
        assert_eq!(stored.status, BeadStatus::Done);
        assert!(stored.agent_id.is_none());
        // updated_at should not have been bumped because we returned early.
        assert_eq!(stored.updated_at, original_updated_at);
    }

    // ----- serde round-trip for persisted state -----

    #[test]
    fn bead_status_serde_roundtrip() {
        for variant in [
            BeadStatus::Backlog,
            BeadStatus::Hooked,
            BeadStatus::Slung,
            BeadStatus::Review,
            BeadStatus::Done,
            BeadStatus::Failed,
            BeadStatus::Escalated,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let back: BeadStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn lane_serde_roundtrip() {
        for variant in [Lane::Critical, Lane::Standard, Lane::Experimental] {
            let json = serde_json::to_string(&variant).unwrap();
            let back: Lane = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }
}
