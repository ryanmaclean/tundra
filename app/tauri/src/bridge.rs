use at_bridge::event_bus::EventBus;
use at_bridge::protocol::BridgeMessage;

use crate::error::TauriError;

/// Manages the connection between Tauri commands and the at-bridge EventBus.
pub struct BridgeManager {
    bus: EventBus,
}

impl BridgeManager {
    /// Wrap an existing `EventBus`.
    pub fn new(bus: EventBus) -> Self {
        Self { bus }
    }

    /// Create a new subscription. The returned receiver will get every message
    /// published to the bus from this point forward.
    pub fn subscribe(&self) -> flume::Receiver<BridgeMessage> {
        self.bus.subscribe()
    }

    /// Publish a message to all current subscribers.
    pub fn publish(&self, msg: BridgeMessage) -> Result<(), TauriError> {
        self.bus.publish(msg);
        Ok(())
    }

    /// Return the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.bus.subscriber_count()
    }
}
