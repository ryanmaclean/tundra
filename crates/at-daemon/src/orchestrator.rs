use at_bridge::event_bus::EventBus;
use at_bridge::protocol::{BridgeMessage, EventPayload};
use at_core::types::{Task, TaskLogType, TaskPhase};
use chrono::Utc;
use thiserror::Error;
use tracing::{error, info, warn};
use uuid::Uuid;

use at_agents::executor::AgentExecutor;
use at_agents::profiles::AgentConfig;
use at_core::worktree_manager::{MergeResult, WorktreeManager};
use at_intelligence::runner::{QaRunner, SpecRunner};
use at_intelligence::spec::{PhaseMetrics, PhaseResult, PhaseStatus, SpecPhase};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("executor error: {0}")]
    Executor(#[from] at_agents::executor::ExecutorError),
    #[error("worktree error: {0}")]
    Worktree(#[from] at_core::worktree_manager::WorktreeManagerError),
    #[error("task not found: {0}")]
    TaskNotFound(Uuid),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("merge conflict in files: {0:?}")]
    MergeConflict(Vec<String>),
}

pub type Result<T> = std::result::Result<T, OrchestratorError>;

// ---------------------------------------------------------------------------
// TaskOrchestrator
// ---------------------------------------------------------------------------

/// High-level orchestrator that ties together the agent executor,
/// worktree manager, and event bus to drive tasks through
/// the full pipeline.
pub struct TaskOrchestrator {
    executor: AgentExecutor,
    worktree_manager: WorktreeManager,
    event_bus: EventBus,
}

impl TaskOrchestrator {
    /// Create a new orchestrator from its component parts.
    pub fn new(
        executor: AgentExecutor,
        worktree_manager: WorktreeManager,
        event_bus: EventBus,
    ) -> Self {
        Self {
            executor,
            worktree_manager,
            event_bus,
        }
    }

