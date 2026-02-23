use at_core::types::*;
use chrono::Utc;
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
        TaskComplexity::Medium,
    )
}

fn make_subtask(title: &str) -> Subtask {
    Subtask {
        id: Uuid::new_v4(),
        title: title.to_string(),
        status: SubtaskStatus::Pending,
        agent_id: None,
        depends_on: Vec::new(),
    }
}

fn make_log_entry(
    phase: TaskPhase,
    log_type: TaskLogType,
    message: &str,
    detail: Option<&str>,
) -> TaskLogEntry {
    TaskLogEntry {
        timestamp: Utc::now(),
        phase,
        log_type,
        message: message.to_string(),
        detail: detail.map(|s| s.to_string()),
    }
}

// ===========================================================================
// 1. Task Overview (15 tests)
// ===========================================================================

#[test]
fn task_creation_all_fields_populated() {
    let bead_id = Uuid::new_v4();
    let task = Task::new(
        "Implement login",
        bead_id,
        TaskCategory::Feature,
        TaskPriority::High,
        TaskComplexity::Large,
    );
    assert_eq!(task.title, "Implement login");
    assert_eq!(task.bead_id, bead_id);
    assert_eq!(task.category, TaskCategory::Feature);
    assert_eq!(task.priority, TaskPriority::High);
    assert_eq!(task.complexity, TaskComplexity::Large);
    assert_eq!(task.phase, TaskPhase::Discovery);
    assert_eq!(task.progress_percent, 0);
    assert!(task.subtasks.is_empty());
    assert!(task.logs.is_empty());
    assert!(task.description.is_none());
    assert!(task.worktree_path.is_none());
    assert!(task.git_branch.is_none());
    assert!(task.started_at.is_none());
    assert!(task.completed_at.is_none());
    assert!(task.error.is_none());
}

#[test]
fn task_progress_from_phase_discovery() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::Discovery);
    assert_eq!(task.progress_percent, 5);
}

#[test]
fn task_progress_from_phase_context_gathering() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::ContextGathering);
    assert_eq!(task.progress_percent, 15);
}

#[test]
fn task_progress_from_phase_spec_creation() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::SpecCreation);
    assert_eq!(task.progress_percent, 25);
}

#[test]
fn task_progress_from_phase_planning() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::Planning);
    assert_eq!(task.progress_percent, 35);
}

#[test]
fn task_progress_from_phase_coding() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::Coding);
    assert_eq!(task.progress_percent, 55);
}

#[test]
fn task_progress_from_phase_qa() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::Qa);
    assert_eq!(task.progress_percent, 70);
}

#[test]
fn task_progress_from_phase_fixing() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::Fixing);
    assert_eq!(task.progress_percent, 80);
}

#[test]
fn task_progress_from_phase_merging() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::Merging);
    assert_eq!(task.progress_percent, 90);
}

#[test]
fn task_progress_from_phase_complete() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::Complete);
    assert_eq!(task.progress_percent, 100);
}

#[test]
fn task_progress_from_phase_error() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::Error);
    assert_eq!(task.progress_percent, 0);
}

#[test]
fn task_progress_from_phase_stopped() {
    let mut task = make_task("t");
    task.set_phase(TaskPhase::Stopped);
    assert_eq!(task.progress_percent, 0);
}

#[test]
fn task_with_description_and_rationale() {
    let mut task = make_task("Add dark mode");
    task.description =
        Some("Implement dark mode toggle.\n\nRationale: users request it frequently.".into());
    assert!(task.description.as_ref().unwrap().contains("Rationale"));
}

#[test]
fn task_timing_fields() {
    let mut task = make_task("Timed task");
    let before = Utc::now();
    assert!(task.created_at <= before);
    assert!(task.updated_at <= before);
    task.started_at = Some(Utc::now());
    task.completed_at = Some(Utc::now());
    assert!(task.started_at.unwrap() <= task.completed_at.unwrap());
}

