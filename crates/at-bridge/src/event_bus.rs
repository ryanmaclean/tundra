use std::sync::{Arc, Mutex};

use crate::protocol::BridgeMessage;

/// A broadcast-style event bus built on top of flume channels.
///
/// Each call to [`subscribe`] creates a new receiver that will receive all
/// messages published after the subscription was created. The bus is
/// thread-safe and can be cloned cheaply (it wraps its internals in an `Arc`).
#[derive(Clone)]
pub struct EventBus {
    inner: Arc<Mutex<Vec<flume::Sender<BridgeMessage>>>>,
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
    /// from this point forward.
    pub fn subscribe(&self) -> flume::Receiver<BridgeMessage> {
        let (tx, rx) = flume::unbounded();
        let mut senders = self.inner.lock().expect("EventBus lock poisoned");
        senders.push(tx);
        rx
    }

    /// Publish a message to all current subscribers.
    ///
    /// Disconnected subscribers (whose receivers have been dropped) are
    /// automatically pruned.
    pub fn publish(&self, msg: BridgeMessage) {
        let mut senders = self.inner.lock().expect("EventBus lock poisoned");
        senders.retain(|tx| tx.send(msg.clone()).is_ok());
    }

    /// Return the number of currently active subscribers.
    pub fn subscriber_count(&self) -> usize {
        let senders = self.inner.lock().expect("EventBus lock poisoned");
        senders.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
