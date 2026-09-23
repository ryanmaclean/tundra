use std::sync::Arc;

use anyhow::Result;
use at_core::cache::CacheDb;
use at_core::types::{Bead, BeadStatus, Lane};
use chrono::{DateTime, Utc};
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Default maximum number of concurrent agents when none is specified.
const DEFAULT_MAX_CONCURRENT: u32 = 10;

/// Assigns beads from the backlog to agents based on priority ordering.
///
/// Ordering rules (highest to lowest), see [`rank_beads`]:
/// 1. Critical lane first, then Standard, then Experimental.
/// 2. Within the same lane, higher [`score`] wins (priority, age, convoy age,
///    retry penalty).
/// 3. Ties broken by `created_at` (oldest first), then by `id` (ascending).
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

    /// Pick the highest-ranked backlog bead (see [`rank_beads`]).
    ///
    /// Returns `None` when the backlog is empty.
    pub async fn next_bead(&self, cache: &CacheDb) -> Option<Bead> {
        let mut backlog = cache.list_beads_by_status(BeadStatus::Backlog).await.ok()?;
        if backlog.is_empty() {
            return None;
        }

        let now = Utc::now();
        rank_beads(&mut backlog, now);
        let next = backlog.into_iter().next()?;

        debug!(
            bead_id = %next.id,
            lane = ?next.lane,
            priority = next.priority,
            score = score(&next, now),
            "next bead selected"
        );

        Some(next)
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
// Priority scoring
//
// Derived from gastown `internal/refinery/score.go` (ScoreConfig / ScoreMR).
// Copyright (c) 2025 Steve Yegge. Used under the MIT License:
// https://github.com/steveyegge/gastown/blob/main/LICENSE
// ---------------------------------------------------------------------------

/// Bead `metadata` key holding the number of times the bead has been retried
/// (non-negative integer). Absent means 0.
pub const RETRY_COUNT_KEY: &str = "retry_count";

/// Bead `metadata` key holding the RFC 3339 creation time of the bead's convoy.
/// Only consulted when `bead.convoy_id` is set. Absent means "no convoy bonus".
pub const CONVOY_CREATED_AT_KEY: &str = "convoy_created_at";

/// Tunable weights for [`score_with`]. Higher score = scheduled sooner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreWeights {
    /// Starting score before any factor (keeps scores positive).
    pub base: i64,
    /// Points per full hour of convoy age (anti-starvation for old convoys).
    pub convoy_age_per_hour: i64,
    /// Points per unit of bead `priority` (tundra: higher = more urgent).
    pub priority: i64,
    /// Points subtracted per retry (anti-thrash).
    pub retry_penalty: i64,
    /// Cap on the total retry penalty.
    pub max_retry_penalty: i64,
    /// Points per full hour of bead age (FIFO tiebreaker).
    pub bead_age_per_hour: i64,
}

impl Default for ScoreWeights {
    /// gastown `DefaultScoreConfig()`: 1000 / 10 / 100 / 50 / 300 / 1.
    fn default() -> Self {
        Self {
            base: 1000,
            convoy_age_per_hour: 10,
            priority: 100,
            retry_penalty: 50,
            max_retry_penalty: 300,
            bead_age_per_hour: 1,
        }
    }
}

/// Inputs to [`score_with`], decoupled from [`Bead`] so callers can supply a
/// convoy creation time from an external lookup (gastown `ScoreInput`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreInput {
    /// Bead priority; higher is more urgent. Negative values score as 0.
    pub priority: i32,
    /// When the bead was created.
    pub created_at: DateTime<Utc>,
    /// When the bead's convoy was created, if it belongs to one.
    pub convoy_created_at: Option<DateTime<Utc>>,
    /// How many times the bead has been retried (0 = first attempt).
    pub retry_count: u32,
}

impl ScoreInput {
    /// Build scoring inputs from a bead, reading [`RETRY_COUNT_KEY`] and
    /// [`CONVOY_CREATED_AT_KEY`] from its metadata.
    pub fn from_bead(bead: &Bead) -> Self {
        let meta = bead.metadata.as_ref();
        let retry_count = meta
            .and_then(|m| m.get(RETRY_COUNT_KEY))
            .and_then(serde_json::Value::as_u64)
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX));
        let convoy_created_at = bead
            .convoy_id
            .and(meta)
            .and_then(|m| m.get(CONVOY_CREATED_AT_KEY))
            .and_then(serde_json::Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc));
        Self {
            priority: bead.priority,
            created_at: bead.created_at,
            convoy_created_at,
            retry_count,
        }
    }
}

