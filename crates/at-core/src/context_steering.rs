//! Context steering — assembles the right context for each agent at each phase.
//!
//! Implements the 4-level progressive disclosure model:
//! - **L0 (Identity)**: Agent name, role, core directive (~200 tokens)
//! - **L1 (Project)**: CLAUDE.md, AGENTS.md, conventions (~2K tokens)
//! - **L2 (Task)**: Current task spec, related decisions, dependencies (~4K tokens)
//! - **L3 (Deep)**: Referenced files, code context, skill bodies (~8K+ tokens)
//!
//! Also implements context steering techniques:
//! - **Relevance scoring**: Score context nodes against the current task
//! - **Token budgeting**: Stay within model context limits
//! - **Phase-aware loading**: Different phases need different context
//! - **Memory injection**: Episodic, semantic, procedural memories
//! - **Convention enforcement**: Project rules always loaded first

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::context_engine::{
    AgentDefinition, ContextNodeKind, ProjectContextLoader, SkillDefinition,
};

// ---------------------------------------------------------------------------
// Disclosure Level
// ---------------------------------------------------------------------------

/// Progressive disclosure level controls how much context is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisclosureLevel {
    /// L0: Identity only — agent name, role, core directive (~200 tokens)
    Identity = 0,
    /// L1: Project context — CLAUDE.md, AGENTS.md, conventions (~2K tokens)
    Project = 1,
    /// L2: Task context — current spec, decisions, dependencies (~4K tokens)
    Task = 2,
    /// L3: Deep context — referenced files, code, skill bodies (~8K+ tokens)
    Deep = 3,
}

impl DisclosureLevel {
    /// Suggested token budget for each level.
    pub fn token_budget(&self) -> usize {
        match self {
            DisclosureLevel::Identity => 200,
            DisclosureLevel::Project => 2_000,
            DisclosureLevel::Task => 4_000,
            DisclosureLevel::Deep => 16_000,
        }
    }
}

// ---------------------------------------------------------------------------
// Phase Context Profile — what context each workflow phase needs
// ---------------------------------------------------------------------------

/// Defines what context a workflow phase should receive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseContextProfile {
    /// Minimum disclosure level for this phase.
    pub min_level: DisclosureLevel,
    /// Maximum disclosure level (if budget allows).
    pub max_level: DisclosureLevel,
    /// Which context node kinds are relevant to this phase.
    pub relevant_kinds: Vec<ContextNodeKind>,
    /// Extra keywords to boost relevance scoring for this phase.
    pub boost_keywords: Vec<String>,
    /// Whether to include memory entries.
    pub include_memories: bool,
    /// Whether to include the full task spec.
    pub include_task_spec: bool,
}

impl PhaseContextProfile {
    /// Profile for the discovery/research phase.
    pub fn discovery() -> Self {
        Self {
            min_level: DisclosureLevel::Project,
            max_level: DisclosureLevel::Task,
            relevant_kinds: vec![
                ContextNodeKind::ProjectConfig,
                ContextNodeKind::Task,
                ContextNodeKind::Convention,
            ],
            boost_keywords: vec!["architecture".into(), "structure".into(), "overview".into()],
            include_memories: true,
            include_task_spec: true,
        }
    }

    /// Profile for spec creation phase.
    pub fn spec_creation() -> Self {
        Self {
            min_level: DisclosureLevel::Task,
            max_level: DisclosureLevel::Deep,
            relevant_kinds: vec![
                ContextNodeKind::ProjectConfig,
                ContextNodeKind::Task,
                ContextNodeKind::Decision,
                ContextNodeKind::Convention,
            ],
            boost_keywords: vec![
                "requirements".into(),
                "acceptance".into(),
                "criteria".into(),
                "spec".into(),
            ],
            include_memories: true,
            include_task_spec: true,
        }
    }

    /// Profile for planning phase.
    pub fn planning() -> Self {
        Self {
            min_level: DisclosureLevel::Task,
            max_level: DisclosureLevel::Deep,
            relevant_kinds: vec![
                ContextNodeKind::ProjectConfig,
                ContextNodeKind::Task,
                ContextNodeKind::CodeArtifact,
                ContextNodeKind::Decision,
            ],
            boost_keywords: vec![
                "plan".into(),
                "implementation".into(),
                "steps".into(),
                "files".into(),
            ],
            include_memories: true,
            include_task_spec: true,
        }
    }

    /// Profile for coding phase — needs deep context.
    pub fn coding() -> Self {
        Self {
            min_level: DisclosureLevel::Deep,
            max_level: DisclosureLevel::Deep,
            relevant_kinds: vec![
                ContextNodeKind::ProjectConfig,
                ContextNodeKind::Task,
                ContextNodeKind::CodeArtifact,
                ContextNodeKind::Skill,
                ContextNodeKind::Convention,
            ],
            boost_keywords: vec![
                "implement".into(),
                "code".into(),
                "function".into(),
                "module".into(),
            ],
            include_memories: false,
            include_task_spec: true,
        }
    }

    /// Profile for QA/review phase.
    pub fn qa() -> Self {
        Self {
            min_level: DisclosureLevel::Task,
            max_level: DisclosureLevel::Deep,
            relevant_kinds: vec![
                ContextNodeKind::Task,
                ContextNodeKind::CodeArtifact,
                ContextNodeKind::Convention,
            ],
            boost_keywords: vec![
                "test".into(),
                "verify".into(),
                "review".into(),
                "quality".into(),
            ],
            include_memories: false,
            include_task_spec: true,
        }
    }

