use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use at_core::types::AgentRole;
use at_harness::audit_chain::{AuditChain, AuditError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ApprovalPolicy
// ---------------------------------------------------------------------------

/// Policy governing whether a tool invocation is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    /// Trusted tool, always allowed without human intervention.
    AutoApprove,
    /// Potentially dangerous tool, requires explicit human approval.
    RequireApproval,
    /// Never allowed under any circumstances.
    Deny,
}

// ---------------------------------------------------------------------------
// ApprovalStatus
// ---------------------------------------------------------------------------

/// Status of a pending approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
}

// ---------------------------------------------------------------------------
// PendingApproval
// ---------------------------------------------------------------------------

/// A request for human approval of a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub requested_at: DateTime<Utc>,
    pub status: ApprovalStatus,
    pub resolved_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur when processing tool approval requests.
///
/// The approval system manages human approval workflows for potentially
/// dangerous tool invocations. These errors represent failures in the
/// approval lifecycle: missing requests, already-resolved requests, or
/// policy-denied operations.
#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    /// The requested approval request ID does not exist.
    ///
    /// This typically occurs when:
    /// - The approval ID is invalid or was never created
    /// - The approval request has been removed from the system
    #[error("approval request not found: {0}")]
    NotFound(Uuid),

    /// The approval request has already been approved or denied.
    ///
    /// Attempting to resolve an already-resolved approval is not allowed.
    /// Check the approval status before attempting resolution.
    #[error("approval request already resolved: {0}")]
    AlreadyResolved(Uuid),

    /// The tool is denied by policy and cannot be executed.
    ///
    /// This occurs when a tool's approval policy is set to [`ApprovalPolicy::Deny`],
    /// preventing the tool from being invoked under any circumstances.
    /// The contained string identifies the denied tool name.
    #[error("tool denied by policy: {0}")]
    Denied(String),

    /// The decision could not be written to the audit chain, so it was not
    /// applied (approvals fail closed when auditing is enabled).
    #[error("approval audit log write failed: {0}")]
    Audit(String),
}

/// Result type for approval operations.
///
/// Alias for `std::result::Result<T, ApprovalError>` used throughout
/// the approval system to indicate operations that may fail with an
/// [`ApprovalError`].
pub type Result<T> = std::result::Result<T, ApprovalError>;

// ---------------------------------------------------------------------------
// ToolApprovalSystem
// ---------------------------------------------------------------------------

/// Manages tool approval policies and pending approval requests.
///
/// The approval system sits between the agent executor and the tools layer.
/// Before a tool is invoked, the executor calls `check_approval` to determine
/// whether the tool is auto-approved, requires human approval, or is denied.
///
/// When an audit chain is attached ([`Self::with_audit_log`]), every policy
/// check, request, approval and denial is appended to a hash-chained JSONL
/// log (see [`at_harness::audit_chain`]). Approve/deny fail closed: if the
/// entry cannot be written the decision is not applied.
pub struct ToolApprovalSystem {
    /// Per-tool default policies.
    policies: HashMap<String, ApprovalPolicy>,
    /// Per-role policy overrides as a list of (tool_name, role, policy) triples.
    role_overrides: Vec<(String, AgentRole, ApprovalPolicy)>,
    /// Outstanding and resolved approval requests.
    approvals: Vec<PendingApproval>,
    /// Tamper-evident record of every decision, if enabled.
    audit: Option<Arc<AuditChain>>,
}

