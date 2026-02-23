use at_agents::approval::{ApprovalPolicy, ApprovalStatus, ToolApprovalSystem};
use at_agents::roles::{
    role_config_for, CrewAgent, DeaconAgent, MayorAgent, PolecatAgent, RefineryAgent, RoleConfig,
    WitnessAgent,
};
use at_core::types::AgentRole;

// ===========================================================================
// Role system prompt tests
// ===========================================================================

#[test]
fn mayor_has_nonempty_system_prompt() {
    let agent = MayorAgent::new();
    let prompt = agent.system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.len() > 50, "system prompt should be detailed");
    assert!(
        prompt.contains("orchestrat"),
        "Mayor should mention orchestration"
    );
}

#[test]
fn deacon_has_nonempty_system_prompt() {
    let agent = DeaconAgent::new();
    let prompt = agent.system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.len() > 50);
    assert!(prompt.contains("review"), "Deacon should mention review");
}

#[test]
fn witness_has_nonempty_system_prompt() {
    let agent = WitnessAgent::new();
    let prompt = agent.system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.len() > 50);
    assert!(prompt.contains("test"), "Witness should mention testing");
}

#[test]
fn refinery_has_nonempty_system_prompt() {
    let agent = RefineryAgent::new();
    let prompt = agent.system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.len() > 50);
    assert!(
        prompt.contains("refactor"),
        "Refinery should mention refactoring"
    );
}

#[test]
fn polecat_has_nonempty_system_prompt() {
    let agent = PolecatAgent::new();
    let prompt = agent.system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.len() > 50);
    assert!(
        prompt.contains("security") || prompt.contains("vulnerabilit"),
        "Polecat should mention security"
    );
}

#[test]
fn crew_has_nonempty_system_prompt() {
    let agent = CrewAgent::new();
    let prompt = agent.system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.len() > 50);
    assert!(
        prompt.contains("implement") || prompt.contains("worker"),
        "Crew should mention implementation or working"
    );
}

// ===========================================================================
// Role allowed_tools tests
// ===========================================================================

#[test]
fn mayor_cannot_write_files() {
    let agent = MayorAgent::new();
    let tools = agent.allowed_tools();
    assert!(
        !tools.contains(&"file_write".to_string()),
        "Mayor should not have file_write permission"
    );
    assert!(
        tools.contains(&"file_read".to_string()),
        "Mayor should be able to read files"
    );
    assert!(
        tools.contains(&"task_assign".to_string()),
        "Mayor should be able to assign tasks"
    );
}

#[test]
fn deacon_cannot_write_files() {
    let agent = DeaconAgent::new();
    let tools = agent.allowed_tools();
    assert!(
        !tools.contains(&"file_write".to_string()),
        "Deacon should not have file_write permission"
    );
    assert!(
        tools.contains(&"git_diff".to_string()),
        "Deacon should be able to view diffs"
    );
}

#[test]
fn witness_can_execute_shell() {
    let agent = WitnessAgent::new();
    let tools = agent.allowed_tools();
    assert!(
        tools.contains(&"shell_execute".to_string()),
        "Witness needs shell access to run tests"
    );
    assert!(
        !tools.contains(&"file_write".to_string()),
        "Witness should not write files"
    );
}

#[test]
fn refinery_can_read_and_write() {
    let agent = RefineryAgent::new();
    let tools = agent.allowed_tools();
    assert!(tools.contains(&"file_read".to_string()));
    assert!(
        tools.contains(&"file_write".to_string()),
        "Refinery needs write access for refactoring"
    );
}

#[test]
fn polecat_cannot_write_files() {
    let agent = PolecatAgent::new();
    let tools = agent.allowed_tools();
    assert!(
        !tools.contains(&"file_write".to_string()),
        "Polecat should not write files"
    );
    assert!(
        tools.contains(&"shell_execute".to_string()),
        "Polecat needs shell for security scanning tools"
    );
}

#[test]
fn crew_has_full_dev_tools() {
    let agent = CrewAgent::new();
    let tools = agent.allowed_tools();
    assert!(tools.contains(&"file_read".to_string()));
    assert!(tools.contains(&"file_write".to_string()));
    assert!(tools.contains(&"shell_execute".to_string()));
    assert!(tools.contains(&"git_commit".to_string()));
}

// ===========================================================================
// Role max_turns tests
// ===========================================================================

#[test]
fn mayor_has_highest_turn_limit() {
    let mayor = MayorAgent::new();
    let crew = CrewAgent::new();
    assert!(
        mayor.max_turns() >= crew.max_turns(),
        "Mayor (orchestrator) should have at least as many turns as Crew"
    );
}

#[test]
fn all_roles_have_positive_max_turns() {
    let roles: Vec<Box<dyn RoleConfig + Send + Sync>> = vec![
        Box::new(MayorAgent::new()),
        Box::new(DeaconAgent::new()),
        Box::new(WitnessAgent::new()),
        Box::new(RefineryAgent::new()),
        Box::new(PolecatAgent::new()),
        Box::new(CrewAgent::new()),
    ];
    for role in &roles {
        assert!(role.max_turns() > 0, "max_turns should be positive");
    }
}

// ===========================================================================
// Role pre/post execute hook tests
// ===========================================================================

#[test]
fn mayor_pre_execute_returns_preamble() {
    let agent = MayorAgent::new();
    let result = agent.pre_execute("Build feature X");
    assert!(result.is_some());
    assert!(result.unwrap().contains("Build feature X"));
}

#[test]
fn crew_pre_execute_returns_preamble() {
    let agent = CrewAgent::new();
    let result = agent.pre_execute("Implement auth module");
    assert!(result.is_some());
    assert!(result.unwrap().contains("Implement auth module"));
}

