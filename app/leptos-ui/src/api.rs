use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

/// Default API base when not running in Tauri (standalone web dev).
// Use IPv4 loopback by default to avoid localhost IPv6 resolution mismatches in browsers.
const DEFAULT_API_BASE: &str = "http://127.0.0.1:9090";

/// Get the API base URL at runtime. In Tauri, reads `window.__TUNDRA_API_PORT__` (injected by
/// the desktop app). In standalone web mode, uses DEFAULT_API_BASE. Allows dynamic ports — no
/// hardcoding — so the embedded daemon can bind to port 0 and the frontend discovers it.
/// Returns the API base URL. Public for use by terminal_view, terminals, etc.
pub fn get_api_base() -> String {
    if let Some(window) = web_sys::window() {
        let port_js = js_sys::Reflect::get(&window, &JsValue::from_str("__TUNDRA_API_PORT__"));
        if let Ok(port_val) = port_js {
            if let Some(p) = port_val.as_f64() {
                let port = p as u16;
                return format!("http://127.0.0.1:{}", port);
            }
        }
    }
    DEFAULT_API_BASE.to_string()
}

/// Best-effort detection for backend connectivity failures.
/// Used by pages to switch into demo/offline fallback UX.
pub fn is_connection_error(message: &str) -> bool {
    let m = message.to_lowercase();
    m.contains("failed to connect")
        || m.contains("networkerror")
        || m.contains("network error")
        || m.contains("localhost")
        || m.contains("127.0.0.1")
        || m.contains("econnrefused")
        || m.contains("connection refused")
        || m.contains("timed out")
}

// ── Generic fetch helpers ──

/// Extract a clean error message from a JsValue.
/// Avoids dumping raw JsValue stack traces to the UI.
fn js_err(e: JsValue) -> String {
    // Try to get the .message property (TypeError, Error, etc.)
    if let Some(err) = e.dyn_ref::<js_sys::Error>() {
        return err
            .message()
            .as_string()
            .unwrap_or_else(|| "Unknown error".to_string());
    }
    // Try .toString()
    if let Some(s) = e.as_string() {
        return s;
    }
    "Network error".to_string()
}

async fn fetch_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let opts = RequestInit::new();
    opts.set_method("GET");

    let request = Request::new_with_str_and_init(url, &opts).map_err(js_err)?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(js_err)?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| format!("Failed to connect to {}", get_api_base()))?;

    let resp: Response = resp_value.dyn_into().map_err(js_err)?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let json = JsFuture::from(resp.json().map_err(js_err)?)
        .await
        .map_err(js_err)?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("Parse error: {e}"))
}

async fn post_json<T: Serialize, R: for<'de> Deserialize<'de>>(
    url: &str,
    body: &T,
) -> Result<R, String> {
    let body_str = serde_json::to_string(body).map_err(|e| format!("Serialize: {e}"))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&JsValue::from_str(&body_str));

    let request = Request::new_with_str_and_init(url, &opts).map_err(js_err)?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(js_err)?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(js_err)?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| format!("Failed to connect to {}", get_api_base()))?;

    let resp: Response = resp_value.dyn_into().map_err(js_err)?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let json = JsFuture::from(resp.json().map_err(js_err)?)
        .await
        .map_err(js_err)?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("Parse error: {e}"))
}

// ── API response types (matching backend JSON) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiBead {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub lane: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub priority_label: Option<String>,
    #[serde(default)]
    pub agent_profile: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub thinking_level: Option<String>,
    #[serde(default)]
    pub complexity: Option<String>,
    #[serde(default)]
    pub impact: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiAgent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKpi {
    #[serde(default)]
    pub total_beads: u64,
    #[serde(default)]
    pub backlog: u64,
    #[serde(default)]
    pub hooked: u64,
    #[serde(default)]
    pub slung: u64,
    #[serde(default)]
    pub review: u64,
    #[serde(default)]
    pub done: u64,
    #[serde(default)]
    pub failed: u64,
    #[serde(default)]
    pub active_agents: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiStatus {
    #[serde(default)]
    pub version: String,
    #[serde(default, alias = "uptime_seconds")]
    pub uptime_secs: u64,
    #[serde(default)]
    pub agent_count: usize,
    #[serde(default)]
    pub bead_count: usize,
}

// ── Stack types (stacked diffs) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiStackNode {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub pr_number: Option<u32>,
    #[serde(default)]
    pub stack_position: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiStack {
    pub root: ApiStackNode,
    #[serde(default)]
    pub children: Vec<ApiStackNode>,
    #[serde(default)]
    pub total: u32,
}

// ── Request body types ──

#[derive(Debug, Serialize)]
struct CreateBeadRequest {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lane: Option<String>,
}

#[derive(Debug, Serialize)]
struct UpdateStatusRequest {
    status: String,
}

async fn put_json<T: Serialize, R: for<'de> Deserialize<'de>>(
    url: &str,
    body: &T,
) -> Result<R, String> {
    let body_str = serde_json::to_string(body).map_err(|e| format!("Serialize: {e}"))?;

    let opts = RequestInit::new();
    opts.set_method("PUT");
    opts.set_body(&JsValue::from_str(&body_str));

    let request = Request::new_with_str_and_init(url, &opts).map_err(js_err)?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(js_err)?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(js_err)?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| format!("Failed to connect to {}", get_api_base()))?;

    let resp: Response = resp_value.dyn_into().map_err(js_err)?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let json = JsFuture::from(resp.json().map_err(js_err)?)
        .await
        .map_err(js_err)?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("Parse error: {e}"))
}