impl ToolApprovalSystem {
    /// Create a new approval system with default policies.
    pub fn new() -> Self {
        let mut policies = HashMap::new();

        // Default policies
        policies.insert("file_read".to_string(), ApprovalPolicy::AutoApprove);
        policies.insert("list_directory".to_string(), ApprovalPolicy::AutoApprove);
        policies.insert("search_files".to_string(), ApprovalPolicy::AutoApprove);
        policies.insert("git_diff".to_string(), ApprovalPolicy::AutoApprove);
        policies.insert("git_log".to_string(), ApprovalPolicy::AutoApprove);
        policies.insert("git_blame".to_string(), ApprovalPolicy::AutoApprove);
        policies.insert("task_status".to_string(), ApprovalPolicy::AutoApprove);

        policies.insert("file_write".to_string(), ApprovalPolicy::RequireApproval);
        policies.insert("shell_execute".to_string(), ApprovalPolicy::RequireApproval);
        policies.insert("git_push".to_string(), ApprovalPolicy::RequireApproval);
        policies.insert("git_add".to_string(), ApprovalPolicy::RequireApproval);
        policies.insert("git_commit".to_string(), ApprovalPolicy::RequireApproval);
        policies.insert("task_assign".to_string(), ApprovalPolicy::RequireApproval);
        policies.insert("agent_spawn".to_string(), ApprovalPolicy::RequireApproval);
        policies.insert("agent_stop".to_string(), ApprovalPolicy::RequireApproval);

        policies.insert("delete".to_string(), ApprovalPolicy::Deny);
        policies.insert("file_delete".to_string(), ApprovalPolicy::Deny);
        policies.insert("force_push".to_string(), ApprovalPolicy::Deny);

        Self {
            policies,
            role_overrides: Vec::new(),
            approvals: Vec::new(),
            audit: None,
        }
    }

    /// Create a new approval system with auto-approve for everything (useful for testing).
    pub fn permissive() -> Self {
        Self {
            policies: HashMap::new(),
            role_overrides: Vec::new(),
            approvals: Vec::new(),
            audit: None,
        }
    }

    /// Default audit log location: `~/.auto-tundra/audit/approvals.jsonl`.
    pub fn default_audit_log_path() -> PathBuf {
        at_core::lockfile::data_dir()
            .join("audit")
            .join("approvals.jsonl")
    }

    /// Record every decision into `chain`.
    pub fn with_audit_chain(mut self, chain: Arc<AuditChain>) -> Self {
        self.audit = Some(chain);
        self
    }

    /// Record every decision into the JSONL chain at `path` (created if missing).
    pub fn with_audit_log(self, path: impl Into<PathBuf>) -> std::result::Result<Self, AuditError> {
        Ok(self.with_audit_chain(Arc::new(AuditChain::open(path)?)))
    }

    /// Record into [`Self::default_audit_log_path`]; logs and continues
    /// without auditing if the file cannot be opened.
    pub fn with_default_audit_log(self) -> Self {
        let path = Self::default_audit_log_path();
        match AuditChain::open(&path) {
            Ok(chain) => self.with_audit_chain(Arc::new(chain)),
            Err(e) => {
                tracing::error!(path = %path.display(), error = %e, "approval audit log disabled");
                self
            }
        }
    }

    /// The attached audit chain, if any.
    pub fn audit_chain(&self) -> Option<&Arc<AuditChain>> {
        self.audit.as_ref()
    }

    /// Append to the audit chain (no-op when auditing is off).
    fn audit(
        &self,
        kind: &str,
        actor: Option<String>,
        payload: serde_json::Value,
    ) -> std::result::Result<(), AuditError> {
        match &self.audit {
            Some(chain) => chain.append(kind, actor.as_deref(), payload).map(|_| ()),
            None => Ok(()),
        }
    }

    /// Set a default policy for a tool.
    pub fn set_policy(&mut self, tool_name: impl Into<String>, policy: ApprovalPolicy) {
        self.policies.insert(tool_name.into(), policy);
    }

    /// Set a role-specific policy override for a tool.
    pub fn set_role_override(
        &mut self,
        tool_name: impl Into<String>,
        role: AgentRole,
        policy: ApprovalPolicy,
    ) {
        let tool = tool_name.into();
        // Remove any existing override for this tool+role combo
        self.role_overrides
            .retain(|(t, r, _)| !(t == &tool && r == &role));
        self.role_overrides.push((tool, role, policy));
    }

    /// Check the approval policy for a tool invocation by a given role.
    ///
    /// Resolution order:
    /// 1. Role-specific override (if set)
    /// 2. Default tool policy (if set)
    /// 3. RequireApproval (if unknown tool)
    pub fn check_approval(&self, tool_name: &str, agent_role: &AgentRole) -> ApprovalPolicy {
        let (policy, source) = self.resolve_policy(tool_name, agent_role);
        if let Err(e) = self.audit(
            "approval.policy_checked",
            None,
            serde_json::json!({
                "tool": tool_name,
                "role": agent_role,
                "policy": policy,
                "source": source,
            }),
        ) {
            tracing::error!(tool = %tool_name, error = %e, "failed to audit approval policy check");
        }
        policy
    }

