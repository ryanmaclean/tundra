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
    /// - Paused   + Fail    -> Failed
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
            (AgentState::Paused, AgentEvent::Fail) => AgentState::Failed,
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
                | (AgentState::Paused, AgentEvent::Fail)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive a fresh machine to the `Paused` state via `Idle -> Start -> Spawned -> Pause`.
    fn paused() -> AgentStateMachine {
        let mut sm = AgentStateMachine::new();
        sm.transition(AgentEvent::Start).unwrap();
        sm.transition(AgentEvent::Spawned).unwrap();
        sm.transition(AgentEvent::Pause).unwrap();
        assert_eq!(sm.state(), AgentState::Paused);
        sm
    }

    #[test]
    fn paused_fail_yields_failed() {
        let mut sm = paused();
        let next = sm.transition(AgentEvent::Fail).expect("Paused + Fail must succeed");
        assert_eq!(next, AgentState::Failed);
        assert_eq!(sm.state(), AgentState::Failed);
    }

    #[test]
    fn can_transition_paused_fail_returns_true() {
        let sm = paused();
        assert!(sm.can_transition(AgentEvent::Fail));
    }

    #[test]
    fn can_transition_matches_transition_for_all_state_event_pairs() {
        // 7 states × 7 events = 49 pairs.
        // For every pair, `can_transition` must agree with whether `transition` succeeds.
        let all_states = [
            AgentState::Idle,
            AgentState::Spawning,
            AgentState::Active,
            AgentState::Paused,
            AgentState::Stopping,
            AgentState::Stopped,
            AgentState::Failed,
        ];
        let all_events = [
            AgentEvent::Start,
            AgentEvent::Spawned,
            AgentEvent::Pause,
            AgentEvent::Resume,
            AgentEvent::Stop,
            AgentEvent::Fail,
            AgentEvent::Recover,
        ];

        let mut checked = 0usize;
        for &state in &all_states {
            for &event in &all_events {
                let sm_check = machine_in_state(state);
                let can = sm_check.can_transition(event);

                let mut sm_try = machine_in_state(state);
                let did = sm_try.transition(event).is_ok();

                assert_eq!(
                    can, did,
                    "can_transition disagrees with transition for ({state:?}, {event:?}): \
                     can_transition={can}, transition succeeded={did}"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 49, "expected exactly 49 state×event pairs");
    }

    /// Return a machine already in `state` by driving it through canonical transitions.
    fn machine_in_state(state: AgentState) -> AgentStateMachine {
        let mut sm = AgentStateMachine::new();
        match state {
            AgentState::Idle => {}
            AgentState::Spawning => {
                sm.transition(AgentEvent::Start).unwrap();
            }
            AgentState::Active => {
                sm.transition(AgentEvent::Start).unwrap();
                sm.transition(AgentEvent::Spawned).unwrap();
            }
            AgentState::Paused => {
                sm.transition(AgentEvent::Start).unwrap();
                sm.transition(AgentEvent::Spawned).unwrap();
                sm.transition(AgentEvent::Pause).unwrap();
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
        }
        assert_eq!(sm.state(), state);
        sm
    }
}
