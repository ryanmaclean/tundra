//! Recursive Language Model (RLM) patterns for agent orchestration.
//!
//! Implements the 2026 RLM paradigm (MIT/Prime Intellect) adapted for
//! multi-agent software engineering:
//!
//! - **Context folding**: Compress large inputs into external storage,
//!   let agents inspect slices programmatically
//! - **Recursive decomposition**: Break tasks into sub-tasks, dispatch
//!   to sub-agents, synthesize results
//! - **Progressive refinement**: Iteratively improve outputs across
//!   multiple agent turns (answer diffusion)
//! - **Token efficiency**: Main orchestrator never sees full context;
//!   sub-agents get only their relevant slice
//!
//! Key insight: "Context management matters more than context length."
//! Instead of cramming 10M tokens into one call, fold context recursively.

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Context Fold — external storage for large contexts
// ---------------------------------------------------------------------------

/// A context fold stores large data externally and provides slice access.
///
/// Inspired by RLM's Python REPL pattern: the main model never sees the
/// full input. Instead, it asks for slices, searches, and transformations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFold {
    pub id: Uuid,
    /// Human-readable label.
    pub label: String,
    /// The full content (stored externally, not in LLM context).
    pub content: String,
    /// Total estimated tokens.
    pub total_tokens: usize,
    /// Pre-computed summaries at different granularities.
    pub summaries: Vec<FoldSummary>,
    /// Index of named sections for fast lookup.
    pub sections: HashMap<String, SectionSpan>,
    pub created_at: DateTime<Utc>,
}

/// A summary at a specific compression level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldSummary {
    /// Compression ratio (e.g., 0.1 = 10% of original).
    pub ratio: f64,
    /// The summarized text.
    pub text: String,
    /// Estimated tokens.
    pub tokens: usize,
}

/// A named section within the fold (line range).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionSpan {
    pub start_line: usize,
    pub end_line: usize,
}

impl ContextFold {
    pub fn new(label: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let total_tokens = content.len() / 4;
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            content,
            total_tokens,
            summaries: Vec::new(),
            sections: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Get a slice of the content by line range.
    pub fn slice(&self, start_line: usize, end_line: usize) -> String {
        let lines: Vec<&str> = self.content.lines().collect();
        let start = start_line.min(lines.len());
        let end = end_line.min(lines.len());
        lines[start..end].join("\n")
    }

    /// Get a named section.
    pub fn get_section(&self, name: &str) -> Option<String> {
        self.sections
            .get(name)
            .map(|span| self.slice(span.start_line, span.end_line))
    }

    /// Search the content for a pattern (simple substring search).
    pub fn search(&self, pattern: &str) -> Vec<SearchHit> {
        let pattern_lower = pattern.to_lowercase();
        self.content
            .lines()
            .enumerate()
            .filter(|(_, line)| line.to_lowercase().contains(&pattern_lower))
            .map(|(line_num, line)| SearchHit {
                line_num,
                content: line.to_string(),
            })
            .collect()
    }

    /// Add a pre-computed summary.
    pub fn add_summary(&mut self, ratio: f64, text: impl Into<String>) {
        let text = text.into();
        let tokens = text.len() / 4;
        self.summaries.push(FoldSummary {
            ratio,
            text,
            tokens,
        });
    }

    /// Get the best summary for a given token budget.
    pub fn best_summary(&self, token_budget: usize) -> Option<&FoldSummary> {
        // Find the most detailed summary that fits the budget
        self.summaries
            .iter()
            .filter(|s| s.tokens <= token_budget)
            .max_by(|a, b| a.ratio.partial_cmp(&b.ratio).unwrap())
    }

    /// Register a named section.
    pub fn register_section(
        &mut self,
        name: impl Into<String>,
        start_line: usize,
        end_line: usize,
    ) {
        self.sections.insert(
            name.into(),
            SectionSpan {
                start_line,
                end_line,
            },
        );
    }

    /// Auto-detect sections based on markdown headers.
    pub fn auto_detect_sections(&mut self) {
        let mut current_section: Option<(String, usize)> = None;

        for (i, line) in self.content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("# ")
                || trimmed.starts_with("## ")
                || trimmed.starts_with("### ")
            {
                // Close previous section
                if let Some((name, start)) = current_section.take() {
                    self.sections.insert(
                        name,
                        SectionSpan {
                            start_line: start,
                            end_line: i,
                        },
                    );
                }
                // Start new section
                let name = trimmed
                    .trim_start_matches('#')
                    .trim()
                    .to_lowercase()
                    .replace(' ', "_");
                current_section = Some((name, i));
            }
        }

        // Close last section
        let line_count = self.content.lines().count();
        if let Some((name, start)) = current_section {
            self.sections.insert(
                name,
                SectionSpan {
                    start_line: start,
                    end_line: line_count,
                },
            );
        }
    }

