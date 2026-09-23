// ---------------------------------------------------------------------------
// MCP HTTP+SSE Transport
//
// Implements the Model Context Protocol SSE transport so Claude Code (and
// other MCP clients) can connect to at-tundra as an MCP server.
//
// Spec: https://spec.modelcontextprotocol.io/specification/2024-11-05/basic/transports/
//
// Handshake:
//   1. Client opens GET /mcp/sse   → SSE stream
//   2. Server sends:  event: endpoint\ndata: /mcp/messages?session_id=<uuid>
//   3. Client POSTs initialize to /mcp/messages?session_id=<uuid>
//   4. Server sends initialize result on SSE stream
//   5. Client sends notifications/initialized (no-op)
//   6. Client may call tools/list and tools/call
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    Json,
};
use futures_util::stream::Stream;
use serde::Deserialize;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

use at_core::types::{Bead, BeadStatus, KpiSnapshot, Lane};
use at_harness::mcp::{
    error_codes, InitializeResult, JsonRpcRequest, JsonRpcResponse, McpTool, ServerCapabilities,
    ServerInfo, ToolAnnotations, ToolCallRequest, ToolCallResult, ToolsCapability,
    MCP_PROTOCOL_VERSION,
};

use super::state::ApiState;

// ---------------------------------------------------------------------------
// Session store — maps session_id → mpsc sender for SSE channel
// ---------------------------------------------------------------------------

/// Thread-safe map from session UUID to SSE message sender.
pub type McpSessionStore = Arc<RwLock<HashMap<Uuid, mpsc::Sender<String>>>>;

/// Create a new, empty session store.
pub fn new_session_store() -> McpSessionStore {
    Arc::new(RwLock::new(HashMap::new()))
}

// ---------------------------------------------------------------------------
// GET /mcp/sse
// ---------------------------------------------------------------------------