#[test]
fn task_with_worktree_and_branch() {
    let mut task = make_task("Branch task");
    task.worktree_path = Some("/tmp/worktrees/feat-login".into());
    task.git_branch = Some("feat/login".into());
    assert_eq!(
        task.worktree_path.as_deref(),
        Some("/tmp/worktrees/feat-login")
    );
    assert_eq!(task.git_branch.as_deref(), Some("feat/login"));
}

// ===========================================================================
// 2. Subtask Management (15 tests)
// ===========================================================================

#[test]
fn add_subtasks_to_task() {
    let mut task = make_task("parent");
    task.subtasks.push(make_subtask("child 1"));
    task.subtasks.push(make_subtask("child 2"));
    assert_eq!(task.subtasks.len(), 2);
}

#[test]
fn subtask_status_pending_to_in_progress_to_complete() {
    let mut s = make_subtask("sub");
    assert_eq!(s.status, SubtaskStatus::Pending);
    s.status = SubtaskStatus::InProgress;
    assert_eq!(s.status, SubtaskStatus::InProgress);
    s.status = SubtaskStatus::Complete;
    assert_eq!(s.status, SubtaskStatus::Complete);
}

#[test]
fn subtask_status_pending_to_in_progress_to_failed() {
    let mut s = make_subtask("sub");
    s.status = SubtaskStatus::InProgress;
    s.status = SubtaskStatus::Failed;
    assert_eq!(s.status, SubtaskStatus::Failed);
}

#[test]
fn subtask_skip_status() {
    let mut s = make_subtask("skippable");
    s.status = SubtaskStatus::Skipped;
    assert_eq!(s.status, SubtaskStatus::Skipped);
}

#[test]
fn subtask_completion_percentage() {
    let mut task = make_task("parent");
    for i in 0..5 {
        let mut s = make_subtask(&format!("sub {}", i));
        if i < 3 {
            s.status = SubtaskStatus::Complete;
        }
        task.subtasks.push(s);
    }
    let completed = task
        .subtasks
        .iter()
        .filter(|s| s.status == SubtaskStatus::Complete)
        .count();
    let pct = (completed as f64 / task.subtasks.len() as f64 * 100.0) as u8;
    assert_eq!(pct, 60);
}

#[test]
fn subtask_ordering_preservation() {
    let mut task = make_task("ordered");
    let titles: Vec<String> = (0..5).map(|i| format!("step {}", i)).collect();
    for t in &titles {
        task.subtasks.push(make_subtask(t));
    }
    let result: Vec<&str> = task.subtasks.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        result,
        vec!["step 0", "step 1", "step 2", "step 3", "step 4"]
    );
}

#[test]
fn subtask_dependencies() {
    let dep_id = Uuid::new_v4();
    let mut s = make_subtask("dependent");
    s.depends_on = vec![dep_id];
    assert_eq!(s.depends_on.len(), 1);
    assert_eq!(s.depends_on[0], dep_id);
}

#[test]
fn subtask_multiple_dependencies() {
    let dep1 = Uuid::new_v4();
    let dep2 = Uuid::new_v4();
    let mut s = make_subtask("multi-dep");
    s.depends_on = vec![dep1, dep2];
    assert_eq!(s.depends_on.len(), 2);
}

#[test]
fn subtask_agent_assignment() {
    let agent_id = Uuid::new_v4();
    let mut s = make_subtask("assigned");
    s.agent_id = Some(agent_id);
    assert_eq!(s.agent_id, Some(agent_id));
}

#[test]
fn subtask_empty_list() {
    let task = make_task("no subs");
    assert!(task.subtasks.is_empty());
    let completed = task
        .subtasks
        .iter()
        .filter(|s| s.status == SubtaskStatus::Complete)
        .count();
    assert_eq!(completed, 0);
}

#[test]
fn subtask_filter_by_status_pending() {
    let mut task = make_task("filter");
    for i in 0..6 {
        let mut s = make_subtask(&format!("s{}", i));
        s.status = if i % 2 == 0 {
            SubtaskStatus::Pending
        } else {
            SubtaskStatus::Complete
        };
        task.subtasks.push(s);
    }
    let pending: Vec<_> = task
        .subtasks
        .iter()
        .filter(|s| s.status == SubtaskStatus::Pending)
        .collect();
    assert_eq!(pending.len(), 3);
}

