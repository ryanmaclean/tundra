//! Live agent registry sync.
//!
//! Executors (`at-agents`) do not hold a handle to `ApiState`; they report
//! agent lifecycle on the [`EventBus`]:
//!
//! | Message | Effect on the registry |
//! |---------|------------------------|
//! | `AgentCreated(agent)` | insert (or newer-wins update if already present) |
//! | `AgentUpdated(agent)` | replace when `agent.last_seen` is not older than the stored one; insert if absent |
//! | `AgentDeleted(id)` | remove |
//! | `Event { event_type: "agent_heartbeat", agent_id, timestamp }` | advance `last_seen` to `timestamp` unless the agent is `Stopped` |
//!
//! [`spawn_registry_sync`] applies these to the shared agent map
//! (`ApiState.agents`), which is what the daemon's stuck-agent patrol reads.
//! Heartbeats never change `status`, so a late heartbeat cannot resurrect an
//! agent the patrol already force-killed.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use at_core::types::{Agent, AgentStatus};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::event_bus::EventBus;
use crate::protocol::{BridgeMessage, EVENT_AGENT_HEARTBEAT};

/// What [`apply`] did with a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Inserted,
    Updated,
    Heartbeat,
    Removed,
    /// Not a registry message, stale, or targeted an unknown/stopped agent.
    Ignored,
}

/// Whether `msg` can change the agent registry (subscription filter).
pub fn is_registry_message(msg: &BridgeMessage) -> bool {
    match msg {
        BridgeMessage::AgentCreated(_)
        | BridgeMessage::AgentUpdated(_)
        | BridgeMessage::AgentDeleted(_) => true,
        BridgeMessage::Event(p) => p.event_type == EVENT_AGENT_HEARTBEAT && p.agent_id.is_some(),
        _ => false,
    }
}

/// Apply one bus message to the registry map.
pub fn apply(agents: &mut HashMap<Uuid, Agent>, msg: &BridgeMessage) -> Applied {
    match msg {
        BridgeMessage::AgentCreated(a) | BridgeMessage::AgentUpdated(a) => {
            match agents.get_mut(&a.id) {
                None => {
                    agents.insert(a.id, a.clone());
                    Applied::Inserted
                }
                Some(cur) if a.last_seen >= cur.last_seen => {
                    *cur = a.clone();
                    Applied::Updated
                }
                Some(_) => Applied::Ignored,
            }
        }
        BridgeMessage::AgentDeleted(id) => {
            if agents.remove(id).is_some() {
                Applied::Removed
            } else {
                Applied::Ignored
            }
        }
        BridgeMessage::Event(p) if p.event_type == EVENT_AGENT_HEARTBEAT => {
            let Some(agent) = p.agent_id.and_then(|id| agents.get_mut(&id)) else {
                return Applied::Ignored;
            };
            if agent.status == AgentStatus::Stopped {
                return Applied::Ignored;
            }
            if p.timestamp > agent.last_seen {
                agent.last_seen = p.timestamp;
            }
            Applied::Heartbeat
        }
        _ => Applied::Ignored,
    }
}

