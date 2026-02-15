use uuid::Uuid;

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
///
/// This is a stub implementation. Real logic will be wired in when the Tauri
/// frontend and the daemon backend are connected.
pub struct IpcHandler {
    event_bus: EventBus,
}

impl IpcHandler {
    /// Create a new handler wired to the given event bus.
    pub fn new(event_bus: EventBus) -> Self {
        Self { event_bus }
    }

    /// Return a reference to the underlying event bus.
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// Route an incoming message to the appropriate handler and return a
    /// response message.
    pub fn handle_message(&self, msg: BridgeMessage) -> Result<BridgeMessage> {
        match msg {
            BridgeMessage::GetStatus => self.handle_get_status(),
            BridgeMessage::ListBeads { status } => self.handle_list_beads(status),
            BridgeMessage::ListAgents => self.handle_list_agents(),
            BridgeMessage::SlingBead { bead_id, agent_id } => {
                self.handle_sling_bead(bead_id, agent_id)
            }
            BridgeMessage::HookBead { title, agent_name } => {
                self.handle_hook_bead(title, agent_name)
            }
            BridgeMessage::DoneBead { bead_id, failed } => {
                self.handle_done_bead(bead_id, failed)
            }
            BridgeMessage::NudgeAgent {
                agent_name,
                message,
            } => self.handle_nudge_agent(agent_name, message),
            BridgeMessage::GetKpi => self.handle_get_kpi(),
            // Backend -> Frontend messages should not arrive as requests.
            _ => Err(IpcError::UnknownMessage),
        }
    }

    // ------------------------------------------------------------------
    // Stub handlers – return placeholder data
    // ------------------------------------------------------------------

    fn handle_get_status(&self) -> Result<BridgeMessage> {
        Ok(BridgeMessage::StatusUpdate(StatusPayload {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: 0,
            agents_active: 0,
            beads_active: 0,
        }))
    }

    fn handle_list_beads(&self, _status: Option<String>) -> Result<BridgeMessage> {
        Ok(BridgeMessage::BeadList(Vec::new()))
    }

    fn handle_list_agents(&self) -> Result<BridgeMessage> {
        Ok(BridgeMessage::AgentList(Vec::new()))
    }

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

    fn handle_get_kpi(&self) -> Result<BridgeMessage> {
        Ok(BridgeMessage::KpiUpdate(KpiPayload {
            total_beads: 0,
            backlog: 0,
            hooked: 0,
            slung: 0,
            review: 0,
            done: 0,
            failed: 0,
            active_agents: 0,
        }))
    }
}
