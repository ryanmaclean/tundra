//! Exhaustive tests for agent profiles, roles, phase configuration,
//! tool approval, and agent lifecycle (state machine + supervisor).

use at_agents::approval::{ApprovalPolicy, ToolApprovalSystem};
use at_agents::profiles::{AgentConfig, ThinkingLevel};
use at_agents::roles::{
    role_config_for, CrewAgent, DeaconAgent, MayorAgent, PolecatAgent, RefineryAgent, RoleConfig,
    WitnessAgent,
};
use at_agents::state_machine::{AgentEvent, AgentState, AgentStateMachine};
use at_agents::supervisor::AgentSupervisor;
use at_core::types::{AgentRole, CliType, TaskPhase};
use std::collections::HashMap;

// ===========================================================================
// Agent Role Definitions (matching MCP agent categories)
// ===========================================================================

#[test]
fn test_all_six_roles_defined() {
    // Verify all six roles exist and can be constructed
    let roles = [
        AgentRole::Mayor,
        AgentRole::Deacon,
        AgentRole::Witness,
        AgentRole::Refinery,
        AgentRole::Polecat,
        AgentRole::Crew,
    ];
    assert_eq!(roles.len(), 6);

    // Verify role_config_for works for each
    for role in &roles {
        let config = role_config_for(role);
        assert!(!config.system_prompt().is_empty());
    }
}

#[test]
fn test_mayor_role_has_system_prompt() {
    let agent = MayorAgent::new();
    let prompt = agent.system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.contains("Mayor"));
    assert!(prompt.contains("orchestrator"));
}

#[test]
fn test_deacon_role_has_system_prompt() {
    let agent = DeaconAgent::new();
    let prompt = agent.system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.contains("Deacon"));
    assert!(prompt.contains("code review"));
}

#[test]
fn test_witness_role_has_system_prompt() {
    let agent = WitnessAgent::new();
    let prompt = agent.system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.contains("Witness"));
    assert!(prompt.contains("test"));
}

#[test]
fn test_refinery_role_has_system_prompt() {
    let agent = RefineryAgent::new();
    let prompt = agent.system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.contains("Refinery"));
    assert!(prompt.contains("refactor"));
}

#[test]
fn test_role_display_names() {
    // Verify AgentRole serializes to expected snake_case names
    let pairs = [
        (AgentRole::Mayor, "\"mayor\""),
        (AgentRole::Deacon, "\"deacon\""),
        (AgentRole::Witness, "\"witness\""),
        (AgentRole::Refinery, "\"refinery\""),
        (AgentRole::Polecat, "\"polecat\""),
        (AgentRole::Crew, "\"crew\""),
    ];
    for (role, expected_json) in &pairs {
        let json = serde_json::to_string(role).unwrap();
        assert_eq!(
            &json, expected_json,
            "Role {:?} did not serialize as expected",
            role
        );
    }
}

#[test]
fn test_role_serialization_roundtrip() {
    let roles = [
        AgentRole::Mayor,
        AgentRole::Deacon,
        AgentRole::Witness,
        AgentRole::Refinery,
        AgentRole::Polecat,
        AgentRole::Crew,
    ];
    for role in &roles {
        let json = serde_json::to_string(role).unwrap();
        let deserialized: AgentRole = serde_json::from_str(&json).unwrap();
        assert_eq!(&deserialized, role);
    }
}

// ===========================================================================
// Agent role-specific configuration details
// ===========================================================================

#[test]
fn test_mayor_allowed_tools_include_orchestration() {
    let agent = MayorAgent::new();
    let tools = agent.allowed_tools();
    assert!(tools.contains(&"task_assign".to_string()));
    assert!(tools.contains(&"agent_spawn".to_string()));
    assert!(tools.contains(&"agent_stop".to_string()));
}