#[test]
fn subtask_filter_by_status_in_progress() {
    let mut task = make_task("filter ip");
    for i in 0..4 {
        let mut s = make_subtask(&format!("s{}", i));
        s.status = if i == 1 {
            SubtaskStatus::InProgress
        } else {
            SubtaskStatus::Pending
        };
        task.subtasks.push(s);
    }
    let in_progress: Vec<_> = task
        .subtasks
        .iter()
        .filter(|s| s.status == SubtaskStatus::InProgress)
        .collect();
    assert_eq!(in_progress.len(), 1);
}

#[test]
fn subtask_count_completed_vs_total() {
    let mut task = make_task("count");
    for i in 0..10 {
        let mut s = make_subtask(&format!("s{}", i));
        if i < 7 {
            s.status = SubtaskStatus::Complete;
        }
        task.subtasks.push(s);
    }
    let total = task.subtasks.len();
    let completed = task
        .subtasks
        .iter()
        .filter(|s| s.status == SubtaskStatus::Complete)
        .count();
    assert_eq!(total, 10);
    assert_eq!(completed, 7);
}

#[test]
fn subtask_acceptance_criteria_as_titles() {
    let mut task = make_task("AC task");
    let criteria = vec![
        "User can log in with email",
        "Error message on invalid password",
        "Session persists across refresh",
    ];
    for c in &criteria {
        task.subtasks.push(make_subtask(c));
    }
    let titles: Vec<&str> = task.subtasks.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, criteria);
}

#[test]
fn subtask_all_statuses_represented() {
    let statuses = vec![
        SubtaskStatus::Pending,
        SubtaskStatus::InProgress,
        SubtaskStatus::Complete,
        SubtaskStatus::Failed,
        SubtaskStatus::Skipped,
    ];
    let mut task = make_task("all statuses");
    for (i, st) in statuses.iter().enumerate() {
        let mut s = make_subtask(&format!("s{}", i));
        s.status = st.clone();
        task.subtasks.push(s);
    }
    assert_eq!(task.subtasks.len(), 5);
    assert_eq!(task.subtasks[0].status, SubtaskStatus::Pending);
    assert_eq!(task.subtasks[1].status, SubtaskStatus::InProgress);
    assert_eq!(task.subtasks[2].status, SubtaskStatus::Complete);
    assert_eq!(task.subtasks[3].status, SubtaskStatus::Failed);
    assert_eq!(task.subtasks[4].status, SubtaskStatus::Skipped);
}

// ===========================================================================
// 3. Task Logging (15 tests)
// ===========================================================================

#[test]
fn log_text_entry() {
    let mut task = make_task("log test");
    task.log(TaskLogType::Text, "Hello world");
    assert_eq!(task.logs.len(), 1);
    assert_eq!(task.logs[0].log_type, TaskLogType::Text);
    assert_eq!(task.logs[0].message, "Hello world");
}

#[test]
fn log_phase_start_and_end() {
    let mut task = make_task("phase log");
    task.set_phase(TaskPhase::Coding);
    task.log(TaskLogType::PhaseStart, "Starting coding phase");
    task.log(TaskLogType::PhaseEnd, "Finished coding phase");
    assert_eq!(task.logs.len(), 2);
    assert_eq!(task.logs[0].log_type, TaskLogType::PhaseStart);
    assert_eq!(task.logs[1].log_type, TaskLogType::PhaseEnd);
    assert_eq!(task.logs[0].phase, TaskPhase::Coding);
    assert_eq!(task.logs[1].phase, TaskPhase::Coding);
}

#[test]
fn log_tool_start_and_end() {
    let mut task = make_task("tool log");
    task.log(TaskLogType::ToolStart, "Running cargo test");
    task.log(TaskLogType::ToolEnd, "cargo test complete");
    assert_eq!(task.logs[0].log_type, TaskLogType::ToolStart);
    assert_eq!(task.logs[1].log_type, TaskLogType::ToolEnd);
}

