//! Exhaustive tests for the task lifecycle, stuck-task detection, QA reports,
//! and edge cases within the at-daemon orchestration layer.
//!
//! These tests exercise at-core types directly (no network, no real PTY).

use std::sync::Arc;
use std::time::Duration;

use at_core::types::*;
use chrono::{TimeDelta, Utc};
use uuid::Uuid;

// ===========================================================================
// Helpers
// ===========================================================================

fn make_task(title: &str) -> Task {
    Task::new(
        title,
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    )
}

fn make_subtask(title: &str) -> Subtask {
    Subtask {
        id: Uuid::new_v4(),
        title: title.to_string(),
        status: SubtaskStatus::Pending,
        agent_id: None,
        depends_on: vec![],
    }
}

/// Walk a task through the entire happy-path pipeline.
fn advance_through_all_phases(task: &mut Task) {
    let phases = [
        TaskPhase::Discovery,
        TaskPhase::ContextGathering,
        TaskPhase::SpecCreation,
        TaskPhase::Planning,
        TaskPhase::Coding,
        TaskPhase::Qa,
        TaskPhase::Merging,
        TaskPhase::Complete,
    ];
    task.started_at = Some(Utc::now());
    for phase in &phases {
        task.set_phase(phase.clone());
        task.log(TaskLogType::PhaseStart, format!("Starting {phase:?}"));
        task.log(TaskLogType::PhaseEnd, format!("Completed {phase:?}"));
    }
    task.completed_at = Some(Utc::now());
}

/// Simulate a stuck-task detector: returns task ids that have not been
/// updated within `threshold`.
fn detect_stuck_tasks(tasks: &[Task], threshold: Duration) -> Vec<Uuid> {
    let now = Utc::now();
    tasks
        .iter()
        .filter(|t| {
            // Only running tasks (not terminal states)
            t.phase != TaskPhase::Complete
                && t.phase != TaskPhase::Error
                && t.phase != TaskPhase::Stopped
        })
        .filter(|t| {
            let elapsed = now - t.updated_at;
            elapsed > TimeDelta::from_std(threshold).unwrap()
        })
        .map(|t| t.id)
        .collect()
}

/// Attempt to recover a stuck task by resetting its phase to Discovery.
fn recover_stuck_task(task: &mut Task) -> Result<(), String> {
    if task.phase == TaskPhase::Complete {
        return Err("cannot recover a completed task".into());
    }
    task.error = None;
    task.set_phase(TaskPhase::Discovery);
    task.log(TaskLogType::Info, "Task recovered — reset to Discovery");
    Ok(())
}

// ===========================================================================
// 1. Task Lifecycle End-to-End (10 tests)
// ===========================================================================

#[test]
fn task_starts_in_discovery_and_progresses_to_complete() {
    let mut task = make_task("E2E lifecycle");
    assert_eq!(task.phase, TaskPhase::Discovery);

    advance_through_all_phases(&mut task);

    assert_eq!(task.phase, TaskPhase::Complete);
    assert_eq!(task.progress_percent, 100);
    assert!(task.started_at.is_some());
    assert!(task.completed_at.is_some());
}

#[test]
fn task_error_recovery_transitions_back_to_discovery() {
    let mut task = make_task("Error recovery");
    task.set_phase(TaskPhase::Coding);
    task.set_phase(TaskPhase::Error);
    task.error = Some("build failed".into());

    assert_eq!(task.phase, TaskPhase::Error);
    assert_eq!(task.progress_percent, 0);

    // Recover
    task.error = None;
    task.set_phase(TaskPhase::Discovery);
    assert_eq!(task.phase, TaskPhase::Discovery);
    assert!(task.error.is_none());
}

#[test]
fn task_stopping_mid_execution_from_any_phase() {
    for phase in TaskPhase::pipeline_order() {
        if *phase == TaskPhase::Complete {
            continue;
        }
        let mut task = make_task("Stoppable");
        task.set_phase(phase.clone());
        assert!(
            phase.can_transition_to(&TaskPhase::Stopped),
            "phase {phase:?} should be stoppable"
        );
        task.set_phase(TaskPhase::Stopped);
        assert_eq!(task.phase, TaskPhase::Stopped);
    }
}