#[test]
fn test_deacon_allowed_tools_include_git_review() {
    let agent = DeaconAgent::new();
    let tools = agent.allowed_tools();
    assert!(tools.contains(&"git_diff".to_string()));
    assert!(tools.contains(&"git_blame".to_string()));
    // Deacon should NOT have write access
    assert!(!tools.contains(&"file_write".to_string()));
}

#[test]
fn test_crew_allowed_tools_include_write_and_git() {
    let agent = CrewAgent::new();
    let tools = agent.allowed_tools();
    assert!(tools.contains(&"file_write".to_string()));
    assert!(tools.contains(&"git_add".to_string()));
    assert!(tools.contains(&"git_commit".to_string()));
    assert!(tools.contains(&"shell_execute".to_string()));
}

#[test]
fn test_witness_allowed_tools_include_shell_execute() {
    let agent = WitnessAgent::new();
    let tools = agent.allowed_tools();
    assert!(tools.contains(&"shell_execute".to_string()));
    // Witness should NOT have write access
    assert!(!tools.contains(&"file_write".to_string()));
}

#[test]
fn test_refinery_allowed_tools_include_write_and_shell() {
    let agent = RefineryAgent::new();
    let tools = agent.allowed_tools();
    assert!(tools.contains(&"file_write".to_string()));
    assert!(tools.contains(&"shell_execute".to_string()));
}

#[test]
fn test_polecat_allowed_tools_include_security_scanning() {
    let agent = PolecatAgent::new();
    let tools = agent.allowed_tools();
    assert!(tools.contains(&"shell_execute".to_string()));
    assert!(tools.contains(&"git_log".to_string()));
    // Polecat should NOT have write access
    assert!(!tools.contains(&"file_write".to_string()));
}

#[test]
fn test_all_roles_have_preferred_model() {
    let agents: Vec<Box<dyn RoleConfig + Send + Sync>> = vec![
        Box::new(MayorAgent::new()),
        Box::new(DeaconAgent::new()),
        Box::new(WitnessAgent::new()),
        Box::new(RefineryAgent::new()),
        Box::new(PolecatAgent::new()),
        Box::new(CrewAgent::new()),
    ];
    for agent in &agents {
        let model = agent.preferred_model();
        assert!(model.is_some(), "Every role should have a preferred model");
        assert!(model.unwrap().contains("claude"));
    }
}

#[test]
fn test_role_max_turns_vary_by_role() {
    let mayor = MayorAgent::new();
    let deacon = DeaconAgent::new();
    let crew = CrewAgent::new();

    // Mayor has highest max_turns (orchestrator)
    assert!(mayor.max_turns() >= crew.max_turns());
    assert!(crew.max_turns() >= deacon.max_turns());
}

#[test]
fn test_mayor_pre_execute_contains_task() {
    let agent = MayorAgent::new();
    let result = agent.pre_execute("build a REST API");
    assert!(result.is_some());
    assert!(result.unwrap().contains("build a REST API"));
}

#[test]
fn test_deacon_post_execute_reports_issues() {
    let agent = DeaconAgent::new();

    // Output with warnings
    let result = agent.post_execute("[WARNING] unused variable\n[ERROR] missing return");
    assert!(result.is_some());
    assert!(result.unwrap().contains("2 issues"));

    // Clean output
    let result = agent.post_execute("All looks good");
    assert!(result.is_some());
    assert!(result.unwrap().contains("Approved"));
}

#[test]
fn test_witness_post_execute_detects_failures() {
    let agent = WitnessAgent::new();

    let result = agent.post_execute("test result: FAILED 3 tests");
    assert!(result.is_some());
    assert!(result.unwrap().contains("FAILURES"));

    let result = agent.post_execute("all 42 tests passed");
    assert!(result.is_some());
    assert!(result.unwrap().contains("passed"));
}

#[test]
fn test_polecat_post_execute_detects_critical() {
    let agent = PolecatAgent::new();

    let result = agent.post_execute("Found CRITICAL vulnerability CVE-2024-1234");
    assert!(result.is_some());
    assert!(result.unwrap().contains("BLOCK merge"));

    let result = agent.post_execute("Found HIGH severity issue");
    assert!(result.is_some());
    assert!(result.unwrap().contains("BLOCK merge"));

    let result = agent.post_execute("No issues found");
    assert!(result.is_some());
    assert!(result.unwrap().contains("no critical"));
}

