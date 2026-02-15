use at_bridge::event_bus::EventBus;
use at_bridge::protocol::BridgeMessage;

#[test]
fn test_new_bus_has_no_subscribers() {
    let bus = EventBus::new();
    assert_eq!(bus.subscriber_count(), 0);
}

#[test]
fn test_subscribe_increments_count() {
    let bus = EventBus::new();
    let _rx1 = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 1);
    let _rx2 = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 2);
}

#[test]
fn test_publish_delivers_to_subscriber() {
    let bus = EventBus::new();
    let rx = bus.subscribe();

    bus.publish(BridgeMessage::GetStatus);

    let msg = rx.try_recv().expect("should receive message");
    assert!(matches!(msg, BridgeMessage::GetStatus));
}

#[test]
fn test_publish_delivers_to_multiple_subscribers() {
    let bus = EventBus::new();
    let rx1 = bus.subscribe();
    let rx2 = bus.subscribe();
    let rx3 = bus.subscribe();

    bus.publish(BridgeMessage::GetKpi);

    assert!(matches!(rx1.try_recv().unwrap(), BridgeMessage::GetKpi));
    assert!(matches!(rx2.try_recv().unwrap(), BridgeMessage::GetKpi));
    assert!(matches!(rx3.try_recv().unwrap(), BridgeMessage::GetKpi));
}

#[test]
fn test_dropped_receiver_is_pruned() {
    let bus = EventBus::new();
    let rx1 = bus.subscribe();
    let rx2 = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 2);

    drop(rx1);
    // Publish triggers pruning of disconnected senders.
    bus.publish(BridgeMessage::GetStatus);
    assert_eq!(bus.subscriber_count(), 1);

    // The surviving subscriber still receives the message.
    assert!(rx2.try_recv().is_ok());
}

#[test]
fn test_multiple_messages_ordering() {
    let bus = EventBus::new();
    let rx = bus.subscribe();

    bus.publish(BridgeMessage::GetStatus);
    bus.publish(BridgeMessage::GetKpi);
    bus.publish(BridgeMessage::ListAgents);

    assert!(matches!(rx.try_recv().unwrap(), BridgeMessage::GetStatus));
    assert!(matches!(rx.try_recv().unwrap(), BridgeMessage::GetKpi));
    assert!(matches!(rx.try_recv().unwrap(), BridgeMessage::ListAgents));
}

#[test]
fn test_subscriber_does_not_receive_messages_before_subscription() {
    let bus = EventBus::new();

    // Publish before subscribing.
    bus.publish(BridgeMessage::GetStatus);

    let rx = bus.subscribe();
    assert!(rx.try_recv().is_err());
}

#[test]
fn test_clone_shares_state() {
    let bus1 = EventBus::new();
    let bus2 = bus1.clone();

    let rx = bus1.subscribe();
    assert_eq!(bus2.subscriber_count(), 1);

    bus2.publish(BridgeMessage::GetStatus);
    assert!(rx.try_recv().is_ok());
}
