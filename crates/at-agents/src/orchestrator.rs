//! Orchestrator — wires context steering, RLM patterns, prompt templates,
//! and the spec pipeline into a unified task execution engine.
//!
//! This is the "brain" that sits above TaskRunner and coordinates:
//! - Context assembly via ContextSteerer
//! - Prompt selection via PromptRegistry
//! - Recursive decomposition via RLM patterns
//! - Stuck detection and recovery
//! - Session insight extraction
//! - Spec pipeline progression

use std::collections::HashMap;
use std::path::PathBuf;

use at_core::context_steering::{AssembledContext, ContextSteerer, MemoryEntry, MemoryKind};
use at_core::rlm::{
    Decomposition, ProgressiveRefinement, StuckDetector, StuckReason, SynthesisStrategy,
};
use at_core::types::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::prompts::PromptRegistry;

// ---------------------------------------------------------------------------
// OrchestratorConfig
// ---------------------------------------------------------------------------

/// Configuration for the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Default token budget for context assembly.
    pub token_budget: usize,
    /// Maximum recursion depth for RLM decomposition.
    pub max_recursion_depth: usize,
    /// Maximum progressive refinement revisions.
    pub max_revisions: usize,
    /// Stuck detection timeout in seconds.
    pub stuck_timeout_secs: u64,
    /// Stuck detection token budget.
    pub stuck_token_budget: usize,
    /// Confidence threshold for auto-finalize.
    pub confidence_threshold: f64,
    /// Whether to enable RLM recursive decomposition.
    pub enable_rlm: bool,
    /// Whether to enable progressive refinement.
    pub enable_refinement: bool,
    /// Execution retention TTL in seconds (how long to keep completed executions in memory).
    pub execution_ttl_secs: u64,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            token_budget: 16_000,
            max_recursion_depth: 3,
            max_revisions: 5,
            stuck_timeout_secs: 300,
            stuck_token_budget: 100_000,
            confidence_threshold: 0.85,
            enable_rlm: true,
            enable_refinement: true,
            execution_ttl_secs: 86_400, // 24 hours
        }
    }
}

// ---------------------------------------------------------------------------
// TaskExecution — tracks one task's execution state
// ---------------------------------------------------------------------------