// ===========================================================================
// Agent Profiles (matching 4 presets from screenshot: Auto Optimized,
// Balanced, Complex Tasks, Quick Edits)
// ===========================================================================

#[test]
fn test_default_profile_is_auto_optimized() {
    // Auto Optimized: Coding phase gets high thinking + generous timeout
    let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
    assert_eq!(config.thinking_level, ThinkingLevel::High);
    assert_eq!(config.timeout_secs, 600);
    assert_eq!(config.max_tokens, 32_000);
}

#[test]
fn test_profile_balanced_exists() {
    // Balanced: SpecCreation/Planning phase gets medium thinking
    let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Planning);
    assert_eq!(config.thinking_level, ThinkingLevel::Medium);
    assert_eq!(config.timeout_secs, 300);
    assert_eq!(config.max_tokens, 16_000);
}

#[test]
fn test_profile_complex_tasks_exists() {
    // Complex Tasks: Coding has highest thinking budget (50k tokens)
    let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
    let budget = config.thinking_level.budget_tokens();
    assert_eq!(budget, Some(50_000));
}

#[test]
fn test_profile_quick_edits_exists() {
    // Quick Edits: Discovery/Merging have low thinking + short timeout
    let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Discovery);
    assert_eq!(config.thinking_level, ThinkingLevel::Low);
    assert_eq!(config.timeout_secs, 120);
    assert_eq!(config.max_tokens, 8_000);
}

#[test]
fn test_profile_has_model_config() {
    let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
    assert!(!config.model.is_empty());
    assert!(config.model.contains("claude"));
}

#[test]
fn test_profile_has_thinking_level() {
    // Verify all ThinkingLevel variants have correct budget_tokens
    assert_eq!(ThinkingLevel::None.budget_tokens(), None);
    assert_eq!(ThinkingLevel::Low.budget_tokens(), Some(5_000));
    assert_eq!(ThinkingLevel::Medium.budget_tokens(), Some(10_000));
    assert_eq!(ThinkingLevel::High.budget_tokens(), Some(50_000));
}

#[test]
fn test_profile_serialization_roundtrip() {
    let config = AgentConfig {
        cli_type: CliType::Claude,
        model: "claude-sonnet-4-20250514".to_string(),
        thinking_level: ThinkingLevel::High,
        max_tokens: 32_000,
        timeout_secs: 600,
        env_vars: HashMap::from([("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string())]),
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: AgentConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.model, config.model);
    assert_eq!(deserialized.thinking_level, config.thinking_level);
    assert_eq!(deserialized.max_tokens, config.max_tokens);
    assert_eq!(deserialized.timeout_secs, config.timeout_secs);
    assert_eq!(
        deserialized.env_vars.get("ANTHROPIC_API_KEY").unwrap(),
        "sk-test"
    );
}

// ===========================================================================
// Phase Configuration (Open Creation, Implementation, Code Review)
// ===========================================================================

#[test]
fn test_phase_config_has_open_creation() {
    // Open Creation maps to SpecCreation phase
    let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::SpecCreation);
    assert_eq!(config.thinking_level, ThinkingLevel::Medium);
    assert!(config.max_tokens > 0);
}

#[test]
fn test_phase_config_has_implementation() {
    // Implementation maps to Coding phase
    let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
    assert_eq!(config.thinking_level, ThinkingLevel::High);
    assert!(config.timeout_secs > 0);
}

#[test]
fn test_phase_config_has_code_review() {
    // Code Review maps to Qa phase
    let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Qa);
    assert_eq!(config.thinking_level, ThinkingLevel::Medium);
}

#[test]
fn test_phase_config_model_per_phase() {
    // Each phase uses the appropriate default model for its CLI type
    let phases = [
        TaskPhase::Discovery,
        TaskPhase::ContextGathering,
        TaskPhase::SpecCreation,
        TaskPhase::Planning,
        TaskPhase::Coding,
        TaskPhase::Qa,
        TaskPhase::Fixing,
        TaskPhase::Merging,
    ];
    for phase in &phases {
        let config = AgentConfig::default_for_phase(CliType::Claude, phase.clone());
        assert!(
            config.model.contains("claude"),
            "Phase {:?} should use Claude model, got {}",
            phase,
            config.model
        );
    }

    // Different CLI types use different models
    let codex_config = AgentConfig::default_for_phase(CliType::Codex, TaskPhase::Coding);
    assert!(codex_config.model.contains("o3"));

    let gemini_config = AgentConfig::default_for_phase(CliType::Gemini, TaskPhase::Coding);
    assert!(gemini_config.model.contains("gemini"));
}

#[test]
fn test_phase_config_thinking_level_per_phase() {
    // Terminal states use None thinking
    let complete = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Complete);
    assert_eq!(complete.thinking_level, ThinkingLevel::None);

    let error = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Error);
    assert_eq!(error.thinking_level, ThinkingLevel::None);

    let stopped = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Stopped);
    assert_eq!(stopped.thinking_level, ThinkingLevel::None);

    // Active phases use non-None thinking
    let coding = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
    assert_ne!(coding.thinking_level, ThinkingLevel::None);

    let qa = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Qa);
    assert_ne!(qa.thinking_level, ThinkingLevel::None);
}

