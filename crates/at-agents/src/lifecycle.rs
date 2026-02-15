use at_core::types::{AgentRole, Bead};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    #[error("lifecycle error: {0}")]
    General(String),

    #[error("agent not ready")]
    NotReady,
}

pub type Result<T> = std::result::Result<T, LifecycleError>;

// ---------------------------------------------------------------------------
// AgentLifecycle trait
// ---------------------------------------------------------------------------

/// Trait that every agent role must implement to participate in the
/// supervisor-managed lifecycle.
#[async_trait::async_trait]
pub trait AgentLifecycle: Send + Sync {
    /// The role this agent fulfils in the gastown taxonomy.
    fn role(&self) -> AgentRole;

    /// Called when the agent is started.
    async fn on_start(&mut self) -> Result<()>;

    /// Called when a new bead is assigned to this agent.
    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()>;

    /// Called when a bead has been completed by this agent.
    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()>;

    /// Periodic heartbeat; the agent should report health here.
    async fn on_heartbeat(&mut self) -> Result<()>;

    /// Called when the agent is being stopped.
    async fn on_stop(&mut self) -> Result<()>;
}