    /// Total line count.
    pub fn line_count(&self) -> usize {
        self.content.lines().count()
    }
}

/// A search hit within a context fold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub line_num: usize,
    pub content: String,
}

// ---------------------------------------------------------------------------
// RecursiveDecomposition — break tasks into sub-tasks
// ---------------------------------------------------------------------------

/// A recursive task decomposition tree.
///
/// The orchestrator decomposes a task, dispatches sub-tasks to sub-agents,
/// and synthesizes their results. This mirrors RLM's `llm_batch()` pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decomposition {
    pub id: Uuid,
    pub task_description: String,
    /// Sub-tasks at this level, indexed by ID for O(1) lookup.
    pub subtasks: HashMap<Uuid, SubTask>,
    /// Next sequence number for maintaining insertion order.
    next_sequence: usize,
    /// Synthesis strategy for combining sub-task results.
    pub synthesis: SynthesisStrategy,
    /// Maximum recursion depth.
    pub max_depth: usize,
    /// Current recursion depth.
    pub depth: usize,
}

/// A sub-task in the decomposition tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub id: Uuid,
    pub description: String,
    pub status: SubTaskStatus,
    /// Sequence number for maintaining insertion order.
    pub sequence: usize,
    /// Context slice for this sub-task (reference into a ContextFold).
    pub context_fold_id: Option<Uuid>,
    pub context_slice: Option<(usize, usize)>,
    /// Result from executing this sub-task.
    pub result: Option<String>,
    /// Assigned agent role.
    pub agent_role: Option<String>,
    /// Whether this can run in parallel with siblings.
    pub parallelizable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubTaskStatus {
    Pending,
    Running,
    Complete,
    Failed,
    Skipped,
}

/// Strategy for synthesizing sub-task results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SynthesisStrategy {
    /// Concatenate all results in order.
    Concatenate,
    /// Use an LLM to merge/summarize results.
    LlmMerge,
    /// Take the best result (by score or last).
    BestOf,
    /// Voting — multiple agents produce answers, take majority.
    Vote,
    /// Progressive refinement — each result refines the previous.
    Refine,
}

