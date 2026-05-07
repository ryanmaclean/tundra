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

#[cfg(test)]
mod tests {
    use super::{AgentEvent, AgentState, AgentStateMachine, StateMachineError};

    // -----------------------------------------------------------------------
    // Helper: advance the machine through a sequence of known-valid events.
    // -----------------------------------------------------------------------

    fn advance(sm: &mut AgentStateMachine, events: &[AgentEvent]) -> AgentState {
        let mut state = sm.state();
        for &ev in events {
            state = sm.transition(ev).unwrap_or_else(|e| {
                panic!("unexpected invalid transition: {e}");
            });
        }
        state
    }

    // -----------------------------------------------------------------------
    // Initial state
    // -----------------------------------------------------------------------

    #[test]
    fn new_machine_starts_in_idle() {
        let sm = AgentStateMachine::new();
        assert_eq!(sm.state(), AgentState::Idle);
    }

    #[test]
    fn default_machine_starts_in_idle() {
        let sm = AgentStateMachine::default();
        assert_eq!(sm.state(), AgentState::Idle);
    }

    // -----------------------------------------------------------------------
    // Legal transitions — one test per edge in the documented graph
    // -----------------------------------------------------------------------

    #[test]
    fn idle_start_yields_spawning() {
        let mut sm = AgentStateMachine::new();
        let next = sm.transition(AgentEvent::Start).unwrap();
        assert_eq!(next, AgentState::Spawning);
        assert_eq!(sm.state(), AgentState::Spawning);
    }