/// Score a bead with the default weights. Higher = scheduled sooner.
///
/// ```text
/// score = 1000
///       + 10  * whole_hours(now - convoy_created_at)   // if in a convoy
///       + 100 * max(priority, 0)
///       - min(50 * retry_count, 300)
///       + 1   * whole_hours(now - created_at)
/// ```
///
/// Pure: depends only on `bead` and `now`.
pub fn score(bead: &Bead, now: DateTime<Utc>) -> i64 {
    score_with(&ScoreInput::from_bead(bead), now, &ScoreWeights::default())
}

/// Score explicit inputs with explicit weights (gastown `ScoreMR`).
///
/// Ages in the future (clock skew) contribute 0.
pub fn score_with(input: &ScoreInput, now: DateTime<Utc>, w: &ScoreWeights) -> i64 {
    let mut total = w.base;

    if let Some(convoy_at) = input.convoy_created_at {
        total = total.saturating_add(
            w.convoy_age_per_hour
                .saturating_mul(whole_hours(convoy_at, now)),
        );
    }

    // gastown uses `weight * clamp(4 - p, 0, 4)` with P0 = most urgent. Tundra's
    // `priority` runs the other way (higher = more urgent, unbounded), so the
    // bonus is `weight * p`, clamped at 0 from below.
    total = total.saturating_add(w.priority.saturating_mul(i64::from(input.priority.max(0))));

    let penalty = w
        .retry_penalty
        .saturating_mul(i64::from(input.retry_count))
        .min(w.max_retry_penalty);
    total = total.saturating_sub(penalty);

    total.saturating_add(
        w.bead_age_per_hour
            .saturating_mul(whole_hours(input.created_at, now)),
    )
}

/// Whole hours from `since` to `now`, 0 if `since` is in the future.
fn whole_hours(since: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    now.signed_duration_since(since).num_hours().max(0)
}

/// Sort beads into scheduling order, best first.
///
/// Keys: lane (Critical > Standard > Experimental), then [`score`] descending,
/// then `created_at` ascending, then `id` ascending. The final `id` key makes
/// the order total, so the result does not depend on input order.
pub fn rank_beads(beads: &mut [Bead], now: DateTime<Utc>) {
    beads.sort_by_cached_key(|b| {
        (
            std::cmp::Reverse(lane_rank(&b.lane)),
            std::cmp::Reverse(score(b, now)),
            b.created_at,
            b.id,
        )
    });
}