#[test]
fn log_error_entry() {
    let mut task = make_task("err log");
    task.log(TaskLogType::Error, "Compilation failed");
    assert_eq!(task.logs[0].log_type, TaskLogType::Error);
    assert_eq!(task.logs[0].message, "Compilation failed");
}

#[test]
fn log_success_entry() {
    let mut task = make_task("ok log");
    task.log(TaskLogType::Success, "All tests pass");
    assert_eq!(task.logs[0].log_type, TaskLogType::Success);
}

#[test]
fn log_info_entry() {
    let mut task = make_task("info log");
    task.log(TaskLogType::Info, "Found 12 files to process");
    assert_eq!(task.logs[0].log_type, TaskLogType::Info);
    assert_eq!(task.logs[0].message, "Found 12 files to process");
}

#[test]
fn logs_preserve_chronological_order() {
    let mut task = make_task("chrono");
    for i in 0..5 {
        task.log(TaskLogType::Text, format!("msg {}", i));
    }
    for w in task.logs.windows(2) {
        assert!(w[0].timestamp <= w[1].timestamp);
    }
}

#[test]
fn logs_with_detail_field() {
    let mut task = make_task("detail");
    task.logs.push(make_log_entry(
        TaskPhase::Coding,
        TaskLogType::ToolEnd,
        "cargo test finished",
        Some("test result: 42 passed, 0 failed"),
    ));
    assert_eq!(
        task.logs[0].detail.as_deref(),
        Some("test result: 42 passed, 0 failed")
    );
}

#[test]
fn logs_without_detail_field() {
    let mut task = make_task("no detail");
    task.log(TaskLogType::Text, "simple message");
    assert!(task.logs[0].detail.is_none());
}

#[test]
fn filter_logs_by_phase_planning() {
    let mut task = make_task("phase filter");
    task.set_phase(TaskPhase::Planning);
    task.log(TaskLogType::Text, "planning msg");
    task.set_phase(TaskPhase::Coding);
    task.log(TaskLogType::Text, "coding msg");
    task.set_phase(TaskPhase::Qa);
    task.log(TaskLogType::Text, "qa msg");

    let planning_logs: Vec<_> = task
        .logs
        .iter()
        .filter(|l| l.phase == TaskPhase::Planning)
        .collect();
    assert_eq!(planning_logs.len(), 1);
    assert_eq!(planning_logs[0].message, "planning msg");
}

#[test]
fn filter_logs_by_phase_coding() {
    let mut task = make_task("coding filter");
    task.set_phase(TaskPhase::Coding);
    task.log(TaskLogType::Text, "code line 1");
    task.log(TaskLogType::Text, "code line 2");
    task.set_phase(TaskPhase::Qa);
    task.log(TaskLogType::Text, "qa check");

    let coding_logs: Vec<_> = task
        .logs
        .iter()
        .filter(|l| l.phase == TaskPhase::Coding)
        .collect();
    assert_eq!(coding_logs.len(), 2);
}

#[test]
fn filter_logs_by_type() {
    let mut task = make_task("type filter");
    task.log(TaskLogType::Error, "err 1");
    task.log(TaskLogType::Text, "text 1");
    task.log(TaskLogType::Error, "err 2");
    task.log(TaskLogType::Success, "ok");

    let errors: Vec<_> = task
        .logs
        .iter()
        .filter(|l| l.log_type == TaskLogType::Error)
        .collect();
    assert_eq!(errors.len(), 2);
}

#[test]
fn log_entry_count_per_phase() {
    let mut task = make_task("count per phase");
    task.set_phase(TaskPhase::Planning);
    for _ in 0..3 {
        task.log(TaskLogType::Text, "plan");
    }
    task.set_phase(TaskPhase::Coding);
    for _ in 0..5 {
        task.log(TaskLogType::Text, "code");
    }

    let planning_count = task
        .logs
        .iter()
        .filter(|l| l.phase == TaskPhase::Planning)
        .count();
    let coding_count = task
        .logs
        .iter()
        .filter(|l| l.phase == TaskPhase::Coding)
        .count();
    assert_eq!(planning_count, 3);
    assert_eq!(coding_count, 5);
}

