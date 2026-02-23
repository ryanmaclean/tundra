use at_core::types::{AgentRole, Bead};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use uuid::Uuid;

use crate::lifecycle::{AgentLifecycle, Result};

// ---------------------------------------------------------------------------
// RoleConfig — defines the execution profile for each agent role
// ---------------------------------------------------------------------------

/// Trait that provides role-specific configuration for agent execution.
/// Each role defines its own system prompt, allowed tools, turn limits,
/// and pre/post execution hooks.
pub trait RoleConfig {
    /// Return a detailed system prompt appropriate for this role.
    fn system_prompt(&self) -> &str;

    /// Return the list of tool names this role is allowed to invoke.
    fn allowed_tools(&self) -> Vec<String>;

    /// Return the maximum number of turns this agent may take per task.
    fn max_turns(&self) -> u32;

    /// Pre-execution hook: called before the agent begins processing a task.
    /// Returns an optional preamble string to prepend to the task prompt.
    fn pre_execute(&self, task_description: &str) -> Option<String> {
        let _ = task_description;
        None
    }

    /// Post-execution hook: called after execution completes.
    /// Returns an optional summary or cleanup instruction.
    fn post_execute(&self, output: &str) -> Option<String> {
        let _ = output;
        None
    }

    /// Return the preferred model identifier for this role, if any.
    fn preferred_model(&self) -> Option<&str> {
        None
    }
}

// ---------------------------------------------------------------------------
// PrioritizedBead — used by MayorAgent's priority queue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct PrioritizedBead {
    priority: i32,
    bead_id: Uuid,
}

impl PartialEq for PrioritizedBead {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.bead_id == other.bead_id
    }
}

impl Eq for PrioritizedBead {}

impl PartialOrd for PrioritizedBead {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedBead {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority)
    }
}

// ===========================================================================
// MayorAgent — orchestrator, assigns work, manages convoys
// ===========================================================================

const MAYOR_SYSTEM_PROMPT: &str = "\
You are the Mayor agent, the top-level orchestrator for a software engineering convoy. \
Your responsibilities include:
- Decomposing high-level tasks into smaller work units (beads) for other agents.
- Assigning beads to the appropriate specialist agents (Deacon, Witness, Refinery, Polecat, Crew).
- Monitoring progress across all active agents and re-prioritizing work as needed.
- Resolving conflicts when multiple agents produce contradictory results.
- Ensuring the overall task pipeline moves from Discovery through to Merging/Complete.
- Summarizing status for the human operator when requested.

You must NOT write code directly. Delegate implementation work to Crew agents. \
Delegate code review to Deacon, testing to Witness, refactoring to Refinery, \
and security scanning to Polecat.

Always output structured JSON events when assigning or re-prioritizing work.";

pub struct MayorAgent {
    queue: BinaryHeap<PrioritizedBead>,
}

impl MayorAgent {
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
        }
    }

    /// Number of beads in the priority queue.
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }
}

impl Default for MayorAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl RoleConfig for MayorAgent {
    fn system_prompt(&self) -> &str {
        MAYOR_SYSTEM_PROMPT
    }

    fn allowed_tools(&self) -> Vec<String> {
        vec![
            "file_read".to_string(),
            "list_directory".to_string(),
            "search_files".to_string(),
            "task_assign".to_string(),
            "task_status".to_string(),
            "agent_spawn".to_string(),
            "agent_stop".to_string(),
        ]
    }

    fn max_turns(&self) -> u32 {
        100
    }

    fn pre_execute(&self, task_description: &str) -> Option<String> {
        Some(format!(
            "As the Mayor, analyze this task and produce a decomposition plan before delegating:\n{}",
            task_description
        ))
    }

    fn post_execute(&self, output: &str) -> Option<String> {
        if output.contains("error") || output.contains("ERROR") {
            Some("Review the errors reported and consider reassigning failed beads.".to_string())
        } else {
            Some("Delegation complete. Monitor agent progress.".to_string())
        }
    }

    fn preferred_model(&self) -> Option<&str> {
        Some("claude-sonnet-4-20250514")
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for MayorAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Mayor
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("MayorAgent started — ready to orchestrate");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, "MayorAgent received bead, enqueuing");
        self.queue.push(PrioritizedBead {
            priority: bead.priority,
            bead_id: bead.id,
        });
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "MayorAgent noted bead completion");
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(queue_len = self.queue.len(), "MayorAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("MayorAgent stopping");
        Ok(())
    }
}

// ===========================================================================
// DeaconAgent — code reviewer, quality gatekeeper
// ===========================================================================