    /// Profile for merging phase — minimal context.
    pub fn merging() -> Self {
        Self {
            min_level: DisclosureLevel::Project,
            max_level: DisclosureLevel::Task,
            relevant_kinds: vec![ContextNodeKind::ProjectConfig, ContextNodeKind::Convention],
            boost_keywords: vec!["merge".into(), "commit".into(), "branch".into()],
            include_memories: false,
            include_task_spec: false,
        }
    }

    /// Get profile for a named phase.
    pub fn for_phase(phase_name: &str) -> Self {
        match phase_name {
            "discovery" | "context_gathering" => Self::discovery(),
            "spec_creation" => Self::spec_creation(),
            "planning" => Self::planning(),
            "coding" => Self::coding(),
            "qa" => Self::qa(),
            "merging" => Self::merging(),
            _ => Self::coding(), // default to deep context
        }
    }
}

// ---------------------------------------------------------------------------
// ContextBlock — assembled context ready for injection
// ---------------------------------------------------------------------------

/// A block of assembled context ready for LLM injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBlock {
    /// Label for this block (e.g., "CLAUDE.md", "task_spec").
    pub label: String,
    /// The text content.
    pub content: String,
    /// Which disclosure level this block belongs to.
    pub level: DisclosureLevel,
    /// Estimated token count.
    pub estimated_tokens: usize,
    /// Relevance score (0.0–1.0) for this task.
    pub relevance: f64,
}

impl ContextBlock {
    pub fn new(
        label: impl Into<String>,
        content: impl Into<String>,
        level: DisclosureLevel,
    ) -> Self {
        let content = content.into();
        let estimated_tokens = content.len() / 4;
        Self {
            label: label.into(),
            content,
            level,
            estimated_tokens,
            relevance: 1.0,
        }
    }

    pub fn with_relevance(mut self, relevance: f64) -> Self {
        self.relevance = relevance;
        self
    }
}

// ---------------------------------------------------------------------------
// AssembledContext — the complete context package for an agent turn
// ---------------------------------------------------------------------------

/// The fully assembled context for a single agent invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledContext {
    /// Ordered context blocks (highest priority first).
    pub blocks: Vec<ContextBlock>,
    /// Total estimated tokens across all blocks.
    pub total_tokens: usize,
    /// The disclosure level achieved.
    pub level_reached: DisclosureLevel,
    /// Metadata about the assembly.
    pub metadata: ContextMetadata,
}

/// Metadata about how context was assembled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMetadata {
    pub phase: String,
    pub agent_name: String,
    pub token_budget: usize,
    pub blocks_included: usize,
    pub blocks_dropped: usize,
}

impl AssembledContext {
    /// Render the context into a single string for injection into a system prompt.
    pub fn render(&self) -> String {
        let mut parts = Vec::with_capacity(self.blocks.len());
        for block in &self.blocks {
            parts.push(format!(
                "<context source=\"{}\">\n{}\n</context>",
                block.label, block.content
            ));
        }
        parts.join("\n\n")
    }

    /// Render as XML-tagged sections (Claude's preferred format).
    pub fn render_xml(&self) -> String {
        let mut parts = Vec::with_capacity(self.blocks.len() + 2);
        parts.push("<project-context>".to_string());
        for block in &self.blocks {
            parts.push(format!(
                "<{tag} relevance=\"{rel:.2}\">\n{content}\n</{tag}>",
                tag = sanitize_xml_tag(&block.label),
                rel = block.relevance,
                content = block.content,
            ));
        }
        parts.push("</project-context>".to_string());
        parts.join("\n")
    }

    /// Check if context is within budget.
    pub fn is_within_budget(&self, budget: usize) -> bool {
        self.total_tokens <= budget
    }
}

/// Sanitize a label into a valid XML tag name.
fn sanitize_xml_tag(label: &str) -> String {
    label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

// ---------------------------------------------------------------------------
// ContextSteerer — the main context assembly engine
// ---------------------------------------------------------------------------

/// Assembles context for agent invocations using progressive disclosure.
///
/// The steerer loads project context, scores it for relevance, and assembles
/// the right context blocks within a token budget for each agent+phase combo.
pub struct ContextSteerer {
    /// Project root for filesystem loading.
    project_root: PathBuf,
    /// Cached project-level context blocks (L1).
    project_context: Vec<ContextBlock>,
    /// Cached agent definitions.
    agent_definitions: Vec<AgentDefinition>,
    /// Cached skill definitions.
    skill_definitions: Vec<SkillDefinition>,
    /// Convention rules extracted from CLAUDE.md.
    conventions: Vec<String>,
    /// Memory entries (episodic, semantic, procedural).
    memories: Vec<MemoryEntry>,
    /// Whether project context has been loaded.
    loaded: bool,
}

/// A memory entry for injection into agent context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub kind: MemoryKind,
    pub content: String,
    /// Relevance score (0.0–1.0).
    pub relevance: f64,
    /// Keywords for matching.
    pub keywords: Vec<String>,
    /// Cortex-mem L0/L1/L2 weighting for cross-session memory prioritization.
    #[serde(default)]
    pub weight: MemoryWeight,
}

