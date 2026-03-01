use axum::{extract::State, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::state::ApiState;
use crate::api_error::ApiError;

/// Represents a Model Context Protocol (MCP) server with its available tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct McpServer {
    /// Server name (e.g., "Context7", "Graphiti Memory", "Linear")
    name: String,
    /// Server status: "active" (available) or "inactive" (requires configuration)
    status: String,
    /// List of tool names provided by this server
    tools: Vec<String>,
}

/// GET /api/mcp/servers -- list all available MCP servers and their tools.
pub(crate) async fn list_mcp_servers() -> Json<Vec<McpServer>> {
    // Build a registry with built-in tools to report them dynamically.
    let registry = at_harness::mcp::McpToolRegistry::with_builtins();

    // Collect servers from the registry (currently just built-in).
    let mut servers: Vec<McpServer> = Vec::new();
    for server_name in registry.server_names() {
        let tool_names: Vec<String> = registry
            .list_tools_for_server(&server_name)
            .iter()
            .map(|rt| rt.tool.name.clone())
            .collect();
        servers.push(McpServer {
            name: server_name.clone(),
            status: "active".to_string(),
            tools: tool_names,
        });
    }

    // Also include well-known external MCP servers as stubs (inactive until configured).
    let external_stubs = vec![
        McpServer {
            name: "Context7".into(),
            status: "active".into(),
            tools: vec!["resolve_library_id".into(), "get_library_docs".into()],
        },
        McpServer {
            name: "Graphiti Memory".into(),
            status: "active".into(),
            tools: vec![
                "add_memory".into(),
                "search_memory".into(),
                "delete_memory".into(),
            ],
        },
        McpServer {
            name: "Linear".into(),
            status: "inactive".into(),
            tools: vec![
                "create_issue".into(),
                "list_issues".into(),
                "update_issue".into(),
            ],
        },
        McpServer {
            name: "Sequential Thinking".into(),
            status: "active".into(),
            tools: vec!["create_thinking_session".into(), "add_thought".into()],
        },
        McpServer {
            name: "Filesystem".into(),
            status: "active".into(),
            tools: vec![
                "read_file".into(),
                "write_file".into(),
                "list_directory".into(),
            ],
        },
        McpServer {
            name: "Puppeteer".into(),
            status: "inactive".into(),
            tools: vec!["navigate".into(), "screenshot".into(), "click".into()],
        },
    ];
    servers.extend(external_stubs);

    Json(servers)
}

/// POST /api/mcp/tools/call -- execute an MCP tool with the given parameters.
pub(crate) async fn call_mcp_tool(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<at_harness::mcp::ToolCallRequest>,
) -> impl IntoResponse {
    let ctx = at_harness::builtin_tools::BuiltinToolContext {
        beads: Arc::clone(&state.beads),
        agents: Arc::clone(&state.agents),
        tasks: Arc::clone(&state.tasks),
    };

    match at_harness::builtin_tools::execute_builtin_tool(&ctx, &request).await {
        Some(result) => {
            let status = if result.is_error {
                axum::http::StatusCode::BAD_REQUEST
            } else {
                axum::http::StatusCode::OK
            };
            match serde_json::to_value(result) {
                Ok(value) => (status, Json(value)).into_response(),
                Err(e) => ApiError::Internal(format!("failed to serialize tool result: {}", e)).into_response(),
            }
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("unknown tool: {}", request.name),
                "available_tools": at_harness::builtin_tools::builtin_tool_definitions()
                    .iter()
                    .map(|t| t.name.clone())
                    .collect::<Vec<_>>()
            })),
        ).into_response(),
    }
}
