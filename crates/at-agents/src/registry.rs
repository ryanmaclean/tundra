use std::collections::HashMap;
use std::path::Path;

use at_core::context_engine::{AgentDefinition, ProjectContextLoader, SkillDefinition};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::roles::RoleConfig;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("agent not found: `{0}`")]
    AgentNotFound(String),
    #[error("skill not found: `{0}`")]
    SkillNotFound(String),
    #[error("duplicate agent name: `{0}`")]
    DuplicateAgent(String),
    #[error("duplicate skill name: `{0}`")]
    DuplicateSkill(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// PluginAgent — dynamic agent loaded from markdown
// ---------------------------------------------------------------------------

/// A dynamically loaded agent backed by an `AgentDefinition` from a markdown
/// file (e.g. `.claude/agents/researcher.md`).  Implements `RoleConfig` so it
/// can be used interchangeably with the built-in agent roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAgent {
    pub definition: AgentDefinition,
}

impl PluginAgent {
    pub fn new(definition: AgentDefinition) -> Self {
        Self { definition }
    }

    pub fn name(&self) -> &str {
        &self.definition.name
    }
}

impl RoleConfig for PluginAgent {
    fn system_prompt(&self) -> &str {
        &self.definition.instructions
    }

    fn allowed_tools(&self) -> Vec<String> {
        self.definition.allowed_tools.clone()
    }

    fn max_turns(&self) -> u32 {
        50 // default for plugin agents
    }

    fn preferred_model(&self) -> Option<&str> {
        self.definition.model.as_deref()
    }
}

// ---------------------------------------------------------------------------
// PluginSkill — dynamic skill loaded from markdown
// ---------------------------------------------------------------------------

/// A dynamically loaded skill backed by a `SkillDefinition` from a markdown
/// file (e.g. `.claude/skills/deploy/SKILL.md`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSkill {
    pub definition: SkillDefinition,
}

impl PluginSkill {
    pub fn new(definition: SkillDefinition) -> Self {
        Self { definition }
    }

    pub fn name(&self) -> &str {
        &self.definition.name
    }

    /// Generate the prompt text for invoking this skill.
    pub fn to_prompt(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("# Skill: {}", self.definition.name));
        if !self.definition.description.is_empty() {
            parts.push(self.definition.description.clone());
        }
        parts.push(self.definition.body.clone());
        parts.join("\n\n")
    }
}

// ---------------------------------------------------------------------------
// AgentRegistry — central plugin registry
// ---------------------------------------------------------------------------