/// Cortex-mem inspired L0/L1/L2 memory weighting.
///
/// Controls how memories are prioritized during context assembly:
/// - **L0 (Core)**: Always included — project identity, critical patterns (weight ≥ 0.9)
/// - **L1 (Active)**: Recently accessed or frequently used (weight 0.5–0.9)
/// - **L2 (Archive)**: Old or rarely accessed — only included when budget allows (weight < 0.5)
///
/// Weight decays over time based on `last_accessed` and increases with `access_count`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryWeight {
    /// Memory tier: L0 (always), L1 (active), L2 (archive).
    pub tier: MemoryTier,
    /// Number of times this memory was accessed across sessions.
    pub access_count: u32,
    /// Timestamp of last access (ISO 8601).
    #[serde(default = "default_timestamp")]
    pub last_accessed: String,
    /// Timestamp of creation (ISO 8601).
    #[serde(default = "default_timestamp")]
    pub created_at: String,
    /// Computed weight (0.0–1.0). Recalculated on access.
    pub computed_weight: f64,
}

fn default_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

impl Default for MemoryWeight {
    fn default() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            tier: MemoryTier::L1,
            access_count: 0,
            last_accessed: now.clone(),
            created_at: now,
            computed_weight: 0.7,
        }
    }
}

impl MemoryWeight {
    /// Create a new L0 (core) memory weight — always included.
    pub fn core() -> Self {
        Self {
            tier: MemoryTier::L0,
            computed_weight: 1.0,
            ..Self::default()
        }
    }

    /// Create a new L1 (active) memory weight.
    pub fn active() -> Self {
        Self {
            tier: MemoryTier::L1,
            computed_weight: 0.7,
            ..Self::default()
        }
    }

    /// Create a new L2 (archive) memory weight.
    pub fn archive() -> Self {
        Self {
            tier: MemoryTier::L2,
            computed_weight: 0.3,
            ..Self::default()
        }
    }

    /// Record an access and recalculate weight.
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = chrono::Utc::now().to_rfc3339();
        self.recompute();
    }

    /// Recompute the weight based on tier, recency, and frequency.
    ///
    /// Formula:
    ///   weight = tier_base + frequency_bonus + recency_bonus
    ///
    /// - tier_base: L0=0.9, L1=0.5, L2=0.2
    /// - frequency_bonus: min(access_count * 0.02, 0.1)  — caps at +0.1
    /// - recency_bonus: decays from 0.1 to 0.0 over 7 days
    pub fn recompute(&mut self) {
        let tier_base = match self.tier {
            MemoryTier::L0 => 0.9,
            MemoryTier::L1 => 0.5,
            MemoryTier::L2 => 0.2,
        };

        let frequency_bonus = (self.access_count as f64 * 0.02).min(0.1);

        let recency_bonus =
            if let Ok(last) = chrono::DateTime::parse_from_rfc3339(&self.last_accessed) {
                let age_hours = (chrono::Utc::now() - last.with_timezone(&chrono::Utc))
                    .num_hours()
                    .max(0) as f64;
                let decay_hours = 7.0 * 24.0; // 7-day half-life
                (0.1 * (-age_hours / decay_hours).exp()).max(0.0)
            } else {
                0.0
            };

        self.computed_weight = (tier_base + frequency_bonus + recency_bonus).min(1.0);

        // Auto-promote/demote based on computed weight
        if self.tier != MemoryTier::L0 {
            if self.computed_weight >= 0.8 {
                self.tier = MemoryTier::L1; // promote to active
            } else if self.computed_weight < 0.3 {
                self.tier = MemoryTier::L2; // demote to archive
            }
        }
    }

    /// Check if this memory should be included given a remaining token budget.
    /// L0 always included, L1 when budget > 50%, L2 only when budget > 80%.
    pub fn should_include(&self, budget_remaining_pct: f64) -> bool {
        match self.tier {
            MemoryTier::L0 => true,
            MemoryTier::L1 => budget_remaining_pct > 0.3,
            MemoryTier::L2 => budget_remaining_pct > 0.7,
        }
    }
}

/// Memory tier classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTier {
    /// Always included — project identity, critical patterns.
    L0,
    /// Active — recently accessed, frequently used.
    L1,
    /// Archive — old or rarely accessed.
    L2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// What happened (events, outcomes, errors).
    Episodic,
    /// What is known (facts, patterns, structures).
    Semantic,
    /// How to do things (procedures, workflows).
    Procedural,
}

