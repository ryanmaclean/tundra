//! Runner system — executes agent sessions for spec, insights, ideation, and roadmap.
//!
//! Mirrors Auto Claude's `apps/backend/runners/` with:
//! - **SpecRunner**: Drives the spec pipeline through its phases
//! - **InsightsRunner**: Extracts insights from completed sessions
//! - **IdeationRunner**: Generates improvement ideas by category
//! - **RoadmapRunner**: Discovers and plans future features
//! - **AnalysisRunner**: AI-powered codebase analysis

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::spec::{PhaseResult, PhaseStatus, SpecPhase};
use at_core::types::{QaIssue, QaReport, QaSeverity, QaStatus};

// ---------------------------------------------------------------------------
// RunnerResult — common result type for all runners
// ---------------------------------------------------------------------------

/// Result of executing a runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerResult {
    pub id: Uuid,
    pub runner_type: RunnerType,
    pub status: RunnerStatus,
    pub output: String,
    pub metrics: RunnerMetrics,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerType {
    Spec,
    Insights,
    Ideation,
    Roadmap,
    Analysis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerStatus {
    Pending,
    Running,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerMetrics {
    pub total_tokens: usize,
    pub total_duration_ms: u64,
    pub phases_completed: usize,
    pub errors: usize,
}

impl Default for RunnerMetrics {
    fn default() -> Self {
        Self {
            total_tokens: 0,
            total_duration_ms: 0,
            phases_completed: 0,
            errors: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// SpecRunner — drives the spec pipeline
// ---------------------------------------------------------------------------

/// Drives a specification through its phases (discovery → requirements → writing → critique → validation).
pub struct SpecRunner {
    /// Phase results collected during the run.
    results: Vec<PhaseResult>,
    /// Current phase.
    current_phase: SpecPhase,
    /// Whether the runner has been cancelled.
    cancelled: bool,
}

impl SpecRunner {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            current_phase: SpecPhase::Discovery,
            cancelled: false,
        }
    }

    /// Get the current phase.
    pub fn current_phase(&self) -> SpecPhase {
        self.current_phase
    }

    /// Record a phase result and advance to the next phase.
    pub fn record_result(&mut self, result: PhaseResult) -> Option<SpecPhase> {
        let next = result.phase.next();
        self.results.push(result);
        if let Some(next_phase) = next {
            self.current_phase = next_phase;
        }
        next
    }

    /// Get all phase results.
    pub fn results(&self) -> &[PhaseResult] {
        &self.results
    }

    /// Cancel the run.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Build a summary of metrics across all phases.
    pub fn summary_metrics(&self) -> RunnerMetrics {
        let mut metrics = RunnerMetrics::default();
        for result in &self.results {
            metrics.total_tokens += result.metrics.tokens_used;
            metrics.total_duration_ms += result.metrics.duration_ms;
            if result.status == PhaseStatus::Complete {
                metrics.phases_completed += 1;
            }
            if result.status == PhaseStatus::Failed {
                metrics.errors += 1;
            }
        }
        metrics
    }
}

impl Default for SpecRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// InsightsRunner — extracts session insights
// ---------------------------------------------------------------------------

/// Extracts insights from completed agent sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInsight {
    pub id: Uuid,
    pub session_id: Uuid,
    pub category: InsightCategory,
    pub content: String,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightCategory {
    Pattern,
    Error,
    Performance,
    Dependency,
    Convention,
    Process,
}

/// Collects insights from agent sessions.
pub struct InsightsRunner {
    insights: Vec<SessionInsight>,
}

impl InsightsRunner {
    pub fn new() -> Self {
        Self {
            insights: Vec::new(),
        }
    }

    /// Add an insight from a session.
    pub fn add_insight(
        &mut self,
        session_id: Uuid,
        category: InsightCategory,
        content: impl Into<String>,
        confidence: f64,
    ) -> Uuid {
        let insight = SessionInsight {
            id: Uuid::new_v4(),
            session_id,
            category,
            content: content.into(),
            confidence: confidence.clamp(0.0, 1.0),
            created_at: Utc::now(),
        };
        let id = insight.id;
        self.insights.push(insight);
        id
    }

    /// Get all insights.
    pub fn all_insights(&self) -> &[SessionInsight] {
        &self.insights
    }

    /// Get insights for a specific session.
    pub fn for_session(&self, session_id: &Uuid) -> Vec<&SessionInsight> {
        self.insights
            .iter()
            .filter(|i| i.session_id == *session_id)
            .collect()
    }

    /// Get insights by category.
    pub fn by_category(&self, category: InsightCategory) -> Vec<&SessionInsight> {
        self.insights
            .iter()
            .filter(|i| i.category == category)
            .collect()
    }

    /// Count of insights.
    pub fn count(&self) -> usize {
        self.insights.len()
    }

    /// Generate a context summary of recent insights.
    pub fn context_summary(&self, limit: usize) -> String {
        let mut sorted: Vec<&SessionInsight> = self.insights.iter().collect();
        sorted.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        sorted.truncate(limit);

        let mut parts = vec!["## Recent Insights".to_string()];
        for insight in sorted {
            parts.push(format!(
                "- [{:?}] (conf: {:.0}%) {}",
                insight.category,
                insight.confidence * 100.0,
                insight.content,
            ));
        }
        parts.join("\n")
    }
}

impl Default for InsightsRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// QaRunner — runs QA checks and produces QaReport
// ---------------------------------------------------------------------------

/// Runs QA checks on a completed coding phase and produces a QaReport.
pub struct QaRunner {
    report: Option<QaReport>,
}

impl QaRunner {
    pub fn new() -> Self {
        Self { report: None }
    }

    /// Run QA checks for a task (placeholder implementation).
    /// In a full implementation, this would:
    /// - Analyze code changes in the worktree
    /// - Run linters, formatters, tests
    /// - Check for security issues, performance problems
    /// - Generate structured QaIssue objects
    pub fn run_qa_checks(
        &mut self,
        task_id: Uuid,
        _task_title: &str,
        worktree_path: Option<&str>,
    ) -> QaReport {
        // Placeholder: create a basic QA report
        // In production, this would analyze the codebase, run tests, etc.
        let mut report = QaReport::new(task_id, QaStatus::Pending);

        // Simulate some checks
        if let Some(worktree) = worktree_path {
            // Placeholder: check if worktree exists and has changes
            report.issues.push(QaIssue {
                id: Uuid::new_v4(),
                severity: QaSeverity::Minor,
                description: format!("Worktree found at {}", worktree),
                file: None,
                line: None,
            });
        }

        // Determine status based on issues
        let critical_count = report
            .issues
            .iter()
            .filter(|i| i.severity == QaSeverity::Critical)
            .count();
        let major_count = report
            .issues
            .iter()
            .filter(|i| i.severity == QaSeverity::Major)
            .count();

        report.status = if critical_count > 0 || major_count > 2 {
            QaStatus::Failed
        } else if report.issues.is_empty() {
            QaStatus::Passed
        } else {
            QaStatus::Pending
        };

        self.report = Some(report.clone());
        report
    }

    /// Get the generated report.
    pub fn report(&self) -> Option<&QaReport> {
        self.report.as_ref()
    }
}

impl Default for QaRunner {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// RecoveryAction — handles failed sessions
// ---------------------------------------------------------------------------

/// Actions that can be taken to recover from a failed session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    /// Retry the same phase with the same inputs.
    Retry,
    /// Roll back git changes and retry.
    Rollback,
    /// Skip this phase and continue.
    Skip,
    /// Escalate to a human or higher-level agent.
    Escalate,
}

/// Records of recovery actions taken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryRecord {
    pub id: Uuid,
    pub session_id: Uuid,
    pub phase: SpecPhase,
    pub action: RecoveryAction,
    pub reason: String,
    pub successful: bool,
    pub timestamp: DateTime<Utc>,
}

/// Manages recovery from failed sessions.
pub struct RecoveryManager {
    records: Vec<RecoveryRecord>,
}

impl RecoveryManager {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Record a recovery action.
    pub fn record(
        &mut self,
        session_id: Uuid,
        phase: SpecPhase,
        action: RecoveryAction,
        reason: impl Into<String>,
    ) -> Uuid {
        let record = RecoveryRecord {
            id: Uuid::new_v4(),
            session_id,
            phase,
            action,
            reason: reason.into(),
            successful: false,
            timestamp: Utc::now(),
        };
        let id = record.id;
        self.records.push(record);
        id
    }

    /// Mark a recovery as successful.
    pub fn mark_successful(&mut self, id: &Uuid) -> bool {
        if let Some(record) = self.records.iter_mut().find(|r| r.id == *id) {
            record.successful = true;
            true
        } else {
            false
        }
    }

    /// Get recovery history for a session.
    pub fn for_session(&self, session_id: &Uuid) -> Vec<&RecoveryRecord> {
        self.records
            .iter()
            .filter(|r| r.session_id == *session_id)
            .collect()
    }

    /// Suggest a recovery action based on failure history.
    pub fn suggest_action(&self, session_id: &Uuid, phase: SpecPhase) -> RecoveryAction {
        let history = self.for_session(session_id);
        let phase_failures: Vec<_> = history.iter().filter(|r| r.phase == phase).collect();

        match phase_failures.len() {
            0 => RecoveryAction::Retry,
            1 => RecoveryAction::Rollback,
            2 => RecoveryAction::Skip,
            _ => RecoveryAction::Escalate,
        }
    }

    /// Number of recovery records.
    pub fn count(&self) -> usize {
        self.records.len()
    }
}

impl Default for RecoveryManager {
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
    use crate::spec::PhaseMetrics;

    // -- SpecRunner --

    #[test]
    fn spec_runner_new() {
        let runner = SpecRunner::new();
        assert_eq!(runner.current_phase(), SpecPhase::Discovery);
        assert!(runner.results().is_empty());
    }

    #[test]
    fn spec_runner_record_and_advance() {
        let mut runner = SpecRunner::new();
        let next = runner.record_result(PhaseResult {
            id: Uuid::new_v4(),
            phase: SpecPhase::Discovery,
            status: PhaseStatus::Complete,
            content: "found".into(),
            artifacts: vec![],
            metrics: PhaseMetrics::default(),
            created_at: Utc::now(),
        });
        assert_eq!(next, Some(SpecPhase::Requirements));
        assert_eq!(runner.current_phase(), SpecPhase::Requirements);
        assert_eq!(runner.results().len(), 1);
    }

    #[test]
    fn spec_runner_cancel() {
        let mut runner = SpecRunner::new();
        assert!(!runner.is_cancelled());
        runner.cancel();
        assert!(runner.is_cancelled());
    }

    #[test]
    fn spec_runner_summary_metrics() {
        let mut runner = SpecRunner::new();
        runner.record_result(PhaseResult {
            id: Uuid::new_v4(),
            phase: SpecPhase::Discovery,
            status: PhaseStatus::Complete,
            content: "".into(),
            artifacts: vec![],
            metrics: PhaseMetrics {
                tokens_used: 500,
                duration_ms: 100,
                files_read: 3,
                llm_calls: 1,
            },
            created_at: Utc::now(),
        });
        runner.record_result(PhaseResult {
            id: Uuid::new_v4(),
            phase: SpecPhase::Requirements,
            status: PhaseStatus::Failed,
            content: "".into(),
            artifacts: vec![],
            metrics: PhaseMetrics {
                tokens_used: 200,
                duration_ms: 50,
                files_read: 1,
                llm_calls: 1,
            },
            created_at: Utc::now(),
        });

        let metrics = runner.summary_metrics();
        assert_eq!(metrics.total_tokens, 700);
        assert_eq!(metrics.phases_completed, 1);
        assert_eq!(metrics.errors, 1);
    }

    // -- InsightsRunner --

    #[test]
    fn insights_runner_add_and_get() {
        let mut runner = InsightsRunner::new();
        let session_id = Uuid::new_v4();
        runner.add_insight(
            session_id,
            InsightCategory::Pattern,
            "Use iterators over loops",
            0.9,
        );
        assert_eq!(runner.count(), 1);
        assert_eq!(runner.for_session(&session_id).len(), 1);
    }

    #[test]
    fn insights_runner_by_category() {
        let mut runner = InsightsRunner::new();
        let sid = Uuid::new_v4();
        runner.add_insight(sid, InsightCategory::Pattern, "p1", 0.8);
        runner.add_insight(sid, InsightCategory::Error, "e1", 0.7);
        runner.add_insight(sid, InsightCategory::Pattern, "p2", 0.6);

        assert_eq!(runner.by_category(InsightCategory::Pattern).len(), 2);
        assert_eq!(runner.by_category(InsightCategory::Error).len(), 1);
    }

    #[test]
    fn insights_runner_context_summary() {
        let mut runner = InsightsRunner::new();
        let sid = Uuid::new_v4();
        runner.add_insight(sid, InsightCategory::Pattern, "Use Option", 0.9);
        runner.add_insight(sid, InsightCategory::Convention, "snake_case", 0.7);

        let summary = runner.context_summary(5);
        assert!(summary.contains("Recent Insights"));
        assert!(summary.contains("Use Option"));
    }

    #[test]
    fn insights_confidence_clamped() {
        let mut runner = InsightsRunner::new();
        let sid = Uuid::new_v4();
        runner.add_insight(sid, InsightCategory::Pattern, "test", 1.5);
        assert!(runner.all_insights()[0].confidence <= 1.0);
    }

    // -- RecoveryManager --

    #[test]
    fn recovery_manager_record() {
        let mut mgr = RecoveryManager::new();
        let sid = Uuid::new_v4();
        let id = mgr.record(sid, SpecPhase::Discovery, RecoveryAction::Retry, "timeout");
        assert_eq!(mgr.count(), 1);
        assert!(!mgr.for_session(&sid)[0].successful);

        mgr.mark_successful(&id);
        assert!(mgr.for_session(&sid)[0].successful);
    }

    #[test]
    fn recovery_suggest_action_escalation() {
        let mut mgr = RecoveryManager::new();
        let sid = Uuid::new_v4();

        // First failure: suggest retry
        assert_eq!(
            mgr.suggest_action(&sid, SpecPhase::Discovery),
            RecoveryAction::Retry
        );

        // After 1 failure: suggest retry
        mgr.record(sid, SpecPhase::Discovery, RecoveryAction::Retry, "err1");
        assert_eq!(
            mgr.suggest_action(&sid, SpecPhase::Discovery),
            RecoveryAction::Rollback
        );

        // After 2 failures: suggest skip
        mgr.record(sid, SpecPhase::Discovery, RecoveryAction::Rollback, "err2");
        assert_eq!(
            mgr.suggest_action(&sid, SpecPhase::Discovery),
            RecoveryAction::Skip
        );

        // After 3+ failures: escalate
        mgr.record(sid, SpecPhase::Discovery, RecoveryAction::Skip, "err3");
        assert_eq!(
            mgr.suggest_action(&sid, SpecPhase::Discovery),
            RecoveryAction::Escalate
        );
    }

    #[test]
    fn recovery_mark_nonexistent() {
        let mut mgr = RecoveryManager::new();
        assert!(!mgr.mark_successful(&Uuid::new_v4()));
    }

    // -- Serialization --

    #[test]
    fn runner_result_serialization() {
        let result = RunnerResult {
            id: Uuid::new_v4(),
            runner_type: RunnerType::Spec,
            status: RunnerStatus::Complete,
            output: "done".into(),
            metrics: RunnerMetrics::default(),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let deser: RunnerResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.runner_type, RunnerType::Spec);
    }

    #[test]
    fn session_insight_serialization() {
        let insight = SessionInsight {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            category: InsightCategory::Pattern,
            content: "use iterators".into(),
            confidence: 0.85,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&insight).unwrap();
        let deser: SessionInsight = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.category, InsightCategory::Pattern);
    }

    #[test]
    fn recovery_record_serialization() {
        let record = RecoveryRecord {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            phase: SpecPhase::Writing,
            action: RecoveryAction::Rollback,
            reason: "test failure".into(),
            successful: false,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let deser: RecoveryRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.action, RecoveryAction::Rollback);
    }
}