/// Central registry for both built-in and dynamically loaded agents/skills.
///
/// The registry supports:
/// - Registering agents from markdown files (`.claude/agents/*.md`)
/// - Registering skills from markdown files (`.claude/skills/*/SKILL.md`)
/// - Querying agents/skills by name
/// - Bulk loading from a project directory via `ProjectContextLoader`
#[derive(Debug)]
pub struct AgentRegistry {
    agents: HashMap<String, PluginAgent>,
    skills: HashMap<String, PluginSkill>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            skills: HashMap::new(),
        }
    }

    // -- Agent operations --

    /// Register a plugin agent.  Returns error if name already exists.
    pub fn register_agent(&mut self, agent: PluginAgent) -> Result<(), RegistryError> {
        let name = agent.name().to_string();
        if self.agents.contains_key(&name) {
            return Err(RegistryError::DuplicateAgent(name));
        }
        debug!(name = %name, "registered plugin agent");
        self.agents.insert(name, agent);
        Ok(())
    }

    /// Get a plugin agent by name.
    pub fn get_agent(&self, name: &str) -> Option<&PluginAgent> {
        self.agents.get(name)
    }

    /// List all registered agent names.
    pub fn agent_names(&self) -> Vec<&str> {
        self.agents.keys().map(|s| s.as_str()).collect()
    }

    /// Number of registered agents.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Remove a plugin agent by name.
    pub fn unregister_agent(&mut self, name: &str) -> Option<PluginAgent> {
        self.agents.remove(name)
    }

    // -- Skill operations --

    /// Register a plugin skill.  Returns error if name already exists.
    pub fn register_skill(&mut self, skill: PluginSkill) -> Result<(), RegistryError> {
        let name = skill.name().to_string();
        if self.skills.contains_key(&name) {
            return Err(RegistryError::DuplicateSkill(name));
        }
        debug!(name = %name, "registered plugin skill");
        self.skills.insert(name, skill);
        Ok(())
    }

    /// Get a plugin skill by name.
    pub fn get_skill(&self, name: &str) -> Option<&PluginSkill> {
        self.skills.get(name)
    }

    /// List all registered skill names.
    pub fn skill_names(&self) -> Vec<&str> {
        self.skills.keys().map(|s| s.as_str()).collect()
    }

    /// Number of registered skills.
    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }

    /// Remove a plugin skill by name.
    pub fn unregister_skill(&mut self, name: &str) -> Option<PluginSkill> {
        self.skills.remove(name)
    }

    // -- Bulk loading --

    /// Load agents and skills from a project directory by scanning for
    /// `.claude/agents/*.md` and `.claude/skills/*/SKILL.md`.
    pub fn load_from_project(&mut self, project_root: &Path) -> Result<LoadResult, RegistryError> {
        let loader = ProjectContextLoader::new(project_root.to_path_buf());
        let mut result = LoadResult::default();

        // Load agents
        let agents = loader.load_agent_definitions();
        for def in agents {
            let name = def.name.clone();
            match self.register_agent(PluginAgent::new(def)) {
                Ok(()) => result.agents_loaded += 1,
                Err(RegistryError::DuplicateAgent(_)) => {
                    warn!(name = %name, "skipping duplicate agent");
                    result.agents_skipped += 1;
                }
                Err(e) => return Err(e),
            }
        }

        // Load skills
        let skills = loader.load_skill_definitions();
        for def in skills {
            let name = def.name.clone();
            match self.register_skill(PluginSkill::new(def)) {
                Ok(()) => result.skills_loaded += 1,
                Err(RegistryError::DuplicateSkill(_)) => {
                    warn!(name = %name, "skipping duplicate skill");
                    result.skills_skipped += 1;
                }
                Err(e) => return Err(e),
            }
        }

        info!(
            agents = result.agents_loaded,
            skills = result.skills_loaded,
            "loaded plugins from project"
        );

        Ok(result)
    }

    /// Get a cloned `PluginAgent` for use as a `RoleConfig`.
    pub fn clone_agent(&self, name: &str) -> Result<PluginAgent, RegistryError> {
        self.agents
            .get(name)
            .cloned()
            .ok_or_else(|| RegistryError::AgentNotFound(name.to_string()))
    }

    /// Generate a combined context string from all registered agents and skills,
    /// suitable for injection into an LLM system prompt.
    pub fn context_summary(&self) -> String {
        let mut parts = Vec::new();

        if !self.agents.is_empty() {
            parts.push("## Available Agents".to_string());
            for (name, agent) in &self.agents {
                let desc = &agent.definition.description;
                parts.push(format!("- **{}**: {}", name, desc));
            }
        }

        if !self.skills.is_empty() {
            parts.push("## Available Skills".to_string());
            for (name, skill) in &self.skills {
                let desc = &skill.definition.description;
                if desc.is_empty() {
                    parts.push(format!("- **{}**: (no description)", name));
                } else {
                    parts.push(format!("- **{}**: {}", name, desc));
                }
            }
        }

        parts.join("\n")
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// LoadResult
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct LoadResult {
    pub agents_loaded: usize,
    pub agents_skipped: usize,
    pub skills_loaded: usize,
    pub skills_skipped: usize,
}

impl LoadResult {
    pub fn total_loaded(&self) -> usize {
        self.agents_loaded + self.skills_loaded
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_agent_def(name: &str) -> AgentDefinition {
        AgentDefinition {
            name: name.to_string(),
            description: format!("Test agent {}", name),
            instructions: format!("You are the {} agent.", name),
            allowed_tools: vec!["file_read".to_string(), "shell_execute".to_string()],
            model: Some("claude-sonnet-4-20250514".to_string()),
            source: PathBuf::from(format!(".claude/agents/{}.md", name)),
        }
    }

    fn make_skill_def(name: &str) -> SkillDefinition {
        SkillDefinition {
            name: name.to_string(),
            description: format!("Test skill {}", name),
            allowed_tools: vec!["file_read".to_string()],
            body: format!("Skill body for {}", name),
            path: PathBuf::from(format!(".claude/skills/{}/SKILL.md", name)),
            references: vec![],
        }
    }

    #[test]
    fn register_and_get_agent() {
        let mut reg = AgentRegistry::new();
        reg.register_agent(PluginAgent::new(make_agent_def("researcher")))
            .unwrap();
        assert_eq!(reg.agent_count(), 1);
        let a = reg.get_agent("researcher").unwrap();
        assert_eq!(a.name(), "researcher");
    }

    #[test]
    fn register_duplicate_agent_fails() {
        let mut reg = AgentRegistry::new();
        reg.register_agent(PluginAgent::new(make_agent_def("researcher")))
            .unwrap();
        let err = reg
            .register_agent(PluginAgent::new(make_agent_def("researcher")))
            .unwrap_err();
        assert!(matches!(err, RegistryError::DuplicateAgent(_)));
    }

    #[test]
    fn register_and_get_skill() {
        let mut reg = AgentRegistry::new();
        reg.register_skill(PluginSkill::new(make_skill_def("deploy")))
            .unwrap();
        assert_eq!(reg.skill_count(), 1);
        let s = reg.get_skill("deploy").unwrap();
        assert_eq!(s.name(), "deploy");
    }

    #[test]
    fn register_duplicate_skill_fails() {
        let mut reg = AgentRegistry::new();
        reg.register_skill(PluginSkill::new(make_skill_def("deploy")))
            .unwrap();
        let err = reg
            .register_skill(PluginSkill::new(make_skill_def("deploy")))
            .unwrap_err();
        assert!(matches!(err, RegistryError::DuplicateSkill(_)));
    }

    #[test]
    fn unregister_agent() {
        let mut reg = AgentRegistry::new();
        reg.register_agent(PluginAgent::new(make_agent_def("temp")))
            .unwrap();
        assert_eq!(reg.agent_count(), 1);
        let removed = reg.unregister_agent("temp");
        assert!(removed.is_some());
        assert_eq!(reg.agent_count(), 0);
    }

    #[test]
    fn unregister_skill() {
        let mut reg = AgentRegistry::new();
        reg.register_skill(PluginSkill::new(make_skill_def("temp")))
            .unwrap();
        let removed = reg.unregister_skill("temp");
        assert!(removed.is_some());
        assert_eq!(reg.skill_count(), 0);
    }

    #[test]
    fn agent_names_list() {
        let mut reg = AgentRegistry::new();
        reg.register_agent(PluginAgent::new(make_agent_def("alpha")))
            .unwrap();
        reg.register_agent(PluginAgent::new(make_agent_def("beta")))
            .unwrap();
        let mut names = reg.agent_names();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn skill_names_list() {
        let mut reg = AgentRegistry::new();
        reg.register_skill(PluginSkill::new(make_skill_def("deploy")))
            .unwrap();
        reg.register_skill(PluginSkill::new(make_skill_def("test")))
            .unwrap();
        let mut names = reg.skill_names();
        names.sort();
        assert_eq!(names, vec!["deploy", "test"]);
    }

    #[test]
    fn plugin_agent_implements_role_config() {
        let agent = PluginAgent::new(make_agent_def("coder"));
        assert!(agent.system_prompt().contains("coder"));
        assert_eq!(agent.allowed_tools(), vec!["file_read", "shell_execute"]);
        assert_eq!(agent.max_turns(), 50);
        assert_eq!(agent.preferred_model(), Some("claude-sonnet-4-20250514"));
    }

    #[test]
    fn plugin_skill_to_prompt() {
        let skill = PluginSkill::new(make_skill_def("deploy"));
        let prompt = skill.to_prompt();
        assert!(prompt.contains("# Skill: deploy"));
        assert!(prompt.contains("Test skill deploy"));
        assert!(prompt.contains("Skill body for deploy"));
    }

    #[test]
    fn clone_agent_registered() {
        let mut reg = AgentRegistry::new();
        reg.register_agent(PluginAgent::new(make_agent_def("reviewer")))
            .unwrap();
        let agent = reg.clone_agent("reviewer").unwrap();
        assert!(agent.system_prompt().contains("reviewer"));
    }

    #[test]
    fn clone_agent_missing() {
        let reg = AgentRegistry::new();
        let err = reg.clone_agent("nope").unwrap_err();
        assert!(matches!(err, RegistryError::AgentNotFound(_)));
    }

    #[test]
    fn context_summary_includes_agents_and_skills() {
        let mut reg = AgentRegistry::new();
        reg.register_agent(PluginAgent::new(make_agent_def("researcher")))
            .unwrap();
        reg.register_skill(PluginSkill::new(make_skill_def("deploy")))
            .unwrap();
        let summary = reg.context_summary();
        assert!(summary.contains("Available Agents"));
        assert!(summary.contains("researcher"));
        assert!(summary.contains("Available Skills"));
        assert!(summary.contains("deploy"));
    }

    #[test]
    fn context_summary_empty_registry() {
        let reg = AgentRegistry::new();
        let summary = reg.context_summary();
        assert!(summary.is_empty());
    }

    #[test]
    fn load_result_total() {
        let result = LoadResult {
            agents_loaded: 3,
            agents_skipped: 1,
            skills_loaded: 2,
            skills_skipped: 0,
        };
        assert_eq!(result.total_loaded(), 5);
    }

    #[test]
    fn load_from_empty_project() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = AgentRegistry::new();
        let result = reg.load_from_project(dir.path()).unwrap();
        assert_eq!(result.agents_loaded, 0);
        assert_eq!(result.skills_loaded, 0);
    }

    #[test]
    fn load_from_project_with_agents() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("researcher.md"),
            "---\nname: researcher\nallowed_tools:\n  - file_read\n---\nYou are a researcher.",
        )
        .unwrap();

        let mut reg = AgentRegistry::new();
        let result = reg.load_from_project(dir.path()).unwrap();
        assert_eq!(result.agents_loaded, 1);
        assert!(reg.get_agent("researcher").is_some());
    }

    #[test]
    fn load_from_project_with_skills() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".claude").join("skills").join("deploy");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: deploy\ndescription: Deploy to production\nallowed_tools:\n  - shell_execute\n---\nRun the deploy script.",
        )
        .unwrap();

        let mut reg = AgentRegistry::new();
        let result = reg.load_from_project(dir.path()).unwrap();
        assert_eq!(result.skills_loaded, 1);
        assert!(reg.get_skill("deploy").is_some());
    }

    #[test]
    fn load_from_project_with_both() {
        let dir = tempfile::tempdir().unwrap();

        // Agent
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("planner.md"),
            "---\nname: planner\nallowed_tools:\n  - file_read\n---\nYou plan things.",
        )
        .unwrap();

        // Skill
        let skill_dir = dir.path().join(".claude").join("skills").join("test");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: test-runner\ndescription: Run tests\nallowed_tools:\n  - shell_execute\n---\nRun cargo test.",
        )
        .unwrap();

        let mut reg = AgentRegistry::new();
        let result = reg.load_from_project(dir.path()).unwrap();
        assert_eq!(result.agents_loaded, 1);
        assert_eq!(result.skills_loaded, 1);
        assert_eq!(result.total_loaded(), 2);
    }

    #[test]
    fn load_skips_duplicates_on_reload() {
        let dir = tempfile::tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("worker.md"),
            "---\nname: worker\nallowed_tools:\n  - file_read\n---\nYou work.",
        )
        .unwrap();

        let mut reg = AgentRegistry::new();
        let r1 = reg.load_from_project(dir.path()).unwrap();
        assert_eq!(r1.agents_loaded, 1);

        // Reload same project — should skip the duplicate
        let r2 = reg.load_from_project(dir.path()).unwrap();
        assert_eq!(r2.agents_loaded, 0);
        assert_eq!(r2.agents_skipped, 1);
        assert_eq!(reg.agent_count(), 1); // still only one
    }

    #[test]
    fn default_registry_is_empty() {
        let reg = AgentRegistry::default();
        assert_eq!(reg.agent_count(), 0);
        assert_eq!(reg.skill_count(), 0);
    }

    #[test]
    fn get_missing_agent_returns_none() {
        let reg = AgentRegistry::new();
        assert!(reg.get_agent("nonexistent").is_none());
    }

    #[test]
    fn get_missing_skill_returns_none() {
        let reg = AgentRegistry::new();
        assert!(reg.get_skill("nonexistent").is_none());
    }
}
