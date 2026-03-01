//! Spec pipeline — multi-phase specification creation and validation.
//!
//! Mirrors Auto Claude's `apps/backend/spec/` with phases for:
//! - **Discovery**: Understand what needs to be built
//! - **Requirements**: Gather and structure requirements
//! - **Writing**: Produce a formal specification
//! - **Critique**: Review spec for completeness
//! - **Validation**: Verify implementation matches spec
//!
//! Each phase produces a `PhaseResult` that feeds into the next phase.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// PhaseResult — output of each spec phase
// ---------------------------------------------------------------------------

/// The result of executing a single spec phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResult {
    pub id: Uuid,
    pub phase: SpecPhase,
    pub status: PhaseStatus,
    /// The content produced by this phase.
    pub content: String,
    /// Structured findings or artifacts.
    pub artifacts: Vec<SpecArtifact>,
    /// Execution metrics.
    pub metrics: PhaseMetrics,
    pub created_at: DateTime<Utc>,
}

/// Which spec phase this result belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecPhase {
    Discovery,
    Requirements,
    Writing,
    Critique,
    Validation,
    Compaction,
}

impl SpecPhase {
    /// Get the next phase in the pipeline (if any).
    pub fn next(&self) -> Option<SpecPhase> {
        match self {
            SpecPhase::Discovery => Some(SpecPhase::Requirements),
            SpecPhase::Requirements => Some(SpecPhase::Writing),
            SpecPhase::Writing => Some(SpecPhase::Critique),
            SpecPhase::Critique => Some(SpecPhase::Validation),
            SpecPhase::Validation => None,
            SpecPhase::Compaction => None,
        }
    }

    /// Human-readable name.
    pub fn label(&self) -> &'static str {
        match self {
            SpecPhase::Discovery => "Discovery",
            SpecPhase::Requirements => "Requirements",
            SpecPhase::Writing => "Spec Writing",
            SpecPhase::Critique => "Critique",
            SpecPhase::Validation => "Validation",
            SpecPhase::Compaction => "Compaction",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Pending,
    Running,
    Complete,
    Failed,
    Skipped,
}

// ---------------------------------------------------------------------------
// SpecArtifact — structured outputs from phases
// ---------------------------------------------------------------------------

/// A structured artifact produced by a spec phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecArtifact {
    pub kind: ArtifactKind,
    pub title: String,
    pub content: String,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Requirement,
    AcceptanceCriteria,
    FileChange,
    DataModel,
    ApiContract,
    TestCase,
    Finding,
    Suggestion,
}

// ---------------------------------------------------------------------------
// PhaseMetrics — execution metrics for a phase
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhaseMetrics {
    pub tokens_used: usize,
    pub duration_ms: u64,
    pub files_read: usize,
    pub llm_calls: usize,
}

// ---------------------------------------------------------------------------
// Spec — the complete specification document
// ---------------------------------------------------------------------------