#[test]
fn deacon_post_execute_reports_issues() {
    let agent = DeaconAgent::new();
    let output_with_issues = "line1\n[WARNING] unused import\nline3\n[ERROR] missing return";
    let result = agent.post_execute(output_with_issues);
    assert!(result.is_some());
    assert!(result.unwrap().contains("2 issues"));
}

#[test]
fn deacon_post_execute_reports_clean() {
    let agent = DeaconAgent::new();
    let clean_output = "All checks passed\nCode looks good";
    let result = agent.post_execute(clean_output);
    assert!(result.is_some());
    assert!(result.unwrap().contains("Approved"));
}

#[test]
fn witness_post_execute_detects_failures() {
    let agent = WitnessAgent::new();
    let failed_output = "test result: FAILED. 3 passed; 1 failed";
    let result = agent.post_execute(failed_output);
    assert!(result.is_some());
    assert!(result.unwrap().contains("FAILURES"));
}

#[test]
fn witness_post_execute_detects_success() {
    let agent = WitnessAgent::new();
    let ok_output = "test result: ok. 42 passed; 0 failed";
    let result = agent.post_execute(ok_output);
    assert!(result.is_some());
    assert!(result.unwrap().contains("passed"));
}

// ===========================================================================
// Role preferred_model tests
// ===========================================================================

#[test]
fn all_roles_have_preferred_model() {
    let roles: Vec<Box<dyn RoleConfig + Send + Sync>> = vec![
        Box::new(MayorAgent::new()),
        Box::new(DeaconAgent::new()),
        Box::new(WitnessAgent::new()),
        Box::new(RefineryAgent::new()),
        Box::new(PolecatAgent::new()),
        Box::new(CrewAgent::new()),
    ];
    for role in &roles {
        assert!(
            role.preferred_model().is_some(),
            "each role should specify a preferred model"
        );
    }
}

// ===========================================================================
// role_config_for helper tests
// ===========================================================================

#[test]
fn role_config_for_returns_correct_prompts() {
    let mayor_config = role_config_for(&AgentRole::Mayor);
    assert!(mayor_config.system_prompt().contains("orchestrat"));

    let crew_config = role_config_for(&AgentRole::Crew);
    assert!(
        crew_config.system_prompt().contains("worker")
            || crew_config.system_prompt().contains("implement")
    );
}

// ===========================================================================
// Tool approval integration tests
// ===========================================================================

#[test]
fn approval_system_default_policies_match_spec() {
    let system = ToolApprovalSystem::new();

    // file_read = AutoApprove
    assert_eq!(
        system.check_approval("file_read", &AgentRole::Crew),
        ApprovalPolicy::AutoApprove
    );
    // file_write = RequireApproval
    assert_eq!(
        system.check_approval("file_write", &AgentRole::Crew),
        ApprovalPolicy::RequireApproval
    );
    // shell_execute = RequireApproval
    assert_eq!(
        system.check_approval("shell_execute", &AgentRole::Crew),
        ApprovalPolicy::RequireApproval
    );
    // git_push = RequireApproval
    assert_eq!(
        system.check_approval("git_push", &AgentRole::Crew),
        ApprovalPolicy::RequireApproval
    );
    // delete = Deny
    assert_eq!(
        system.check_approval("delete", &AgentRole::Crew),
        ApprovalPolicy::Deny
    );
}

#[test]
fn approval_full_lifecycle() {
    let mut system = ToolApprovalSystem::new();
    let agent_id = uuid::Uuid::new_v4();

    // Request approval for file_write
    let approval = system.request_approval(
        agent_id,
        "file_write",
        serde_json::json!({"path": "src/main.rs", "content": "fn main() {}"}),
    );
    let id = approval.id;
    assert_eq!(approval.agent_id, agent_id);
    assert_eq!(approval.tool_name, "file_write");
    assert_eq!(approval.status, ApprovalStatus::Pending);

    // Should show up as pending
    assert_eq!(system.list_pending().len(), 1);

    // Approve it
    system.approve(id).unwrap();

    // Should no longer be pending
    assert!(system.list_pending().is_empty());

    // But should still exist in the full list
    assert_eq!(system.list_all().len(), 1);
    assert!(system.is_approved(id));
}

#[test]
fn approval_deny_prevents_execution() {
    let mut system = ToolApprovalSystem::new();
    let agent_id = uuid::Uuid::new_v4();

    let approval = system.request_approval(
        agent_id,
        "shell_execute",
        serde_json::json!({"cmd": "rm -rf /tmp/data"}),
    );
    let id = approval.id;

    // Deny it
    system.deny(id).unwrap();

    // Verify denied
    assert!(!system.is_approved(id));
    let approval = system.get_approval(id).unwrap();
    assert_eq!(approval.status, ApprovalStatus::Denied);
}

// ===========================================================================
// Executor command building tests
// ===========================================================================

#[test]
fn executor_build_prompt_includes_system_prompt() {
    let agent = CrewAgent::new();
    let system_prompt = agent.system_prompt();
    let pre_hook = agent.pre_execute("Build feature");

    // Verify the system prompt and pre-hook produce useful content
    assert!(system_prompt.len() > 100);
    assert!(pre_hook.is_some());

    // Simulate what execute_task_with_role would build
    let base_prompt = "Task: Build feature\nDescription: Some desc";
    let full_prompt = if let Some(preamble) = pre_hook {
        format!(
            "System: {}\n\n{}\n\n{}",
            system_prompt, preamble, base_prompt
        )
    } else {
        format!("System: {}\n\n{}", system_prompt, base_prompt)
    };

    assert!(full_prompt.contains("System:"));
    assert!(full_prompt.contains("worker") || full_prompt.contains("Crew"));
    assert!(full_prompt.contains("Build feature"));
}
