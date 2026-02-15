use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level configuration loaded from `~/.auto-tundra/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            dolt: DoltConfig::default(),
            cache: CacheConfig::default(),
            providers: ProvidersConfig::default(),
            agents: AgentsConfig::default(),
            security: SecurityConfig::default(),
            daemon: DaemonConfig::default(),
            ui: UiConfig::default(),
            bridge: BridgeConfig::default(),
        }
    }
}

impl Config {
    /// Load config from `~/.auto-tundra/config.toml`, falling back to
    /// defaults when the file does not exist.
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::default_path();
        if path.exists() {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| ConfigError::Io(e.to_string()))?;
            let cfg: Config =
                toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
            Ok(cfg)
        } else {
            Ok(Config::default())
        }
    }

    /// Load from a specific path.
    pub fn load_from(path: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let path = path.into();
        let text =
            std::fs::read_to_string(&path).map_err(|e| ConfigError::Io(e.to_string()))?;
        let cfg: Config =
            toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        Ok(cfg)
    }

    /// Serialize config to TOML string.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(|e| ConfigError::Parse(e.to_string()))
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
    #[serde(default = "default_max_tokens")]
    pub default_max_tokens: u32,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            anthropic_key_env: None,
            openai_key_env: None,
            google_key_env: None,
            default_max_tokens: default_max_tokens(),
        }
    }
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
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_agents(),
            heartbeat_interval_secs: default_heartbeat(),
            auto_restart: false,
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
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allow_shell_exec: false,
            sandbox: true,
            allowed_paths: Vec::new(),
        }
    }
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
