use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level configuration loaded from `~/.auto-tundra/config.toml`.
///
/// **Security**: This struct NEVER stores API keys, tokens, or secrets.
/// All credentials are read from environment variables at runtime.
/// See [`CredentialProvider`] for the env-var-based credential model.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub dolt: DoltConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub bridge: BridgeConfig,
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub kanban: KanbanConfig,
    #[serde(default)]
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub integrations: IntegrationConfig,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub language: LanguageConfig,
    #[serde(default)]
    pub dev_tools: DevToolsConfig,
    #[serde(default)]
    pub agent_profile: AgentProfileConfig,
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub api_profiles: ApiProfilesConfig,
    #[serde(default)]
    pub updates: UpdatesConfig,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub debug: DebugConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("general", &self.general)
            .field("providers", &self.providers)
            .field("agents", &self.agents)
            .field("security", &self.security)
            .field("daemon", &self.daemon)
            .field("ui", &self.ui)
            .field("display", &self.display)
            .field("kanban", &self.kanban)
            .field("terminal", &self.terminal)
            .field("integrations", &self.integrations)
            .field("appearance", &self.appearance)
            .field("language", &self.language)
            .field("dev_tools", &self.dev_tools)
            .field("agent_profile", &self.agent_profile)
            .field("paths", &self.paths)
            .field("api_profiles", &self.api_profiles)
            .field("updates", &self.updates)
            .field("notifications", &self.notifications)
            .field("debug", &self.debug)
            .field("memory", &self.memory)
            .finish()
    }
}

impl Config {
    /// Load config from `~/.auto-tundra/config.toml`, falling back to
    /// defaults when the file does not exist.
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::default_path();
        if path.exists() {
            let text =
                std::fs::read_to_string(&path).map_err(|e| ConfigError::Io(e.to_string()))?;
            let cfg: Config =
                toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
            cfg.validate()?;
            Ok(cfg)
        } else {
            let cfg = Config::default();
            cfg.validate()?;
            Ok(cfg)
        }
    }

    /// Load from a specific path.
    pub fn load_from(path: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let path = path.into();
        let text = std::fs::read_to_string(&path).map_err(|e| ConfigError::Io(e.to_string()))?;
        let cfg: Config = toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Serialize config to TOML string.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        self.validate()?;
        toml::to_string_pretty(self).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Semantic validation for settings that are not fully expressible via type checks.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.kanban.validate()?;
        self.security.validate_profiles()?;
        Ok(())
    }

    fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".auto-tundra")
            .join("config.toml")
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("validation: {0}")]
    Validation(String),
}

// ---------------------------------------------------------------------------
// Section structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_project_name")]
    pub project_name: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub workspace_root: Option<String>,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            project_name: default_project_name(),
            log_level: default_log_level(),
            workspace_root: None,
        }
    }
}

fn default_project_name() -> String {
    "auto-tundra".into()
}
fn default_log_level() -> String {
    "info".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoltConfig {
    #[serde(default = "default_dolt_dir")]
    pub dir: String,
    #[serde(default = "default_dolt_port")]
    pub port: u16,
    #[serde(default)]
    pub auto_commit: bool,
}

impl Default for DoltConfig {
    fn default() -> Self {
        Self {
            dir: default_dolt_dir(),
            port: default_dolt_port(),
            auto_commit: false,
        }
    }
}

fn default_dolt_dir() -> String {
    "./dolt".into()
}
fn default_dolt_port() -> u16 {
    3306
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_path")]
    pub path: String,
    #[serde(default = "default_cache_max_mb")]
    pub max_size_mb: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            path: default_cache_path(),
            max_size_mb: default_cache_max_mb(),
        }
    }
}