impl ContextSteerer {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            project_context: Vec::new(),
            agent_definitions: Vec::new(),
            skill_definitions: Vec::new(),
            conventions: Vec::new(),
            memories: Vec::new(),
            loaded: false,
        }
    }

    /// Load project context from the filesystem.
    pub fn load_project(&mut self) {
        let loader = ProjectContextLoader::new(&self.project_root);
        let snapshot = loader.load_snapshot_cached();

        // Avoid duplicate blocks if load_project is called multiple times.
        self.project_context.clear();
        self.agent_definitions.clear();
        self.skill_definitions.clear();
        self.conventions.clear();
        self.memories.clear();

        // L1: CLAUDE.md
        if let Some(content) = snapshot.claude_md {
            self.conventions = extract_conventions(&content);
            self.project_context.push(ContextBlock::new(
                "CLAUDE.md",
                content,
                DisclosureLevel::Project,
            ));
        }

        // L1: AGENTS.md
        if let Some(content) = snapshot.agents_md {
            self.project_context.push(ContextBlock::new(
                "AGENTS.md",
                content,
                DisclosureLevel::Project,
            ));
        }

        // L1: todo.md
        if let Some(content) = snapshot.todo_md {
            self.project_context
                .push(ContextBlock::new("todo.md", content, DisclosureLevel::Task));
        }

        // Load agent and skill definitions
        self.agent_definitions = snapshot.agent_definitions;
        self.skill_definitions = snapshot.skill_definitions;

        // Load MEMORY.md if present — L0 core memory (always included)
        let memory_path = self.project_root.join(".claude").join("MEMORY.md");
        if let Ok(content) = std::fs::read_to_string(&memory_path) {
            self.memories.push(MemoryEntry {
                kind: MemoryKind::Semantic,
                content,
                relevance: 0.8,
                keywords: vec!["memory".into(), "pattern".into()],
                weight: MemoryWeight::core(),
            });
        }

        // Load project-level memories from .claude/memory/ — L1 active by default
        let memory_dir = self.project_root.join(".claude").join("memory");
        if let Ok(entries) = std::fs::read_dir(&memory_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let name = path.file_stem().unwrap_or_default().to_string_lossy();
                        self.memories.push(MemoryEntry {
                            kind: MemoryKind::Semantic,
                            content,
                            relevance: 0.6,
                            keywords: vec![name.to_string()],
                            weight: MemoryWeight::active(),
                        });
                    }
                }
            }
        }

        self.loaded = true;
    }

    /// Add a memory entry.
    pub fn add_memory(&mut self, entry: MemoryEntry) {
        self.memories.push(entry);
    }

    /// Assemble context for a specific agent and phase.
    pub fn assemble(
        &self,
        agent_name: &str,
        phase_name: &str,
        task_spec: Option<&str>,
        token_budget: usize,
    ) -> AssembledContext {
        let profile = PhaseContextProfile::for_phase(phase_name);
        let mut blocks = Vec::new();
        let mut total_tokens = 0;
        let mut blocks_dropped = 0;

        // L0: Agent identity (always included)
        if let Some(agent) = self.agent_definitions.iter().find(|a| a.name == agent_name) {
            let identity = format!(
                "You are the **{}** agent.\n{}\n\nModel: {}",
                agent.name,
                agent.description,
                agent.model.as_deref().unwrap_or("default"),
            );
            let block = ContextBlock::new("agent_identity", identity, DisclosureLevel::Identity);
            total_tokens += block.estimated_tokens;
            blocks.push(block);
        }

        // L1: Project context (CLAUDE.md, AGENTS.md) — filtered by profile
        if profile.min_level <= DisclosureLevel::Project {
            for ctx in &self.project_context {
                if ctx.level <= profile.max_level {
                    if total_tokens + ctx.estimated_tokens <= token_budget {
                        total_tokens += ctx.estimated_tokens;
                        blocks.push(ctx.clone());
                    } else {
                        blocks_dropped += 1;
                    }
                }
            }
        }

        // L1: Conventions (extracted from CLAUDE.md)
        if !self.conventions.is_empty() && profile.min_level <= DisclosureLevel::Project {
            let conv_text = self.conventions.join("\n- ");
            let conv_block = ContextBlock::new(
                "conventions",
                format!("## Project Conventions\n- {}", conv_text),
                DisclosureLevel::Project,
            );
            if total_tokens + conv_block.estimated_tokens <= token_budget {
                total_tokens += conv_block.estimated_tokens;
                blocks.push(conv_block);
            }
        }

        // L2: Task spec
        if profile.include_task_spec {
            if let Some(spec) = task_spec {
                let block = ContextBlock::new("task_spec", spec, DisclosureLevel::Task);
                if total_tokens + block.estimated_tokens <= token_budget {
                    total_tokens += block.estimated_tokens;
                    blocks.push(block);
                } else {
                    blocks_dropped += 1;
                }
            }
        }

        // L2: Memories — filtered by cortex-mem L0/L1/L2 tier weighting
        if profile.include_memories {
            let budget_remaining_pct = if token_budget > 0 {
                1.0 - (total_tokens as f64 / token_budget as f64)
            } else {
                0.0
            };

            let mut relevant_memories: Vec<&MemoryEntry> = self
                .memories
                .iter()
                .filter(|m| {
                    // First check tier-based budget gating
                    if !m.weight.should_include(budget_remaining_pct) {
                        return false;
                    }
                    // Then check keyword relevance or high relevance/weight
                    m.keywords.iter().any(|k| {
                        profile
                            .boost_keywords
                            .iter()
                            .any(|bk| k.contains(bk.as_str()) || bk.contains(k.as_str()))
                    }) || m.relevance > 0.7
                        || m.weight.computed_weight > 0.8
                })
                .collect();

            // Sort by combined weight: computed_weight (tier+recency+frequency) + relevance
            relevant_memories.sort_by(|a, b| {
                let score_a = a.weight.computed_weight * 0.6 + a.relevance * 0.4;
                let score_b = b.weight.computed_weight * 0.6 + b.relevance * 0.4;
                score_b.partial_cmp(&score_a).unwrap()
            });

            for mem in relevant_memories {
                let block = ContextBlock::new(
                    format!("memory_{:?}", mem.kind).to_lowercase(),
                    &mem.content,
                    DisclosureLevel::Task,
                )
                .with_relevance(mem.relevance);
                if total_tokens + block.estimated_tokens <= token_budget {
                    total_tokens += block.estimated_tokens;
                    blocks.push(block);
                } else {
                    blocks_dropped += 1;
                }
            }
        }

        // L3: Skill bodies (if phase needs deep context)
        if profile.max_level >= DisclosureLevel::Deep
            && profile.relevant_kinds.contains(&ContextNodeKind::Skill)
        {
            for skill in &self.skill_definitions {
                // Score skill relevance to the current task
                let relevance = score_relevance(
                    &skill.name,
                    &skill.description,
                    &profile.boost_keywords,
                    task_spec.unwrap_or(""),
                );
                if relevance > 0.3 {
                    let block = ContextBlock::new(
                        format!("skill:{}", skill.name),
                        &skill.body,
                        DisclosureLevel::Deep,
                    )
                    .with_relevance(relevance);
                    if total_tokens + block.estimated_tokens <= token_budget {
                        total_tokens += block.estimated_tokens;
                        blocks.push(block);
                    } else {
                        blocks_dropped += 1;
                    }
                }
            }
        }

        // Determine the highest disclosure level reached
        let level_reached = blocks
            .iter()
            .map(|b| b.level)
            .max()
            .unwrap_or(DisclosureLevel::Identity);

        AssembledContext {
            blocks,
            total_tokens,
            level_reached,
            metadata: ContextMetadata {
                phase: phase_name.to_string(),
                agent_name: agent_name.to_string(),
                token_budget,
                blocks_included: 0, // set below
                blocks_dropped,
            },
        }
        .finalize()
    }

    /// Get loaded agent definitions.
    pub fn agent_definitions(&self) -> &[AgentDefinition] {
        &self.agent_definitions
    }

    /// Get loaded skill definitions.
    pub fn skill_definitions(&self) -> &[SkillDefinition] {
        &self.skill_definitions
    }

    /// Check if project context has been loaded.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Number of memory entries.
    pub fn memory_count(&self) -> usize {
        self.memories.len()
    }
}

