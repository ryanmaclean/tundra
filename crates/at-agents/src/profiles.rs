use std::collections::HashMap;

use at_core::types::{CliType, TaskPhase};
use serde::{Deserialize, Serialize};

use crate::roles::RoleConfig;

// ---------------------------------------------------------------------------
// ThinkingLevel
// ---------------------------------------------------------------------------

/// Controls the amount of "thinking" (chain-of-thought) the agent uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    None,
    Low,
    Medium,
    High,
}

impl ThinkingLevel {
    /// Return a nominal thinking-token budget for this level.
    /// Returns `None` for `ThinkingLevel::None` (thinking disabled).
    ///
    /// Note: the Claude CLI has no `--thinking-budget` flag (it is rejected
    /// with "unknown option"); use [`effort`](ThinkingLevel::effort) for CLI
    /// args instead.
    pub fn budget_tokens(&self) -> Option<u32> {
        match self {
            ThinkingLevel::None => None,
            ThinkingLevel::Low => Some(5_000),
            ThinkingLevel::Medium => Some(10_000),
            ThinkingLevel::High => Some(50_000),
        }
    }

    /// Value for the Claude CLI `--effort` flag (`low`/`medium`/`high`).
    /// Returns `None` for `ThinkingLevel::None` (flag omitted, CLI default).
    pub fn effort(&self) -> Option<&'static str> {
        match self {
            ThinkingLevel::None => None,
            ThinkingLevel::Low => Some("low"),
            ThinkingLevel::Medium => Some("medium"),
            ThinkingLevel::High => Some("high"),
        }
    }
}

// ---------------------------------------------------------------------------
// AgentConfig
// ---------------------------------------------------------------------------

/// Configuration for spawning a CLI agent process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Which CLI tool to invoke.
    pub cli_type: CliType,
    /// The model identifier (e.g. "claude-sonnet-4-20250514").
    pub model: String,
    /// How much thinking/chain-of-thought to request.
    pub thinking_level: ThinkingLevel,
    /// Maximum output tokens.
    pub max_tokens: u32,
    /// Per-task timeout in seconds.
    pub timeout_secs: u64,
    /// Extra environment variables to set for the spawned process.
    pub env_vars: HashMap<String, String>,
}

impl AgentConfig {
    /// Build a sensible default configuration for a given CLI type and task phase.
    ///
    /// Different phases benefit from different models and thinking levels:
    /// - Discovery/ContextGathering: lighter model, low thinking
    /// - SpecCreation/Planning: heavier thinking
    /// - Coding: high thinking, generous timeout
    /// - QA/Fixing: medium thinking
    /// - Merging: low thinking, shorter timeout
    pub fn default_for_phase(cli_type: CliType, phase: TaskPhase) -> Self {
        let (model, thinking, max_tokens, timeout) = match phase {
            TaskPhase::Discovery | TaskPhase::ContextGathering => {
                (default_model_for(&cli_type), ThinkingLevel::Low, 8_000, 120)
            }
            TaskPhase::SpecCreation | TaskPhase::Planning => (
                default_model_for(&cli_type),
                ThinkingLevel::Medium,
                16_000,
                300,
            ),
            TaskPhase::Coding => (
                default_model_for(&cli_type),
                ThinkingLevel::High,
                32_000,
                600,
            ),
            TaskPhase::Qa | TaskPhase::Fixing => (
                default_model_for(&cli_type),
                ThinkingLevel::Medium,
                16_000,
                300,
            ),
            TaskPhase::Merging => (default_model_for(&cli_type), ThinkingLevel::Low, 8_000, 120),
            // Terminal states - use minimal defaults
            TaskPhase::Complete | TaskPhase::Error | TaskPhase::Stopped => {
                (default_model_for(&cli_type), ThinkingLevel::None, 4_000, 60)
            }
        };

        Self {
            cli_type,
            model,
            thinking_level: thinking,
            max_tokens,
            timeout_secs: timeout,
            env_vars: HashMap::new(),
        }
    }