/// Subscribe to agent lifecycle messages on `bus` and keep `agents` (and the
/// cached `agent_count`) in sync for as long as the registry is shared.
///
/// The first subscription is taken before this returns, so messages published
/// right after the call are not missed. If the bus drops the subscriber (its
/// channel filled up) the task resubscribes; heartbeats resume on the next
/// executor beat. Must be called from a Tokio runtime.
pub fn spawn_registry_sync(
    bus: EventBus,
    agents: Arc<RwLock<HashMap<Uuid, Agent>>>,
    agent_count: Arc<AtomicUsize>,
) -> tokio::task::JoinHandle<()> {
    let mut rx = bus.subscribe_filtered(is_registry_message);
    tokio::spawn(async move {
        loop {
            while let Ok(msg) = rx.recv_async().await {
                let mut map = agents.write().await;
                if apply(&mut map, &msg) != Applied::Ignored {
                    agent_count.store(map.len(), Ordering::Relaxed);
                }
            }
            // This task keeps the bus alive, so a closed channel means the
            // bus dropped us as a slow subscriber. Stop once nothing else
            // holds the registry; otherwise resubscribe.
            if Arc::strong_count(&agents) == 1 {
                return;
            }
            tracing::warn!("agent registry subscriber dropped by event bus, resubscribing");
            rx = bus.subscribe_filtered(is_registry_message);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::EventPayload;
    use at_core::types::{AgentRole, CliType};
    use chrono::{Duration, Utc};

    fn live() -> Agent {
        let mut a = Agent::new("exec", AgentRole::Crew, CliType::Claude);
        a.status = AgentStatus::Active;
        a.session_id = Some("proc-1".into());
        a
    }

    fn beat(id: Uuid, at: chrono::DateTime<Utc>) -> BridgeMessage {
        BridgeMessage::Event(EventPayload {
            event_type: EVENT_AGENT_HEARTBEAT.into(),
            agent_id: Some(id),
            bead_id: None,
            message: String::new(),
            timestamp: at,
        })
    }

    #[test]
    fn created_inserts_and_heartbeat_advances_last_seen() {
        let mut map = HashMap::new();
        let a = live();
        assert_eq!(
            apply(&mut map, &BridgeMessage::AgentCreated(a.clone())),
            Applied::Inserted
        );
        let later = a.last_seen + Duration::seconds(7);
        assert_eq!(apply(&mut map, &beat(a.id, later)), Applied::Heartbeat);
        assert_eq!(map[&a.id].last_seen, later);
        assert_eq!(map[&a.id].status, AgentStatus::Active);
    }

    #[test]
    fn heartbeat_never_moves_last_seen_backwards() {
        let mut map = HashMap::new();
        let a = live();
        apply(&mut map, &BridgeMessage::AgentCreated(a.clone()));
        let earlier = a.last_seen - Duration::seconds(5);
        assert_eq!(apply(&mut map, &beat(a.id, earlier)), Applied::Heartbeat);
        assert_eq!(map[&a.id].last_seen, a.last_seen);
    }

    #[test]
    fn heartbeat_does_not_resurrect_stopped_agent() {
        let mut map = HashMap::new();
        let mut a = live();
        a.status = AgentStatus::Stopped;
        apply(&mut map, &BridgeMessage::AgentCreated(a.clone()));
        let later = a.last_seen + Duration::seconds(60);
        assert_eq!(apply(&mut map, &beat(a.id, later)), Applied::Ignored);
        assert_eq!(map[&a.id].last_seen, a.last_seen);
        assert_eq!(map[&a.id].status, AgentStatus::Stopped);
    }

    #[test]
    fn heartbeat_for_unknown_agent_is_ignored() {
        let mut map = HashMap::new();
        assert_eq!(
            apply(&mut map, &beat(Uuid::new_v4(), Utc::now())),
            Applied::Ignored
        );
        assert!(map.is_empty());
    }

    #[test]
    fn stale_update_is_ignored_newer_update_wins() {
        let mut map = HashMap::new();
        let a = live();
        apply(&mut map, &BridgeMessage::AgentCreated(a.clone()));

        let mut stale = a.clone();
        stale.status = AgentStatus::Idle;
        stale.last_seen = a.last_seen - Duration::seconds(1);
        assert_eq!(
            apply(&mut map, &BridgeMessage::AgentUpdated(stale)),
            Applied::Ignored
        );
        assert_eq!(map[&a.id].status, AgentStatus::Active);

        let mut exited = a.clone();
        exited.status = AgentStatus::Stopped;
        exited.last_seen = a.last_seen + Duration::seconds(1);
        assert_eq!(
            apply(&mut map, &BridgeMessage::AgentUpdated(exited)),
            Applied::Updated
        );
        assert_eq!(map[&a.id].status, AgentStatus::Stopped);
    }

    #[test]
    fn deleted_removes() {
        let mut map = HashMap::new();
        let a = live();
        apply(&mut map, &BridgeMessage::AgentCreated(a.clone()));
        assert_eq!(
            apply(&mut map, &BridgeMessage::AgentDeleted(a.id)),
            Applied::Removed
        );
        assert!(map.is_empty());
        assert_eq!(
            apply(&mut map, &BridgeMessage::AgentDeleted(a.id)),
            Applied::Ignored
        );
    }

    #[test]
    fn filter_accepts_only_registry_messages() {
        assert!(is_registry_message(&BridgeMessage::AgentDeleted(
            Uuid::new_v4()
        )));
        assert!(is_registry_message(&beat(Uuid::new_v4(), Utc::now())));
        let mut no_agent = beat(Uuid::new_v4(), Utc::now());
        if let BridgeMessage::Event(p) = &mut no_agent {
            p.agent_id = None;
        }
        assert!(!is_registry_message(&no_agent));
        assert!(!is_registry_message(&BridgeMessage::GetKpi));
    }

    #[tokio::test]
    async fn sync_task_applies_bus_messages() {
        let bus = EventBus::new();
        let agents = Arc::new(RwLock::new(HashMap::new()));
        let count = Arc::new(AtomicUsize::new(0));
        let _h = spawn_registry_sync(bus.clone(), agents.clone(), count.clone());

        let a = live();
        bus.publish(BridgeMessage::AgentCreated(a.clone()));
        let later = a.last_seen + Duration::seconds(3);
        bus.publish(beat(a.id, later));

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if agents.read().await.get(&a.id).map(|x| x.last_seen) == Some(later) {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "heartbeat not applied"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
}