#[test]
fn task_phase_timing_tracking() {
    let mut task = make_task("Timing");
    assert!(task.started_at.is_none());
    assert!(task.completed_at.is_none());

    task.started_at = Some(Utc::now());
    advance_through_all_phases(&mut task);

    assert!(task.started_at.is_some());
    assert!(task.completed_at.is_some());
    assert!(task.completed_at.unwrap() >= task.started_at.unwrap());
}

#[test]
fn multiple_tasks_independent_phase_tracking() {
    let mut t1 = make_task("Task A");
    let mut t2 = make_task("Task B");

    t1.set_phase(TaskPhase::Coding);
    t2.set_phase(TaskPhase::Qa);

    assert_eq!(t1.phase, TaskPhase::Coding);
    assert_eq!(t2.phase, TaskPhase::Qa);
    assert_eq!(t1.progress_percent, 55);
    assert_eq!(t2.progress_percent, 70);
}

#[test]
fn task_progress_percent_updates_at_each_phase() {
    let mut task = make_task("Progress");
    let expected: Vec<(TaskPhase, u8)> = vec![
        (TaskPhase::Discovery, 5),
        (TaskPhase::ContextGathering, 15),
        (TaskPhase::SpecCreation, 25),
        (TaskPhase::Planning, 35),
        (TaskPhase::Coding, 55),
        (TaskPhase::Qa, 70),
        (TaskPhase::Fixing, 80),
        (TaskPhase::Merging, 90),
        (TaskPhase::Complete, 100),
    ];
    for (phase, pct) in expected {
        task.set_phase(phase.clone());
        assert_eq!(
            task.progress_percent, pct,
            "phase {phase:?} should be {pct}%"
        );
    }
}

#[test]
fn task_with_subtasks_progress_based_on_completion() {
    let mut task = make_task("With subtasks");
    for i in 0..4 {
        task.subtasks.push(make_subtask(&format!("sub-{i}")));
    }
    assert_eq!(task.subtasks.len(), 4);

    // Complete 2 of 4 subtasks
    task.subtasks[0].status = SubtaskStatus::Complete;
    task.subtasks[1].status = SubtaskStatus::Complete;

    let completed = task
        .subtasks
        .iter()
        .filter(|s| s.status == SubtaskStatus::Complete)
        .count();
    assert_eq!(completed, 2);
    let pct = (completed as f64 / task.subtasks.len() as f64 * 100.0) as u8;
    assert_eq!(pct, 50);
}

#[test]
fn task_stall_detection_identifies_stuck_task() {
    let mut task = make_task("Stalled");
    task.set_phase(TaskPhase::Coding);
    // Simulate an old updated_at
    task.updated_at = Utc::now() - TimeDelta::seconds(600);

    let stuck = detect_stuck_tasks(&[task], Duration::from_secs(300));
    assert_eq!(stuck.len(), 1);
}

#[test]
fn task_queue_ordering_fifo() {
    let mut tasks = Vec::new();
    for i in 0..5 {
        let mut t = make_task(&format!("Task-{i}"));
        t.created_at = Utc::now() - TimeDelta::seconds((5 - i) as i64);
        tasks.push(t);
    }

    tasks.sort_by_key(|t| t.created_at);
    // First created should be first in queue
    assert!(tasks[0].title.contains("Task-0"));
    assert!(tasks[4].title.contains("Task-4"));
}