/// Open an SSE connection. The server immediately sends an `endpoint` event
/// telling the client where to POST JSON-RPC messages.
pub async fn handle_sse(
    State(state): State<Arc<ApiState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id = Uuid::new_v4();
    let (tx, mut rx) = mpsc::channel::<String>(64);

    // Register the sender in the session store.
    {
        let mut sessions = state.mcp_sessions.write().await;
        sessions.insert(session_id, tx);
    }

    info!(session_id = %session_id, "MCP SSE session opened");

    // Build the SSE stream from the mpsc receiver.
    let stream = async_stream::stream! {
        // First event: tell the client where to POST.
        let endpoint = format!("/mcp/messages?session_id={}", session_id);
        yield Ok(Event::default().event("endpoint").data(endpoint));

        // Relay any messages the server sends on the channel.
        while let Some(msg) = rx.recv().await {
            yield Ok(Event::default().data(msg));
        }

        // Channel closed — clean up session.
        debug!(session_id = %session_id, "MCP SSE session closed");
    };

    // Spawn a cleanup task: if the SSE consumer drops, remove the session.
    let sessions_cleanup = Arc::clone(&state.mcp_sessions);
    tokio::spawn(async move {
        // The stream owns rx; when the stream is dropped the channel closes.
        // We rely on the channel sender being removed from the store when the
        // POST handler gets a SendError (channel closed) rather than here, so
        // this cleanup is just a belt-and-suspenders safety net.
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        let mut sessions = sessions_cleanup.write().await;
        sessions.remove(&session_id);
        debug!(session_id = %session_id, "MCP session TTL expired, removed");
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// POST /mcp/messages?session_id=<uuid>
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SessionQuery {
    session_id: Uuid,
}

/// Receive a JSON-RPC message from the client and send the response back on
/// the SSE channel for the given session.
pub async fn handle_message(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<SessionQuery>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let session_id = q.session_id;
    debug!(session_id = %session_id, method = %request.method, "MCP message received");

    let response = dispatch_request(&state, &request).await;

    // Notifications (no id) have no response.
    if request.id.is_none() && response.is_none() {
        return axum::http::StatusCode::ACCEPTED.into_response();
    }

    let rpc_response = response.unwrap_or_else(|| {
        JsonRpcResponse::error(request.id.clone(), error_codes::METHOD_NOT_FOUND, "Method not found")
    });

    let serialized = match serde_json::to_string(&rpc_response) {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to serialize MCP response: {}", e);
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Look up the SSE sender for this session.
    let sender = {
        let sessions = state.mcp_sessions.read().await;
        sessions.get(&session_id).cloned()
    };

    match sender {
        Some(tx) => {
            if tx.send(serialized).await.is_err() {
                warn!(session_id = %session_id, "MCP SSE channel closed, removing session");
                let mut sessions = state.mcp_sessions.write().await;
                sessions.remove(&session_id);
                return axum::http::StatusCode::GONE.into_response();
            }
            axum::http::StatusCode::ACCEPTED.into_response()
        }
        None => {
            warn!(session_id = %session_id, "MCP session not found");
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "session not found" })),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC method dispatcher
// ---------------------------------------------------------------------------

async fn dispatch_request(
    state: &Arc<ApiState>,
    request: &JsonRpcRequest,
) -> Option<JsonRpcResponse> {
    match request.method.as_str() {
        "initialize" => Some(handle_initialize(request)),
        "notifications/initialized" => {
            // No-op: client has finished initialization.
            None
        }
        "tools/list" => Some(handle_tools_list(request)),
        "tools/call" => Some(handle_tools_call(state, request).await),
        _ => Some(JsonRpcResponse::error(
            request.id.clone(),
            error_codes::METHOD_NOT_FOUND,
            format!("Method not found: {}", request.method),
        )),
    }
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

fn handle_initialize(request: &JsonRpcRequest) -> JsonRpcResponse {
    let result = InitializeResult {
        protocol_version: MCP_PROTOCOL_VERSION.to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability { list_changed: false }),
            resources: None,
            prompts: None,
        },
        server_info: ServerInfo {
            name: "at-tundra".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };

    match serde_json::to_value(&result) {
        Ok(v) => JsonRpcResponse::success(request.id.clone(), v),
        Err(e) => JsonRpcResponse::error(
            request.id.clone(),
            error_codes::INTERNAL_ERROR,
            format!("Failed to serialize initialize result: {e}"),
        ),
    }
}

// ---------------------------------------------------------------------------
// tools/list
// ---------------------------------------------------------------------------

fn handle_tools_list(request: &JsonRpcRequest) -> JsonRpcResponse {
    let tools = all_tool_definitions();
    let value = serde_json::json!({ "tools": tools });
    JsonRpcResponse::success(request.id.clone(), value)
}

/// Combine built-in tools with the 4 new MCP-specific tools.
fn all_tool_definitions() -> Vec<McpTool> {
    let mut tools = at_harness::builtin_tools::builtin_tool_definitions();
    tools.push(list_beads_tool());
    tools.push(get_kpi_tool());
    tools.push(create_bead_tool());
    tools.push(update_bead_status_tool());
    tools
}

// ---------------------------------------------------------------------------
// tools/call
// ---------------------------------------------------------------------------

async fn handle_tools_call(
    state: &Arc<ApiState>,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    let params = match &request.params {
        Some(p) => p.clone(),
        None => {
            return JsonRpcResponse::error(
                request.id.clone(),
                error_codes::INVALID_PARAMS,
                "Missing params for tools/call",
            );
        }
    };

    let tool_request: ToolCallRequest = match serde_json::from_value(params) {
        Ok(r) => r,
        Err(e) => {
            return JsonRpcResponse::error(
                request.id.clone(),
                error_codes::INVALID_PARAMS,
                format!("Invalid tool call params: {e}"),
            );
        }
    };

    let tool_name = tool_request.name.clone();
    info!(tool = %tool_name, "MCP tools/call");

    // Try MCP-specific tools first, then fall through to built-ins.
    let result = match tool_name.as_str() {
        "list_beads" => Some(exec_list_beads(state, &tool_request.arguments).await),
        "get_kpi" => Some(exec_get_kpi(state).await),
        "create_bead" => Some(exec_create_bead(state, &tool_request.arguments).await),
        "update_bead_status" => {
            Some(exec_update_bead_status(state, &tool_request.arguments).await)
        }
        _ => {
            // Fall back to built-in tools (run_task, list_agents, manage_beads, etc.)
            let ctx = at_harness::builtin_tools::BuiltinToolContext {
                beads: Arc::clone(&state.beads),
                agents: Arc::clone(&state.agents),
                tasks: Arc::clone(&state.tasks),
            };
            at_harness::builtin_tools::execute_builtin_tool(&ctx, &tool_request).await
        }
    };

    match result {
        Some(r) => match serde_json::to_value(r) {
            Ok(v) => JsonRpcResponse::success(request.id.clone(), v),
            Err(e) => JsonRpcResponse::error(
                request.id.clone(),
                error_codes::INTERNAL_ERROR,
                format!("Failed to serialize tool result: {e}"),
            ),
        },
        None => JsonRpcResponse::error(
            request.id.clone(),
            error_codes::METHOD_NOT_FOUND,
            format!("Unknown tool: {tool_name}"),
        ),
    }
}

// ---------------------------------------------------------------------------
// New tool definitions
// ---------------------------------------------------------------------------

fn list_beads_tool() -> McpTool {
    McpTool {
        name: "list_beads".to_string(),
        description: "List all beads with their id, title, status, and priority.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "status_filter": {
                    "type": "string",
                    "enum": ["backlog", "hooked", "slung", "review", "done", "failed", "escalated"],
                    "description": "Optional filter by bead status"
                }
            }
        }),
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        }),
    }
}

