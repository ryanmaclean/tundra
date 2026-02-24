use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::ApiState;
use at_bridge::terminal::{TerminalInfo, TerminalStatus};
use at_core::cache::CacheDb;
use at_core::types::{Bead, BeadStatus, Lane};
use at_daemon::patrol::{reap_orphan_ptys, PatrolRunner};
use chrono::{Duration, Utc};
use uuid::Uuid;

#[tokio::test]
async fn patrol_empty_cache_returns_clean_report() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");
    let runner = PatrolRunner::new(30);

    let report = runner
        .run_patrol(&cache)
        .await
        .expect("patrol should succeed");

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
    let report = runner
        .run_patrol(&cache)
        .await
        .expect("patrol should succeed");

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
    let report = runner
        .run_patrol(&cache)
        .await
        .expect("patrol should succeed");

    assert_eq!(report.stuck_beads, 0);
}

// ---------------------------------------------------------------------------
// Orphan PTY reaper tests
// ---------------------------------------------------------------------------

fn make_test_terminal(status: TerminalStatus) -> TerminalInfo {
    TerminalInfo {
        id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        title: "test terminal".to_string(),
        status,
        cols: 80,
        rows: 24,
        font_size: 14,
        font_family: "\"Iosevka Term\",\"JetBrains Mono\",\"SF Mono\",\"Menlo\",monospace"
            .to_string(),
        line_height: 1.02,
        letter_spacing: 0.15,
        profile: "bundled-card".to_string(),
        cursor_style: "block".to_string(),
        cursor_blink: true,
        auto_name: None,
        persistent: false,
    }
}

#[tokio::test]
async fn reap_orphan_ptys_empty_state_returns_zero() {
    let state = Arc::new(ApiState::new(EventBus::new()));
    let reaped = reap_orphan_ptys(&state).await;
    assert_eq!(reaped, 0);
}

#[tokio::test]
async fn reap_orphan_ptys_detects_terminal_without_pty_handle() {
    let state = Arc::new(ApiState::new(EventBus::new()));

    // Register a terminal but do NOT insert a corresponding PTY handle.
    let info = make_test_terminal(TerminalStatus::Active);
    let tid = info.id;
    {
        let mut registry = state.terminal_registry.write().await;
        registry.register(info);
    }

    // The terminal has no PTY handle, so it should be considered an orphan.
    let reaped = reap_orphan_ptys(&state).await;
    assert_eq!(reaped, 1);

    // After reaping, the terminal should be removed from the registry.
    {
        let registry = state.terminal_registry.read().await;
        assert!(registry.get(&tid).is_none());
    }
}

#[tokio::test]
async fn reap_orphan_ptys_multiple_orphans() {
    let state = Arc::new(ApiState::new(EventBus::new()));

    // Register three terminals, none with PTY handles.
    for _ in 0..3 {
        let info = make_test_terminal(TerminalStatus::Active);
        let mut registry = state.terminal_registry.write().await;
        registry.register(info);
    }

    let reaped = reap_orphan_ptys(&state).await;
    assert_eq!(reaped, 3);

    // All should be removed.
    let registry = state.terminal_registry.read().await;
    assert!(registry.list().is_empty());
}

#[tokio::test]
async fn reap_orphan_ptys_idempotent_on_second_call() {
    let state = Arc::new(ApiState::new(EventBus::new()));

    let info = make_test_terminal(TerminalStatus::Active);
    {
        let mut registry = state.terminal_registry.write().await;
        registry.register(info);
    }

    let first = reap_orphan_ptys(&state).await;
    assert_eq!(first, 1);

    // Second call should find nothing.
    let second = reap_orphan_ptys(&state).await;
    assert_eq!(second, 0);
}
