use std::collections::HashMap;

use at_core::types::AgentRole;
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

#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    #[error("approval request not found: {0}")]
    NotFound(Uuid),
    #[error("approval request already resolved: {0}")]
    AlreadyResolved(Uuid),
    #[error("tool denied by policy: {0}")]
    Denied(String),
}

pub type Result<T> = std::result::Result<T, ApprovalError>;

// ---------------------------------------------------------------------------
// ToolApprovalSystem
// ---------------------------------------------------------------------------

/// Manages tool approval policies and pending approval requests.
///
/// The approval system sits between the agent executor and the tools layer.
/// Before a tool is invoked, the executor calls `check_approval` to determine
/// whether the tool is auto-approved, requires human approval, or is denied.
pub struct ToolApprovalSystem {
    /// Per-tool default policies.
    policies: HashMap<String, ApprovalPolicy>,
    /// Per-role policy overrides as a list of (tool_name, role, policy) triples.
    role_overrides: Vec<(String, AgentRole, ApprovalPolicy)>,
    /// Outstanding and resolved approval requests.
    approvals: Vec<PendingApproval>,
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
        }
    }

    /// Create a new approval system with auto-approve for everything (useful for testing).
    pub fn permissive() -> Self {
        Self {
            policies: HashMap::new(),
            role_overrides: Vec::new(),
            approvals: Vec::new(),
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
        // Check role-specific override first
        if let Some((_, _, policy)) = self
            .role_overrides
            .iter()
            .find(|(t, r, _)| t == tool_name && r == agent_role)
        {
            return *policy;
        }

        // Fall back to default policy
        if let Some(policy) = self.policies.get(tool_name) {
            return *policy;
        }

        // Unknown tools require approval by default
        ApprovalPolicy::RequireApproval
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
        self.approvals.push(approval);
        self.approvals.last().unwrap()
    }

    /// Approve a pending request by its ID.
    pub fn approve(&mut self, approval_id: Uuid) -> Result<()> {
        let approval = self
            .approvals
            .iter_mut()
            .find(|a| a.id == approval_id)
            .ok_or(ApprovalError::NotFound(approval_id))?;

        if approval.status != ApprovalStatus::Pending {
            return Err(ApprovalError::AlreadyResolved(approval_id));
        }

        approval.status = ApprovalStatus::Approved;
        approval.resolved_at = Some(Utc::now());
        Ok(())
    }

    /// Deny a pending request by its ID.
    pub fn deny(&mut self, approval_id: Uuid) -> Result<()> {
        let approval = self
            .approvals
            .iter_mut()
            .find(|a| a.id == approval_id)
            .ok_or(ApprovalError::NotFound(approval_id))?;

        if approval.status != ApprovalStatus::Pending {
            return Err(ApprovalError::AlreadyResolved(approval_id));
        }

        approval.status = ApprovalStatus::Denied;
        approval.resolved_at = Some(Utc::now());
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
    fn permissive_system_defaults_to_require_approval_for_unknown() {
        let system = ToolApprovalSystem::permissive();
        // No policies set, so everything falls through to RequireApproval
        assert_eq!(
            system.check_approval("anything", &AgentRole::Crew),
            ApprovalPolicy::RequireApproval
        );
    }
}