#[test]
fn test_phase_config_cli_args_generation() {
    let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
    let args = config.to_cli_args();
    assert!(args.contains(&"--model".to_string()));
    assert!(args.contains(&"--print".to_string()));
    assert!(args.contains(&"--thinking-budget".to_string()));
    assert!(args.contains(&"50000".to_string()));
}

#[test]
fn test_phase_config_binary_names() {
    assert_eq!(
        AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding).binary_name(),
        "claude"
    );
    assert_eq!(
        AgentConfig::default_for_phase(CliType::Codex, TaskPhase::Coding).binary_name(),
        "codex"
    );
    assert_eq!(
        AgentConfig::default_for_phase(CliType::Gemini, TaskPhase::Coding).binary_name(),
        "gemini"
    );
    assert_eq!(
        AgentConfig::default_for_phase(CliType::OpenCode, TaskPhase::Coding).binary_name(),
        "opencode"
    );
}

// ===========================================================================
// Tool Approval System
// ===========================================================================

#[test]
fn test_auto_approve_policy() {
    let system = ToolApprovalSystem::new();
    // Read-only tools are auto-approved
    assert_eq!(
        system.check_approval("file_read", &AgentRole::Crew),
        ApprovalPolicy::AutoApprove
    );
    assert_eq!(
        system.check_approval("list_directory", &AgentRole::Mayor),
        ApprovalPolicy::AutoApprove
    );
    assert_eq!(
        system.check_approval("search_files", &AgentRole::Deacon),
        ApprovalPolicy::AutoApprove
    );
    assert_eq!(
        system.check_approval("git_diff", &AgentRole::Witness),
        ApprovalPolicy::AutoApprove
    );
    assert_eq!(
        system.check_approval("git_log", &AgentRole::Polecat),
        ApprovalPolicy::AutoApprove
    );
}

#[test]
fn test_require_approval_policy() {
    let system = ToolApprovalSystem::new();
    // Write/execute tools require approval
    assert_eq!(
        system.check_approval("file_write", &AgentRole::Crew),
        ApprovalPolicy::RequireApproval
    );
    assert_eq!(
        system.check_approval("shell_execute", &AgentRole::Witness),
        ApprovalPolicy::RequireApproval
    );
    assert_eq!(
        system.check_approval("git_push", &AgentRole::Crew),
        ApprovalPolicy::RequireApproval
    );
    assert_eq!(
        system.check_approval("agent_spawn", &AgentRole::Mayor),
        ApprovalPolicy::RequireApproval
    );
}