impl AssembledContext {
    fn finalize(mut self) -> Self {
        self.metadata.blocks_included = self.blocks.len();
        self
    }
}

// ---------------------------------------------------------------------------
// Relevance Scoring
// ---------------------------------------------------------------------------

/// Score relevance of a named item against boost keywords and task spec.
///
/// Returns a value from 0.0 to 1.0.
fn score_relevance(
    name: &str,
    description: &str,
    boost_keywords: &[String],
    task_spec: &str,
) -> f64 {
    let name_lower = name.to_lowercase();
    let desc_lower = description.to_lowercase();
    let task_lower = task_spec.to_lowercase();

    let mut score = 0.0;
    let mut matches = 0;

    // Check boost keywords against name + description
    for keyword in boost_keywords {
        let kw = keyword.to_lowercase();
        if name_lower.contains(&kw) {
            score += 0.3;
            matches += 1;
        }
        if desc_lower.contains(&kw) {
            score += 0.2;
            matches += 1;
        }
    }

    // Check task spec mentions the name
    if task_lower.contains(&name_lower) {
        score += 0.4;
        matches += 1;
    }

    // Normalize
    if matches > 0 {
        (score / matches as f64).min(1.0).max(0.0) + 0.1
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Convention Extraction
// ---------------------------------------------------------------------------

/// Extract convention rules from a CLAUDE.md file.
///
/// Looks for lines that start with "- " under any heading containing
/// "convention", "rule", "standard", or "guideline".
fn extract_conventions(content: &str) -> Vec<String> {
    let mut conventions = Vec::new();
    let mut in_conventions_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Detect section headers
        if trimmed.starts_with('#') {
            let heading = trimmed.to_lowercase();
            in_conventions_section = heading.contains("convention")
                || heading.contains("rule")
                || heading.contains("standard")
                || heading.contains("guideline")
                || heading.contains("requirement");
            continue;
        }

        // Collect bullet points in convention sections
        if in_conventions_section && trimmed.starts_with("- ") {
            conventions.push(trimmed[2..].to_string());
        }

        // End section on empty line after content
        if in_conventions_section && trimmed.is_empty() && !conventions.is_empty() {
            // Keep going — could be multiline
        }
    }

    conventions
}

// ---------------------------------------------------------------------------
// ContextSteeringConfig — serializable configuration
// ---------------------------------------------------------------------------

/// Configuration for context steering behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSteeringConfig {
    /// Default token budget per agent invocation.
    pub default_token_budget: usize,
    /// Per-phase token budget overrides.
    pub phase_budgets: HashMap<String, usize>,
    /// Whether to include memory entries by default.
    pub include_memories: bool,
    /// Maximum number of skill bodies to include.
    pub max_skills: usize,
    /// Minimum relevance score for skill inclusion.
    pub min_skill_relevance: f64,
}