fn get_kpi_tool() -> McpTool {
    McpTool {
        name: "get_kpi".to_string(),
        description: "Return the current KPI snapshot: bead counts by status and active agent count.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        }),
    }
}

fn create_bead_tool() -> McpTool {
    McpTool {
        name: "create_bead".to_string(),
        description: "Create a new bead (work item) in the backlog.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Title of the bead"
                },
                "description": {
                    "type": "string",
                    "description": "Optional detailed description"
                },
                "lane": {
                    "type": "string",
                    "enum": ["experimental", "standard", "critical"],
                    "description": "Work lane (default: standard)"
                }
            },
            "required": ["title"]
        }),
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(false),
            idempotent_hint: Some(false),
            open_world_hint: Some(false),
        }),
    }
}

fn update_bead_status_tool() -> McpTool {
    McpTool {
        name: "update_bead_status".to_string(),
        description: "Update the status of an existing bead.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "UUID of the bead to update"
                },
                "status": {
                    "type": "string",
                    "enum": ["backlog", "hooked", "slung", "review", "done", "failed", "escalated"],
                    "description": "Target status"
                }
            },
            "required": ["id", "status"]
        }),
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        }),
    }
}

// ---------------------------------------------------------------------------
// New tool executors
// ---------------------------------------------------------------------------

async fn exec_list_beads(state: &Arc<ApiState>, args: &serde_json::Value) -> ToolCallResult {
    let beads = state.beads.read().await;

    let status_filter: Option<BeadStatus> = args
        .get("status_filter")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_value::<BeadStatus>(serde_json::json!(s)).ok());

    let items: Vec<serde_json::Value> = beads
        .values()
        .filter(|b| status_filter.as_ref().is_none_or(|f| &b.status == f))
        .map(|b| {
            serde_json::json!({
                "id": b.id,
                "title": b.title,
                "status": b.status,
                "priority": b.priority,
                "lane": b.lane,
                "description": b.description,
            })
        })
        .collect();

    ToolCallResult::text(
        serde_json::json!({ "beads": items, "count": items.len() }).to_string(),
    )
}

async fn exec_get_kpi(state: &Arc<ApiState>) -> ToolCallResult {
    let kpi: KpiSnapshot = state.kpi.read().await.clone();
    match serde_json::to_string(&kpi) {
        Ok(s) => ToolCallResult::text(s),
        Err(e) => ToolCallResult::error(format!("Failed to serialize KPI: {e}")),
    }
}

async fn exec_create_bead(state: &Arc<ApiState>, args: &serde_json::Value) -> ToolCallResult {
    let title = match args.get("title").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return ToolCallResult::error("missing required parameter: title"),
    };

    let lane = args
        .get("lane")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_value::<Lane>(serde_json::json!(s)).ok())
        .unwrap_or(Lane::Standard);

    let mut bead = Bead::new(title, lane);
    bead.description = args
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);

    let bead_json = match serde_json::to_string(&bead) {
        Ok(j) => j,
        Err(e) => return ToolCallResult::error(format!("Failed to serialize bead: {e}")),
    };

    state.beads.write().await.insert(bead.id, bead);
    ToolCallResult::text(bead_json)
}