#[test]
fn phase_based_log_grouping() {
    let mut task = make_task("grouping");
    let phases = [TaskPhase::Planning, TaskPhase::Coding, TaskPhase::Qa];
    for p in &phases {
        task.set_phase(p.clone());
        task.log(TaskLogType::PhaseStart, format!("start {:?}", p));
        task.log(TaskLogType::Text, format!("work in {:?}", p));
        task.log(TaskLogType::PhaseEnd, format!("end {:?}", p));
    }

    // Group by phase
    let mut groups: std::collections::HashMap<String, Vec<&TaskLogEntry>> =
        std::collections::HashMap::new();
    for entry in &task.logs {
        groups
            .entry(format!("{:?}", entry.phase))
            .or_default()
            .push(entry);
    }
    assert_eq!(groups.len(), 3);
    for (_, entries) in &groups {
        assert_eq!(entries.len(), 3);
    }
}

#[test]
fn logging_updates_task_updated_at() {
    let mut task = make_task("update ts");
    let before = task.updated_at;
    std::thread::sleep(std::time::Duration::from_millis(2));
    task.log(TaskLogType::Info, "tick");
    assert!(task.updated_at >= before);
}

// ===========================================================================
// 4. Task Phase Transitions (15 tests)
// ===========================================================================

#[test]
fn valid_full_pipeline_transition_chain() {
    let chain = [
        TaskPhase::Discovery,
        TaskPhase::ContextGathering,
        TaskPhase::SpecCreation,
        TaskPhase::Planning,
        TaskPhase::Coding,
        TaskPhase::Qa,
        TaskPhase::Merging,
        TaskPhase::Complete,
    ];
    for w in chain.windows(2) {
        assert!(
            w[0].can_transition_to(&w[1]),
            "{:?} should transition to {:?}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn invalid_transition_discovery_to_coding() {
    assert!(!TaskPhase::Discovery.can_transition_to(&TaskPhase::Coding));
}

#[test]
fn invalid_transition_coding_to_discovery() {
    assert!(!TaskPhase::Coding.can_transition_to(&TaskPhase::Discovery));
}

#[test]
fn invalid_transition_complete_to_coding() {
    assert!(!TaskPhase::Complete.can_transition_to(&TaskPhase::Coding));
}

#[test]
fn invalid_transition_qa_to_planning() {
    assert!(!TaskPhase::Qa.can_transition_to(&TaskPhase::Planning));
}

#[test]
fn any_phase_can_transition_to_error() {
    for phase in TaskPhase::pipeline_order() {
        assert!(
            phase.can_transition_to(&TaskPhase::Error),
            "{:?} should transition to Error",
            phase
        );
    }
}

#[test]
fn any_phase_can_transition_to_stopped() {
    for phase in TaskPhase::pipeline_order() {
        assert!(
            phase.can_transition_to(&TaskPhase::Stopped),
            "{:?} should transition to Stopped",
            phase
        );
    }
}

#[test]
fn error_and_stopped_can_also_go_to_error_stopped() {
    // Error -> Error, Error -> Stopped, Stopped -> Error, Stopped -> Stopped
    // The wildcard match (_, Error) and (_, Stopped) covers these.
    assert!(TaskPhase::Error.can_transition_to(&TaskPhase::Error));
    assert!(TaskPhase::Error.can_transition_to(&TaskPhase::Stopped));
    assert!(TaskPhase::Stopped.can_transition_to(&TaskPhase::Error));
    assert!(TaskPhase::Stopped.can_transition_to(&TaskPhase::Stopped));
}

#[test]
fn phase_progress_percent_mapping_all_values() {
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
        (TaskPhase::Error, 0),
        (TaskPhase::Stopped, 0),
    ];
    for (phase, pct) in expected {
        assert_eq!(phase.progress_percent(), pct, "Mismatch for {:?}", phase);
    }
}

#[test]
fn pipeline_order_consistency() {
    let order = TaskPhase::pipeline_order();
    assert_eq!(order.len(), 9);
    assert_eq!(order[0], TaskPhase::Discovery);
    assert_eq!(order[order.len() - 1], TaskPhase::Complete);
    // Progress should be monotonically non-decreasing along the pipeline
    for w in order.windows(2) {
        assert!(
            w[0].progress_percent() <= w[1].progress_percent(),
            "{:?} ({}%) should be <= {:?} ({}%)",
            w[0],
            w[0].progress_percent(),
            w[1],
            w[1].progress_percent()
        );
    }
}

#[test]
fn pipeline_order_excludes_terminal_states() {
    let order = TaskPhase::pipeline_order();
    assert!(!order.contains(&TaskPhase::Error));
    assert!(!order.contains(&TaskPhase::Stopped));
}

#[test]
fn fixing_can_go_back_to_qa() {
    assert!(TaskPhase::Fixing.can_transition_to(&TaskPhase::Qa));
}

#[test]
fn fixing_can_go_back_to_coding() {
    assert!(TaskPhase::Fixing.can_transition_to(&TaskPhase::Coding));
}

#[test]
fn set_phase_updates_progress_percent_automatically() {
    let mut task = make_task("auto progress");
    assert_eq!(task.progress_percent, 0);
    task.set_phase(TaskPhase::Coding);
    assert_eq!(task.progress_percent, 55);
    assert_eq!(task.phase, TaskPhase::Coding);
    task.set_phase(TaskPhase::Complete);
    assert_eq!(task.progress_percent, 100);
}

#[test]
fn set_phase_updates_updated_at() {
    let mut task = make_task("phase ts");
    let before = task.updated_at;
    std::thread::sleep(std::time::Duration::from_millis(2));
    task.set_phase(TaskPhase::Planning);
    assert!(task.updated_at >= before);
}

// ===========================================================================
// 5. Edge Cases (8 tests)
// ===========================================================================

#[test]
fn task_with_100_plus_subtasks() {
    let mut task = make_task("big");
    for i in 0..150 {
        task.subtasks.push(make_subtask(&format!("sub {}", i)));
    }
    assert_eq!(task.subtasks.len(), 150);
}

#[test]
fn task_with_1000_plus_log_entries() {
    let mut task = make_task("chatty");
    task.set_phase(TaskPhase::Coding);
    for i in 0..1200 {
        task.log(TaskLogType::Text, format!("log line {}", i));
    }
    assert_eq!(task.logs.len(), 1200);
}

#[test]
fn task_serialization_deserialization_roundtrip() {
    let mut task = make_task("serde task");
    task.description = Some("desc".into());
    task.worktree_path = Some("/tmp/wt".into());
    task.git_branch = Some("feat/x".into());
    task.set_phase(TaskPhase::Coding);
    task.subtasks.push(make_subtask("s1"));
    task.log(TaskLogType::Info, "hello");
    task.error = Some("oops".into());
    task.started_at = Some(Utc::now());

    let json = serde_json::to_string(&task).expect("serialize");
    let deserialized: Task = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.title, task.title);
    assert_eq!(deserialized.description, task.description);
    assert_eq!(deserialized.phase, task.phase);
    assert_eq!(deserialized.progress_percent, task.progress_percent);
    assert_eq!(deserialized.subtasks.len(), 1);
    assert_eq!(deserialized.logs.len(), 1);
    assert_eq!(deserialized.worktree_path, task.worktree_path);
    assert_eq!(deserialized.git_branch, task.git_branch);
    assert_eq!(deserialized.error, task.error);
    assert_eq!(deserialized.category, task.category);
    assert_eq!(deserialized.priority, task.priority);
    assert_eq!(deserialized.complexity, task.complexity);
}

