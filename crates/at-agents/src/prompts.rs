//! Prompt template system for specialized agent roles.
//!
//! Each agent role has a prompt template that defines its specialized behavior.
//! Templates are loaded from `.claude/prompts/*.md` or use built-in defaults.
//! Variables in templates are expanded at runtime: `{title}`, `{description}`,
//! `{context}`, `{conventions}`, `{task_spec}`.
//!
//! This mirrors Auto Claude's `apps/backend/prompts/` directory with 23+ templates.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use at_core::types::AgentRole;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// PromptTemplate
// ---------------------------------------------------------------------------

/// A prompt template for a specific agent role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub role: AgentRole,
    pub name: String,
    /// The raw template text with `{variable}` placeholders.
    pub template: String,
    /// Source of this template (built-in or file path).
    pub source: PromptSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptSource {
    BuiltIn,
    File(PathBuf),
}

impl PromptTemplate {
    /// Render the template with the given variables.
    pub fn render(&self, vars: &HashMap<String, String>) -> String {
        let mut output = self.template.clone();
        for (key, value) in vars {
            output = output.replace(&format!("{{{}}}", key), value);
        }
        output
    }

    /// Render with common task variables.
    pub fn render_task(&self, title: &str, description: &str, context: &str) -> String {
        let mut vars = HashMap::new();
        vars.insert("title".into(), title.into());
        vars.insert("description".into(), description.into());
        vars.insert("context".into(), context.into());
        self.render(&vars)
    }
}

// ---------------------------------------------------------------------------
// PromptRegistry — loads and serves prompt templates
// ---------------------------------------------------------------------------

/// Registry of prompt templates for all agent roles.
#[derive(Debug)]
pub struct PromptRegistry {
    templates: HashMap<AgentRole, PromptTemplate>,
}

impl PromptRegistry {
    /// Create a new registry pre-loaded with built-in defaults.
    pub fn new() -> Self {
        let mut reg = Self {
            templates: HashMap::new(),
        };
        reg.load_defaults();
        reg
    }

    /// Get the prompt template for a given role.
    pub fn get(&self, role: &AgentRole) -> Option<&PromptTemplate> {
        self.templates.get(role)
    }

    /// Override a template (e.g., from a project-specific file).
    pub fn set(&mut self, template: PromptTemplate) {
        self.templates.insert(template.role.clone(), template);
    }

    /// Number of loaded templates.
    pub fn count(&self) -> usize {
        self.templates.len()
    }

    /// Load project-specific prompt overrides from `.claude/prompts/`.
    pub fn load_from_project(&mut self, project_root: &Path) {
        let prompts_dir = project_root.join(".claude").join("prompts");
        if let Ok(entries) = std::fs::read_dir(&prompts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let name = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        if let Some(role) = role_from_prompt_name(&name) {
                            self.set(PromptTemplate {
                                role,
                                name: name.clone(),
                                template: content,
                                source: PromptSource::File(path),
                            });
                        }
                    }
                }
            }
        }
    }

    /// List all registered role names.
    pub fn roles(&self) -> Vec<&AgentRole> {
        self.templates.keys().collect()
    }

    fn load_defaults(&mut self) {
        let defaults = built_in_templates();
        for tpl in defaults {
            self.templates.insert(tpl.role.clone(), tpl);
        }
    }
}