fn default_cache_path() -> String {
    "~/.auto-tundra/cache.db".into()
}
fn default_cache_max_mb() -> u64 {
    256
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub anthropic_key_env: Option<String>,
    #[serde(default)]
    pub openai_key_env: Option<String>,
    #[serde(default)]
    pub google_key_env: Option<String>,
    /// Local inference server base URL (OpenAI-compatible, e.g. vllm.rs).
    #[serde(default = "default_local_base_url")]
    pub local_base_url: String,
    /// Default local model alias/id.
    #[serde(default = "default_local_model")]
    pub local_model: String,
    /// Optional API key env-var for local servers that require auth.
    #[serde(default = "default_local_api_key_env")]
    pub local_api_key_env: String,
    #[serde(default = "default_max_tokens")]
    pub default_max_tokens: u32,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            anthropic_key_env: None,
            openai_key_env: None,
            google_key_env: None,
            local_base_url: default_local_base_url(),
            local_model: default_local_model(),
            local_api_key_env: default_local_api_key_env(),
            default_max_tokens: default_max_tokens(),
        }
    }
}

fn default_local_base_url() -> String {
    "http://127.0.0.1:11434".into()
}

fn default_local_model() -> String {
    "qwen2.5-coder:14b".into()
}

fn default_local_api_key_env() -> String {
    "LOCAL_API_KEY".into()
}

fn default_max_tokens() -> u32 {
    16384
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    #[serde(default = "default_max_agents")]
    pub max_concurrent: u32,
    #[serde(default = "default_heartbeat")]
    pub heartbeat_interval_secs: u64,
    #[serde(default)]
    pub auto_restart: bool,
    /// When true, agents work in repo root instead of worktrees.
    #[serde(default)]
    pub direct_mode: bool,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_agents(),
            heartbeat_interval_secs: default_heartbeat(),
            auto_restart: false,
            direct_mode: false,
        }
    }
}

fn default_max_agents() -> u32 {
    8
}
fn default_heartbeat() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub allow_shell_exec: bool,
    #[serde(default)]
    pub sandbox: bool,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_auto_lock_timeout")]
    pub auto_lock_timeout_mins: u32,
    #[serde(default = "default_true")]
    pub sandbox_mode: bool,
    /// Active execution profile name in `execution_profiles`.
    #[serde(default = "default_execution_profile")]
    pub active_execution_profile: String,
    /// Sandbox/approval profile matrix used by CLI/agent executors.
    #[serde(default = "default_execution_profiles")]
    pub execution_profiles: Vec<ExecutionProfile>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allow_shell_exec: false,
            sandbox: true,
            allowed_paths: Vec::new(),
            allowed_origins: Vec::new(),
            auto_lock_timeout_mins: default_auto_lock_timeout(),
            sandbox_mode: true,
            active_execution_profile: default_execution_profile(),
            execution_profiles: default_execution_profiles(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    Never,
    OnFailure,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionProfile {
    pub name: String,
    #[serde(default = "default_true")]
    pub sandbox: bool,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default)]
    pub allow_shell_exec: bool,
    #[serde(default = "default_approval_mode")]
    pub approval_mode: ApprovalMode,
}

impl SecurityConfig {
    pub fn validate_profiles(&self) -> Result<(), ConfigError> {
        if self.execution_profiles.is_empty() {
            return Err(ConfigError::Validation(
                "security.execution_profiles must not be empty".to_string(),
            ));
        }
        let mut names = std::collections::BTreeSet::new();
        for profile in &self.execution_profiles {
            let name = profile.name.trim();
            if name.is_empty() {
                return Err(ConfigError::Validation(
                    "security.execution_profiles entries must have non-empty name".to_string(),
                ));
            }
            if !names.insert(name.to_string()) {
                return Err(ConfigError::Validation(format!(
                    "security.execution_profiles contains duplicate profile '{}'",
                    name
                )));
            }
        }
        if !names.contains(self.active_execution_profile.trim()) {
            return Err(ConfigError::Validation(format!(
                "security.active_execution_profile '{}' not found in execution_profiles",
                self.active_execution_profile
            )));
        }
        Ok(())
    }

    pub fn active_profile(&self) -> Option<&ExecutionProfile> {
        self.execution_profiles
            .iter()
            .find(|p| p.name == self.active_execution_profile)
    }
}

