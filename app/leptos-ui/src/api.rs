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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiCredentialStatus {
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub daemon_auth: bool,
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

pub async fn fetch_credential_status() -> Result<ApiCredentialStatus, String> {
    fetch_json(&format!("{API_BASE}/api/credentials/status")).await
}

// ── DELETE helper ──

async fn delete_request(url: &str) -> Result<(), String> {
    let opts = RequestInit::new();
    opts.set_method("DELETE");

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
    if resp.ok() {
        Ok(())
    } else {
        Err(format!("DELETE failed with status {}", resp.status()))
    }
}

async fn post_empty<R: for<'de> Deserialize<'de>>(url: &str) -> Result<R, String> {
    let opts = RequestInit::new();
    opts.set_method("POST");

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
    pub category: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SendInsightsMessageRequest {
    pub content: String,
}

// ── Additional public API functions ──

pub async fn stop_agent(id: &str) -> Result<ApiAgent, String> {
    post_empty(&format!("{API_BASE}/api/agents/{id}/stop")).await
}

pub async fn fetch_sessions() -> Result<Vec<ApiSession>, String> {
    fetch_json(&format!("{API_BASE}/api/sessions")).await
}

pub async fn fetch_convoys() -> Result<Vec<ApiConvoy>, String> {
    fetch_json(&format!("{API_BASE}/api/convoys")).await
}

pub async fn fetch_worktrees() -> Result<Vec<ApiWorktree>, String> {
    fetch_json(&format!("{API_BASE}/api/worktrees")).await
}

pub async fn delete_worktree(id: &str) -> Result<(), String> {
    delete_request(&format!("{API_BASE}/api/worktrees/{id}")).await
}

pub async fn fetch_costs() -> Result<ApiCosts, String> {
    fetch_json(&format!("{API_BASE}/api/costs")).await
}

pub async fn fetch_mcp_servers() -> Result<Vec<ApiMcpServer>, String> {
    fetch_json(&format!("{API_BASE}/api/mcp/servers")).await
}

pub async fn fetch_memory() -> Result<Vec<ApiMemoryEntry>, String> {
    fetch_json(&format!("{API_BASE}/api/memory")).await
}

pub async fn search_memory(query: &str) -> Result<Vec<ApiMemoryEntry>, String> {
    fetch_json(&format!("{API_BASE}/api/memory/search?q={query}")).await
}

pub async fn add_memory(category: &str, content: &str) -> Result<ApiMemoryEntry, String> {
    let body = AddMemoryRequest {
        category: category.to_string(),
        content: content.to_string(),
    };
    post_json(&format!("{API_BASE}/api/memory"), &body).await
}

pub async fn fetch_roadmap() -> Result<Vec<ApiRoadmapItem>, String> {
    fetch_json(&format!("{API_BASE}/api/roadmap")).await
}

pub async fn generate_roadmap() -> Result<Vec<ApiRoadmapItem>, String> {
    post_empty(&format!("{API_BASE}/api/roadmap/generate")).await
}

pub async fn fetch_ideas() -> Result<Vec<ApiIdea>, String> {
    fetch_json(&format!("{API_BASE}/api/ideation/ideas")).await
}

pub async fn generate_ideas() -> Result<Vec<ApiIdea>, String> {
    post_empty(&format!("{API_BASE}/api/ideation/generate")).await
}

pub async fn fetch_insights_sessions() -> Result<Vec<ApiInsightsSession>, String> {
    fetch_json(&format!("{API_BASE}/api/insights/sessions")).await
}

pub async fn fetch_insights_messages(session_id: &str) -> Result<Vec<ApiInsightsMessage>, String> {
    fetch_json(&format!("{API_BASE}/api/insights/sessions/{session_id}/messages")).await
}

pub async fn send_insights_message(session_id: &str, content: &str) -> Result<ApiInsightsMessage, String> {
    let body = SendInsightsMessageRequest {
        content: content.to_string(),
    };
    post_json(&format!("{API_BASE}/api/insights/sessions/{session_id}/messages"), &body).await
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

pub async fn fetch_notifications(unread_only: bool, limit: usize, offset: usize) -> Result<Vec<ApiNotification>, String> {
    let unread_param = if unread_only { "&unread=true" } else { "" };
    fetch_json(&format!(
        "{API_BASE}/api/notifications?limit={limit}&offset={offset}{unread_param}"
    )).await
}

pub async fn fetch_notification_count() -> Result<ApiNotificationCount, String> {
    fetch_json(&format!("{API_BASE}/api/notifications/count")).await
}

pub async fn mark_notification_read(id: &str) -> Result<serde_json::Value, String> {
    post_empty(&format!("{API_BASE}/api/notifications/{id}/read")).await
}

pub async fn mark_all_notifications_read() -> Result<serde_json::Value, String> {
    post_empty(&format!("{API_BASE}/api/notifications/read-all")).await
}

pub async fn delete_notification(id: &str) -> Result<(), String> {
    delete_request(&format!("{API_BASE}/api/notifications/{id}")).await
}

/// Return the WebSocket URL for event streaming.
pub fn events_ws_url() -> String {
    // Replace http:// with ws://
    let ws_base = API_BASE.replace("http://", "ws://").replace("https://", "wss://");
    format!("{ws_base}/api/events/ws")
}