impl Default for ContextSteeringConfig {
    fn default() -> Self {
        Self {
            default_token_budget: 16_000,
            phase_budgets: HashMap::new(),
            include_memories: true,
            max_skills: 5,
            min_skill_relevance: 0.3,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- DisclosureLevel --

    #[test]
    fn disclosure_level_ordering() {
        assert!(DisclosureLevel::Identity < DisclosureLevel::Project);
        assert!(DisclosureLevel::Project < DisclosureLevel::Task);
        assert!(DisclosureLevel::Task < DisclosureLevel::Deep);
    }

    #[test]
    fn disclosure_level_budgets() {
        assert_eq!(DisclosureLevel::Identity.token_budget(), 200);
        assert_eq!(DisclosureLevel::Project.token_budget(), 2_000);
        assert_eq!(DisclosureLevel::Task.token_budget(), 4_000);
        assert_eq!(DisclosureLevel::Deep.token_budget(), 16_000);
    }

    // -- PhaseContextProfile --

    #[test]
    fn phase_profile_discovery() {
        let p = PhaseContextProfile::discovery();
        assert_eq!(p.min_level, DisclosureLevel::Project);
        assert!(p.include_memories);
        assert!(p.include_task_spec);
    }

    #[test]
    fn phase_profile_coding() {
        let p = PhaseContextProfile::coding();
        assert_eq!(p.min_level, DisclosureLevel::Deep);
        assert!(!p.include_memories);
        assert!(p.include_task_spec);
    }

    #[test]
    fn phase_profile_merging() {
        let p = PhaseContextProfile::merging();
        assert!(!p.include_task_spec);
        assert!(!p.include_memories);
    }

    #[test]
    fn phase_profile_for_unknown_defaults_to_coding() {
        let p = PhaseContextProfile::for_phase("unknown_phase");
        assert_eq!(p.min_level, DisclosureLevel::Deep);
    }

    // -- ContextBlock --

    #[test]
    fn context_block_creation() {
        let block = ContextBlock::new("test", "Hello world content", DisclosureLevel::Project);
        assert_eq!(block.label, "test");
        assert_eq!(block.level, DisclosureLevel::Project);
        assert!(block.estimated_tokens > 0);
        assert_eq!(block.relevance, 1.0);
    }

    #[test]
    fn context_block_with_relevance() {
        let block = ContextBlock::new("x", "content", DisclosureLevel::Task).with_relevance(0.75);
        assert!((block.relevance - 0.75).abs() < f64::EPSILON);
    }

    // -- AssembledContext --

    #[test]
    fn assembled_context_render() {
        let ctx = AssembledContext {
            blocks: vec![
                ContextBlock::new("CLAUDE.md", "Be helpful", DisclosureLevel::Project),
                ContextBlock::new("task", "Fix the bug", DisclosureLevel::Task),
            ],
            total_tokens: 100,
            level_reached: DisclosureLevel::Task,
            metadata: ContextMetadata {
                phase: "coding".into(),
                agent_name: "coder".into(),
                token_budget: 8000,
                blocks_included: 2,
                blocks_dropped: 0,
            },
        };

        let rendered = ctx.render();
        assert!(rendered.contains("CLAUDE.md"));
        assert!(rendered.contains("Be helpful"));
        assert!(rendered.contains("Fix the bug"));
    }

    #[test]
    fn assembled_context_render_xml() {
        let ctx = AssembledContext {
            blocks: vec![ContextBlock::new(
                "CLAUDE.md",
                "rules",
                DisclosureLevel::Project,
            )],
            total_tokens: 10,
            level_reached: DisclosureLevel::Project,
            metadata: ContextMetadata {
                phase: "discovery".into(),
                agent_name: "researcher".into(),
                token_budget: 4000,
                blocks_included: 1,
                blocks_dropped: 0,
            },
        };

        let xml = ctx.render_xml();
        assert!(xml.contains("<project-context>"));
        assert!(xml.contains("</project-context>"));
        assert!(xml.contains("<claude-md"));
        assert!(xml.contains("relevance="));
    }

    #[test]
    fn assembled_context_within_budget() {
        let ctx = AssembledContext {
            blocks: vec![],
            total_tokens: 100,
            level_reached: DisclosureLevel::Identity,
            metadata: ContextMetadata {
                phase: "test".into(),
                agent_name: "test".into(),
                token_budget: 200,
                blocks_included: 0,
                blocks_dropped: 0,
            },
        };
        assert!(ctx.is_within_budget(200));
        assert!(!ctx.is_within_budget(50));
    }

    // -- Relevance Scoring --

    #[test]
    fn score_relevance_with_keyword_match() {
        let score = score_relevance(
            "rust-patterns",
            "coding patterns for Rust",
            &["code".into(), "patterns".into()],
            "implement the code changes",
        );
        assert!(score > 0.0);
    }

    #[test]
    fn score_relevance_no_match() {
        let score = score_relevance(
            "deploy",
            "deployment automation",
            &["code".into(), "test".into()],
            "write unit tests",
        );
        assert!(score < 0.3);
    }

    #[test]
    fn score_relevance_task_mentions_name() {
        let score = score_relevance(
            "deploy",
            "deploy stuff",
            &[],
            "we need to deploy the application",
        );
        assert!(score > 0.3);
    }

    // -- Convention Extraction --

    #[test]
    fn extract_conventions_from_claude_md() {
        let content = "# Project\nSome intro\n\n## Conventions\n- Use snake_case for variables\n- Always handle errors\n- No unwrap in production code\n\n## Other\nStuff";
        let conv = extract_conventions(content);
        assert_eq!(conv.len(), 3);
        assert!(conv[0].contains("snake_case"));
    }

    #[test]
    fn extract_conventions_no_section() {
        let content = "# Project\nJust a readme.";
        let conv = extract_conventions(content);
        assert!(conv.is_empty());
    }

    #[test]
    fn extract_conventions_multiple_sections() {
        let content = "## Rules\n- Rule 1\n- Rule 2\n\n## Guidelines\n- Guideline 1\n";
        let conv = extract_conventions(content);
        assert_eq!(conv.len(), 3);
    }

    // -- ContextSteerer --

    #[test]
    fn steerer_new_is_not_loaded() {
        let steerer = ContextSteerer::new("/nonexistent");
        assert!(!steerer.is_loaded());
        assert_eq!(steerer.memory_count(), 0);
    }

    #[test]
    fn steerer_load_nonexistent_project() {
        let mut steerer = ContextSteerer::new("/nonexistent/path");
        steerer.load_project();
        assert!(steerer.is_loaded());
        assert!(steerer.agent_definitions().is_empty());
        assert!(steerer.skill_definitions().is_empty());
    }

    #[test]
    fn steerer_load_project_with_claude_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("CLAUDE.md"),
            "# Project\n\n## Conventions\n- Use Rust 2021 edition\n- Format with rustfmt\n",
        )
        .unwrap();

        let mut steerer = ContextSteerer::new(dir.path());
        steerer.load_project();
        assert!(steerer.is_loaded());
        assert!(!steerer.project_context.is_empty());
    }

