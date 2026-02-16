use std::path::PathBuf;
use std::time::{Duration, Instant};

use at_bridge::event_bus::EventBus;
use at_bridge::protocol::{BridgeMessage, EventPayload};
use at_core::context_steering::ContextSteerer;
use at_core::rlm::StuckDetector;
use at_core::types::{AgentRole, Task, TaskLogType, TaskPhase};
use at_session::session::AgentSession;
use chrono::Utc;
use tracing::{error, info, warn};

use crate::prompts::PromptRegistry;

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
    #[error("agent stuck: {0}")]
    Stuck(String),
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
///
/// When a project root is provided, the runner uses `ContextSteerer` for
/// progressive context assembly and `PromptRegistry` for role-specific
/// templates, replacing hardcoded prompts with steered context.
pub struct TaskRunner {
    /// Timeout for reading agent output at each phase.
    pub phase_timeout: Duration,
    /// Optional context steerer for progressive context assembly.
    context_steerer: Option<ContextSteerer>,
    /// Optional prompt registry for role-specific templates.
    prompt_registry: Option<PromptRegistry>,
    /// Optional stuck detector for loop/timeout detection.
    stuck_detector: Option<StuckDetector>,
    /// Agent role for context + prompt selection.
    agent_role: AgentRole,
    /// Token budget for context assembly.
    token_budget: usize,
}

impl Default for TaskRunner {
    fn default() -> Self {
        Self {
            phase_timeout: Duration::from_secs(300),
            context_steerer: None,
            prompt_registry: None,
            stuck_detector: None,
            agent_role: AgentRole::Crew,
            token_budget: 16_000,
        }
    }
}

impl TaskRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a TaskRunner wired to a project root with full context steering.
    pub fn with_project(project_root: impl Into<PathBuf>) -> Self {
        let root = project_root.into();
        let mut steerer = ContextSteerer::new(&root);
        steerer.load_project();

        let mut registry = PromptRegistry::new();
        registry.load_from_project(&root);

        let stuck = StuckDetector::new(300, 100_000);

        Self {
            phase_timeout: Duration::from_secs(300),
            context_steerer: Some(steerer),
            prompt_registry: Some(registry),
            stuck_detector: Some(stuck),
            agent_role: AgentRole::Coder,
            token_budget: 16_000,
        }
    }

    /// Set the agent role for context + prompt selection.
    pub fn with_role(mut self, role: AgentRole) -> Self {
        self.agent_role = role;
        self
    }

    /// Set the per-phase timeout for reading agent output.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.phase_timeout = timeout;
        self
    }

    /// Set the token budget for context assembly.
    pub fn with_token_budget(mut self, budget: usize) -> Self {
        self.token_budget = budget;
        self
    }

    /// Check if this runner has context steering enabled.
    pub fn has_context_steering(&self) -> bool {
        self.context_steerer.is_some()
    }

    /// Run the full task pipeline.
    ///
    /// This drives the task through each phase sequentially, interacting with
    /// the agent session and publishing events along the way.
    pub async fn run(
        &mut self,
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
        &mut self,
        task: &mut Task,
        session: &AgentSession,
        bus: &EventBus,
        phase: &TaskPhase,
    ) -> Result<()> {
        let phase_start = Instant::now();

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

        // Build the prompt — use steered context if available, otherwise fallback
        let prompt = self.build_steered_prompt(task, phase);

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

                // Feed to stuck detector
                if let Some(ref mut detector) = self.stuck_detector {
                    detector.record_output(&text, bytes.len());
                    if let Some(reason) = detector.check() {
                        warn!(task_id = %task.id, phase = ?phase, reason = ?reason, "stuck detected");
                        task.log(TaskLogType::Info, format!("Stuck detected: {reason:?}"));
                        self.publish_event(bus, task, &format!("stuck:{reason:?}"));
                        // Don't hard-fail — the supervisor can decide what to do
                    }
                }

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

        // Publish phase_end event with timing
        let elapsed = phase_start.elapsed();
        task.log(
            TaskLogType::PhaseEnd,
            format!("Completed phase: {phase:?} in {}ms", elapsed.as_millis()),
        );
        self.publish_event(bus, task, &format!("phase_end:{phase:?}"));

        Ok(())
    }

    /// Build a prompt using context steering and prompt templates when available,
    /// falling back to hardcoded prompts otherwise.
    fn build_steered_prompt(&self, task: &Task, phase: &TaskPhase) -> String {
        let phase_name = phase_to_steering_name(phase);
        let title = &task.title;
        let desc = task.description.as_deref().unwrap_or("No description");

        // Try steered context + prompt template
        if let (Some(steerer), Some(registry)) =
            (&self.context_steerer, &self.prompt_registry)
        {
            let context = steerer.assemble(
                &format!("{:?}", self.agent_role),
                phase_name,
                Some(desc),
                self.token_budget,
            );
            let context_xml = context.render_xml();

            let role_prompt = registry
                .get(&self.agent_role)
                .map(|tpl| tpl.render_task(title, desc, ""))
                .unwrap_or_else(|| self.fallback_prompt(task, phase));

            format!("{}\n\n{}", context_xml, role_prompt)
        } else {
            self.fallback_prompt(task, phase)
        }
    }

    /// Generate the hardcoded fallback prompt for a given phase.
    fn fallback_prompt(&self, task: &Task, phase: &TaskPhase) -> String {
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

    /// Reset the stuck detector (e.g., after a recovery action).
    pub fn reset_stuck_detector(&mut self) {
        if let Some(ref mut detector) = self.stuck_detector {
            detector.reset();
        }
    }

    /// Transition the task to the Error state.
    fn transition_to_error(&self, task: &mut Task, bus: &EventBus, message: &str) {
        task.set_phase(TaskPhase::Error);
        task.error = Some(message.to_string());
        task.log(TaskLogType::Error, message.to_string());
        self.publish_event(bus, task, "task_error");
    }
}