impl Decomposition {
    pub fn new(task: impl Into<String>, max_depth: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_description: task.into(),
            subtasks: HashMap::new(),
            next_sequence: 0,
            synthesis: SynthesisStrategy::Concatenate,
            max_depth,
            depth: 0,
        }
    }

    /// Add a sub-task.
    pub fn add_subtask(&mut self, description: impl Into<String>) -> Uuid {
        let subtask = SubTask {
            id: Uuid::new_v4(),
            description: description.into(),
            status: SubTaskStatus::Pending,
            sequence: self.next_sequence,
            context_fold_id: None,
            context_slice: None,
            result: None,
            agent_role: None,
            parallelizable: true,
        };
        let id = subtask.id;
        self.next_sequence += 1;
        self.subtasks.insert(id, subtask);
        id
    }

    /// Get pending sub-tasks ready for dispatch.
    pub fn pending_subtasks(&self) -> Vec<&SubTask> {
        self.subtasks
            .values()
            .filter(|s| s.status == SubTaskStatus::Pending)
            .collect()
    }

    /// Get parallelizable pending sub-tasks.
    pub fn parallel_batch(&self) -> Vec<&SubTask> {
        self.subtasks
            .values()
            .filter(|s| s.status == SubTaskStatus::Pending && s.parallelizable)
            .collect()
    }

    /// Record a sub-task result.
    pub fn record_result(&mut self, subtask_id: &Uuid, result: impl Into<String>) -> bool {
        if let Some(st) = self.subtasks.get_mut(subtask_id) {
            st.result = Some(result.into());
            st.status = SubTaskStatus::Complete;
            true
        } else {
            false
        }
    }

    /// Mark a sub-task as failed.
    pub fn mark_failed(&mut self, subtask_id: &Uuid) -> bool {
        if let Some(st) = self.subtasks.get_mut(subtask_id) {
            st.status = SubTaskStatus::Failed;
            true
        } else {
            false
        }
    }

    /// Check if all sub-tasks are complete.
    pub fn is_complete(&self) -> bool {
        !self.subtasks.is_empty()
            && self
                .subtasks
                .values()
                .all(|s| s.status == SubTaskStatus::Complete || s.status == SubTaskStatus::Skipped)
    }

    /// Check if any sub-task failed.
    pub fn has_failures(&self) -> bool {
        self.subtasks
            .values()
            .any(|s| s.status == SubTaskStatus::Failed)
    }

    /// Synthesize results from completed sub-tasks.
    pub fn synthesize(&self) -> String {
        // Collect subtasks with results and sort by sequence to maintain order
        let mut completed: Vec<_> = self
            .subtasks
            .values()
            .filter_map(|s| s.result.as_ref().map(|r| (s.sequence, r.as_str())))
            .collect();
        completed.sort_by_key(|(seq, _)| *seq);
        let results: Vec<&str> = completed.iter().map(|(_, r)| *r).collect();

        match self.synthesis {
            SynthesisStrategy::Concatenate => results.join("\n\n---\n\n"),
            SynthesisStrategy::BestOf => results.last().copied().unwrap_or("").to_string(),
            SynthesisStrategy::Refine => results.last().copied().unwrap_or("").to_string(),
            // LlmMerge and Vote require actual LLM calls — return concat as fallback
            SynthesisStrategy::LlmMerge | SynthesisStrategy::Vote => results.join("\n\n---\n\n"),
        }
    }

    /// Whether further recursion is allowed.
    pub fn can_recurse(&self) -> bool {
        self.depth < self.max_depth
    }

    /// Create a child decomposition for a sub-task.
    pub fn child(&self, task: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_description: task.into(),
            subtasks: HashMap::new(),
            next_sequence: 0,
            synthesis: self.synthesis,
            max_depth: self.max_depth,
            depth: self.depth + 1,
        }
    }
}

// ---------------------------------------------------------------------------
// ProgressiveRefinement — iterative answer improvement
// ---------------------------------------------------------------------------

/// Tracks progressive refinement of an answer across multiple agent turns.
///
/// Inspired by RLM's "answer diffusion" — the agent refines its answer
/// over multiple iterations rather than committing to a single response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressiveRefinement {
    pub id: Uuid,
    pub task: String,
    /// Answer revisions in order.
    pub revisions: Vec<Revision>,
    /// Maximum revisions allowed.
    pub max_revisions: usize,
    /// Whether the answer has been finalized.
    pub finalized: bool,
}

/// A single revision of an answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Revision {
    pub version: usize,
    pub content: String,
    /// What changed from the previous version.
    pub delta: Option<String>,
    /// Confidence score (0.0–1.0).
    pub confidence: f64,
    pub timestamp: DateTime<Utc>,
}