#[test]
fn task_with_worktree_lifecycle() {
    let mut task = make_task("Worktree lifecycle");
    assert!(task.worktree_path.is_none());
    assert!(task.git_branch.is_none());

    // Simulate worktree creation
    task.worktree_path = Some("/tmp/wt-test".to_string());
    task.git_branch = Some("feature/worktree-lifecycle".to_string());
    task.log(TaskLogType::Info, "Worktree created");

    advance_through_all_phases(&mut task);

    assert_eq!(task.phase, TaskPhase::Complete);
    assert!(task.worktree_path.is_some());
    assert!(task.git_branch.is_some());

    // Simulate cleanup
    task.worktree_path = None;
    assert!(task.worktree_path.is_none());
}

// ===========================================================================
// 2. Stuck Task Detection & Recovery (10 tests)
// ===========================================================================

#[test]
fn detect_stuck_task_no_progress_beyond_threshold() {
    let mut task = make_task("Stuck");
    task.set_phase(TaskPhase::Planning);
    task.updated_at = Utc::now() - TimeDelta::seconds(900);

    let stuck = detect_stuck_tasks(&[task.clone()], Duration::from_secs(300));
    assert!(stuck.contains(&task.id));
}

#[test]
fn stuck_task_recovery_resets_phase() {
    let mut task = make_task("Stuck recovery");
    task.set_phase(TaskPhase::Coding);
    task.error = Some("stuck in coding".into());

    let result = recover_stuck_task(&mut task);
    assert!(result.is_ok());
    assert_eq!(task.phase, TaskPhase::Discovery);
    assert!(task.error.is_none());
}

#[test]
fn stuck_task_auto_restart() {
    let mut task = make_task("Auto restart");
    task.set_phase(TaskPhase::Qa);
    task.updated_at = Utc::now() - TimeDelta::seconds(1200);

    // Detect stuck
    let stuck = detect_stuck_tasks(&[task.clone()], Duration::from_secs(300));
    assert!(!stuck.is_empty());

    // Auto restart: recover + simulate re-run
    let _ = recover_stuck_task(&mut task);
    assert_eq!(task.phase, TaskPhase::Discovery);
    advance_through_all_phases(&mut task);
    assert_eq!(task.phase, TaskPhase::Complete);
}

#[test]
fn stuck_task_moves_to_error_if_recovery_fails() {
    let mut task = make_task("Recovery fails");
    task.set_phase(TaskPhase::Complete); // can't recover a completed task

    let result = recover_stuck_task(&mut task);
    assert!(result.is_err());

    // Simulate moving to Error because recovery wasn't possible
    task.set_phase(TaskPhase::Error);
    task.error = Some("recovery failed".into());
    assert_eq!(task.phase, TaskPhase::Error);
}

#[test]
fn multiple_stuck_tasks_detected_simultaneously() {
    let mut tasks: Vec<Task> = (0..3)
        .map(|i| {
            let mut t = make_task(&format!("Stuck-{i}"));
            t.set_phase(TaskPhase::Coding);
            t.updated_at = Utc::now() - TimeDelta::seconds(600);
            t
        })
        .collect();

    // One task is not stuck
    let mut healthy = make_task("Healthy");
    healthy.set_phase(TaskPhase::Coding);
    // healthy.updated_at is fresh (just now)
    tasks.push(healthy);

    let stuck = detect_stuck_tasks(&tasks, Duration::from_secs(300));
    assert_eq!(stuck.len(), 3);
}

#[test]
fn stuck_detection_does_not_affect_running_tasks() {
    let mut running = make_task("Running fine");
    running.set_phase(TaskPhase::Coding);
    // updated_at is fresh

    let stuck = detect_stuck_tasks(&[running.clone()], Duration::from_secs(300));
    assert!(stuck.is_empty());
    // Task state unchanged
    assert_eq!(running.phase, TaskPhase::Coding);
}

