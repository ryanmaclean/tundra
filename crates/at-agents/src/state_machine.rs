use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// AgentState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    Spawning,
    Active,
    Paused,
    Stopping,
    Stopped,
    Failed,
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            AgentState::Idle => "Idle",
            AgentState::Spawning => "Spawning",
            AgentState::Active => "Active",
            AgentState::Paused => "Paused",
            AgentState::Stopping => "Stopping",
            AgentState::Stopped => "Stopped",
            AgentState::Failed => "Failed",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// AgentEvent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEvent {
    Start,
    Spawned,
    Pause,
    Resume,
    Stop,
    Fail,
    Recover,
}

impl fmt::Display for AgentEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            AgentEvent::Start => "Start",
            AgentEvent::Spawned => "Spawned",
            AgentEvent::Pause => "Pause",
            AgentEvent::Resume => "Resume",
            AgentEvent::Stop => "Stop",
            AgentEvent::Fail => "Fail",
            AgentEvent::Recover => "Recover",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur during agent state machine transitions.
///
/// The state machine enforces valid state transitions for agent lifecycle
/// management (Idle → Spawning → Active → Stopped, etc.). This error indicates
/// an attempt to perform an invalid state transition.
#[derive(Debug, thiserror::Error)]
pub enum StateMachineError {
    /// An invalid state transition was attempted.
    ///
    /// This occurs when attempting to apply an [`AgentEvent`] that is not
    /// valid for the current [`AgentState`]. For example:
    /// - Trying to pause an agent that is already stopped
    /// - Attempting to spawn an agent that is already active
    /// - Recovering from a non-failed state
    ///
    /// The error contains the current state and the event that could not be
    /// applied, which helps identify the invalid transition attempt.
    #[error("invalid transition: cannot apply {event} in state {state}")]
    InvalidTransition {
        /// The current state when the invalid transition was attempted.
        state: AgentState,
        /// The event that could not be applied in the current state.
        event: AgentEvent,
    },
}

// ---------------------------------------------------------------------------
// AgentStateMachine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AgentStateMachine {
    current: AgentState,
    history: Vec<(AgentState, AgentEvent, AgentState)>,
}

impl AgentStateMachine {
    /// Create a new state machine starting in `Idle`.
    pub fn new() -> Self {
        Self {
            current: AgentState::Idle,
            history: Vec::new(),
        }
    }

    /// Return the current state.
    pub fn state(&self) -> AgentState {
        self.current
    }

    /// Return the full transition history.
    pub fn history(&self) -> &[(AgentState, AgentEvent, AgentState)] {
        &self.history
    }

    /// Attempt a state transition driven by `event`.
    ///
    /// Valid transitions:
    /// - Idle     + Start   -> Spawning
    /// - Spawning + Spawned -> Active
    /// - Spawning + Fail    -> Failed
    /// - Active   + Pause   -> Paused
    /// - Active   + Stop    -> Stopping
    /// - Active   + Fail    -> Failed
    /// - Paused   + Resume  -> Active
    /// - Paused   + Stop    -> Stopping
    /// - Stopping + Stop    -> Stopped
    /// - Stopping + Fail    -> Failed
    /// - Failed   + Recover -> Idle
    pub fn transition(&mut self, event: AgentEvent) -> Result<AgentState, StateMachineError> {
        let next = match (self.current, event) {
            (AgentState::Idle, AgentEvent::Start) => AgentState::Spawning,
            (AgentState::Spawning, AgentEvent::Spawned) => AgentState::Active,
            (AgentState::Spawning, AgentEvent::Fail) => AgentState::Failed,
            (AgentState::Active, AgentEvent::Pause) => AgentState::Paused,
            (AgentState::Active, AgentEvent::Stop) => AgentState::Stopping,
            (AgentState::Active, AgentEvent::Fail) => AgentState::Failed,
            (AgentState::Paused, AgentEvent::Resume) => AgentState::Active,
            (AgentState::Paused, AgentEvent::Stop) => AgentState::Stopping,
            (AgentState::Stopping, AgentEvent::Stop) => AgentState::Stopped,
            (AgentState::Stopping, AgentEvent::Fail) => AgentState::Failed,
            (AgentState::Failed, AgentEvent::Recover) => AgentState::Idle,
            _ => {
                return Err(StateMachineError::InvalidTransition {
                    state: self.current,
                    event,
                });
            }
        };

        let from = self.current;
        self.current = next;
        self.history.push((from, event, next));
        tracing::debug!(from = %from, event = %event, to = %next, "agent state transition");
        Ok(next)
    }

    /// Returns `true` if the given event is valid in the current state.
    pub fn can_transition(&self, event: AgentEvent) -> bool {
        matches!(
            (self.current, event),
            (AgentState::Idle, AgentEvent::Start)
                | (AgentState::Spawning, AgentEvent::Spawned)
                | (AgentState::Spawning, AgentEvent::Fail)
                | (AgentState::Active, AgentEvent::Pause)
                | (AgentState::Active, AgentEvent::Stop)
                | (AgentState::Active, AgentEvent::Fail)
                | (AgentState::Paused, AgentEvent::Resume)
                | (AgentState::Paused, AgentEvent::Stop)
                | (AgentState::Stopping, AgentEvent::Stop)
                | (AgentState::Stopping, AgentEvent::Fail)
                | (AgentState::Failed, AgentEvent::Recover)
        )
    }
}

impl Default for AgentStateMachine {
    fn default() -> Self {
        Self::new()
    }
}
