use std::collections::HashMap;

use at_core::types::{CliType, TaskPhase};
use serde::{Deserialize, Serialize};

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
    /// Return the budget-tokens value for Claude's `--thinking-budget` flag.
    /// Returns `None` for `ThinkingLevel::None` (thinking disabled).
    pub fn budget_tokens(&self) -> Option<u32> {
        match self {
            ThinkingLevel::None => None,
            ThinkingLevel::Low => Some(5_000),
            ThinkingLevel::Medium => Some(10_000),
            ThinkingLevel::High => Some(50_000),
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
            TaskPhase::Discovery | TaskPhase::ContextGathering => (
                default_model_for(&cli_type),
                ThinkingLevel::Low,
                8_000,
                120,
            ),
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
            TaskPhase::Merging => (
                default_model_for(&cli_type),
                ThinkingLevel::Low,
                8_000,
                120,
            ),
            // Terminal states - use minimal defaults
            TaskPhase::Complete | TaskPhase::Error | TaskPhase::Stopped => (
                default_model_for(&cli_type),
                ThinkingLevel::None,
                4_000,
                60,
            ),
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
    /// - Claude: `claude --model {model} --print [--thinking-budget N]`
    /// - Codex: `codex --model {model}`
    /// - Gemini: `gemini --model {model}`
    /// - OpenCode: `opencode --model {model}`
    pub fn to_cli_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        match self.cli_type {
            CliType::Claude => {
                args.push("--model".to_string());
                args.push(self.model.clone());
                args.push("--print".to_string());
                if let Some(budget) = self.thinking_level.budget_tokens() {
                    args.push("--thinking-budget".to_string());
                    args.push(budget.to_string());
                }
                args.push("--max-turns".to_string());
                args.push("50".to_string());
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
        assert!(args.contains(&"--thinking-budget".to_string()));
        assert!(args.contains(&"50000".to_string()));
        assert!(args.contains(&"--max-turns".to_string()));
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
