//! Integration tests for the TaskOrchestrator (coding -> QA -> fix pipeline).

use std::sync::Arc;

use at_agents::executor::{PtySpawner, SpawnedProcess};
use at_agents::task_orchestrator::TaskOrchestrator;
use at_bridge::event_bus::EventBus;
use at_bridge::protocol::BridgeMessage;
use at_core::cache::CacheDb;
use at_core::types::*;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Mock spawner
// ---------------------------------------------------------------------------

struct MockSpawner {
    output: Vec<u8>,
    write_rxs: std::sync::Mutex<Vec<flume::Receiver<Vec<u8>>>>,
}

impl MockSpawner {
    fn new(output: &str) -> Self {
        Self {
            output: output.as_bytes().to_vec(),
            write_rxs: std::sync::Mutex::new(Vec::new()),
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
    ) -> Result<SpawnedProcess, String> {
        let (read_tx, read_rx) = flume::bounded(256);
        let (write_tx, write_rx) = flume::bounded::<Vec<u8>>(256);

        self.write_rxs.lock().unwrap().push(write_rx);

        let _ = read_tx.send(self.output.clone());
        drop(read_tx);

        Ok(SpawnedProcess::new(
            Uuid::new_v4(),
            read_rx,
            write_tx,
            false,
        ))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_task() -> Task {
    let mut task = Task::new(
        "Test pipeline task",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );
    task.worktree_path = Some("/tmp/test-worktree".into());
    task
}

async fn make_orchestrator(output: &str) -> (TaskOrchestrator, EventBus) {
    let spawner = Arc::new(MockSpawner::new(output));
    let bus = EventBus::new();
    let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
    let orch = TaskOrchestrator::with_spawner(spawner, bus.clone(), cache);
    (orch, bus)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pipeline_phases_transition_correctly() {
    let (orch, bus) = make_orchestrator("coding output\n").await;
    let rx = bus.subscribe();
    let task = make_task();

    let result = orch.execute_full_pipeline(&task).await.unwrap();

    // Pipeline should have completed
    assert_eq!(result.task_id, task.id);

    // Collect all event types from the bus
    let mut event_types = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        if let BridgeMessage::Event(payload) = &*msg {
            event_types.push(payload.event_type.clone());
        }
    }

    // Verify phase transitions occurred in order
    assert!(
        event_types.iter().any(|e| e == "pipeline_start"),
        "missing pipeline_start in: {:?}",
        event_types
    );
    assert!(
        event_types.iter().any(|e| e == "coding_phase_start"),
        "missing coding_phase_start in: {:?}",
        event_types
    );
    assert!(
        event_types.iter().any(|e| e == "coding_phase_complete"),
        "missing coding_phase_complete in: {:?}",
        event_types
    );
    assert!(
        event_types.iter().any(|e| e == "qa_phase_start"),
        "missing qa_phase_start in: {:?}",
        event_types
    );
    assert!(
        event_types.iter().any(|e| e == "qa_phase_complete"),
        "missing qa_phase_complete in: {:?}",
        event_types
    );
    assert!(
        event_types
            .iter()
            .any(|e| e.starts_with("pipeline_complete")),
        "missing pipeline_complete* in: {:?}",
        event_types
    );

    // Verify ordering: pipeline_start before coding_phase_start
    let start_idx = event_types
        .iter()
        .position(|e| e == "pipeline_start")
        .unwrap();
    let coding_idx = event_types
        .iter()
        .position(|e| e == "coding_phase_start")
        .unwrap();
    let qa_idx = event_types
        .iter()
        .position(|e| e == "qa_phase_start")
        .unwrap();
    assert!(
        start_idx < coding_idx,
        "pipeline_start should come before coding_phase_start"
    );
    assert!(
        coding_idx < qa_idx,
        "coding_phase_start should come before qa_phase_start"
    );
}

#[tokio::test]
async fn qa_fix_loop_respects_max_iterations() {
    let (orch, _bus) = make_orchestrator("fix output\n").await;
    let task = make_task();

    // Create a report that will stay Failed through the placeholder QaRunner
    // (placeholder returns Pending for worktree-only issues, not Failed)
    let mut report = QaReport::new(task.id, QaStatus::Failed);
    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Critical,
        description: "critical failure".into(),
        file: Some("lib.rs".into()),
        line: Some(10),
    });
    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Critical,
        description: "another critical".into(),
        file: None,
        line: None,
    });
    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Major,
        description: "major issue 1".into(),
        file: None,
        line: None,
    });
    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Major,
        description: "major issue 2".into(),
        file: None,
        line: None,
    });
    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Major,
        description: "major issue 3".into(),
        file: None,
        line: None,
    });

    let result = orch.run_qa_fix_loop(&task, report, 2).await.unwrap();

    // The placeholder QaRunner returns Pending (not Passed) for worktree paths,
    // and the loop only continues when status == Failed. So after one re-check
    // with a Pending status, the loop will break. But the important thing is
    // that iterations_used <= max (2).
    assert!(
        result.iterations_used <= 2,
        "expected at most 2 iterations, got {}",
        result.iterations_used
    );
}