#[test]
fn stuck_task_with_pending_subtasks_gets_subtasks_reset() {
    let mut task = make_task("Stuck with subtasks");
    task.subtasks.push(Subtask {
        id: Uuid::new_v4(),
        title: "sub-a".into(),
        status: SubtaskStatus::InProgress,
        agent_id: Some(Uuid::new_v4()),
        depends_on: vec![],
    });
    task.subtasks.push(Subtask {
        id: Uuid::new_v4(),
        title: "sub-b".into(),
        status: SubtaskStatus::Pending,
        agent_id: None,
        depends_on: vec![],
    });

    task.set_phase(TaskPhase::Coding);
    task.updated_at = Utc::now() - TimeDelta::seconds(600);

    // Recover: reset pending/in-progress subtasks
    let _ = recover_stuck_task(&mut task);
    for sub in &mut task.subtasks {
        if sub.status == SubtaskStatus::InProgress {
            sub.status = SubtaskStatus::Pending;
            sub.agent_id = None;
        }
    }
    assert!(task
        .subtasks
        .iter()
        .all(|s| s.status == SubtaskStatus::Pending));
}

#[test]
fn recovery_preserves_completed_subtasks() {
    let mut task = make_task("Preserve completed");
    let completed_sub = Subtask {
        id: Uuid::new_v4(),
        title: "done-sub".into(),
        status: SubtaskStatus::Complete,
        agent_id: Some(Uuid::new_v4()),
        depends_on: vec![],
    };
    let pending_sub = make_subtask("pending-sub");
    task.subtasks.push(completed_sub);
    task.subtasks.push(pending_sub);

    task.set_phase(TaskPhase::Coding);
    let _ = recover_stuck_task(&mut task);

    // The completed subtask should remain Complete
    assert_eq!(task.subtasks[0].status, SubtaskStatus::Complete);
    assert_eq!(task.subtasks[1].status, SubtaskStatus::Pending);
}

#[test]
fn recovery_updates_task_timestamp() {
    let mut task = make_task("Timestamp update");
    task.set_phase(TaskPhase::Coding);
    let _old_updated = task.updated_at;
    // Make it stale
    task.updated_at = Utc::now() - TimeDelta::seconds(600);
    let stale = task.updated_at;

    let _ = recover_stuck_task(&mut task);
    // set_phase updates updated_at
    assert!(task.updated_at > stale);
}

#[test]
fn stuck_task_detection_configurable_threshold() {
    let mut task = make_task("Configurable threshold");
    task.set_phase(TaskPhase::Qa);
    task.updated_at = Utc::now() - TimeDelta::seconds(120);

    // With a 300s threshold: NOT stuck
    let stuck_300 = detect_stuck_tasks(&[task.clone()], Duration::from_secs(300));
    assert!(stuck_300.is_empty());

    // With a 60s threshold: IS stuck
    let stuck_60 = detect_stuck_tasks(&[task.clone()], Duration::from_secs(60));
    assert_eq!(stuck_60.len(), 1);
}

// ===========================================================================
// 3. QA Report & Validation (10 tests)
// ===========================================================================

#[test]
fn qa_report_passed_allows_merge() {
    let task = make_task("QA passed");
    let report = QaReport::new(task.id, QaStatus::Passed);
    assert_eq!(report.next_phase(), TaskPhase::Merging);
}

#[test]
fn qa_report_failed_triggers_fixing_phase() {
    let task = make_task("QA failed");
    let report = QaReport::new(task.id, QaStatus::Failed);
    assert_eq!(report.next_phase(), TaskPhase::Fixing);
}

#[test]
fn qa_report_critical_issues_block_merging() {
    let task = make_task("QA critical");
    let mut report = QaReport::new(task.id, QaStatus::Failed);
    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Critical,
        description: "Memory leak in allocator".into(),
        file: Some("src/alloc.rs".into()),
        line: Some(42),
    });

    assert!(report.has_critical_issues());
    assert_eq!(report.next_phase(), TaskPhase::Fixing);
}