fn default_true() -> bool {
    true
}
fn default_auto_lock_timeout() -> u32 {
    15
}
fn default_execution_profile() -> String {
    "balanced".into()
}
fn default_approval_mode() -> ApprovalMode {
    ApprovalMode::OnFailure
}
fn default_execution_profiles() -> Vec<ExecutionProfile> {
    vec![
        ExecutionProfile {
            name: "safe".to_string(),
            sandbox: true,
            allow_network: false,
            allow_shell_exec: false,
            approval_mode: ApprovalMode::Always,
        },
        ExecutionProfile {
            name: "balanced".to_string(),
            sandbox: true,
            allow_network: true,
            allow_shell_exec: true,
            approval_mode: ApprovalMode::OnFailure,
        },
        ExecutionProfile {
            name: "trusted".to_string(),
            sandbox: false,
            allow_network: true,
            allow_shell_exec: true,
            approval_mode: ApprovalMode::Never,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_daemon_port")]
    pub port: u16,
    #[serde(default = "default_daemon_host")]
    pub host: String,
    #[serde(default)]
    pub tls: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            port: default_daemon_port(),
            host: default_daemon_host(),
            tls: false,
        }
    }
}

fn default_daemon_port() -> u16 {
    9876
}
fn default_daemon_host() -> String {
    "127.0.0.1".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_ui_theme")]
    pub theme: String,
    #[serde(default = "default_refresh_ms")]
    pub refresh_ms: u64,
    #[serde(default)]
    pub show_token_costs: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_ui_theme(),
            refresh_ms: default_refresh_ms(),
            show_token_costs: false,
        }
    }
}

fn default_ui_theme() -> String {
    "dark".into()
}
fn default_refresh_ms() -> u64 {
    500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    #[serde(default = "default_bridge_transport")]
    pub transport: String,
    #[serde(default = "default_bridge_socket")]
    pub socket_path: String,
    #[serde(default = "default_bridge_buffer")]
    pub buffer_size: usize,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            transport: default_bridge_transport(),
            socket_path: default_bridge_socket(),
            buffer_size: default_bridge_buffer(),
        }
    }
}

fn default_bridge_transport() -> String {
    "unix".into()
}
fn default_bridge_socket() -> String {
    "/tmp/auto-tundra.sock".into()
}
fn default_bridge_buffer() -> usize {
    8192
}

// ---------------------------------------------------------------------------
// Display settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    #[serde(default = "default_display_theme")]
    pub theme: String,
    #[serde(default = "default_display_font_size")]
    pub font_size: u8,
    #[serde(default)]
    pub compact_mode: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            theme: default_display_theme(),
            font_size: default_display_font_size(),
            compact_mode: false,
        }
    }
}

fn default_display_theme() -> String {
    "dark".into()
}
fn default_display_font_size() -> u8 {
    14
}

// ---------------------------------------------------------------------------
// Kanban settings (planning poker, columns)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanbanConfig {
    #[serde(default = "default_kanban_column_mode")]
    pub column_mode: String,
    #[serde(default)]
    pub planning_poker: PlanningPokerConfig,
}

impl Default for KanbanConfig {
    fn default() -> Self {
        Self {
            column_mode: default_kanban_column_mode(),
            planning_poker: PlanningPokerConfig::default(),
        }
    }
}

impl KanbanConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mode = self.column_mode.trim();
        if mode.is_empty() {
            return Err(ConfigError::Validation(
                "kanban.column_mode must not be empty".to_string(),
            ));
        }
        self.planning_poker.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningPokerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_poker_default_deck")]
    pub default_deck: String,
    #[serde(default = "default_true")]
    pub allow_custom_deck: bool,
    #[serde(default)]
    pub reveal_requires_all_votes: bool,
    #[serde(default = "default_poker_round_duration_seconds")]
    pub round_duration_seconds: u64,
}

impl Default for PlanningPokerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_deck: default_poker_default_deck(),
            allow_custom_deck: true,
            reveal_requires_all_votes: false,
            round_duration_seconds: default_poker_round_duration_seconds(),
        }
    }
}

