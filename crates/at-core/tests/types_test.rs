use at_core::types::*;
use uuid::Uuid;

#[test]
fn bead_status_valid_transitions() {
    assert!(BeadStatus::Backlog.can_transition_to(&BeadStatus::Hooked));
    assert!(BeadStatus::Hooked.can_transition_to(&BeadStatus::Slung));
    assert!(BeadStatus::Hooked.can_transition_to(&BeadStatus::Backlog));
    assert!(BeadStatus::Slung.can_transition_to(&BeadStatus::Review));
    assert!(BeadStatus::Slung.can_transition_to(&BeadStatus::Failed));
    assert!(BeadStatus::Slung.can_transition_to(&BeadStatus::Escalated));
    assert!(BeadStatus::Review.can_transition_to(&BeadStatus::Done));
    assert!(BeadStatus::Review.can_transition_to(&BeadStatus::Slung));
    assert!(BeadStatus::Review.can_transition_to(&BeadStatus::Failed));
    assert!(BeadStatus::Failed.can_transition_to(&BeadStatus::Backlog));
    assert!(BeadStatus::Escalated.can_transition_to(&BeadStatus::Backlog));
}

#[test]
fn bead_status_invalid_transitions() {
    assert!(!BeadStatus::Backlog.can_transition_to(&BeadStatus::Done));
    assert!(!BeadStatus::Done.can_transition_to(&BeadStatus::Backlog));
    assert!(!BeadStatus::Hooked.can_transition_to(&BeadStatus::Review));
    assert!(!BeadStatus::Review.can_transition_to(&BeadStatus::Hooked));
}

#[test]
fn bead_creation() {
    let bead = Bead::new("test task", Lane::Standard);
    assert_eq!(bead.title, "test task");
    assert_eq!(bead.status, BeadStatus::Backlog);
    assert_eq!(bead.lane, Lane::Standard);
    assert_eq!(bead.priority, 0);
    assert!(bead.description.is_none());
    assert!(bead.agent_id.is_none());
}

#[test]
fn agent_status_glyph() {
    assert_eq!(AgentStatus::Active.glyph(), "@");
    assert_eq!(AgentStatus::Idle.glyph(), "*");
    assert_eq!(AgentStatus::Pending.glyph(), "!");
    assert_eq!(AgentStatus::Unknown.glyph(), "?");
    assert_eq!(AgentStatus::Stopped.glyph(), "x");
}

#[test]
fn lane_ordering() {
    assert!(Lane::Experimental < Lane::Standard);
    assert!(Lane::Standard < Lane::Critical);
    assert!(Lane::Experimental < Lane::Critical);
}

#[test]
fn serialization_roundtrip() {
    let bead = Bead::new("roundtrip", Lane::Critical);
    let json = serde_json::to_string(&bead).expect("serialize");
    let back: Bead = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.title, "roundtrip");
    assert_eq!(back.lane, Lane::Critical);
    assert_eq!(back.status, BeadStatus::Backlog);

    let agent = Agent::new("agent-1", AgentRole::Mayor, CliType::Claude);
    let json = serde_json::to_string(&agent).expect("serialize agent");
    let back: Agent = serde_json::from_str(&json).expect("deserialize agent");
    assert_eq!(back.name, "agent-1");
    assert_eq!(back.role, AgentRole::Mayor);
    assert_eq!(back.cli_type, CliType::Claude);
}

// ---------------------------------------------------------------------------
// Task pipeline tests
// ---------------------------------------------------------------------------

#[test]
fn task_creation() {
    let bead_id = Uuid::new_v4();
    let task = Task::new(
        "implement login",
        bead_id,
        TaskCategory::Feature,
        TaskPriority::High,
        TaskComplexity::Medium,
    );
    assert_eq!(task.title, "implement login");
    assert_eq!(task.bead_id, bead_id);
    assert_eq!(task.phase, TaskPhase::Discovery);
    assert_eq!(task.progress_percent, 0);
    assert_eq!(task.category, TaskCategory::Feature);
    assert_eq!(task.priority, TaskPriority::High);
    assert_eq!(task.complexity, TaskComplexity::Medium);
    assert!(task.description.is_none());
    assert!(task.started_at.is_none());
    assert!(task.completed_at.is_none());
    assert!(task.error.is_none());
    assert!(task.subtasks.is_empty());
    assert!(task.logs.is_empty());
}