impl ProgressiveRefinement {
    pub fn new(task: impl Into<String>, max_revisions: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            task: task.into(),
            revisions: Vec::new(),
            max_revisions,
            finalized: false,
        }
    }

    /// Submit a new revision.
    pub fn revise(
        &mut self,
        content: impl Into<String>,
        delta: Option<String>,
        confidence: f64,
    ) -> bool {
        if self.finalized || self.revisions.len() >= self.max_revisions {
            return false;
        }
        let version = self.revisions.len() + 1;
        self.revisions.push(Revision {
            version,
            content: content.into(),
            delta,
            confidence: confidence.clamp(0.0, 1.0),
            timestamp: Utc::now(),
        });
        true
    }

    /// Get the latest revision.
    pub fn latest(&self) -> Option<&Revision> {
        self.revisions.last()
    }

    /// Finalize the answer (no more revisions).
    pub fn finalize(&mut self) {
        self.finalized = true;
    }

    /// Check if confidence is above threshold.
    pub fn is_confident(&self, threshold: f64) -> bool {
        self.latest()
            .map(|r| r.confidence >= threshold)
            .unwrap_or(false)
    }

    /// Number of revisions so far.
    pub fn revision_count(&self) -> usize {
        self.revisions.len()
    }

    /// Whether more revisions are allowed.
    pub fn can_revise(&self) -> bool {
        !self.finalized && self.revisions.len() < self.max_revisions
    }
}

// ---------------------------------------------------------------------------
// StuckDetector — detect and recover from stuck agents
// ---------------------------------------------------------------------------

/// Detects when an agent is stuck and triggers recovery.
///
/// Monitors agent progress and detects:
/// - No output for a configurable duration
/// - Repeated identical outputs (loops)
/// - Token budget exhaustion without progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StuckDetector {
    /// Maximum seconds without progress before declaring stuck.
    pub timeout_secs: u64,
    /// Maximum number of identical consecutive outputs before declaring stuck.
    pub max_repeats: usize,
    /// Recent outputs for loop detection.
    recent_outputs: VecDeque<String>,
    /// Timestamp of last meaningful progress.
    last_progress: DateTime<Utc>,
    /// Total tokens consumed.
    tokens_consumed: usize,
    /// Token budget.
    token_budget: usize,
}

/// Why an agent was detected as stuck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StuckReason {
    Timeout,
    OutputLoop,
    BudgetExhausted,
    NoProgress,
}

impl StuckDetector {
    pub fn new(timeout_secs: u64, token_budget: usize) -> Self {
        Self {
            timeout_secs,
            max_repeats: 3,
            recent_outputs: VecDeque::new(),
            last_progress: Utc::now(),
            tokens_consumed: 0,
            token_budget,
        }
    }

    /// Record an agent output.
    pub fn record_output(&mut self, output: &str, tokens: usize) {
        self.tokens_consumed += tokens;
        self.recent_outputs.push_back(output.to_string());

        // Keep only last N outputs for loop detection
        if self.recent_outputs.len() > self.max_repeats + 1 {
            self.recent_outputs.pop_front();
        }

        // If output is different from previous, count as progress
        if self.recent_outputs.len() < 2
            || self.recent_outputs.back() != self.recent_outputs.get(self.recent_outputs.len() - 2)
        {
            self.last_progress = Utc::now();
        }
    }

    /// Check if the agent is stuck.
    pub fn check(&self) -> Option<StuckReason> {
        // Check timeout
        let elapsed = Utc::now()
            .signed_duration_since(self.last_progress)
            .num_seconds() as u64;
        if elapsed > self.timeout_secs {
            return Some(StuckReason::Timeout);
        }

        // Check output loop
        if self.recent_outputs.len() >= self.max_repeats {
            let last = &self.recent_outputs[self.recent_outputs.len() - 1];
            let all_same = self
                .recent_outputs
                .iter()
                .rev()
                .take(self.max_repeats)
                .all(|o| o == last);
            if all_same && !last.is_empty() {
                return Some(StuckReason::OutputLoop);
            }
        }

        // Check budget exhaustion
        if self.tokens_consumed >= self.token_budget {
            return Some(StuckReason::BudgetExhausted);
        }

        None
    }