/// A complete task specification produced by the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub id: Uuid,
    pub task_title: String,
    pub task_description: String,
    /// Phase results in order.
    pub phases: Vec<PhaseResult>,
    /// Extracted requirements.
    pub requirements: Vec<Requirement>,
    /// Acceptance criteria.
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    /// Overall quality score (1-5).
    pub quality_score: Option<f64>,
    /// Files that need to be modified.
    pub affected_files: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Spec {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            task_title: title.into(),
            task_description: description.into(),
            phases: Vec::new(),
            requirements: Vec::new(),
            acceptance_criteria: Vec::new(),
            quality_score: None,
            affected_files: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a phase result.
    pub fn add_phase_result(&mut self, result: PhaseResult) {
        self.updated_at = Utc::now();
        self.phases.push(result);
    }

    /// Get the latest phase result.
    pub fn latest_phase(&self) -> Option<&PhaseResult> {
        self.phases.last()
    }

    /// Get phase result for a specific phase.
    pub fn get_phase(&self, phase: SpecPhase) -> Option<&PhaseResult> {
        self.phases.iter().find(|p| p.phase == phase)
    }

    /// Check if all required phases are complete.
    pub fn is_complete(&self) -> bool {
        let required = [
            SpecPhase::Discovery,
            SpecPhase::Requirements,
            SpecPhase::Writing,
        ];
        required.iter().all(|phase| {
            self.phases
                .iter()
                .any(|p| p.phase == *phase && p.status == PhaseStatus::Complete)
        })
    }

    /// Check if the spec has been validated.
    pub fn is_validated(&self) -> bool {
        self.phases
            .iter()
            .any(|p| p.phase == SpecPhase::Validation && p.status == PhaseStatus::Complete)
    }

    /// Total tokens used across all phases.
    pub fn total_tokens(&self) -> usize {
        self.phases.iter().map(|p| p.metrics.tokens_used).sum()
    }

    /// Add a requirement.
    pub fn add_requirement(&mut self, req: Requirement) {
        self.requirements.push(req);
    }

    /// Add an acceptance criterion.
    pub fn add_criterion(&mut self, criterion: AcceptanceCriterion) {
        self.acceptance_criteria.push(criterion);
    }

    /// Generate a summary of the spec suitable for agent context injection.
    pub fn to_context_summary(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("## Specification: {}", self.task_title));
        parts.push(self.task_description.clone());

        if !self.requirements.is_empty() {
            parts.push("\n### Requirements".into());
            for (i, req) in self.requirements.iter().enumerate() {
                parts.push(format!(
                    "{}. [{}] {}",
                    i + 1,
                    req.priority.label(),
                    req.description
                ));
            }
        }

        if !self.acceptance_criteria.is_empty() {
            parts.push("\n### Acceptance Criteria".into());
            for (i, ac) in self.acceptance_criteria.iter().enumerate() {
                let status = if ac.verified { "PASS" } else { "PENDING" };
                parts.push(format!("{}. [{}] {}", i + 1, status, ac.description));
            }
        }

        if !self.affected_files.is_empty() {
            parts.push("\n### Affected Files".into());
            for file in &self.affected_files {
                parts.push(format!("- {}", file));
            }
        }

        parts.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Requirement & AcceptanceCriterion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub id: Uuid,
    pub description: String,
    pub priority: RequirementPriority,
    pub source: String,
}

impl Requirement {
    pub fn new(
        description: impl Into<String>,
        priority: RequirementPriority,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            description: description.into(),
            priority,
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementPriority {
    Must,
    Should,
    Could,
    Wont,
}

impl RequirementPriority {
    pub fn label(&self) -> &'static str {
        match self {
            RequirementPriority::Must => "MUST",
            RequirementPriority::Should => "SHOULD",
            RequirementPriority::Could => "COULD",
            RequirementPriority::Wont => "WONT",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceCriterion {
    pub id: Uuid,
    pub description: String,
    pub verified: bool,
    pub evidence: Option<String>,
}

impl AcceptanceCriterion {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            description: description.into(),
            verified: false,
            evidence: None,
        }
    }

    pub fn verify(&mut self, evidence: impl Into<String>) {
        self.verified = true;
        self.evidence = Some(evidence.into());
    }
}

// ---------------------------------------------------------------------------
// SpecPipeline — orchestrates the spec creation process
// ---------------------------------------------------------------------------

/// Orchestrates the spec pipeline from discovery through validation.
pub struct SpecPipeline {
    specs: HashMap<Uuid, Spec>,
}

impl SpecPipeline {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
        }
    }

