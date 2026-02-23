use ahash::AHashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::protocol::BridgeMessage;

// ---------------------------------------------------------------------------
// Transport errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("connection closed")]
    ConnectionClosed,

    #[error("send failed: {0}")]
    SendFailed(String),

    #[error("receive failed: {0}")]
    ReceiveFailed(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("transport not connected")]
    NotConnected,

    #[error("transport already connected")]
    AlreadyConnected,

    #[error("timeout after {0}ms")]
    Timeout(u64),
}

pub type Result<T> = std::result::Result<T, TransportError>;

// ---------------------------------------------------------------------------
// TransportKind — identifies which transport backend is in use
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    Stdio,
    WebSocket,
    Ipc,
    InProcess,
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportKind::Stdio => write!(f, "stdio"),
            TransportKind::WebSocket => write!(f, "websocket"),
            TransportKind::Ipc => write!(f, "ipc"),
            TransportKind::InProcess => write!(f, "in-process"),
        }
    }
}

// ---------------------------------------------------------------------------
// TransportState — connection lifecycle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Failed,
}

// ---------------------------------------------------------------------------
// ProxyTransport trait — the core abstraction (Lapce pattern)
// ---------------------------------------------------------------------------

/// A transport-agnostic channel for sending/receiving BridgeMessages.
///
/// Inspired by Lapce's proxy transport architecture: the frontend never
/// talks directly to the backend over a specific wire protocol. Instead it
/// goes through a `ProxyTransport` that can be swapped between stdio (CLI),
/// WebSocket (browser), IPC (Tauri), or in-process (tests) without changing
/// any calling code.
#[async_trait]
pub trait ProxyTransport: Send + Sync + 'static {
    /// The transport kind this implementation provides.
    fn kind(&self) -> TransportKind;

    /// Current connection state.
    fn state(&self) -> TransportState;

    /// Send a message through the transport.
    async fn send(&self, msg: BridgeMessage) -> Result<()>;

    /// Receive the next message. Blocks (async) until one arrives.
    async fn recv(&self) -> Result<BridgeMessage>;

    /// Attempt to connect or reconnect.
    async fn connect(&mut self) -> Result<()>;

    /// Gracefully close the transport.
    async fn disconnect(&mut self) -> Result<()>;
}

// ---------------------------------------------------------------------------
// InProcessTransport — for testing and in-memory use
// ---------------------------------------------------------------------------

/// An in-process transport backed by flume channels.
/// Useful for tests and when frontend/backend live in the same process.
pub struct InProcessTransport {
    state: TransportState,
    tx: flume::Sender<BridgeMessage>,
    rx: flume::Receiver<BridgeMessage>,
    _peer_tx: flume::Sender<BridgeMessage>,
    _peer_rx: flume::Receiver<BridgeMessage>,
}

impl InProcessTransport {
    /// Create a pair of connected in-process transports.
    pub fn pair() -> (Self, Self) {
        let (tx_a, rx_b) = flume::unbounded();
        let (tx_b, rx_a) = flume::unbounded();

        let a = Self {
            state: TransportState::Connected,
            tx: tx_a.clone(),
            rx: rx_a.clone(),
            _peer_tx: tx_b.clone(),
            _peer_rx: rx_b.clone(),
        };
        let b = Self {
            state: TransportState::Connected,
            tx: tx_b,
            rx: rx_b,
            _peer_tx: tx_a,
            _peer_rx: rx_a,
        };
        (a, b)
    }
}