impl PlanningPokerConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let allowed = ["fibonacci", "modified_fibonacci", "powers_of_two", "tshirt"];
        if !allowed.contains(&self.default_deck.as_str()) {
            return Err(ConfigError::Validation(format!(
                "kanban.planning_poker.default_deck '{}' is not supported",
                self.default_deck
            )));
        }
        if self.round_duration_seconds == 0 || self.round_duration_seconds > 86_400 {
            return Err(ConfigError::Validation(
                "kanban.planning_poker.round_duration_seconds must be between 1 and 86400"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

fn default_kanban_column_mode() -> String {
    "classic_8".into()
}
fn default_poker_default_deck() -> String {
    "fibonacci".into()
}
fn default_poker_round_duration_seconds() -> u64 {
    300
}

// ---------------------------------------------------------------------------
// Terminal settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    #[serde(default = "default_term_font_family")]
    pub font_family: String,
    #[serde(default = "default_term_font_size")]
    pub font_size: u8,
    #[serde(default = "default_cursor_style")]
    pub cursor_style: String,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            font_family: default_term_font_family(),
            font_size: default_term_font_size(),
            cursor_style: default_cursor_style(),
        }
    }
}

fn default_term_font_family() -> String {
    "JetBrains Mono".into()
}
fn default_term_font_size() -> u8 {
    14
}
fn default_cursor_style() -> String {
    "block".into()
}

// ---------------------------------------------------------------------------
// Integration settings (UI-facing)
// ---------------------------------------------------------------------------

/// Integration settings — references env var names, NEVER stores actual tokens.
///
/// All credentials are resolved at runtime via [`CredentialProvider`].
/// Config only stores the *name* of the env var to read, e.g. `"GITHUB_TOKEN"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationConfig {
    /// Env var name for GitHub personal access token (default: `GITHUB_TOKEN`).
    #[serde(default = "default_github_env")]
    pub github_token_env: String,
    /// GitHub repository owner (org or user).
    #[serde(default)]
    pub github_owner: Option<String>,
    /// GitHub repository name.
    #[serde(default)]
    pub github_repo: Option<String>,
    /// Env var name for GitLab token (default: `GITLAB_TOKEN`).
    #[serde(default = "default_gitlab_env")]
    pub gitlab_token_env: String,
    /// GitLab project ID (numeric or `group/project` path).
    #[serde(default)]
    pub gitlab_project_id: Option<String>,
    /// GitLab instance URL for self-hosted (default: `https://gitlab.com`).
    #[serde(default)]
    pub gitlab_url: Option<String>,
    /// Env var name for Linear API key (default: `LINEAR_API_KEY`).
    #[serde(default = "default_linear_env")]
    pub linear_api_key_env: String,
    /// Linear team ID to scope issues.
    #[serde(default)]
    pub linear_team_id: Option<String>,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            github_token_env: default_github_env(),
            github_owner: None,
            github_repo: None,
            gitlab_token_env: default_gitlab_env(),
            gitlab_project_id: None,
            gitlab_url: None,
            linear_api_key_env: default_linear_env(),
            linear_team_id: None,
        }
    }
}

fn default_github_env() -> String {
    "GITHUB_TOKEN".into()
}
fn default_gitlab_env() -> String {
    "GITLAB_TOKEN".into()
}
fn default_linear_env() -> String {
    "LINEAR_API_KEY".into()
}

// ---------------------------------------------------------------------------
// Appearance settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default = "default_appearance_mode")]
    pub appearance_mode: String,
    #[serde(default = "default_color_theme")]
    pub color_theme: String,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            appearance_mode: default_appearance_mode(),
            color_theme: default_color_theme(),
        }
    }
}

fn default_appearance_mode() -> String {
    "system".into()
}
fn default_color_theme() -> String {
    "arctic".into()
}

// ---------------------------------------------------------------------------
// Language settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageConfig {
    #[serde(default = "default_interface_language")]
    pub interface_language: String,
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self {
            interface_language: default_interface_language(),
        }
    }
}

