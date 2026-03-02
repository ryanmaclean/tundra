//! TaskOrchestrator -- wires the coding -> QA -> fix loop into a unified
//! pipeline that can be triggered from the HTTP API.
//!
//! This module connects the `AgentExecutor` (PTY spawning), the `QaRunner`
//! (QA checks), and the `EventBus` (real-time notifications) into a cohesive
//! execution pipeline for the Coding, QA, and QA-fix phases.

use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::protocol::{BridgeMessage, EventPayload};
use at_core::types::{CliType, QaReport, QaStatus, Subtask, Task, TaskPhase};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};
use uuid::Uuid;

use crate::executor::{AgentExecutor, PtySpawner};
use crate::profiles::AgentConfig;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("executor error: {0}")]
    Executor(#[from] crate::executor::ExecutorError),
    #[error("qa failed after {0} fix iterations")]
    QaExhausted(usize),
    #[error("task has no worktree path")]
    NoWorktree,
    #[error("task is in invalid phase for pipeline: {0:?}")]
    InvalidPhase(TaskPhase),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, PipelineError>;

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Result of the coding phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodingResult {
    pub task_id: Uuid,
    pub success: bool,
    pub output: String,
    pub duration_ms: u64,
    pub subtasks_completed: usize,
}

/// Result of a single QA fix iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaFixResult {
    pub task_id: Uuid,
    pub passed: bool,
    pub iterations_used: usize,
    pub final_report: QaReport,
}

/// Result of the full pipeline execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    pub task_id: Uuid,
    pub coding_result: CodingResult,
    pub qa_fix_result: QaFixResult,
    pub total_duration_ms: u64,
}

// ---------------------------------------------------------------------------
// TaskOrchestrator
// ---------------------------------------------------------------------------

/// Orchestrates the coding -> QA -> fix loop for a task.
///
/// Uses `AgentExecutor` to spawn CLI agents, runs QA checks via `QaRunner`,
/// and iterates the fix loop until QA passes or the iteration budget is
/// exhausted. All phase transitions are published to the `EventBus`.
pub struct TaskOrchestrator {
    executor: AgentExecutor,
    event_bus: EventBus,
    /// Maximum QA fix iterations (default: 3).
    pub max_fix_iterations: usize,
    /// Which CLI tool to use for spawning agents (default: Claude).
    pub cli_type: CliType,
    /// When true, agents work in repo root instead of worktrees.
    pub direct_mode: bool,
}

impl TaskOrchestrator {
    /// Create a new orchestrator backed by a real PTY pool.
    pub fn new(pty_pool: Arc<at_session::pty_pool::PtyPool>, event_bus: EventBus) -> Self {
        Self {
            executor: AgentExecutor::new(pty_pool, event_bus.clone()),
            event_bus,
            max_fix_iterations: 3,
            cli_type: CliType::Claude,
            direct_mode: false,
        }
    }

    /// Create an orchestrator with a custom spawner (useful for testing).
    pub fn with_spawner(spawner: Arc<dyn PtySpawner>, event_bus: EventBus) -> Self {
        Self {
            executor: AgentExecutor::with_spawner(spawner, event_bus.clone()),
            event_bus,
            max_fix_iterations: 3,
            cli_type: CliType::Claude,
            direct_mode: false,
        }
    }

    /// Set the CLI type for agent spawning.
    pub fn with_cli_type(mut self, cli_type: CliType) -> Self {
        self.cli_type = cli_type;
        self
    }

    /// Enable or disable direct mode (work in repo root instead of worktrees).
    pub fn with_direct_mode(mut self, direct_mode: bool) -> Self {
        self.direct_mode = direct_mode;
        self
    }

    // -----------------------------------------------------------------------
    // Phase: Coding
    // -----------------------------------------------------------------------