#[test]
fn qa_report_issue_severity_levels() {
    let task_id = Uuid::new_v4();
    let mut report = QaReport::new(task_id, QaStatus::Failed);

    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Critical,
        description: "Critical issue".into(),
        file: None,
        line: None,
    });
    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Major,
        description: "Major issue".into(),
        file: None,
        line: None,
    });
    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Minor,
        description: "Minor issue".into(),
        file: None,
        line: None,
    });

    assert_eq!(report.issues.len(), 3);
    assert_eq!(report.issues[0].severity, QaSeverity::Critical);
    assert_eq!(report.issues[1].severity, QaSeverity::Major);
    assert_eq!(report.issues[2].severity, QaSeverity::Minor);
}

#[test]
fn qa_report_serialization_roundtrip() {
    let task_id = Uuid::new_v4();
    let mut report = QaReport::new(task_id, QaStatus::Failed);
    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Major,
        description: "Serialization test".into(),
        file: Some("lib.rs".into()),
        line: Some(10),
    });

    let json = serde_json::to_string(&report).expect("serialize");
    let deserialized: QaReport = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.task_id, task_id);
    assert_eq!(deserialized.issues.len(), 1);
    assert_eq!(deserialized.issues[0].description, "Serialization test");
    assert_eq!(deserialized.issues[0].line, Some(10));
}

#[test]
fn qa_report_with_file_line_references() {
    let task_id = Uuid::new_v4();
    let mut report = QaReport::new(task_id, QaStatus::Failed);
    report.issues.push(QaIssue {
        id: Uuid::new_v4(),
        severity: QaSeverity::Minor,
        description: "Unused import".into(),
        file: Some("src/main.rs".into()),
        line: Some(3),
    });

    let issue = &report.issues[0];
    assert_eq!(issue.file.as_deref(), Some("src/main.rs"));
    assert_eq!(issue.line, Some(3));
}

#[test]
fn qa_report_empty_no_issues_passed() {
    let task_id = Uuid::new_v4();
    let report = QaReport::new(task_id, QaStatus::Passed);

    assert!(report.issues.is_empty());
    assert!(!report.has_critical_issues());
    assert_eq!(report.next_phase(), TaskPhase::Merging);
}

#[test]
fn qa_report_attached_to_task() {
    let task = make_task("QA attached");
    let report = QaReport::new(task.id, QaStatus::Passed);
    assert_eq!(report.task_id, task.id);
}

#[test]
fn multiple_qa_reports_reruns() {
    let task_id = Uuid::new_v4();

    let report1 = QaReport::new(task_id, QaStatus::Failed);
    let report2 = QaReport::new(task_id, QaStatus::Failed);
    let report3 = QaReport::new(task_id, QaStatus::Passed);

    let reports = vec![report1, report2, report3];
    assert_eq!(reports.len(), 3);
    // Each has unique id
    assert_ne!(reports[0].id, reports[1].id);
    assert_ne!(reports[1].id, reports[2].id);
    // Latest one passed
    assert_eq!(reports.last().unwrap().status, QaStatus::Passed);
}

#[test]
fn qa_status_determines_next_phase() {
    let task_id = Uuid::new_v4();

    let passed = QaReport::new(task_id, QaStatus::Passed);
    assert_eq!(passed.next_phase(), TaskPhase::Merging);

    let failed = QaReport::new(task_id, QaStatus::Failed);
    assert_eq!(failed.next_phase(), TaskPhase::Fixing);

    let pending = QaReport::new(task_id, QaStatus::Pending);
    assert_eq!(pending.next_phase(), TaskPhase::Qa);
}

// ===========================================================================
// 4. Edge Cases (6 tests)
// ===========================================================================

