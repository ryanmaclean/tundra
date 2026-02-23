use std::sync::Arc;
use std::time::Duration;

use at_core::cache::CacheDb;
use at_core::types::{Agent, AgentRole, Bead, BeadStatus, CliType, Lane};
use at_daemon::scheduler::TaskScheduler;
use uuid::Uuid;

#[tokio::test]
async fn empty_backlog_returns_none() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");
    let scheduler = TaskScheduler::default();

    let next = scheduler.next_bead(&cache).await;
    assert!(next.is_none());
}

#[tokio::test]
async fn critical_lane_takes_priority_over_standard() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    let standard = Bead::new("standard task", Lane::Standard);
    cache.upsert_bead(&standard).await.expect("upsert");

    let critical = Bead::new("critical task", Lane::Critical);
    cache.upsert_bead(&critical).await.expect("upsert");

    let experimental = Bead::new("experimental task", Lane::Experimental);
    cache.upsert_bead(&experimental).await.expect("upsert");

    let scheduler = TaskScheduler::default();
    let next = scheduler
        .next_bead(&cache)
        .await
        .expect("should find a bead");

    assert_eq!(
        next.id, critical.id,
        "critical lane bead should be picked first"
    );
}

#[tokio::test]
async fn higher_priority_wins_within_same_lane() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    let mut low = Bead::new("low priority", Lane::Standard);
    low.priority = 1;
    cache.upsert_bead(&low).await.expect("upsert");

    let mut high = Bead::new("high priority", Lane::Standard);
    high.priority = 10;
    cache.upsert_bead(&high).await.expect("upsert");

    let mut mid = Bead::new("mid priority", Lane::Standard);
    mid.priority = 5;
    cache.upsert_bead(&mid).await.expect("upsert");

    let scheduler = TaskScheduler::default();
    let next = scheduler
        .next_bead(&cache)
        .await
        .expect("should find a bead");

    assert_eq!(next.id, high.id, "highest priority bead should be picked");
}

#[tokio::test]
async fn older_bead_wins_on_tie() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    let mut older = Bead::new("older task", Lane::Standard);
    older.priority = 5;
    older.created_at = chrono::Utc::now() - chrono::Duration::hours(2);
    older.updated_at = older.created_at;
    cache.upsert_bead(&older).await.expect("upsert");

    let mut newer = Bead::new("newer task", Lane::Standard);
    newer.priority = 5;
    newer.created_at = chrono::Utc::now();
    newer.updated_at = newer.created_at;
    cache.upsert_bead(&newer).await.expect("upsert");

    let scheduler = TaskScheduler::default();
    let next = scheduler
        .next_bead(&cache)
        .await
        .expect("should find a bead");

    assert_eq!(next.id, older.id, "older bead should win on priority tie");
}

#[tokio::test]
async fn assign_bead_transitions_to_hooked() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    let bead = Bead::new("assignable task", Lane::Standard);
    let bead_id = bead.id;
    cache.upsert_bead(&bead).await.expect("upsert");

    let agent = Agent::new("worker", AgentRole::Crew, CliType::Claude);
    let agent_id = agent.id;
    cache.upsert_agent(&agent).await.expect("upsert");

    let scheduler = TaskScheduler::default();
    scheduler
        .assign_bead(&cache, bead_id, agent_id)
        .await
        .expect("assign should succeed");

    let updated = cache
        .get_bead(bead_id)
        .await
        .expect("get")
        .expect("bead exists");
    assert_eq!(updated.status, BeadStatus::Hooked);
    assert_eq!(updated.agent_id, Some(agent_id));
    assert!(updated.hooked_at.is_some());
}

#[tokio::test]
async fn assign_bead_rejects_invalid_transition() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    // A bead already in Done status cannot transition to Hooked.
    let mut bead = Bead::new("done task", Lane::Standard);
    bead.status = BeadStatus::Done;
    let bead_id = bead.id;
    cache.upsert_bead(&bead).await.expect("upsert");

    let scheduler = TaskScheduler::default();
    let result = scheduler.assign_bead(&cache, bead_id, Uuid::new_v4()).await;

    assert!(result.is_err(), "should reject invalid status transition");
}

#[tokio::test]
async fn only_backlog_beads_are_scheduled() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    // Insert a hooked bead -- should not appear in next_bead.
    let mut hooked = Bead::new("hooked task", Lane::Critical);
    hooked.status = BeadStatus::Hooked;
    cache.upsert_bead(&hooked).await.expect("upsert");

    let scheduler = TaskScheduler::default();
    let next = scheduler.next_bead(&cache).await;
    assert!(next.is_none(), "hooked beads should not be scheduled");
}

// ===========================================================================
// Concurrency gate (semaphore) tests
// ===========================================================================

#[tokio::test]
async fn default_scheduler_has_10_slots() {
    let scheduler = TaskScheduler::default();
    assert_eq!(scheduler.max_concurrent(), 10);
    assert_eq!(scheduler.available_slots(), 10);
}

#[tokio::test]
async fn custom_concurrency_limit() {
    let scheduler = TaskScheduler::new(3);
    assert_eq!(scheduler.max_concurrent(), 3);
    assert_eq!(scheduler.available_slots(), 3);
}

#[tokio::test]
async fn zero_concurrency_falls_back_to_default() {
    let scheduler = TaskScheduler::new(0);
    assert_eq!(scheduler.max_concurrent(), 10);
    assert_eq!(scheduler.available_slots(), 10);
}

#[tokio::test]
async fn acquiring_permit_reduces_available_slots() {
    let scheduler = TaskScheduler::new(2);
    let gate = scheduler.concurrency_gate();

    let _permit1 = gate.acquire().await.expect("acquire permit");
    assert_eq!(scheduler.available_slots(), 1);

    let _permit2 = gate.acquire().await.expect("acquire permit");
    assert_eq!(scheduler.available_slots(), 0);
}

#[tokio::test]
async fn dropping_permit_restores_slot() {
    let scheduler = TaskScheduler::new(1);
    let gate = scheduler.concurrency_gate();

    let permit = gate.acquire().await.expect("acquire permit");
    assert_eq!(scheduler.available_slots(), 0);

    drop(permit);
    assert_eq!(scheduler.available_slots(), 1);
}

#[tokio::test]
async fn semaphore_blocks_when_exhausted() {
    let scheduler = Arc::new(TaskScheduler::new(1));
    let gate = scheduler.concurrency_gate();

    // Acquire the only permit.
    let _permit = gate.clone().acquire_owned().await.expect("acquire permit");
    assert_eq!(scheduler.available_slots(), 0);

    // Spawning a second acquire should not complete within the timeout.
    let gate2 = gate.clone();
    let handle = tokio::spawn(async move {
        gate2.acquire().await.expect("acquire permit");
    });

    // Give it a brief moment — it should NOT complete.
    let result = tokio::time::timeout(Duration::from_millis(50), handle).await;
    assert!(
        result.is_err(),
        "second acquire should block when semaphore is exhausted"
    );
}

#[tokio::test]
async fn concurrency_gate_is_shared_across_clones() {
    let scheduler = TaskScheduler::new(2);
    let gate_a = scheduler.concurrency_gate();
    let gate_b = scheduler.concurrency_gate();

    let _permit = gate_a.acquire().await.expect("acquire from gate_a");
    // gate_b shares the same underlying semaphore.
    assert_eq!(scheduler.available_slots(), 1);

    let _permit2 = gate_b.acquire().await.expect("acquire from gate_b");
    assert_eq!(scheduler.available_slots(), 0);
}