    /// Reset the detector (after recovery).
    pub fn reset(&mut self) {
        self.recent_outputs.clear();
        self.last_progress = Utc::now();
        self.tokens_consumed = 0;
    }

    /// Tokens remaining in budget.
    pub fn tokens_remaining(&self) -> usize {
        self.token_budget.saturating_sub(self.tokens_consumed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- ContextFold --

    #[test]
    fn context_fold_creation() {
        let fold = ContextFold::new("test", "line 1\nline 2\nline 3");
        assert_eq!(fold.label, "test");
        assert_eq!(fold.line_count(), 3);
        assert!(fold.total_tokens > 0);
    }

    #[test]
    fn context_fold_slice() {
        let fold = ContextFold::new("test", "a\nb\nc\nd\ne");
        assert_eq!(fold.slice(1, 3), "b\nc");
        assert_eq!(fold.slice(0, 1), "a");
    }

    #[test]
    fn context_fold_slice_clamped() {
        let fold = ContextFold::new("test", "a\nb");
        let slice = fold.slice(0, 100);
        assert_eq!(slice, "a\nb");
    }

    #[test]
    fn context_fold_search() {
        let fold = ContextFold::new("test", "hello world\nfoo bar\nhello again");
        let hits = fold.search("hello");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].line_num, 0);
        assert_eq!(hits[1].line_num, 2);
    }