#[async_trait]
impl ProxyTransport for InProcessTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::InProcess
    }

    fn state(&self) -> TransportState {
        self.state
    }

    async fn send(&self, msg: BridgeMessage) -> Result<()> {
        if self.state != TransportState::Connected {
            return Err(TransportError::NotConnected);
        }
        self.tx
            .send_async(msg)
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))
    }

    async fn recv(&self) -> Result<BridgeMessage> {
        if self.state != TransportState::Connected {
            return Err(TransportError::NotConnected);
        }
        self.rx
            .recv_async()
            .await
            .map_err(|e| TransportError::ReceiveFailed(e.to_string()))
    }

    async fn connect(&mut self) -> Result<()> {
        if self.state == TransportState::Connected {
            return Err(TransportError::AlreadyConnected);
        }
        self.state = TransportState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.state = TransportState::Disconnected;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TransportMetrics — observability
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransportMetrics {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub reconnect_count: u32,
    pub last_error: Option<String>,
}

// ---------------------------------------------------------------------------
// TransportRouter — multiplexes messages across multiple transports
// ---------------------------------------------------------------------------

/// Routes messages to/from multiple transports identified by session ID.
///
/// This allows the backend to maintain connections to multiple frontends
/// simultaneously (e.g., a TUI client and a web dashboard).
pub struct TransportRouter {
    transports: Arc<Mutex<AHashMap<Uuid, Box<dyn ProxyTransport>>>>,
    metrics: Arc<Mutex<AHashMap<Uuid, TransportMetrics>>>,
}

impl TransportRouter {
    pub fn new() -> Self {
        Self {
            transports: Arc::new(Mutex::new(AHashMap::new())),
            metrics: Arc::new(Mutex::new(AHashMap::new())),
        }
    }

    /// Register a transport for a session.
    pub fn register(&self, session_id: Uuid, transport: Box<dyn ProxyTransport>) {
        let kind = transport.kind();
        let mut ts = self.transports.lock().expect("router lock");
        ts.insert(session_id, transport);
        let mut ms = self.metrics.lock().expect("metrics lock");
        ms.insert(session_id, TransportMetrics::default());
        tracing::info!(%session_id, %kind, "transport registered");
    }

    /// Remove a transport for a session.
    pub fn unregister(&self, session_id: &Uuid) -> Option<Box<dyn ProxyTransport>> {
        let mut ts = self.transports.lock().expect("router lock");
        let removed = ts.remove(session_id);
        let mut ms = self.metrics.lock().expect("metrics lock");
        ms.remove(session_id);
        if removed.is_some() {
            tracing::info!(%session_id, "transport unregistered");
        }
        removed
    }

    /// Broadcast a message to all connected transports.
    #[allow(clippy::await_holding_lock)]
    pub async fn broadcast(&self, msg: BridgeMessage) -> Vec<(Uuid, TransportError)> {
        let session_ids: Vec<Uuid> = {
            let ts = self.transports.lock().expect("router lock");
            ts.keys().copied().collect()
        };

        let mut errors = Vec::new();
        for sid in session_ids {
            let result = {
                let ts = self.transports.lock().expect("router lock");
                if let Some(t) = ts.get(&sid) {
                    if t.state() == TransportState::Connected {
                        Some(t.kind())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if result.is_some() {
                let ts = self.transports.lock().expect("router lock");
                if let Some(t) = ts.get(&sid) {
                    if let Err(e) = t.send(msg.clone()).await {
                        errors.push((sid, e));
                    } else {
                        let mut ms = self.metrics.lock().expect("metrics lock");
                        if let Some(m) = ms.get_mut(&sid) {
                            m.messages_sent += 1;
                        }
                    }
                }
            }
        }

        errors
    }

    /// Get the number of registered transports.
    pub fn transport_count(&self) -> usize {
        let ts = self.transports.lock().expect("router lock");
        ts.len()
    }

    /// Get metrics for a specific session.
    pub fn metrics_for(&self, session_id: &Uuid) -> Option<TransportMetrics> {
        let ms = self.metrics.lock().expect("metrics lock");
        ms.get(session_id).cloned()
    }

    /// Get all session IDs with their transport kinds.
    pub fn sessions(&self) -> Vec<(Uuid, TransportKind)> {
        let ts = self.transports.lock().expect("router lock");
        ts.iter().map(|(id, t)| (*id, t.kind())).collect()
    }
}

impl Default for TransportRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_process_pair_send_recv() {
        let (a, b) = InProcessTransport::pair();
        a.send(BridgeMessage::GetStatus).await.unwrap();
        let msg = b.recv().await.unwrap();
        assert!(matches!(msg, BridgeMessage::GetStatus));
    }

    #[tokio::test]
    async fn in_process_bidirectional() {
        let (a, b) = InProcessTransport::pair();
        a.send(BridgeMessage::GetKpi).await.unwrap();
        let msg = b.recv().await.unwrap();
        assert!(matches!(msg, BridgeMessage::GetKpi));

        b.send(BridgeMessage::ListAgents).await.unwrap();
        let msg = a.recv().await.unwrap();
        assert!(matches!(msg, BridgeMessage::ListAgents));
    }

    #[tokio::test]
    async fn in_process_disconnect_prevents_send() {
        let (mut a, _b) = InProcessTransport::pair();
        a.disconnect().await.unwrap();
        let result = a.send(BridgeMessage::GetStatus).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn in_process_connect_errors_when_connected() {
        let (mut a, _b) = InProcessTransport::pair();
        let result = a.connect().await;
        assert!(matches!(result, Err(TransportError::AlreadyConnected)));
    }

    #[test]
    fn transport_kind_display_and_serialize() {
        assert_eq!(TransportKind::Stdio.to_string(), "stdio");
        assert_eq!(TransportKind::InProcess.to_string(), "in-process");

        let json = serde_json::to_string(&TransportKind::WebSocket).unwrap();
        assert_eq!(json, "\"web_socket\"");
        let back: TransportKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, TransportKind::WebSocket);
    }

    #[tokio::test]
    async fn router_register_and_broadcast() {
        let router = TransportRouter::new();
        let (a, b) = InProcessTransport::pair();
        let sid = Uuid::new_v4();
        router.register(sid, Box::new(a));
        assert_eq!(router.transport_count(), 1);

        let errors = router.broadcast(BridgeMessage::GetStatus).await;
        assert!(errors.is_empty());

        let msg = b.recv().await.unwrap();
        assert!(matches!(msg, BridgeMessage::GetStatus));
    }

    #[test]
    fn router_unregister() {
        let router = TransportRouter::new();
        let (a, _b) = InProcessTransport::pair();
        let sid = Uuid::new_v4();
        router.register(sid, Box::new(a));
        assert_eq!(router.transport_count(), 1);
        router.unregister(&sid);
        assert_eq!(router.transport_count(), 0);
    }

    #[test]
    fn router_sessions_list() {
        let router = TransportRouter::new();
        let (a, _b) = InProcessTransport::pair();
        let sid = Uuid::new_v4();
        router.register(sid, Box::new(a));
        let sessions = router.sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].0, sid);
        assert_eq!(sessions[0].1, TransportKind::InProcess);
    }

    #[test]
    fn router_metrics_tracking() {
        let router = TransportRouter::new();
        let (a, _b) = InProcessTransport::pair();
        let sid = Uuid::new_v4();
        router.register(sid, Box::new(a));
        let m = router.metrics_for(&sid).unwrap();
        assert_eq!(m.messages_sent, 0);
        assert_eq!(m.messages_received, 0);
    }

    #[test]
    fn transport_metrics_default() {
        let m = TransportMetrics::default();
        assert_eq!(m.messages_sent, 0);
        assert_eq!(m.reconnect_count, 0);
        assert!(m.last_error.is_none());
    }
}