async fn exec_update_bead_status(state: &Arc<ApiState>, args: &serde_json::Value) -> ToolCallResult {
    let id_str = match args.get("id").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolCallResult::error("missing required parameter: id"),
    };

    let bead_id: Uuid = match id_str.parse() {
        Ok(u) => u,
        Err(_) => return ToolCallResult::error(format!("invalid UUID: {id_str}")),
    };

    let status_str = match args.get("status").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolCallResult::error("missing required parameter: status"),
    };

    let new_status: BeadStatus = match serde_json::from_value(serde_json::json!(status_str)) {
        Ok(s) => s,
        Err(_) => return ToolCallResult::error(format!("invalid status: {status_str}")),
    };

    let mut beads = state.beads.write().await;
    match beads.get_mut(&bead_id) {
        Some(bead) => {
            bead.status = new_status;
            bead.updated_at = chrono::Utc::now();
            let result = serde_json::json!({
                "id": bead.id,
                "title": bead.title,
                "status": bead.status,
            });
            ToolCallResult::text(result.to_string())
        }
        None => ToolCallResult::error(format!("bead not found: {id_str}")),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::EventBus;

    fn make_state() -> Arc<ApiState> {
        Arc::new(ApiState::new(EventBus::new()))
    }

    #[test]
    fn initialize_response_has_correct_protocol_version() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "initialize".to_string(),
            params: None,
        };
        let resp = handle_initialize(&req);
        assert!(resp.result.is_some());
        let r = resp.result.unwrap();
        assert_eq!(r["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(r["serverInfo"]["name"], "at-tundra");
    }

    #[test]
    fn tools_list_includes_all_tools() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "tools/list".to_string(),
            params: None,
        };
        let resp = handle_tools_list(&req);
        assert!(resp.result.is_some());
        let tools = resp.result.unwrap()["tools"].clone();
        let tool_names: Vec<String> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();

        // Built-ins
        assert!(tool_names.contains(&"run_task".to_string()));
        assert!(tool_names.contains(&"list_agents".to_string()));
        assert!(tool_names.contains(&"manage_beads".to_string()));
        assert!(tool_names.contains(&"get_build_status".to_string()));
        assert!(tool_names.contains(&"get_task_logs".to_string()));
        // New MCP-specific
        assert!(tool_names.contains(&"list_beads".to_string()));
        assert!(tool_names.contains(&"get_kpi".to_string()));
        assert!(tool_names.contains(&"create_bead".to_string()));
        assert!(tool_names.contains(&"update_bead_status".to_string()));
    }

    #[tokio::test]
    async fn exec_get_kpi_returns_json() {
        let state = make_state();
        let result = exec_get_kpi(&state).await;
        assert!(!result.is_error);
        let text = result.text_content().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(v.get("total_beads").is_some());
    }

    #[tokio::test]
    async fn exec_create_bead_and_list() {
        let state = make_state();
        let args = serde_json::json!({ "title": "Test bead", "lane": "standard" });
        let create_result = exec_create_bead(&state, &args).await;
        assert!(!create_result.is_error);

        let list_result = exec_list_beads(&state, &serde_json::json!({})).await;
        assert!(!list_result.is_error);
        let text = list_result.text_content().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(v["count"], 1);
    }

    #[tokio::test]
    async fn exec_update_bead_status_works() {
        let state = make_state();

        // Create a bead first
        let args = serde_json::json!({ "title": "Status test bead" });
        let create_result = exec_create_bead(&state, &args).await;
        let bead_json: serde_json::Value =
            serde_json::from_str(create_result.text_content().unwrap()).unwrap();
        let bead_id = bead_json["id"].as_str().unwrap();

        // Update its status
        let update_args = serde_json::json!({ "id": bead_id, "status": "hooked" });
        let result = exec_update_bead_status(&state, &update_args).await;
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(result.text_content().unwrap()).unwrap();
        assert_eq!(v["status"], "hooked");
    }

    #[tokio::test]
    async fn exec_update_bead_status_unknown_id() {
        let state = make_state();
        let args = serde_json::json!({ "id": Uuid::new_v4().to_string(), "status": "done" });
        let result = exec_update_bead_status(&state, &args).await;
        assert!(result.is_error);
    }
}