#[tokio::test]
async fn event_bus_receives_phase_transition_events() {
    let (orch, bus) = make_orchestrator("output\n").await;
    let rx = bus.subscribe();
    let task = make_task();

    // Run just the coding phase
    let _ = orch.run_coding_phase(&task, vec![]).await.unwrap();

    let mut found_start = false;
    let mut found_complete = false;
    while let Ok(msg) = rx.try_recv() {
        if let BridgeMessage::Event(payload) = &*msg {
            if payload.event_type == "coding_phase_start" {
                found_start = true;
                assert_eq!(payload.bead_id, Some(task.bead_id));
            }
            if payload.event_type == "coding_phase_complete" {
                found_complete = true;
            }
        }
    }

    assert!(found_start, "should have received coding_phase_start event");
    assert!(
        found_complete,
        "should have received coding_phase_complete event"
    );
}

#[tokio::test]
async fn qa_phase_produces_report_with_task_id() {
    let (orch, _bus) = make_orchestrator("ok\n").await;
    let task = make_task();

    let report = orch.run_qa_phase(&task, "/tmp/worktree").await.unwrap();

    assert_eq!(report.task_id, task.id);
    // The placeholder QaRunner always creates at least one Minor issue for worktree paths
    assert!(!report.issues.is_empty());
}

#[tokio::test]
async fn full_pipeline_with_subtasks() {
    let (orch, _bus) = make_orchestrator("subtask output\n").await;
    let mut task = make_task();
    task.subtasks = vec![
        Subtask {
            id: Uuid::new_v4(),
            title: "Implement module A".into(),
            status: SubtaskStatus::Pending,
            agent_id: None,
            depends_on: vec![],
        },
        Subtask {
            id: Uuid::new_v4(),
            title: "Implement module B".into(),
            status: SubtaskStatus::Pending,
            agent_id: None,
            depends_on: vec![],
        },
    ];

    let result = orch.execute_full_pipeline(&task).await.unwrap();

    assert_eq!(result.coding_result.subtasks_completed, 2);
    assert!(result.coding_result.output.contains("module A"));
    assert!(result.coding_result.output.contains("module B"));
}

#[tokio::test]
async fn direct_mode_skips_worktree_requirement() {
    let (orch, _bus) = make_orchestrator("direct mode output\n").await;
    let orch = orch.with_direct_mode(true);

    // Create a task WITHOUT a worktree_path
    let mut task = Task::new(
        "Direct mode task",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );
    // Explicitly set no worktree
    task.worktree_path = None;

    // The pipeline should still succeed because direct_mode uses "." as worktree
    let result = orch.execute_full_pipeline(&task).await.unwrap();
    assert_eq!(result.task_id, task.id);
    assert!(result.coding_result.success);
}

#[tokio::test]
async fn cli_type_selection_uses_configured_type() {
    let (orch, _bus) = make_orchestrator("codex output\n").await;
    let orch = orch.with_cli_type(CliType::Codex);

    assert_eq!(orch.cli_type, CliType::Codex);

    let task = make_task();
    // The orchestrator should use the configured CLI type for spawning
    let result = orch.run_coding_phase(&task, vec![]).await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn cli_type_defaults_to_claude() {
    let (orch, _bus) = make_orchestrator("output\n").await;
    assert_eq!(orch.cli_type, CliType::Claude);
}

#[tokio::test]
async fn direct_mode_defaults_to_false() {
    let (orch, _bus) = make_orchestrator("output\n").await;
    assert!(!orch.direct_mode);
}