#[test]
fn task_phase_valid_transitions() {
    // Normal pipeline flow
    assert!(TaskPhase::Discovery.can_transition_to(&TaskPhase::ContextGathering));
    assert!(TaskPhase::ContextGathering.can_transition_to(&TaskPhase::SpecCreation));
    assert!(TaskPhase::SpecCreation.can_transition_to(&TaskPhase::Planning));
    assert!(TaskPhase::Planning.can_transition_to(&TaskPhase::Coding));
    assert!(TaskPhase::Coding.can_transition_to(&TaskPhase::Qa));
    assert!(TaskPhase::Qa.can_transition_to(&TaskPhase::Merging));
    assert!(TaskPhase::Merging.can_transition_to(&TaskPhase::Complete));

    // QA can go to Fixing
    assert!(TaskPhase::Qa.can_transition_to(&TaskPhase::Fixing));
    // Fixing can loop back
    assert!(TaskPhase::Fixing.can_transition_to(&TaskPhase::Qa));
    assert!(TaskPhase::Fixing.can_transition_to(&TaskPhase::Coding));

    // Any phase can go to Error or Stopped
    assert!(TaskPhase::Discovery.can_transition_to(&TaskPhase::Error));
    assert!(TaskPhase::Coding.can_transition_to(&TaskPhase::Error));
    assert!(TaskPhase::Qa.can_transition_to(&TaskPhase::Stopped));
    assert!(TaskPhase::Merging.can_transition_to(&TaskPhase::Stopped));
}

#[test]
fn task_phase_invalid_transitions() {
    // Can't skip phases
    assert!(!TaskPhase::Discovery.can_transition_to(&TaskPhase::Coding));
    assert!(!TaskPhase::Discovery.can_transition_to(&TaskPhase::Complete));
    assert!(!TaskPhase::Planning.can_transition_to(&TaskPhase::Merging));

    // Can't go backwards (except Fixing loops)
    assert!(!TaskPhase::Coding.can_transition_to(&TaskPhase::Discovery));
    assert!(!TaskPhase::Qa.can_transition_to(&TaskPhase::Planning));
}

#[test]
fn task_phase_progress_percentages() {
    assert_eq!(TaskPhase::Discovery.progress_percent(), 5);
    assert_eq!(TaskPhase::ContextGathering.progress_percent(), 15);
    assert_eq!(TaskPhase::SpecCreation.progress_percent(), 25);
    assert_eq!(TaskPhase::Planning.progress_percent(), 35);
    assert_eq!(TaskPhase::Coding.progress_percent(), 55);
    assert_eq!(TaskPhase::Qa.progress_percent(), 70);
    assert_eq!(TaskPhase::Fixing.progress_percent(), 80);
    assert_eq!(TaskPhase::Merging.progress_percent(), 90);
    assert_eq!(TaskPhase::Complete.progress_percent(), 100);
    assert_eq!(TaskPhase::Error.progress_percent(), 0);
    assert_eq!(TaskPhase::Stopped.progress_percent(), 0);
}

#[test]
fn task_set_phase_updates_progress() {
    let mut task = Task::new(
        "test",
        Uuid::new_v4(),
        TaskCategory::BugFix,
        TaskPriority::Low,
        TaskComplexity::Trivial,
    );
    assert_eq!(task.phase, TaskPhase::Discovery);
    assert_eq!(task.progress_percent, 0);

    task.set_phase(TaskPhase::Coding);
    assert_eq!(task.phase, TaskPhase::Coding);
    assert_eq!(task.progress_percent, 55);

    task.set_phase(TaskPhase::Complete);
    assert_eq!(task.phase, TaskPhase::Complete);
    assert_eq!(task.progress_percent, 100);
}

#[test]
fn task_logging() {
    let mut task = Task::new(
        "test",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );

    task.log(TaskLogType::Info, "Starting work");
    task.log(TaskLogType::Error, "Something failed");

    assert_eq!(task.logs.len(), 2);
    assert_eq!(task.logs[0].log_type, TaskLogType::Info);
    assert_eq!(task.logs[0].message, "Starting work");
    assert_eq!(task.logs[1].log_type, TaskLogType::Error);
    assert_eq!(task.logs[1].message, "Something failed");
}

#[test]
fn task_serialization_roundtrip() {
    let task = Task::new(
        "serialize me",
        Uuid::new_v4(),
        TaskCategory::Security,
        TaskPriority::Urgent,
        TaskComplexity::Complex,
    );
    let json = serde_json::to_string(&task).expect("serialize task");
    let back: Task = serde_json::from_str(&json).expect("deserialize task");
    assert_eq!(back.title, "serialize me");
    assert_eq!(back.category, TaskCategory::Security);
    assert_eq!(back.priority, TaskPriority::Urgent);
    assert_eq!(back.complexity, TaskComplexity::Complex);
    assert_eq!(back.phase, TaskPhase::Discovery);
}

#[test]
fn subtask_status_serialization() {
    let statuses = vec![
        SubtaskStatus::Pending,
        SubtaskStatus::InProgress,
        SubtaskStatus::Complete,
        SubtaskStatus::Failed,
        SubtaskStatus::Skipped,
    ];
    for status in statuses {
        let json = serde_json::to_string(&status).expect("serialize");
        let back: SubtaskStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, status);
    }
}

#[test]
fn task_pipeline_order() {
    let order = TaskPhase::pipeline_order();
    assert_eq!(order.len(), 9);
    assert_eq!(order[0], TaskPhase::Discovery);
    assert_eq!(order[order.len() - 1], TaskPhase::Complete);
}

#[test]
fn all_task_categories_serialize() {
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
        let json = serde_json::to_string(&cat).expect("serialize");
        let back: TaskCategory = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, cat);
    }
}
