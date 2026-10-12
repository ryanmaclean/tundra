use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

// Re-export shared API types from at-api-types crate
pub use at_api_types::{
    AddMemoryRequest, ApiAgent, ApiBead, ApiChangelogEntry, ApiChangelogSection, ApiConvoy,
    ApiCostSession, ApiCosts, ApiGithubIssue, ApiGithubPr, ApiIdea, ApiKpi, ApiMcpServer,
    ApiMemoryEntry, ApiRoadmap, ApiRoadmapFeature, ApiRoadmapItem, ApiSession, ApiStack,
    ApiStackNode, ApiWorktree, CreateBeadRequest, CreateTaskRequest, SendInsightsMessageRequest,
    SendInsightsMessageWithModelRequest, UpdateStatusRequest,
};

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

// ── Tauri IPC helpers ──

/// Tauri invoke binding for calling backend commands via IPC
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "tauri"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Check if we're running in a Tauri context
pub fn is_tauri() -> bool {
    if let Some(window) = web_sys::window() {
        js_sys::Reflect::has(&window, &JsValue::from_str("__TAURI__")).unwrap_or(false)
    } else {
        false
    }
}

/// Invoke a Tauri command with no arguments
pub async fn tauri_invoke<R: for<'de> Deserialize<'de>>(cmd: &str) -> Result<R, String> {
    let result = invoke(cmd, JsValue::NULL).await.map_err(js_err)?;
    serde_wasm_bindgen::from_value(result).map_err(|e| format!("Tauri invoke error: {e}"))
}

