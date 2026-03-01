use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::Mutex;

use anyhow::Result;
use at_core::cache::CacheDb;
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
                Err(e) => {
                    tracing::warn!(agent_name = %name, error = %e, "failed to query agent");
                }
            }
        }

        Ok(stale)
    }
}