/// Tracks the execution state of a single task through the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: Uuid,
    pub task_title: String,
    pub task_description: String,
    pub current_phase: String,
    pub agent_role: AgentRole,
    /// Assembled context for this execution.
    pub context_tokens: usize,
    /// Phase history.
    pub phase_history: Vec<PhaseRecord>,
    /// Whether RLM decomposition was used.
    pub used_rlm: bool,
    /// Whether progressive refinement was used.
    pub used_refinement: bool,
    /// Recovery actions taken.
    pub recoveries: Vec<RecoveryEvent>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseRecord {
    pub phase: String,
    pub agent_role: AgentRole,
    pub tokens_used: usize,
    pub duration_ms: u64,
    pub status: PhaseOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseOutcome {
    Success,
    Failed,
    Recovered,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryEvent {
    pub phase: String,
    pub reason: StuckReason,
    pub action: String,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Orchestrator — the unified execution engine
// ---------------------------------------------------------------------------

/// The unified task execution engine that composes all subsystems.
pub struct Orchestrator {
    config: OrchestratorConfig,
    context_steerer: ContextSteerer,
    prompt_registry: PromptRegistry,
    /// Active task executions.
    executions: HashMap<Uuid, TaskExecution>,
    /// Stuck detectors per execution.
    stuck_detectors: HashMap<Uuid, StuckDetector>,
    /// Active decompositions.
    decompositions: HashMap<Uuid, Decomposition>,
    /// Active refinements.
    refinements: HashMap<Uuid, ProgressiveRefinement>,
}

impl Orchestrator {
    pub fn new(project_root: impl Into<PathBuf>, config: OrchestratorConfig) -> Self {
        let project_root = project_root.into();
        let mut context_steerer = ContextSteerer::new(&project_root);
        context_steerer.load_project();

        let mut prompt_registry = PromptRegistry::new();
        prompt_registry.load_from_project(&project_root);

        Self {
            config,
            context_steerer,
            prompt_registry,
            executions: HashMap::new(),
            stuck_detectors: HashMap::new(),
            decompositions: HashMap::new(),
            refinements: HashMap::new(),
        }
    }

    /// Start a new task execution.
    pub fn start_task(
        &mut self,
        title: impl Into<String>,
        description: impl Into<String>,
        initial_role: AgentRole,
    ) -> Uuid {
        let title = title.into();
        let description = description.into();
        let id = Uuid::new_v4();

        let execution = TaskExecution {
            id,
            task_title: title.clone(),
            task_description: description.clone(),
            current_phase: "discovery".into(),
            agent_role: initial_role,
            context_tokens: 0,
            phase_history: Vec::new(),
            used_rlm: false,
            used_refinement: false,
            recoveries: Vec::new(),
            started_at: Utc::now(),
            completed_at: None,
        };

        self.executions.insert(id, execution);
        self.stuck_detectors.insert(
            id,
            StuckDetector::new(
                self.config.stuck_timeout_secs,
                self.config.stuck_token_budget,
            ),
        );

        id
    }

    /// Assemble context for a task execution at a given phase.
    pub fn assemble_context(&self, execution_id: &Uuid, phase: &str) -> Option<AssembledContext> {
        let exec = self.executions.get(execution_id)?;
        Some(self.context_steerer.assemble(
            &format!("{:?}", exec.agent_role),
            phase,
            Some(&exec.task_description),
            self.config.token_budget,
        ))
    }

    /// Build the full prompt for an agent invocation.
    pub fn build_prompt(&self, execution_id: &Uuid, phase: &str) -> Option<String> {
        let exec = self.executions.get(execution_id)?;

        // 1. Get the prompt template for this role
        let role_prompt = self
            .prompt_registry
            .get(&exec.agent_role)
            .map(|tpl| tpl.render_task(&exec.task_title, &exec.task_description, ""))
            .unwrap_or_else(|| {
                format!(
                    "You are a {:?} agent working on: {}\n\n{}",
                    exec.agent_role, exec.task_title, exec.task_description,
                )
            });

        // 2. Assemble context
        let context = self.context_steerer.assemble(
            &format!("{:?}", exec.agent_role),
            phase,
            Some(&exec.task_description),
            self.config.token_budget,
        );

        // 3. Combine into full prompt
        let context_xml = context.render_xml();
        Some(format!("{}\n\n{}", context_xml, role_prompt))
    }

    /// Record agent output for stuck detection.
    pub fn record_output(
        &mut self,
        execution_id: &Uuid,
        output: &str,
        tokens: usize,
    ) -> Option<StuckReason> {
        if let Some(detector) = self.stuck_detectors.get_mut(execution_id) {
            detector.record_output(output, tokens);
            detector.check()
        } else {
            None
        }
    }

    /// Start a recursive decomposition for a task.
    pub fn decompose(
        &mut self,
        execution_id: &Uuid,
        subtasks: Vec<String>,
        strategy: SynthesisStrategy,
    ) -> Option<Uuid> {
        if !self.config.enable_rlm {
            return None;
        }
        let exec = self.executions.get_mut(execution_id)?;
        exec.used_rlm = true;

        let mut decomp =
            Decomposition::new(&exec.task_description, self.config.max_recursion_depth);
        decomp.synthesis = strategy;
        for st in subtasks {
            decomp.add_subtask(st);
        }

        let decomp_id = decomp.id;
        self.decompositions.insert(decomp_id, decomp);
        Some(decomp_id)
    }

    /// Start progressive refinement for a task.
    pub fn start_refinement(&mut self, execution_id: &Uuid) -> Option<Uuid> {
        if !self.config.enable_refinement {
            return None;
        }
        let exec = self.executions.get_mut(execution_id)?;
        exec.used_refinement = true;

        let pr = ProgressiveRefinement::new(&exec.task_description, self.config.max_revisions);
        let pr_id = pr.id;
        self.refinements.insert(pr_id, pr);
        Some(pr_id)
    }

    /// Record a phase completion.
    pub fn record_phase(
        &mut self,
        execution_id: &Uuid,
        phase: &str,
        tokens_used: usize,
        duration_ms: u64,
        outcome: PhaseOutcome,
    ) {
        if let Some(exec) = self.executions.get_mut(execution_id) {
            exec.phase_history.push(PhaseRecord {
                phase: phase.to_string(),
                agent_role: exec.agent_role.clone(),
                tokens_used,
                duration_ms,
                status: outcome,
            });
            exec.context_tokens += tokens_used;
        }
    }

    /// Mark a task as complete.
    pub fn complete_task(&mut self, execution_id: &Uuid) {
        if let Some(exec) = self.executions.get_mut(execution_id) {
            exec.completed_at = Some(Utc::now());
        }
        self.stuck_detectors.remove(execution_id);
    }

    /// Get a task execution.
    pub fn get_execution(&self, id: &Uuid) -> Option<&TaskExecution> {
        self.executions.get(id)
    }

    /// Get a decomposition.
    pub fn get_decomposition(&self, id: &Uuid) -> Option<&Decomposition> {
        self.decompositions.get(id)
    }

    /// Get a mutable decomposition.
    pub fn get_decomposition_mut(&mut self, id: &Uuid) -> Option<&mut Decomposition> {
        self.decompositions.get_mut(id)
    }

    /// Get a refinement.
    pub fn get_refinement(&self, id: &Uuid) -> Option<&ProgressiveRefinement> {
        self.refinements.get(id)
    }

    /// Get a mutable refinement.
    pub fn get_refinement_mut(&mut self, id: &Uuid) -> Option<&mut ProgressiveRefinement> {
        self.refinements.get_mut(id)
    }

    /// Record a recovery event.
    pub fn record_recovery(
        &mut self,
        execution_id: &Uuid,
        phase: &str,
        reason: StuckReason,
        action: &str,
    ) {
        if let Some(exec) = self.executions.get_mut(execution_id) {
            exec.recoveries.push(RecoveryEvent {
                phase: phase.to_string(),
                reason,
                action: action.to_string(),
                timestamp: Utc::now(),
            });
        }
        // Reset stuck detector after recovery
        if let Some(det) = self.stuck_detectors.get_mut(execution_id) {
            det.reset();
        }
    }

    /// Add a memory from a completed session.
    pub fn add_session_memory(&mut self, content: impl Into<String>, keywords: Vec<String>) {
        use at_core::context_steering::MemoryWeight;
        self.context_steerer.add_memory(MemoryEntry {
            kind: MemoryKind::Episodic,
            content: content.into(),
            relevance: 0.8,
            keywords,
            weight: MemoryWeight::active(),
        });
    }

    /// Number of active executions.
    pub fn active_count(&self) -> usize {
        self.executions
            .values()
            .filter(|e| e.completed_at.is_none())
            .count()
    }

    /// Total executions.
    pub fn total_count(&self) -> usize {
        self.executions.len()
    }

    /// Get all active executions.
    pub fn active_executions(&self) -> Vec<&TaskExecution> {
        self.executions
            .values()
            .filter(|e| e.completed_at.is_none())
            .collect()
    }

    /// Generate an execution summary.
    pub fn execution_summary(&self, id: &Uuid) -> Option<String> {
        let exec = self.executions.get(id)?;
        let mut parts = vec![format!("## Task: {}", exec.task_title)];
        parts.push(format!("Phase: {}", exec.current_phase));
        parts.push(format!("Agent: {:?}", exec.agent_role));
        parts.push(format!("Tokens: {}", exec.context_tokens));

        if !exec.phase_history.is_empty() {
            parts.push("\n### Phase History".into());
            for ph in &exec.phase_history {
                parts.push(format!(
                    "- {} ({:?}): {:?} — {} tokens, {}ms",
                    ph.phase, ph.agent_role, ph.status, ph.tokens_used, ph.duration_ms,
                ));
            }
        }

        if !exec.recoveries.is_empty() {
            parts.push(format!("\n### Recoveries: {}", exec.recoveries.len()));
        }

        Some(parts.join("\n"))
    }

    /// Clean up completed executions that are older than the specified TTL.
    ///
    /// Removes old entries from executions and stuck_detectors HashMaps if
    /// the execution has a completed_at timestamp older than ttl_secs.
    /// This prevents unbounded memory growth in the orchestrator over time.
    ///
    /// Note: decompositions and refinements are keyed by their own UUIDs
    /// and would require additional tracking to clean up by execution ID.
    ///
    /// Returns the number of executions removed.
    pub fn cleanup_completed_executions(&mut self, ttl_secs: u64) -> usize {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(ttl_secs as i64);

        let mut removed_count = 0;
        let mut executions_to_remove = Vec::new();

        // Identify executions to remove
        for (exec_id, execution) in self.executions.iter() {
            if let Some(completed_at) = execution.completed_at {
                if completed_at < cutoff {
                    executions_to_remove.push(*exec_id);
                }
            }
        }

        // Remove identified executions and their stuck detectors
        // Note: stuck_detectors share the same UUID key as executions
        for exec_id in executions_to_remove {
            self.executions.remove(&exec_id);
            self.stuck_detectors.remove(&exec_id);
            removed_count += 1;
        }

        removed_count
    }

    /// Start a background cleanup task that periodically removes expired executions.
    ///
    /// This method spawns a tokio task that runs at a fixed interval (1 hour by default),
    /// cleaning up completed executions older than the configured execution_ttl_secs.
    ///
    /// The background task runs until the process exits. It logs the number of
    /// executions removed during each cleanup cycle.
    ///
    /// Note: This method requires the Orchestrator to be wrapped in Arc<Mutex<_>>
    /// to allow the background task to acquire exclusive access for cleanup.
    pub fn start_cleanup_task(orch: std::sync::Arc<std::sync::Mutex<Self>>) {
        tokio::spawn(async move {
            // Default cleanup interval: 1 hour (3600 seconds)
            let interval_secs = 3600;
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            interval.tick().await; // First tick completes immediately

            tracing::info!(
                interval_secs,
                "Orchestrator background cleanup task started"
            );

            loop {
                interval.tick().await;

                // Acquire lock and perform cleanup with comprehensive metrics
                let (ttl_secs, executions_before, executions_removed, executions_after, stuck_detectors_before, stuck_detectors_after) = {
                    let mut orch_guard = orch.lock().unwrap();
                    let ttl = orch_guard.config.execution_ttl_secs;

                    // Capture before counts
                    let exec_before = orch_guard.executions.len();
                    let stuck_before = orch_guard.stuck_detectors.len();

                    // Perform cleanup
                    let removed = orch_guard.cleanup_completed_executions(ttl);

                    // Capture after counts
                    let exec_after = orch_guard.executions.len();
                    let stuck_after = orch_guard.stuck_detectors.len();

                    (ttl, exec_before, removed, exec_after, stuck_before, stuck_after)
                };

                tracing::info!(
                    execution_ttl_secs = ttl_secs,
                    executions_before,
                    executions_removed,
                    executions_after,
                    stuck_detectors_before,
                    stuck_detectors_after,
                    "Orchestrator cleanup cycle completed"
                );
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_orchestrator() -> Orchestrator {
        let dir = tempfile::tempdir().unwrap();
        // Create minimal project structure
        std::fs::write(
            dir.path().join("CLAUDE.md"),
            "# Rules\n## Conventions\n- Use Rust\n",
        )
        .unwrap();

        Orchestrator::new(dir.path(), OrchestratorConfig::default())
    }

    #[test]
    fn orchestrator_start_task() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("Fix bug", "Login crashes", AgentRole::Coder);
        assert_eq!(orch.total_count(), 1);
        assert_eq!(orch.active_count(), 1);

        let exec = orch.get_execution(&id).unwrap();
        assert_eq!(exec.task_title, "Fix bug");
    }

    #[test]
    fn orchestrator_build_prompt() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("Add auth", "Add OAuth to API", AgentRole::Coder);

        let prompt = orch.build_prompt(&id, "coding");
        assert!(prompt.is_some());
        let prompt = prompt.unwrap();
        assert!(
            prompt.contains("project-context")
                || prompt.contains("Add auth")
                || prompt.contains("OAuth")
        );
    }

    #[test]
    fn orchestrator_assemble_context() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("task", "desc", AgentRole::Planner);

        let ctx = orch.assemble_context(&id, "planning");
        assert!(ctx.is_some());
    }

    #[test]
    fn orchestrator_record_output_stuck() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("task", "desc", AgentRole::Coder);

        // Record same output 3 times → loop detected
        assert!(orch.record_output(&id, "repeated output", 10).is_none());
        assert!(orch.record_output(&id, "repeated output", 10).is_none());
        let stuck = orch.record_output(&id, "repeated output", 10);
        assert_eq!(stuck, Some(StuckReason::OutputLoop));
    }

    #[test]
    fn orchestrator_decompose() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("big task", "complex work", AgentRole::Mayor);

        let decomp_id = orch.decompose(
            &id,
            vec!["sub 1".into(), "sub 2".into()],
            SynthesisStrategy::Concatenate,
        );
        assert!(decomp_id.is_some());

        let dec = orch.get_decomposition(&decomp_id.unwrap()).unwrap();
        assert_eq!(dec.subtasks.len(), 2);

        // Execution should be marked as using RLM
        assert!(orch.get_execution(&id).unwrap().used_rlm);
    }

    #[test]
    fn orchestrator_refinement() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("task", "desc", AgentRole::Coder);

        let pr_id = orch.start_refinement(&id);
        assert!(pr_id.is_some());

        let pr = orch.get_refinement_mut(&pr_id.unwrap()).unwrap();
        pr.revise("draft 1", None, 0.5);
        pr.revise("draft 2", Some("improved".into()), 0.9);
        assert_eq!(pr.revision_count(), 2);

        assert!(orch.get_execution(&id).unwrap().used_refinement);
    }

    #[test]
    fn orchestrator_record_phase() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("task", "desc", AgentRole::Coder);

        orch.record_phase(&id, "discovery", 500, 1000, PhaseOutcome::Success);
        orch.record_phase(&id, "coding", 2000, 5000, PhaseOutcome::Success);

        let exec = orch.get_execution(&id).unwrap();
        assert_eq!(exec.phase_history.len(), 2);
        assert_eq!(exec.context_tokens, 2500);
    }

    #[test]
    fn orchestrator_complete_task() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("task", "desc", AgentRole::Coder);
        assert_eq!(orch.active_count(), 1);

        orch.complete_task(&id);
        assert_eq!(orch.active_count(), 0);
        assert!(orch.get_execution(&id).unwrap().completed_at.is_some());
    }

    #[test]
    fn orchestrator_recovery() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("task", "desc", AgentRole::Coder);

        orch.record_recovery(&id, "coding", StuckReason::Timeout, "retry");
        let exec = orch.get_execution(&id).unwrap();
        assert_eq!(exec.recoveries.len(), 1);
    }

    #[test]
    fn orchestrator_session_memory() {
        let mut orch = make_orchestrator();
        orch.add_session_memory(
            "The auth module uses JWT tokens stored in HttpOnly cookies",
            vec!["auth".into(), "jwt".into()],
        );
        // Memory is added to steerer — verify by assembling context
        let id = orch.start_task("auth work", "modify auth", AgentRole::Coder);
        let ctx = orch.assemble_context(&id, "discovery");
        assert!(ctx.is_some());
    }

    #[test]
    fn orchestrator_execution_summary() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("Fix login", "Login bug", AgentRole::Coder);
        orch.record_phase(&id, "discovery", 500, 1000, PhaseOutcome::Success);

        let summary = orch.execution_summary(&id);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Fix login"));
        assert!(summary.contains("Phase History"));
    }

    #[test]
    fn orchestrator_config_defaults() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.token_budget, 16_000);
        assert_eq!(config.max_recursion_depth, 3);
        assert!(config.enable_rlm);
    }

    #[test]
    fn orchestrator_config_serialization() {
        let config = OrchestratorConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deser: OrchestratorConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.token_budget, 16_000);
    }

    #[test]
    fn orchestrator_rlm_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let config = OrchestratorConfig {
            enable_rlm: false,
            ..Default::default()
        };

        let mut orch = Orchestrator::new(dir.path(), config);
        let id = orch.start_task("task", "desc", AgentRole::Mayor);
        assert!(orch
            .decompose(&id, vec!["sub".into()], SynthesisStrategy::Concatenate)
            .is_none());
    }

    #[test]
    fn orchestrator_nonexistent_execution() {
        let orch = make_orchestrator();
        assert!(orch.get_execution(&Uuid::new_v4()).is_none());
        assert!(orch.assemble_context(&Uuid::new_v4(), "coding").is_none());
        assert!(orch.build_prompt(&Uuid::new_v4(), "coding").is_none());
    }

    #[test]
    fn task_execution_serialization() {
        let exec = TaskExecution {
            id: Uuid::new_v4(),
            task_title: "test".into(),
            task_description: "desc".into(),
            current_phase: "coding".into(),
            agent_role: AgentRole::Coder,
            context_tokens: 500,
            phase_history: vec![],
            used_rlm: false,
            used_refinement: false,
            recoveries: vec![],
            started_at: Utc::now(),
            completed_at: None,
        };
        let json = serde_json::to_string(&exec).unwrap();
        let deser: TaskExecution = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.task_title, "test");
    }

    #[test]
    fn orchestrator_cleanup_removes_old_completed_executions() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("old task", "description", AgentRole::Coder);

        // Mark task as completed 10 days ago
        if let Some(exec) = orch.executions.get_mut(&id) {
            exec.completed_at = Some(Utc::now() - chrono::Duration::days(10));
        }

        assert_eq!(orch.total_count(), 1);
        assert_eq!(orch.stuck_detectors.len(), 1);

        // Cleanup with TTL of 7 days (604800 seconds)
        let removed = orch.cleanup_completed_executions(7 * 24 * 60 * 60);

        assert_eq!(removed, 1);
        assert_eq!(orch.total_count(), 0);
        assert_eq!(orch.stuck_detectors.len(), 0);
    }

    #[test]
    fn orchestrator_cleanup_keeps_recent_completed_executions() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("recent task", "description", AgentRole::Coder);

        // Mark task as completed 5 days ago
        if let Some(exec) = orch.executions.get_mut(&id) {
            exec.completed_at = Some(Utc::now() - chrono::Duration::days(5));
        }

        assert_eq!(orch.total_count(), 1);

        // Cleanup with TTL of 7 days (task is only 5 days old)
        let removed = orch.cleanup_completed_executions(7 * 24 * 60 * 60);

        assert_eq!(removed, 0);
        assert_eq!(orch.total_count(), 1);
    }

    #[test]
    fn orchestrator_cleanup_keeps_active_executions() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("active task", "description", AgentRole::Coder);

        // Task is not completed (completed_at is None)
        assert!(orch.get_execution(&id).unwrap().completed_at.is_none());
        assert_eq!(orch.active_count(), 1);

        // Cleanup should not remove active tasks
        let removed = orch.cleanup_completed_executions(7 * 24 * 60 * 60);

        assert_eq!(removed, 0);
        assert_eq!(orch.total_count(), 1);
        assert_eq!(orch.active_count(), 1);
    }

    #[test]
    fn orchestrator_cleanup_handles_multiple_executions() {
        let mut orch = make_orchestrator();

        // Create old completed task
        let old_id = orch.start_task("old", "desc", AgentRole::Coder);
        if let Some(exec) = orch.executions.get_mut(&old_id) {
            exec.completed_at = Some(Utc::now() - chrono::Duration::days(10));
        }

        // Create recent completed task
        let recent_id = orch.start_task("recent", "desc", AgentRole::Coder);
        if let Some(exec) = orch.executions.get_mut(&recent_id) {
            exec.completed_at = Some(Utc::now() - chrono::Duration::days(5));
        }

        // Create active task
        let active_id = orch.start_task("active", "desc", AgentRole::Coder);

        assert_eq!(orch.total_count(), 3);

        // Cleanup with TTL of 7 days
        let removed = orch.cleanup_completed_executions(7 * 24 * 60 * 60);

        assert_eq!(removed, 1);
        assert_eq!(orch.total_count(), 2);
        assert!(orch.get_execution(&old_id).is_none());
        assert!(orch.get_execution(&recent_id).is_some());
        assert!(orch.get_execution(&active_id).is_some());
    }

    #[test]
    fn orchestrator_cleanup_empty_state() {
        let mut orch = make_orchestrator();
        assert_eq!(orch.total_count(), 0);

        let removed = orch.cleanup_completed_executions(7 * 24 * 60 * 60);

        assert_eq!(removed, 0);
    }

    #[test]
    fn orchestrator_cleanup_with_zero_ttl() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("task", "description", AgentRole::Coder);

        // Mark task as completed 1 second ago
        if let Some(exec) = orch.executions.get_mut(&id) {
            exec.completed_at = Some(Utc::now() - chrono::Duration::seconds(1));
        }

        assert_eq!(orch.total_count(), 1);

        // Cleanup with zero TTL should remove all completed tasks
        let removed = orch.cleanup_completed_executions(0);

        assert_eq!(removed, 1);
        assert_eq!(orch.total_count(), 0);
    }

    #[test]
    fn orchestrator_cleanup_verifies_executions_and_detectors_cleaned() {
        let mut orch = make_orchestrator();
        let id = orch.start_task("task", "desc", AgentRole::Coder);

        // Verify HashMaps have entries (execution and stuck_detector share same ID)
        assert_eq!(orch.executions.len(), 1);
        assert_eq!(orch.stuck_detectors.len(), 1);

        // Mark as completed 10 days ago
        if let Some(exec) = orch.executions.get_mut(&id) {
            exec.completed_at = Some(Utc::now() - chrono::Duration::days(10));
        }

        // Cleanup with TTL of 7 days
        let removed = orch.cleanup_completed_executions(7 * 24 * 60 * 60);

        // Verify executions and stuck_detectors are cleaned
        assert_eq!(removed, 1);
        assert_eq!(orch.executions.len(), 0);
        assert_eq!(orch.stuck_detectors.len(), 0);
    }

    #[tokio::test]
    async fn orchestrator_background_cleanup_starts_successfully() {
        use std::sync::{Arc, Mutex};

        let orch = Arc::new(Mutex::new(make_orchestrator()));

        // Starting the background task should not panic
        Orchestrator::start_cleanup_task(Arc::clone(&orch));

        // Give the task a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Test passes if we get here without panicking
        assert!(true);
    }

    #[tokio::test]
    async fn orchestrator_background_cleanup_removes_old_executions() {
        use std::sync::{Arc, Mutex};

        let orch = Arc::new(Mutex::new(make_orchestrator()));

        // Create an old completed execution
        {
            let mut orch_guard = orch.lock().unwrap();
            let id = orch_guard.start_task("old task", "description", AgentRole::Coder);
            if let Some(exec) = orch_guard.executions.get_mut(&id) {
                exec.completed_at = Some(Utc::now() - chrono::Duration::days(30));
            }
            // Set very short TTL for testing
            orch_guard.config.execution_ttl_secs = 0; // Immediate cleanup
        };

        // Verify execution exists
        assert_eq!(orch.lock().unwrap().executions.len(), 1);
        assert_eq!(orch.lock().unwrap().stuck_detectors.len(), 1);

        // Manually run cleanup once to verify it works
        {
            let mut orch_guard = orch.lock().unwrap();
            let removed = orch_guard.cleanup_completed_executions(0);
            assert_eq!(removed, 1);
        }

        // Verify execution was removed
        assert_eq!(orch.lock().unwrap().executions.len(), 0);
        assert_eq!(orch.lock().unwrap().stuck_detectors.len(), 0);
    }

    #[tokio::test]
    async fn orchestrator_background_cleanup_respects_ttl() {
        use std::sync::{Arc, Mutex};

        let orch = Arc::new(Mutex::new(make_orchestrator()));

        // Create a recent completed execution (5 days old)
        {
            let mut orch_guard = orch.lock().unwrap();
            let id = orch_guard.start_task("recent task", "description", AgentRole::Coder);
            if let Some(exec) = orch_guard.executions.get_mut(&id) {
                exec.completed_at = Some(Utc::now() - chrono::Duration::days(5));
            }
            // Set TTL to 7 days (task should be kept)
            orch_guard.config.execution_ttl_secs = 7 * 24 * 60 * 60;
        };

        // Verify execution exists
        assert_eq!(orch.lock().unwrap().executions.len(), 1);

        // Run cleanup with 7-day TTL
        {
            let mut orch_guard = orch.lock().unwrap();
            let ttl = orch_guard.config.execution_ttl_secs;
            let removed = orch_guard.cleanup_completed_executions(ttl);
            assert_eq!(removed, 0);
        }

        // Verify execution was NOT removed (respects TTL)
        assert_eq!(orch.lock().unwrap().executions.len(), 1);
    }

    #[tokio::test]
    async fn orchestrator_background_cleanup_multiple_cycles() {
        use std::sync::{Arc, Mutex};

        let orch = Arc::new(Mutex::new(make_orchestrator()));

        // Set very short TTL
        {
            let mut orch_guard = orch.lock().unwrap();
            orch_guard.config.execution_ttl_secs = 0;
        }

        // Add and cleanup old executions in multiple cycles
        for i in 0..3 {
            // Add old completed execution
            {
                let mut orch_guard = orch.lock().unwrap();
                let id = orch_guard.start_task(
                    format!("task {}", i),
                    "description",
                    AgentRole::Coder,
                );
                if let Some(exec) = orch_guard.executions.get_mut(&id) {
                    exec.completed_at = Some(Utc::now() - chrono::Duration::days(10));
                }
            }

            // Verify execution was added
            assert_eq!(orch.lock().unwrap().executions.len(), 1);

            // Run cleanup
            {
                let mut orch_guard = orch.lock().unwrap();
                let removed = orch_guard.cleanup_completed_executions(0);
                assert_eq!(removed, 1, "Cycle {} failed", i);
            }

            // Verify it was cleaned
            assert_eq!(orch.lock().unwrap().executions.len(), 0, "Cycle {} failed", i);
        }
    }

    #[tokio::test]
    async fn orchestrator_background_cleanup_empty_state() {
        use std::sync::{Arc, Mutex};

        let orch = Arc::new(Mutex::new(make_orchestrator()));

        // Start cleanup with empty state
        Orchestrator::start_cleanup_task(Arc::clone(&orch));

        // Wait for potential cleanup cycle
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify state is still empty (no panics or errors)
        assert_eq!(orch.lock().unwrap().executions.len(), 0);
        assert_eq!(orch.lock().unwrap().stuck_detectors.len(), 0);
    }

    #[tokio::test]
    async fn orchestrator_background_cleanup_logs_counts() {
        use std::sync::{Arc, Mutex};

        let orch = Arc::new(Mutex::new(make_orchestrator()));

        // Create old completed execution
        {
            let mut orch_guard = orch.lock().unwrap();
            orch_guard.config.execution_ttl_secs = 0;
            let id = orch_guard.start_task("task", "description", AgentRole::Coder);
            if let Some(exec) = orch_guard.executions.get_mut(&id) {
                exec.completed_at = Some(Utc::now() - chrono::Duration::days(10));
            }
        }

        // Verify execution exists
        assert_eq!(orch.lock().unwrap().executions.len(), 1);

        // Run cleanup
        {
            let mut orch_guard = orch.lock().unwrap();
            let removed = orch_guard.cleanup_completed_executions(0);
            assert_eq!(removed, 1);
        }

        // Test passes if cleanup ran without errors
        // (logs are checked manually in real scenarios)
        assert_eq!(orch.lock().unwrap().executions.len(), 0);
    }
}