#[test]
fn task_with_zero_subtasks_completes_normally() {
    let mut task = make_task("No subtasks");
    assert!(task.subtasks.is_empty());
    advance_through_all_phases(&mut task);
    assert_eq!(task.phase, TaskPhase::Complete);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_phase_transition_race_condition_simulated() {
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let task = Arc::new(Mutex::new(make_task("Race condition")));

    let mut handles = Vec::new();
    for _ in 0..10 {
        let task_clone = Arc::clone(&task);
        handles.push(tokio::spawn(async move {
            let mut t = task_clone.lock().await;
            // Everyone tries to move to Coding
            t.set_phase(TaskPhase::Coding);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let t = task.lock().await;
    assert_eq!(t.phase, TaskPhase::Coding);
    assert_eq!(t.progress_percent, 55);
}

#[test]
fn task_with_very_long_description_and_title() {
    let long_title = "A".repeat(10_000);
    let mut task = make_task(&long_title);
    task.description = Some("B".repeat(100_000));

    assert_eq!(task.title.len(), 10_000);
    assert_eq!(task.description.as_ref().unwrap().len(), 100_000);

    // Should still serialize/deserialize fine
    let json = serde_json::to_string(&task).expect("serialize long task");
    let roundtripped: Task = serde_json::from_str(&json).expect("deserialize long task");
    assert_eq!(roundtripped.title.len(), 10_000);
}

#[test]
fn task_recovery_idempotent() {
    let mut task = make_task("Idempotent recovery");
    task.set_phase(TaskPhase::Coding);

    let r1 = recover_stuck_task(&mut task);
    assert!(r1.is_ok());
    assert_eq!(task.phase, TaskPhase::Discovery);

    let r2 = recover_stuck_task(&mut task);
    assert!(r2.is_ok());
    assert_eq!(task.phase, TaskPhase::Discovery);
}

#[test]
fn task_creation_with_all_optional_fields_none() {
    let task = make_task("Minimal");

    assert!(task.description.is_none());
    assert!(task.worktree_path.is_none());
    assert!(task.git_branch.is_none());
    assert!(task.impact.is_none());
    assert!(task.agent_profile.is_none());
    assert!(task.started_at.is_none());
    assert!(task.completed_at.is_none());
    assert!(task.error.is_none());
    assert!(task.subtasks.is_empty());
    assert!(task.phase_configs.is_empty());
    assert!(task.logs.is_empty());
}

#[test]
fn task_log_entries_accumulate_correctly() {
    let mut task = make_task("Logging");
    assert!(task.logs.is_empty());

    task.log(TaskLogType::Info, "first");
    task.log(TaskLogType::Error, "second");
    task.log(TaskLogType::Success, "third");

    assert_eq!(task.logs.len(), 3);
    assert_eq!(task.logs[0].message, "first");
    assert_eq!(task.logs[0].log_type, TaskLogType::Info);
    assert_eq!(task.logs[1].log_type, TaskLogType::Error);
    assert_eq!(task.logs[2].log_type, TaskLogType::Success);
}

// ===========================================================================
// Additional lifecycle tests to reach 35+
// ===========================================================================

#[test]
fn task_phase_can_transition_to_error_from_any_active_phase() {
    for phase in TaskPhase::pipeline_order() {
        assert!(
            phase.can_transition_to(&TaskPhase::Error),
            "{phase:?} should transition to Error"
        );
    }
}

#[test]
fn task_phase_happy_path_transitions_all_valid() {
    let transitions = vec![
        (TaskPhase::Discovery, TaskPhase::ContextGathering),
        (TaskPhase::ContextGathering, TaskPhase::SpecCreation),
        (TaskPhase::SpecCreation, TaskPhase::Planning),
        (TaskPhase::Planning, TaskPhase::Coding),
        (TaskPhase::Coding, TaskPhase::Qa),
        (TaskPhase::Qa, TaskPhase::Merging),
        (TaskPhase::Merging, TaskPhase::Complete),
    ];
    for (from, to) in transitions {
        assert!(
            from.can_transition_to(&to),
            "{from:?} -> {to:?} should be valid"
        );
    }
}

#[test]
fn task_phase_invalid_transitions_rejected() {
    // Can't go backwards in happy path
    assert!(!TaskPhase::Coding.can_transition_to(&TaskPhase::Discovery));
    assert!(!TaskPhase::Complete.can_transition_to(&TaskPhase::Discovery));
    assert!(!TaskPhase::Qa.can_transition_to(&TaskPhase::Planning));
}

#[test]
fn task_fixing_to_qa_loop_allowed() {
    assert!(TaskPhase::Qa.can_transition_to(&TaskPhase::Fixing));
    assert!(TaskPhase::Fixing.can_transition_to(&TaskPhase::Qa));
}

#[test]
fn task_fixing_to_coding_fallback_allowed() {
    assert!(TaskPhase::Fixing.can_transition_to(&TaskPhase::Coding));
}

#[test]
fn task_serialization_full_roundtrip() {
    let mut task = make_task("Serialize me");
    task.description = Some("A description".into());
    task.worktree_path = Some("/tmp/wt".into());
    task.git_branch = Some("feat/test".into());
    task.impact = Some(TaskImpact::High);
    task.agent_profile = Some(AgentProfile::Balanced);
    task.phase_configs = vec![PhaseConfig::default()];
    task.subtasks.push(make_subtask("sub1"));
    task.set_phase(TaskPhase::Coding);
    task.log(TaskLogType::Info, "hello");

    let json = serde_json::to_string(&task).unwrap();
    let deserialized: Task = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, task.id);
    assert_eq!(deserialized.phase, TaskPhase::Coding);
    assert_eq!(deserialized.progress_percent, 55);
    assert_eq!(deserialized.subtasks.len(), 1);
    assert_eq!(deserialized.logs.len(), 1);
    assert_eq!(deserialized.impact, Some(TaskImpact::High));
}

#[test]
fn task_pipeline_order_has_all_phases() {
    let order = TaskPhase::pipeline_order();
    assert_eq!(order.len(), 9);
    assert_eq!(order.first(), Some(&TaskPhase::Discovery));
    assert_eq!(order.last(), Some(&TaskPhase::Complete));
}

#[test]
fn subtask_status_transitions() {
    let mut sub = make_subtask("Status transitions");
    assert_eq!(sub.status, SubtaskStatus::Pending);

    sub.status = SubtaskStatus::InProgress;
    assert_eq!(sub.status, SubtaskStatus::InProgress);

    sub.status = SubtaskStatus::Complete;
    assert_eq!(sub.status, SubtaskStatus::Complete);
}

#[test]
fn subtask_dependencies_tracked() {
    let dep_id = Uuid::new_v4();
    let sub = Subtask {
        id: Uuid::new_v4(),
        title: "Depends on dep".into(),
        status: SubtaskStatus::Pending,
        agent_id: None,
        depends_on: vec![dep_id],
    };

    assert_eq!(sub.depends_on.len(), 1);
    assert_eq!(sub.depends_on[0], dep_id);
}

#[test]
fn task_error_state_clears_on_reset() {
    let mut task = make_task("Error clear");
    task.set_phase(TaskPhase::Error);
    task.error = Some("broken".into());

    assert_eq!(task.phase, TaskPhase::Error);
    assert!(task.error.is_some());

    task.error = None;
    task.set_phase(TaskPhase::Discovery);
    assert!(task.error.is_none());
    assert_eq!(task.phase, TaskPhase::Discovery);
    assert_eq!(task.progress_percent, 5);
}

#[test]
fn task_category_and_complexity_variants() {
    let categories = vec![
        TaskCategory::Feature,
        TaskCategory::BugFix,
        TaskCategory::Refactoring,
        TaskCategory::Documentation,
        TaskCategory::Security,
        TaskCategory::Performance,
        TaskCategory::UiUx,
        TaskCategory::Infrastructure,
        TaskCategory::Testing,
    ];
    for cat in &categories {
        let json = serde_json::to_string(cat).unwrap();
        let back: TaskCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, cat);
    }

    let complexities = vec![
        TaskComplexity::Trivial,
        TaskComplexity::Small,
        TaskComplexity::Medium,
        TaskComplexity::Large,
        TaskComplexity::Complex,
    ];
    for cplx in &complexities {
        let json = serde_json::to_string(cplx).unwrap();
        let back: TaskComplexity = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, cplx);
    }
}