impl Default for PromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a prompt filename to an AgentRole.
fn role_from_prompt_name(name: &str) -> Option<AgentRole> {
    match name {
        "coder" => Some(AgentRole::Coder),
        "coder_recovery" => Some(AgentRole::CoderRecovery),
        "planner" => Some(AgentRole::Planner),
        "followup_planner" => Some(AgentRole::FollowupPlanner),
        "qa_reviewer" => Some(AgentRole::QaReviewer),
        "qa_fixer" => Some(AgentRole::QaFixer),
        "spec_gatherer" => Some(AgentRole::SpecGatherer),
        "spec_writer" => Some(AgentRole::SpecWriter),
        "spec_researcher" => Some(AgentRole::SpecResearcher),
        "spec_critic" => Some(AgentRole::SpecCritic),
        "spec_validator" | "validate_spec" => Some(AgentRole::SpecValidator),
        "validation_fixer" => Some(AgentRole::ValidationFixer),
        "insight_extractor" => Some(AgentRole::InsightExtractor),
        "complexity_assessor" => Some(AgentRole::ComplexityAssessor),
        "competitor_analysis" => Some(AgentRole::CompetitorAnalysis),
        "ideation_code_quality" => Some(AgentRole::IdeationCodeQuality),
        "ideation_performance" => Some(AgentRole::IdeationPerformance),
        "ideation_security" => Some(AgentRole::IdeationSecurity),
        "ideation_documentation" => Some(AgentRole::IdeationDocumentation),
        "ideation_ui_ux" => Some(AgentRole::IdeationUiUx),
        "ideation_code_improvements" => Some(AgentRole::IdeationCodeImprovements),
        "roadmap_discovery" => Some(AgentRole::RoadmapDiscovery),
        "roadmap_features" => Some(AgentRole::RoadmapFeatures),
        "commit_message" => Some(AgentRole::CommitMessage),
        "pr_template_filler" | "pr_template" => Some(AgentRole::PrTemplateFiller),
        "merge_resolver" => Some(AgentRole::MergeResolver),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Built-in prompt templates
// ---------------------------------------------------------------------------

fn built_in_templates() -> Vec<PromptTemplate> {
    vec![
        // -- Spec Pipeline --
        PromptTemplate {
            role: AgentRole::SpecGatherer,
            name: "spec_gatherer".into(),
            template: SPEC_GATHERER_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::SpecWriter,
            name: "spec_writer".into(),
            template: SPEC_WRITER_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::SpecResearcher,
            name: "spec_researcher".into(),
            template: SPEC_RESEARCHER_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::SpecCritic,
            name: "spec_critic".into(),
            template: SPEC_CRITIC_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::SpecValidator,
            name: "spec_validator".into(),
            template: SPEC_VALIDATOR_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        // -- Planning --
        PromptTemplate {
            role: AgentRole::Planner,
            name: "planner".into(),
            template: PLANNER_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::FollowupPlanner,
            name: "followup_planner".into(),
            template: FOLLOWUP_PLANNER_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        // -- Coding --
        PromptTemplate {
            role: AgentRole::Coder,
            name: "coder".into(),
            template: CODER_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::CoderRecovery,
            name: "coder_recovery".into(),
            template: CODER_RECOVERY_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        // -- QA --
        PromptTemplate {
            role: AgentRole::QaReviewer,
            name: "qa_reviewer".into(),
            template: QA_REVIEWER_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::QaFixer,
            name: "qa_fixer".into(),
            template: QA_FIXER_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::ValidationFixer,
            name: "validation_fixer".into(),
            template: VALIDATION_FIXER_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        // -- Analysis --
        PromptTemplate {
            role: AgentRole::InsightExtractor,
            name: "insight_extractor".into(),
            template: INSIGHT_EXTRACTOR_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::ComplexityAssessor,
            name: "complexity_assessor".into(),
            template: COMPLEXITY_ASSESSOR_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        // -- Ideation --
        PromptTemplate {
            role: AgentRole::IdeationCodeQuality,
            name: "ideation_code_quality".into(),
            template: IDEATION_CODE_QUALITY_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::IdeationSecurity,
            name: "ideation_security".into(),
            template: IDEATION_SECURITY_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        // -- Roadmap --
        PromptTemplate {
            role: AgentRole::RoadmapDiscovery,
            name: "roadmap_discovery".into(),
            template: ROADMAP_DISCOVERY_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::RoadmapFeatures,
            name: "roadmap_features".into(),
            template: ROADMAP_FEATURES_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        // -- Utilities --
        PromptTemplate {
            role: AgentRole::CommitMessage,
            name: "commit_message".into(),
            template: COMMIT_MESSAGE_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
        PromptTemplate {
            role: AgentRole::PrTemplateFiller,
            name: "pr_template_filler".into(),
            template: PR_TEMPLATE_FILLER_PROMPT.into(),
            source: PromptSource::BuiltIn,
        },
    ]
}

// ---------------------------------------------------------------------------
// Built-in prompt text constants
// ---------------------------------------------------------------------------

const SPEC_GATHERER_PROMPT: &str = "\
You are the Spec Gatherer agent. Your job is to understand what needs to be built.

{context}

## Task
Title: {title}
Description: {description}

## Instructions
1. Analyze the task description and identify all requirements.
2. Read relevant files to understand the current codebase structure.
3. Identify acceptance criteria and edge cases.
4. List dependencies and affected components.
5. Output a structured requirements summary.

Be thorough but concise. Focus on WHAT needs to happen, not HOW.";

const SPEC_WRITER_PROMPT: &str = "\
You are the Spec Writer agent. You produce detailed technical specifications.

{context}

## Task
Title: {title}
Description: {description}

## Instructions
1. Take the gathered requirements and write a formal specification.
2. Define clear acceptance criteria with testable conditions.
3. Specify data models, API contracts, and interface changes.
4. Document error handling requirements.
5. List out-of-scope items explicitly.

Output a structured spec document that a Coder agent can implement from.";

const SPEC_RESEARCHER_PROMPT: &str = "\
You are the Spec Researcher agent. You deep-dive into the codebase to inform specifications.

{context}

## Task
Title: {title}

## Instructions
1. Search the codebase for patterns relevant to this task.
2. Identify existing abstractions that should be reused.
3. Find similar implementations to use as reference.
4. Note any technical constraints or limitations.
5. Document dependencies and integration points.

Focus on reading and understanding code. Do NOT modify any files.";

const SPEC_CRITIC_PROMPT: &str = "\
You are the Spec Critic agent. You review specifications for completeness and correctness.

{context}

## Specification to Review
{description}

## Instructions
1. Check for missing requirements or edge cases.
2. Verify acceptance criteria are testable and complete.
3. Look for contradictions or ambiguities.
4. Assess feasibility given the current codebase.
5. Rate the spec quality (1-5) with specific improvement suggestions.

Be constructive but rigorous. A good spec prevents wasted implementation effort.";

const SPEC_VALIDATOR_PROMPT: &str = "\
You are the Spec Validator agent. You verify implementations match their specifications.

{context}

## Task
Title: {title}

## Instructions
1. Compare the implementation against the spec's acceptance criteria.
2. Run tests and verify each criterion passes.
3. Check for regressions in existing functionality.
4. Verify error handling matches spec requirements.
5. Report pass/fail for each criterion with evidence.

Be systematic. Every acceptance criterion must be explicitly verified.";

const PLANNER_PROMPT: &str = "\
You are the Planner agent. You create implementation plans from specifications.

{context}

## Task
Title: {title}
Description: {description}

## Instructions
1. Break the spec into ordered subtasks.
2. Identify which files need to be created or modified.
3. Define dependencies between subtasks.
4. Estimate complexity for each subtask.
5. Assign suggested agent roles for each subtask.

Output a structured plan with clear ordering and dependencies.";

const FOLLOWUP_PLANNER_PROMPT: &str = "\
You are the Follow-up Planner agent. You handle incomplete or failed subtasks.

{context}

## Task
Title: {title}

## Instructions
1. Analyze the current state of the implementation.
2. Identify what was completed and what remains.
3. Assess any errors or failures from previous attempts.
4. Create a recovery plan with corrective actions.
5. Re-prioritize remaining work based on current state.

Focus on practical recovery. Don't restart from scratch unless necessary.";

const CODER_PROMPT: &str = "\
You are the Coder agent, an autonomous software implementation specialist.

{context}

## Task
Title: {title}
Description: {description}

## Instructions
1. Follow the implementation plan precisely.
2. Write clean, tested code following project conventions.
3. Run tests after each significant change.
4. Commit changes with clear messages.
5. If you encounter blockers, document them and move to the next subtask.

Focus on correctness first, then cleanliness. Every change should have a test.";

const CODER_RECOVERY_PROMPT: &str = "\
You are the Coder Recovery agent. You fix failed implementations.

{context}

## Task
Title: {title}

## Instructions
1. Analyze the error or failure from the previous coding session.
2. Identify the root cause (compilation error, test failure, logic bug).
3. Apply the minimal fix needed to resolve the issue.
4. Verify the fix by running relevant tests.
5. If the fix is complex, create a rollback plan first.

Be surgical. Fix the specific issue without introducing new changes.";

const QA_REVIEWER_PROMPT: &str = "\
You are the QA Reviewer agent. You review code changes for quality and correctness.

{context}

## Task
Title: {title}

## Instructions
1. Review all changed files for correctness.
2. Check that tests cover the new functionality.
3. Verify error handling is complete.
4. Ensure code follows project conventions.
5. Rate each file change (pass/fail) with specific feedback.

Focus on catching bugs, not style preferences. Flag only actionable issues.";

const QA_FIXER_PROMPT: &str = "\
You are the QA Fixer agent. You resolve issues found during QA review.

{context}

## Task
Title: {title}

## Instructions
1. Read the QA review feedback.
2. Fix each flagged issue.
3. Add or update tests for each fix.
4. Run the full test suite to verify no regressions.
5. Report what was fixed and what tests were added.

Each fix should be a separate commit with a clear message.";

const VALIDATION_FIXER_PROMPT: &str = "\
You are the Validation Fixer agent. You fix spec validation failures.

{context}

## Task
Title: {title}

## Instructions
1. Identify which acceptance criteria are failing.
2. Determine why each criterion fails.
3. Implement the minimal changes to pass validation.
4. Re-run validation to confirm the fix.
5. Document any spec changes needed.";

const INSIGHT_EXTRACTOR_PROMPT: &str = "\
You are the Insight Extractor agent. You analyze agent sessions and extract learnings.

{context}

## Instructions
1. Review the session transcript and tool usage.
2. Identify patterns in what worked and what didn't.
3. Extract reusable insights (e.g., 'file X is always needed for feature Y').
4. Note any recurring errors or blockers.
5. Suggest process improvements for future sessions.

Output structured insights that can be stored as memories for future sessions.";

const COMPLEXITY_ASSESSOR_PROMPT: &str = "\
You are the Complexity Assessor agent. You evaluate task complexity.

{context}

## Task
Title: {title}
Description: {description}

## Instructions
1. Assess the number of files likely to be modified.
2. Evaluate the risk of regressions.
3. Estimate the number of subtasks needed.
4. Rate overall complexity (1-5).
5. Identify the highest-risk components.

Output a structured complexity assessment with a suggested approach.";

const IDEATION_CODE_QUALITY_PROMPT: &str = "\
You are the Code Quality Ideation agent. You identify code quality improvements.

{context}

## Instructions
1. Scan for code smells, duplication, and complexity hotspots.
2. Identify refactoring opportunities.
3. Suggest naming improvements.
4. Find dead code or unused dependencies.
5. Prioritize suggestions by impact.

Output structured improvement suggestions with file paths and descriptions.";

const IDEATION_SECURITY_PROMPT: &str = "\
You are the Security Ideation agent. You proactively identify security concerns.

{context}

## Instructions
1. Scan for common vulnerability patterns (injection, XSS, CSRF).
2. Check authentication and authorization logic.
3. Review dependency security (known CVEs).
4. Identify hardcoded secrets or credentials.
5. Suggest security hardening measures.

Output structured findings with severity ratings and remediation steps.";

const ROADMAP_DISCOVERY_PROMPT: &str = "\
You are the Roadmap Discovery agent. You analyze the project to suggest future work.

{context}

## Instructions
1. Analyze the codebase for incomplete features and TODOs.
2. Identify technical debt and maintenance needs.
3. Suggest feature improvements based on usage patterns.
4. Categorize findings by priority and effort.
5. Output a structured roadmap of suggested work items.";

const ROADMAP_FEATURES_PROMPT: &str = "\
You are the Roadmap Features agent. You flesh out feature ideas into actionable items.

{context}

## Instructions
1. Take a high-level feature idea and break it into phases.
2. Define acceptance criteria for each phase.
3. Identify dependencies and prerequisites.
4. Estimate effort for each phase.
5. Suggest an implementation order.

Output a structured feature roadmap ready for task creation.";

const COMMIT_MESSAGE_PROMPT: &str = "\
You are the Commit Message agent. You generate clear, conventional commit messages.

## Changes
{description}

## Instructions
Generate a commit message following Conventional Commits format:
- type(scope): subject
- Body explaining what changed and why
- Footer with breaking changes or issue references

Keep the subject under 72 characters. Focus on WHY, not WHAT.";

const PR_TEMPLATE_FILLER_PROMPT: &str = "\
You are the PR Template Filler agent. You create pull request descriptions.

{context}

## Changes
{description}

## Instructions
1. Summarize all changes made in this PR.
2. Link to the original task/issue.
3. List files changed with brief descriptions.
4. Note any breaking changes.
5. Suggest reviewers based on code ownership.

Output a complete PR description following the project's template.";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_registry_has_defaults() {
        let reg = PromptRegistry::new();
        assert!(reg.count() > 0);
        assert!(reg.get(&AgentRole::Coder).is_some());
        assert!(reg.get(&AgentRole::SpecGatherer).is_some());
        assert!(reg.get(&AgentRole::QaReviewer).is_some());
    }

    #[test]
    fn prompt_template_render() {
        let tpl = PromptTemplate {
            role: AgentRole::Coder,
            name: "test".into(),
            template: "Task: {title}\nDesc: {description}".into(),
            source: PromptSource::BuiltIn,
        };
        let rendered = tpl.render_task("Fix bug", "Login crashes", "");
        assert!(rendered.contains("Fix bug"));
        assert!(rendered.contains("Login crashes"));
    }

    #[test]
    fn prompt_template_render_with_context() {
        let tpl = PromptTemplate {
            role: AgentRole::Planner,
            name: "test".into(),
            template: "{context}\n\nTask: {title}".into(),
            source: PromptSource::BuiltIn,
        };
        let rendered = tpl.render_task("Plan work", "", "Use Rust conventions");
        assert!(rendered.contains("Use Rust conventions"));
        assert!(rendered.contains("Plan work"));
    }

    #[test]
    fn prompt_registry_override() {
        let mut reg = PromptRegistry::new();
        let custom = PromptTemplate {
            role: AgentRole::Coder,
            name: "custom_coder".into(),
            template: "Custom: {title}".into(),
            source: PromptSource::File(PathBuf::from("custom.md")),
        };
        reg.set(custom);
        let tpl = reg.get(&AgentRole::Coder).unwrap();
        assert_eq!(tpl.name, "custom_coder");
    }

    #[test]
    fn role_from_prompt_name_mapping() {
        assert_eq!(role_from_prompt_name("coder"), Some(AgentRole::Coder));
        assert_eq!(
            role_from_prompt_name("qa_reviewer"),
            Some(AgentRole::QaReviewer)
        );
        assert_eq!(
            role_from_prompt_name("spec_gatherer"),
            Some(AgentRole::SpecGatherer)
        );
        assert_eq!(
            role_from_prompt_name("roadmap_features"),
            Some(AgentRole::RoadmapFeatures)
        );
        assert_eq!(role_from_prompt_name("nonexistent"), None);
    }

    #[test]
    fn prompt_registry_load_from_project() {
        let dir = tempfile::tempdir().unwrap();
        let prompts_dir = dir.path().join(".claude").join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        std::fs::write(
            prompts_dir.join("coder.md"),
            "Custom coder prompt for {title}",
        )
        .unwrap();

        let mut reg = PromptRegistry::new();
        reg.load_from_project(dir.path());

        let tpl = reg.get(&AgentRole::Coder).unwrap();
        assert!(matches!(tpl.source, PromptSource::File(_)));
        assert!(tpl.template.contains("Custom coder"));
    }

    #[test]
    fn prompt_registry_ignores_unknown_files() {
        let dir = tempfile::tempdir().unwrap();
        let prompts_dir = dir.path().join(".claude").join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        std::fs::write(prompts_dir.join("random.md"), "Some content").unwrap();

        let mut reg = PromptRegistry::new();
        let count_before = reg.count();
        reg.load_from_project(dir.path());
        assert_eq!(reg.count(), count_before);
    }

    #[test]
    fn built_in_templates_not_empty() {
        let templates = built_in_templates();
        assert!(templates.len() >= 15);
        for tpl in &templates {
            assert!(!tpl.template.is_empty());
            assert!(matches!(tpl.source, PromptSource::BuiltIn));
        }
    }

    #[test]
    fn prompt_template_serialization() {
        let tpl = PromptTemplate {
            role: AgentRole::Coder,
            name: "coder".into(),
            template: "test template".into(),
            source: PromptSource::BuiltIn,
        };
        let json = serde_json::to_string(&tpl).unwrap();
        let deser: PromptTemplate = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "coder");
    }

    #[test]
    fn prompt_registry_roles_list() {
        let reg = PromptRegistry::new();
        let roles = reg.roles();
        assert!(roles.contains(&&AgentRole::Coder));
        assert!(roles.contains(&&AgentRole::QaReviewer));
    }

    #[test]
    fn prompt_render_preserves_unknown_vars() {
        let tpl = PromptTemplate {
            role: AgentRole::Coder,
            name: "test".into(),
            template: "{title} and {unknown_var}".into(),
            source: PromptSource::BuiltIn,
        };
        let rendered = tpl.render_task("Hello", "", "");
        assert!(rendered.contains("Hello"));
        assert!(rendered.contains("{unknown_var}"));
    }
}
