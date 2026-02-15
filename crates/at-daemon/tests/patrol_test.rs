use at_core::cache::CacheDb;
use at_core::types::{Bead, BeadStatus, Lane};
use at_daemon::patrol::PatrolRunner;
use chrono::{Duration, Utc};

#[tokio::test]
async fn patrol_empty_cache_returns_clean_report() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");
    let runner = PatrolRunner::new(30);

    let report = runner.run_patrol(&cache).await.expect("patrol should succeed");

    assert_eq!(report.stale_agents, 0);
    assert_eq!(report.stuck_beads, 0);
    assert_eq!(report.orphan_ptys, 0);
    assert!(report.stuck_bead_ids.is_empty());
}

#[tokio::test]
async fn patrol_detects_stuck_slung_beads() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    // Create a bead that has been slung for 2 hours (well past 30-minute timeout).
    let mut bead = Bead::new("stuck task", Lane::Standard);
    bead.status = BeadStatus::Slung;
    bead.slung_at = Some(Utc::now() - Duration::hours(2));
    cache.upsert_bead(&bead).await.expect("upsert bead");

    // Create a bead that was just slung (should not be stuck).
    let mut fresh_bead = Bead::new("fresh task", Lane::Standard);
    fresh_bead.status = BeadStatus::Slung;
    fresh_bead.slung_at = Some(Utc::now());
    cache.upsert_bead(&fresh_bead).await.expect("upsert bead");

    let runner = PatrolRunner::new(30);
    let report = runner.run_patrol(&cache).await.expect("patrol should succeed");

    assert_eq!(report.stuck_beads, 1);
    assert_eq!(report.stuck_bead_ids.len(), 1);
    assert_eq!(report.stuck_bead_ids[0], bead.id);
}

#[tokio::test]
async fn patrol_ignores_non_slung_beads() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    let bead = Bead::new("backlog task", Lane::Critical);
    cache.upsert_bead(&bead).await.expect("upsert bead");

    let runner = PatrolRunner::new(30);
    let report = runner.run_patrol(&cache).await.expect("patrol should succeed");

    assert_eq!(report.stuck_beads, 0);
}
