//! Context engineering layer for LLM agent orchestration.
//!
//! Implements the 2026 SOTA patterns:
//! - **Progressive disclosure**: Load minimum context, expand on demand
//! - **Context graph**: Track entities, decisions, and relationships
//! - **AGENTS.md / SKILL.md / TODO.md** loading from filesystem
//! - **Memory management**: Episodic, semantic, and procedural memory
//!
//! Based on the agentskills.io specification and Claude Code patterns.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Context Node (for the context graph)
// ---------------------------------------------------------------------------

/// A node in the context graph representing an entity, decision, or artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextNode {
    pub id: Uuid,
    pub kind: ContextNodeKind,
    pub label: String,
    pub content: String,
    /// Relevance score (0.0–1.0) for progressive disclosure.
    pub relevance: f64,
    /// Edges to other nodes.
    pub edges: Vec<ContextEdge>,
    pub created_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

impl ContextNode {
    pub fn new(
        kind: ContextNodeKind,
        label: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            label: label.into(),
            content: content.into(),
            relevance: 1.0,
            edges: Vec::new(),
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Estimated token count (rough: 4 chars ≈ 1 token).
    pub fn estimated_tokens(&self) -> usize {
        self.content.len() / 4
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextNodeKind {
    /// Project-level context (AGENTS.md, CLAUDE.md)
    ProjectConfig,
    /// Agent skill definition (SKILL.md)
    Skill,
    /// Task or work item
    Task,
    /// Decision trace (why something was chosen)
    Decision,
    /// Code artifact (file, function, module)
    CodeArtifact,
    /// Memory entry (episodic, semantic, procedural)
    Memory,
    /// User preference or convention
    Convention,
}

/// A directed edge in the context graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEdge {
    pub target_id: Uuid,
    pub relation: EdgeRelation,
    pub weight: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeRelation {
    DependsOn,
    Implements,
    References,
    CreatedBy,
    Supersedes,
    RelatedTo,
}

// ---------------------------------------------------------------------------
// Context Graph
// ---------------------------------------------------------------------------

/// A queryable graph of context nodes for LLM agent decision-making.
///
/// Acts as a "governed memory layer" connecting entities, events, decisions,
/// and evidence so agents can answer *why*, not just *what*.
pub struct ContextGraph {
    nodes: HashMap<Uuid, ContextNode>,
    /// Index: kind → node IDs
    by_kind: HashMap<ContextNodeKind, Vec<Uuid>>,
    /// Index: label → node ID (for fast lookup)
    by_label: HashMap<String, Uuid>,
}

impl ContextGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            by_kind: HashMap::new(),
            by_label: HashMap::new(),
        }
    }

    /// Insert a node into the graph.
    pub fn insert(&mut self, node: ContextNode) -> Uuid {
        let id = node.id;
        self.by_kind.entry(node.kind).or_default().push(id);
        self.by_label.insert(node.label.clone(), id);
        self.nodes.insert(id, node);
        id
    }

    /// Add a directed edge between two nodes.
    pub fn add_edge(&mut self, from: Uuid, to: Uuid, relation: EdgeRelation) -> bool {
        if !self.nodes.contains_key(&to) {
            return false;
        }
        if let Some(node) = self.nodes.get_mut(&from) {
            node.edges.push(ContextEdge {
                target_id: to,
                relation,
                weight: 1.0,
            });
            true
        } else {
            false
        }
    }

    /// Get a node by ID.
    pub fn get(&self, id: &Uuid) -> Option<&ContextNode> {
        self.nodes.get(id)
    }

    /// Get a node by label.
    pub fn get_by_label(&self, label: &str) -> Option<&ContextNode> {
        self.by_label.get(label).and_then(|id| self.nodes.get(id))
    }

    /// Get all nodes of a given kind.
    pub fn get_by_kind(&self, kind: ContextNodeKind) -> Vec<&ContextNode> {
        self.by_kind
            .get(&kind)
            .map(|ids| ids.iter().filter_map(|id| self.nodes.get(id)).collect())
            .unwrap_or_default()
    }

    /// Total number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Collect context for a task, using progressive disclosure.
    ///
    /// Returns nodes sorted by relevance, limited to `token_budget` tokens.
    pub fn collect_context(&self, _task_id: &Uuid, token_budget: usize) -> Vec<&ContextNode> {
        let mut candidates: Vec<&ContextNode> = self.nodes.values().collect();
        candidates.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut result = Vec::new();
        let mut total_tokens = 0;

        for node in candidates {
            let tokens = node.estimated_tokens();
            if total_tokens + tokens > token_budget {
                break;
            }
            result.push(node);
            total_tokens += tokens;
        }

        result
    }