const DEACON_SYSTEM_PROMPT: &str = "\
You are the Deacon agent, a meticulous code reviewer and quality gatekeeper. \
Your responsibilities include:
- Reviewing code changes for correctness, style, and adherence to project conventions.
- Identifying potential bugs, logic errors, and edge cases.
- Checking that new code has appropriate test coverage.
- Verifying documentation is updated when interfaces change.
- Providing actionable, specific feedback with file paths and line references.
- Approving or rejecting changes with a clear rationale.

You must NOT modify code directly. Provide review comments and let the Crew or \
Refinery agents make the changes. Focus on finding issues, not fixing them.

Output structured review events with severity (info, warning, error) for each finding.";

pub struct DeaconAgent {
    checks_performed: u64,
}

impl DeaconAgent {
    pub fn new() -> Self {
        Self {
            checks_performed: 0,
        }
    }

    pub fn checks_performed(&self) -> u64 {
        self.checks_performed
    }
}

impl Default for DeaconAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl RoleConfig for DeaconAgent {
    fn system_prompt(&self) -> &str {
        DEACON_SYSTEM_PROMPT
    }

    fn allowed_tools(&self) -> Vec<String> {
        vec![
            "file_read".to_string(),
            "list_directory".to_string(),
            "search_files".to_string(),
            "git_diff".to_string(),
            "git_log".to_string(),
            "git_blame".to_string(),
        ]
    }

    fn max_turns(&self) -> u32 {
        30
    }

    fn pre_execute(&self, task_description: &str) -> Option<String> {
        Some(format!(
            "Review the following changes carefully. Check for correctness, style, and test coverage:\n{}",
            task_description
        ))
    }

    fn post_execute(&self, output: &str) -> Option<String> {
        let issue_count = output.matches("[WARNING]").count() + output.matches("[ERROR]").count();
        if issue_count > 0 {
            Some(format!(
                "Review complete: found {} issues requiring attention.",
                issue_count
            ))
        } else {
            Some("Review complete: no issues found. Approved.".to_string())
        }
    }

    fn preferred_model(&self) -> Option<&str> {
        Some("claude-sonnet-4-20250514")
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for DeaconAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Deacon
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("DeaconAgent started — code review mode active");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, "DeaconAgent assigned review bead");
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "DeaconAgent completed review");
        self.checks_performed += 1;
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(checks = self.checks_performed, "DeaconAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("DeaconAgent stopping");
        Ok(())
    }
}

// ===========================================================================
// WitnessAgent — test runner, verification, audit trail
// ===========================================================================

const WITNESS_SYSTEM_PROMPT: &str = "\
You are the Witness agent, responsible for running tests and verifying correctness. \
Your responsibilities include:
- Running the project test suite (unit tests, integration tests, end-to-end tests).
- Analyzing test output to identify failures, flaky tests, and regressions.
- Verifying that new code changes do not break existing functionality.
- Generating test coverage reports and identifying untested code paths.
- Maintaining an audit trail of all test runs and their results.
- Reporting structured test results with pass/fail counts and failure details.

You may execute shell commands to run tests but must NOT modify source code. \
If tests fail, report the failures clearly so Crew or Refinery agents can fix them.

Output structured test result events with pass count, fail count, and failure details.";

pub struct WitnessAgent {
    events_observed: u64,
}

impl WitnessAgent {
    pub fn new() -> Self {
        Self { events_observed: 0 }
    }

    pub fn events_observed(&self) -> u64 {
        self.events_observed
    }
}

impl Default for WitnessAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl RoleConfig for WitnessAgent {
    fn system_prompt(&self) -> &str {
        WITNESS_SYSTEM_PROMPT
    }

    fn allowed_tools(&self) -> Vec<String> {
        vec![
            "file_read".to_string(),
            "list_directory".to_string(),
            "search_files".to_string(),
            "shell_execute".to_string(),
            "git_diff".to_string(),
        ]
    }

    fn max_turns(&self) -> u32 {
        50
    }

    fn pre_execute(&self, task_description: &str) -> Option<String> {
        Some(format!(
            "Run all relevant tests for the following changes and report results:\n{}",
            task_description
        ))
    }

    fn post_execute(&self, output: &str) -> Option<String> {
        let has_failures = output.contains("FAILED") || output.contains("failures:");
        if has_failures {
            Some("Test run completed with FAILURES. See details above.".to_string())
        } else {
            Some("All tests passed successfully.".to_string())
        }
    }

    fn preferred_model(&self) -> Option<&str> {
        Some("claude-sonnet-4-20250514")
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for WitnessAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Witness
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("WitnessAgent started — test runner active");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, "WitnessAgent assigned test bead");
        self.events_observed += 1;
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "WitnessAgent recorded test completion");
        self.events_observed += 1;
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(events = self.events_observed, "WitnessAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("WitnessAgent stopping — audit trail sealed");
        Ok(())
    }
}

