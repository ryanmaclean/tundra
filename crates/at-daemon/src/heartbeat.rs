use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::Mutex;

use anyhow::Result;
use at_core::cache::{CacheDb, CacheError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An agent that has not sent a heartbeat within the staleness threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleAgent {
    /// The agent's unique identifier.
    pub agent_id: Uuid,
    /// The agent's name.
    pub name: String,
    /// When the agent was last seen.
    pub last_seen: DateTime<Utc>,
    /// How long the agent has been stale.
    #[serde(with = "duration_serde")]
    pub duration_since: Duration,
}

mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    #[derive(Serialize, Deserialize)]
    struct DurationRepr {
        secs: u64,
        nanos: u32,
    }

    pub fn serialize<S: Serializer>(dur: &Duration, s: S) -> Result<S::Ok, S::Error> {
        DurationRepr {
            secs: dur.as_secs(),
            nanos: dur.subsec_nanos(),
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let repr = DurationRepr::deserialize(d)?;
        Ok(Duration::new(repr.secs, repr.nanos))
    }
}

/// Tracks agent heartbeats and detects stale agents.
///
/// Because `CacheDb` does not expose a `list_agents` method, the monitor
/// maintains an internal registry of agent names. Agents must be registered
/// via [`HeartbeatMonitor::register_agent`] before they can be checked.
/// Alternatively, [`HeartbeatMonitor::check_agents`] queries the cache for
/// each registered agent by name.
pub struct HeartbeatMonitor {
    /// Duration after which an agent is considered stale.
    staleness_threshold: Duration,
    /// Internal registry: agent name -> agent_id.
    tracked_agents: Mutex<HashMap<String, Uuid>>,
}

impl HeartbeatMonitor {
    /// Create a new heartbeat monitor with the given staleness threshold.
    pub fn new(staleness_threshold: Duration) -> Self {
        Self {
            staleness_threshold,
            tracked_agents: Mutex::new(HashMap::new()),
        }
    }

    /// Register an agent for heartbeat tracking.
    pub async fn register_agent(&self, name: String, id: Uuid) {
        let mut agents = self.tracked_agents.lock().await;
        agents.insert(name, id);
    }

    /// Remove an agent from tracking.
    pub async fn unregister_agent(&self, name: &str) {
        let mut agents = self.tracked_agents.lock().await;
        agents.remove(name);
    }

    /// Return the current staleness threshold.
    pub fn staleness_threshold(&self) -> Duration {
        self.staleness_threshold
    }

    /// Check all registered agents for staleness by querying the cache.
    ///
    /// Returns a list of agents whose `last_seen` timestamp exceeds the
    /// staleness threshold relative to now.
    pub async fn check_agents(&self, cache: &CacheDb) -> Result<Vec<StaleAgent>> {
        let now = Utc::now();
        let tracked: Vec<(String, Uuid)> = {
            let agents = self.tracked_agents.lock().await;
            agents.iter().map(|(k, v)| (k.clone(), *v)).collect()
        };

        let mut stale = Vec::new();
        for (name, id) in tracked {
            match cache.get_agent_by_name(&name).await {
                Ok(Some(agent)) => {
                    let elapsed = now
                        .signed_duration_since(agent.last_seen)
                        .to_std()
                        .unwrap_or(Duration::ZERO);
                    if elapsed > self.staleness_threshold {
                        stale.push(StaleAgent {
                            agent_id: id,
                            name: agent.name,
                            last_seen: agent.last_seen,
                            duration_since: elapsed,
                        });
                    }
                }
                Ok(None) => {
                    // Agent not in cache; treat as stale with zero last_seen info.
                    stale.push(StaleAgent {
                        agent_id: id,
                        name,
                        last_seen: DateTime::<Utc>::MIN_UTC,
                        duration_since: self.staleness_threshold + Duration::from_secs(1),
                    });
                }
                Err(CacheError::InvalidRow {
                    ref context,
                    ref source,
                }) => {
                    // Persistent schema corruption — will not resolve on retry.
                    // Log at error level so operators know a manual fix is needed.
                    tracing::error!(
                        agent_name = %name,
                        context = %context,
                        source = %source,
                        "agent row contains corrupt data (schema drift?); skipping this agent"
                    );
                }
                Err(CacheError::Db(ref db_err)) => {
                    // Transient database error — may resolve on the next heartbeat tick.
                    tracing::warn!(
                        agent_name = %name,
                        error = %db_err,
                        "transient DB error querying agent; will retry next tick"
                    );
                }
            }
        }

        Ok(stale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::types::{Agent, AgentRole, CliType};

    fn make_agent(name: &str, last_seen: DateTime<Utc>) -> Agent {
        let mut agent = Agent::new(name, AgentRole::Crew, CliType::Claude);
        agent.last_seen = last_seen;
        agent
    }

    #[test]
    fn staleness_threshold_round_trips() {
        let monitor = HeartbeatMonitor::new(Duration::from_secs(42));
        assert_eq!(monitor.staleness_threshold(), Duration::from_secs(42));
    }

    #[test]
    fn zero_threshold_is_preserved() {
        let monitor = HeartbeatMonitor::new(Duration::ZERO);
        assert_eq!(monitor.staleness_threshold(), Duration::ZERO);
    }

    #[test]
    fn stale_agent_serde_round_trip() {
        let stale = StaleAgent {
            agent_id: Uuid::new_v4(),
            name: "alpha".to_string(),
            last_seen: DateTime::<Utc>::MIN_UTC,
            duration_since: Duration::new(7, 250),
        };

        let json = serde_json::to_string(&stale).expect("serialize");
        let back: StaleAgent = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.agent_id, stale.agent_id);
        assert_eq!(back.name, stale.name);
        assert_eq!(back.duration_since, stale.duration_since);
        assert_eq!(back.last_seen, stale.last_seen);
    }

    #[tokio::test]
    async fn register_and_unregister_round_trip() {
        let monitor = HeartbeatMonitor::new(Duration::from_secs(60));
        let id = Uuid::new_v4();
        monitor.register_agent("ranger".to_string(), id).await;

        // Internal map exposed through check_agents: with empty cache the
        // tracked agent is reported stale (Ok(None) branch).
        let cache = CacheDb::new_in_memory().await.expect("cache");
        let stale = monitor.check_agents(&cache).await.expect("check");
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].name, "ranger");
        assert_eq!(stale[0].agent_id, id);

        monitor.unregister_agent("ranger").await;
        let stale = monitor.check_agents(&cache).await.expect("check");
        assert!(stale.is_empty(), "agent should not be tracked anymore");
    }

    #[tokio::test]
    async fn unregistering_unknown_name_is_noop() {
        let monitor = HeartbeatMonitor::new(Duration::from_secs(10));
        // Removing a name that was never registered must not panic.
        monitor.unregister_agent("ghost").await;

        let cache = CacheDb::new_in_memory().await.expect("cache");
        let stale = monitor.check_agents(&cache).await.expect("check");
        assert!(stale.is_empty());
    }

    #[tokio::test]
    async fn empty_monitor_reports_no_stale_agents() {
        let monitor = HeartbeatMonitor::new(Duration::from_secs(5));
        let cache = CacheDb::new_in_memory().await.expect("cache");
        let stale = monitor.check_agents(&cache).await.expect("check");
        assert!(stale.is_empty());
    }

    #[tokio::test]
    async fn agent_missing_from_cache_is_marked_stale() {
        let monitor = HeartbeatMonitor::new(Duration::from_secs(30));
        let id = Uuid::new_v4();
        monitor.register_agent("phantom".to_string(), id).await;

        let cache = CacheDb::new_in_memory().await.expect("cache");
        let stale = monitor.check_agents(&cache).await.expect("check");

        assert_eq!(stale.len(), 1);
        let entry = &stale[0];
        assert_eq!(entry.agent_id, id);
        assert_eq!(entry.name, "phantom");
        assert_eq!(entry.last_seen, DateTime::<Utc>::MIN_UTC);
        // Sentinel duration must exceed the threshold so callers treat as stale.
        assert!(entry.duration_since > monitor.staleness_threshold());
    }

    #[tokio::test]
    async fn fresh_heartbeat_is_not_stale() {
        let threshold = Duration::from_secs(60);
        let monitor = HeartbeatMonitor::new(threshold);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Insert an agent whose last_seen is "now" — well within threshold.
        let agent = make_agent("fresh", Utc::now());
        cache.upsert_agent(&agent).await.expect("upsert");

        monitor.register_agent("fresh".to_string(), agent.id).await;
        let stale = monitor.check_agents(&cache).await.expect("check");
        assert!(
            stale.is_empty(),
            "fresh agent should not be flagged: {stale:?}"
        );
    }

    #[tokio::test]
    async fn agent_just_beyond_threshold_is_stale() {
        let threshold = Duration::from_secs(60);
        let monitor = HeartbeatMonitor::new(threshold);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // last_seen is 120s in the past — clearly beyond a 60s threshold.
        let last_seen = Utc::now() - chrono::Duration::seconds(120);
        let agent = make_agent("slowpoke", last_seen);
        cache.upsert_agent(&agent).await.expect("upsert");

        monitor
            .register_agent("slowpoke".to_string(), agent.id)
            .await;
        let stale = monitor.check_agents(&cache).await.expect("check");

        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].name, "slowpoke");
        assert!(stale[0].duration_since > threshold);
    }

    #[tokio::test]
    async fn agent_inside_threshold_is_not_stale_boundary() {
        // One second in the past, threshold an hour away.
        let threshold = Duration::from_secs(3600);
        let monitor = HeartbeatMonitor::new(threshold);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        let last_seen = Utc::now() - chrono::Duration::seconds(1);
        let agent = make_agent("just-pinged", last_seen);
        cache.upsert_agent(&agent).await.expect("upsert");

        monitor
            .register_agent("just-pinged".to_string(), agent.id)
            .await;
        let stale = monitor.check_agents(&cache).await.expect("check");
        assert!(stale.is_empty());
    }

    #[tokio::test]
    async fn check_agents_handles_mixed_population() {
        let threshold = Duration::from_secs(30);
        let monitor = HeartbeatMonitor::new(threshold);
        let cache = CacheDb::new_in_memory().await.expect("cache");

        let fresh = make_agent("alpha", Utc::now());
        let stale = make_agent("bravo", Utc::now() - chrono::Duration::seconds(600));
        cache.upsert_agent(&fresh).await.expect("upsert alpha");
        cache.upsert_agent(&stale).await.expect("upsert bravo");

        monitor.register_agent("alpha".to_string(), fresh.id).await;
        monitor.register_agent("bravo".to_string(), stale.id).await;
        // charlie is registered but never inserted — should land in the
        // "missing from cache" branch.
        let charlie_id = Uuid::new_v4();
        monitor
            .register_agent("charlie".to_string(), charlie_id)
            .await;

        let result = monitor.check_agents(&cache).await.expect("check");

        let names: std::collections::HashSet<_> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(!names.contains("alpha"), "alpha should be fresh");
        assert!(names.contains("bravo"), "bravo should be stale");
        assert!(names.contains("charlie"), "charlie should be stale");
        assert_eq!(result.len(), 2);
    }

    // ----- CacheError::InvalidRow regression tests -----

    /// A registered agent whose cache row has a corrupt enum (unrecognised
    /// `role` value) must be silently skipped — `check_agents` must not panic
    /// and must still return `Ok(...)` for the other agents.
    #[tokio::test]
    async fn check_agents_skips_corrupt_invalid_row_agent() {
        let monitor = HeartbeatMonitor::new(Duration::from_secs(30));
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // Insert an agent with an unknown role so row_to_agent returns InvalidRow.
        cache
            .insert_raw_agent_for_test("corrupt-agent", "NOT_A_VALID_ROLE", "claude")
            .await
            .expect("raw insert");

        // Also insert a healthy agent — it must still show up in results.
        let healthy = make_agent("healthy-agent", Utc::now() - chrono::Duration::seconds(120));
        cache.upsert_agent(&healthy).await.expect("upsert healthy");

        monitor
            .register_agent("corrupt-agent".to_string(), Uuid::new_v4())
            .await;
        monitor
            .register_agent("healthy-agent".to_string(), healthy.id)
            .await;

        // Must not panic; corrupt agent is skipped, healthy agent is reported.
        let result = monitor
            .check_agents(&cache)
            .await
            .expect("check_agents must not fail on InvalidRow");

        let names: std::collections::HashSet<_> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(
            !names.contains("corrupt-agent"),
            "corrupt agent should be skipped, not included in stale list"
        );
        assert!(
            names.contains("healthy-agent"),
            "healthy (but stale) agent must still be reported"
        );
    }

    /// A `CacheError::Db` (simulated via an agent that is simply missing from
    /// the cache) must also not abort `check_agents`.  The missing-from-cache
    /// path produces `Ok(None)`, which is already tested; here we confirm the
    /// loop continues past an agent whose row would produce a transient error.
    /// (The cleanest way to verify the loop-continues contract without a mock
    /// DB is to exercise the happy path alongside the skip path.)
    #[tokio::test]
    async fn check_agents_continues_past_corrupt_row_and_returns_ok() {
        let monitor = HeartbeatMonitor::new(Duration::from_secs(1));
        let cache = CacheDb::new_in_memory().await.expect("cache");

        // One corrupt agent (InvalidRow) and one genuinely stale agent.
        cache
            .insert_raw_agent_for_test("bad-cli-type", "crew", "COMPLETELY_BOGUS_CLI_TYPE")
            .await
            .expect("raw insert");

        let stale = make_agent("real-stale", Utc::now() - chrono::Duration::seconds(999));
        cache.upsert_agent(&stale).await.expect("upsert stale");

        monitor
            .register_agent("bad-cli-type".to_string(), Uuid::new_v4())
            .await;
        monitor
            .register_agent("real-stale".to_string(), stale.id)
            .await;

        let result = monitor
            .check_agents(&cache)
            .await
            .expect("must return Ok even when an InvalidRow is encountered");

        // The stale, non-corrupt agent must be reported.
        assert!(
            result.iter().any(|s| s.name == "real-stale"),
            "stale healthy agent must appear in result: {result:?}"
        );
    }
}