#[test]
fn test_deny_policy() {
    let system = ToolApprovalSystem::new();
    // Destructive tools are denied
    assert_eq!(
        system.check_approval("delete", &AgentRole::Crew),
        ApprovalPolicy::Deny
    );
    assert_eq!(
        system.check_approval("file_delete", &AgentRole::Mayor),
        ApprovalPolicy::Deny
    );
    assert_eq!(
        system.check_approval("force_push", &AgentRole::Crew),
        ApprovalPolicy::Deny
    );
}

#[test]
fn test_approval_check_returns_correct_action() {
    let system = ToolApprovalSystem::new();

    // Unknown tools default to RequireApproval
    assert_eq!(
        system.check_approval("custom_tool_xyz", &AgentRole::Crew),
        ApprovalPolicy::RequireApproval
    );

    // Custom policy override
    let mut system2 = ToolApprovalSystem::new();
    system2.set_policy("custom_tool_xyz", ApprovalPolicy::AutoApprove);
    assert_eq!(
        system2.check_approval("custom_tool_xyz", &AgentRole::Crew),
        ApprovalPolicy::AutoApprove
    );
}

#[test]
fn test_approval_store_records_decisions() {
    let mut system = ToolApprovalSystem::new();
    let agent_id = uuid::Uuid::new_v4();

    // Request approval
    let approval_id = system
        .request_approval(
            agent_id,
            "file_write",
            serde_json::json!({"path": "test.rs"}),
        )
        .id;

    // Verify pending
    assert_eq!(system.list_pending().len(), 1);
    assert_eq!(system.list_all().len(), 1);
    assert!(!system.is_approved(approval_id));

    // Approve
    system.approve(approval_id).unwrap();
    assert!(system.is_approved(approval_id));
    assert!(system.list_pending().is_empty());
    assert_eq!(system.list_all().len(), 1);

    // Verify resolved_at is set
    let approval = system.get_approval(approval_id).unwrap();
    assert!(approval.resolved_at.is_some());

    // Double-approve fails
    assert!(system.approve(approval_id).is_err());
}

#[test]
fn test_approval_deny_flow() {
    let mut system = ToolApprovalSystem::new();
    let agent_id = uuid::Uuid::new_v4();

    let approval_id = system
        .request_approval(
            agent_id,
            "shell_execute",
            serde_json::json!({"cmd": "rm -rf /"}),
        )
        .id;

    system.deny(approval_id).unwrap();
    assert!(!system.is_approved(approval_id));

    let approval = system.get_approval(approval_id).unwrap();
    assert_eq!(approval.status, at_agents::approval::ApprovalStatus::Denied);
    assert!(approval.resolved_at.is_some());

    // Double-deny fails
    assert!(system.deny(approval_id).is_err());
}

#[test]
fn test_approval_policies_per_role() {
    let mut system = ToolApprovalSystem::new();

    // Give Mayor auto-approve for task_assign
    system.set_role_override("task_assign", AgentRole::Mayor, ApprovalPolicy::AutoApprove);

    // Mayor gets auto-approve
    assert_eq!(
        system.check_approval("task_assign", &AgentRole::Mayor),
        ApprovalPolicy::AutoApprove
    );

    // Other roles still require approval
    assert_eq!(
        system.check_approval("task_assign", &AgentRole::Crew),
        ApprovalPolicy::RequireApproval
    );
    assert_eq!(
        system.check_approval("task_assign", &AgentRole::Deacon),
        ApprovalPolicy::RequireApproval
    );

    // Add another role-specific override
    system.set_role_override(
        "shell_execute",
        AgentRole::Witness,
        ApprovalPolicy::AutoApprove,
    );
    assert_eq!(
        system.check_approval("shell_execute", &AgentRole::Witness),
        ApprovalPolicy::AutoApprove
    );
    assert_eq!(
        system.check_approval("shell_execute", &AgentRole::Crew),
        ApprovalPolicy::RequireApproval
    );
}