#[test]
fn task_clone_preserves_all_fields() {
    let mut task = make_task("clone me");
    task.description = Some("desc".into());
    task.worktree_path = Some("/tmp/wt".into());
    task.git_branch = Some("feat/y".into());
    task.set_phase(TaskPhase::Qa);
    task.subtasks.push(make_subtask("c1"));
    task.log(TaskLogType::Error, "err");
    task.error = Some("fail".into());
    task.started_at = Some(Utc::now());
    task.completed_at = Some(Utc::now());

    let cloned = task.clone();
    assert_eq!(cloned.id, task.id);
    assert_eq!(cloned.title, task.title);
    assert_eq!(cloned.description, task.description);
    assert_eq!(cloned.bead_id, task.bead_id);
    assert_eq!(cloned.phase, task.phase);
    assert_eq!(cloned.progress_percent, task.progress_percent);
    assert_eq!(cloned.subtasks.len(), task.subtasks.len());
    assert_eq!(cloned.subtasks[0].title, task.subtasks[0].title);
    assert_eq!(cloned.worktree_path, task.worktree_path);
    assert_eq!(cloned.git_branch, task.git_branch);
    assert_eq!(cloned.category, task.category);
    assert_eq!(cloned.priority, task.priority);
    assert_eq!(cloned.complexity, task.complexity);
    assert_eq!(cloned.error, task.error);
    assert_eq!(cloned.logs.len(), task.logs.len());
}