    #[test]
    fn spawning_spawned_yields_active() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start]);
        let next = sm.transition(AgentEvent::Spawned).unwrap();
        assert_eq!(next, AgentState::Active);
        assert_eq!(sm.state(), AgentState::Active);
    }

    #[test]
    fn spawning_fail_yields_failed() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start]);
        let next = sm.transition(AgentEvent::Fail).unwrap();
        assert_eq!(next, AgentState::Failed);
        assert_eq!(sm.state(), AgentState::Failed);
    }

    #[test]
    fn active_pause_yields_paused() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start, AgentEvent::Spawned]);
        let next = sm.transition(AgentEvent::Pause).unwrap();
        assert_eq!(next, AgentState::Paused);
        assert_eq!(sm.state(), AgentState::Paused);
    }

    #[test]
    fn active_stop_yields_stopping() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start, AgentEvent::Spawned]);
        let next = sm.transition(AgentEvent::Stop).unwrap();
        assert_eq!(next, AgentState::Stopping);
        assert_eq!(sm.state(), AgentState::Stopping);
    }

    #[test]
    fn active_fail_yields_failed() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start, AgentEvent::Spawned]);
        let next = sm.transition(AgentEvent::Fail).unwrap();
        assert_eq!(next, AgentState::Failed);
        assert_eq!(sm.state(), AgentState::Failed);
    }

    #[test]
    fn paused_resume_yields_active() {
        let mut sm = AgentStateMachine::new();
        advance(
            &mut sm,
            &[AgentEvent::Start, AgentEvent::Spawned, AgentEvent::Pause],
        );
        let next = sm.transition(AgentEvent::Resume).unwrap();
        assert_eq!(next, AgentState::Active);
        assert_eq!(sm.state(), AgentState::Active);
    }

    #[test]
    fn paused_stop_yields_stopping() {
        let mut sm = AgentStateMachine::new();
        advance(
            &mut sm,
            &[AgentEvent::Start, AgentEvent::Spawned, AgentEvent::Pause],
        );
        let next = sm.transition(AgentEvent::Stop).unwrap();
        assert_eq!(next, AgentState::Stopping);
        assert_eq!(sm.state(), AgentState::Stopping);
    }

    #[test]
    fn stopping_stop_yields_stopped() {
        let mut sm = AgentStateMachine::new();
        advance(
            &mut sm,
            &[AgentEvent::Start, AgentEvent::Spawned, AgentEvent::Stop],
        );
        let next = sm.transition(AgentEvent::Stop).unwrap();
        assert_eq!(next, AgentState::Stopped);
        assert_eq!(sm.state(), AgentState::Stopped);
    }

    #[test]
    fn stopping_fail_yields_failed() {
        let mut sm = AgentStateMachine::new();
        advance(
            &mut sm,
            &[AgentEvent::Start, AgentEvent::Spawned, AgentEvent::Stop],
        );
        let next = sm.transition(AgentEvent::Fail).unwrap();
        assert_eq!(next, AgentState::Failed);
        assert_eq!(sm.state(), AgentState::Failed);
    }

    #[test]
    fn failed_recover_yields_idle() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start, AgentEvent::Fail]);
        let next = sm.transition(AgentEvent::Recover).unwrap();
        assert_eq!(next, AgentState::Idle);
        assert_eq!(sm.state(), AgentState::Idle);
    }

    // -----------------------------------------------------------------------
    // Error-recovery path — exhaustive multi-step test
    // -----------------------------------------------------------------------

    #[test]
    fn full_recovery_cycle_active_fail_recover_then_active_again() {
        let mut sm = AgentStateMachine::new();
        // Reach Active
        assert_eq!(advance(&mut sm, &[AgentEvent::Start]), AgentState::Spawning);
        assert_eq!(
            advance(&mut sm, &[AgentEvent::Spawned]),
            AgentState::Active
        );
        // Fail during active
        assert_eq!(advance(&mut sm, &[AgentEvent::Fail]), AgentState::Failed);
        // Recover resets to Idle
        assert_eq!(
            advance(&mut sm, &[AgentEvent::Recover]),
            AgentState::Idle
        );
        // Full second cycle succeeds
        assert_eq!(advance(&mut sm, &[AgentEvent::Start]), AgentState::Spawning);
        assert_eq!(
            advance(&mut sm, &[AgentEvent::Spawned]),
            AgentState::Active
        );
        assert_eq!(sm.state(), AgentState::Active);
    }

    // -----------------------------------------------------------------------
    // Illegal transitions — at least 2 per class; state must not change
    // -----------------------------------------------------------------------

    // Idle rejects everything except Start
    #[test]
    fn idle_rejects_spawned() {
        let mut sm = AgentStateMachine::new();
        let err = sm.transition(AgentEvent::Spawned).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Idle,
                event: AgentEvent::Spawned,
            }
        ));
        assert_eq!(sm.state(), AgentState::Idle);
    }

    #[test]
    fn idle_rejects_pause() {
        let mut sm = AgentStateMachine::new();
        let err = sm.transition(AgentEvent::Pause).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Idle,
                event: AgentEvent::Pause,
            }
        ));
        assert_eq!(sm.state(), AgentState::Idle);
    }

    #[test]
    fn idle_rejects_stop() {
        let mut sm = AgentStateMachine::new();
        let err = sm.transition(AgentEvent::Stop).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Idle,
                event: AgentEvent::Stop,
            }
        ));
        assert_eq!(sm.state(), AgentState::Idle);
    }

    // Active rejects events not in {Pause, Stop, Fail}
    #[test]
    fn active_rejects_start() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start, AgentEvent::Spawned]);
        let err = sm.transition(AgentEvent::Start).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Active,
                event: AgentEvent::Start,
            }
        ));
        assert_eq!(sm.state(), AgentState::Active);
    }

    #[test]
    fn active_rejects_recover() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start, AgentEvent::Spawned]);
        let err = sm.transition(AgentEvent::Recover).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Active,
                event: AgentEvent::Recover,
            }
        ));
        assert_eq!(sm.state(), AgentState::Active);
    }

    // Stopped is a terminal state — no event is accepted
    #[test]
    fn stopped_rejects_start() {
        let mut sm = AgentStateMachine::new();
        advance(
            &mut sm,
            &[
                AgentEvent::Start,
                AgentEvent::Spawned,
                AgentEvent::Stop,
                AgentEvent::Stop,
            ],
        );
        assert_eq!(sm.state(), AgentState::Stopped);
        let err = sm.transition(AgentEvent::Start).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Stopped,
                event: AgentEvent::Start,
            }
        ));
        assert_eq!(sm.state(), AgentState::Stopped);
    }

    #[test]
    fn stopped_rejects_recover() {
        let mut sm = AgentStateMachine::new();
        advance(
            &mut sm,
            &[
                AgentEvent::Start,
                AgentEvent::Spawned,
                AgentEvent::Stop,
                AgentEvent::Stop,
            ],
        );
        let err = sm.transition(AgentEvent::Recover).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Stopped,
                event: AgentEvent::Recover,
            }
        ));
        assert_eq!(sm.state(), AgentState::Stopped);
    }

    // Failed rejects everything except Recover
    #[test]
    fn failed_rejects_start() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start, AgentEvent::Fail]);
        let err = sm.transition(AgentEvent::Start).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Failed,
                event: AgentEvent::Start,
            }
        ));
        assert_eq!(sm.state(), AgentState::Failed);
    }

    #[test]
    fn failed_rejects_stop() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start, AgentEvent::Fail]);
        let err = sm.transition(AgentEvent::Stop).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Failed,
                event: AgentEvent::Stop,
            }
        ));
        assert_eq!(sm.state(), AgentState::Failed);
    }

    // Paused rejects events not in {Resume, Stop}
    #[test]
    fn paused_rejects_start() {
        let mut sm = AgentStateMachine::new();
        advance(
            &mut sm,
            &[AgentEvent::Start, AgentEvent::Spawned, AgentEvent::Pause],
        );
        let err = sm.transition(AgentEvent::Start).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Paused,
                event: AgentEvent::Start,
            }
        ));
        assert_eq!(sm.state(), AgentState::Paused);
    }

    #[test]
    fn paused_rejects_fail() {
        let mut sm = AgentStateMachine::new();
        advance(
            &mut sm,
            &[AgentEvent::Start, AgentEvent::Spawned, AgentEvent::Pause],
        );
        let err = sm.transition(AgentEvent::Fail).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Paused,
                event: AgentEvent::Fail,
            }
        ));
        assert_eq!(sm.state(), AgentState::Paused);
    }

    // Spawning rejects events not in {Spawned, Fail}
    #[test]
    fn spawning_rejects_pause() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start]);
        let err = sm.transition(AgentEvent::Pause).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Spawning,
                event: AgentEvent::Pause,
            }
        ));
        assert_eq!(sm.state(), AgentState::Spawning);
    }

    #[test]
    fn spawning_rejects_recover() {
        let mut sm = AgentStateMachine::new();
        advance(&mut sm, &[AgentEvent::Start]);
        let err = sm.transition(AgentEvent::Recover).unwrap_err();
        assert!(matches!(
            err,
            StateMachineError::InvalidTransition {
                state: AgentState::Spawning,
                event: AgentEvent::Recover,
            }
        ));
        assert_eq!(sm.state(), AgentState::Spawning);
    }

    // -----------------------------------------------------------------------
    // can_transition must agree with transition for every (state, event) pair
    // -----------------------------------------------------------------------

    #[test]
    fn can_transition_matches_transition_for_all_state_event_pairs() {
        let all_events = [
            AgentEvent::Start,
            AgentEvent::Spawned,
            AgentEvent::Pause,
            AgentEvent::Resume,
            AgentEvent::Stop,
            AgentEvent::Fail,
            AgentEvent::Recover,
        ];

        // Each entry: (setup path, expected resulting state)
        let state_paths: &[(&[AgentEvent], AgentState)] = &[
            (&[], AgentState::Idle),
            (&[AgentEvent::Start], AgentState::Spawning),
            (&[AgentEvent::Start, AgentEvent::Spawned], AgentState::Active),
            (
                &[AgentEvent::Start, AgentEvent::Spawned, AgentEvent::Pause],
                AgentState::Paused,
            ),
            (
                &[AgentEvent::Start, AgentEvent::Spawned, AgentEvent::Stop],
                AgentState::Stopping,
            ),
            (
                &[
                    AgentEvent::Start,
                    AgentEvent::Spawned,
                    AgentEvent::Stop,
                    AgentEvent::Stop,
                ],
                AgentState::Stopped,
            ),
            (&[AgentEvent::Start, AgentEvent::Fail], AgentState::Failed),
        ];

        for (path, expected_state) in state_paths {
            for &ev in &all_events {
                // can_transition probe (read-only)
                let mut sm_can = AgentStateMachine::new();
                advance(&mut sm_can, path);
                assert_eq!(
                    sm_can.state(),
                    *expected_state,
                    "setup path didn't yield expected state"
                );
                let can = sm_can.can_transition(ev);

                // transition probe (consumes state mutably)
                let mut sm_do = AgentStateMachine::new();
                advance(&mut sm_do, path);
                let result = sm_do.transition(ev);

                assert_eq!(
                    can,
                    result.is_ok(),
                    "can_transition({ev:?}) = {can} but transition({ev:?}) returned {:?} \
                     in state {expected_state:?}",
                    result
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // History recording
    // -----------------------------------------------------------------------

    #[test]
    fn history_records_each_successful_transition() {
        let mut sm = AgentStateMachine::new();
        sm.transition(AgentEvent::Start).unwrap();
        sm.transition(AgentEvent::Spawned).unwrap();

        let h = sm.history();
        assert_eq!(h.len(), 2);
        assert_eq!(
            h[0],
            (AgentState::Idle, AgentEvent::Start, AgentState::Spawning)
        );
        assert_eq!(
            h[1],
            (AgentState::Spawning, AgentEvent::Spawned, AgentState::Active)
        );
    }

    #[test]
    fn failed_transition_does_not_append_history() {
        let mut sm = AgentStateMachine::new();
        // Invalid event in Idle — history must stay empty
        let _ = sm.transition(AgentEvent::Pause);
        assert_eq!(sm.history().len(), 0);
        assert_eq!(sm.state(), AgentState::Idle);
    }
}