#[test]
fn test_approval_nonexistent_id() {
    let mut system = ToolApprovalSystem::new();
    let fake_id = uuid::Uuid::new_v4();
    assert!(system.approve(fake_id).is_err());
    assert!(system.deny(fake_id).is_err());
    assert!(system.get_approval(fake_id).is_none());
    assert!(!system.is_approved(fake_id));
}

#[test]
fn test_approval_multiple_pending() {
    let mut system = ToolApprovalSystem::new();
    let agent_id = uuid::Uuid::new_v4();

    let id1 = system
        .request_approval(agent_id, "file_write", serde_json::json!({}))
        .id;
    let id2 = system
        .request_approval(agent_id, "shell_execute", serde_json::json!({}))
        .id;
    let _id3 = system
        .request_approval(agent_id, "git_push", serde_json::json!({}))
        .id;

    assert_eq!(system.list_pending().len(), 3);

    system.approve(id1).unwrap();
    assert_eq!(system.list_pending().len(), 2);

    system.deny(id2).unwrap();
    assert_eq!(system.list_pending().len(), 1);

    assert_eq!(system.list_all().len(), 3);
}

#[test]
fn test_approval_policy_serialization_roundtrip() {
    let policies = [
        ApprovalPolicy::AutoApprove,
        ApprovalPolicy::RequireApproval,
        ApprovalPolicy::Deny,
    ];
    for policy in &policies {
        let json = serde_json::to_string(policy).unwrap();
        let deserialized: ApprovalPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(&deserialized, policy);
    }
}

#[test]
fn test_permissive_approval_system() {
    let system = ToolApprovalSystem::permissive();
    // No policies set so everything falls back to RequireApproval
    assert_eq!(
        system.check_approval("anything", &AgentRole::Crew),
        ApprovalPolicy::RequireApproval
    );
}

// ===========================================================================
// Agent Lifecycle — State Machine
// ===========================================================================

#[test]
fn test_agent_state_idle_to_spawning() {
    let mut sm = AgentStateMachine::new();
    assert_eq!(sm.state(), AgentState::Idle);

    let next = sm.transition(AgentEvent::Start).unwrap();
    assert_eq!(next, AgentState::Spawning);
    assert_eq!(sm.state(), AgentState::Spawning);
}

#[test]
fn test_agent_state_spawning_to_running() {
    let mut sm = AgentStateMachine::new();
    sm.transition(AgentEvent::Start).unwrap(); // Idle -> Spawning
    let next = sm.transition(AgentEvent::Spawned).unwrap(); // Spawning -> Active
    assert_eq!(next, AgentState::Active);
    assert_eq!(sm.state(), AgentState::Active);
}

#[test]
fn test_agent_state_running_to_stopped() {
    let mut sm = AgentStateMachine::new();
    sm.transition(AgentEvent::Start).unwrap(); // Idle -> Spawning
    sm.transition(AgentEvent::Spawned).unwrap(); // Spawning -> Active
    sm.transition(AgentEvent::Stop).unwrap(); // Active -> Stopping
    let next = sm.transition(AgentEvent::Stop).unwrap(); // Stopping -> Stopped
    assert_eq!(next, AgentState::Stopped);
    assert_eq!(sm.state(), AgentState::Stopped);
}

#[test]
fn test_agent_state_invalid_transition() {
    let mut sm = AgentStateMachine::new();
    // Cannot Spawn directly from Idle (need Start first)
    let result = sm.transition(AgentEvent::Spawned);
    assert!(result.is_err());

    // Cannot Stop from Idle
    let result = sm.transition(AgentEvent::Stop);
    assert!(result.is_err());

    // Cannot Pause from Idle
    let result = sm.transition(AgentEvent::Pause);
    assert!(result.is_err());
}

