use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use at_core::types::{Agent, Bead, BeadStatus};

use crate::event_bus::EventBus;
use crate::protocol::{BridgeMessage, KpiPayload, StatusPayload};

/// Errors that can occur during IPC message handling.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("unknown message type")]
    UnknownMessage,

    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, IpcError>;

/// Handles incoming IPC messages and produces responses.
pub struct IpcHandler {
    event_bus: EventBus,
    beads: Arc<RwLock<std::collections::HashMap<Uuid, Bead>>>,
    agents: Arc<RwLock<std::collections::HashMap<Uuid, Agent>>>,
    start_time: std::time::Instant,
}

impl IpcHandler {
    /// Create a new handler wired to the given event bus with shared state.
    pub fn new(
        event_bus: EventBus,
        beads: Arc<RwLock<std::collections::HashMap<Uuid, Bead>>>,
        agents: Arc<RwLock<std::collections::HashMap<Uuid, Agent>>>,
        start_time: std::time::Instant,
    ) -> Self {
        Self {
            event_bus,
            beads,
            agents,
            start_time,
        }
    }

    /// Create a handler with empty state, useful for tests and bootstrapping.
    pub fn new_stub(event_bus: EventBus) -> Self {
        Self {
            event_bus,
            beads: Arc::new(RwLock::new(std::collections::HashMap::new())),
            agents: Arc::new(RwLock::new(std::collections::HashMap::new())),
            start_time: std::time::Instant::now(),
        }
    }

    /// Return a reference to the underlying event bus.
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// Route an incoming message to the appropriate handler and return a
    /// response message.
    pub async fn handle_message(&self, msg: BridgeMessage) -> Result<BridgeMessage> {
        match msg {
            BridgeMessage::GetStatus => self.handle_get_status().await,
            BridgeMessage::ListBeads { status } => self.handle_list_beads(status).await,
            BridgeMessage::ListAgents => self.handle_list_agents().await,
            BridgeMessage::SlingBead { bead_id, agent_id } => {
                self.handle_sling_bead(bead_id, agent_id)
            }
            BridgeMessage::HookBead { title, agent_name } => {
                self.handle_hook_bead(title, agent_name)
            }
            BridgeMessage::DoneBead { bead_id, failed } => self.handle_done_bead(bead_id, failed),
            BridgeMessage::NudgeAgent {
                agent_name,
                message,
            } => self.handle_nudge_agent(agent_name, message),
            BridgeMessage::GetKpi => self.handle_get_kpi().await,
            // Backend -> Frontend messages should not arrive as requests.
            _ => Err(IpcError::UnknownMessage),
        }
    }

    // ------------------------------------------------------------------
    // Handlers that read shared state
    // ------------------------------------------------------------------

    async fn handle_get_status(&self) -> Result<BridgeMessage> {
        let agents = self.agents.read().await;
        let beads = self.beads.read().await;
        let beads_active = beads
            .values()
            .filter(|b| !matches!(b.status, BeadStatus::Done | BeadStatus::Failed))
            .count() as u32;
        Ok(BridgeMessage::StatusUpdate(StatusPayload {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            agents_active: agents.len() as u32,
            beads_active,
        }))
    }

    async fn handle_list_beads(&self, status: Option<String>) -> Result<BridgeMessage> {
        let beads = self.beads.read().await;
        let filtered: Vec<Bead> = match status {
            Some(ref s) => beads
                .values()
                .filter(|b| {
                    // Compare against the snake_case serde representation
                    let bead_status_str = serde_json::to_value(&b.status)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default();
                    bead_status_str == *s
                })
                .cloned()
                .collect(),
            None => beads.values().cloned().collect(),
        };
        Ok(BridgeMessage::BeadList(filtered))
    }

    async fn handle_list_agents(&self) -> Result<BridgeMessage> {
        let agents = self.agents.read().await;
        Ok(BridgeMessage::AgentList(agents.values().cloned().collect()))
    }

    async fn handle_get_kpi(&self) -> Result<BridgeMessage> {
        let beads = self.beads.read().await;
        let agents = self.agents.read().await;

        let mut backlog: u64 = 0;
        let mut hooked: u64 = 0;
        let mut slung: u64 = 0;
        let mut review: u64 = 0;
        let mut done: u64 = 0;
        let mut failed: u64 = 0;

        for bead in beads.values() {
            match bead.status {
                BeadStatus::Backlog => backlog += 1,
                BeadStatus::Hooked => hooked += 1,
                BeadStatus::Slung => slung += 1,
                BeadStatus::Review => review += 1,
                BeadStatus::Done => done += 1,
                BeadStatus::Failed => failed += 1,
                BeadStatus::Escalated => backlog += 1, // count escalated with backlog
            }
        }

        Ok(BridgeMessage::KpiUpdate(KpiPayload {
            total_beads: beads.len() as u64,
            backlog,
            hooked,
            slung,
            review,
            done,
            failed,
            active_agents: agents.len() as u64,
        }))
    }

    // ------------------------------------------------------------------
    // Handlers that publish events (synchronous, no shared state needed)
    // ------------------------------------------------------------------

    fn handle_sling_bead(&self, bead_id: Uuid, agent_id: Uuid) -> Result<BridgeMessage> {
        let msg = BridgeMessage::Event(crate::protocol::EventPayload {
            event_type: "bead_slung".to_string(),
            agent_id: Some(agent_id),
            bead_id: Some(bead_id),
            message: format!("Bead {} slung to agent {}", bead_id, agent_id),
            timestamp: chrono::Utc::now(),
        });
        self.event_bus.publish(msg.clone());
        Ok(msg)
    }

    fn handle_hook_bead(&self, title: String, agent_name: String) -> Result<BridgeMessage> {
        let msg = BridgeMessage::Event(crate::protocol::EventPayload {
            event_type: "bead_hooked".to_string(),
            agent_id: None,
            bead_id: None,
            message: format!("Bead '{}' hooked by agent '{}'", title, agent_name),
            timestamp: chrono::Utc::now(),
        });
        self.event_bus.publish(msg.clone());
        Ok(msg)
    }

    fn handle_done_bead(&self, bead_id: Uuid, failed: bool) -> Result<BridgeMessage> {
        let event_type = if failed { "bead_failed" } else { "bead_done" };
        let msg = BridgeMessage::Event(crate::protocol::EventPayload {
            event_type: event_type.to_string(),
            agent_id: None,
            bead_id: Some(bead_id),
            message: format!(
                "Bead {} {}",
                bead_id,
                if failed { "failed" } else { "completed" }
            ),
            timestamp: chrono::Utc::now(),
        });
        self.event_bus.publish(msg.clone());
        Ok(msg)
    }

    fn handle_nudge_agent(&self, agent_name: String, message: String) -> Result<BridgeMessage> {
        let msg = BridgeMessage::Event(crate::protocol::EventPayload {
            event_type: "agent_nudged".to_string(),
            agent_id: None,
            bead_id: None,
            message: format!("Nudged '{}': {}", agent_name, message),
            timestamp: chrono::Utc::now(),
        });
        self.event_bus.publish(msg.clone());
        Ok(msg)
    }
}