    /// Run the coding phase by spawning an agent for each subtask (or the
    /// whole task if no subtasks are provided).
    pub async fn run_coding_phase(
        &self,
        task: &Task,
        subtasks: Vec<Subtask>,
    ) -> Result<CodingResult> {
        let start = std::time::Instant::now();

        self.emit_phase_event(task, "coding_phase_start");

        let config = AgentConfig::default_for_phase(self.cli_type.clone(), TaskPhase::Coding);

        // If subtasks are provided, execute them sequentially.
        // Otherwise execute the task itself.
        let mut combined_output = String::new();
        let mut all_success = true;
        let mut completed = 0usize;

        if subtasks.is_empty() {
            let result = self.executor.execute_task(task, &config).await?;
            all_success = result.success;
            combined_output.push_str(&result.output);
            if result.success {
                completed = 1;
            }
        } else {
            for st in &subtasks {
                // Build a temporary task variant for each subtask.
                let mut sub_task = task.clone();
                sub_task.title = st.title.clone();
                sub_task.description = Some(format!("Subtask of '{}': {}", task.title, st.title));

                let result = self.executor.execute_task(&sub_task, &config).await?;
                combined_output.push_str(&format!("\n--- Subtask: {} ---\n", st.title));
                combined_output.push_str(&result.output);

                if result.success {
                    completed += 1;
                } else {
                    all_success = false;
                    warn!(
                        task_id = %task.id,
                        subtask = %st.title,
                        "subtask coding failed"
                    );
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        self.emit_phase_event(task, "coding_phase_complete");

        Ok(CodingResult {
            task_id: task.id,
            success: all_success,
            output: combined_output,
            duration_ms,
            subtasks_completed: completed,
        })
    }

    // -----------------------------------------------------------------------
    // Phase: QA
    // -----------------------------------------------------------------------

    /// Run QA checks on the task's worktree, returning a `QaReport`.
    pub async fn run_qa_phase(&self, task: &Task, worktree_path: &str) -> Result<QaReport> {
        self.emit_phase_event(task, "qa_phase_start");

        let mut qa_runner = at_intelligence::runner::QaRunner::new();
        let report = qa_runner.run_qa_checks(task.id, &task.title, Some(worktree_path));

        info!(
            task_id = %task.id,
            status = ?report.status,
            issues = report.issues.len(),
            "QA phase completed"
        );

        self.emit_phase_event(task, "qa_phase_complete");

        Ok(report)
    }

    // -----------------------------------------------------------------------
    // Phase: QA Fix Loop
    // -----------------------------------------------------------------------

    /// Iterate: run QA, if it fails spawn a fixer agent, re-run QA.
    /// Stops when QA passes or `max_iterations` is reached.
    pub async fn run_qa_fix_loop(
        &self,
        task: &Task,
        initial_report: QaReport,
        max_iterations: usize,
    ) -> Result<QaFixResult> {
        let mut report = initial_report;
        let mut iterations = 0usize;

        let worktree = task.worktree_path.as_deref().unwrap_or(".");

        while report.status != QaStatus::Passed && iterations < max_iterations {
            iterations += 1;
            info!(
                task_id = %task.id,
                iteration = iterations,
                issues = report.issues.len(),
                "starting QA fix iteration"
            );

            self.emit_phase_event(task, &format!("qa_fix_iteration_{}", iterations));

            // Build a fix prompt that includes QA issues
            let fix_config =
                AgentConfig::default_for_phase(self.cli_type.clone(), TaskPhase::Fixing);
            let mut fix_task = task.clone();
            fix_task.description = Some(format!(
                "Fix QA issues for task '{}':\n{}",
                task.title,
                format_qa_issues(&report),
            ));

            let fix_result = self.executor.execute_task(&fix_task, &fix_config).await?;

            if !fix_result.success {
                warn!(
                    task_id = %task.id,
                    iteration = iterations,
                    "fix agent failed"
                );
            }

            // Re-run QA
            let mut qa_runner = at_intelligence::runner::QaRunner::new();
            report = qa_runner.run_qa_checks(task.id, &task.title, Some(worktree));
        }

        let passed = report.status == QaStatus::Passed;
        if !passed {
            warn!(
                task_id = %task.id,
                iterations = iterations,
                "QA fix loop exhausted without passing"
            );
        }

        Ok(QaFixResult {
            task_id: task.id,
            passed,
            iterations_used: iterations,
            final_report: report,
        })
    }

    // -----------------------------------------------------------------------
    // Full pipeline
    // -----------------------------------------------------------------------

    /// Execute the full coding -> QA -> fix pipeline.
    ///
    /// 1. Coding phase: spawn agent(s) for the task/subtasks
    /// 2. QA phase: run checks on the worktree
    /// 3. QA fix loop: iterate fix->QA until pass or budget exhausted
    /// 4. Emit final result events
    pub async fn execute_full_pipeline(&self, task: &Task) -> Result<PipelineResult> {
        let pipeline_start = std::time::Instant::now();

        self.emit_phase_event(task, "pipeline_start");

        // 1. Coding
        let coding_result = self.run_coding_phase(task, task.subtasks.clone()).await?;

        if !coding_result.success {
            warn!(task_id = %task.id, "coding phase failed; proceeding to QA anyway");
        }

        // 2. QA — in direct mode, always use repo root (".")
        let worktree = if self.direct_mode {
            "."
        } else {
            task.worktree_path.as_deref().unwrap_or(".")
        };
        let qa_report = self.run_qa_phase(task, worktree).await?;

        // 3. QA fix loop (only if QA did not pass)
        let qa_fix_result = if qa_report.status == QaStatus::Passed {
            QaFixResult {
                task_id: task.id,
                passed: true,
                iterations_used: 0,
                final_report: qa_report,
            }
        } else {
            self.run_qa_fix_loop(task, qa_report, self.max_fix_iterations)
                .await?
        };

        let total_duration_ms = pipeline_start.elapsed().as_millis() as u64;

        let event_type = if qa_fix_result.passed {
            "pipeline_complete"
        } else {
            "pipeline_complete_with_failures"
        };
        self.emit_phase_event(task, event_type);

        info!(
            task_id = %task.id,
            passed = qa_fix_result.passed,
            fix_iterations = qa_fix_result.iterations_used,
            total_ms = total_duration_ms,
            "pipeline finished"
        );

        Ok(PipelineResult {
            task_id: task.id,
            coding_result,
            qa_fix_result,
            total_duration_ms,
        })
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn emit_phase_event(&self, task: &Task, event_type: &str) {
        self.event_bus.publish(BridgeMessage::Event(EventPayload {
            event_type: event_type.to_string(),
            agent_id: None,
            bead_id: Some(task.bead_id),
            message: format!("Task '{}': {}", task.title, event_type),
            timestamp: Utc::now(),
        }));
    }
}

/// Format QA issues into a human-readable string for the fix agent prompt.
fn format_qa_issues(report: &QaReport) -> String {
    let mut out = String::new();
    for issue in &report.issues {
        out.push_str(&format!("- [{:?}] {}", issue.severity, issue.description));
        if let Some(ref file) = issue.file {
            out.push_str(&format!(" (file: {}", file));
            if let Some(line) = issue.line {
                out.push_str(&format!(", line: {}", line));
            }
            out.push(')');
        }
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::SpawnedProcess;
    use at_core::types::*;

    // -- Mock spawner --

    struct MockSpawner {
        output_chunks: Vec<Vec<u8>>,
        _write_rxs: std::sync::Mutex<Vec<flume::Receiver<Vec<u8>>>>,
    }

    impl MockSpawner {
        fn new(output_chunks: Vec<Vec<u8>>) -> Self {
            Self {
                output_chunks,
                _write_rxs: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn success(output: &str) -> Self {
            Self::new(vec![output.as_bytes().to_vec()])
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

            self._write_rxs.lock().unwrap().push(write_rx);

            for chunk in &self.output_chunks {
                let _ = read_tx.send(chunk.clone());
            }
            drop(read_tx);

            Ok(SpawnedProcess::new(
                Uuid::new_v4(),
                read_rx,
                write_tx,
                false, // not alive => reads drain immediately
            ))
        }
    }

    fn make_task() -> Task {
        let mut task = Task::new(
            "Implement feature X",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        );
        task.worktree_path = Some("/tmp/test-worktree".to_string());
        task
    }

    #[tokio::test]
    async fn coding_phase_produces_result() {
        let spawner = Arc::new(MockSpawner::success("code written successfully\n"));
        let bus = EventBus::new();
        let orch = TaskOrchestrator::with_spawner(spawner, bus);

        let task = make_task();
        let result = orch.run_coding_phase(&task, vec![]).await.unwrap();

        assert_eq!(result.task_id, task.id);
        assert!(result.success);
        assert!(result.output.contains("code written"));
    }

    #[tokio::test]
    async fn coding_phase_with_subtasks() {
        let spawner = Arc::new(MockSpawner::success("subtask done\n"));
        let bus = EventBus::new();
        let orch = TaskOrchestrator::with_spawner(spawner, bus);

        let task = make_task();
        let subtasks = vec![
            Subtask {
                id: Uuid::new_v4(),
                title: "Sub A".into(),
                status: SubtaskStatus::Pending,
                agent_id: None,
                depends_on: vec![],
            },
            Subtask {
                id: Uuid::new_v4(),
                title: "Sub B".into(),
                status: SubtaskStatus::Pending,
                agent_id: None,
                depends_on: vec![],
            },
        ];

        let result = orch.run_coding_phase(&task, subtasks).await.unwrap();
        assert_eq!(result.subtasks_completed, 2);
        assert!(result.output.contains("Sub A"));
        assert!(result.output.contains("Sub B"));
    }

    #[tokio::test]
    async fn qa_phase_returns_report() {
        let spawner = Arc::new(MockSpawner::success("ok\n"));
        let bus = EventBus::new();
        let orch = TaskOrchestrator::with_spawner(spawner, bus);

        let task = make_task();
        let report = orch.run_qa_phase(&task, "/tmp/wt").await.unwrap();

        assert_eq!(report.task_id, task.id);
        // The QaRunner placeholder returns a report with a Minor issue for the worktree
        assert!(!report.issues.is_empty());
    }

    #[tokio::test]
    async fn qa_fix_loop_terminates_at_max_iterations() {
        let spawner = Arc::new(MockSpawner::success("fix attempted\n"));
        let bus = EventBus::new();
        let orch = TaskOrchestrator::with_spawner(spawner, bus);

        let task = make_task();

        // Create a failing report
        let mut report = QaReport::new(task.id, QaStatus::Failed);
        report.issues.push(QaIssue {
            id: Uuid::new_v4(),
            severity: QaSeverity::Critical,
            description: "critical bug".into(),
            file: Some("main.rs".into()),
            line: Some(42),
        });

        // The QaRunner placeholder will return Pending (not Failed) for non-critical-heavy reports
        // after fix iterations, so the loop should still terminate at max iterations.
        let result = orch.run_qa_fix_loop(&task, report, 2).await.unwrap();

        // Should have used exactly 2 iterations (the max)
        // OR passed earlier if the placeholder QA happened to pass.
        assert!(result.iterations_used <= 2);
    }

    #[tokio::test]
    async fn full_pipeline_emits_events() {
        let spawner = Arc::new(MockSpawner::success("pipeline output\n"));
        let bus = EventBus::new();
        let rx = bus.subscribe();
        let orch = TaskOrchestrator::with_spawner(spawner, bus);

        let task = make_task();
        let result = orch.execute_full_pipeline(&task).await.unwrap();

        assert_eq!(result.task_id, task.id);
        // total_duration_ms is unsigned, so always non-negative

        // Check that the event bus received phase events
        let mut events = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let BridgeMessage::Event(payload) = &*msg {
                events.push(payload.event_type.clone());
            }
        }

        assert!(
            events.iter().any(|e| e == "pipeline_start"),
            "expected pipeline_start event, got: {:?}",
            events
        );
        assert!(
            events.iter().any(|e| e == "coding_phase_start"),
            "expected coding_phase_start event, got: {:?}",
            events
        );
        assert!(
            events.iter().any(|e| e == "qa_phase_start"),
            "expected qa_phase_start event, got: {:?}",
            events
        );
        assert!(
            events.iter().any(|e| e.starts_with("pipeline_complete")),
            "expected pipeline_complete event, got: {:?}",
            events
        );
    }

    #[tokio::test]
    async fn pipeline_result_serializable() {
        let result = PipelineResult {
            task_id: Uuid::new_v4(),
            coding_result: CodingResult {
                task_id: Uuid::new_v4(),
                success: true,
                output: "done".into(),
                duration_ms: 100,
                subtasks_completed: 1,
            },
            qa_fix_result: QaFixResult {
                task_id: Uuid::new_v4(),
                passed: true,
                iterations_used: 0,
                final_report: QaReport::new(Uuid::new_v4(), QaStatus::Passed),
            },
            total_duration_ms: 200,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deser: PipelineResult = serde_json::from_str(&json).unwrap();
        assert!(deser.qa_fix_result.passed);
    }
}
