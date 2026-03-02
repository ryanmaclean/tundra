use at_core::types::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::lifecycle::AgentLifecycle;
use crate::roles::{CrewAgent, DeaconAgent, MayorAgent, PolecatAgent, RefineryAgent, WitnessAgent};
use crate::state_machine::{AgentEvent, AgentState, AgentStateMachine};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur during agent supervision and lifecycle management.
///
/// The supervisor orchestrates multiple agents, managing their state transitions,
/// lifecycle hooks, and recovery from failures. These errors represent failures
/// in agent spawning, state management, lifecycle operations, or general
/// supervision tasks.
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    /// The requested agent ID does not exist in the supervisor.
    ///
    /// This occurs when:
    /// - The agent ID is invalid or was never spawned
    /// - The agent has been stopped and removed from the supervisor
    ///
    /// The contained [`Uuid`] is the agent ID that was not found.
    #[error("agent not found: {0}")]
    AgentNotFound(Uuid),

    /// An error occurred during agent state transition.
    ///
    /// This wraps [`crate::state_machine::StateMachineError`] from the underlying
    /// state machine and indicates an invalid state transition was attempted
    /// (e.g., pausing an agent that is already stopped).
    ///
    /// The error is automatically converted from
    /// [`crate::state_machine::StateMachineError`] via the `#[from]` attribute.
    #[error("state machine error: {0}")]
    StateMachine(#[from] crate::state_machine::StateMachineError),

    /// An error occurred during agent lifecycle hook execution.
    ///
    /// This wraps [`crate::lifecycle::LifecycleError`] from the agent lifecycle
    /// system and indicates a failure during `on_start()`, `on_stop()`,
    /// `on_heartbeat()`, or other lifecycle callbacks.
    ///
    /// The error is automatically converted from [`crate::lifecycle::LifecycleError`]
    /// via the `#[from]` attribute.
    #[error("lifecycle error: {0}")]
    Lifecycle(#[from] crate::lifecycle::LifecycleError),

    /// A general supervisor error occurred.
    ///
    /// This is a catch-all for supervision failures that don't fit other
    /// categories, such as internal consistency errors or unexpected conditions.
    /// The contained string provides error details.
    #[error("supervisor error: {0}")]
    General(String),
}

/// Result type for supervisor operations.
///
/// Alias for `std::result::Result<T, SupervisorError>` used throughout
/// the supervisor module to indicate operations that may fail with a
/// [`SupervisorError`].
pub type Result<T> = std::result::Result<T, SupervisorError>;

// ---------------------------------------------------------------------------
// AgentInfo — public view of a managed agent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: Uuid,
    pub name: String,
    pub role: AgentRole,
    pub state: AgentState,
    pub last_seen: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ManagedAgent — internal bookkeeping
// ---------------------------------------------------------------------------

struct ManagedAgent {
    id: Uuid,
    name: String,
    role: AgentRole,
    sm: AgentStateMachine,
    lifecycle: Box<dyn AgentLifecycle>,
    last_seen: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// AgentSupervisor
// ---------------------------------------------------------------------------

pub struct AgentSupervisor {
    agents: Arc<Mutex<HashMap<Uuid, ManagedAgent>>>,
}

impl AgentSupervisor {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Spawn a new agent with the given name and role.
    /// Returns the unique id assigned to the agent.
    pub async fn spawn_agent(&self, name: impl Into<String>, role: AgentRole) -> Result<Uuid> {
        let name = name.into();
        let id = Uuid::new_v4();
        let mut sm = AgentStateMachine::new();

        // Transition Idle -> Spawning
        sm.transition(AgentEvent::Start)?;

        let mut lifecycle: Box<dyn AgentLifecycle> = match role {
            AgentRole::Mayor => Box::new(MayorAgent::new()),
            AgentRole::Deacon | AgentRole::QaReviewer | AgentRole::SpecCritic => {
                Box::new(DeaconAgent::new())
            }
            AgentRole::Witness | AgentRole::QaFixer | AgentRole::ValidationFixer => {
                Box::new(WitnessAgent::new())
            }
            AgentRole::Refinery => Box::new(RefineryAgent::new()),
            AgentRole::Polecat => Box::new(PolecatAgent::new()),
            // All other roles use Crew as the base lifecycle for now.
            // Specialized prompts are injected via context steering, not lifecycle.
            _ => Box::new(CrewAgent::new()),
        };

        // Call on_start and transition Spawning -> Active
        lifecycle.on_start().await?;
        sm.transition(AgentEvent::Spawned)?;

        let managed = ManagedAgent {
            id,
            name: name.clone(),
            role: role.clone(),
            sm,
            lifecycle,
            last_seen: Utc::now(),
        };

        self.agents.lock().await.insert(id, managed);
        tracing::info!(id = %id, name = %name, role = ?role, "agent spawned");
        Ok(id)
    }

    /// Stop an active agent.
    pub async fn stop_agent(&self, id: Uuid) -> Result<()> {
        let mut agents = self.agents.lock().await;
        let agent = agents
            .get_mut(&id)
            .ok_or(SupervisorError::AgentNotFound(id))?;

        agent.sm.transition(AgentEvent::Stop)?;
        agent.lifecycle.on_stop().await?;
        agent.sm.transition(AgentEvent::Stop)?; // Stopping -> Stopped
        agent.last_seen = Utc::now();

        tracing::info!(id = %id, "agent stopped");
        Ok(())
    }

    /// List all managed agents.
    pub async fn list_agents(&self) -> Vec<AgentInfo> {
        let agents = self.agents.lock().await;
        agents
            .values()
            .map(|a| AgentInfo {
                id: a.id,
                name: a.name.clone(),
                role: a.role.clone(),
                state: a.sm.state(),
                last_seen: a.last_seen,
            })
            .collect()
    }

    /// Send heartbeat to all active agents.
    pub async fn send_heartbeat_all(&self) -> Result<()> {
        let mut agents = self.agents.lock().await;
        for agent in agents.values_mut() {
            if agent.sm.state() == AgentState::Active {
                agent.lifecycle.on_heartbeat().await?;
                agent.last_seen = Utc::now();
            }
        }
        Ok(())
    }

    /// Restart agents that are in the Failed state.
    pub async fn restart_failed(&self) -> Result<Vec<Uuid>> {
        let mut restarted = Vec::new();
        let mut agents = self.agents.lock().await;

        for agent in agents.values_mut() {
            if agent.sm.state() == AgentState::Failed {
                // Recover: Failed -> Idle
                agent.sm.transition(AgentEvent::Recover)?;
                // Start: Idle -> Spawning
                agent.sm.transition(AgentEvent::Start)?;
                // on_start + Spawned: Spawning -> Active
                agent.lifecycle.on_start().await?;
                agent.sm.transition(AgentEvent::Spawned)?;
                agent.last_seen = Utc::now();
                restarted.push(agent.id);
                tracing::info!(id = %agent.id, "agent restarted after failure");
            }
        }

        Ok(restarted)
    }

    /// Return the number of managed agents.
    pub async fn agent_count(&self) -> usize {
        self.agents.lock().await.len()
    }
}

impl Default for AgentSupervisor {
    fn default() -> Self {
        Self::new()
    }
}