// ===========================================================================
// RefineryAgent — refactorer, code quality improver
// ===========================================================================

const REFINERY_SYSTEM_PROMPT: &str = "\
You are the Refinery agent, responsible for code refactoring and quality improvement. \
Your responsibilities include:
- Refactoring code to improve readability, maintainability, and performance.
- Running linters (clippy, rustfmt) and fixing style violations.
- Reducing code duplication by extracting common patterns.
- Improving type safety and error handling.
- Simplifying complex functions and reducing cyclomatic complexity.
- Ensuring consistent naming conventions and module organization.

You may read and write files to perform refactoring. You should run the formatter \
and linter after making changes to ensure compliance. Do NOT add new features; \
focus purely on improving existing code structure.

Output structured events describing each refactoring action taken.";

pub struct RefineryAgent {
    runs_completed: u64,
}

impl RefineryAgent {
    pub fn new() -> Self {
        Self { runs_completed: 0 }
    }

    pub fn runs_completed(&self) -> u64 {
        self.runs_completed
    }
}

impl Default for RefineryAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl RoleConfig for RefineryAgent {
    fn system_prompt(&self) -> &str {
        REFINERY_SYSTEM_PROMPT
    }

    fn allowed_tools(&self) -> Vec<String> {
        vec![
            "file_read".to_string(),
            "file_write".to_string(),
            "list_directory".to_string(),
            "search_files".to_string(),
            "shell_execute".to_string(),
            "git_diff".to_string(),
        ]
    }

    fn max_turns(&self) -> u32 {
        40
    }

    fn pre_execute(&self, task_description: &str) -> Option<String> {
        Some(format!(
            "Refactor the following code. Focus on readability and maintainability. \
             Run clippy and rustfmt after changes:\n{}",
            task_description
        ))
    }

    fn post_execute(&self, output: &str) -> Option<String> {
        if output.contains("warning") || output.contains("clippy") {
            Some("Refactoring complete but warnings remain. Consider a follow-up pass.".to_string())
        } else {
            Some("Refactoring complete. Code is clean.".to_string())
        }
    }

    fn preferred_model(&self) -> Option<&str> {
        Some("claude-sonnet-4-20250514")
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for RefineryAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Refinery
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("RefineryAgent started — quality gates armed");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, "RefineryAgent queued refactoring run");
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "RefineryAgent refactoring run complete");
        self.runs_completed += 1;
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(runs = self.runs_completed, "RefineryAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("RefineryAgent stopping");
        Ok(())
    }
}

// ===========================================================================
// PolecatAgent — security scanner, vulnerability detection
// ===========================================================================

const POLECAT_SYSTEM_PROMPT: &str = "\
You are the Polecat agent, responsible for security scanning and vulnerability detection. \
Your responsibilities include:
- Scanning code for common security vulnerabilities (injection, XSS, CSRF, etc.).
- Checking dependencies for known CVEs using cargo-audit or similar tools.
- Reviewing authentication and authorization logic for flaws.
- Identifying hardcoded secrets, credentials, or API keys in the codebase.
- Verifying that sensitive data is properly sanitized and encrypted.
- Checking git history for accidentally committed secrets.
- Managing git worktrees and branch isolation for secure testing.

You may read files and execute security scanning tools, but must NOT modify source code. \
Report all findings with severity levels (low, medium, high, critical).

Output structured security finding events with CVE references where applicable.";

pub struct PolecatAgent {
    branches_managed: u64,
}

impl PolecatAgent {
    pub fn new() -> Self {
        Self {
            branches_managed: 0,
        }
    }

    pub fn branches_managed(&self) -> u64 {
        self.branches_managed
    }
}

impl Default for PolecatAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl RoleConfig for PolecatAgent {
    fn system_prompt(&self) -> &str {
        POLECAT_SYSTEM_PROMPT
    }

    fn allowed_tools(&self) -> Vec<String> {
        vec![
            "file_read".to_string(),
            "list_directory".to_string(),
            "search_files".to_string(),
            "shell_execute".to_string(),
            "git_diff".to_string(),
            "git_log".to_string(),
        ]
    }

    fn max_turns(&self) -> u32 {
        30
    }

    fn pre_execute(&self, task_description: &str) -> Option<String> {
        Some(format!(
            "Perform a security scan of the following changes. Check for vulnerabilities, \
             hardcoded secrets, and dependency issues:\n{}",
            task_description
        ))
    }

    fn post_execute(&self, output: &str) -> Option<String> {
        let critical = output.matches("CRITICAL").count();
        let high = output.matches("HIGH").count();
        if critical > 0 || high > 0 {
            Some(format!(
                "Security scan complete: {} critical, {} high severity findings. BLOCK merge.",
                critical, high
            ))
        } else {
            Some("Security scan complete: no critical or high severity findings.".to_string())
        }
    }

