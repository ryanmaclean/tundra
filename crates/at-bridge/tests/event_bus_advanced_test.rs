use at_bridge::event_bus::EventBus;
use at_bridge::protocol::{BridgeMessage, EventPayload, KpiPayload, StatusPayload};
use chrono::Utc;
use std::sync::{Arc, Barrier};
use std::thread;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Event Publishing
// ---------------------------------------------------------------------------

#[test]
fn test_publish_event_to_subscribers() {
    let bus = EventBus::new();
    let rx = bus.subscribe();

    let event = BridgeMessage::Event(EventPayload {
        event_type: "bead_state_changed".to_string(),
        agent_id: None,
        bead_id: Some(Uuid::new_v4()),
        message: "State changed to review".to_string(),
        timestamp: Utc::now(),
    });

    bus.publish(event);

    let received = rx.try_recv().expect("subscriber should receive event");
    match &*received {
        BridgeMessage::Event(ep) => {
            assert_eq!(ep.event_type, "bead_state_changed");
            assert_eq!(ep.message, "State changed to review");
        }
        other => panic!("expected Event, got {:?}", other),
    }
}

#[test]
fn test_publish_multiple_events_in_order() {
    let bus = EventBus::new();
    let rx = bus.subscribe();

    let types = ["first", "second", "third", "fourth", "fifth"];
    for t in &types {
        bus.publish(BridgeMessage::Event(EventPayload {
            event_type: t.to_string(),
            agent_id: None,
            bead_id: None,
            message: format!("msg_{t}"),
            timestamp: Utc::now(),
        }));
    }

    for t in &types {
        let msg = rx.try_recv().expect("should receive message");
        match &*msg {
            BridgeMessage::Event(ep) => assert_eq!(ep.event_type, *t),
            other => panic!("expected Event, got {:?}", other),
        }
    }

    // No more messages
    assert!(rx.try_recv().is_err());
}

#[test]
fn test_publish_with_no_subscribers_doesnt_panic() {
    let bus = EventBus::new();
    assert_eq!(bus.subscriber_count(), 0);

    // Publishing to an empty bus should not panic
    bus.publish(BridgeMessage::GetStatus);
    bus.publish(BridgeMessage::GetKpi);
    bus.publish(BridgeMessage::ListAgents);

    // Still zero subscribers
    assert_eq!(bus.subscriber_count(), 0);
}

// ---------------------------------------------------------------------------
// Subscriber Management
// ---------------------------------------------------------------------------

#[test]
fn test_subscribe_returns_receiver() {
    let bus = EventBus::new();
    let rx = bus.subscribe();

    bus.publish(BridgeMessage::GetStatus);
    let msg = rx.try_recv();
    assert!(msg.is_ok());
    assert!(matches!(*msg.unwrap(), BridgeMessage::GetStatus));
}

#[test]
fn test_multiple_subscribers_all_receive() {
    let bus = EventBus::new();
    let receivers: Vec<_> = (0..5).map(|_| bus.subscribe()).collect();

    let bead_id = Uuid::new_v4();
    bus.publish(BridgeMessage::Event(EventPayload {
        event_type: "task_completed".to_string(),
        agent_id: None,
        bead_id: Some(bead_id),
        message: "done".to_string(),
        timestamp: Utc::now(),
    }));

    for (i, rx) in receivers.iter().enumerate() {
        let msg = rx
            .try_recv()
            .unwrap_or_else(|_| panic!("subscriber {} should have received the message", i));
        match &*msg {
            BridgeMessage::Event(ep) => {
                assert_eq!(ep.bead_id, Some(bead_id));
            }
            other => panic!("subscriber {} got unexpected {:?}", i, other),
        }
    }
}

#[test]
fn test_subscriber_count() {
    let bus = EventBus::new();
    assert_eq!(bus.subscriber_count(), 0);

    let rx1 = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 1);

    let rx2 = bus.subscribe();
    let rx3 = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 3);

    // Drop one receiver and publish to trigger pruning
    drop(rx1);
    bus.publish(BridgeMessage::GetStatus);
    assert_eq!(bus.subscriber_count(), 2);

    // Remaining receivers still work
    assert!(rx2.try_recv().is_ok());
    assert!(rx3.try_recv().is_ok());
}