    /// Generate the CLI arguments list for spawning the agent process.
    ///
    /// Each CLI type has its own flag conventions:
    /// - Claude: `claude --model {model} --print [--effort low|medium|high] --max-turns 50`
    ///   (the prompt itself is appended by the executor, see
    ///   [`prompt_in_args`](AgentConfig::prompt_in_args))
    /// - Codex: `codex --model {model}`
    /// - Gemini: `gemini --model {model}`
    /// - OpenCode: `opencode --model {model}`
    pub fn to_cli_args(&self) -> Vec<String> {
        self.cli_args_with_turns(DEFAULT_MAX_TURNS)
    }

    /// Generate CLI arguments constrained by a role's configuration.
    ///
    /// On top of [`to_cli_args`](AgentConfig::to_cli_args), for Claude:
    /// - `--max-turns` is the role's [`max_turns`](RoleConfig::max_turns)
    ///   instead of the default 50;
    /// - the role's [`preferred_model`](RoleConfig::preferred_model) replaces
    ///   the configured model (`"inherit"` keeps it);
    /// - `--allowedTools` lists the role's allowed tools, mapped to Claude
    ///   tool names by [`claude_tool_names`], minus anything in `denied_tools`;
    /// - `--disallowedTools` lists `denied_tools` (tools whose approval policy
    ///   is `Deny` for the role).
    ///
    /// Other CLIs have no equivalent flags; they get
    /// [`to_cli_args`](AgentConfig::to_cli_args)
    /// unchanged. Tool lists are emitted one argv element per tool, so the
    /// executor's trailing `--` ends the variadic flag before the prompt.
    pub fn to_cli_args_for_role(
        &self,
        role: &dyn RoleConfig,
        denied_tools: &[String],
    ) -> Vec<String> {
        if self.cli_type != CliType::Claude {
            return self.to_cli_args();
        }

        let mut config = self.clone();
        if let Some(model) = role.preferred_model().filter(|m| *m != "inherit") {
            config.model = model.to_string();
        }
        let mut args = config.cli_args_with_turns(role.max_turns());

        let denied: Vec<String> = denied_tools
            .iter()
            .flat_map(|t| claude_tool_names(t))
            .fold(Vec::new(), push_unique);
        let allowed: Vec<String> = role
            .allowed_tools()
            .iter()
            .filter(|t| !denied_tools.contains(t))
            .flat_map(|t| claude_tool_names(t))
            .filter(|t| !denied.contains(t))
            .fold(Vec::new(), push_unique);

        if !allowed.is_empty() {
            args.push("--allowedTools".to_string());
            args.extend(allowed);
        }
        if !denied.is_empty() {
            args.push("--disallowedTools".to_string());
            args.extend(denied);
        }
        args
    }

    fn cli_args_with_turns(&self, max_turns: u32) -> Vec<String> {
        let mut args = Vec::new();

        match self.cli_type {
            CliType::Claude => {
                args.push("--model".to_string());
                args.push(self.model.clone());
                args.push("--print".to_string());
                if let Some(effort) = self.thinking_level.effort() {
                    args.push("--effort".to_string());
                    args.push(effort.to_string());
                }
                args.push("--max-turns".to_string());
                args.push(max_turns.to_string());
            }
            CliType::Codex => {
                args.push("--model".to_string());
                args.push(self.model.clone());
            }
            CliType::Gemini => {
                args.push("--model".to_string());
                args.push(self.model.clone());
            }
            CliType::OpenCode => {
                args.push("--model".to_string());
                args.push(self.model.clone());
            }
        }

        args
    }

    /// Whether the prompt must be passed as a trailing positional argument
    /// rather than written to stdin.
    ///
    /// `claude --print` does not read its prompt from a TTY stdin (the
    /// executor runs agents in a PTY), so Claude takes it on the command line.
    pub fn prompt_in_args(&self) -> bool {
        matches!(self.cli_type, CliType::Claude)
    }

    /// Return the binary name for this config's CLI type.
    pub fn binary_name(&self) -> &'static str {
        match self.cli_type {
            CliType::Claude => "claude",
            CliType::Codex => "codex",
            CliType::Gemini => "gemini",
            CliType::OpenCode => "opencode",
        }
    }
}