    /// Get the subgraph reachable from a given node (BFS).
    pub fn subgraph(&self, root: &Uuid, max_depth: usize) -> Vec<&ContextNode> {
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut result = Vec::new();

        if let Some(node) = self.nodes.get(root) {
            visited.insert(*root);
            queue.push_back((*root, 0));
            result.push(node);
        }

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            if let Some(node) = self.nodes.get(&current_id) {
                for edge in &node.edges {
                    if !visited.contains(&edge.target_id) {
                        visited.insert(edge.target_id);
                        if let Some(target) = self.nodes.get(&edge.target_id) {
                            result.push(target);
                            queue.push_back((edge.target_id, depth + 1));
                        }
                    }
                }
            }
        }

        result
    }
}

impl Default for ContextGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Skill Definition (agentskills.io spec)
// ---------------------------------------------------------------------------

/// A skill loaded from a SKILL.md file.
///
/// Follows the agentskills.io specification:
/// - YAML frontmatter with name, description, tools
/// - Markdown body with instructions
/// - Optional bundled files (scripts/, references/, assets/)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    /// Tools this skill is allowed to use.
    pub allowed_tools: Vec<String>,
    /// The full markdown body (Level 2 content).
    pub body: String,
    /// Path to the skill directory on disk.
    pub path: PathBuf,
    /// Optional bundled reference files (Level 3+ content).
    pub references: Vec<String>,
}

// ---------------------------------------------------------------------------
// Agent Definition (agents.md / .claude/agents/)
// ---------------------------------------------------------------------------

/// An agent definition loaded from markdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    /// System prompt / instructions for this agent.
    pub instructions: String,
    /// Tools available to this agent.
    pub allowed_tools: Vec<String>,
    /// Model preference for this agent.
    pub model: Option<String>,
    /// Source file path.
    pub source: PathBuf,
}

// ---------------------------------------------------------------------------
// Project Context Loader
// ---------------------------------------------------------------------------

/// Loads project context from the filesystem following Claude Code conventions.
///
/// Scans for:
/// - `AGENTS.md` at project root
/// - `CLAUDE.md` at project root and subdirectories
/// - `.claude/agents/*.md` for custom agent definitions
/// - `.claude/skills/*/SKILL.md` for skill definitions
/// - `todo.md` / `plan.md` for task state
pub struct ProjectContextLoader {
    project_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ProjectContextSnapshot {
    pub agents_md: Option<String>,
    pub claude_md: Option<String>,
    pub todo_md: Option<String>,
    pub agent_definitions: Vec<AgentDefinition>,
    pub skill_definitions: Vec<SkillDefinition>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct ContextCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub rebuilds: u64,
}

#[derive(Debug, Clone)]
struct CachedSnapshot {
    fingerprint: u64,
    snapshot: ProjectContextSnapshot,
}

static CONTEXT_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedSnapshot>>> = OnceLock::new();
static CONTEXT_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CONTEXT_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static CONTEXT_CACHE_REBUILDS: AtomicU64 = AtomicU64::new(0);

impl ProjectContextLoader {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }

    /// Returns global context-cache statistics for this process.
    pub fn context_cache_stats() -> ContextCacheStats {
        ContextCacheStats {
            hits: CONTEXT_CACHE_HITS.load(Ordering::Relaxed),
            misses: CONTEXT_CACHE_MISSES.load(Ordering::Relaxed),
            rebuilds: CONTEXT_CACHE_REBUILDS.load(Ordering::Relaxed),
        }
    }

    /// Load a fully parsed project context snapshot with fingerprint-based caching.
    ///
    /// Cache invalidation is driven by a filesystem fingerprint over known
    /// context paths (`AGENTS.md`, `CLAUDE.md`, `todo/plan`, `.claude/agents`,
    /// `.claude/skills`).
    pub fn load_snapshot_cached(&self) -> ProjectContextSnapshot {
        let key = std::fs::canonicalize(&self.project_root).unwrap_or(self.project_root.clone());
        let fingerprint = self.compute_context_fingerprint();
        let cache = CONTEXT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

        {
            let guard = cache.lock().expect("context cache lock poisoned");
            if let Some(entry) = guard.get(&key) {
                if entry.fingerprint == fingerprint {
                    CONTEXT_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
                    return entry.snapshot.clone();
                }
            }
        }

        CONTEXT_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
        let snapshot = self.load_snapshot_uncached();
        {
            let mut guard = cache.lock().expect("context cache lock poisoned");
            guard.insert(
                key,
                CachedSnapshot {
                    fingerprint,
                    snapshot: snapshot.clone(),
                },
            );
        }
        CONTEXT_CACHE_REBUILDS.fetch_add(1, Ordering::Relaxed);
        snapshot
    }