    #[test]
    fn context_fold_search_case_insensitive() {
        let fold = ContextFold::new("test", "Hello World\nHELLO");
        let hits = fold.search("hello");
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn context_fold_sections() {
        let mut fold = ContextFold::new("test", "a\nb\nc\nd");
        fold.register_section("middle", 1, 3);
        let section = fold.get_section("middle").unwrap();
        assert_eq!(section, "b\nc");
        assert!(fold.get_section("nonexistent").is_none());
    }

    #[test]
    fn context_fold_auto_detect_sections() {
        let content = "# Intro\nIntro text\n## Methods\nMethod text\n### Details\nDetail text";
        let mut fold = ContextFold::new("test", content);
        fold.auto_detect_sections();
        assert!(fold.sections.contains_key("intro"));
        assert!(fold.sections.contains_key("methods"));
        assert!(fold.sections.contains_key("details"));
    }

    #[test]
    fn context_fold_summaries() {
        let mut fold = ContextFold::new("test", "x".repeat(4000));
        fold.add_summary(0.1, "Brief summary");
        fold.add_summary(0.5, "Detailed summary with more info here");

        // Budget of 5 tokens should get the brief summary
        let best = fold.best_summary(5);
        assert!(best.is_some());
        assert!(best.unwrap().ratio < 0.2);

        // Budget of 1000 should get the detailed summary
        let best = fold.best_summary(1000);
        assert!(best.unwrap().ratio > 0.3);
    }

    #[test]
    fn context_fold_no_summary_for_tiny_budget() {
        let mut fold = ContextFold::new("test", "content");
        fold.add_summary(0.1, "This is a summary that has some tokens");
        assert!(fold.best_summary(1).is_none());
    }

    // -- Decomposition --

    #[test]
    fn decomposition_creation() {
        let dec = Decomposition::new("Build feature", 3);
        assert_eq!(dec.max_depth, 3);
        assert_eq!(dec.depth, 0);
        assert!(dec.subtasks.is_empty());
        assert!(dec.can_recurse());
    }

    #[test]
    fn decomposition_add_subtasks() {
        let mut dec = Decomposition::new("task", 3);
        dec.add_subtask("subtask 1");
        dec.add_subtask("subtask 2");
        assert_eq!(dec.subtasks.len(), 2);
        assert_eq!(dec.pending_subtasks().len(), 2);
    }

    #[test]
    fn decomposition_record_result() {
        let mut dec = Decomposition::new("task", 3);
        let id = dec.add_subtask("sub");
        assert!(dec.record_result(&id, "done"));
        assert_eq!(dec.subtasks.get(&id).unwrap().status, SubTaskStatus::Complete);
        assert!(!dec.record_result(&Uuid::new_v4(), "nope"));
    }

    #[test]
    fn decomposition_completion() {
        let mut dec = Decomposition::new("task", 3);
        let id1 = dec.add_subtask("a");
        let id2 = dec.add_subtask("b");

        assert!(!dec.is_complete());
        dec.record_result(&id1, "done a");
        assert!(!dec.is_complete());
        dec.record_result(&id2, "done b");
        assert!(dec.is_complete());
    }

    #[test]
    fn decomposition_failures() {
        let mut dec = Decomposition::new("task", 3);
        let id = dec.add_subtask("will fail");
        dec.mark_failed(&id);
        assert!(dec.has_failures());
    }

    #[test]
    fn decomposition_synthesize_concat() {
        let mut dec = Decomposition::new("task", 3);
        let id1 = dec.add_subtask("a");
        let id2 = dec.add_subtask("b");
        dec.record_result(&id1, "result A");
        dec.record_result(&id2, "result B");

        let synth = dec.synthesize();
        assert!(synth.contains("result A"));
        assert!(synth.contains("result B"));
    }

    #[test]
    fn decomposition_synthesize_best_of() {
        let mut dec = Decomposition::new("task", 3);
        dec.synthesis = SynthesisStrategy::BestOf;
        let id1 = dec.add_subtask("a");
        let id2 = dec.add_subtask("b");
        dec.record_result(&id1, "first");
        dec.record_result(&id2, "second");

        let synth = dec.synthesize();
        assert_eq!(synth, "second");
    }

    #[test]
    fn decomposition_parallel_batch() {
        let mut dec = Decomposition::new("task", 3);
        dec.add_subtask("par 1");
        let id2 = dec.add_subtask("seq");
        dec.subtasks
            .get_mut(&id2)
            .unwrap()
            .parallelizable = false;

        assert_eq!(dec.parallel_batch().len(), 1);
    }

    #[test]
    fn decomposition_child() {
        let parent = Decomposition::new("parent", 3);
        let child = parent.child("child task");
        assert_eq!(child.depth, 1);
        assert_eq!(child.max_depth, 3);
        assert!(child.can_recurse());

        // Depth 3 cannot recurse further
        let deep = Decomposition {
            depth: 3,
            max_depth: 3,
            ..Decomposition::new("deep", 3)
        };
        assert!(!deep.can_recurse());
    }

    // -- ProgressiveRefinement --

    #[test]
    fn refinement_creation() {
        let pr = ProgressiveRefinement::new("fix bug", 5);
        assert_eq!(pr.max_revisions, 5);
        assert!(!pr.finalized);
        assert!(pr.can_revise());
        assert_eq!(pr.revision_count(), 0);
    }

    #[test]
    fn refinement_revisions() {
        let mut pr = ProgressiveRefinement::new("task", 3);
        assert!(pr.revise("draft 1", None, 0.5));
        assert!(pr.revise("draft 2", Some("fixed typo".into()), 0.8));

        assert_eq!(pr.revision_count(), 2);
        assert_eq!(pr.latest().unwrap().version, 2);
        assert!(pr.latest().unwrap().confidence > 0.7);
    }

    #[test]
    fn refinement_max_revisions() {
        let mut pr = ProgressiveRefinement::new("task", 2);
        assert!(pr.revise("v1", None, 0.5));
        assert!(pr.revise("v2", None, 0.9));
        assert!(!pr.revise("v3", None, 1.0)); // over limit
        assert_eq!(pr.revision_count(), 2);
    }

    #[test]
    fn refinement_finalize() {
        let mut pr = ProgressiveRefinement::new("task", 10);
        pr.revise("draft", None, 0.9);
        pr.finalize();
        assert!(pr.finalized);
        assert!(!pr.can_revise());
        assert!(!pr.revise("more", None, 1.0)); // can't revise after finalize
    }

    #[test]
    fn refinement_confidence() {
        let mut pr = ProgressiveRefinement::new("task", 5);
        assert!(!pr.is_confident(0.8));
        pr.revise("low", None, 0.3);
        assert!(!pr.is_confident(0.8));
        pr.revise("high", None, 0.95);
        assert!(pr.is_confident(0.8));
    }

    // -- StuckDetector --

    #[test]
    fn stuck_detector_creation() {
        let det = StuckDetector::new(60, 10000);
        assert_eq!(det.timeout_secs, 60);
        assert_eq!(det.token_budget, 10000);
        assert!(det.check().is_none());
    }

    #[test]
    fn stuck_detector_output_loop() {
        let mut det = StuckDetector::new(300, 100000);
        det.record_output("same output", 10);
        det.record_output("same output", 10);
        det.record_output("same output", 10);
        assert_eq!(det.check(), Some(StuckReason::OutputLoop));
    }

    #[test]
    fn stuck_detector_no_loop_with_varied_output() {
        let mut det = StuckDetector::new(300, 100000);
        det.record_output("output 1", 10);
        det.record_output("output 2", 10);
        det.record_output("output 3", 10);
        assert!(det.check().is_none());
    }

    #[test]
    fn stuck_detector_budget_exhausted() {
        let mut det = StuckDetector::new(300, 100);
        det.record_output("big output", 50);
        assert!(det.check().is_none());
        det.record_output("more output", 60);
        assert_eq!(det.check(), Some(StuckReason::BudgetExhausted));
    }

    #[test]
    fn stuck_detector_tokens_remaining() {
        let mut det = StuckDetector::new(300, 1000);
        det.record_output("x", 400);
        assert_eq!(det.tokens_remaining(), 600);
    }

    #[test]
    fn stuck_detector_reset() {
        let mut det = StuckDetector::new(300, 100);
        det.record_output("x", 80);
        det.reset();
        assert_eq!(det.tokens_remaining(), 100);
        assert!(det.check().is_none());
    }

    #[test]
    fn stuck_detector_empty_outputs_dont_trigger_loop() {
        let mut det = StuckDetector::new(300, 100000);
        det.record_output("", 0);
        det.record_output("", 0);
        det.record_output("", 0);
        // Empty strings should not trigger loop (they're not meaningful repeats)
        assert!(det.check().is_none());
    }

    // -- Serialization --

    #[test]
    fn context_fold_serialization() {
        let fold = ContextFold::new("test", "content");
        let json = serde_json::to_string(&fold).unwrap();
        let deser: ContextFold = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.label, "test");
    }

    #[test]
    fn decomposition_serialization() {
        let mut dec = Decomposition::new("task", 3);
        dec.add_subtask("sub");
        let json = serde_json::to_string(&dec).unwrap();
        let deser: Decomposition = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.subtasks.len(), 1);
    }

    #[test]
    fn refinement_serialization() {
        let mut pr = ProgressiveRefinement::new("task", 5);
        pr.revise("draft", None, 0.7);
        let json = serde_json::to_string(&pr).unwrap();
        let deser: ProgressiveRefinement = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.revision_count(), 1);
    }

    #[test]
    fn stuck_reason_serialization() {
        let reason = StuckReason::OutputLoop;
        let json = serde_json::to_string(&reason).unwrap();
        assert_eq!(json, "\"output_loop\"");
    }

    #[test]
    fn synthesis_strategy_variants() {
        let strategies = [
            SynthesisStrategy::Concatenate,
            SynthesisStrategy::LlmMerge,
            SynthesisStrategy::BestOf,
            SynthesisStrategy::Vote,
            SynthesisStrategy::Refine,
        ];
        for s in strategies {
            let json = serde_json::to_string(&s).unwrap();
            let deser: SynthesisStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, s);
        }
    }
}