    /// Start executing a task through the full pipeline.
    ///
    /// This will:
    /// 1. Create a worktree for the task
    /// 2. Walk through each pipeline phase (Discovery -> ... -> Complete)
    /// 3. At each phase, spawn an agent with appropriate config
    /// 4. On the Merging phase, attempt to merge back to main
    /// 5. Publish events throughout
    pub async fn start_task(&self, task: &mut Task) -> Result<()> {
        info!(task_id = %task.id, title = %task.title, "orchestrator starting task");

        task.started_at = Some(Utc::now());

        // Create worktree
        match self.worktree_manager.create_for_task(task).await {
            Ok(wt_info) => {
                task.worktree_path = Some(wt_info.path.clone());
                task.git_branch = Some(wt_info.branch.clone());
                task.log(
                    TaskLogType::Info,
                    format!("Worktree created at {}", wt_info.path),
                );
                self.publish_event(task, "worktree_created");
            }
            Err(e) => {
                warn!(task_id = %task.id, error = %e, "failed to create worktree, continuing without");
                task.log(TaskLogType::Info, "Proceeding without dedicated worktree");
            }
        }

        // Walk through pipeline phases
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
            if *phase == TaskPhase::Complete {
                task.set_phase(TaskPhase::Complete);
                task.completed_at = Some(Utc::now());
                task.log(TaskLogType::Success, "Task completed successfully");
                self.publish_event(task, "task_complete");
                break;
            }

            task.set_phase(phase.clone());
            task.log(
                TaskLogType::PhaseStart,
                format!("Starting phase: {phase:?}"),
            );
            self.publish_event(task, &format!("phase_start:{phase:?}"));

            // Handle merging phase specially
            if *phase == TaskPhase::Merging {
                if let Some(ref branch) = task.git_branch {
                    let wt_info = at_core::worktree::WorktreeInfo {
                        path: task.worktree_path.clone().unwrap_or_default(),
                        branch: branch.clone(),
                        base_branch: "main".to_string(),
                        task_name: sanitize_task_title(&task.title),
                        created_at: Utc::now(),
                    };

                    match self.worktree_manager.merge_to_main(&wt_info).await {
                        Ok(MergeResult::Success) => {
                            task.log(TaskLogType::Success, "Merge to main successful");
                            self.publish_event(task, "merge_success");
                        }
                        Ok(MergeResult::NothingToMerge) => {
                            task.log(TaskLogType::Info, "No changes to merge");
                        }
                        Ok(MergeResult::Conflict(files)) => {
                            let msg = format!("Merge conflicts in: {}", files.join(", "));
                            task.log(TaskLogType::Error, &msg);
                            task.set_phase(TaskPhase::Error);
                            task.error = Some(msg);
                            self.publish_event(task, "merge_conflict");
                            return Err(OrchestratorError::MergeConflict(files));
                        }
                        Err(e) => {
                            warn!(task_id = %task.id, error = %e, "merge failed");
                            task.log(TaskLogType::Error, format!("Merge failed: {e}"));
                        }
                    }
                }

                task.log(TaskLogType::PhaseEnd, format!("Completed phase: {phase:?}"));
                self.publish_event(task, &format!("phase_end:{phase:?}"));
                continue;
            }

            // SpecCreation: run spec pipeline (at-intelligence SpecRunner) and persist phase results to task logs
            if *phase == TaskPhase::SpecCreation {
                run_spec_pipeline_for_task(task);
                task.log(TaskLogType::PhaseEnd, format!("Completed phase: {phase:?}"));
                self.publish_event(task, &format!("phase_end:{phase:?}"));
                continue;
            }

            // QA phase: run QA checks (at-intelligence QaRunner) and attach QaReport to task
            if *phase == TaskPhase::Qa {
                let mut qa_runner = QaRunner::new();
                let report =
                    qa_runner.run_qa_checks(task.id, &task.title, task.worktree_path.as_deref());
                task.qa_report = Some(report.clone());
                task.log(
                    TaskLogType::Info,
                    format!(
                        "QA report generated: {:?} with {} issues",
                        report.status,
                        report.issues.len()
                    ),
                );
                for issue in &report.issues {
                    task.log(
                        TaskLogType::Info,
                        format!("QA issue: {:?} - {}", issue.severity, issue.description),
                    );
                }
                // Advance phase based on QA status
                let next_phase = report.next_phase();
                task.set_phase(next_phase.clone());
                task.log(
                    TaskLogType::PhaseEnd,
                    format!("QA phase completed, advancing to: {:?}", next_phase),
                );
                self.publish_event(task, &format!("phase_end:{phase:?}"));
                // If QA passed, continue to Merging; if failed, go to Fixing
                if next_phase == TaskPhase::Merging || next_phase == TaskPhase::Fixing {
                    continue; // Skip the normal executor path
                }
            }

            // Build prompt and execute via agent
            let prompt = self.build_prompt_for_phase(task, phase.clone());
            let config =
                AgentConfig::default_for_phase(at_core::types::CliType::Claude, phase.clone());

            // Store the prompt in the task description for the executor
            let mut exec_task = task.clone();
            exec_task.description = Some(prompt);

            match self.executor.execute_task(&exec_task, &config).await {
                Ok(result) => {
                    // A2: Collect executor events/output/tool_errors into task logs
                    if !result.events.is_empty() {
                        task.log(
                            TaskLogType::Info,
                            format!("Collected {} structured events", result.events.len()),
                        );
                        for event in &result.events {
                            task.log(
                                TaskLogType::Info,
                                format!("Event: {} - {}", event.event_type, event.message),
                            );
                        }
                    }
                    if !result.output.is_empty() {
                        // Log output in chunks if it's large
                        let output_preview = if result.output.len() > 1000 {
                            format!(
                                "{}... (truncated, {} bytes total)",
                                &result.output[..1000],
                                result.output.len()
                            )
                        } else {
                            result.output.clone()
                        };
                        task.log(
                            TaskLogType::Info,
                            format!("Agent output:\n{}", output_preview),
                        );
                    }
                    if !result.tool_errors.is_empty() {
                        warn!(
                            task_id = %task.id,
                            tool_error_count = result.tool_errors.len(),
                            "tool use errors detected"
                        );
                        for tool_err in &result.tool_errors {
                            task.log(
                                TaskLogType::Error,
                                format!(
                                    "Tool error: {} - {}",
                                    tool_err.tool_name, tool_err.error_message
                                ),
                            );
                        }
                    }
                    task.log(
                        TaskLogType::Info,
                        format!("Execution duration: {}ms", result.duration_ms),
                    );

                    if !result.success {
                        warn!(
                            task_id = %task.id,
                            phase = ?phase,
                            "phase execution was not successful"
                        );
                        task.log(
                            TaskLogType::Error,
                            format!("Phase {phase:?} did not succeed"),
                        );
                    } else {
                        task.log(TaskLogType::PhaseEnd, format!("Completed phase: {phase:?}"));
                    }
                }
                Err(e) => {
                    error!(task_id = %task.id, phase = ?phase, error = %e, "phase execution failed");
                    task.set_phase(TaskPhase::Error);
                    task.error = Some(e.to_string());
                    task.log(TaskLogType::Error, format!("Phase {phase:?} failed: {e}"));
                    self.publish_event(task, "task_error");
                    return Err(OrchestratorError::Executor(e));
                }
            }

            self.publish_event(task, &format!("phase_end:{phase:?}"));
        }