// ── Settings types (mirroring backend Config) ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiGeneralSettings {
    #[serde(default)]
    pub project_name: String,
    #[serde(default)]
    pub log_level: String,
    #[serde(default)]
    pub workspace_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiDisplaySettings {
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub font_size: u8,
    #[serde(default)]
    pub compact_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiAgentsSettings {
    #[serde(default)]
    pub max_concurrent: u32,
    #[serde(default)]
    pub heartbeat_interval_secs: u64,
    #[serde(default)]
    pub auto_restart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiTerminalSettings {
    #[serde(default)]
    pub font_family: String,
    #[serde(default)]
    pub font_size: u8,
    #[serde(default)]
    pub cursor_style: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiSecuritySettings {
    #[serde(default)]
    pub allow_shell_exec: bool,
    #[serde(default)]
    pub sandbox: bool,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub auto_lock_timeout_mins: u32,
    #[serde(default)]
    pub sandbox_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiIntegrationSettings {
    #[serde(default)]
    pub github_token_env: String,
    #[serde(default)]
    pub github_owner: Option<String>,
    #[serde(default)]
    pub github_repo: Option<String>,
    #[serde(default)]
    pub gitlab_token_env: String,
    #[serde(default)]
    pub linear_api_key_env: String,
    #[serde(default)]
    pub linear_team_id: Option<String>,
    #[serde(default)]
    pub openai_api_key_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiCredentialStatus {
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub daemon_auth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiAppearanceSettings {
    #[serde(default)]
    pub appearance_mode: String,
    #[serde(default)]
    pub color_theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiLanguageSettings {
    #[serde(default)]
    pub interface_language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiDevToolsSettings {
    #[serde(default)]
    pub preferred_ide: String,
    #[serde(default)]
    pub preferred_terminal: String,
    #[serde(default)]
    pub auto_name_terminals: bool,
    #[serde(default)]
    pub yolo_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiPhaseConfig {
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub thinking_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiAgentProfileSettings {
    #[serde(default)]
    pub default_profile: String,
    #[serde(default)]
    pub agent_framework: String,
    #[serde(default)]
    pub ai_terminal_naming: bool,
    #[serde(default)]
    pub phase_configs: Vec<ApiPhaseConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiPathsSettings {
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiClaudeAccount {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub authenticated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiApiProfileEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiApiProfilesSettings {
    #[serde(default)]
    pub profiles: Vec<ApiApiProfileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiUpdatesSettings {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub is_latest: bool,
    #[serde(default)]
    pub auto_update_projects: bool,
    #[serde(default)]
    pub beta_updates: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiNotificationSettings {
    #[serde(default)]
    pub on_task_complete: bool,
    #[serde(default)]
    pub on_task_failed: bool,
    #[serde(default)]
    pub on_review_needed: bool,
    #[serde(default)]
    pub sound_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiDebugSettings {
    #[serde(default)]
    pub anonymous_error_reporting: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiMemorySettings {
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiSettings {
    #[serde(default)]
    pub general: ApiGeneralSettings,
    #[serde(default)]
    pub display: ApiDisplaySettings,
    #[serde(default)]
    pub agents: ApiAgentsSettings,
    #[serde(default)]
    pub terminal: ApiTerminalSettings,
    #[serde(default)]
    pub security: ApiSecuritySettings,
    #[serde(default)]
    pub integrations: ApiIntegrationSettings,
    #[serde(default)]
    pub appearance: ApiAppearanceSettings,
    #[serde(default)]
    pub language: ApiLanguageSettings,
    #[serde(default)]
    pub dev_tools: ApiDevToolsSettings,
    #[serde(default)]
    pub agent_profile: ApiAgentProfileSettings,
    #[serde(default)]
    pub paths: ApiPathsSettings,
    #[serde(default)]
    pub api_profiles: ApiApiProfilesSettings,
    #[serde(default)]
    pub updates: ApiUpdatesSettings,
    #[serde(default)]
    pub notifications: ApiNotificationSettings,
    #[serde(default)]
    pub debug: ApiDebugSettings,
    #[serde(default)]
    pub memory: ApiMemorySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiLocalProviderSettings {
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiLocalProviderProbe {
    pub endpoint: String,
    pub flavor: String,
    pub reachable: bool,
    pub model_count: usize,
    pub sample_models: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiPipelineQueueStatus {
    pub limit: usize,
    pub waiting: usize,
    pub running: usize,
    pub available_permits: usize,
}

// ── Public API functions ──

pub async fn fetch_beads() -> Result<Vec<ApiBead>, String> {
    fetch_json(&format!("{}/api/beads", get_api_base())).await
}

pub async fn fetch_agents() -> Result<Vec<ApiAgent>, String> {
    fetch_json(&format!("{}/api/agents", get_api_base())).await
}

pub async fn fetch_kpi() -> Result<ApiKpi, String> {
    fetch_json(&format!("{}/api/kpi", get_api_base())).await
}

pub async fn fetch_status() -> Result<ApiStatus, String> {
    fetch_json(&format!("{}/api/status", get_api_base())).await
}

pub async fn fetch_pipeline_queue_status() -> Result<ApiPipelineQueueStatus, String> {
    fetch_json(&format!("{}/api/pipeline/queue", get_api_base())).await
}

pub async fn fetch_stacks() -> Result<Vec<ApiStack>, String> {
    match fetch_json::<Vec<ApiStack>>(&format!("{}/api/stacks", get_api_base())).await {
        Ok(stacks) => Ok(stacks),
        Err(_) => {
            // Return demo stacks when backend is offline
            Ok(demo_stacks())
        }
    }
}

fn demo_stacks() -> Vec<ApiStack> {
    vec![
        ApiStack {
            root: ApiStackNode {
                id: "bead-006".into(),
                title: "Build agent executor".into(),
                phase: "In Progress".into(),
                git_branch: Some("feat/agent-executor".into()),
                pr_number: Some(41),
                stack_position: 0,
            },
            children: vec![
                ApiStackNode {
                    id: "bead-007".into(),
                    title: "MCP tool integration".into(),
                    phase: "In Progress".into(),
                    git_branch: Some("feat/mcp-integration".into()),
                    pr_number: Some(39),
                    stack_position: 1,
                },
                ApiStackNode {
                    id: "bead-010".into(),
                    title: "Review agent executor v1".into(),
                    phase: "AI Review".into(),
                    git_branch: Some("feat/executor-review".into()),
                    pr_number: None,
                    stack_position: 2,
                },
            ],
            total: 3,
        },
        ApiStack {
            root: ApiStackNode {
                id: "bead-013".into(),
                title: "Core types refactor".into(),
                phase: "Human Review".into(),
                git_branch: Some("refactor/core-types".into()),
                pr_number: Some(39),
                stack_position: 0,
            },
            children: vec![ApiStackNode {
                id: "bead-014".into(),
                title: "Security audit: API auth".into(),
                phase: "Human Review".into(),
                git_branch: Some("feat/api-auth-audit".into()),
                pr_number: None,
                stack_position: 1,
            }],
            total: 2,
        },
        ApiStack {
            root: ApiStackNode {
                id: "bead-016".into(),
                title: "Setup project scaffolding".into(),
                phase: "Done".into(),
                git_branch: Some("feat/scaffolding".into()),
                pr_number: Some(33),
                stack_position: 0,
            },
            children: vec![
                ApiStackNode {
                    id: "bead-017".into(),
                    title: "Implement core types".into(),
                    phase: "Done".into(),
                    git_branch: Some("feat/core-types".into()),
                    pr_number: Some(28),
                    stack_position: 1,
                },
                ApiStackNode {
                    id: "bead-018".into(),
                    title: "Logger setup".into(),
                    phase: "Done".into(),
                    git_branch: Some("feat/logger".into()),
                    pr_number: Some(25),
                    stack_position: 2,
                },
            ],
            total: 3,
        },
    ]
}

pub async fn create_bead(
    title: &str,
    description: Option<&str>,
    lane: Option<&str>,
) -> Result<ApiBead, String> {
    let body = CreateBeadRequest {
        title: title.to_string(),
        description: description.map(|s| s.to_string()),
        lane: lane.map(|s| s.to_string()),
    };
    post_json(&format!("{}/api/beads", get_api_base()), &body).await
}

pub async fn update_bead_status(id: &str, status: &str) -> Result<ApiBead, String> {
    let body = UpdateStatusRequest {
        status: status.to_string(),
    };
    post_json(&format!("{}/api/beads/{id}/status", get_api_base()), &body).await
}

pub async fn fetch_settings() -> Result<ApiSettings, String> {
    fetch_json(&format!("{}/api/settings", get_api_base())).await
}

pub async fn fetch_local_provider_settings() -> Result<ApiLocalProviderSettings, String> {
    let raw: serde_json::Value = fetch_json(&format!("{}/api/settings", get_api_base())).await?;
    let providers = raw.get("providers").cloned().unwrap_or_default();

    let base_url = providers
        .get("local_base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("http://127.0.0.1:11434")
        .to_string();
    let model = providers
        .get("local_model")
        .and_then(|v| v.as_str())
        .unwrap_or("qwen2.5-coder:14b")
        .to_string();
    let api_key_env = providers
        .get("local_api_key_env")
        .and_then(|v| v.as_str())
        .unwrap_or("LOCAL_API_KEY")
        .to_string();

    Ok(ApiLocalProviderSettings {
        base_url,
        model,
        api_key_env,
    })
}

pub async fn probe_local_provider(base_url: &str) -> Result<ApiLocalProviderProbe, String> {
    let base = base_url.trim_end_matches('/').to_string();

    // Ollama native API: GET /api/tags => { models: [{ name, ... }] }
    if let Ok(tags) = fetch_json::<serde_json::Value>(&format!("{base}/api/tags")).await {
        let names = tags
            .get("models")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        m.get("name")
                            .and_then(|n| n.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let model_count = names.len();
        let sample_models = names.into_iter().take(5).collect::<Vec<_>>();
        return Ok(ApiLocalProviderProbe {
            endpoint: base,
            flavor: "ollama".to_string(),
            reachable: true,
            model_count,
            sample_models,
            message: if model_count > 0 {
                format!("Connected to Ollama. Found {model_count} model(s).")
            } else {
                "Connected to Ollama, but no local models are installed.".to_string()
            },
        });
    }

    // OpenAI-compatible local APIs: GET /v1/models => { data: [{ id, ... }] }
    if let Ok(models) = fetch_json::<serde_json::Value>(&format!("{base}/v1/models")).await {
        let ids = models
            .get("data")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let model_count = ids.len();
        let sample_models = ids.into_iter().take(5).collect::<Vec<_>>();
        return Ok(ApiLocalProviderProbe {
            endpoint: base,
            flavor: "openai-compatible".to_string(),
            reachable: true,
            model_count,
            sample_models,
            message: if model_count > 0 {
                format!(
                    "Connected to OpenAI-compatible local provider. Found {model_count} model(s)."
                )
            } else {
                "Connected to local provider, but no models were listed.".to_string()
            },
        });
    }

    Err("Failed to connect to local provider endpoint. Tried /api/tags and /v1/models.".to_string())
}

pub async fn save_settings(settings: &ApiSettings) -> Result<ApiSettings, String> {
    put_json(&format!("{}/api/settings", get_api_base()), settings).await
}

pub async fn fetch_credential_status() -> Result<ApiCredentialStatus, String> {
    fetch_json(&format!("{}/api/credentials/status", get_api_base())).await
}

// ── DELETE helper ──

async fn delete_request(url: &str) -> Result<(), String> {
    let opts = RequestInit::new();
    opts.set_method("DELETE");

    let request = Request::new_with_str_and_init(url, &opts).map_err(js_err)?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(js_err)?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| format!("Failed to connect to {}", get_api_base()))?;

    let resp: Response = resp_value.dyn_into().map_err(js_err)?;
    if resp.ok() {
        Ok(())
    } else {
        Err(format!("DELETE failed with status {}", resp.status()))
    }
}

async fn post_empty<R: for<'de> Deserialize<'de>>(url: &str) -> Result<R, String> {
    let opts = RequestInit::new();
    opts.set_method("POST");

    let request = Request::new_with_str_and_init(url, &opts).map_err(js_err)?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(js_err)?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| format!("Failed to connect to {}", get_api_base()))?;

    let resp: Response = resp_value.dyn_into().map_err(js_err)?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let json = JsFuture::from(resp.json().map_err(js_err)?)
        .await
        .map_err(js_err)?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("Parse error: {e}"))
}

// ── Additional API response types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSession {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub agent_name: String,
    #[serde(default)]
    pub cli_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConvoy {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub bead_count: u32,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub bead_ids: Vec<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiWorktree {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub bead_id: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCosts {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub sessions: Vec<ApiCostSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCostSession {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub agent_name: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMcpServer {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMemoryEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub created_at: String,
}

/// A single feature within a roadmap (matches backend `RoadmapFeature`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRoadmapFeature {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub estimated_effort: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub created_at: String,
}

/// A roadmap container with nested features (matches backend `Roadmap`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRoadmap {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub features: Vec<ApiRoadmapFeature>,
    #[serde(default)]
    pub generated_at: String,
}

/// Flat roadmap item used by the UI after flattening nested roadmaps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRoadmapItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: String,
}

/// Flatten a list of `ApiRoadmap` into `Vec<ApiRoadmapItem>` for UI consumption.
fn flatten_roadmaps(roadmaps: Vec<ApiRoadmap>) -> Vec<ApiRoadmapItem> {
    roadmaps
        .into_iter()
        .flat_map(|r| {
            r.features.into_iter().map(|f| ApiRoadmapItem {
                id: f.id,
                title: f.title,
                description: f.description,
                status: f.status,
                priority: match f.priority {
                    0..=3 => "high".to_string(),
                    4..=6 => "medium".to_string(),
                    _ => "low".to_string(),
                },
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiIdea {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub impact: String,
    #[serde(default)]
    pub effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiInsightsSession {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiInsightsMessage {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct AddMemoryRequest {
    pub key: String,
    pub value: String,
    pub category: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct SendInsightsMessageRequest {
    pub content: String,
}

// ── Additional public API functions ──

pub async fn stop_agent(id: &str) -> Result<ApiAgent, String> {
    post_empty(&format!("{}/api/agents/{id}/stop", get_api_base())).await
}

pub async fn assign_agent(task_id: &str) -> Result<serde_json::Value, String> {
    post_empty(&format!("{}/api/tasks/{task_id}/assign", get_api_base())).await
}

pub async fn fetch_sessions() -> Result<Vec<ApiSession>, String> {
    fetch_json(&format!("{}/api/sessions", get_api_base())).await
}

pub async fn fetch_convoys() -> Result<Vec<ApiConvoy>, String> {
    fetch_json(&format!("{}/api/convoys", get_api_base())).await
}

pub async fn fetch_worktrees() -> Result<Vec<ApiWorktree>, String> {
    fetch_json(&format!("{}/api/worktrees", get_api_base())).await
}

pub async fn delete_worktree(id: &str) -> Result<(), String> {
    delete_request(&format!("{}/api/worktrees/{id}", get_api_base())).await
}

pub async fn fetch_costs() -> Result<ApiCosts, String> {
    fetch_json(&format!("{}/api/costs", get_api_base())).await
}

pub async fn fetch_mcp_servers() -> Result<Vec<ApiMcpServer>, String> {
    fetch_json(&format!("{}/api/mcp/servers", get_api_base())).await
}

#[derive(Debug, Serialize)]
struct AddMcpServerRequest {
    name: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<Vec<String>>,
}

pub async fn add_mcp_server(
    name: &str,
    command: &str,
    args: Option<Vec<String>>,
) -> Result<ApiMcpServer, String> {
    let body = AddMcpServerRequest {
        name: name.to_string(),
        command: command.to_string(),
        args,
    };
    post_json(&format!("{}/api/mcp/servers", get_api_base()), &body).await
}

pub async fn fetch_memory() -> Result<Vec<ApiMemoryEntry>, String> {
    fetch_json(&format!("{}/api/memory", get_api_base())).await
}

pub async fn search_memory(query: &str) -> Result<Vec<ApiMemoryEntry>, String> {
    fetch_json(&format!("{}/api/memory/search?q={query}", get_api_base())).await
}

pub async fn add_memory(category: &str, content: &str) -> Result<ApiMemoryEntry, String> {
    let body = AddMemoryRequest {
        key: content
            .split_whitespace()
            .take(5)
            .collect::<Vec<_>>()
            .join("_"),
        value: content.to_string(),
        category: category.to_string(),
        source: "ui".to_string(),
    };
    post_json(&format!("{}/api/memory", get_api_base()), &body).await
}

pub async fn fetch_roadmap() -> Result<Vec<ApiRoadmapItem>, String> {
    let roadmaps: Vec<ApiRoadmap> = fetch_json(&format!("{}/api/roadmap", get_api_base())).await?;
    Ok(flatten_roadmaps(roadmaps))
}

pub async fn generate_roadmap() -> Result<Vec<ApiRoadmapItem>, String> {
    let roadmap: ApiRoadmap =
        post_empty(&format!("{}/api/roadmap/generate", get_api_base())).await?;
    Ok(flatten_roadmaps(vec![roadmap]))
}

pub async fn fetch_ideas() -> Result<Vec<ApiIdea>, String> {
    fetch_json(&format!("{}/api/ideation/ideas", get_api_base())).await
}

pub async fn generate_ideas() -> Result<Vec<ApiIdea>, String> {
    // The backend returns IdeationResult { ideas, analysis_type, generated_at }
    // — we need to unwrap the .ideas field.
    #[derive(Deserialize)]
    struct IdeationResult {
        ideas: Vec<ApiIdea>,
    }
    let result: IdeationResult =
        post_empty(&format!("{}/api/ideation/generate", get_api_base())).await?;
    Ok(result.ideas)
}

pub async fn fetch_insights_sessions() -> Result<Vec<ApiInsightsSession>, String> {
    fetch_json(&format!("{}/api/insights/sessions", get_api_base())).await
}

pub async fn fetch_insights_messages(session_id: &str) -> Result<Vec<ApiInsightsMessage>, String> {
    fetch_json(&format!(
        "{}/api/insights/sessions/{session_id}/messages",
        get_api_base()
    ))
    .await
}

pub async fn send_insights_message(
    session_id: &str,
    content: &str,
) -> Result<ApiInsightsMessage, String> {
    let body = SendInsightsMessageRequest {
        content: content.to_string(),
    };
    let _: serde_json::Value = post_json(
        &format!(
            "{}/api/insights/sessions/{session_id}/messages",
            get_api_base()
        ),
        &body,
    )
    .await?;

    // Backend returns {"ok": true} — re-fetch messages to pick up any
    // server-side assistant reply that may have been generated.
    let messages = fetch_insights_messages(session_id).await?;
    if let Some(last) = messages.into_iter().rev().find(|m| m.role == "assistant") {
        Ok(last)
    } else {
        // No assistant reply was generated yet; return acknowledgement.
        Ok(ApiInsightsMessage {
            id: String::new(),
            role: "assistant".to_string(),
            content: "Message acknowledged.".to_string(),
        })
    }
}

// ── Notification types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiNotification {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub action_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiNotificationCount {
    #[serde(default)]
    pub unread: u64,
    #[serde(default)]
    pub total: u64,
}

// ── Notification API functions ──

pub async fn fetch_notifications(
    unread_only: bool,
    limit: usize,
    offset: usize,
) -> Result<Vec<ApiNotification>, String> {
    let unread_param = if unread_only { "&unread=true" } else { "" };
    fetch_json(&format!(
        "{}/api/notifications?limit={limit}&offset={offset}{unread_param}",
        get_api_base()
    ))
    .await
}

pub async fn fetch_notification_count() -> Result<ApiNotificationCount, String> {
    fetch_json(&format!("{}/api/notifications/count", get_api_base())).await
}

pub async fn mark_notification_read(id: &str) -> Result<serde_json::Value, String> {
    post_empty(&format!("{}/api/notifications/{id}/read", get_api_base())).await
}

pub async fn mark_all_notifications_read() -> Result<serde_json::Value, String> {
    post_empty(&format!("{}/api/notifications/read-all", get_api_base())).await
}

pub async fn delete_notification(id: &str) -> Result<(), String> {
    delete_request(&format!("{}/api/notifications/{id}", get_api_base())).await
}

// ── GitHub API types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiGithubIssue {
    #[serde(default)]
    pub number: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiGithubPr {
    #[serde(default)]
    pub number: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub reviewers: Vec<String>,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

// ── GitLab API types ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiGitLabUser {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiGitLabMergeRequest {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub iid: u32,
    #[serde(default)]
    pub project_id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub author: ApiGitLabUser,
    #[serde(default)]
    pub source_branch: String,
    #[serde(default)]
    pub target_branch: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub merge_status: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub web_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiGitLabReviewFinding {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiGitLabReviewResult {
    #[serde(default)]
    pub findings: Vec<ApiGitLabReviewFinding>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub approved: bool,
    #[serde(default)]
    pub reviewed_at: String,
}

#[derive(Debug, Serialize)]
struct ReviewGitLabMrRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity_threshold: Option<String>,
}

// ── Task API types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTask {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub bead_id: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub complexity: String,
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Serialize)]
pub struct CreateTaskRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub bead_id: String,
    pub priority: String,
    pub complexity: String,
    pub category: String,
}

// ── GitHub API functions ──

pub async fn fetch_github_issues() -> Result<Vec<ApiGithubIssue>, String> {
    fetch_json(&format!("{}/api/github/issues", get_api_base())).await
}

pub async fn fetch_github_prs() -> Result<Vec<ApiGithubPr>, String> {
    fetch_json(&format!("{}/api/github/prs", get_api_base())).await
}

pub async fn fetch_gitlab_merge_requests(
    project_id: Option<&str>,
    state: Option<&str>,
) -> Result<Vec<ApiGitLabMergeRequest>, String> {
    let mut url = format!("{}/api/gitlab/merge-requests", get_api_base());
    let mut query = Vec::new();
    if let Some(pid) = project_id {
        if !pid.trim().is_empty() {
            query.push(format!("project_id={}", pid.trim()));
        }
    }
    if let Some(s) = state {
        if !s.trim().is_empty() {
            query.push(format!("state={}", s.trim()));
        }
    }
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query.join("&"));
    }
    fetch_json(&url).await
}

pub async fn review_gitlab_merge_request(
    iid: u32,
    project_id: Option<&str>,
    strict: Option<bool>,
    severity_threshold: Option<&str>,
) -> Result<ApiGitLabReviewResult, String> {
    let body = ReviewGitLabMrRequest {
        project_id: project_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        strict,
        severity_threshold: severity_threshold
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
    };
    post_json(
        &format!("{}/api/gitlab/merge-requests/{iid}/review", get_api_base()),
        &body,
    )
    .await
}

pub async fn sync_github() -> Result<serde_json::Value, String> {
    post_empty(&format!("{}/api/github/sync", get_api_base())).await
}

pub async fn import_issue_as_bead(issue_number: u32) -> Result<ApiBead, String> {
    post_empty(&format!(
        "{}/api/github/issues/{issue_number}/import",
        get_api_base()
    ))
    .await
}

// ── Task API functions ──

pub async fn create_task(
    title: &str,
    description: Option<&str>,
    bead_id: &str,
    priority: &str,
    complexity: &str,
    category: &str,
) -> Result<ApiTask, String> {
    let body = CreateTaskRequest {
        title: title.to_string(),
        description: description.map(|s| s.to_string()),
        bead_id: bead_id.to_string(),
        priority: priority.to_string(),
        complexity: complexity.to_string(),
        category: category.to_string(),
    };
    post_json(&format!("{}/api/tasks", get_api_base()), &body).await
}

// ── Changelog API types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiChangelogSection {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiChangelogEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub sections: Vec<ApiChangelogSection>,
}

#[derive(Debug, Serialize)]
struct GenerateChangelogRequest {
    commits: String,
    version: String,
}

// ── Changelog API functions ──

pub async fn fetch_changelog() -> Result<Vec<ApiChangelogEntry>, String> {
    fetch_json(&format!("{}/api/changelog", get_api_base())).await
}

pub async fn generate_changelog(commits: &str, version: &str) -> Result<ApiChangelogEntry, String> {
    let body = GenerateChangelogRequest {
        commits: commits.to_string(),
        version: version.to_string(),
    };
    post_json(&format!("{}/api/changelog/generate", get_api_base()), &body).await
}

// ── GitHub Release API ──

#[derive(Debug, Serialize)]
struct PublishGithubReleaseRequest {
    tag_name: String,
    name: String,
    body: String,
    draft: bool,
    prerelease: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiGithubRelease {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub tag_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub html_url: String,
}

pub async fn publish_github_release(
    tag_name: &str,
    name: &str,
    body: &str,
) -> Result<ApiGithubRelease, String> {
    let req = PublishGithubReleaseRequest {
        tag_name: tag_name.to_string(),
        name: name.to_string(),
        body: body.to_string(),
        draft: false,
        prerelease: false,
    };
    post_json(&format!("{}/api/github/releases", get_api_base()), &req).await
}

// ── Roadmap feature creation ──

#[derive(Debug, Serialize)]
struct AddRoadmapFeatureRequest {
    title: String,
    description: String,
    status: String,
    priority: String,
}

pub async fn add_roadmap_feature(
    title: &str,
    description: &str,
    status: &str,
    priority: &str,
) -> Result<ApiRoadmapItem, String> {
    let body = AddRoadmapFeatureRequest {
        title: title.to_string(),
        description: description.to_string(),
        status: status.to_string(),
        priority: priority.to_string(),
    };
    let feature: ApiRoadmapFeature =
        post_json(&format!("{}/api/roadmap/features", get_api_base()), &body).await?;
    Ok(ApiRoadmapItem {
        id: feature.id,
        title: feature.title,
        description: feature.description,
        status: feature.status,
        priority: match feature.priority {
            0..=3 => "high".to_string(),
            4..=6 => "medium".to_string(),
            _ => "low".to_string(),
        },
    })
}

pub async fn update_roadmap_feature_status(id: &str, status: &str) -> Result<(), String> {
    let body = UpdateStatusRequest {
        status: status.to_string(),
    };
    let _: serde_json::Value = put_json(
        &format!("{}/api/roadmap/features/{id}/status", get_api_base()),
        &body,
    )
    .await?;
    Ok(())
}

// ── File Tree API types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiFileNode {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default)]
    pub children: Vec<ApiFileNode>,
}

pub async fn fetch_file_tree() -> Result<Vec<ApiFileNode>, String> {
    // Backend GET /api/context returns a context summary object, not a file list.
    // Extract useful top-level keys and present them as virtual tree nodes.
    let summary: serde_json::Value = fetch_json(&format!("{}/api/context", get_api_base())).await?;

    let mut nodes = Vec::new();
    if let Some(obj) = summary.as_object() {
        for (key, val) in obj {
            let child_text = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Array(arr) => format!("{} items", arr.len()),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => serde_json::to_string(val).unwrap_or_default(),
            };
            nodes.push(ApiFileNode {
                name: key.clone(),
                path: key.clone(),
                is_dir: val.is_object() || val.is_array(),
                children: if let Some(arr) = val.as_array() {
                    arr.iter()
                        .filter_map(|v| {
                            v.as_str().map(|s| ApiFileNode {
                                name: s.to_string(),
                                path: s.to_string(),
                                is_dir: false,
                                children: Vec::new(),
                            })
                        })
                        .collect()
                } else {
                    vec![ApiFileNode {
                        name: child_text,
                        path: String::new(),
                        is_dir: false,
                        children: Vec::new(),
                    }]
                },
            });
        }
    }
    Ok(nodes)
}

// ── Diff API types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDiffFile {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub change_type: String,
    #[serde(default)]
    pub diff_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDiffResponse {
    #[serde(default)]
    pub files: Vec<ApiDiffFile>,
}

// ── QA API types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiQaCheck {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub passed: bool,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiQaReport {
    #[serde(default)]
    pub passed: bool,
    #[serde(default)]
    pub checks: Vec<ApiQaCheck>,
    #[serde(default)]
    pub suggestions: Vec<String>,
}

// ── Diff & QA API functions ──

pub async fn fetch_task_diff(task_id: &str) -> Result<ApiDiffResponse, String> {
    fetch_json(&format!("{}/api/tasks/{task_id}/diff", get_api_base())).await
}

pub async fn run_task_qa(task_id: &str) -> Result<ApiQaReport, String> {
    post_empty(&format!("{}/api/tasks/{task_id}/qa", get_api_base())).await
}

// ── Insights with model ──

#[derive(Debug, Serialize)]
pub struct SendInsightsMessageWithModelRequest {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

pub async fn send_insights_message_with_model(
    session_id: &str,
    content: &str,
    model: Option<&str>,
) -> Result<ApiInsightsMessage, String> {
    let body = SendInsightsMessageWithModelRequest {
        content: content.to_string(),
        model: model.map(|s| s.to_string()),
    };
    let _: serde_json::Value = post_json(
        &format!(
            "{}/api/insights/sessions/{session_id}/messages",
            get_api_base()
        ),
        &body,
    )
    .await?;

    // Re-fetch messages to pick up any server-side assistant reply.
    let messages = fetch_insights_messages(session_id).await?;
    if let Some(last) = messages.into_iter().rev().find(|m| m.role == "assistant") {
        Ok(last)
    } else {
        Ok(ApiInsightsMessage {
            id: String::new(),
            role: "assistant".to_string(),
            content: "Message acknowledged.".to_string(),
        })
    }
}

/// Return the WebSocket URL for event streaming.
pub fn events_ws_url() -> String {
    let base = get_api_base();
    let ws_base = base
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    format!("{ws_base}/api/events/ws")
}

// ── Project API types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiProject {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub is_active: bool,
}

#[derive(Debug, Serialize)]
struct CreateProjectRequest {
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct UpdateProjectRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

// ── Project API functions ──

pub async fn fetch_projects() -> Result<Vec<ApiProject>, String> {
    fetch_json(&format!("{}/api/projects", get_api_base())).await
}

pub async fn create_project(name: &str, path: &str) -> Result<ApiProject, String> {
    let body = CreateProjectRequest {
        name: name.to_string(),
        path: path.to_string(),
    };
    post_json(&format!("{}/api/projects", get_api_base()), &body).await
}

pub async fn update_project(
    id: &str,
    name: Option<&str>,
    path: Option<&str>,
) -> Result<ApiProject, String> {
    let body = UpdateProjectRequest {
        name: name.map(|s| s.to_string()),
        path: path.map(|s| s.to_string()),
    };
    put_json(&format!("{}/api/projects/{id}", get_api_base()), &body).await
}

pub async fn delete_project(id: &str) -> Result<(), String> {
    delete_request(&format!("{}/api/projects/{id}", get_api_base())).await
}

pub async fn activate_project(id: &str) -> Result<ApiProject, String> {
    post_empty(&format!("{}/api/projects/{id}/activate", get_api_base())).await
}

pub async fn delete_bead(id: &str) -> Result<(), String> {
    delete_request(&format!("{}/api/beads/{id}", get_api_base())).await
}

pub async fn update_bead(id: &str, bead: &ApiBead) -> Result<ApiBead, String> {
    put_json(&format!("{}/api/beads/{id}", get_api_base()), bead).await
}

pub async fn execute_task(id: &str) -> Result<serde_json::Value, String> {
    post_empty(&format!("{}/api/tasks/{id}/execute", get_api_base())).await
}

// ── GitHub PR action functions ──

pub async fn checkout_pr_branch(pr_number: u64) -> Result<serde_json::Value, String> {
    post_empty(&format!(
        "{}/api/github/prs/{pr_number}/checkout",
        get_api_base()
    ))
    .await
}

pub async fn review_pr(pr_number: u64) -> Result<serde_json::Value, String> {
    post_empty(&format!(
        "{}/api/github/prs/{pr_number}/review",
        get_api_base()
    ))
    .await
}

pub async fn merge_pr(pr_number: u64) -> Result<serde_json::Value, String> {
    post_empty(&format!(
        "{}/api/github/prs/{pr_number}/merge",
        get_api_base()
    ))
    .await
}

pub async fn fetch_github_releases() -> Result<Vec<ApiGithubRelease>, String> {
    fetch_json(&format!("{}/api/github/releases", get_api_base())).await
}

// ── GitHub Issues analysis ──

pub async fn analyze_issues() -> Result<serde_json::Value, String> {
    post_empty(&format!("{}/api/github/issues/analyze", get_api_base())).await
}

// ── Worktree actions ──

pub async fn merge_worktree(id: &str) -> Result<serde_json::Value, String> {
    post_empty(&format!("{}/api/worktrees/{id}/merge", get_api_base())).await
}

// ── Updates ──

pub async fn check_updates() -> Result<ApiUpdatesSettings, String> {
    fetch_json(&format!("{}/api/updates/check", get_api_base())).await
}

// ── CLI detection ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiCliAvailable {
    #[serde(default)]
    pub tools: Vec<String>,
}

pub async fn fetch_cli_available() -> Result<ApiCliAvailable, String> {
    fetch_json(&format!("{}/api/cli/available", get_api_base())).await
}

// (Stacked Diffs API types and functions are defined above near line 186)
