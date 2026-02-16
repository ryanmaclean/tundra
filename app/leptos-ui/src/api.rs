use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

const API_BASE: &str = "http://localhost:9090";

// ── Generic fetch helpers ──

async fn fetch_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let opts = RequestInit::new();
    opts.set_method("GET");

    let request =
        Request::new_with_str_and_init(url, &opts).map_err(|e| format!("{:?}", e))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{:?}", e))?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{:?}", e))?;

    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{:?}", e))?;
    let json = JsFuture::from(resp.json().map_err(|e| format!("{:?}", e))?)
        .await
        .map_err(|e| format!("{:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("{:?}", e))
}

async fn post_json<T: Serialize, R: for<'de> Deserialize<'de>>(
    url: &str,
    body: &T,
) -> Result<R, String> {
    let body_str = serde_json::to_string(body).map_err(|e| format!("{:?}", e))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&JsValue::from_str(&body_str));

    let request =
        Request::new_with_str_and_init(url, &opts).map_err(|e| format!("{:?}", e))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{:?}", e))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{:?}", e))?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{:?}", e))?;

    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{:?}", e))?;
    let json = JsFuture::from(resp.json().map_err(|e| format!("{:?}", e))?)
        .await
        .map_err(|e| format!("{:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("{:?}", e))
}

// ── API response types (matching backend JSON) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiBead {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub status: String,
    pub lane: String,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiAgent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub role: String,
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
    #[serde(default)]
    pub uptime_secs: u64,
    #[serde(default)]
    pub agent_count: usize,
    #[serde(default)]
    pub bead_count: usize,
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
    let body_str = serde_json::to_string(body).map_err(|e| format!("{:?}", e))?;

    let opts = RequestInit::new();
    opts.set_method("PUT");
    opts.set_body(&JsValue::from_str(&body_str));

    let request =
        Request::new_with_str_and_init(url, &opts).map_err(|e| format!("{:?}", e))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{:?}", e))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{:?}", e))?;

    let window = web_sys::window().ok_or("no global window")?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{:?}", e))?;

    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{:?}", e))?;
    let json = JsFuture::from(resp.json().map_err(|e| format!("{:?}", e))?)
        .await
        .map_err(|e| format!("{:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("{:?}", e))
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
    pub mask_api_keys: bool,
    #[serde(default)]
    pub auto_lock_timeout_mins: u32,
    #[serde(default)]
    pub sandbox_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiIntegrationSettings {
    #[serde(default)]
    pub github_token: Option<String>,
    #[serde(default)]
    pub gitlab_token: Option<String>,
    #[serde(default)]
    pub linear_api_key: Option<String>,
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
}

// ── Public API functions ──

pub async fn fetch_beads() -> Result<Vec<ApiBead>, String> {
    fetch_json(&format!("{API_BASE}/api/beads")).await
}

pub async fn fetch_agents() -> Result<Vec<ApiAgent>, String> {
    fetch_json(&format!("{API_BASE}/api/agents")).await
}

pub async fn fetch_kpi() -> Result<ApiKpi, String> {
    fetch_json(&format!("{API_BASE}/api/kpi")).await
}

pub async fn fetch_status() -> Result<ApiStatus, String> {
    fetch_json(&format!("{API_BASE}/api/status")).await
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
    post_json(&format!("{API_BASE}/api/beads"), &body).await
}

pub async fn update_bead_status(id: &str, status: &str) -> Result<ApiBead, String> {
    let body = UpdateStatusRequest {
        status: status.to_string(),
    };
    post_json(&format!("{API_BASE}/api/beads/{id}/status"), &body).await
}

pub async fn fetch_settings() -> Result<ApiSettings, String> {
    fetch_json(&format!("{API_BASE}/api/settings")).await
}

pub async fn save_settings(settings: &ApiSettings) -> Result<ApiSettings, String> {
    put_json(&format!("{API_BASE}/api/settings"), settings).await
}