fn default_interface_language() -> String {
    "en".into()
}

// ---------------------------------------------------------------------------
// Dev tools settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevToolsConfig {
    #[serde(default = "default_preferred_ide")]
    pub preferred_ide: String,
    #[serde(default = "default_preferred_terminal")]
    pub preferred_terminal: String,
    #[serde(default)]
    pub auto_name_terminals: bool,
    #[serde(default)]
    pub yolo_mode: bool,
    #[serde(default = "default_terminal_font_family")]
    pub terminal_font_family: String,
    #[serde(default = "default_terminal_font_size")]
    pub terminal_font_size: u16,
    #[serde(default = "default_terminal_cursor_style")]
    pub terminal_cursor_style: String,
    #[serde(default = "default_terminal_cursor_blink")]
    pub terminal_cursor_blink: bool,
    #[serde(default = "default_terminal_scrollback_lines")]
    pub terminal_scrollback_lines: u32,
}

impl Default for DevToolsConfig {
    fn default() -> Self {
        Self {
            preferred_ide: default_preferred_ide(),
            preferred_terminal: default_preferred_terminal(),
            auto_name_terminals: false,
            yolo_mode: false,
            terminal_font_family: default_terminal_font_family(),
            terminal_font_size: default_terminal_font_size(),
            terminal_cursor_style: default_terminal_cursor_style(),
            terminal_cursor_blink: default_terminal_cursor_blink(),
            terminal_scrollback_lines: default_terminal_scrollback_lines(),
        }
    }
}

fn default_preferred_ide() -> String {
    "vscode".into()
}
fn default_preferred_terminal() -> String {
    "default".into()
}
fn default_terminal_font_family() -> String {
    "JetBrains Mono, monospace".into()
}
fn default_terminal_font_size() -> u16 {
    14
}
fn default_terminal_cursor_style() -> String {
    "block".into()
}
fn default_terminal_cursor_blink() -> bool {
    true
}
fn default_terminal_scrollback_lines() -> u32 {
    5000
}

// ---------------------------------------------------------------------------
// Agent profile settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPhaseConfig {
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub thinking_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfileConfig {
    #[serde(default = "default_agent_profile")]
    pub default_profile: String,
    #[serde(default = "default_agent_framework")]
    pub agent_framework: String,
    #[serde(default)]
    pub ai_terminal_naming: bool,
    #[serde(default)]
    pub phase_configs: Vec<AgentPhaseConfig>,
}

impl Default for AgentProfileConfig {
    fn default() -> Self {
        Self {
            default_profile: default_agent_profile(),
            agent_framework: default_agent_framework(),
            ai_terminal_naming: false,
            phase_configs: Vec::new(),
        }
    }
}

fn default_agent_profile() -> String {
    "default".into()
}
fn default_agent_framework() -> String {
    "auto-tundra".into()
}

// ---------------------------------------------------------------------------
// Paths settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathsConfig {
    #[serde(default)]
    pub python_path: String,
    #[serde(default)]
    pub git_path: String,
    #[serde(default)]
    pub github_cli_path: String,
    #[serde(default)]
    pub claude_cli_path: String,
    #[serde(default)]
    pub auto_claude_path: String,
}

// ---------------------------------------------------------------------------
// API profiles settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiProfileEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiProfilesConfig {
    #[serde(default)]
    pub profiles: Vec<ApiProfileEntry>,
}

// ---------------------------------------------------------------------------
// Updates settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdatesConfig {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub is_latest: bool,
    #[serde(default)]
    pub auto_update_projects: bool,
    #[serde(default)]
    pub beta_updates: bool,
}

// ---------------------------------------------------------------------------
// Notification settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_true")]
    pub on_task_complete: bool,
    #[serde(default = "default_true")]
    pub on_task_failed: bool,
    #[serde(default = "default_true")]
    pub on_review_needed: bool,
    #[serde(default = "default_true")]
    pub sound_enabled: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            on_task_complete: true,
            on_task_failed: true,
            on_review_needed: true,
            sound_enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Debug settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DebugConfig {
    #[serde(default)]
    pub anonymous_error_reporting: bool,
}