/// `--max-turns` used when no role config supplies one.
pub const DEFAULT_MAX_TURNS: u32 = 50;

fn push_unique(mut acc: Vec<String>, item: String) -> Vec<String> {
    if !acc.contains(&item) {
        acc.push(item);
    }
    acc
}

/// Map an internal tool name (the vocabulary of [`RoleConfig::allowed_tools`]
/// and `ToolApprovalSystem`) to Claude Code tool specifiers.
///
/// Names that already look like Claude tools (start with an uppercase letter,
/// e.g. `Read` or `Bash(git diff:*)` from a plugin agent's front-matter) pass
/// through unchanged. Internal orchestration tools with no CLI equivalent
/// (`task_assign`, `agent_spawn`, ...) and unknown names map to nothing.
pub fn claude_tool_names(tool: &str) -> Vec<String> {
    let mapped: &[&str] = match tool {
        "file_read" => &["Read"],
        "list_directory" => &["Glob"],
        "search_files" => &["Grep", "Glob"],
        "file_write" => &["Edit", "Write"],
        "shell_execute" => &["Bash"],
        "git_diff" => &["Bash(git diff:*)"],
        "git_log" => &["Bash(git log:*)"],
        "git_blame" => &["Bash(git blame:*)"],
        "git_add" => &["Bash(git add:*)"],
        "git_commit" => &["Bash(git commit:*)"],
        "git_push" => &["Bash(git push:*)"],
        "force_push" => &["Bash(git push --force:*)", "Bash(git push -f:*)"],
        "delete" | "file_delete" => &["Bash(rm:*)"],
        other if other.starts_with(|c: char| c.is_ascii_uppercase()) => {
            return vec![other.to_string()];
        }
        _ => &[],
    };
    mapped.iter().map(|s| s.to_string()).collect()
}