    fn preferred_model(&self) -> Option<&str> {
        Some("claude-sonnet-4-20250514")
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for PolecatAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Polecat
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("PolecatAgent started — security scanner online");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, branch = ?bead.git_branch, "PolecatAgent scanning");
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "PolecatAgent scan complete");
        self.branches_managed += 1;
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(branches = self.branches_managed, "PolecatAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("PolecatAgent stopping — cleaning up worktrees");
        Ok(())
    }
}

// ===========================================================================
// CrewAgent — general worker, implements code changes
// ===========================================================================

const CREW_SYSTEM_PROMPT: &str = "\
You are a Crew agent, a general-purpose software engineering worker. \
Your responsibilities include:
- Implementing code changes as specified in your assigned bead.
- Writing new functions, modules, and types as needed.
- Adding appropriate error handling and documentation.
- Writing unit tests for new code you produce.
- Following the project's existing coding conventions and patterns.
- Committing changes with clear, descriptive commit messages.

You have full read/write access to the codebase and can execute shell commands \
for building and running tests. Follow the plan provided by the Mayor agent. \
If you encounter ambiguity, prefer the simplest correct solution.

Output structured events for each significant action (file created, file modified, \
test added, build result).";

pub struct CrewAgent {
    beads_executed: u64,
}

impl CrewAgent {
    pub fn new() -> Self {
        Self { beads_executed: 0 }
    }

    pub fn beads_executed(&self) -> u64 {
        self.beads_executed
    }
}

impl Default for CrewAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl RoleConfig for CrewAgent {
    fn system_prompt(&self) -> &str {
        CREW_SYSTEM_PROMPT
    }

    fn allowed_tools(&self) -> Vec<String> {
        vec![
            "file_read".to_string(),
            "file_write".to_string(),
            "list_directory".to_string(),
            "search_files".to_string(),
            "shell_execute".to_string(),
            "git_diff".to_string(),
            "git_add".to_string(),
            "git_commit".to_string(),
        ]
    }

    fn max_turns(&self) -> u32 {
        50
    }

    fn pre_execute(&self, task_description: &str) -> Option<String> {
        Some(format!(
            "Implement the following task. Write clean, tested code:\n{}",
            task_description
        ))
    }

    fn post_execute(&self, output: &str) -> Option<String> {
        if output.contains("error[E") || output.contains("FAILED") {
            Some(
                "Implementation has compilation errors or test failures. Needs fixing.".to_string(),
            )
        } else {
            Some("Implementation complete. Ready for review.".to_string())
        }
    }

    fn preferred_model(&self) -> Option<&str> {
        Some("claude-sonnet-4-20250514")
    }
}

#[async_trait::async_trait]
impl AgentLifecycle for CrewAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Crew
    }

    async fn on_start(&mut self) -> Result<()> {
        tracing::info!("CrewAgent started — ready for work");
        Ok(())
    }

    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()> {
        tracing::info!(bead_id = %bead.id, "CrewAgent picked up bead");
        Ok(())
    }

    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()> {
        tracing::info!(bead_id = %bead_id, "CrewAgent finished bead");
        self.beads_executed += 1;
        Ok(())
    }

    async fn on_heartbeat(&mut self) -> Result<()> {
        tracing::debug!(executed = self.beads_executed, "CrewAgent heartbeat");
        Ok(())
    }

    async fn on_stop(&mut self) -> Result<()> {
        tracing::info!("CrewAgent stopping");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helper: get RoleConfig for any AgentRole
// ---------------------------------------------------------------------------

/// Create a boxed RoleConfig for the given agent role.
///
/// Specialized agent roles (SpecGatherer, QaReviewer, etc.) map to
/// a base lifecycle agent with the role-specific behavior handled via
/// context steering (prompt injection from prompts/*.md templates).
pub fn role_config_for(role: &AgentRole) -> Box<dyn RoleConfig + Send + Sync> {
    match role {
        AgentRole::Mayor => Box::new(MayorAgent::new()),
        AgentRole::Deacon | AgentRole::QaReviewer | AgentRole::SpecCritic => {
            Box::new(DeaconAgent::new())
        }
        AgentRole::Witness | AgentRole::QaFixer | AgentRole::ValidationFixer => {
            Box::new(WitnessAgent::new())
        }
        AgentRole::Refinery => Box::new(RefineryAgent::new()),
        AgentRole::Polecat => Box::new(PolecatAgent::new()),
        // All specialized roles use Crew as the base.
        // Their unique behavior comes from prompt templates + context steering.
        _ => Box::new(CrewAgent::new()),
    }
}