#[test]
fn test_agent_state_pause_resume_cycle() {
    let mut sm = AgentStateMachine::new();
    sm.transition(AgentEvent::Start).unwrap();
    sm.transition(AgentEvent::Spawned).unwrap();

    // Active -> Paused
    sm.transition(AgentEvent::Pause).unwrap();
    assert_eq!(sm.state(), AgentState::Paused);

    // Paused -> Active
    sm.transition(AgentEvent::Resume).unwrap();
    assert_eq!(sm.state(), AgentState::Active);
}

#[test]
fn test_agent_state_failure_and_recovery() {
    let mut sm = AgentStateMachine::new();
    sm.transition(AgentEvent::Start).unwrap();
    sm.transition(AgentEvent::Spawned).unwrap();

    // Active -> Failed
    sm.transition(AgentEvent::Fail).unwrap();
    assert_eq!(sm.state(), AgentState::Failed);

    // Failed -> Idle (Recover)
    sm.transition(AgentEvent::Recover).unwrap();
    assert_eq!(sm.state(), AgentState::Idle);
}

#[test]
fn test_agent_state_can_transition() {
    let sm = AgentStateMachine::new();
    assert!(sm.can_transition(AgentEvent::Start));
    assert!(!sm.can_transition(AgentEvent::Spawned));
    assert!(!sm.can_transition(AgentEvent::Stop));
    assert!(!sm.can_transition(AgentEvent::Pause));
}

#[test]
fn test_agent_state_history_tracking() {
    let mut sm = AgentStateMachine::new();
    sm.transition(AgentEvent::Start).unwrap();
    sm.transition(AgentEvent::Spawned).unwrap();
    sm.transition(AgentEvent::Stop).unwrap();

    let history = sm.history();
    assert_eq!(history.len(), 3);
    assert_eq!(
        history[0],
        (AgentState::Idle, AgentEvent::Start, AgentState::Spawning)
    );
    assert_eq!(
        history[1],
        (
            AgentState::Spawning,
            AgentEvent::Spawned,
            AgentState::Active
        )
    );
    assert_eq!(
        history[2],
        (AgentState::Active, AgentEvent::Stop, AgentState::Stopping)
    );
}

#[test]
fn test_agent_state_display() {
    assert_eq!(AgentState::Idle.to_string(), "Idle");
    assert_eq!(AgentState::Spawning.to_string(), "Spawning");
    assert_eq!(AgentState::Active.to_string(), "Active");
    assert_eq!(AgentState::Paused.to_string(), "Paused");
    assert_eq!(AgentState::Stopping.to_string(), "Stopping");
    assert_eq!(AgentState::Stopped.to_string(), "Stopped");
    assert_eq!(AgentState::Failed.to_string(), "Failed");
}

#[test]
fn test_agent_event_display() {
    assert_eq!(AgentEvent::Start.to_string(), "Start");
    assert_eq!(AgentEvent::Spawned.to_string(), "Spawned");
    assert_eq!(AgentEvent::Pause.to_string(), "Pause");
    assert_eq!(AgentEvent::Resume.to_string(), "Resume");
    assert_eq!(AgentEvent::Stop.to_string(), "Stop");
    assert_eq!(AgentEvent::Fail.to_string(), "Fail");
    assert_eq!(AgentEvent::Recover.to_string(), "Recover");
}

#[test]
fn test_agent_state_serialization_roundtrip() {
    let states = [
        AgentState::Idle,
        AgentState::Spawning,
        AgentState::Active,
        AgentState::Paused,
        AgentState::Stopping,
        AgentState::Stopped,
        AgentState::Failed,
    ];
    for state in &states {
        let json = serde_json::to_string(state).unwrap();
        let deserialized: AgentState = serde_json::from_str(&json).unwrap();
        assert_eq!(&deserialized, state);
    }
}

// ===========================================================================
// Agent Supervisor
// ===========================================================================

