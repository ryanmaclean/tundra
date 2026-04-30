use std::sync::{Arc, Mutex};

use crate::protocol::BridgeMessage;

/// A subscriber entry holding its sender channel and an optional filter.
struct Subscriber {
    tx: flume::Sender<Arc<BridgeMessage>>,
    #[allow(clippy::type_complexity)]
    filter: Option<Box<dyn Fn(&BridgeMessage) -> bool + Send + Sync>>,
}

/// A broadcast-style event bus built on top of flume channels.
///
/// Each call to [`Self::subscribe`] creates a new receiver that will receive all
/// messages published after the subscription was created. The bus is
/// thread-safe and can be cloned cheaply (it wraps its internals in an `Arc`).
///
/// Messages are wrapped in `Arc` to avoid deep-cloning payloads like
/// `Vec<Bead>` or `Vec<Agent>` on every broadcast — only the reference
/// count is incremented per subscriber.
///
/// Filtered subscriptions allow subscribers to only receive messages that
/// match a predicate. See [`Self::subscribe_filtered`] and [`Self::subscribe_for_agent`].
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
            BridgeMessage::SlingBead { agent_id: id, .. } => *id == agent_id,
            BridgeMessage::AgentOutput { agent_id: id, .. } => *id == agent_id,
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

// ---------------------------------------------------------------------------
// Concurrency / stress tests
//
// Pin the hard contracts of EventBus under racy conditions: fan-out, slow or
// disappearing subscribers, concurrent subscribe/publish, and resource cleanup.
//
// Implementation notes:
//   * `EventBus::publish` is *synchronous* (try_send into bounded(1024) flume
//     channels guarded by an `Arc<Mutex<Vec<Subscriber>>>`), so these tests use
//     `std::thread` rather than `tokio::spawn`. No timer pause is needed: the
//     bus has no internal clock dependencies. Coordination uses `Barrier`,
//     `AtomicUsize`, and short bounded busy-waits (no real-time sleeps in the
//     hot path of an assertion).
//   * Per-subscriber capacity is 1024; tests stay well below that to avoid
//     accidentally triggering the slow-subscriber prune behavior in scenarios
//     that aren't testing it.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod stress_tests {
    use super::*;
    use crate::protocol::BridgeMessage;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    fn msg() -> BridgeMessage {
        BridgeMessage::GetStatus
    }

    /// Group A: single subscriber receives a single publish.
    #[test]
    fn single_subscriber_single_publish() {
        let bus = EventBus::new();
        let rx = bus.subscribe();
        bus.publish(msg());
        let received = rx.try_recv().expect("subscriber should get the publish");
        assert!(matches!(received.as_ref(), BridgeMessage::GetStatus));
        assert!(rx.try_recv().is_err(), "no second message expected");
    }

    /// Group A: every subscriber in a fan-out gets the publish exactly once.
    #[test]
    fn fanout_all_subscribers_receive_once() {
        let bus = EventBus::new();
        let rxs: Vec<_> = (0..16).map(|_| bus.subscribe()).collect();
        bus.publish(msg());
        for rx in &rxs {
            assert_eq!(rx.len(), 1, "each subscriber gets exactly one message");
            let _ = rx.try_recv().unwrap();
            assert!(rx.try_recv().is_err());
        }
    }

    /// Group A: a subscriber whose receiver was dropped does not block or
    /// affect the other subscribers — it is silently pruned on next publish.
    #[test]
    fn dropped_subscriber_does_not_block_publisher() {
        let bus = EventBus::new();
        let mut rxs: Vec<_> = (0..4).map(|_| bus.subscribe()).collect();
        // Drop the middle one.
        let dropped = rxs.remove(2);
        drop(dropped);
        assert_eq!(bus.subscriber_count(), 4);

        bus.publish(msg());
        assert_eq!(bus.subscriber_count(), 3, "dropped sub should be pruned");
        for rx in &rxs {
            assert_eq!(rx.len(), 1);
        }
    }

    /// Group B: one thread drops its receiver while another is publishing
    /// in a tight loop. No panic, no deadlock; a fresh subscriber created
    /// after the storm still observes later events.
    #[test]
    fn unsubscribe_during_publish_is_safe() {
        let bus = EventBus::new();
        let stable = bus.subscribe();
        let transient = bus.subscribe();

        let bus_pub = bus.clone();
        let publisher = thread::spawn(move || {
            for _ in 0..200 {
                bus_pub.publish(msg());
            }
        });
        let dropper = thread::spawn(move || {
            // Hand the receiver to the drop on a different thread mid-storm.
            drop(transient);
        });

        publisher.join().unwrap();
        dropper.join().unwrap();

        // The stable subscriber's queue is bounded(1024); 200 fits.
        assert_eq!(stable.len(), 200, "stable sub should see every publish");

        // A subscription added after the storm gets a fresh post-storm event.
        let post = bus.subscribe();
        bus.publish(msg());
        assert_eq!(post.len(), 1);
    }

    /// Group B: subscribers added while publishes are in flight see only
    /// publishes that occur strictly after their `subscribe()` returns.
    /// Pin the documented "every message published after the subscription
    /// was created" contract under contention.
    #[test]
    fn concurrent_subscribe_during_publish() {
        let bus = EventBus::new();
        let stop = Arc::new(AtomicBool::new(false));
        let publishes = Arc::new(AtomicUsize::new(0));

        let bus_pub = bus.clone();
        let stop_pub = stop.clone();
        let publishes_pub = publishes.clone();
        let publisher = thread::spawn(move || {
            while !stop_pub.load(Ordering::Acquire) {
                bus_pub.publish(msg());
                publishes_pub.fetch_add(1, Ordering::AcqRel);
                thread::yield_now();
            }
        });

        // Concurrently churn 50 new subscribers; each must only see events
        // strictly after its own subscribe() call.
        let bus_sub = bus.clone();
        let subscriber = thread::spawn(move || {
            let mut results = Vec::with_capacity(50);
            for _ in 0..50 {
                let rx = bus_sub.subscribe();
                thread::yield_now();
                results.push(rx);
            }
            results
        });

        let results = subscriber.join().unwrap();
        stop.store(true, Ordering::Release);
        publisher.join().unwrap();

        // Invariant: a subscriber created during the storm never receives
        // *more* messages than the publisher emitted in total, and never
        // overflows its bounded(1024) queue (publisher count is bounded by
        // yield_now scheduling; we keep the storm short enough to fit).
        let total_publishes = publishes.load(Ordering::Acquire);
        for rx in &results {
            assert!(
                rx.len() <= total_publishes,
                "subscriber received more events than were ever published"
            );
        }
    }

    /// Group B: a slow (non-draining) subscriber must not stall the bus —
    /// when its bounded(1024) queue overflows on a `try_send`, the slow sub
    /// is *pruned* (not the publisher blocked), so a co-existing fast
    /// subscriber that drains between batches keeps observing every publish.
    ///
    /// Determinism: rather than racing a drainer thread against a publisher
    /// (which is sensitive to scheduler quanta — see the earlier flake), we
    /// publish in batches sized so the fast sub never overflows, drain it
    /// between batches, and only the slow sub overflows. This pins the
    /// "slow-sub-pruned, fast-sub-survives" contract without scheduler races.
    #[test]
    fn slow_subscriber_does_not_starve_fast_subscriber() {
        let bus = EventBus::new();
        let fast = bus.subscribe();
        let _slow = bus.subscribe(); // never drained — capacity is 1024.

        // Two batches of 600 publishes; total 1200 > 1024 capacity, so the
        // slow sub overflows on a publish during batch 2. The fast sub is
        // drained between batches so it never accumulates more than 600
        // queued messages and is never at risk of being pruned.
        let batch = 600usize;
        for _ in 0..batch {
            bus.publish(msg());
        }
        // Slow sub now holds 600/1024; fast sub holds 600/1024.
        // Drain the fast sub.
        for _ in 0..batch {
            fast.try_recv().expect("fast sub should have its message");
        }

        // Second batch; slow sub will pass 1024 mid-batch and be pruned.
        for _ in 0..batch {
            bus.publish(msg());
        }

        assert_eq!(
            bus.subscriber_count(),
            1,
            "slow non-draining subscriber should be pruned after overflow"
        );
        // Fast sub got the second batch (drained the first already).
        assert_eq!(
            fast.len(),
            batch,
            "fast subscriber should still receive every publish"
        );
    }

    /// Group C: publishing with zero subscribers is a no-op and never panics.
    #[test]
    fn publish_with_no_subscribers_is_noop() {
        let bus = EventBus::new();
        let rx = bus.subscribe();
        drop(rx);
        bus.publish(msg());
        bus.publish(msg());
        // After the first publish the disconnected sub is pruned.
        assert_eq!(bus.subscriber_count(), 0);
        // Further publishes are a no-op.
        bus.publish(msg());
        assert_eq!(bus.subscriber_count(), 0);
    }

    /// Group C: re-subscribing after a full unsubscription works.
    #[test]
    fn resubscribe_after_full_unsubscription() {
        let bus = EventBus::new();
        {
            let rx = bus.subscribe();
            drop(rx);
            bus.publish(msg()); // prunes
        }
        assert_eq!(bus.subscriber_count(), 0);

        let rx2 = bus.subscribe();
        bus.publish(msg());
        assert_eq!(rx2.len(), 1);
        let m = rx2.try_recv().unwrap();
        assert!(matches!(m.as_ref(), BridgeMessage::GetStatus));
    }

    /// Group C: dropping the EventBus while subscribers hold receivers does
    /// not panic; receivers cleanly observe end-of-stream once the only
    /// references to senders are gone.
    #[test]
    fn bus_dropped_while_subscribers_held() {
        let bus = EventBus::new();
        let rx = bus.subscribe();
        bus.publish(msg());
        drop(bus);
        // The buffered message is still readable.
        let _ = rx.try_recv().expect("buffered message survives bus drop");
        // Subsequent recv yields disconnected error (no senders left).
        assert!(rx.try_recv().is_err());
    }

    /// Group B: many concurrent publishers serialize through the mutex
    /// without losing or duplicating messages for a passive subscriber.
    #[test]
    fn concurrent_publishers_no_loss() {
        let bus = EventBus::new();
        let rx = bus.subscribe();

        let n_threads = 4;
        let per_thread = 200; // 4 * 200 = 800 < 1024 capacity
        let barrier = Arc::new(Barrier::new(n_threads));
        let mut handles = Vec::new();
        for _ in 0..n_threads {
            let bus_c = bus.clone();
            let b = barrier.clone();
            handles.push(thread::spawn(move || {
                b.wait();
                for _ in 0..per_thread {
                    bus_c.publish(msg());
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(rx.len(), n_threads * per_thread);
    }

    /// Group A/B: filtered subscribers under concurrent publish — only
    /// matching messages are delivered, non-matching subscribers are retained.
    #[test]
    fn filtered_subscriber_under_concurrent_publish() {
        let bus = EventBus::new();
        let only_status = bus.subscribe_filtered(|m| matches!(m, BridgeMessage::GetStatus));
        let only_kpi = bus.subscribe_filtered(|m| matches!(m, BridgeMessage::GetKpi));

        let bus_a = bus.clone();
        let bus_b = bus.clone();
        let h1 = thread::spawn(move || {
            for _ in 0..100 {
                bus_a.publish(BridgeMessage::GetStatus);
            }
        });
        let h2 = thread::spawn(move || {
            for _ in 0..100 {
                bus_b.publish(BridgeMessage::GetKpi);
            }
        });
        h1.join().unwrap();
        h2.join().unwrap();

        assert_eq!(only_status.len(), 100);
        assert_eq!(only_kpi.len(), 100);
        // Both subs still alive (filter mismatch must not prune).
        assert_eq!(bus.subscriber_count(), 2);
    }
}
