use std::sync::{Arc, Mutex};

use crate::protocol::BridgeMessage;

/// A subscriber entry holding its sender channel and an optional filter.
struct Subscriber {
    tx: flume::Sender<Arc<BridgeMessage>>,
    filter: Option<Box<dyn Fn(&BridgeMessage) -> bool + Send + Sync>>,
}

/// A broadcast-style event bus built on top of flume channels.
///
/// Each call to [`subscribe`] creates a new receiver that will receive all
/// messages published after the subscription was created. The bus is
/// thread-safe and can be cloned cheaply (it wraps its internals in an `Arc`).
///
/// Messages are wrapped in `Arc` to avoid deep-cloning payloads like
/// `Vec<Bead>` or `Vec<Agent>` on every broadcast — only the reference
/// count is incremented per subscriber.
///
/// Filtered subscriptions allow subscribers to only receive messages that
/// match a predicate. See [`subscribe_filtered`] and [`subscribe_for_agent`].
#[derive(Clone)]
pub struct EventBus {
    inner: Arc<Mutex<Vec<Subscriber>>>,
}

impl EventBus {
    /// Create a new, empty event bus with no subscribers.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a new subscriber and return its receiving end.
    ///
    /// The returned `Receiver` will receive every message published to the bus
    /// from this point forward. Messages arrive as `Arc<BridgeMessage>` —
    /// dereference or use `.as_ref()` to access the inner message.
    pub fn subscribe(&self) -> flume::Receiver<Arc<BridgeMessage>> {
        let (tx, rx) = flume::bounded(1024);
        let mut subs = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("EventBus lock was poisoned, recovering");
            e.into_inner()
        });
        subs.push(Subscriber { tx, filter: None });
        rx
    }

    /// Register a filtered subscriber. Only messages for which `filter`
    /// returns `true` will be delivered.
    pub fn subscribe_filtered<F>(&self, filter: F) -> flume::Receiver<Arc<BridgeMessage>>
    where
        F: Fn(&BridgeMessage) -> bool + Send + Sync + 'static,
    {
        let (tx, rx) = flume::bounded(1024);
        let mut subs = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("EventBus lock was poisoned, recovering");
            e.into_inner()
        });
        subs.push(Subscriber {
            tx,
            filter: Some(Box::new(filter)),
        });
        rx
    }

    /// Subscribe to messages targeting a specific agent.
    ///
    /// Filters on any `agent_id` field present in BridgeMessage variants:
    /// - `SlingBead { agent_id, .. }`
    /// - `AgentOutput { agent_id, .. }`
    /// - `Event(EventPayload { agent_id: Some(..), .. })`
    pub fn subscribe_for_agent(&self, agent_id: uuid::Uuid) -> flume::Receiver<Arc<BridgeMessage>> {
        self.subscribe_filtered(move |msg| match msg {
            BridgeMessage::SlingBead {
                agent_id: id,
                ..
            } => *id == agent_id,
            BridgeMessage::AgentOutput {
                agent_id: id,
                ..
            } => *id == agent_id,
            BridgeMessage::Event(payload) => payload.agent_id == Some(agent_id),
            _ => false,
        })
    }

    /// Publish a message to all current subscribers.
    ///
    /// The message is wrapped in `Arc` once and only reference counts are
    /// cloned per subscriber — no deep copies of payload data.
    /// Disconnected subscribers (whose receivers have been dropped) are
    /// automatically pruned. Filtered subscribers that do not match the
    /// message are skipped (but retained).
    pub fn publish(&self, msg: BridgeMessage) {
        let msg = Arc::new(msg);
        let mut subs = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("EventBus lock was poisoned, recovering");
            e.into_inner()
        });
        subs.retain(|sub| {
            // If there is a filter and the message doesn't match, skip but keep.
            if let Some(ref f) = sub.filter {
                if !f(&msg) {
                    return true;
                }
            }
            match sub.tx.try_send(Arc::clone(&msg)) {
                Ok(()) => true,
                Err(flume::TrySendError::Full(_)) => {
                    tracing::warn!("dropping slow event subscriber (channel full)");
                    false
                }
                Err(flume::TrySendError::Disconnected(_)) => false,
            }
        });
    }

    /// Return the number of currently active subscribers.
    pub fn subscriber_count(&self) -> usize {
        let subs = self.inner.lock().unwrap_or_else(|e| {
            tracing::warn!("EventBus lock was poisoned, recovering");
            e.into_inner()
        });
        subs.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{EventPayload, StatusPayload};
    use uuid::Uuid;

    fn status_msg() -> BridgeMessage {
        BridgeMessage::StatusUpdate(StatusPayload {
            version: "1.0".into(),
            uptime_seconds: 42,
            agents_active: 1,
            beads_active: 0,
        })
    }

    fn agent_output_msg(agent_id: Uuid) -> BridgeMessage {
        BridgeMessage::AgentOutput {
            agent_id,
            output: "hello".into(),
        }
    }

    fn sling_msg(agent_id: Uuid) -> BridgeMessage {
        BridgeMessage::SlingBead {
            bead_id: Uuid::new_v4(),
            agent_id,
        }
    }

    fn event_msg(agent_id: Option<Uuid>) -> BridgeMessage {
        BridgeMessage::Event(EventPayload {
            event_type: "test".into(),
            agent_id,
            bead_id: None,
            message: "evt".into(),
            timestamp: chrono::Utc::now(),
        })
    }

    #[test]
    fn unfiltered_subscriber_gets_all_messages() {
        let bus = EventBus::new();
        let rx = bus.subscribe();

        bus.publish(status_msg());
        bus.publish(BridgeMessage::GetStatus);
        bus.publish(agent_output_msg(Uuid::new_v4()));

        assert_eq!(rx.len(), 3);
    }

    #[test]
    fn filtered_subscriber_only_gets_matching() {
        let bus = EventBus::new();
        let rx = bus.subscribe_filtered(|msg| matches!(msg, BridgeMessage::GetStatus));

        bus.publish(status_msg());
        bus.publish(BridgeMessage::GetStatus);
        bus.publish(BridgeMessage::ListAgents);
        bus.publish(BridgeMessage::GetStatus);

        // Only the two GetStatus messages should arrive.
        assert_eq!(rx.len(), 2);
    }

    #[test]
    fn mixed_filtered_and_unfiltered() {
        let bus = EventBus::new();

        let rx_all = bus.subscribe();
        let rx_filtered = bus.subscribe_filtered(|msg| matches!(msg, BridgeMessage::GetKpi));

        bus.publish(BridgeMessage::GetStatus);
        bus.publish(BridgeMessage::GetKpi);
        bus.publish(BridgeMessage::ListAgents);

        assert_eq!(rx_all.len(), 3);
        assert_eq!(rx_filtered.len(), 1);
    }

    #[test]
    fn agent_specific_subscription() {
        let target = Uuid::new_v4();
        let other = Uuid::new_v4();

        let bus = EventBus::new();
        let rx = bus.subscribe_for_agent(target);

        bus.publish(agent_output_msg(target));
        bus.publish(agent_output_msg(other));
        bus.publish(sling_msg(target));
        bus.publish(sling_msg(other));
        bus.publish(event_msg(Some(target)));
        bus.publish(event_msg(Some(other)));
        bus.publish(event_msg(None));
        bus.publish(status_msg()); // no agent_id at all

        // Should receive: agent_output(target), sling(target), event(Some(target)) = 3
        assert_eq!(rx.len(), 3);
    }

    #[test]
    fn disconnected_filtered_subscribers_are_pruned() {
        let bus = EventBus::new();

        let rx_keep = bus.subscribe();
        let rx_drop = bus.subscribe_filtered(|msg| matches!(msg, BridgeMessage::GetStatus));
        assert_eq!(bus.subscriber_count(), 2);

        // Drop the filtered receiver to disconnect it.
        drop(rx_drop);

        // Publish a matching message — the disconnected subscriber should be pruned.
        bus.publish(BridgeMessage::GetStatus);
        assert_eq!(bus.subscriber_count(), 1);

        // The surviving subscriber still works.
        assert_eq!(rx_keep.len(), 1);
    }

    #[test]
    fn disconnected_unfiltered_subscribers_are_pruned() {
        let bus = EventBus::new();

        let rx = bus.subscribe();
        let _rx2 = bus.subscribe_filtered(|_| true);
        assert_eq!(bus.subscriber_count(), 2);

        drop(rx);
        bus.publish(BridgeMessage::GetStatus);
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[test]
    fn existing_subscribe_still_works() {
        // Ensures the original API contract is preserved.
        let bus = EventBus::new();
        let rx = bus.subscribe();

        bus.publish(BridgeMessage::GetStatus);
        let msg = rx.try_recv().unwrap();
        assert!(matches!(msg.as_ref(), BridgeMessage::GetStatus));
    }
}