#[cfg(test)]
mod score_tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use serde_json::json;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    /// A bead created exactly at `t0()` with no priority, retries, or convoy.
    fn bead(title: &str) -> Bead {
        let mut b = Bead::new(title, Lane::Standard);
        b.created_at = t0();
        b.updated_at = t0();
        b
    }

    #[test]
    fn base_score_is_1000() {
        assert_eq!(score(&bead("b"), t0()), 1000);
    }

    #[test]
    fn bead_age_adds_one_point_per_whole_hour() {
        let b = bead("b");
        assert_eq!(score(&b, t0() + Duration::minutes(59)), 1000);
        assert_eq!(score(&b, t0() + Duration::hours(1)), 1001);
        assert_eq!(score(&b, t0() + Duration::hours(48)), 1048);
    }

    #[test]
    fn future_created_at_contributes_nothing() {
        assert_eq!(score(&bead("b"), t0() - Duration::hours(5)), 1000);
    }

    #[test]
    fn priority_adds_100_per_unit_and_negative_clamps_to_zero() {
        let mut b = bead("b");
        b.priority = 3;
        assert_eq!(score(&b, t0()), 1300);
        b.priority = -7;
        assert_eq!(score(&b, t0()), 1000);
    }

    #[test]
    fn retry_penalty_is_50_per_retry_capped_at_300() {
        let mut b = bead("b");
        b.metadata = Some(json!({ RETRY_COUNT_KEY: 2 }));
        assert_eq!(score(&b, t0()), 900);
        b.metadata = Some(json!({ RETRY_COUNT_KEY: 6 }));
        assert_eq!(score(&b, t0()), 700);
        b.metadata = Some(json!({ RETRY_COUNT_KEY: 40 }));
        assert_eq!(score(&b, t0()), 700, "penalty is capped at 300");
    }

    #[test]
    fn convoy_age_adds_10_per_hour_only_when_in_a_convoy() {
        let convoy_at = (t0() - Duration::hours(24)).to_rfc3339();
        let mut b = bead("b");
        b.metadata = Some(json!({ CONVOY_CREATED_AT_KEY: convoy_at }));
        assert_eq!(score(&b, t0()), 1000, "no convoy_id: convoy age ignored");

        b.convoy_id = Some(Uuid::new_v4());
        assert_eq!(score(&b, t0()), 1240);
    }

    #[test]
    fn terms_combine_like_gastown_formula() {
        let mut b = bead("b");
        b.priority = 2;
        b.convoy_id = Some(Uuid::new_v4());
        b.metadata = Some(json!({
            RETRY_COUNT_KEY: 1,
            CONVOY_CREATED_AT_KEY: (t0() - Duration::hours(10)).to_rfc3339(),
        }));
        let now = t0() + Duration::hours(3);
        // 1000 + 10*13 (convoy) + 100*2 - 50*1 + 1*3 (bead)
        assert_eq!(score(&b, now), 1000 + 130 + 200 - 50 + 3);
    }

    #[test]
    fn custom_weights_are_honoured() {
        let input = ScoreInput {
            priority: 1,
            created_at: t0(),
            convoy_created_at: None,
            retry_count: 0,
        };
        let w = ScoreWeights {
            base: 0,
            priority: 7,
            ..ScoreWeights::default()
        };
        assert_eq!(score_with(&input, t0(), &w), 7);
    }

    #[test]
    fn lane_outranks_any_score() {
        let mut exp = bead("experimental");
        exp.lane = Lane::Experimental;
        exp.priority = 1_000;
        let crit = {
            let mut b = bead("critical");
            b.lane = Lane::Critical;
            b
        };
        let mut v = vec![exp, crit.clone()];
        rank_beads(&mut v, t0());
        assert_eq!(v[0].id, crit.id);
    }

    #[test]
    fn old_retried_bead_can_outrank_fresh_one_via_age() {
        // 1 retry (-50) is overcome by 60h of age (+60).
        let mut old = bead("old");
        old.metadata = Some(json!({ RETRY_COUNT_KEY: 1 }));
        let mut fresh = bead("fresh");
        fresh.created_at = t0() + Duration::hours(60);
        let mut v = vec![fresh, old.clone()];
        rank_beads(&mut v, t0() + Duration::hours(60));
        assert_eq!(v[0].id, old.id);
    }

    #[test]
    fn full_tie_breaks_by_id_regardless_of_input_order() {
        let a = bead("a");
        let b = bead("b");
        let c = bead("c");
        let mut ids = vec![a.id, b.id, c.id];
        ids.sort();

        for perm in [
            vec![a.clone(), b.clone(), c.clone()],
            vec![c.clone(), b.clone(), a.clone()],
            vec![b.clone(), c.clone(), a.clone()],
        ] {
            let mut v = perm;
            rank_beads(&mut v, t0());
            let got: Vec<Uuid> = v.iter().map(|x| x.id).collect();
            assert_eq!(got, ids, "equal-score beads must order by id ascending");
        }
    }

    #[test]
    fn equal_score_breaks_by_created_at_before_id() {
        // Both within the same whole hour, so bead age scores are equal.
        let mut earlier = bead("earlier");
        earlier.created_at = t0();
        let mut later = bead("later");
        later.created_at = t0() + Duration::minutes(10);
        // Force the id key to favour `later` so created_at must be what decides.
        if earlier.id < later.id {
            std::mem::swap(&mut earlier.id, &mut later.id);
        }
        let now = t0() + Duration::minutes(30);
        assert_eq!(score(&earlier, now), score(&later, now));
        let mut v = vec![later, earlier.clone()];
        rank_beads(&mut v, now);
        assert_eq!(v[0].id, earlier.id);
    }
}