    /// Create a new spec.
    pub fn create_spec(
        &mut self,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> &Spec {
        let spec = Spec::new(title, description);
        let id = spec.id;
        self.specs.insert(id, spec);
        self.specs.get(&id).unwrap()
    }

    /// Get a spec by ID.
    pub fn get_spec(&self, id: &Uuid) -> Option<&Spec> {
        self.specs.get(id)
    }

    /// Get a mutable spec by ID.
    pub fn get_spec_mut(&mut self, id: &Uuid) -> Option<&mut Spec> {
        self.specs.get_mut(id)
    }

    /// Record a phase result for a spec.
    pub fn record_phase(&mut self, spec_id: &Uuid, result: PhaseResult) -> Result<(), SpecError> {
        let spec = self
            .specs
            .get_mut(spec_id)
            .ok_or(SpecError::NotFound(*spec_id))?;
        spec.add_phase_result(result);
        Ok(())
    }

    /// List all specs.
    pub fn list_specs(&self) -> Vec<&Spec> {
        self.specs.values().collect()
    }

    /// Get specs that are ready for the next phase.
    pub fn pending_work(&self) -> Vec<(&Spec, SpecPhase)> {
        let mut work = Vec::new();
        for spec in self.specs.values() {
            if let Some(latest) = spec.latest_phase() {
                if latest.status == PhaseStatus::Complete {
                    if let Some(next) = latest.phase.next() {
                        work.push((spec, next));
                    }
                }
            } else {
                // No phases yet — needs discovery
                work.push((spec, SpecPhase::Discovery));
            }
        }
        work
    }

    /// Number of specs.
    pub fn count(&self) -> usize {
        self.specs.len()
    }

    /// Remove a spec.
    pub fn remove_spec(&mut self, id: &Uuid) -> Option<Spec> {
        self.specs.remove(id)
    }
}

impl Default for SpecPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Complexity Assessment
// ---------------------------------------------------------------------------

/// Complexity assessment for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityAssessment {
    pub id: Uuid,
    pub task_title: String,
    /// Overall complexity rating (1-5).
    pub rating: u8,
    /// Estimated number of files to modify.
    pub estimated_files: usize,
    /// Estimated number of subtasks.
    pub estimated_subtasks: usize,
    /// Risk level.
    pub risk: RiskLevel,
    /// Breakdown by dimension.
    pub dimensions: Vec<ComplexityDimension>,
    pub assessed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityDimension {
    pub name: String,
    pub score: u8,
    pub rationale: String,
}

impl ComplexityAssessment {
    pub fn new(task_title: impl Into<String>, rating: u8) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_title: task_title.into(),
            rating: rating.min(5),
            estimated_files: 0,
            estimated_subtasks: 0,
            risk: RiskLevel::Medium,
            dimensions: Vec::new(),
            assessed_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur during spec pipeline operations.
///
/// This enum provides error handling for the spec creation and validation
/// pipeline, covering issues with spec lifecycle management and phase transitions.
///
/// # Examples
///
/// ```rust
/// use at_intelligence::spec::{SpecPipeline, SpecError, SpecPhase, PhaseResult, PhaseStatus, PhaseMetrics};
/// use uuid::Uuid;
/// use chrono::Utc;
///
/// fn handle_spec_operations(pipeline: &mut SpecPipeline) -> Result<(), SpecError> {
///     let spec = pipeline.create_spec("Add feature", "Description");
///     let spec_id = spec.id;
///
///     // Try to record a phase result
///     pipeline.record_phase(&spec_id, PhaseResult {
///         id: Uuid::new_v4(),
///         phase: SpecPhase::Discovery,
///         status: PhaseStatus::Complete,
///         content: "Analysis complete".to_string(),
///         artifacts: vec![],
///         metrics: PhaseMetrics::default(),
///         created_at: Utc::now(),
///     })?;
///
///     // Try with invalid ID - will return NotFound error
///     let invalid_id = Uuid::new_v4();
///     match pipeline.record_phase(&invalid_id, PhaseResult {
///         id: Uuid::new_v4(),
///         phase: SpecPhase::Requirements,
///         status: PhaseStatus::Complete,
///         content: "".to_string(),
///         artifacts: vec![],
///         metrics: PhaseMetrics::default(),
///         created_at: Utc::now(),
///     }) {
///         Err(SpecError::NotFound(_)) => println!("Spec not found"),
///         _ => {}
///     }
///
///     Ok(())
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    /// The requested spec was not found.
    ///
    /// This occurs when:
    /// - Attempting to retrieve a spec with an invalid ID
    /// - The spec was deleted or never created
    /// - Using a stale reference after removal
    ///
    /// The contained UUID identifies the spec that was not found.
    #[error("spec not found: {0}")]
    NotFound(Uuid),

    /// An invalid phase transition was attempted.
    ///
    /// This occurs when:
    /// - Trying to skip required phases in the pipeline
    /// - Attempting to transition backward in the phase sequence
    /// - Violating phase ordering constraints
    ///
    /// The `from` and `to` fields show the attempted invalid transition.
    /// Valid transitions follow: Discovery → Requirements → Writing → Critique → Validation.
    #[error("invalid phase transition: {from:?} -> {to:?}")]
    InvalidTransition { from: SpecPhase, to: SpecPhase },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_creation() {
        let spec = Spec::new("Fix login", "Login page crashes on submit");
        assert_eq!(spec.task_title, "Fix login");
        assert!(spec.phases.is_empty());
        assert!(!spec.is_complete());
    }