    fn load_snapshot_uncached(&self) -> ProjectContextSnapshot {
        ProjectContextSnapshot {
            agents_md: self.load_agents_md(),
            claude_md: self.load_claude_md(),
            todo_md: self.load_todo_md(),
            agent_definitions: self.load_agent_definitions(),
            skill_definitions: self.load_skill_definitions(),
        }
    }

    fn hash_file_meta<H: Hasher>(&self, hasher: &mut H, path: &Path) {
        path.to_string_lossy().hash(hasher);
        if let Ok(meta) = std::fs::metadata(path) {
            meta.len().hash(hasher);
            if let Ok(modified) = meta.modified() {
                if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                    dur.as_secs().hash(hasher);
                    dur.subsec_nanos().hash(hasher);
                }
            }
        }
    }

    fn compute_context_fingerprint(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.project_root.to_string_lossy().hash(&mut hasher);

        for name in ["AGENTS.md", "CLAUDE.md", "todo.md", "TODO.md", "plan.md", "PLAN.md"] {
            self.hash_file_meta(&mut hasher, &self.project_root.join(name));
        }

        let agents_dir = self.project_root.join(".claude").join("agents");
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            let mut paths: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map_or(false, |ext| ext == "md"))
                .collect();
            paths.sort();
            for path in paths {
                self.hash_file_meta(&mut hasher, &path);
            }
        }

        let skills_dir = self.project_root.join(".claude").join("skills");
        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
            let mut paths = Vec::new();
            for entry in entries.flatten() {
                let skill_md = entry.path().join("SKILL.md");
                paths.push(skill_md);
            }
            paths.sort();
            for path in paths {
                self.hash_file_meta(&mut hasher, &path);
            }
        }

        hasher.finish()
    }

    /// Load the AGENTS.md file if it exists.
    pub fn load_agents_md(&self) -> Option<String> {
        let path = self.project_root.join("AGENTS.md");
        std::fs::read_to_string(&path).ok()
    }

    /// Load the CLAUDE.md file if it exists.
    pub fn load_claude_md(&self) -> Option<String> {
        let path = self.project_root.join("CLAUDE.md");
        std::fs::read_to_string(&path).ok()
    }

    /// Load todo.md / plan.md if it exists.
    pub fn load_todo_md(&self) -> Option<String> {
        for name in ["todo.md", "TODO.md", "plan.md", "PLAN.md"] {
            let path = self.project_root.join(name);
            if let Ok(content) = std::fs::read_to_string(&path) {
                return Some(content);
            }
        }
        None
    }

    /// Discover and load all agent definitions from `.claude/agents/`.
    pub fn load_agent_definitions(&self) -> Vec<AgentDefinition> {
        let agents_dir = self.project_root.join(".claude").join("agents");
        let mut agents = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Some(agent) = parse_agent_definition(&path, &content) {
                            agents.push(agent);
                        }
                    }
                }
            }
        }

        agents
    }

    /// Discover and load all skill definitions from `.claude/skills/`.
    pub fn load_skill_definitions(&self) -> Vec<SkillDefinition> {
        let skills_dir = self.project_root.join(".claude").join("skills");
        let mut skills = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                let skill_dir = entry.path();
                if skill_dir.is_dir() {
                    let skill_md = skill_dir.join("SKILL.md");
                    if let Ok(content) = std::fs::read_to_string(&skill_md) {
                        if let Some(skill) = parse_skill_definition(&skill_dir, &content) {
                            skills.push(skill);
                        }
                    }
                }
            }
        }

        skills
    }

    /// Build a context graph from all discovered project context.
    pub fn build_context_graph(&self) -> ContextGraph {
        let mut graph = ContextGraph::new();
        let snapshot = self.load_snapshot_cached();

        // AGENTS.md
        if let Some(content) = snapshot.agents_md {
            graph.insert(ContextNode::new(
                ContextNodeKind::ProjectConfig,
                "AGENTS.md",
                content,
            ));
        }

        // CLAUDE.md
        if let Some(content) = snapshot.claude_md {
            graph.insert(ContextNode::new(
                ContextNodeKind::ProjectConfig,
                "CLAUDE.md",
                content,
            ));
        }

        // todo.md
        if let Some(content) = snapshot.todo_md {
            graph.insert(ContextNode::new(ContextNodeKind::Task, "todo.md", content));
        }

        // Agent definitions
        for agent in snapshot.agent_definitions {
            graph.insert(ContextNode::new(
                ContextNodeKind::ProjectConfig,
                format!("agent:{}", agent.name),
                agent.instructions,
            ));
        }

        // Skill definitions
        for skill in snapshot.skill_definitions {
            graph.insert(ContextNode::new(
                ContextNodeKind::Skill,
                format!("skill:{}", skill.name),
                skill.body,
            ));
        }

        graph
    }
}