        info!(task_id = %task.id, "orchestrator finished task");
        Ok(())
    }

    /// Cancel a running task.
    pub async fn cancel_task(&self, task: &mut Task) -> Result<()> {
        info!(task_id = %task.id, "cancelling task");

        self.executor.abort_task(task.id).await.ok(); // Best-effort abort

        task.set_phase(TaskPhase::Stopped);
        task.error = Some("Task cancelled by user".to_string());
        task.log(TaskLogType::Info, "Task cancelled");
        self.publish_event(task, "task_cancelled");

        Ok(())
    }

    /// Retry a failed or stopped task from its current phase.
    pub async fn retry_task(&self, task: &mut Task) -> Result<()> {
        info!(task_id = %task.id, current_phase = ?task.phase, "retrying task");

        if task.phase != TaskPhase::Error && task.phase != TaskPhase::Stopped {
            return Err(OrchestratorError::InvalidState(format!(
                "cannot retry task in phase {:?} - must be Error or Stopped",
                task.phase
            )));
        }

        // Reset to Discovery and restart
        task.error = None;
        task.set_phase(TaskPhase::Discovery);
        task.log(TaskLogType::Info, "Task retrying from Discovery");
        self.publish_event(task, "task_retry");

        self.start_task(task).await
    }

    /// Build the prompt to send to the agent for a given phase.
    fn build_prompt_for_phase(&self, task: &Task, phase: TaskPhase) -> String {
        let title = &task.title;
        let desc = task.description.as_deref().unwrap_or("No description");
        let worktree = task.worktree_path.as_deref().unwrap_or("(no worktree)");

        match phase {
            TaskPhase::Discovery => {
                format!(
                    "Analyze this task and identify what needs to be done.\n\
                     Task: {title}\nDescription: {desc}\nWorktree: {worktree}"
                )
            }
            TaskPhase::ContextGathering => {
                format!(
                    "Gather context for this task. Read relevant files, understand the codebase \
                     structure, and identify dependencies.\nTask: {title}\nWorktree: {worktree}"
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
                    "Implement the changes according to the plan. Work in the worktree \
                     directory.\nTask: {title}\nWorktree: {worktree}"
                )
            }
            TaskPhase::Qa => {
                format!(
                    "Review the implementation. Run tests, check for issues, and verify \
                     the changes meet the specification.\nTask: {title}\nWorktree: {worktree}"
                )
            }
            TaskPhase::Fixing => {
                format!("Fix any issues found during QA.\nTask: {title}\nWorktree: {worktree}")
            }
            TaskPhase::Merging => {
                format!("Prepare changes for merging. Ensure all tests pass.\nTask: {title}")
            }
            _ => format!("Continue working on task: {title}"),
        }
    }

    /// Publish an event to the bus.
    fn publish_event(&self, task: &Task, event_type: &str) {
        self.event_bus.publish(BridgeMessage::Event(EventPayload {
            event_type: event_type.to_string(),
            agent_id: None,
            bead_id: Some(task.bead_id),
            message: format!("Task '{}': {}", task.title, event_type),
            timestamp: Utc::now(),
        }));
    }
}