    fn resolve_policy(
        &self,
        tool_name: &str,
        agent_role: &AgentRole,
    ) -> (ApprovalPolicy, &'static str) {
        // Check role-specific override first
        if let Some((_, _, policy)) = self
            .role_overrides
            .iter()
            .find(|(t, r, _)| t == tool_name && r == agent_role)
        {
            return (*policy, "role_override");
        }

        // Fall back to default policy
        if let Some(policy) = self.policies.get(tool_name) {
            return (*policy, "tool_policy");
        }

        // Unknown tools require approval by default
        (ApprovalPolicy::RequireApproval, "unknown_tool_default")
    }

    /// Tools whose resolved policy for `agent_role` is [`ApprovalPolicy::Deny`]
    /// (tool defaults plus that role's overrides), sorted by name.
    ///
    /// Used to translate the policy table into CLI deny-lists before an agent
    /// process is spawned. Does not write to the audit chain.
    pub fn denied_tools(&self, agent_role: &AgentRole) -> Vec<String> {
        let mut names: Vec<&str> = self.policies.keys().map(String::as_str).collect();
        names.extend(
            self.role_overrides
                .iter()
                .filter(|(_, r, _)| r == agent_role)
                .map(|(t, _, _)| t.as_str()),
        );
        names.sort_unstable();
        names.dedup();
        names
            .into_iter()
            .filter(|t| self.resolve_policy(t, agent_role).0 == ApprovalPolicy::Deny)
            .map(str::to_string)
            .collect()
    }

    /// Create a pending approval request for a tool invocation.
    pub fn request_approval(
        &mut self,
        agent_id: Uuid,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> &PendingApproval {
        let approval = PendingApproval {
            id: Uuid::new_v4(),
            agent_id,
            tool_name: tool_name.into(),
            arguments,
            requested_at: Utc::now(),
            status: ApprovalStatus::Pending,
            resolved_at: None,
        };
        // Tool arguments can carry secrets; the audit log gets a redacted copy.
        let mut arguments = approval.arguments.clone();
        at_harness::output_guard::guard_json(&mut arguments);
        if let Err(e) = self.audit(
            "approval.requested",
            Some(format!("agent:{}", approval.agent_id)),
            serde_json::json!({
                "approval_id": approval.id,
                "tool": approval.tool_name,
                "arguments": arguments,
            }),
        ) {
            tracing::error!(approval_id = %approval.id, error = %e, "failed to audit approval request");
        }
        self.approvals.push(approval);
        self.approvals.last().unwrap()
    }

    /// Approve a pending request by its ID.
    pub fn approve(&mut self, approval_id: Uuid) -> Result<()> {
        self.resolve(approval_id, ApprovalStatus::Approved)
    }

    /// Deny a pending request by its ID.
    pub fn deny(&mut self, approval_id: Uuid) -> Result<()> {
        self.resolve(approval_id, ApprovalStatus::Denied)
    }

    /// Resolve a pending request, auditing before the state changes.
    fn resolve(&mut self, approval_id: Uuid, status: ApprovalStatus) -> Result<()> {
        let idx = self
            .approvals
            .iter()
            .position(|a| a.id == approval_id)
            .ok_or(ApprovalError::NotFound(approval_id))?;
        if self.approvals[idx].status != ApprovalStatus::Pending {
            return Err(ApprovalError::AlreadyResolved(approval_id));
        }

        let resolved_at = Utc::now();
        let approval = &self.approvals[idx];
        let kind = match status {
            ApprovalStatus::Approved => "approval.approved",
            _ => "approval.denied",
        };
        self.audit(
            kind,
            None,
            serde_json::json!({
                "approval_id": approval.id,
                "agent_id": approval.agent_id,
                "tool": approval.tool_name,
                "requested_at": approval.requested_at,
                "resolved_at": resolved_at,
            }),
        )
        .map_err(|e| ApprovalError::Audit(e.to_string()))?;

        let approval = &mut self.approvals[idx];
        approval.status = status;
        approval.resolved_at = Some(resolved_at);
        Ok(())
    }

    /// List all pending (unresolved) approval requests.
    pub fn list_pending(&self) -> Vec<&PendingApproval> {
        self.approvals
            .iter()
            .filter(|a| a.status == ApprovalStatus::Pending)
            .collect()
    }

    /// List all approval requests (including resolved).
    pub fn list_all(&self) -> &[PendingApproval] {
        &self.approvals
    }

    /// Get a specific approval by ID.
    pub fn get_approval(&self, id: Uuid) -> Option<&PendingApproval> {
        self.approvals.iter().find(|a| a.id == id)
    }

    /// Check if a specific approval request has been approved.
    pub fn is_approved(&self, approval_id: Uuid) -> bool {
        self.approvals
            .iter()
            .find(|a| a.id == approval_id)
            .map(|a| a.status == ApprovalStatus::Approved)
            .unwrap_or(false)
    }
}

impl Default for ToolApprovalSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policies_file_read_auto_approve() {
        let system = ToolApprovalSystem::new();
        assert_eq!(
            system.check_approval("file_read", &AgentRole::Crew),
            ApprovalPolicy::AutoApprove
        );
    }