/// Invoke a Tauri command with arguments
pub async fn tauri_invoke_with_args<T: Serialize, R: for<'de> Deserialize<'de>>(
    cmd: &str,
    args: &T,
) -> Result<R, String> {
    let js_args =
        serde_wasm_bindgen::to_value(args).map_err(|e| format!("Serialize error: {e}"))?;
    let result = invoke(cmd, js_args).await.map_err(js_err)?;
    serde_wasm_bindgen::from_value(result).map_err(|e| format!("Tauri invoke error: {e}"))
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
// Core types (ApiBead, ApiAgent, ApiKpi, ApiSession, ApiConvoy) are imported from at-api-types crate

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

// ── Request body types ──
// CreateBeadRequest, UpdateStatusRequest, AddMemoryRequest imported from at-api-types crate

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

async fn patch_json<T: Serialize, R: for<'de> Deserialize<'de>>(
    url: &str,
    body: &T,
) -> Result<R, String> {
    let body_str = serde_json::to_string(body).map_err(|e| format!("Serialize: {e}"))?;

    let opts = RequestInit::new();
    opts.set_method("PATCH");
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
    if is_tauri() {
        // Use Tauri IPC
        #[derive(Serialize)]
        struct ListBeadsArgs {
            status: Option<String>,
        }
        let args = ListBeadsArgs { status: None };
        tauri_invoke_with_args("cmd_list_beads", &args).await
    } else {
        // Fallback to HTTP
        fetch_json(&format!("{}/api/beads", get_api_base())).await
    }
}

pub async fn fetch_agents() -> Result<Vec<ApiAgent>, String> {
    if is_tauri() {
        tauri_invoke("cmd_list_agents").await
    } else {
        // Fallback to HTTP
        fetch_json(&format!("{}/api/agents", get_api_base())).await
    }
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
    if is_tauri() {
        // Use Tauri IPC
        #[derive(Serialize)]
        struct CreateBeadArgs {
            title: String,
            description: Option<String>,
            lane: Option<String>,
            tags: Option<Vec<String>>,
        }
        let args = CreateBeadArgs {
            title: title.to_string(),
            description: description.map(|s| s.to_string()),
            lane: lane.map(|s| s.to_string()),
            tags: None,
        };
        tauri_invoke_with_args("cmd_create_bead", &args).await
    } else {
        // Fallback to HTTP
        let body = CreateBeadRequest {
            title: title.to_string(),
            description: description.map(|s| s.to_string()),
            lane: lane.map(|s| s.to_string()),
        };
        post_json(&format!("{}/api/beads", get_api_base()), &body).await
    }
}

pub async fn update_bead_status(id: &str, status: &str) -> Result<ApiBead, String> {
    let body = UpdateStatusRequest {
        status: status.to_string(),
    };
    post_json(&format!("{}/api/beads/{id}/status", get_api_base()), &body).await
}

pub async fn fetch_settings() -> Result<ApiSettings, String> {
    if is_tauri() {
        // Use Tauri IPC
        tauri_invoke("cmd_get_settings").await
    } else {
        // Fallback to HTTP
        fetch_json(&format!("{}/api/settings", get_api_base())).await
    }
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


/// A successful load plus the initial editor projection, not a replacement Config.
/// Unedited backend fields are omitted even when UI defaults differ from storage.
#[derive(Clone)]
pub struct SettingsEditSession {
    loaded: ApiSettings,
    initial: ApiSettings,
}

impl SettingsEditSession {
    pub fn new(loaded: ApiSettings, initial: ApiSettings) -> Self { Self { loaded, initial } }

    /// Only loaded or acknowledged profiles are displayed and used for appends.
    pub fn profiles(&self) -> &[ApiApiProfileEntry] { &self.loaded.api_profiles.profiles }

    fn profile_create_patch(&self, profile: &ApiApiProfileEntry) -> Result<serde_json::Value, String> {
        if profile.name.trim().is_empty() {
            return Err("A profile name is required".into());
        }
        // Existing optional URL/key-env semantics are unchanged. This stores an
        // environment variable name, never resolves or reads its credential.
        let mut profiles = self.profiles().to_vec();
        profiles.push(profile.clone());
        Ok(serde_json::json!({"api_profiles":{"profiles":profiles}}))
    }

    fn acknowledge_profile_create(&self, partial: &serde_json::Value, saved: ApiSettings)
        -> Result<Self, String> {
        let actual = serde_json::to_value(&saved.api_profiles.profiles).map_err(|e| e.to_string())?;
        if actual != partial["api_profiles"]["profiles"] {
            return Err("Profile response did not acknowledge the submitted list; reload before retrying".into());
        }
        let mut next = self.clone();
        next.loaded.api_profiles = saved.api_profiles;
        // Do not reset initial to the live ordinary editor draft: none of those
        // edits were submitted. Even dirty fields predating this call stay dirty.
        Ok(next)
    }

    pub fn patch(&self, edited: &ApiSettings) -> Result<serde_json::Value, String> {
        use serde_json::{Map, Value};
        let before = serde_json::to_value(&self.initial).map_err(|e| e.to_string())?;
        let after = serde_json::to_value(edited).map_err(|e| e.to_string())?;
        let mut patch = Map::new();
        // Only fields actually owned by this editor. In particular, no profiles,
        // provider configuration, backend security defaults, or version metadata.
        let fields: &[(&str, &[&str])] = &[
            ("appearance", &["appearance_mode", "color_theme"]),
            ("display", &["theme", "font_size"]),
            ("language", &["interface_language"]),
            ("dev_tools", &["preferred_ide", "preferred_terminal", "auto_name_terminals", "yolo_mode"]),
            ("agent_profile", &["default_profile", "agent_framework", "ai_terminal_naming"]),
            ("paths", &["python_path", "git_path", "github_cli_path", "claude_cli_path"]),
            ("integrations", &["github_token_env", "github_owner", "github_repo", "gitlab_token_env", "linear_api_key_env", "linear_team_id", "openai_api_key_env"]),
            ("updates", &["auto_update_projects", "beta_updates"]),
            ("notifications", &["on_task_complete", "on_task_failed", "on_review_needed", "sound_enabled"]),
            ("debug", &["anonymous_error_reporting"]),
            ("memory", &["enable_memory", "enable_agent_memory_access", "graphiti_server_url", "embedding_provider", "embedding_model"]),
        ];
        for (section, keys) in fields {
            let mut changed = Map::new();
            for key in *keys {
                if before[*section][*key] != after[*section][*key] {
                    changed.insert((*key).to_string(), after[*section][*key].clone());
                }
            }
            if !changed.is_empty() { patch.insert((*section).into(), Value::Object(changed)); }
        }
        let old_phases = &self.initial.agent_profile.phase_configs;
        let new_phases = &edited.agent_profile.phase_configs;
        if serde_json::to_value(old_phases).map_err(|e| e.to_string())?
            != serde_json::to_value(new_phases).map_err(|e| e.to_string())? {
            // PATCH replaces arrays. Preserve unknown/unedited stored phases by
            // identity, not index. Reject ambiguous identities rather than guess.
            for phases in [old_phases, new_phases, &self.loaded.agent_profile.phase_configs] {
                let mut names = std::collections::HashSet::new();
                if phases.iter().any(|p| p.phase.is_empty() || !names.insert(&p.phase)) {
                    return Err("Cannot edit ambiguous phase identities".into());
                }
            }
            if old_phases.len() != new_phases.len()
                || old_phases.iter().any(|p| !new_phases.iter().any(|n| n.phase == p.phase)) {
                return Err("This editor cannot add or remove phase identities".into());
            }
            let mut merged = self.loaded.agent_profile.phase_configs.clone();
            let mut changed = false;
            for next in new_phases {
                let prior = old_phases.iter().find(|p| p.phase == next.phase).unwrap();
                let model_changed = prior.model != next.model;
                let thinking_changed = prior.thinking_level != next.thinking_level;
                if !model_changed && !thinking_changed { continue; }
                changed = true;
                if let Some(saved) = merged.iter_mut().find(|p| p.phase == next.phase) {
                    if model_changed { saved.model = next.model.clone(); }
                    if thinking_changed { saved.thinking_level = next.thinking_level.clone(); }
                } else { merged.push(next.clone()); }
            }
            if changed {
                let section = patch.entry("agent_profile").or_insert_with(|| Value::Object(Map::new()));
                section.as_object_mut().unwrap().insert("phase_configs".into(),
                    serde_json::to_value(merged).map_err(|e| e.to_string())?);
            }
        }
        Ok(Value::Object(patch))
    }
}

/// Build the ordinary-save payload only after a successful initial load.
/// Reset deliberately uses save_settings (full replacement) instead.
pub fn settings_edit_patch(session: Option<&SettingsEditSession>, edited: &ApiSettings)
    -> Result<serde_json::Value, String> {
    session.ok_or_else(|| "Settings have not loaded; reload before saving".to_string())?.patch(edited)
}

/// Dedicated profile creation shares the real PATCH route but not ordinary-save
/// acknowledgement. A failed load/request/response cannot fabricate a saved list.
pub async fn create_api_profile(session: Option<&SettingsEditSession>, profile: &ApiApiProfileEntry)
    -> Result<SettingsEditSession, String> {
    let session = session.ok_or_else(|| "Settings have not loaded; reload before creating a profile".to_string())?;
    let partial = session.profile_create_patch(profile)?;
    let saved = patch_settings(&partial).await?;
    session.acknowledge_profile_create(&partial, saved)
}

pub async fn patch_settings(partial: &serde_json::Value) -> Result<ApiSettings, String> {
    if is_tauri() {
        #[derive(Serialize)]
        struct CmdPatchSettingsArgs<'a> { partial: &'a serde_json::Value }
        tauri_invoke_with_args("cmd_patch_settings", &CmdPatchSettingsArgs { partial }).await
    } else {
        patch_json(&format!("{}/api/settings", get_api_base()), partial).await
    }
}

pub async fn save_settings(settings: &ApiSettings) -> Result<ApiSettings, String> {
    if is_tauri() {
        // Use Tauri IPC
        #[derive(Serialize)]
        struct CmdPutSettingsArgs {
            config: ApiSettings,
        }
        let args = CmdPutSettingsArgs {
            config: settings.clone(),
        };
        tauri_invoke_with_args("cmd_put_settings", &args).await
    } else {
        // Fallback to HTTP
        put_json(&format!("{}/api/settings", get_api_base()), settings).await
    }
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
// Extended types (ApiWorktree, ApiCosts, ApiCostSession, ApiMcpServer, ApiMemoryEntry, etc.)
// imported from at-api-types crate

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
    if is_tauri() {
        #[derive(serde::Serialize)]
        struct ListWorktreesArgs {
            limit: Option<usize>,
            offset: Option<usize>,
        }
        let args = ListWorktreesArgs {
            limit: None,
            offset: None,
        };
        tauri_invoke_with_args("cmd_list_worktrees", &args).await
    } else {
        // Fallback to HTTP
        fetch_json(&format!("{}/api/worktrees", get_api_base())).await
    }
}

pub async fn delete_worktree(id: &str) -> Result<(), String> {
    if is_tauri() {
        #[derive(serde::Serialize)]
        struct DeleteWorktreeArgs {
            id: String,
        }
        let args = DeleteWorktreeArgs { id: id.to_string() };
        let _result: String = tauri_invoke_with_args("cmd_delete_worktree", &args).await?;
        Ok(())
    } else {
        // Fallback to HTTP
        delete_request(&format!("{}/api/worktrees/{id}", get_api_base())).await
    }
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

pub async fn convert_idea_to_task(idea_id: &str) -> Result<serde_json::Value, String> {
    post_empty(&format!(
        "{}/api/ideation/ideas/{idea_id}/convert",
        get_api_base()
    ))
    .await
}

pub async fn fetch_insights_sessions() -> Result<Vec<ApiInsightsSession>, String> {
    if is_tauri() {
        let result: serde_json::Value = tauri_invoke("cmd_insights_list_sessions").await?;
        serde_json::from_value(result).map_err(|e| format!("Parse error: {e}"))
    } else {
        // Fallback to HTTP
        fetch_json(&format!("{}/api/insights/sessions", get_api_base())).await
    }
}

pub async fn fetch_insights_messages(session_id: &str) -> Result<Vec<ApiInsightsMessage>, String> {
    if is_tauri() {
        #[derive(serde::Serialize)]
        struct GetMessagesArgs {
            id: String,
        }
        let args = GetMessagesArgs {
            id: session_id.to_string(),
        };
        let result: serde_json::Value =
            tauri_invoke_with_args("cmd_insights_get_messages", &args).await?;
        serde_json::from_value(result).map_err(|e| format!("Parse error: {e}"))
    } else {
        // Fallback to HTTP
        fetch_json(&format!(
            "{}/api/insights/sessions/{session_id}/messages",
            get_api_base()
        ))
        .await
    }
}

pub async fn send_insights_message(
    session_id: &str,
    content: &str,
) -> Result<ApiInsightsMessage, String> {
    if is_tauri() {
        #[derive(serde::Serialize)]
        struct AddMessageArgs {
            id: String,
            content: String,
        }
        let args = AddMessageArgs {
            id: session_id.to_string(),
            content: content.to_string(),
        };
        let _success: bool = tauri_invoke_with_args("cmd_insights_add_message", &args).await?;

        // Re-fetch messages to pick up any server-side assistant reply that may have been generated.
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
    } else {
        // Fallback to HTTP
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

// ── GitLab API types ──
// ApiGithubIssue, ApiGithubPr imported from at-api-types crate

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

// ── GitHub API functions ──

pub async fn fetch_github_issues() -> Result<Vec<ApiGithubIssue>, String> {
    if is_tauri() {
        #[derive(serde::Serialize)]
        struct ListGithubIssuesArgs {
            state_filter: Option<String>,
            labels: Option<String>,
            limit: Option<usize>,
            offset: Option<usize>,
        }
        let args = ListGithubIssuesArgs {
            state_filter: None,
            labels: None,
            limit: None,
            offset: None,
        };
        tauri_invoke_with_args("cmd_list_github_issues", &args).await
    } else {
        // Fallback to HTTP
        fetch_json(&format!("{}/api/github/issues", get_api_base())).await
    }
}

pub async fn fetch_github_prs() -> Result<Vec<ApiGithubPr>, String> {
    if is_tauri() {
        #[derive(serde::Serialize)]
        struct ListGithubPrsArgs {
            state_filter: Option<String>,
            limit: Option<usize>,
            offset: Option<usize>,
        }
        let args = ListGithubPrsArgs {
            state_filter: None,
            limit: None,
            offset: None,
        };
        tauri_invoke_with_args("cmd_list_github_prs", &args).await
    } else {
        // Fallback to HTTP
        fetch_json(&format!("{}/api/github/prs", get_api_base())).await
    }
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
    if is_tauri() {
        let result: (String, u64, u64) = tauri_invoke("cmd_sync_github_issues").await?;
        Ok(serde_json::json!({
            "message": result.0,
            "imported_count": result.1,
            "statuses_synced": result.2
        }))
    } else {
        // Fallback to HTTP
        post_empty(&format!("{}/api/github/sync", get_api_base())).await
    }
}

pub async fn import_issue_as_bead(issue_number: u32) -> Result<ApiBead, String> {
    if is_tauri() {
        #[derive(serde::Serialize)]
        struct ImportGithubIssueArgs {
            issue_number: u64,
        }
        let args = ImportGithubIssueArgs {
            issue_number: issue_number as u64,
        };
        tauri_invoke_with_args("cmd_import_github_issue", &args).await
    } else {
        // Fallback to HTTP
        post_empty(&format!(
            "{}/api/github/issues/{issue_number}/import",
            get_api_base()
        ))
        .await
    }
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
// ApiChangelogSection, ApiChangelogEntry imported from at-api-types crate

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
    if is_tauri() {
        // Use Tauri IPC
        #[derive(Serialize)]
        struct UpdateBeadArgs {
            id: String,
            title: String,
            description: Option<String>,
            lane: Option<String>,
            status: Option<String>,
        }
        let args = UpdateBeadArgs {
            id: id.to_string(),
            title: bead.title.clone(),
            description: bead.description.clone(),
            lane: Some(bead.lane.clone()),
            status: Some(bead.status.clone()),
        };
        tauri_invoke_with_args("cmd_update_bead", &args).await
    } else {
        // Fallback to HTTP
        put_json(&format!("{}/api/beads/{id}", get_api_base()), bead).await
    }
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

#[cfg(test)]
mod settings_edit_tests {
    use super::*;
    use serde_json::json;

    fn settings(value: serde_json::Value) -> ApiSettings {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn ordinary_edit_omits_profiles_unloaded_sections_and_projection_defaults() {
        let loaded = settings(json!({
            "api_profiles":{"profiles":[{"name":"private", "base_url":"https://example.invalid", "api_key_env":"PRIVATE_KEY"}]},
            "general":{"project_name":"keep"}, "display":{"font_size":19},
            "paths":{"auto_claude_path":"/keep"}, "updates":{"version":"9.9"}
        }));
        // Represents the existing editor's lossy projection (rounding/defaults).
        let initial = ApiSettings::default();
        let session = SettingsEditSession::new(loaded, initial.clone());
        assert_eq!(session.patch(&initial).unwrap(), json!({}));
        let mut edited = initial;
        edited.notifications.sound_enabled = !edited.notifications.sound_enabled;
        let patch = settings_edit_patch(Some(&session), &edited).unwrap();
        assert_eq!(patch, json!({"notifications":{"sound_enabled":edited.notifications.sound_enabled}}));
        for omitted in ["api_profiles", "general", "display", "paths", "updates", "security", "terminal"] {
            assert!(patch.get(omitted).is_none(), "unexpected replacement: {omitted}");
        }
    }

    #[test]
    fn missing_initial_load_cannot_prepare_a_default_save() {
        assert!(settings_edit_patch(None, &ApiSettings::default()).is_err());
    }

    #[test]
    fn clearing_an_edited_optional_leaf_sends_null() {
        let initial = settings(json!({"integrations":{"github_owner":"old-org"}}));
        let session = SettingsEditSession::new(initial.clone(), initial.clone());
        let mut edited = initial;
        edited.integrations.github_owner = None;
        assert_eq!(session.patch(&edited).unwrap(), json!({"integrations":{"github_owner":null}}));
    }

    #[test]
    fn phase_edits_use_identity_preserve_unknown_and_omit_unchanged_arrays() {
        let loaded = settings(json!({"agent_profile":{"phase_configs":[
            {"phase":"Unknown future phase","model":"keep","thinking_level":"keep"},
            {"phase":"Ideation","model":"stored-idea","thinking_level":"stored-thinking"},
            {"phase":"Utility","model":"stored-utility","thinking_level":"stored-low"}
        ]}}));
        let initial = settings(json!({"agent_profile":{"phase_configs":[
            {"phase":"Utility","model":"projection","thinking_level":"default"},
            {"phase":"Ideation","model":"stored-idea","thinking_level":"stored-thinking"}
        ]}}));
        let session = SettingsEditSession::new(loaded.clone(), initial.clone());
        assert_eq!(session.patch(&initial).unwrap(), json!({}));
        let mut reordered = initial.clone();
        reordered.agent_profile.phase_configs.reverse();
        assert_eq!(session.patch(&reordered).unwrap(), json!({}));
        reordered.agent_profile.phase_configs[0].model = "edited-idea".into();
        let patch = session.patch(&reordered).unwrap();
        let phases = &patch["agent_profile"]["phase_configs"];
        assert_eq!(phases[0]["phase"], "Unknown future phase");
        assert_eq!(phases[0]["model"], "keep");
        assert_eq!(phases[1]["model"], "edited-idea");
        assert_eq!(phases[1]["thinking_level"], "stored-thinking");
        assert_eq!(phases[2]["model"], "stored-utility");
        assert_eq!(phases[2]["thinking_level"], "stored-low");
        let mut duplicate = loaded;
        duplicate.agent_profile.phase_configs.push(duplicate.agent_profile.phase_configs[1].clone());
        assert!(SettingsEditSession::new(duplicate, initial).patch(&reordered).is_err());
    }

    #[test]
    fn successful_acknowledgement_does_not_mark_later_unsent_edits_saved() {
        let initial = ApiSettings::default();
        let mut submitted = initial.clone();
        submitted.display.font_size = 18;
        let mut later = submitted.clone();
        later.language.interface_language = "fr".into();
        // Same submitted-draft/returned-snapshot binding as the Save handler.
        let acknowledged = SettingsEditSession::new(submitted.clone(), submitted);
        assert_eq!(acknowledged.patch(&later).unwrap(), json!({"language":{"interface_language":"fr"}}));
        // Failed requests keep the old session, so the original edit remains dirty.
        let failed = SettingsEditSession::new(initial.clone(), initial);
        assert_eq!(failed.patch(&later).unwrap()["display"]["font_size"], 18);
    }
    #[test]
    fn profile_creation_preserves_existing_list_and_ordinary_dirty_baseline() {
        let mut loaded = ApiSettings::default();
        loaded.api_profiles.profiles.push(ApiApiProfileEntry {
            name: "existing".into(), base_url: "https://existing.invalid".into(), api_key_env: "EXISTING_KEY".into(),
        });
        let initial = ApiSettings::default();
        let session = SettingsEditSession::new(loaded.clone(), initial.clone());
        let mut dirty_before = initial.clone();
        dirty_before.display.font_size = 23;
        let submitted = ApiApiProfileEntry {
            name: "new".into(), base_url: "https://new.invalid".into(), api_key_env: "NEW_KEY".into(),
        };
        let partial = session.profile_create_patch(&submitted).unwrap();
        assert_eq!(partial.as_object().unwrap().len(), 1);
        assert_eq!(partial["api_profiles"]["profiles"][0]["name"], "existing");
        assert_eq!(partial["api_profiles"]["profiles"][1]["name"], "new");
        assert_eq!(session.profiles().len(), 1, "preparing is not acknowledgement");
        let mut response = loaded;
        response.api_profiles.profiles.push(submitted.clone());
        // Unrelated fields in a full response must not acknowledge ordinary edits.
        response.display.font_size = 23;
        let saved = session.acknowledge_profile_create(&partial, response).unwrap();
        assert_eq!(saved.profiles(), &[session.profiles()[0].clone(), submitted]);
        assert_eq!(saved.patch(&dirty_before).unwrap(), json!({"display":{"font_size":23}}));
        let mut later = dirty_before;
        later.language.interface_language = "fr".into();
        assert_eq!(saved.patch(&later).unwrap(), json!({"display":{"font_size":23},"language":{"interface_language":"fr"}}));
        assert!(saved.patch(&later).unwrap().get("api_profiles").is_none());
        let second = ApiApiProfileEntry { name: "next".into(), ..Default::default() };
        assert_eq!(saved.profile_create_patch(&second).unwrap()["api_profiles"]["profiles"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn profile_creation_rejects_empty_name_and_unacknowledged_response() {
        let initial = ApiSettings::default();
        let session = SettingsEditSession::new(initial.clone(), initial);
        assert!(session.profile_create_patch(&ApiApiProfileEntry {name: "  ".into(), ..Default::default()}).is_err());
        let profile = ApiApiProfileEntry { name: "only".into(), ..Default::default() };
        let partial = session.profile_create_patch(&profile).unwrap();
        // A success-shaped but missing/wrong persisted list is not an acknowledgement.
        assert!(session.acknowledge_profile_create(&partial, ApiSettings::default()).is_err());
        assert!(session.profiles().is_empty());
        assert_eq!(profile.name, "only", "preparation/response rejection must not mutate form data");
    }

}

#[cfg(all(test, target_arch = "wasm32"))]
mod settings_transport_tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    use wasm_bindgen_test::*;
    wasm_bindgen_test_configure!(run_in_browser);

    struct RestoreProperty { object: js_sys::Object, key: JsValue, had: bool, old: JsValue }
    impl RestoreProperty {
        fn new(object: &js_sys::Object, name: &str) -> Self {
            let key = JsValue::from_str(name);
            Self { object: object.clone(), had: js_sys::Reflect::has(object, &key).unwrap(),
                old: js_sys::Reflect::get(object, &key).unwrap(), key }
        }
    }
    impl Drop for RestoreProperty {
        fn drop(&mut self) {
            if self.had { let _ = js_sys::Reflect::set(&self.object, &self.key, &self.old); }
            else { let _ = js_sys::Reflect::delete_property(&self.object, &self.key); }
        }
    }

    // Calls the production API entrypoints with controlled browser/IPC boundaries.
    // Separate backend tests exercise disk persistence; this is not a desktop launch.
    #[wasm_bindgen_test]
    async fn ordinary_save_uses_patch_on_http_and_partial_on_tauri_reset_uses_put() {
        let window: js_sys::Object = web_sys::window().unwrap().unchecked_into();
        let _restore_tauri = RestoreProperty::new(&window, "__TAURI__");
        let _restore_fetch = RestoreProperty::new(&window, "fetch");
        js_sys::Reflect::delete_property(&window, &JsValue::from_str("__TAURI__")).unwrap();
        let requests = Rc::new(RefCell::new(Vec::<Request>::new()));
        let captured = requests.clone();
        let fetch = Closure::wrap(Box::new(move |request: Request| -> js_sys::Promise {
            captured.borrow_mut().push(request);
            let response = Response::new_with_opt_str(Some("{}")).unwrap();
            js_sys::Promise::resolve(response.as_ref())
        }) as Box<dyn FnMut(Request) -> js_sys::Promise>);
        js_sys::Reflect::set(&window, &JsValue::from_str("fetch"), fetch.as_ref()).unwrap();
        let initial = ApiSettings::default();
        let session = SettingsEditSession::new(initial.clone(), initial.clone());
        let mut edited = initial;
        edited.display.font_size = 18;
        let patch = settings_edit_patch(Some(&session), &edited).unwrap();
        patch_settings(&patch).await.unwrap();
        let request = requests.borrow_mut().remove(0);
        assert_eq!(request.method(), "PATCH");
        assert!(request.url().ends_with("/api/settings"));
        let text = JsFuture::from(request.text().unwrap()).await.unwrap().as_string().unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&text).unwrap(), patch);
        save_settings(&ApiSettings::default()).await.unwrap();
        assert_eq!(requests.borrow_mut().remove(0).method(), "PUT");

        let calls = Rc::new(RefCell::new(Vec::<(String, serde_json::Value)>::new()));
        let captured = calls.clone();
        let invoke = Closure::wrap(Box::new(move |command: String, args: JsValue| -> js_sys::Promise {
            captured.borrow_mut().push((command, serde_wasm_bindgen::from_value(args).unwrap()));
            js_sys::Promise::resolve(&js_sys::Object::new())
        }) as Box<dyn FnMut(String, JsValue) -> js_sys::Promise>);
        let tauri = js_sys::Object::new();
        let namespace = js_sys::Object::new();
        js_sys::Reflect::set(&tauri, &JsValue::from_str("invoke"), invoke.as_ref()).unwrap();
        js_sys::Reflect::set(&namespace, &JsValue::from_str("tauri"), &tauri).unwrap();
        js_sys::Reflect::set(&window, &JsValue::from_str("__TAURI__"), &namespace).unwrap();
        patch_settings(&patch).await.unwrap();
        let (command, args) = calls.borrow_mut().remove(0);
        assert_eq!(command, "cmd_patch_settings");
        assert_eq!(args, serde_json::json!({"partial":patch}));
        assert!(requests.borrow().is_empty(), "Tauri save must not fall back to HTTP");
        save_settings(&ApiSettings::default()).await.unwrap();
        let (command, args) = calls.borrow_mut().remove(0);
        assert_eq!(command, "cmd_put_settings");
        assert!(args.get("config").is_some());
        assert!(args.get("partial").is_none());
    }
    #[wasm_bindgen_test]
    async fn profile_create_uses_real_patch_routes_and_requires_acknowledgement() {
        let window: js_sys::Object = web_sys::window().unwrap().unchecked_into();
        let _restore_tauri = RestoreProperty::new(&window, "__TAURI__");
        let _restore_fetch = RestoreProperty::new(&window, "fetch");
        js_sys::Reflect::delete_property(&window, &JsValue::from_str("__TAURI__")).unwrap();
        let mut loaded = ApiSettings::default();
        loaded.api_profiles.profiles.push(ApiApiProfileEntry { name: "existing".into(), ..Default::default() });
        let session = SettingsEditSession::new(loaded.clone(), ApiSettings::default());
        let entry = ApiApiProfileEntry { name: "new".into(), base_url: "https://example.invalid".into(), api_key_env: "NEW_KEY".into() };
        let expected = session.profile_create_patch(&entry).unwrap();
        let mut response = loaded;
        response.api_profiles.profiles.push(entry.clone());
        let response_json = serde_json::to_string(&response).unwrap();
        let requests = Rc::new(RefCell::new(Vec::<Request>::new()));
        let fail = Rc::new(RefCell::new(false));
        let captured = requests.clone();
        let response_body = response_json.clone();
        let fail_fetch = fail.clone();
        let fetch = Closure::wrap(Box::new(move |request: Request| -> js_sys::Promise {
            captured.borrow_mut().push(request);
            if *fail_fetch.borrow() {
                js_sys::Promise::reject(&JsValue::from_str("controlled transport failure"))
            } else {
                js_sys::Promise::resolve(Response::new_with_opt_str(Some(&response_body)).unwrap().as_ref())
            }
        }) as Box<dyn FnMut(Request) -> js_sys::Promise>);
        js_sys::Reflect::set(&window, &JsValue::from_str("fetch"), fetch.as_ref()).unwrap();
        assert!(create_api_profile(None, &entry).await.is_err());
        assert!(requests.borrow().is_empty(), "failed load must not send a request");
        let saved = create_api_profile(Some(&session), &entry).await.unwrap();
        assert_eq!(saved.profiles().len(), 2);
        let request = requests.borrow_mut().remove(0);
        assert_eq!(request.method(), "PATCH");
        assert!(request.url().ends_with("/api/settings"));
        let text = JsFuture::from(request.text().unwrap()).await.unwrap().as_string().unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&text).unwrap(), expected);
        *fail.borrow_mut() = true;
        assert!(create_api_profile(Some(&session), &entry).await.is_err());
        assert_eq!(session.profiles().len(), 1);
        assert_eq!(entry.name, "new");
        requests.borrow_mut().clear();

        let calls = Rc::new(RefCell::new(Vec::<(String, serde_json::Value)>::new()));
        let captured = calls.clone();
        let fail_invoke = fail.clone();
        let invoke = Closure::wrap(Box::new(move |command: String, args: JsValue| -> js_sys::Promise {
            captured.borrow_mut().push((command, serde_wasm_bindgen::from_value(args).unwrap()));
            if *fail_invoke.borrow() {
                js_sys::Promise::reject(&JsValue::from_str("controlled IPC failure"))
            } else {
                js_sys::Promise::resolve(&js_sys::JSON::parse(&response_json).unwrap())
            }
        }) as Box<dyn FnMut(String, JsValue) -> js_sys::Promise>);
        let tauri = js_sys::Object::new();
        let namespace = js_sys::Object::new();
        js_sys::Reflect::set(&tauri, &JsValue::from_str("invoke"), invoke.as_ref()).unwrap();
        js_sys::Reflect::set(&namespace, &JsValue::from_str("tauri"), &tauri).unwrap();
        js_sys::Reflect::set(&window, &JsValue::from_str("__TAURI__"), &namespace).unwrap();
        *fail.borrow_mut() = false;
        let saved = create_api_profile(Some(&session), &entry).await.unwrap();
        assert_eq!(saved.profiles().len(), 2);
        let (command, args) = calls.borrow_mut().remove(0);
        assert_eq!(command, "cmd_patch_settings");
        assert_eq!(args, serde_json::json!({"partial":expected}));
        *fail.borrow_mut() = true;
        assert!(create_api_profile(Some(&session), &entry).await.is_err());
        assert_eq!(session.profiles().len(), 1);
        assert_eq!(entry.name, "new");
        assert!(requests.borrow().is_empty(), "Tauri must not silently fall back to HTTP");
    }

}