    #[test]
    fn steerer_assemble_basic() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "Be helpful.").unwrap();

        let mut steerer = ContextSteerer::new(dir.path());
        steerer.load_project();

        let ctx = steerer.assemble("coder", "coding", Some("Fix the login bug"), 8000);
        assert!(ctx.total_tokens > 0);
        assert!(ctx.metadata.blocks_included > 0);
    }

    #[test]
    fn steerer_assemble_with_agent() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("coder.md"),
            "---\nname: coder\ndescription: Code implementation agent\nmodel: claude-sonnet-4-20250514\n---\nWrite clean code.",
        )
        .unwrap();

        let mut steerer = ContextSteerer::new(dir.path());
        steerer.load_project();

        let ctx = steerer.assemble("coder", "coding", Some("Build the feature"), 8000);

        // Should include agent identity block
        let identity = ctx.blocks.iter().find(|b| b.label == "agent_identity");
        assert!(identity.is_some());
        assert!(identity.unwrap().content.contains("coder"));
    }

    #[test]
    fn steerer_assemble_respects_budget() {
        let dir = tempfile::tempdir().unwrap();
        // Create a large CLAUDE.md that exceeds a tiny budget
        let large_content = "x".repeat(4000); // ~1000 tokens
        std::fs::write(dir.path().join("CLAUDE.md"), &large_content).unwrap();

        let mut steerer = ContextSteerer::new(dir.path());
        steerer.load_project();

        // Budget of 100 tokens — should drop the large block
        let ctx = steerer.assemble("test", "coding", Some("task"), 100);
        assert!(ctx.total_tokens <= 100);
    }

    #[test]
    fn steerer_add_memory() {
        let mut steerer = ContextSteerer::new("/tmp");
        steerer.add_memory(MemoryEntry {
            kind: MemoryKind::Episodic,
            content: "Last time we had a deadlock in the pool".into(),
            relevance: 0.9,
            keywords: vec!["deadlock".into(), "pool".into()],
            weight: MemoryWeight::active(),
        });
        assert_eq!(steerer.memory_count(), 1);
    }

    #[test]
    fn steerer_assemble_includes_memories() {
        let dir = tempfile::tempdir().unwrap();
        let mut steerer = ContextSteerer::new(dir.path());
        steerer.load_project();

        steerer.add_memory(MemoryEntry {
            kind: MemoryKind::Semantic,
            content: "The project uses a monorepo with workspace crates".into(),
            relevance: 0.9,
            keywords: vec!["architecture".into(), "structure".into()],
            weight: MemoryWeight::core(),
        });

        // Discovery phase includes memories
        let ctx = steerer.assemble(
            "researcher",
            "discovery",
            Some("understand the codebase"),
            8000,
        );
        let has_memory = ctx.blocks.iter().any(|b| b.label.contains("memory"));
        assert!(has_memory);
    }

    #[test]
    fn steerer_assemble_merging_excludes_memories() {
        let dir = tempfile::tempdir().unwrap();
        let mut steerer = ContextSteerer::new(dir.path());
        steerer.load_project();

        steerer.add_memory(MemoryEntry {
            kind: MemoryKind::Episodic,
            content: "Memory content".into(),
            relevance: 0.9,
            keywords: vec!["test".into()],
            weight: MemoryWeight::active(),
        });

        // Merging phase excludes memories
        let ctx = steerer.assemble("merger", "merging", None, 8000);
        let has_memory = ctx.blocks.iter().any(|b| b.label.contains("memory"));
        assert!(!has_memory);
    }

    #[test]
    fn steerer_load_with_skills() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".claude").join("skills").join("deploy");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: deploy\ndescription: Deploy to production\n---\nRun deploy script.",
        )
        .unwrap();

        let mut steerer = ContextSteerer::new(dir.path());
        steerer.load_project();
        assert_eq!(steerer.skill_definitions().len(), 1);
    }

    #[test]
    fn steerer_load_memory_files() {
        let dir = tempfile::tempdir().unwrap();
        let memory_dir = dir.path().join(".claude").join("memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("patterns.md"), "Use iterators over loops").unwrap();

        let mut steerer = ContextSteerer::new(dir.path());
        steerer.load_project();
        assert!(steerer.memory_count() >= 1);
    }

    // -- Sanitize XML Tag --

    #[test]
    fn sanitize_xml_tag_basic() {
        assert_eq!(sanitize_xml_tag("CLAUDE.md"), "claude-md");
        assert_eq!(sanitize_xml_tag("agent_identity"), "agent_identity");
        assert_eq!(sanitize_xml_tag("skill:deploy"), "skill-deploy");
    }

    // -- ContextSteeringConfig --

    #[test]
    fn config_defaults() {
        let config = ContextSteeringConfig::default();
        assert_eq!(config.default_token_budget, 16_000);
        assert!(config.include_memories);
        assert_eq!(config.max_skills, 5);
    }

    #[test]
    fn config_serialization() {
        let config = ContextSteeringConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deser: ContextSteeringConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.default_token_budget, config.default_token_budget);
    }

    // -- MemoryEntry --

    #[test]
    fn memory_entry_serialization() {
        let entry = MemoryEntry {
            kind: MemoryKind::Episodic,
            content: "test event".into(),
            relevance: 0.85,
            keywords: vec!["test".into()],
            weight: MemoryWeight::active(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deser: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.kind, MemoryKind::Episodic);
        assert_eq!(deser.weight.tier, MemoryTier::L1);
    }

    // -- MemoryWeight (cortex-mem L0/L1/L2) --

    #[test]
    fn memory_weight_core_always_included() {
        let w = MemoryWeight::core();
        assert_eq!(w.tier, MemoryTier::L0);
        assert!(w.computed_weight >= 0.9);
        assert!(w.should_include(0.0)); // even with no budget left
        assert!(w.should_include(0.1));
        assert!(w.should_include(1.0));
    }

    #[test]
    fn memory_weight_active_needs_budget() {
        let w = MemoryWeight::active();
        assert_eq!(w.tier, MemoryTier::L1);
        assert!(!w.should_include(0.2)); // insufficient budget
        assert!(w.should_include(0.5)); // enough budget
    }

    #[test]
    fn memory_weight_archive_needs_high_budget() {
        let w = MemoryWeight::archive();
        assert_eq!(w.tier, MemoryTier::L2);
        assert!(!w.should_include(0.5)); // not enough
        assert!(w.should_include(0.8)); // enough
    }

    #[test]
    fn memory_weight_record_access_increases_count() {
        let mut w = MemoryWeight::active();
        assert_eq!(w.access_count, 0);
        w.record_access();
        assert_eq!(w.access_count, 1);
        w.record_access();
        assert_eq!(w.access_count, 2);
        // Weight should increase with access frequency
        assert!(w.computed_weight >= 0.5);
    }

    #[test]
    fn memory_weight_recompute_tier_base() {
        let mut w0 = MemoryWeight::core();
        w0.recompute();
        assert!(w0.computed_weight >= 0.9);

        let mut w1 = MemoryWeight::active();
        w1.recompute();
        assert!(w1.computed_weight >= 0.5);

        let mut w2 = MemoryWeight::archive();
        w2.recompute();
        assert!(w2.computed_weight >= 0.2);
        assert!(w2.computed_weight < 0.5);
    }

    #[test]
    fn memory_weight_serialization_roundtrip() {
        let w = MemoryWeight::core();
        let json = serde_json::to_string(&w).unwrap();
        let deser: MemoryWeight = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.tier, MemoryTier::L0);
        assert!(deser.computed_weight >= 0.9);
    }

    #[test]
    fn memory_tier_serialization() {
        let tier = MemoryTier::L2;
        let json = serde_json::to_string(&tier).unwrap();
        assert_eq!(json, "\"l2\"");
        let deser: MemoryTier = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, MemoryTier::L2);
    }

    #[test]
    fn memory_weight_default_is_l1() {
        let w = MemoryWeight::default();
        assert_eq!(w.tier, MemoryTier::L1);
        assert_eq!(w.access_count, 0);
        assert!((w.computed_weight - 0.7).abs() < f64::EPSILON);
    }

    // -- Phase profiles --

    #[test]
    fn all_phase_profiles_have_relevant_kinds() {
        let phases = [
            "discovery",
            "spec_creation",
            "planning",
            "coding",
            "qa",
            "merging",
        ];
        for phase in phases {
            let p = PhaseContextProfile::for_phase(phase);
            assert!(
                !p.relevant_kinds.is_empty(),
                "phase {phase} has no relevant kinds"
            );
            assert!(
                !p.boost_keywords.is_empty(),
                "phase {phase} has no boost keywords"
            );
        }
    }

    // -- DisclosureLevel serialization --

    #[test]
    fn disclosure_level_serialization() {
        let level = DisclosureLevel::Deep;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"deep\"");
        let deser: DisclosureLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, DisclosureLevel::Deep);
    }
}