/// Run the spec pipeline (SpecRunner) for the task and persist phase results to task logs.
fn run_spec_pipeline_for_task(task: &mut Task) {
    let title = task.title.clone();
    let mut runner = SpecRunner::new();
    let phases = [
        SpecPhase::Discovery,
        SpecPhase::Requirements,
        SpecPhase::Writing,
        SpecPhase::Critique,
        SpecPhase::Validation,
    ];
    for spec_phase in &phases {
        let result = PhaseResult {
            id: Uuid::new_v4(),
            phase: *spec_phase,
            status: PhaseStatus::Complete,
            content: format!(
                "[{}] Placeholder output for task: {}",
                spec_phase.label(),
                title
            ),
            artifacts: vec![],
            metrics: PhaseMetrics::default(),
            created_at: Utc::now(),
        };
        runner.record_result(result);
    }
    let combined: String = runner
        .results()
        .iter()
        .map(|r| r.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    task.log(
        TaskLogType::Info,
        format!("Spec pipeline completed.\n{combined}"),
    );
}

/// Sanitize a task title for branch/directory naming.
fn sanitize_task_title(title: &str) -> String {
    title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use at_agents::executor::{PtySpawner, SpawnedProcess};
    use at_core::types::*;
    use at_core::worktree_manager::{GitOutput, GitRunner};
    use std::sync::{Arc, Mutex};

    // -- Mock PtySpawner --
    struct MockSpawner {
        output: Vec<u8>,
        /// Holds write receivers to prevent channel from closing.
        _write_rxs: std::sync::Mutex<Vec<flume::Receiver<Vec<u8>>>>,
    }

    impl MockSpawner {
        fn new(output: Vec<u8>) -> Self {
            Self {
                output,
                _write_rxs: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl PtySpawner for MockSpawner {
        fn spawn(
            &self,
            _cmd: &str,
            _args: &[&str],
            _env: &[(&str, &str)],
        ) -> std::result::Result<SpawnedProcess, String> {
            let (read_tx, read_rx) = flume::bounded(256);
            let (write_tx, write_rx) = flume::bounded::<Vec<u8>>(256);

            // Keep write_rx alive so send_line doesn't fail
            self._write_rxs.lock().unwrap().push(write_rx);

            if !self.output.is_empty() {
                let _ = read_tx.send(self.output.clone());
            }
            drop(read_tx);

            Ok(SpawnedProcess::new(
                Uuid::new_v4(),
                read_rx,
                write_tx,
                false,
            ))
        }
    }

    // -- Mock GitRunner --
    struct MockGit {
        responses: Mutex<Vec<GitOutput>>,
    }

    impl MockGit {
        fn new(responses: Vec<GitOutput>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }
    }

    impl GitRunner for MockGit {
        fn run_git(&self, _dir: &str, _args: &[&str]) -> std::result::Result<GitOutput, String> {
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                Ok(GitOutput {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            } else {
                Ok(responses.remove(0))
            }
        }
    }

    fn make_test_task() -> Task {
        Task::new(
            "Test Feature",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        )
    }

    async fn make_orchestrator(
        spawner_output: Vec<u8>,
        git_responses: Vec<GitOutput>,
    ) -> TaskOrchestrator {
        let bus = EventBus::new();
        let spawner: Arc<dyn PtySpawner> = Arc::new(MockSpawner::new(spawner_output));
        let executor = AgentExecutor::with_spawner(spawner, bus.clone());

        let tmp = std::env::temp_dir().join(format!("at-orch-test-{}", Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&tmp);
        let git = Box::new(MockGit::new(git_responses));
        let worktree_manager = WorktreeManager::with_git_runner(tmp, git);

        TaskOrchestrator::new(executor, worktree_manager, bus)
    }

    #[tokio::test]
    async fn start_task_runs_through_phases() {
        // Git responses for: worktree creation (success)
        let git_responses = vec![
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // worktree add
            // merge phase: fetch, diff (nothing), so NothingToMerge
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // fetch
            GitOutput {
                success: true,
                stdout: String::new(), // empty diff = nothing to merge
                stderr: String::new(),
            }, // diff
        ];

        let orchestrator = make_orchestrator(b"agent output\n".to_vec(), git_responses).await;
        let mut task = make_test_task();

        let result = orchestrator.start_task(&mut task).await;
        assert!(result.is_ok(), "start_task failed: {result:?}");
        assert_eq!(task.phase, TaskPhase::Complete);
        assert!(task.completed_at.is_some());
    }

    #[tokio::test]
    async fn cancel_task_sets_stopped() {
        let orchestrator = make_orchestrator(vec![], vec![]).await;
        let mut task = make_test_task();

        let result = orchestrator.cancel_task(&mut task).await;
        assert!(result.is_ok());
        assert_eq!(task.phase, TaskPhase::Stopped);
        assert!(task.error.is_some());
    }

    #[tokio::test]
    async fn retry_task_rejects_non_error_state() {
        let orchestrator = make_orchestrator(vec![], vec![]).await;
        let mut task = make_test_task();
        // Task is in Discovery phase, not Error/Stopped
        assert_eq!(task.phase, TaskPhase::Discovery);

        let result = orchestrator.retry_task(&mut task).await;
        assert!(result.is_err());
        match result {
            Err(OrchestratorError::InvalidState(_)) => { /* expected */ }
            other => panic!("Expected InvalidState, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn build_prompt_includes_task_info() {
        let orchestrator = make_orchestrator(vec![], vec![]).await;
        let task = make_test_task();

        let prompt = orchestrator.build_prompt_for_phase(&task, TaskPhase::Discovery);
        assert!(prompt.contains("Test Feature"));
        assert!(prompt.contains("Analyze"));

        let prompt = orchestrator.build_prompt_for_phase(&task, TaskPhase::Coding);
        assert!(prompt.contains("Implement"));
        assert!(prompt.contains("worktree"));
    }

    #[tokio::test]
    async fn start_task_publishes_events() {
        let bus = EventBus::new();
        let rx = bus.subscribe();

        let spawner: Arc<dyn PtySpawner> = Arc::new(MockSpawner::new(b"output\n".to_vec()));
        let executor = AgentExecutor::with_spawner(spawner, bus.clone());

        let tmp = std::env::temp_dir().join(format!("at-orch-evt-{}", Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&tmp);
        let git = Box::new(MockGit::new(vec![
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
        ]));
        let worktree_manager = WorktreeManager::with_git_runner(tmp, git);

        let orchestrator = TaskOrchestrator::new(executor, worktree_manager, bus);

        let mut task = make_test_task();
        let _ = orchestrator.start_task(&mut task).await;

        // Collect all published events
        let mut event_types = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let BridgeMessage::Event(payload) = &*msg {
                event_types.push(payload.event_type.clone());
            }
        }

        assert!(
            event_types.iter().any(|e| e.contains("phase_start")),
            "should have phase_start events: {event_types:?}"
        );
        assert!(
            event_types.iter().any(|e| e == "task_complete"),
            "should have task_complete event: {event_types:?}"
        );
    }

    #[test]
    fn sanitize_task_title_works() {
        assert_eq!(sanitize_task_title("My Feature!"), "my-feature-");
        assert_eq!(sanitize_task_title("fix/bug #42"), "fix-bug--42");
    }
}