// ---------------------------------------------------------------------------
// Event Types
// ---------------------------------------------------------------------------

#[test]
fn test_event_bead_created() {
    let bus = EventBus::new();
    let rx = bus.subscribe();

    let bead_id = Uuid::new_v4();
    bus.publish(BridgeMessage::Event(EventPayload {
        event_type: "bead_created".to_string(),
        agent_id: None,
        bead_id: Some(bead_id),
        message: "New bead created".to_string(),
        timestamp: Utc::now(),
    }));

    let msg = rx.try_recv().unwrap();
    match &*msg {
        BridgeMessage::Event(ep) => {
            assert_eq!(ep.event_type, "bead_created");
            assert_eq!(ep.bead_id, Some(bead_id));
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn test_event_bead_state_changed() {
    let bus = EventBus::new();
    let rx = bus.subscribe();

    let bead_id = Uuid::new_v4();
    bus.publish(BridgeMessage::Event(EventPayload {
        event_type: "bead_state_change".to_string(),
        agent_id: None,
        bead_id: Some(bead_id),
        message: "Moved to slung".to_string(),
        timestamp: Utc::now(),
    }));

    match &*rx.try_recv().unwrap() {
        BridgeMessage::Event(ep) => {
            assert_eq!(ep.event_type, "bead_state_change");
            assert_eq!(ep.message, "Moved to slung");
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn test_event_agent_spawned() {
    let bus = EventBus::new();
    let rx = bus.subscribe();

    let agent_id = Uuid::new_v4();
    bus.publish(BridgeMessage::Event(EventPayload {
        event_type: "agent_spawned".to_string(),
        agent_id: Some(agent_id),
        bead_id: None,
        message: "Agent online".to_string(),
        timestamp: Utc::now(),
    }));

    match &*rx.try_recv().unwrap() {
        BridgeMessage::Event(ep) => {
            assert_eq!(ep.event_type, "agent_spawned");
            assert_eq!(ep.agent_id, Some(agent_id));
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn test_event_agent_stopped() {
    let bus = EventBus::new();
    let rx = bus.subscribe();

    let agent_id = Uuid::new_v4();
    bus.publish(BridgeMessage::Event(EventPayload {
        event_type: "agent_stopped".to_string(),
        agent_id: Some(agent_id),
        bead_id: None,
        message: "Agent gracefully stopped".to_string(),
        timestamp: Utc::now(),
    }));

    match &*rx.try_recv().unwrap() {
        BridgeMessage::Event(ep) => {
            assert_eq!(ep.event_type, "agent_stopped");
            assert_eq!(ep.agent_id, Some(agent_id));
            assert_eq!(ep.message, "Agent gracefully stopped");
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn test_event_kpi_updated() {
    let bus = EventBus::new();
    let rx = bus.subscribe();

    bus.publish(BridgeMessage::KpiUpdate(KpiPayload {
        total_beads: 100,
        backlog: 20,
        hooked: 10,
        slung: 30,
        review: 15,
        done: 20,
        failed: 5,
        active_agents: 4,
    }));

    match &*rx.try_recv().unwrap() {
        BridgeMessage::KpiUpdate(kpi) => {
            assert_eq!(kpi.total_beads, 100);
            assert_eq!(kpi.active_agents, 4);
            assert_eq!(kpi.failed, 5);
        }
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn test_event_serialization_roundtrip() {
    let original = BridgeMessage::Event(EventPayload {
        event_type: "bead_state_change".to_string(),
        agent_id: Some(Uuid::new_v4()),
        bead_id: Some(Uuid::new_v4()),
        message: "Moved to done".to_string(),
        timestamp: Utc::now(),
    });

    let json = serde_json::to_string(&original).expect("serialize");
    let deserialized: BridgeMessage = serde_json::from_str(&json).expect("deserialize");

    match (&original, &deserialized) {
        (BridgeMessage::Event(a), BridgeMessage::Event(b)) => {
            assert_eq!(a.event_type, b.event_type);
            assert_eq!(a.agent_id, b.agent_id);
            assert_eq!(a.bead_id, b.bead_id);
            assert_eq!(a.message, b.message);
        }
        _ => panic!("roundtrip changed message type"),
    }

    // Also test other message types roundtrip
    let status = BridgeMessage::StatusUpdate(StatusPayload {
        version: "1.0.0".to_string(),
        uptime_seconds: 3600,
        agents_active: 2,
        beads_active: 10,
    });
    let json2 = serde_json::to_string(&status).unwrap();
    let rt2: BridgeMessage = serde_json::from_str(&json2).unwrap();
    match rt2 {
        BridgeMessage::StatusUpdate(s) => {
            assert_eq!(s.version, "1.0.0");
            assert_eq!(s.uptime_seconds, 3600);
        }
        _ => panic!("roundtrip failed for StatusUpdate"),
    }

    // KpiUpdate roundtrip
    let kpi = BridgeMessage::KpiUpdate(KpiPayload {
        total_beads: 50,
        backlog: 10,
        hooked: 5,
        slung: 15,
        review: 8,
        done: 10,
        failed: 2,
        active_agents: 3,
    });
    let json3 = serde_json::to_string(&kpi).unwrap();
    let rt3: BridgeMessage = serde_json::from_str(&json3).unwrap();
    match rt3 {
        BridgeMessage::KpiUpdate(k) => assert_eq!(k.total_beads, 50),
        _ => panic!("roundtrip failed for KpiUpdate"),
    }
}

// ---------------------------------------------------------------------------
// Concurrent Access
// ---------------------------------------------------------------------------

#[test]
fn test_concurrent_publish_and_subscribe() {
    let bus = EventBus::new();
    let num_publishers = 4;
    let msgs_per_publisher = 50;
    let rx = bus.subscribe();
    let barrier = Arc::new(Barrier::new(num_publishers + 1));

    let mut handles = Vec::new();
    for pub_id in 0..num_publishers {
        let bus_clone = bus.clone();
        let barrier_clone = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            barrier_clone.wait();
            for i in 0..msgs_per_publisher {
                bus_clone.publish(BridgeMessage::Event(EventPayload {
                    event_type: format!("concurrent_{pub_id}_{i}"),
                    agent_id: None,
                    bead_id: None,
                    message: format!("msg from publisher {pub_id}, seq {i}"),
                    timestamp: Utc::now(),
                }));
            }
        });
        handles.push(handle);
    }

    // Release all publishers simultaneously
    barrier.wait();

    for h in handles {
        h.join().expect("publisher thread panicked");
    }

    // Collect all received messages
    let mut received = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        received.push(msg);
    }

    assert_eq!(
        received.len(),
        num_publishers * msgs_per_publisher,
        "expected {} messages, got {}",
        num_publishers * msgs_per_publisher,
        received.len()
    );
}

#[test]
fn test_subscriber_backpressure() {
    // With unbounded channels, messages queue up without backpressure.
    // This test verifies that a slow consumer does not cause the publisher to
    // block or lose messages.
    let bus = EventBus::new();
    let rx = bus.subscribe();

    // Publish many messages without consuming
    let count = 1000;
    for i in 0..count {
        bus.publish(BridgeMessage::Event(EventPayload {
            event_type: format!("event_{i}"),
            agent_id: None,
            bead_id: None,
            message: format!("msg {i}"),
            timestamp: Utc::now(),
        }));
    }

    // Now consume all -- every message should be present
    let mut received = 0;
    while rx.try_recv().is_ok() {
        received += 1;
    }
    assert_eq!(received, count);
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_dropped_subscriber_is_pruned_on_next_publish() {
    let bus = EventBus::new();
    let rx1 = bus.subscribe();
    let rx2 = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 2);

    drop(rx1);
    // Pruning happens on publish
    bus.publish(BridgeMessage::GetStatus);
    assert_eq!(bus.subscriber_count(), 1);

    // rx2 still receives
    assert!(rx2.try_recv().is_ok());
}

#[test]
fn test_bus_clone_shares_subscribers() {
    let bus1 = EventBus::new();
    let bus2 = bus1.clone();

    let rx = bus1.subscribe();
    assert_eq!(bus2.subscriber_count(), 1);

    bus2.publish(BridgeMessage::ListAgents);
    assert!(matches!(*rx.try_recv().unwrap(), BridgeMessage::ListAgents));
}

#[test]
fn test_default_creates_empty_bus() {
    let bus = EventBus::default();
    assert_eq!(bus.subscriber_count(), 0);
}
