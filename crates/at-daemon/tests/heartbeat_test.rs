use std::time::Duration;

use at_core::cache::CacheDb;
use at_core::types::{Agent, AgentRole, AgentStatus, CliType};
use at_daemon::heartbeat::HeartbeatMonitor;
use chrono::Utc;
use uuid::Uuid;

#[tokio::test]
async fn no_registered_agents_returns_empty() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");
    let monitor = HeartbeatMonitor::new(Duration::from_secs(60));

    let stale = monitor
        .check_agents(&cache)
        .await
        .expect("check should succeed");
    assert!(stale.is_empty());
}

#[tokio::test]
async fn fresh_agent_is_not_stale() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    let mut agent = Agent::new("agent-fresh", AgentRole::Crew, CliType::Claude);
    agent.status = AgentStatus::Active;
    agent.last_seen = Utc::now();
    cache.upsert_agent(&agent).await.expect("upsert agent");

    let monitor = HeartbeatMonitor::new(Duration::from_secs(60));
    monitor.register_agent("agent-fresh".to_string(), agent.id).await;

    let stale = monitor
        .check_agents(&cache)
        .await
        .expect("check should succeed");
    assert!(stale.is_empty(), "recently seen agent should not be stale");
}

#[tokio::test]
async fn old_agent_is_detected_as_stale() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    let mut agent = Agent::new("agent-old", AgentRole::Deacon, CliType::Codex);
    agent.status = AgentStatus::Active;
    // Set last_seen to 5 minutes ago.
    agent.last_seen = Utc::now() - chrono::Duration::seconds(300);
    cache.upsert_agent(&agent).await.expect("upsert agent");

    // Threshold is 60 seconds, so 300 seconds ago is definitely stale.
    let monitor = HeartbeatMonitor::new(Duration::from_secs(60));
    monitor.register_agent("agent-old".to_string(), agent.id).await;

    let stale = monitor
        .check_agents(&cache)
        .await
        .expect("check should succeed");
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].agent_id, agent.id);
    assert_eq!(stale[0].name, "agent-old");
    assert!(stale[0].duration_since > Duration::from_secs(60));
}

#[tokio::test]
async fn unregistered_agent_in_cache_is_ignored() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    // Insert an old agent into cache but don't register it with the monitor.
    let mut agent = Agent::new("untracked-agent", AgentRole::Crew, CliType::Claude);
    agent.last_seen = Utc::now() - chrono::Duration::seconds(600);
    cache.upsert_agent(&agent).await.expect("upsert agent");

    let monitor = HeartbeatMonitor::new(Duration::from_secs(60));
    // Not registering the agent.

    let stale = monitor
        .check_agents(&cache)
        .await
        .expect("check should succeed");
    assert!(
        stale.is_empty(),
        "unregistered agents should not be checked"
    );
}

#[tokio::test]
async fn missing_agent_in_cache_reported_as_stale() {
    let cache = CacheDb::new_in_memory().await.expect("in-memory cache");

    let monitor = HeartbeatMonitor::new(Duration::from_secs(60));
    let fake_id = Uuid::new_v4();
    monitor.register_agent("ghost-agent".to_string(), fake_id).await;

    let stale = monitor
        .check_agents(&cache)
        .await
        .expect("check should succeed");
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].agent_id, fake_id);
    assert_eq!(stale[0].name, "ghost-agent");
}
