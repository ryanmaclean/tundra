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

// ---------------------------------------------------------------------------
// Test-only seams
// ---------------------------------------------------------------------------

#[cfg(test)]
impl AgentSupervisor {
    /// Return the current [`AgentState`] for the given agent id, or `None` if
    /// the agent does not exist in the map.
    pub(crate) async fn agent_state(&self, id: Uuid) -> Option<crate::state_machine::AgentState> {
        self.agents.lock().await.get(&id).map(|a| a.sm.state())
    }

    /// Insert a pre-built [`ManagedAgent`] directly into the supervisor map.
    /// Only used by unit tests that need to seed the supervisor with a specific
    /// initial state without going through the full `spawn_agent` path.
    pub(crate) async fn insert_managed(
        &self,
        id: Uuid,
        name: impl Into<String>,
        role: at_core::types::AgentRole,
        sm: crate::state_machine::AgentStateMachine,
        lifecycle: Box<dyn AgentLifecycle>,
    ) {
        self.agents.lock().await.insert(
            id,
            ManagedAgent {
                id,
                name: name.into(),
                role,
                sm,
                lifecycle,
                last_seen: chrono::Utc::now(),
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::types::{AgentRole, Bead};
    use crate::lifecycle::LifecycleError;
    use crate::state_machine::{AgentEvent, AgentState, AgentStateMachine};
    use std::sync::{Arc, Mutex as StdMutex};
    use uuid::Uuid;

    // -----------------------------------------------------------------------
    // MockLifecycle
    // -----------------------------------------------------------------------

    /// Records every lifecycle method call so tests can assert on the exact
    /// sequence of interactions the supervisor performs.  Per-method return
    /// values can be pre-loaded via the `fail_*` flags.
    struct MockLifecycle {
        calls: Arc<StdMutex<Vec<String>>>,
        fail_on_start: bool,
        fail_on_stop: bool,
    }

    impl MockLifecycle {
        fn new() -> Self {
            Self {
                calls: Arc::new(StdMutex::new(Vec::new())),
                fail_on_start: false,
                fail_on_stop: false,
            }
        }
    }

    #[async_trait::async_trait]
    impl AgentLifecycle for MockLifecycle {
        fn role(&self) -> AgentRole {
            AgentRole::Crew
        }

        async fn on_start(&mut self) -> crate::lifecycle::Result<()> {
            self.calls.lock().unwrap().push("on_start".into());
            if self.fail_on_start {
                Err(LifecycleError::General("injected start failure".into()))
            } else {
                Ok(())
            }
        }

        async fn on_task_assigned(&mut self, _bead: &Bead) -> crate::lifecycle::Result<()> {
            self.calls.lock().unwrap().push("on_task_assigned".into());
            Ok(())
        }

        async fn on_task_completed(&mut self, _bead_id: Uuid) -> crate::lifecycle::Result<()> {
            self.calls.lock().unwrap().push("on_task_completed".into());
            Ok(())
        }

        async fn on_heartbeat(&mut self) -> crate::lifecycle::Result<()> {
            self.calls.lock().unwrap().push("on_heartbeat".into());
            Ok(())
        }

        async fn on_stop(&mut self) -> crate::lifecycle::Result<()> {
            self.calls.lock().unwrap().push("on_stop".into());
            if self.fail_on_stop {
                Err(LifecycleError::General("injected stop failure".into()))
            } else {
                Ok(())
            }
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Build a supervisor and seed it with a single agent whose state machine
    /// has already been driven to `initial_state`.  Returns the supervisor, the
    /// agent id, and a shared handle to the mock's call recorder.
    async fn make_supervisor_with_agent(
        initial_state: AgentState,
        lifecycle: Box<dyn AgentLifecycle>,
    ) -> (AgentSupervisor, Uuid) {
        let sup = AgentSupervisor::new();
        let id = Uuid::new_v4();

        // Build a state machine at the desired initial state.
        let sm = build_sm_at(initial_state);
        sup.insert_managed(id, "test-agent", AgentRole::Crew, sm, lifecycle)
            .await;
        (sup, id)
    }

    /// Drive a fresh `AgentStateMachine` to `target` via the minimum set of
    /// valid events.  Panics if `target` is a state this helper doesn't know
    /// how to reach.
    fn build_sm_at(target: AgentState) -> AgentStateMachine {
        let mut sm = AgentStateMachine::new();
        match target {
            AgentState::Idle => { /* already Idle */ }
            AgentState::Spawning => {
                sm.transition(AgentEvent::Start).unwrap();
            }
            AgentState::Active => {
                sm.transition(AgentEvent::Start).unwrap();
                sm.transition(AgentEvent::Spawned).unwrap();
            }
            AgentState::Stopping => {
                sm.transition(AgentEvent::Start).unwrap();
                sm.transition(AgentEvent::Spawned).unwrap();
                sm.transition(AgentEvent::Stop).unwrap();
            }
            AgentState::Stopped => {
                sm.transition(AgentEvent::Start).unwrap();
                sm.transition(AgentEvent::Spawned).unwrap();
                sm.transition(AgentEvent::Stop).unwrap();
                sm.transition(AgentEvent::Stop).unwrap();
            }
            AgentState::Failed => {
                sm.transition(AgentEvent::Start).unwrap();
                sm.transition(AgentEvent::Fail).unwrap();
            }
            AgentState::Paused => {
                sm.transition(AgentEvent::Start).unwrap();
                sm.transition(AgentEvent::Spawned).unwrap();
                sm.transition(AgentEvent::Pause).unwrap();
            }
        }
        sm
    }

    // -----------------------------------------------------------------------
    // Test A: restart_failed — happy path
    // -----------------------------------------------------------------------

    /// When an agent is in the `Failed` state, `restart_failed` must:
    ///   1. call `on_start()` on the lifecycle exactly once,
    ///   2. drive the state machine through Failed -> Idle -> Spawning -> Active,
    ///   3. return the agent's id in the restarted list.
    #[tokio::test]
    async fn restart_failed_happy_path() {
        let mock = MockLifecycle::new();
        let calls = mock.calls.clone();
        let (sup, id) = make_supervisor_with_agent(AgentState::Failed, Box::new(mock)).await;

        let restarted = sup.restart_failed().await.expect("restart_failed must succeed");

        // The agent id is in the returned list.
        assert_eq!(restarted, vec![id], "restarted list must contain the agent id");

        // The lifecycle hook was called exactly once.
        let recorded = calls.lock().unwrap().clone();
        assert_eq!(
            recorded,
            vec!["on_start"],
            "restart_failed must call on_start exactly once; got: {recorded:?}"
        );

        // The internal state map must reflect Active.
        let state = sup
            .agent_state(id)
            .await
            .expect("agent must still be in the map");
        assert_eq!(
            state,
            AgentState::Active,
            "agent must be Active after successful restart; got {state:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Test B: restart_failed — non-Failed state returns error, no state change
    // -----------------------------------------------------------------------

    /// When an agent is `Active` (not `Failed`), `restart_failed` must skip it
    /// without modifying the state map.  If there are no failed agents the
    /// method returns an empty `Ok` list; calling it on an agent that is already
    /// Active should result in zero restarts and zero lifecycle calls.
    #[tokio::test]
    async fn restart_failed_skips_non_failed_agents() {
        let mock = MockLifecycle::new();
        let calls = mock.calls.clone();
        let (sup, id) = make_supervisor_with_agent(AgentState::Active, Box::new(mock)).await;

        let restarted = sup
            .restart_failed()
            .await
            .expect("restart_failed must not error when no agents are Failed");

        // No agents should have been restarted.
        assert!(
            restarted.is_empty(),
            "expected no restarts for an Active agent, got: {restarted:?}"
        );

        // No lifecycle methods should have been invoked.
        let recorded = calls.lock().unwrap().clone();
        assert!(
            recorded.is_empty(),
            "no lifecycle calls expected for a non-Failed agent; got: {recorded:?}"
        );

        // The state map must remain unchanged.
        let state = sup
            .agent_state(id)
            .await
            .expect("agent must still be in the map");
        assert_eq!(
            state,
            AgentState::Active,
            "agent state must remain Active; got {state:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Test C: stop_agent — happy path
    // -----------------------------------------------------------------------

    /// When an agent is `Active`, `stop_agent` must:
    ///   1. call `on_stop()` on the lifecycle exactly once,
    ///   2. drive the state machine from Active -> Stopping -> Stopped.
    #[tokio::test]
    async fn stop_agent_happy_path() {
        let mock = MockLifecycle::new();
        let calls = mock.calls.clone();
        let (sup, id) = make_supervisor_with_agent(AgentState::Active, Box::new(mock)).await;

        sup.stop_agent(id).await.expect("stop_agent must succeed");

        // The lifecycle hook was called exactly once.
        let recorded = calls.lock().unwrap().clone();
        assert_eq!(
            recorded,
            vec!["on_stop"],
            "stop_agent must call on_stop exactly once; got: {recorded:?}"
        );

        // The internal state map must reflect Stopped.
        let state = sup
            .agent_state(id)
            .await
            .expect("agent must still be in the map after stop");
        assert_eq!(
            state,
            AgentState::Stopped,
            "agent must be Stopped after stop_agent; got {state:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Test D: stop_agent — already-Stopped agent returns a StateMachine error
    // -----------------------------------------------------------------------

    /// Calling `stop_agent` on a `Stopped` agent must return a
    /// `SupervisorError::StateMachine(StateMachineError::InvalidTransition)`
    /// because `Stopped + Stop` is not in the transition table.
    #[tokio::test]
    async fn stop_agent_already_stopped_returns_error() {
        let mock = MockLifecycle::new();
        let calls = mock.calls.clone();
        let (sup, id) = make_supervisor_with_agent(AgentState::Stopped, Box::new(mock)).await;

        let result = sup.stop_agent(id).await;

        // Must fail with a StateMachine error.
        assert!(
            result.is_err(),
            "stop_agent on a Stopped agent must return an error"
        );
        assert!(
            matches!(result.unwrap_err(), SupervisorError::StateMachine(_)),
            "error must be SupervisorError::StateMachine"
        );

        // No lifecycle calls should have been made — the state machine check
        // fires before the lifecycle hook.
        let recorded = calls.lock().unwrap().clone();
        assert!(
            recorded.is_empty(),
            "no lifecycle calls expected when state-machine rejects the transition; got: {recorded:?}"
        );

        // The state map must be unchanged.
        let state = sup
            .agent_state(id)
            .await
            .expect("agent must still be in the map");
        assert_eq!(
            state,
            AgentState::Stopped,
            "agent state must remain Stopped; got {state:?}"
        );
    }
}