    #[test]
    fn spec_phase_progression() {
        assert_eq!(SpecPhase::Discovery.next(), Some(SpecPhase::Requirements));
        assert_eq!(SpecPhase::Requirements.next(), Some(SpecPhase::Writing));
        assert_eq!(SpecPhase::Writing.next(), Some(SpecPhase::Critique));
        assert_eq!(SpecPhase::Critique.next(), Some(SpecPhase::Validation));
        assert_eq!(SpecPhase::Validation.next(), None);
    }

    #[test]
    fn spec_is_complete() {
        let mut spec = Spec::new("test", "desc");
        assert!(!spec.is_complete());

        for phase in [
            SpecPhase::Discovery,
            SpecPhase::Requirements,
            SpecPhase::Writing,
        ] {
            spec.add_phase_result(PhaseResult {
                id: Uuid::new_v4(),
                phase,
                status: PhaseStatus::Complete,
                content: "done".into(),
                artifacts: vec![],
                metrics: PhaseMetrics::default(),
                created_at: Utc::now(),
            });
        }

        assert!(spec.is_complete());
        assert!(!spec.is_validated());
    }

    #[test]
    fn spec_is_validated() {
        let mut spec = Spec::new("test", "desc");
        spec.add_phase_result(PhaseResult {
            id: Uuid::new_v4(),
            phase: SpecPhase::Validation,
            status: PhaseStatus::Complete,
            content: "all criteria pass".into(),
            artifacts: vec![],
            metrics: PhaseMetrics::default(),
            created_at: Utc::now(),
        });
        assert!(spec.is_validated());
    }

    #[test]
    fn spec_requirements_and_criteria() {
        let mut spec = Spec::new("test", "desc");
        spec.add_requirement(Requirement::new(
            "Must support OAuth",
            RequirementPriority::Must,
            "user_request",
        ));
        spec.add_criterion(AcceptanceCriterion::new("OAuth login works"));

        assert_eq!(spec.requirements.len(), 1);
        assert_eq!(spec.acceptance_criteria.len(), 1);
    }

    #[test]
    fn acceptance_criterion_verification() {
        let mut ac = AcceptanceCriterion::new("Login works");
        assert!(!ac.verified);
        ac.verify("Test passed: login_test OK");
        assert!(ac.verified);
        assert!(ac.evidence.is_some());
    }

    #[test]
    fn spec_context_summary() {
        let mut spec = Spec::new("Add auth", "Add authentication to the API");
        spec.add_requirement(Requirement::new(
            "Support JWT tokens",
            RequirementPriority::Must,
            "spec",
        ));
        spec.add_criterion(AcceptanceCriterion::new("JWT validation works"));
        spec.affected_files.push("src/auth.rs".into());

        let summary = spec.to_context_summary();
        assert!(summary.contains("Add auth"));
        assert!(summary.contains("JWT"));
        assert!(summary.contains("src/auth.rs"));
    }

    #[test]
    fn spec_total_tokens() {
        let mut spec = Spec::new("t", "d");
        spec.add_phase_result(PhaseResult {
            id: Uuid::new_v4(),
            phase: SpecPhase::Discovery,
            status: PhaseStatus::Complete,
            content: "".into(),
            artifacts: vec![],
            metrics: PhaseMetrics {
                tokens_used: 500,
                ..Default::default()
            },
            created_at: Utc::now(),
        });
        spec.add_phase_result(PhaseResult {
            id: Uuid::new_v4(),
            phase: SpecPhase::Writing,
            status: PhaseStatus::Complete,
            content: "".into(),
            artifacts: vec![],
            metrics: PhaseMetrics {
                tokens_used: 1000,
                ..Default::default()
            },
            created_at: Utc::now(),
        });
        assert_eq!(spec.total_tokens(), 1500);
    }

    // -- Pipeline --

    #[test]
    fn pipeline_create_and_get() {
        let mut pipeline = SpecPipeline::new();
        let spec = pipeline.create_spec("Fix bug", "Details");
        let id = spec.id;

        assert_eq!(pipeline.count(), 1);
        assert!(pipeline.get_spec(&id).is_some());
    }

    #[test]
    fn pipeline_record_phase() {
        let mut pipeline = SpecPipeline::new();
        let spec = pipeline.create_spec("Fix", "d");
        let id = spec.id;

        pipeline
            .record_phase(
                &id,
                PhaseResult {
                    id: Uuid::new_v4(),
                    phase: SpecPhase::Discovery,
                    status: PhaseStatus::Complete,
                    content: "found stuff".into(),
                    artifacts: vec![],
                    metrics: PhaseMetrics::default(),
                    created_at: Utc::now(),
                },
            )
            .unwrap();

        assert_eq!(pipeline.get_spec(&id).unwrap().phases.len(), 1);
    }