// ---------------------------------------------------------------------------
// Memory settings (UI-facing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    #[serde(default)]
    pub enable_memory: bool,
    #[serde(default)]
    pub enable_agent_memory_access: bool,
    #[serde(default)]
    pub graphiti_server_url: String,
    #[serde(default)]
    pub embedding_provider: String,
    #[serde(default)]
    pub embedding_model: String,
}

// ---------------------------------------------------------------------------
// Credential provider — reads secrets from environment at runtime
// ---------------------------------------------------------------------------

/// Reads credentials from environment variables at runtime.
/// Never stores secrets in memory longer than needed.
///
/// Follows the opencode pattern: config stores env var *names*,
/// this provider resolves them to values on demand.
pub struct CredentialProvider;

impl CredentialProvider {
    /// Read the daemon API key from the `AUTO_TUNDRA_API_KEY` env var.
    /// Returns `None` in dev mode (var not set).
    pub fn daemon_api_key() -> Option<String> {
        std::env::var("AUTO_TUNDRA_API_KEY").ok()
    }

    /// Ensure a daemon API key is available, auto-generating one if needed.
    /// Returns a valid API key (never None).
    ///
    /// Behavior:
    /// 1. If `AUTO_TUNDRA_API_KEY` env var is set, returns it
    /// 2. Otherwise, reads or generates `~/.auto-tundra/daemon.key`
    /// 3. Auto-generated keys are stored with 0o600 permissions (owner read/write only)
    pub fn ensure_daemon_api_key() -> String {
        // Check env var first (takes precedence)
        if let Ok(key) = std::env::var("AUTO_TUNDRA_API_KEY") {
            return key;
        }

        // Otherwise, generate or read from file
        Self::generate_and_store_api_key()
    }

    /// Generate and store a new API key, or read existing one from disk.
    /// Creates `~/.auto-tundra/daemon.key` with 0o600 permissions if it doesn't exist.
    fn generate_and_store_api_key() -> String {
        let key_path = Self::daemon_key_path();

        // Read existing key if it exists
        if key_path.exists() {
            if let Ok(key) = std::fs::read_to_string(&key_path) {
                return key.trim().to_string();
            }
        }

        // Generate new key
        let new_key = uuid::Uuid::new_v4().to_string();

        // Ensure parent directory exists
        if let Some(parent) = key_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Write key to file
        if let Err(e) = std::fs::write(&key_path, &new_key) {
            eprintln!(
                "Warning: failed to write daemon key to {:?}: {}",
                key_path, e
            );
            return new_key;
        }

        // Set file permissions to 0o600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&key_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(&key_path, perms);
            }
        }

        new_key
    }

    /// Get the path to the daemon key file.
    fn daemon_key_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".auto-tundra")
            .join("daemon.key")
    }

    /// Read the Anthropic API key from the `ANTHROPIC_API_KEY` env var.
    pub fn anthropic_api_key() -> Option<String> {
        std::env::var("ANTHROPIC_API_KEY").ok()
    }

    /// Read the OpenAI API key from the `OPENAI_API_KEY` env var.
    pub fn openai_api_key() -> Option<String> {
        std::env::var("OPENAI_API_KEY").ok()
    }

    /// Read a credential from a named env var.
    pub fn from_env(var_name: &str) -> Option<String> {
        std::env::var(var_name).ok()
    }

    /// Check which providers have credentials available.
    pub fn available_providers() -> Vec<&'static str> {
        let mut providers = Vec::new();
        if Self::anthropic_api_key().is_some() {
            providers.push("anthropic");
        }
        if Self::openai_api_key().is_some() {
            providers.push("openai");
        }
        if Self::from_env("GITHUB_TOKEN").is_some() {
            providers.push("github");
        }
        if Self::from_env("GITLAB_TOKEN").is_some() {
            providers.push("gitlab");
        }
        if Self::from_env("LINEAR_API_KEY").is_some() {
            providers.push("linear");
        }
        providers
    }
}