#[test]
fn task_with_empty_title() {
    let task = make_task("");
    assert_eq!(task.title, "");
}

#[test]
fn task_with_empty_description() {
    let mut task = make_task("t");
    task.description = Some("".into());
    assert_eq!(task.description.as_deref(), Some(""));
}

#[test]
fn subtask_serde_roundtrip() {
    let agent = Uuid::new_v4();
    let dep = Uuid::new_v4();
    let mut s = make_subtask("serde sub");
    s.status = SubtaskStatus::InProgress;
    s.agent_id = Some(agent);
    s.depends_on = vec![dep];

    let json = serde_json::to_string(&s).expect("serialize subtask");
    let d: Subtask = serde_json::from_str(&json).expect("deserialize subtask");
    assert_eq!(d.title, "serde sub");
    assert_eq!(d.status, SubtaskStatus::InProgress);
    assert_eq!(d.agent_id, Some(agent));
    assert_eq!(d.depends_on, vec![dep]);
}

#[test]
fn log_entry_serde_roundtrip() {
    let entry = make_log_entry(
        TaskPhase::Qa,
        TaskLogType::ToolEnd,
        "done",
        Some("details here"),
    );
    let json = serde_json::to_string(&entry).expect("serialize log");
    let d: TaskLogEntry = serde_json::from_str(&json).expect("deserialize log");
    assert_eq!(d.phase, TaskPhase::Qa);
    assert_eq!(d.log_type, TaskLogType::ToolEnd);
    assert_eq!(d.message, "done");
    assert_eq!(d.detail.as_deref(), Some("details here"));
}

// ===========================================================================
// Additional category/priority/complexity coverage
// ===========================================================================

#[test]
fn task_all_categories() {
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
    for cat in categories {
        let task = Task::new(
            "t",
            Uuid::new_v4(),
            cat.clone(),
            TaskPriority::Low,
            TaskComplexity::Trivial,
        );
        assert_eq!(task.category, cat);
    }
}

#[test]
fn task_all_priorities() {
    let priorities = vec![
        TaskPriority::Low,
        TaskPriority::Medium,
        TaskPriority::High,
        TaskPriority::Urgent,
    ];
    for pri in priorities {
        let task = Task::new(
            "t",
            Uuid::new_v4(),
            TaskCategory::Feature,
            pri.clone(),
            TaskComplexity::Trivial,
        );
        assert_eq!(task.priority, pri);
    }
}

#[test]
fn task_all_complexities() {
    let complexities = vec![
        TaskComplexity::Trivial,
        TaskComplexity::Small,
        TaskComplexity::Medium,
        TaskComplexity::Large,
        TaskComplexity::Complex,
    ];
    for cplx in complexities {
        let task = Task::new(
            "t",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Low,
            cplx.clone(),
        );
        assert_eq!(task.complexity, cplx);
    }
}

#[test]
fn task_error_field_set_on_error_phase() {
    let mut task = make_task("failing");
    task.set_phase(TaskPhase::Error);
    task.error = Some("Build failed: missing dependency".into());
    assert_eq!(task.phase, TaskPhase::Error);
    assert_eq!(task.progress_percent, 0);
    assert!(task.error.is_some());
}