    #[test]
    fn pipeline_record_phase_not_found() {
        let mut pipeline = SpecPipeline::new();
        let result = pipeline.record_phase(
            &Uuid::new_v4(),
            PhaseResult {
                id: Uuid::new_v4(),
                phase: SpecPhase::Discovery,
                status: PhaseStatus::Complete,
                content: "".into(),
                artifacts: vec![],
                metrics: PhaseMetrics::default(),
                created_at: Utc::now(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn pipeline_pending_work() {
        let mut pipeline = SpecPipeline::new();
        let spec = pipeline.create_spec("New", "desc");
        let id = spec.id;

        // New spec needs discovery
        let work = pipeline.pending_work();
        assert_eq!(work.len(), 1);
        assert_eq!(work[0].1, SpecPhase::Discovery);

        // After discovery, needs requirements
        pipeline
            .record_phase(
                &id,
                PhaseResult {
                    id: Uuid::new_v4(),
                    phase: SpecPhase::Discovery,
                    status: PhaseStatus::Complete,
                    content: "".into(),
                    artifacts: vec![],
                    metrics: PhaseMetrics::default(),
                    created_at: Utc::now(),
                },
            )
            .unwrap();

        let work = pipeline.pending_work();
        assert_eq!(work[0].1, SpecPhase::Requirements);
    }

    #[test]
    fn pipeline_remove_spec() {
        let mut pipeline = SpecPipeline::new();
        let spec = pipeline.create_spec("temp", "d");
        let id = spec.id;
        assert!(pipeline.remove_spec(&id).is_some());
        assert_eq!(pipeline.count(), 0);
    }

    // -- Complexity --

    #[test]
    fn complexity_assessment() {
        let assess = ComplexityAssessment::new("Add auth", 4);
        assert_eq!(assess.rating, 4);
        assert_eq!(assess.risk, RiskLevel::Medium);
    }

    #[test]
    fn complexity_rating_clamped() {
        let assess = ComplexityAssessment::new("huge", 10);
        assert_eq!(assess.rating, 5); // clamped
    }

    // -- Serialization --

    #[test]
    fn spec_serialization() {
        let spec = Spec::new("test", "desc");
        let json = serde_json::to_string(&spec).unwrap();
        let deser: Spec = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.task_title, "test");
    }

    #[test]
    fn phase_result_serialization() {
        let result = PhaseResult {
            id: Uuid::new_v4(),
            phase: SpecPhase::Discovery,
            status: PhaseStatus::Complete,
            content: "found".into(),
            artifacts: vec![SpecArtifact {
                kind: ArtifactKind::Requirement,
                title: "req1".into(),
                content: "must do X".into(),
                metadata: HashMap::new(),
            }],
            metrics: PhaseMetrics::default(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let deser: PhaseResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.phase, SpecPhase::Discovery);
        assert_eq!(deser.artifacts.len(), 1);
    }

    #[test]
    fn complexity_serialization() {
        let assess = ComplexityAssessment::new("task", 3);
        let json = serde_json::to_string(&assess).unwrap();
        let deser: ComplexityAssessment = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.rating, 3);
    }

    #[test]
    fn requirement_priority_labels() {
        assert_eq!(RequirementPriority::Must.label(), "MUST");
        assert_eq!(RequirementPriority::Should.label(), "SHOULD");
        assert_eq!(RequirementPriority::Could.label(), "COULD");
        assert_eq!(RequirementPriority::Wont.label(), "WONT");
    }

    #[test]
    fn phase_labels() {
        assert_eq!(SpecPhase::Discovery.label(), "Discovery");
        assert_eq!(SpecPhase::Validation.label(), "Validation");
    }

    #[test]
    fn spec_get_phase() {
        let mut spec = Spec::new("t", "d");
        spec.add_phase_result(PhaseResult {
            id: Uuid::new_v4(),
            phase: SpecPhase::Discovery,
            status: PhaseStatus::Complete,
            content: "done".into(),
            artifacts: vec![],
            metrics: PhaseMetrics::default(),
            created_at: Utc::now(),
        });
        assert!(spec.get_phase(SpecPhase::Discovery).is_some());
        assert!(spec.get_phase(SpecPhase::Writing).is_none());
    }
}