    #[test]
    fn default_policies_file_write_require_approval() {
        let system = ToolApprovalSystem::new();
        assert_eq!(
            system.check_approval("file_write", &AgentRole::Crew),
            ApprovalPolicy::RequireApproval
        );
    }

    #[test]
    fn default_policies_shell_execute_require_approval() {
        let system = ToolApprovalSystem::new();
        assert_eq!(
            system.check_approval("shell_execute", &AgentRole::Witness),
            ApprovalPolicy::RequireApproval
        );
    }

    #[test]
    fn default_policies_git_push_require_approval() {
        let system = ToolApprovalSystem::new();
        assert_eq!(
            system.check_approval("git_push", &AgentRole::Crew),
            ApprovalPolicy::RequireApproval
        );
    }

    #[test]
    fn default_policies_delete_deny() {
        let system = ToolApprovalSystem::new();
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
    fn unknown_tool_requires_approval() {
        let system = ToolApprovalSystem::new();
        assert_eq!(
            system.check_approval("unknown_tool", &AgentRole::Crew),
            ApprovalPolicy::RequireApproval
        );
    }

    #[test]
    fn role_override_takes_precedence() {
        let mut system = ToolApprovalSystem::new();
        // Mayor gets auto-approve for task_assign
        system.set_role_override("task_assign", AgentRole::Mayor, ApprovalPolicy::AutoApprove);

        assert_eq!(
            system.check_approval("task_assign", &AgentRole::Mayor),
            ApprovalPolicy::AutoApprove
        );
        // Crew still requires approval
        assert_eq!(
            system.check_approval("task_assign", &AgentRole::Crew),
            ApprovalPolicy::RequireApproval
        );
    }

    #[test]
    fn request_approve_flow() {
        let mut system = ToolApprovalSystem::new();
        let agent_id = Uuid::new_v4();

        let approval_id = system
            .request_approval(
                agent_id,
                "file_write",
                serde_json::json!({"path": "foo.rs"}),
            )
            .id;

        // Should be pending
        assert_eq!(system.list_pending().len(), 1);
        assert_eq!(system.list_pending()[0].tool_name, "file_write");
        assert!(!system.is_approved(approval_id));

        // Approve it
        system.approve(approval_id).unwrap();
        assert!(system.is_approved(approval_id));
        assert!(system.list_pending().is_empty());

        // Double-approve should fail
        assert!(system.approve(approval_id).is_err());
    }

    #[test]
    fn request_deny_flow() {
        let mut system = ToolApprovalSystem::new();
        let agent_id = Uuid::new_v4();

        let approval_id = system
            .request_approval(
                agent_id,
                "shell_execute",
                serde_json::json!({"cmd": "rm -rf /"}),
            )
            .id;

        // Deny it
        system.deny(approval_id).unwrap();
        assert!(!system.is_approved(approval_id));
        assert!(system.list_pending().is_empty());

        // Check it was marked denied
        let approval = system.get_approval(approval_id).unwrap();
        assert_eq!(approval.status, ApprovalStatus::Denied);
        assert!(approval.resolved_at.is_some());

        // Double-deny should fail
        assert!(system.deny(approval_id).is_err());
    }

    #[test]
    fn approve_nonexistent_returns_error() {
        let mut system = ToolApprovalSystem::new();
        let fake_id = Uuid::new_v4();
        assert!(system.approve(fake_id).is_err());
        assert!(system.deny(fake_id).is_err());
    }

    #[test]
    fn multiple_pending_approvals() {
        let mut system = ToolApprovalSystem::new();
        let agent_id = Uuid::new_v4();

        let _id1 = system
            .request_approval(agent_id, "file_write", serde_json::json!({}))
            .id;
        let id2 = system
            .request_approval(agent_id, "shell_execute", serde_json::json!({}))
            .id;
        let _id3 = system
            .request_approval(agent_id, "git_push", serde_json::json!({}))
            .id;

        assert_eq!(system.list_pending().len(), 3);

        // Approve one
        system.approve(id2).unwrap();
        assert_eq!(system.list_pending().len(), 2);
        assert_eq!(system.list_all().len(), 3);
    }

    #[test]
    fn custom_policy_override() {
        let mut system = ToolApprovalSystem::new();
        system.set_policy("file_read", ApprovalPolicy::Deny);
        assert_eq!(
            system.check_approval("file_read", &AgentRole::Crew),
            ApprovalPolicy::Deny
        );
    }

    #[test]
    fn decisions_are_recorded_in_a_verifiable_audit_chain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit").join("approvals.jsonl");
        let mut system = ToolApprovalSystem::new().with_audit_log(&path).unwrap();
        let agent_id = Uuid::new_v4();

        assert_eq!(
            system.check_approval("git_push", &AgentRole::Crew),
            ApprovalPolicy::RequireApproval
        );
        let secret = format!("ghp_{}", "Q7vL4nR8sT1yU6hD0jF5cGaB3xK9mW2pZe8Y");
        let a = system
            .request_approval(agent_id, "git_push", serde_json::json!({"token": secret}))
            .id;
        let b = system
            .request_approval(agent_id, "shell_execute", serde_json::json!({}))
            .id;
        system.approve(a).unwrap();
        system.deny(b).unwrap();
        assert!(system.approve(a).is_err()); // not a decision, not recorded

        let report = system.audit_chain().unwrap().verify().unwrap();
        assert_eq!(report.entries, 5);
        let log = std::fs::read_to_string(&path).unwrap();
        let kinds: Vec<String> = log
            .lines()
            .map(|l| {
                serde_json::from_str::<serde_json::Value>(l).unwrap()["kind"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "approval.policy_checked",
                "approval.requested",
                "approval.requested",
                "approval.approved",
                "approval.denied"
            ]
        );
        assert!(
            !log.contains(&secret),
            "arguments must be redacted in the audit log"
        );
        assert!(log.contains("\"source\":\"tool_policy\""));
    }

    #[test]
    fn approve_fails_closed_when_audit_write_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("approvals.jsonl");
        let mut system = ToolApprovalSystem::new().with_audit_log(&path).unwrap();
        let id = system
            .request_approval(Uuid::new_v4(), "git_push", serde_json::json!({}))
            .id;
        // Replace the log file with a directory so the next append fails.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(matches!(system.approve(id), Err(ApprovalError::Audit(_))));
        assert_eq!(
            system.get_approval(id).unwrap().status,
            ApprovalStatus::Pending
        );
    }

    #[test]
    fn permissive_system_defaults_to_require_approval_for_unknown() {
        let system = ToolApprovalSystem::permissive();
        // No policies set, so everything falls through to RequireApproval
        assert_eq!(
            system.check_approval("anything", &AgentRole::Crew),
            ApprovalPolicy::RequireApproval
        );
    }
}