/// Return the default model string for a given CLI type.
fn default_model_for(cli_type: &CliType) -> String {
    match cli_type {
        CliType::Claude => "claude-sonnet-4-20250514".to_string(),
        CliType::Codex => "o3-mini".to_string(),
        CliType::Gemini => "gemini-2.5-pro".to_string(),
        CliType::OpenCode => "claude-sonnet-4-20250514".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_level_budget_none_returns_none() {
        assert_eq!(ThinkingLevel::None.budget_tokens(), None);
    }

    #[test]
    fn thinking_level_budget_values() {
        assert_eq!(ThinkingLevel::Low.budget_tokens(), Some(5_000));
        assert_eq!(ThinkingLevel::Medium.budget_tokens(), Some(10_000));
        assert_eq!(ThinkingLevel::High.budget_tokens(), Some(50_000));
    }

    #[test]
    fn claude_cli_args_with_thinking() {
        let config = AgentConfig {
            cli_type: CliType::Claude,
            model: "claude-sonnet-4-20250514".to_string(),
            thinking_level: ThinkingLevel::High,
            max_tokens: 16_000,
            timeout_secs: 300,
            env_vars: HashMap::new(),
        };
        let args = config.to_cli_args();
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"claude-sonnet-4-20250514".to_string()));
        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--effort".to_string()));
        assert!(args.contains(&"high".to_string()));
        assert!(args.contains(&"--max-turns".to_string()));
        assert!(!args.contains(&"--thinking-budget".to_string()));
    }

    #[test]
    fn claude_cli_args_without_thinking() {
        let config = AgentConfig {
            cli_type: CliType::Claude,
            model: "claude-sonnet-4-20250514".to_string(),
            thinking_level: ThinkingLevel::None,
            max_tokens: 8_000,
            timeout_secs: 120,
            env_vars: HashMap::new(),
        };
        let args = config.to_cli_args();
        assert!(args.contains(&"--print".to_string()));
        assert!(!args.contains(&"--thinking-budget".to_string()));
        assert!(!args.contains(&"--effort".to_string()));
    }

    /// Flags the installed `claude` CLI (2.1.x) accepts, checked against
    /// `claude --help` / `printf '' | claude -p <flag>` (`--max-turns` is a
    /// hidden but accepted option). `--thinking-budget` is rejected with
    /// "unknown option", so it must never be generated.
    const CLAUDE_ACCEPTED_FLAGS: &[&str] = &[
        "--model",
        "--print",
        "--effort",
        "--max-turns",
        "--allowedTools",
        "--disallowedTools",
    ];

    #[test]
    fn claude_default_args_only_use_supported_flags_for_every_phase() {
        for phase in [
            TaskPhase::Discovery,
            TaskPhase::ContextGathering,
            TaskPhase::SpecCreation,
            TaskPhase::Planning,
            TaskPhase::Coding,
            TaskPhase::Qa,
            TaskPhase::Fixing,
            TaskPhase::Merging,
            TaskPhase::Complete,
        ] {
            let config = AgentConfig::default_for_phase(CliType::Claude, phase.clone());
            for arg in config.to_cli_args().iter().filter(|a| a.starts_with("--")) {
                assert!(
                    CLAUDE_ACCEPTED_FLAGS.contains(&arg.as_str()),
                    "phase {phase:?} generated unsupported claude flag {arg}"
                );
            }
            if let Some(effort) = config.thinking_level.effort() {
                assert!(["low", "medium", "high"].contains(&effort));
            }
        }
    }

    struct TestRole {
        tools: Vec<String>,
        model: Option<&'static str>,
    }

    impl RoleConfig for TestRole {
        fn system_prompt(&self) -> &str {
            ""
        }
        fn allowed_tools(&self) -> Vec<String> {
            self.tools.clone()
        }
        fn max_turns(&self) -> u32 {
            7
        }
        fn preferred_model(&self) -> Option<&str> {
            self.model
        }
    }

    fn flag_values(args: &[String], flag: &str) -> Vec<String> {
        args.iter()
            .skip_while(|a| a.as_str() != flag)
            .skip(1)
            .take_while(|a| !a.starts_with("--"))
            .cloned()
            .collect()
    }

    #[test]
    fn claude_role_args_apply_turns_model_and_tool_lists() {
        let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
        let role = TestRole {
            tools: vec![
                "file_read".into(),
                "shell_execute".into(),
                "git_diff".into(),
                "task_status".into(),
                "file_delete".into(),
            ],
            model: Some("claude-opus-4-20250514"),
        };
        let denied = vec!["file_delete".to_string(), "force_push".to_string()];
        let args = config.to_cli_args_for_role(&role, &denied);

        assert_eq!(flag_values(&args, "--max-turns"), vec!["7"]);
        assert_eq!(
            flag_values(&args, "--model"),
            vec!["claude-opus-4-20250514"]
        );
        assert_eq!(
            flag_values(&args, "--allowedTools"),
            vec!["Read", "Bash", "Bash(git diff:*)"]
        );
        assert_eq!(
            flag_values(&args, "--disallowedTools"),
            vec![
                "Bash(rm:*)",
                "Bash(git push --force:*)",
                "Bash(git push -f:*)"
            ]
        );
        for arg in args.iter().filter(|a| a.starts_with("--")) {
            assert!(CLAUDE_ACCEPTED_FLAGS.contains(&arg.as_str()), "{arg}");
        }
    }

    #[test]
    fn claude_role_args_pass_through_claude_tool_names_and_inherit_model() {
        let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
        let role = TestRole {
            tools: vec!["Read".into(), "Grep".into()],
            model: Some("inherit"),
        };
        let args = config.to_cli_args_for_role(&role, &[]);
        assert_eq!(flag_values(&args, "--model"), vec![config.model.clone()]);
        assert_eq!(flag_values(&args, "--allowedTools"), vec!["Read", "Grep"]);
        assert!(!args.contains(&"--disallowedTools".to_string()));
    }

    #[test]
    fn non_claude_role_args_are_unchanged() {
        let config = AgentConfig::default_for_phase(CliType::Codex, TaskPhase::Coding);
        let role = TestRole {
            tools: vec!["file_read".into()],
            model: Some("claude-opus-4-20250514"),
        };
        assert_eq!(
            config.to_cli_args_for_role(&role, &[]),
            config.to_cli_args()
        );
    }

    #[test]
    fn claude_takes_prompt_as_argument() {
        let claude = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
        assert!(claude.prompt_in_args());
        let codex = AgentConfig::default_for_phase(CliType::Codex, TaskPhase::Coding);
        assert!(!codex.prompt_in_args());
    }

    #[test]
    fn codex_cli_args() {
        let config = AgentConfig {
            cli_type: CliType::Codex,
            model: "o3-mini".to_string(),
            thinking_level: ThinkingLevel::Medium,
            max_tokens: 16_000,
            timeout_secs: 300,
            env_vars: HashMap::new(),
        };
        let args = config.to_cli_args();
        assert_eq!(args, vec!["--model", "o3-mini"]);
    }

    #[test]
    fn gemini_cli_args() {
        let config = AgentConfig {
            cli_type: CliType::Gemini,
            model: "gemini-2.5-pro".to_string(),
            thinking_level: ThinkingLevel::Low,
            max_tokens: 8_000,
            timeout_secs: 120,
            env_vars: HashMap::new(),
        };
        let args = config.to_cli_args();
        assert_eq!(args, vec!["--model", "gemini-2.5-pro"]);
    }

    #[test]
    fn opencode_cli_args() {
        let config = AgentConfig {
            cli_type: CliType::OpenCode,
            model: "claude-sonnet-4-20250514".to_string(),
            thinking_level: ThinkingLevel::High,
            max_tokens: 32_000,
            timeout_secs: 600,
            env_vars: HashMap::new(),
        };
        let args = config.to_cli_args();
        assert_eq!(args, vec!["--model", "claude-sonnet-4-20250514"]);
    }

    #[test]
    fn default_for_coding_phase_has_high_thinking() {
        let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
        assert_eq!(config.thinking_level, ThinkingLevel::High);
        assert_eq!(config.timeout_secs, 600);
        assert_eq!(config.max_tokens, 32_000);
    }

    #[test]
    fn default_for_discovery_phase_has_low_thinking() {
        let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Discovery);
        assert_eq!(config.thinking_level, ThinkingLevel::Low);
        assert_eq!(config.timeout_secs, 120);
    }

    #[test]
    fn default_for_phase_uses_correct_model() {
        let claude = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
        assert!(claude.model.contains("claude"));

        let codex = AgentConfig::default_for_phase(CliType::Codex, TaskPhase::Coding);
        assert!(codex.model.contains("o3"));

        let gemini = AgentConfig::default_for_phase(CliType::Gemini, TaskPhase::Coding);
        assert!(gemini.model.contains("gemini"));
    }

    #[test]
    fn binary_name_matches_cli_type() {
        let config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
        assert_eq!(config.binary_name(), "claude");

        let config = AgentConfig::default_for_phase(CliType::Codex, TaskPhase::Coding);
        assert_eq!(config.binary_name(), "codex");

        let config = AgentConfig::default_for_phase(CliType::Gemini, TaskPhase::Coding);
        assert_eq!(config.binary_name(), "gemini");

        let config = AgentConfig::default_for_phase(CliType::OpenCode, TaskPhase::Coding);
        assert_eq!(config.binary_name(), "opencode");
    }

    #[test]
    fn agent_config_serialization_roundtrip() {
        let config = AgentConfig {
            cli_type: CliType::Claude,
            model: "test-model".to_string(),
            thinking_level: ThinkingLevel::Medium,
            max_tokens: 10_000,
            timeout_secs: 200,
            env_vars: HashMap::from([("FOO".to_string(), "bar".to_string())]),
        };
        let json = serde_json::to_string(&config).expect("serialize");
        let back: AgentConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.model, "test-model");
        assert_eq!(back.thinking_level, ThinkingLevel::Medium);
        assert_eq!(back.max_tokens, 10_000);
        assert_eq!(back.env_vars.get("FOO").unwrap(), "bar");
    }
}
