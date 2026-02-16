use std::time::Duration;

use at_bridge::event_bus::EventBus;
use at_bridge::protocol::{BridgeMessage, EventPayload};
use at_core::types::{Task, TaskLogType, TaskPhase};
use at_session::session::AgentSession;
use chrono::Utc;
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum TaskRunnerError {
    #[error("task phase error: {0}")]
    PhaseError(String),
    #[error("agent session error: {0}")]
    SessionError(String),
    #[error("task was stopped")]
    Stopped,
}

pub type Result<T> = std::result::Result<T, TaskRunnerError>;

// ---------------------------------------------------------------------------
// TaskRunner
// ---------------------------------------------------------------------------

/// Orchestrates a full task pipeline through the defined phases.
///
/// The runner drives a `Task` through Discovery -> ContextGathering ->
/// SpecCreation -> Planning -> Coding -> QA -> Complete (with possible
/// Fixing/Merging detours). At each phase it publishes events to the
/// `EventBus` and communicates with the agent through the `AgentSession`.
pub struct TaskRunner {
    /// Timeout for reading agent output at each phase.
    pub phase_timeout: Duration,
}

impl Default for TaskRunner {
    fn default() -> Self {
        Self {
            phase_timeout: Duration::from_secs(300),
        }
    }
}

impl TaskRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the per-phase timeout for reading agent output.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.phase_timeout = timeout;
        self
    }

    /// Run the full task pipeline.
    ///
    /// This drives the task through each phase sequentially, interacting with
    /// the agent session and publishing events along the way.
    pub async fn run(
        &self,
        task: &mut Task,
        session: &AgentSession,
        bus: &EventBus,
    ) -> Result<()> {
        info!(task_id = %task.id, title = %task.title, "starting task pipeline");

        task.started_at = Some(Utc::now());

        let phases = vec![
            TaskPhase::Discovery,
            TaskPhase::ContextGathering,
            TaskPhase::SpecCreation,
            TaskPhase::Planning,
            TaskPhase::Coding,
            TaskPhase::Qa,
            TaskPhase::Merging,
            TaskPhase::Complete,
        ];

        for phase in &phases {
            if !session.is_alive() {
                let msg = "agent session died unexpectedly";
                error!(task_id = %task.id, msg);
                self.transition_to_error(task, bus, msg);
                return Err(TaskRunnerError::SessionError(msg.to_string()));
            }

            if let Err(e) = self.execute_phase(task, session, bus, phase).await {
                error!(task_id = %task.id, phase = ?phase, error = %e, "phase execution failed");
                self.transition_to_error(task, bus, &e.to_string());
                return Err(e);
            }

            // If the phase set Error or Stopped, bail out.
            if task.phase == TaskPhase::Error || task.phase == TaskPhase::Stopped {
                return Err(TaskRunnerError::Stopped);
            }
        }

        task.completed_at = Some(Utc::now());
        info!(task_id = %task.id, "task pipeline complete");
        Ok(())
    }

    /// Execute a single phase of the pipeline.
    async fn execute_phase(
        &self,
        task: &mut Task,
        session: &AgentSession,
        bus: &EventBus,
        phase: &TaskPhase,
    ) -> Result<()> {
        // Transition
        task.set_phase(phase.clone());
        task.log(TaskLogType::PhaseStart, format!("Starting phase: {phase:?}"));

        // Publish phase_start event
        self.publish_event(bus, task, &format!("phase_start:{phase:?}"));

        // Complete phase is terminal, nothing to send to the agent.
        if *phase == TaskPhase::Complete {
            task.log(TaskLogType::Success, "Task completed successfully");
            self.publish_event(bus, task, "task_complete");
            return Ok(());
        }

        // Build the prompt for this phase
        let prompt = self.prompt_for_phase(task, phase);

        // Send to agent
        session
            .send_command(&prompt)
            .map_err(|e| TaskRunnerError::SessionError(e.to_string()))?;

        // Read agent output with timeout
        let output = session.read_output_timeout(self.phase_timeout).await;

        let output_text = match output {
            Some(bytes) => {
                let text = String::from_utf8_lossy(&bytes).to_string();
                task.log(TaskLogType::Text, format!("Agent output ({} bytes)", bytes.len()));
                text
            }
            None => {
                warn!(task_id = %task.id, phase = ?phase, "phase timed out waiting for agent output");
                task.log(TaskLogType::Info, "Phase timed out, continuing");
                String::new()
            }
        };

        // Check for errors in output
        if let Some(status) = session.parse_status(&output_text) {
            if status == "error" {
                task.log(TaskLogType::Error, format!("Agent reported error in phase {phase:?}"));
                return Err(TaskRunnerError::PhaseError(format!(
                    "Agent error in phase {phase:?}"
                )));
            }
        }

        // Publish phase_end event
        task.log(TaskLogType::PhaseEnd, format!("Completed phase: {phase:?}"));
        self.publish_event(bus, task, &format!("phase_end:{phase:?}"));

        Ok(())
    }

    /// Generate the prompt to send to the agent for a given phase.
    fn prompt_for_phase(&self, task: &Task, phase: &TaskPhase) -> String {
        let title = &task.title;
        let desc = task.description.as_deref().unwrap_or("No description");

        match phase {
            TaskPhase::Discovery => {
                format!(
                    "Analyze this task and identify what needs to be done.\n\
                     Task: {title}\nDescription: {desc}"
                )
            }
            TaskPhase::ContextGathering => {
                format!(
                    "Gather context for this task. Read relevant files, understand the codebase \
                     structure, and identify dependencies.\nTask: {title}"
                )
            }
            TaskPhase::SpecCreation => {
                format!(
                    "Create a specification for this task. Define acceptance criteria, \
                     interfaces, and expected behavior.\nTask: {title}"
                )
            }
            TaskPhase::Planning => {
                format!(
                    "Plan the implementation. Break down into steps, identify files to modify, \
                     and outline the approach.\nTask: {title}"
                )
            }
            TaskPhase::Coding => {
                format!(
                    "Implement the changes according to the plan.\nTask: {title}"
                )
            }
            TaskPhase::Qa => {
                format!(
                    "Review the implementation. Run tests, check for issues, and verify \
                     the changes meet the specification.\nTask: {title}"
                )
            }
            TaskPhase::Fixing => {
                format!(
                    "Fix any issues found during QA.\nTask: {title}"
                )
            }
            TaskPhase::Merging => {
                format!(
                    "Prepare changes for merging. Ensure all tests pass and the branch is \
                     ready.\nTask: {title}"
                )
            }
            _ => format!("Continue working on task: {title}"),
        }
    }

    /// Publish an event to the bus.
    fn publish_event(&self, bus: &EventBus, task: &Task, event_type: &str) {
        bus.publish(BridgeMessage::Event(EventPayload {
            event_type: event_type.to_string(),
            agent_id: None,
            bead_id: Some(task.bead_id),
            message: format!("Task '{}': {}", task.title, event_type),
            timestamp: Utc::now(),
        }));
    }

    /// Transition the task to the Error state.
    fn transition_to_error(&self, task: &mut Task, bus: &EventBus, message: &str) {
        task.set_phase(TaskPhase::Error);
        task.error = Some(message.to_string());
        task.log(TaskLogType::Error, message.to_string());
        self.publish_event(bus, task, "task_error");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::types::*;
    use uuid::Uuid;

    fn make_test_task() -> Task {
        Task::new(
            "Test task",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        )
    }

    #[test]
    fn task_runner_default_timeout() {
        let runner = TaskRunner::new();
        assert_eq!(runner.phase_timeout, Duration::from_secs(300));
    }

    #[test]
    fn task_runner_custom_timeout() {
        let runner = TaskRunner::new().with_timeout(Duration::from_secs(60));
        assert_eq!(runner.phase_timeout, Duration::from_secs(60));
    }

    #[test]
    fn prompt_for_discovery_contains_title() {
        let runner = TaskRunner::new();
        let task = make_test_task();
        let prompt = runner.prompt_for_phase(&task, &TaskPhase::Discovery);
        assert!(prompt.contains("Test task"));
        assert!(prompt.contains("Analyze"));
    }

    #[test]
    fn prompt_for_coding_contains_title() {
        let runner = TaskRunner::new();
        let task = make_test_task();
        let prompt = runner.prompt_for_phase(&task, &TaskPhase::Coding);
        assert!(prompt.contains("Test task"));
        assert!(prompt.contains("Implement"));
    }

    #[test]
    fn prompt_for_qa_contains_review() {
        let runner = TaskRunner::new();
        let task = make_test_task();
        let prompt = runner.prompt_for_phase(&task, &TaskPhase::Qa);
        assert!(prompt.contains("Review"));
    }

    #[test]
    fn transition_to_error_sets_state() {
        let runner = TaskRunner::new();
        let bus = EventBus::new();
        let rx = bus.subscribe();
        let mut task = make_test_task();

        runner.transition_to_error(&mut task, &bus, "something broke");

        assert_eq!(task.phase, TaskPhase::Error);
        assert_eq!(task.error.as_deref(), Some("something broke"));
        assert_eq!(task.progress_percent, 0);

        // Should have published an event
        let msg = rx.try_recv().expect("should have event");
        match msg {
            BridgeMessage::Event(payload) => {
                assert_eq!(payload.event_type, "task_error");
                assert!(payload.message.contains("Test task"));
            }
            _ => panic!("Expected Event message"),
        }
    }

    #[test]
    fn phase_events_published() {
        let runner = TaskRunner::new();
        let bus = EventBus::new();
        let rx = bus.subscribe();
        let task = make_test_task();

        runner.publish_event(&bus, &task, "phase_start:Discovery");

        let msg = rx.try_recv().expect("should have event");
        match msg {
            BridgeMessage::Event(payload) => {
                assert_eq!(payload.event_type, "phase_start:Discovery");
                assert_eq!(payload.bead_id, Some(task.bead_id));
            }
            _ => panic!("Expected Event message"),
        }
    }
}
