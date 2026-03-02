use at_core::types::{AgentRole, Bead};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur during agent lifecycle operations.
///
/// The lifecycle system manages agent initialization, task assignment, heartbeats,
/// and shutdown. These errors represent failures during lifecycle transitions or
/// when agents are not in the expected state for an operation.
#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    /// A general lifecycle operation failure.
    ///
    /// This is a catch-all for lifecycle errors that don't fit specific
    /// categories, such as initialization failures or resource cleanup errors.
    /// The contained string provides details about the failure.
    #[error("lifecycle error: {0}")]
    General(String),

    /// The agent is not ready to perform the requested operation.
    ///
    /// This occurs when:
    /// - An operation is attempted before `on_start()` completes
    /// - The agent is in a paused or stopped state
    /// - Required initialization steps have not been performed
    ///
    /// The caller should retry after the agent transitions to a ready state.
    #[error("agent not ready")]
    NotReady,
}

/// Result type for lifecycle operations.
///
/// Alias for `std::result::Result<T, LifecycleError>` used throughout
/// the lifecycle system to indicate operations that may fail with a
/// [`LifecycleError`].
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