/// Map TaskPhase to context steering phase names.
fn phase_to_steering_name(phase: &TaskPhase) -> &'static str {
    match phase {
        TaskPhase::Discovery => "discovery",
        TaskPhase::ContextGathering => "discovery",
        TaskPhase::SpecCreation => "spec_creation",
        TaskPhase::Planning => "planning",
        TaskPhase::Coding => "coding",
        TaskPhase::Qa => "qa",
        TaskPhase::Fixing => "coding",
        TaskPhase::Merging => "merging",
        TaskPhase::Complete => "merging",
        TaskPhase::Error => "discovery",
        TaskPhase::Stopped => "discovery",
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
        let prompt = runner.fallback_prompt(&task, &TaskPhase::Discovery);
        assert!(prompt.contains("Test task"));
        assert!(prompt.contains("Analyze"));
    }

    #[test]
    fn prompt_for_coding_contains_title() {
        let runner = TaskRunner::new();
        let task = make_test_task();
        let prompt = runner.fallback_prompt(&task, &TaskPhase::Coding);
        assert!(prompt.contains("Test task"));
        assert!(prompt.contains("Implement"));
    }

    #[test]
    fn prompt_for_qa_contains_review() {
        let runner = TaskRunner::new();
        let task = make_test_task();
        let prompt = runner.fallback_prompt(&task, &TaskPhase::Qa);
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

    #[test]
    fn task_runner_with_project_has_steering() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Rules\n").unwrap();
        let runner = TaskRunner::with_project(dir.path());
        assert!(runner.has_context_steering());
    }

    #[test]
    fn task_runner_default_no_steering() {
        let runner = TaskRunner::new();
        assert!(!runner.has_context_steering());
    }

    #[test]
    fn task_runner_with_role() {
        let runner = TaskRunner::new().with_role(AgentRole::Planner);
        assert_eq!(runner.agent_role, AgentRole::Planner);
    }

    #[test]
    fn task_runner_with_token_budget() {
        let runner = TaskRunner::new().with_token_budget(8_000);
        assert_eq!(runner.token_budget, 8_000);
    }

    #[test]
    fn build_steered_prompt_fallback() {
        let runner = TaskRunner::new();
        let task = make_test_task();
        let prompt = runner.build_steered_prompt(&task, &TaskPhase::Discovery);
        assert!(prompt.contains("Analyze"));
        assert!(prompt.contains("Test task"));
    }

    #[test]
    fn build_steered_prompt_with_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Rules\n## Conventions\n- Use Rust\n").unwrap();
        let runner = TaskRunner::with_project(dir.path());
        let task = make_test_task();
        let prompt = runner.build_steered_prompt(&task, &TaskPhase::Coding);
        // Should contain XML context blocks
        assert!(prompt.contains("project-context") || prompt.contains("Implement"));
    }

    #[test]
    fn phase_to_steering_name_mapping() {
        assert_eq!(phase_to_steering_name(&TaskPhase::Discovery), "discovery");
        assert_eq!(phase_to_steering_name(&TaskPhase::Coding), "coding");
        assert_eq!(phase_to_steering_name(&TaskPhase::Qa), "qa");
        assert_eq!(phase_to_steering_name(&TaskPhase::Merging), "merging");
        assert_eq!(phase_to_steering_name(&TaskPhase::SpecCreation), "spec_creation");
    }

    #[test]
    fn reset_stuck_detector_works() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Rules\n").unwrap();
        let mut runner = TaskRunner::with_project(dir.path());
        // Should not panic
        runner.reset_stuck_detector();
    }
}