// ---------------------------------------------------------------------------
// Markdown Parsing Helpers
// ---------------------------------------------------------------------------

/// Parse an agent definition from markdown with optional YAML frontmatter.
///
/// Format:
/// ```markdown
/// ---
/// name: researcher
/// description: Deep research agent
/// model: claude-sonnet-4-20250514
/// allowed_tools: [Read, Grep, Glob, WebSearch]
/// ---
///
/// ## Instructions
/// You are a research agent...
/// ```
fn parse_agent_definition(path: &Path, content: &str) -> Option<AgentDefinition> {
    let (frontmatter, body) = split_frontmatter(content);

    let name = frontmatter.get("name").cloned().unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into())
    });

    let description = frontmatter
        .get("description")
        .cloned()
        .unwrap_or_else(|| format!("Agent: {name}"));

    let model = frontmatter.get("model").cloned();

    let allowed_tools = frontmatter
        .get("allowed_tools")
        .map(|s| {
            s.trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();

    Some(AgentDefinition {
        name,
        description,
        instructions: body,
        allowed_tools,
        model,
        source: path.to_path_buf(),
    })
}

/// Parse a skill definition from SKILL.md.
fn parse_skill_definition(skill_dir: &Path, content: &str) -> Option<SkillDefinition> {
    let (frontmatter, body) = split_frontmatter(content);

    let name = frontmatter.get("name").cloned().unwrap_or_else(|| {
        skill_dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into())
    });

    let description = frontmatter
        .get("description")
        .cloned()
        .unwrap_or_else(|| format!("Skill: {name}"));

    let allowed_tools = frontmatter
        .get("allowed_tools")
        .map(|s| {
            s.trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Scan for reference files
    let refs_dir = skill_dir.join("references");
    let references = if refs_dir.exists() {
        std::fs::read_dir(&refs_dir)
            .ok()
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Some(SkillDefinition {
        name,
        description,
        body,
        allowed_tools,
        path: skill_dir.to_path_buf(),
        references,
    })
}

/// Split YAML frontmatter from markdown body.
///
/// Returns (frontmatter_map, body_text).
fn split_frontmatter(content: &str) -> (HashMap<String, String>, String) {
    let trimmed = content.trim();
    if !trimmed.starts_with("---") {
        return (HashMap::new(), content.to_string());
    }

    // Find the closing ---
    if let Some(end) = trimmed[3..].find("---") {
        let yaml_section = &trimmed[3..3 + end].trim();
        let body = trimmed[3 + end + 3..].trim().to_string();

        let mut map = HashMap::new();
        for line in yaml_section.lines() {
            if let Some((key, value)) = line.split_once(':') {
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        (map, body)
    } else {
        (HashMap::new(), content.to_string())
    }
}

// ---------------------------------------------------------------------------
// Workflow Definition (declarative task pipelines)
// ---------------------------------------------------------------------------

/// A declarative workflow definition that can be loaded from YAML/TOML.
///
/// Replaces hardcoded phase lists in TaskRunner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub name: String,
    pub description: String,
    pub phases: Vec<WorkflowPhase>,
}

/// A single phase in a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPhase {
    pub name: String,
    /// Which agent role should execute this phase.
    pub agent_role: Option<String>,
    /// Model tier to use for this phase.
    pub model_tier: Option<String>,
    /// Prompt template (with `{title}`, `{description}` placeholders).
    pub prompt_template: String,
    /// Maximum duration in seconds.
    pub timeout_secs: u64,
    /// Phases that must complete before this one can start.
    pub depends_on: Vec<String>,
    /// Whether failure in this phase aborts the workflow.
    pub required: bool,
}

impl WorkflowDefinition {
    /// The default task workflow matching current TaskRunner behavior.
    pub fn default_task_workflow() -> Self {
        Self {
            name: "default".into(),
            description: "Standard task pipeline".into(),
            phases: vec![
                WorkflowPhase {
                    name: "discovery".into(),
                    agent_role: None,
                    model_tier: Some("mid".into()),
                    prompt_template: "Analyze this task and identify what needs to be done.\nTask: {title}\nDescription: {description}".into(),
                    timeout_secs: 300,
                    depends_on: vec![],
                    required: true,
                },
                WorkflowPhase {
                    name: "context_gathering".into(),
                    agent_role: None,
                    model_tier: Some("mid".into()),
                    prompt_template: "Gather context for this task. Read relevant files, understand the codebase structure.\nTask: {title}".into(),
                    timeout_secs: 300,
                    depends_on: vec!["discovery".into()],
                    required: true,
                },
                WorkflowPhase {
                    name: "spec_creation".into(),
                    agent_role: None,
                    model_tier: Some("high".into()),
                    prompt_template: "Create a specification. Define acceptance criteria and expected behavior.\nTask: {title}".into(),
                    timeout_secs: 300,
                    depends_on: vec!["context_gathering".into()],
                    required: true,
                },
                WorkflowPhase {
                    name: "planning".into(),
                    agent_role: None,
                    model_tier: Some("high".into()),
                    prompt_template: "Plan the implementation. Break into steps, identify files to modify.\nTask: {title}".into(),
                    timeout_secs: 300,
                    depends_on: vec!["spec_creation".into()],
                    required: true,
                },
                WorkflowPhase {
                    name: "coding".into(),
                    agent_role: None,
                    model_tier: Some("high".into()),
                    prompt_template: "Implement the changes according to the plan.\nTask: {title}".into(),
                    timeout_secs: 600,
                    depends_on: vec!["planning".into()],
                    required: true,
                },
                WorkflowPhase {
                    name: "qa".into(),
                    agent_role: None,
                    model_tier: Some("mid".into()),
                    prompt_template: "Review the implementation. Run tests and verify changes.\nTask: {title}".into(),
                    timeout_secs: 300,
                    depends_on: vec!["coding".into()],
                    required: true,
                },
                WorkflowPhase {
                    name: "merging".into(),
                    agent_role: None,
                    model_tier: Some("low".into()),
                    prompt_template: "Prepare changes for merging. Ensure tests pass.\nTask: {title}".into(),
                    timeout_secs: 120,
                    depends_on: vec!["qa".into()],
                    required: false,
                },
            ],
        }
    }

    /// Get phases in topological order (respecting depends_on).
    pub fn execution_order(&self) -> Vec<&WorkflowPhase> {
        let mut result = Vec::new();
        let mut completed: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut remaining: Vec<&WorkflowPhase> = self.phases.iter().collect();

        while !remaining.is_empty() {
            let mut progressed = false;
            remaining.retain(|phase| {
                let deps_met = phase
                    .depends_on
                    .iter()
                    .all(|dep| completed.contains(dep.as_str()));
                if deps_met {
                    result.push(*phase);
                    completed.insert(&phase.name);
                    progressed = true;
                    false // remove from remaining
                } else {
                    true // keep in remaining
                }
            });
            if !progressed {
                // Circular dependency or unresolvable — add remaining as-is
                result.extend(remaining.iter());
                break;
            }
        }

        result
    }

    /// Render a phase prompt with task variables.
    pub fn render_prompt(template: &str, title: &str, description: &str) -> String {
        template
            .replace("{title}", title)
            .replace("{description}", description)
    }
}

// ---------------------------------------------------------------------------
// Health Check
// ---------------------------------------------------------------------------

/// Standardized health check result for any service or agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub service: String,
    pub status: HealthState,
    pub message: Option<String>,
    pub checks: Vec<HealthCheck>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: HealthState,
    pub message: Option<String>,
}

impl HealthStatus {
    pub fn healthy(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            status: HealthState::Healthy,
            message: None,
            checks: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    pub fn with_check(mut self, check: HealthCheck) -> Self {
        if check.status == HealthState::Unhealthy {
            self.status = HealthState::Unhealthy;
        } else if check.status == HealthState::Degraded && self.status == HealthState::Healthy {
            self.status = HealthState::Degraded;
        }
        self.checks.push(check);
        self
    }

    pub fn is_healthy(&self) -> bool {
        self.status == HealthState::Healthy
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- ContextNode --

    #[test]
    fn context_node_creation() {
        let node = ContextNode::new(ContextNodeKind::Skill, "test-skill", "Some content here");
        assert_eq!(node.kind, ContextNodeKind::Skill);
        assert_eq!(node.label, "test-skill");
        assert!(!node.content.is_empty());
        assert_eq!(node.relevance, 1.0);
    }

    #[test]
    fn context_node_token_estimate() {
        let node = ContextNode::new(ContextNodeKind::Memory, "x", "a".repeat(400));
        assert_eq!(node.estimated_tokens(), 100);
    }

    // -- ContextGraph --

    #[test]
    fn graph_insert_and_get() {
        let mut graph = ContextGraph::new();
        let node = ContextNode::new(ContextNodeKind::Skill, "my-skill", "content");
        let id = graph.insert(node);

        assert!(graph.get(&id).is_some());
        assert_eq!(graph.node_count(), 1);
    }

    #[test]
    fn graph_get_by_label() {
        let mut graph = ContextGraph::new();
        graph.insert(ContextNode::new(
            ContextNodeKind::ProjectConfig,
            "CLAUDE.md",
            "project config",
        ));

        let found = graph.get_by_label("CLAUDE.md");
        assert!(found.is_some());
        assert_eq!(found.unwrap().label, "CLAUDE.md");
    }

    #[test]
    fn graph_get_by_kind() {
        let mut graph = ContextGraph::new();
        graph.insert(ContextNode::new(ContextNodeKind::Skill, "s1", "skill 1"));
        graph.insert(ContextNode::new(ContextNodeKind::Skill, "s2", "skill 2"));
        graph.insert(ContextNode::new(ContextNodeKind::Memory, "m1", "mem"));

        let skills = graph.get_by_kind(ContextNodeKind::Skill);
        assert_eq!(skills.len(), 2);

        let memories = graph.get_by_kind(ContextNodeKind::Memory);
        assert_eq!(memories.len(), 1);
    }

    #[test]
    fn graph_add_edge() {
        let mut graph = ContextGraph::new();
        let id1 = graph.insert(ContextNode::new(ContextNodeKind::Task, "task", "t"));
        let id2 = graph.insert(ContextNode::new(ContextNodeKind::CodeArtifact, "code", "c"));

        assert!(graph.add_edge(id1, id2, EdgeRelation::References));
        assert_eq!(graph.get(&id1).unwrap().edges.len(), 1);
    }

    #[test]
    fn graph_add_edge_nonexistent_target() {
        let mut graph = ContextGraph::new();
        let id1 = graph.insert(ContextNode::new(ContextNodeKind::Task, "t", "x"));
        assert!(!graph.add_edge(id1, Uuid::new_v4(), EdgeRelation::DependsOn));
    }

    #[test]
    fn graph_collect_context_respects_budget() {
        let mut graph = ContextGraph::new();
        // Each node ≈ 25 tokens (100 chars / 4)
        for i in 0..10 {
            let mut node =
                ContextNode::new(ContextNodeKind::Memory, format!("n{i}"), "x".repeat(100));
            node.relevance = 1.0 - (i as f64 * 0.1);
            graph.insert(node);
        }

        let context = graph.collect_context(&Uuid::new_v4(), 75);
        // Budget of 75 tokens should fit ~3 nodes (25 tokens each)
        assert_eq!(context.len(), 3);
        // Should be sorted by relevance (highest first)
        assert!(context[0].relevance >= context[1].relevance);
    }

    #[test]
    fn graph_subgraph_bfs() {
        let mut graph = ContextGraph::new();
        let root = graph.insert(ContextNode::new(ContextNodeKind::Task, "root", "r"));
        let child1 = graph.insert(ContextNode::new(ContextNodeKind::CodeArtifact, "c1", "c1"));
        let child2 = graph.insert(ContextNode::new(ContextNodeKind::CodeArtifact, "c2", "c2"));
        let grandchild = graph.insert(ContextNode::new(ContextNodeKind::Memory, "gc", "gc"));

        graph.add_edge(root, child1, EdgeRelation::References);
        graph.add_edge(root, child2, EdgeRelation::References);
        graph.add_edge(child1, grandchild, EdgeRelation::DependsOn);

        let sub = graph.subgraph(&root, 1);
        assert_eq!(sub.len(), 3); // root + 2 children (not grandchild, depth=1)

        let sub_deep = graph.subgraph(&root, 2);
        assert_eq!(sub_deep.len(), 4); // root + 2 children + grandchild
    }

    #[test]
    fn graph_subgraph_nonexistent_root() {
        let graph = ContextGraph::new();
        let sub = graph.subgraph(&Uuid::new_v4(), 5);
        assert!(sub.is_empty());
    }

    // -- Frontmatter Parsing --

    #[test]
    fn split_frontmatter_with_yaml() {
        let content = "---\nname: test\ndescription: A test\n---\n\n## Body\nHello";
        let (fm, body) = split_frontmatter(content);
        assert_eq!(fm.get("name").unwrap(), "test");
        assert_eq!(fm.get("description").unwrap(), "A test");
        assert!(body.contains("## Body"));
    }

    #[test]
    fn split_frontmatter_without_yaml() {
        let content = "# Just markdown\nNo frontmatter here";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn split_frontmatter_unclosed() {
        let content = "---\nname: test\nNo closing marker";
        let (fm, _body) = split_frontmatter(content);
        assert!(fm.is_empty());
    }

    // -- Agent Definition Parsing --

    #[test]
    fn parse_agent_definition_with_frontmatter() {
        let content = "---\nname: researcher\ndescription: Research agent\nmodel: claude-sonnet-4-20250514\nallowed_tools: [Read, Grep, Glob]\n---\n\nYou are a research agent.";
        let agent = parse_agent_definition(Path::new("researcher.md"), content).unwrap();
        assert_eq!(agent.name, "researcher");
        assert_eq!(agent.description, "Research agent");
        assert_eq!(agent.model.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(agent.allowed_tools, vec!["Read", "Grep", "Glob"]);
        assert!(agent.instructions.contains("research agent"));
    }

    #[test]
    fn parse_agent_definition_minimal() {
        let content = "Just instructions, no frontmatter.";
        let agent = parse_agent_definition(Path::new("coder.md"), content).unwrap();
        assert_eq!(agent.name, "coder");
        assert!(agent.allowed_tools.is_empty());
    }

    // -- Skill Definition Parsing --

    #[test]
    fn parse_skill_definition_with_frontmatter() {
        let content = "---\nname: rust-patterns\ndescription: Rust coding patterns\nallowed_tools: [Read, Grep]\n---\n\n## Patterns\nUse iterators...";
        let skill = parse_skill_definition(Path::new("rust-patterns"), content).unwrap();
        assert_eq!(skill.name, "rust-patterns");
        assert!(skill.body.contains("Patterns"));
        assert_eq!(skill.allowed_tools, vec!["Read", "Grep"]);
    }

    // -- Workflow Definition --

    #[test]
    fn default_workflow_has_phases() {
        let wf = WorkflowDefinition::default_task_workflow();
        assert!(!wf.phases.is_empty());
        assert_eq!(wf.phases[0].name, "discovery");
    }

    #[test]
    fn workflow_execution_order_respects_deps() {
        let wf = WorkflowDefinition::default_task_workflow();
        let order = wf.execution_order();

        // Discovery should come before context_gathering
        let disc_idx = order.iter().position(|p| p.name == "discovery").unwrap();
        let ctx_idx = order
            .iter()
            .position(|p| p.name == "context_gathering")
            .unwrap();
        assert!(disc_idx < ctx_idx);

        // Coding should come before QA
        let code_idx = order.iter().position(|p| p.name == "coding").unwrap();
        let qa_idx = order.iter().position(|p| p.name == "qa").unwrap();
        assert!(code_idx < qa_idx);
    }

    #[test]
    fn workflow_execution_order_no_deps() {
        let wf = WorkflowDefinition {
            name: "parallel".into(),
            description: "All independent".into(),
            phases: vec![
                WorkflowPhase {
                    name: "a".into(),
                    agent_role: None,
                    model_tier: None,
                    prompt_template: "do A".into(),
                    timeout_secs: 60,
                    depends_on: vec![],
                    required: true,
                },
                WorkflowPhase {
                    name: "b".into(),
                    agent_role: None,
                    model_tier: None,
                    prompt_template: "do B".into(),
                    timeout_secs: 60,
                    depends_on: vec![],
                    required: true,
                },
            ],
        };
        let order = wf.execution_order();
        assert_eq!(order.len(), 2);
    }

    #[test]
    fn workflow_render_prompt() {
        let rendered = WorkflowDefinition::render_prompt(
            "Task: {title}\nDesc: {description}",
            "Fix bug",
            "The login page crashes",
        );
        assert!(rendered.contains("Fix bug"));
        assert!(rendered.contains("The login page crashes"));
    }

    // -- Health Status --

    #[test]
    fn health_status_healthy() {
        let status = HealthStatus::healthy("daemon");
        assert!(status.is_healthy());
        assert_eq!(status.service, "daemon");
    }

    #[test]
    fn health_status_degraded_on_check() {
        let status = HealthStatus::healthy("daemon").with_check(HealthCheck {
            name: "db".into(),
            status: HealthState::Degraded,
            message: Some("slow queries".into()),
        });
        assert_eq!(status.status, HealthState::Degraded);
        assert!(!status.is_healthy());
    }

    #[test]
    fn health_status_unhealthy_overrides() {
        let status = HealthStatus::healthy("daemon")
            .with_check(HealthCheck {
                name: "db".into(),
                status: HealthState::Degraded,
                message: None,
            })
            .with_check(HealthCheck {
                name: "llm".into(),
                status: HealthState::Unhealthy,
                message: Some("API down".into()),
            });
        assert_eq!(status.status, HealthState::Unhealthy);
    }

    // -- Serialization --

    #[test]
    fn context_node_serialization() {
        let node = ContextNode::new(ContextNodeKind::Skill, "test", "content");
        let json = serde_json::to_string(&node).unwrap();
        let deser: ContextNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.label, "test");
        assert_eq!(deser.kind, ContextNodeKind::Skill);
    }

    #[test]
    fn workflow_definition_serialization() {
        let wf = WorkflowDefinition::default_task_workflow();
        let json = serde_json::to_string(&wf).unwrap();
        let deser: WorkflowDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "default");
        assert_eq!(deser.phases.len(), wf.phases.len());
    }

    #[test]
    fn health_status_serialization() {
        let status = HealthStatus::healthy("test-service");
        let json = serde_json::to_string(&status).unwrap();
        let deser: HealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.service, "test-service");
    }

    #[test]
    fn agent_definition_serialization() {
        let agent = AgentDefinition {
            name: "test".into(),
            description: "Test agent".into(),
            instructions: "Be helpful".into(),
            allowed_tools: vec!["Read".into()],
            model: Some("claude-sonnet-4-20250514".into()),
            source: PathBuf::from("test.md"),
        };
        let json = serde_json::to_string(&agent).unwrap();
        let deser: AgentDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "test");
    }

    #[test]
    fn skill_definition_serialization() {
        let skill = SkillDefinition {
            name: "rust".into(),
            description: "Rust patterns".into(),
            body: "Use iterators".into(),
            allowed_tools: vec!["Read".into(), "Grep".into()],
            path: PathBuf::from("skills/rust"),
            references: vec!["patterns.md".into()],
        };
        let json = serde_json::to_string(&skill).unwrap();
        let deser: SkillDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "rust");
        assert_eq!(deser.references.len(), 1);
    }

    // -- ProjectContextLoader (with temp dirs) --

    #[test]
    fn loader_returns_none_for_missing_files() {
        let loader = ProjectContextLoader::new("/nonexistent/path");
        assert!(loader.load_agents_md().is_none());
        assert!(loader.load_claude_md().is_none());
        assert!(loader.load_todo_md().is_none());
    }

    #[test]
    fn loader_loads_agents_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "# Agents\nBe helpful").unwrap();

        let loader = ProjectContextLoader::new(dir.path());
        let content = loader.load_agents_md().unwrap();
        assert!(content.contains("Agents"));
    }

    #[test]
    fn loader_loads_todo_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("todo.md"), "- [ ] Fix bug\n- [x] Done").unwrap();

        let loader = ProjectContextLoader::new(dir.path());
        let content = loader.load_todo_md().unwrap();
        assert!(content.contains("Fix bug"));
    }

    #[test]
    fn loader_loads_agent_definitions() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("coder.md"),
            "---\nname: coder\ndescription: Code writer\n---\n\nWrite clean code.",
        )
        .unwrap();

        let loader = ProjectContextLoader::new(dir.path());
        let agents = loader.load_agent_definitions();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "coder");
    }

    #[test]
    fn loader_loads_skill_definitions() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".claude").join("skills").join("rust");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: rust-patterns\ndescription: Rust coding patterns\n---\n\nUse iterators.",
        )
        .unwrap();

        let loader = ProjectContextLoader::new(dir.path());
        let skills = loader.load_skill_definitions();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "rust-patterns");
    }

    #[test]
    fn loader_builds_context_graph() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "# Agents config").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Project rules").unwrap();

        let loader = ProjectContextLoader::new(dir.path());
        let graph = loader.build_context_graph();
        assert!(graph.node_count() >= 2);
        assert!(graph.get_by_label("AGENTS.md").is_some());
        assert!(graph.get_by_label("CLAUDE.md").is_some());
    }

    // -- Edge Relations --

    #[test]
    fn edge_relation_serialization() {
        let edge = ContextEdge {
            target_id: Uuid::new_v4(),
            relation: EdgeRelation::DependsOn,
            weight: 0.8,
        };
        let json = serde_json::to_string(&edge).unwrap();
        let deser: ContextEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.relation, EdgeRelation::DependsOn);
    }
}