#[test]
fn qa_can_transition_to_fixing_or_merging() {
    assert!(TaskPhase::Qa.can_transition_to(&TaskPhase::Fixing));
    assert!(TaskPhase::Qa.can_transition_to(&TaskPhase::Merging));
}

#[test]
fn task_log_captures_current_phase() {
    let mut task = make_task("phase capture");
    task.set_phase(TaskPhase::Discovery);
    task.log(TaskLogType::Text, "discover");
    task.set_phase(TaskPhase::Coding);
    task.log(TaskLogType::Text, "code");

    assert_eq!(task.logs[0].phase, TaskPhase::Discovery);
    assert_eq!(task.logs[1].phase, TaskPhase::Coding);
}

#[test]
fn task_new_defaults_to_discovery_phase() {
    let task = make_task("fresh");
    assert_eq!(task.phase, TaskPhase::Discovery);
    // Note: progress_percent starts at 0 (not 5) because new() does not call set_phase
    assert_eq!(task.progress_percent, 0);
}

#[test]
fn task_phase_serde_snake_case() {
    let json = serde_json::to_string(&TaskPhase::ContextGathering).unwrap();
    assert_eq!(json, "\"context_gathering\"");

    let json = serde_json::to_string(&TaskPhase::SpecCreation).unwrap();
    assert_eq!(json, "\"spec_creation\"");

    let parsed: TaskPhase = serde_json::from_str("\"qa\"").unwrap();
    assert_eq!(parsed, TaskPhase::Qa);
}

#[test]
fn subtask_status_serde_snake_case() {
    let json = serde_json::to_string(&SubtaskStatus::InProgress).unwrap();
    assert_eq!(json, "\"in_progress\"");

    let parsed: SubtaskStatus = serde_json::from_str("\"complete\"").unwrap();
    assert_eq!(parsed, SubtaskStatus::Complete);
}

#[test]
fn log_type_serde_snake_case() {
    let json = serde_json::to_string(&TaskLogType::PhaseStart).unwrap();
    assert_eq!(json, "\"phase_start\"");

    let json = serde_json::to_string(&TaskLogType::ToolEnd).unwrap();
    assert_eq!(json, "\"tool_end\"");
}

#[test]
fn task_full_lifecycle_integration() {
    let mut task = make_task("full lifecycle");
    task.description = Some("End-to-end flow".into());

    // Add acceptance criteria as subtasks
    let s1 = make_subtask("Parse input");
    let s2 = make_subtask("Transform data");
    let s3 = make_subtask("Write output");
    task.subtasks.push(s1);
    task.subtasks.push(s2);
    task.subtasks.push(s3);

    // Move through phases
    task.set_phase(TaskPhase::Discovery);
    task.log(TaskLogType::PhaseStart, "discovery");
    task.started_at = Some(Utc::now());

    task.set_phase(TaskPhase::ContextGathering);
    task.set_phase(TaskPhase::SpecCreation);
    task.set_phase(TaskPhase::Planning);
    task.set_phase(TaskPhase::Coding);

    // Mark subtasks in progress then done
    task.subtasks[0].status = SubtaskStatus::Complete;
    task.subtasks[1].status = SubtaskStatus::InProgress;
    task.log(TaskLogType::ToolStart, "cargo build");
    task.log(TaskLogType::ToolEnd, "build succeeded");

    task.subtasks[1].status = SubtaskStatus::Complete;
    task.subtasks[2].status = SubtaskStatus::Complete;

    task.set_phase(TaskPhase::Qa);
    task.log(TaskLogType::Success, "all tests pass");

    task.set_phase(TaskPhase::Merging);
    task.set_phase(TaskPhase::Complete);
    task.completed_at = Some(Utc::now());

    assert_eq!(task.phase, TaskPhase::Complete);
    assert_eq!(task.progress_percent, 100);
    assert!(task.started_at.unwrap() <= task.completed_at.unwrap());

    let completed = task
        .subtasks
        .iter()
        .filter(|s| s.status == SubtaskStatus::Complete)
        .count();
    assert_eq!(completed, 3);
    assert!(task.logs.len() >= 4);
}