#[tokio::test]
async fn test_agent_supervisor_spawn() {
    let supervisor = AgentSupervisor::new();
    let id = supervisor
        .spawn_agent("test-mayor", AgentRole::Mayor, CliType::Claude)
        .await
        .unwrap();

    assert_eq!(supervisor.agent_count().await, 1);

    let agents = supervisor.list_agents().await;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, id);
    assert_eq!(agents[0].name, "test-mayor");
    assert_eq!(agents[0].role, AgentRole::Mayor);
    assert_eq!(agents[0].state, AgentState::Active);
}

#[tokio::test]
async fn test_agent_supervisor_list() {
    let supervisor = AgentSupervisor::new();

    supervisor
        .spawn_agent("mayor-1", AgentRole::Mayor, CliType::Claude)
        .await
        .unwrap();
    supervisor
        .spawn_agent("crew-1", AgentRole::Crew, CliType::Claude)
        .await
        .unwrap();
    supervisor
        .spawn_agent("deacon-1", AgentRole::Deacon, CliType::Claude)
        .await
        .unwrap();

    let agents = supervisor.list_agents().await;
    assert_eq!(agents.len(), 3);

    let roles: Vec<AgentRole> = agents.iter().map(|a| a.role.clone()).collect();
    assert!(roles.contains(&AgentRole::Mayor));
    assert!(roles.contains(&AgentRole::Crew));
    assert!(roles.contains(&AgentRole::Deacon));
}

#[tokio::test]
async fn test_agent_supervisor_stop() {
    let supervisor = AgentSupervisor::new();
    let id = supervisor
        .spawn_agent("crew-stop", AgentRole::Crew, CliType::Claude)
        .await
        .unwrap();

    supervisor.stop_agent(id).await.unwrap();

    let agents = supervisor.list_agents().await;
    assert_eq!(agents[0].state, AgentState::Stopped);
}

#[tokio::test]
async fn test_agent_supervisor_stop_nonexistent() {
    let supervisor = AgentSupervisor::new();
    let result = supervisor.stop_agent(uuid::Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_agent_supervisor_heartbeat() {
    let supervisor = AgentSupervisor::new();
    supervisor
        .spawn_agent("crew-hb", AgentRole::Crew, CliType::Claude)
        .await
        .unwrap();

    // Should not error
    supervisor.send_heartbeat_all().await.unwrap();
}

#[tokio::test]
async fn test_agent_supervisor_spawn_all_roles() {
    let supervisor = AgentSupervisor::new();
    let roles = [
        ("m", AgentRole::Mayor),
        ("d", AgentRole::Deacon),
        ("w", AgentRole::Witness),
        ("r", AgentRole::Refinery),
        ("p", AgentRole::Polecat),
        ("c", AgentRole::Crew),
    ];
    for (name, role) in &roles {
        supervisor
            .spawn_agent(*name, role.clone(), CliType::Claude)
            .await
            .unwrap();
    }
    assert_eq!(supervisor.agent_count().await, 6);
}

// ===========================================================================
// Agent Default trait implementations
// ===========================================================================

#[test]
fn test_agent_defaults() {
    let mayor = MayorAgent::default();
    assert_eq!(mayor.queue_len(), 0);

    let deacon = DeaconAgent::default();
    assert_eq!(deacon.checks_performed(), 0);

    let witness = WitnessAgent::default();
    assert_eq!(witness.events_observed(), 0);

    let refinery = RefineryAgent::default();
    assert_eq!(refinery.runs_completed(), 0);

    let polecat = PolecatAgent::default();
    assert_eq!(polecat.branches_managed(), 0);

    let crew = CrewAgent::default();
    assert_eq!(crew.beads_executed(), 0);
}

// ===========================================================================
// ThinkingLevel serialization
// ===========================================================================

#[test]
fn test_thinking_level_serialization_roundtrip() {
    let levels = [
        ThinkingLevel::None,
        ThinkingLevel::Low,
        ThinkingLevel::Medium,
        ThinkingLevel::High,
    ];
    for level in &levels {
        let json = serde_json::to_string(level).unwrap();
        let deserialized: ThinkingLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(&deserialized, level);
    }
}
