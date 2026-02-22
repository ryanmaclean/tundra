use serde_json::json;
use tracing::info;

use crate::mcp::{McpTool, ToolAnnotations, ToolCallRequest, ToolCallResult};

// ---------------------------------------------------------------------------
// Built-in "Auto Claude Tools" MCP server
//
// Always-enabled core tools: run_task, list_agents, manage_beads,
// get_build_status, get_task_logs.
// ---------------------------------------------------------------------------

/// Server name used when registering built-in tools.
pub const BUILTIN_SERVER_NAME: &str = "auto-claude-tools";

/// Return the complete list of built-in MCP tool definitions.
pub fn builtin_tool_definitions() -> Vec<McpTool> {
    vec![
        run_task_tool(),
        list_agents_tool(),
        manage_beads_tool(),
        get_build_status_tool(),
        get_task_logs_tool(),
    ]
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

fn run_task_tool() -> McpTool {
    McpTool {
        name: "run_task".to_string(),
        description: "Execute a task by its UUID. Transitions the task into the execution pipeline."
            .to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "UUID of the task to execute"
                }
            },
            "required": ["task_id"]
        }),
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(false),
            idempotent_hint: Some(false),
            open_world_hint: Some(false),
        }),
    }
}

fn list_agents_tool() -> McpTool {
    McpTool {
        name: "list_agents".to_string(),
        description: "List all registered agents with their roles, statuses, and metadata."
            .to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "status_filter": {
                    "type": "string",
                    "enum": ["active", "idle", "pending", "stopped", "unknown"],
                    "description": "Optional filter by agent status"
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

fn manage_beads_tool() -> McpTool {
    McpTool {
        name: "manage_beads".to_string(),
        description:
            "CRUD operations on beads. Supports list, get, create, and update_status actions."
                .to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "get", "create", "update_status"],
                    "description": "The operation to perform"
                },
                "bead_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "UUID of the bead (required for get, update_status)"
                },
                "title": {
                    "type": "string",
                    "description": "Title for creating a new bead (required for create)"
                },
                "description": {
                    "type": "string",
                    "description": "Optional description for creating a new bead"
                },
                "lane": {
                    "type": "string",
                    "enum": ["experimental", "standard", "critical"],
                    "description": "Lane for the new bead (default: standard)"
                },
                "status": {
                    "type": "string",
                    "enum": ["backlog", "hooked", "slung", "review", "done", "failed", "escalated"],
                    "description": "Target status (required for update_status)"
                }
            },
            "required": ["action"]
        }),
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(false),
            destructive_hint: Some(false),
            idempotent_hint: Some(false),
            open_world_hint: Some(false),
        }),
    }
}

fn get_build_status_tool() -> McpTool {
    McpTool {
        name: "get_build_status".to_string(),
        description: "Get the current build/task progress for a task or all active tasks."
            .to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Optional task UUID. If omitted, returns status of all active tasks."
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

fn get_task_logs_tool() -> McpTool {
    McpTool {
        name: "get_task_logs".to_string(),
        description: "Get execution logs for a specific task.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "UUID of the task"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of log entries to return (default: 50)",
                    "minimum": 1,
                    "maximum": 500
                }
            },
            "required": ["task_id"]
        }),
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        }),
    }
}

// ---------------------------------------------------------------------------
// Tool execution context — passed in by the caller (at-bridge http_api)
// ---------------------------------------------------------------------------

use at_core::types::{Agent, AgentStatus, Bead, BeadStatus, Lane, Task, TaskPhase};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared state needed to execute built-in tools.
#[derive(Clone)]
pub struct BuiltinToolContext {
    pub beads: Arc<RwLock<Vec<Bead>>>,
    pub agents: Arc<RwLock<Vec<Agent>>>,
    pub tasks: Arc<RwLock<Vec<Task>>>,
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/// Execute a built-in tool call against shared state.
///
/// Returns `None` if the tool name is not a built-in tool.
pub async fn execute_builtin_tool(
    ctx: &BuiltinToolContext,
    request: &ToolCallRequest,
) -> Option<ToolCallResult> {
    let result = match request.name.as_str() {
        "run_task" => Some(exec_run_task(ctx, &request.arguments).await),
        "list_agents" => Some(exec_list_agents(ctx, &request.arguments).await),
        "manage_beads" => Some(exec_manage_beads(ctx, &request.arguments).await),
        "get_build_status" => Some(exec_get_build_status(ctx, &request.arguments).await),
        "get_task_logs" => Some(exec_get_task_logs(ctx, &request.arguments).await),
        _ => None,
    };
    if let Some(ref r) = result {
        info!(
            tool = %request.name,
            is_error = r.is_error,
            "executed built-in tool"
        );
    }
    result
}

// ---------------------------------------------------------------------------
// Individual executors
// ---------------------------------------------------------------------------

async fn exec_run_task(ctx: &BuiltinToolContext, args: &serde_json::Value) -> ToolCallResult {
    let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return ToolCallResult::error("missing required parameter: task_id"),
    };

    let task_uuid: uuid::Uuid = match task_id.parse() {
        Ok(u) => u,
        Err(_) => return ToolCallResult::error(format!("invalid UUID: {task_id}")),
    };

    let mut tasks = ctx.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == task_uuid) else {
        return ToolCallResult::error(format!("task not found: {task_id}"));
    };

    // Only allow running tasks that are in Discovery or stopped phases.
    match &task.phase {
        TaskPhase::Discovery | TaskPhase::Stopped | TaskPhase::Error => {}
        other => {
            return ToolCallResult::error(format!(
                "task is already in phase {:?} and cannot be started",
                other
            ));
        }
    }

    task.phase = TaskPhase::ContextGathering;
    task.progress_percent = TaskPhase::ContextGathering.progress_percent();
    task.updated_at = chrono::Utc::now();
    task.started_at = Some(chrono::Utc::now());

    ToolCallResult::text(
        json!({
            "status": "started",
            "task_id": task_id,
            "phase": "context_gathering",
            "progress_percent": task.progress_percent,
        })
        .to_string(),
    )
}

async fn exec_list_agents(ctx: &BuiltinToolContext, args: &serde_json::Value) -> ToolCallResult {
    let agents = ctx.agents.read().await;

    let status_filter = args
        .get("status_filter")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_value::<AgentStatus>(json!(s)).ok());

    let filtered: Vec<&Agent> = match &status_filter {
        Some(status) => agents.iter().filter(|a| &a.status == status).collect(),
        None => agents.iter().collect(),
    };

    let result: Vec<serde_json::Value> = filtered
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "name": a.name,
                "role": a.role,
                "status": a.status,
                "cli_type": a.cli_type,
                "model": a.model,
                "last_seen": a.last_seen,
            })
        })
        .collect();

    ToolCallResult::text(json!({ "agents": result, "count": result.len() }).to_string())
}

async fn exec_manage_beads(ctx: &BuiltinToolContext, args: &serde_json::Value) -> ToolCallResult {
    let action = match args.get("action").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => return ToolCallResult::error("missing required parameter: action"),
    };

    match action {
        "list" => {
            let beads = ctx.beads.read().await;
            let result: Vec<serde_json::Value> = beads
                .iter()
                .map(|b| {
                    json!({
                        "id": b.id,
                        "title": b.title,
                        "status": b.status,
                        "lane": b.lane,
                        "priority": b.priority,
                        "created_at": b.created_at,
                    })
                })
                .collect();
            ToolCallResult::text(
                json!({ "beads": result, "count": result.len() }).to_string(),
            )
        }

        "get" => {
            let bead_id = match args.get("bead_id").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => return ToolCallResult::error("missing required parameter: bead_id"),
            };
            let bead_uuid: uuid::Uuid = match bead_id.parse() {
                Ok(u) => u,
                Err(_) => return ToolCallResult::error(format!("invalid UUID: {bead_id}")),
            };
            let beads = ctx.beads.read().await;
            match beads.iter().find(|b| b.id == bead_uuid) {
                Some(bead) => ToolCallResult::text(serde_json::to_string(bead).unwrap()),
                None => ToolCallResult::error(format!("bead not found: {bead_id}")),
            }
        }

        "create" => {
            let title = match args.get("title").and_then(|v| v.as_str()) {
                Some(t) => t,
                None => return ToolCallResult::error("missing required parameter: title"),
            };
            let lane = args
                .get("lane")
                .and_then(|v| v.as_str())
                .and_then(|s| serde_json::from_value::<Lane>(json!(s)).ok())
                .unwrap_or(Lane::Standard);

            let mut bead = Bead::new(title, lane);
            bead.description = args.get("description").and_then(|v| v.as_str()).map(String::from);

            let bead_json = serde_json::to_string(&bead).unwrap();
            let mut beads = ctx.beads.write().await;
            beads.push(bead);

            ToolCallResult::text(bead_json)
        }

        "update_status" => {
            let bead_id = match args.get("bead_id").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => return ToolCallResult::error("missing required parameter: bead_id"),
            };
            let bead_uuid: uuid::Uuid = match bead_id.parse() {
                Ok(u) => u,
                Err(_) => return ToolCallResult::error(format!("invalid UUID: {bead_id}")),
            };
            let new_status = match args.get("status").and_then(|v| v.as_str()) {
                Some(s) => match serde_json::from_value::<BeadStatus>(json!(s)) {
                    Ok(st) => st,
                    Err(_) => return ToolCallResult::error(format!("invalid status: {s}")),
                },
                None => return ToolCallResult::error("missing required parameter: status"),
            };

            let mut beads = ctx.beads.write().await;
            let Some(bead) = beads.iter_mut().find(|b| b.id == bead_uuid) else {
                return ToolCallResult::error(format!("bead not found: {bead_id}"));
            };

            if !bead.status.can_transition_to(&new_status) {
                return ToolCallResult::error(format!(
                    "invalid transition from {:?} to {:?}",
                    bead.status, new_status
                ));
            }

            bead.status = new_status;
            bead.updated_at = chrono::Utc::now();
            ToolCallResult::text(serde_json::to_string(&*bead).unwrap())
        }

        other => ToolCallResult::error(format!(
            "unknown action: {other}. Valid actions: list, get, create, update_status"
        )),
    }
}

async fn exec_get_build_status(
    ctx: &BuiltinToolContext,
    args: &serde_json::Value,
) -> ToolCallResult {
    let tasks = ctx.tasks.read().await;

    if let Some(task_id_str) = args.get("task_id").and_then(|v| v.as_str()) {
        let task_uuid: uuid::Uuid = match task_id_str.parse() {
            Ok(u) => u,
            Err(_) => return ToolCallResult::error(format!("invalid UUID: {task_id_str}")),
        };
        match tasks.iter().find(|t| t.id == task_uuid) {
            Some(task) => ToolCallResult::text(
                json!({
                    "task_id": task.id,
                    "title": task.title,
                    "phase": task.phase,
                    "progress_percent": task.progress_percent,
                    "started_at": task.started_at,
                    "error": task.error,
                })
                .to_string(),
            ),
            None => ToolCallResult::error(format!("task not found: {task_id_str}")),
        }
    } else {
        // Return all active (non-terminal) tasks.
        let active: Vec<serde_json::Value> = tasks
            .iter()
            .filter(|t| !matches!(t.phase, TaskPhase::Complete | TaskPhase::Error | TaskPhase::Stopped))
            .map(|t| {
                json!({
                    "task_id": t.id,
                    "title": t.title,
                    "phase": t.phase,
                    "progress_percent": t.progress_percent,
                })
            })
            .collect();
        ToolCallResult::text(
            json!({ "active_tasks": active, "count": active.len() }).to_string(),
        )
    }
}

async fn exec_get_task_logs(
    ctx: &BuiltinToolContext,
    args: &serde_json::Value,
) -> ToolCallResult {
    let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return ToolCallResult::error("missing required parameter: task_id"),
    };
    let task_uuid: uuid::Uuid = match task_id.parse() {
        Ok(u) => u,
        Err(_) => return ToolCallResult::error(format!("invalid UUID: {task_id}")),
    };
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as usize;

    let tasks = ctx.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == task_uuid) else {
        return ToolCallResult::error(format!("task not found: {task_id}"));
    };

    let logs: Vec<&at_core::types::TaskLogEntry> =
        task.logs.iter().rev().take(limit).collect();
    ToolCallResult::text(
        json!({
            "task_id": task_id,
            "logs": logs,
            "total": task.logs.len(),
            "returned": logs.len(),
        })
        .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::types::*;

    fn make_ctx() -> BuiltinToolContext {
        BuiltinToolContext {
            beads: Arc::new(RwLock::new(Vec::new())),
            agents: Arc::new(RwLock::new(Vec::new())),
            tasks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    #[test]
    fn tool_definitions_are_valid() {
        let tools = builtin_tool_definitions();
        assert_eq!(tools.len(), 5);

        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"run_task"));
        assert!(names.contains(&"list_agents"));
        assert!(names.contains(&"manage_beads"));
        assert!(names.contains(&"get_build_status"));
        assert!(names.contains(&"get_task_logs"));

        // All tools should have valid JSON Schema input.
        for tool in &tools {
            assert_eq!(tool.input_schema["type"], "object");
        }
    }

    #[test]
    fn tool_schemas_serialize_roundtrip() {
        for tool in builtin_tool_definitions() {
            let json = serde_json::to_string(&tool).unwrap();
            let parsed: McpTool = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.name, tool.name);
        }
    }

    #[test]
    fn read_only_annotations_are_correct() {
        let tools = builtin_tool_definitions();
        for tool in &tools {
            let ann = tool.annotations.as_ref().unwrap();
            match tool.name.as_str() {
                "list_agents" | "get_build_status" | "get_task_logs" => {
                    assert_eq!(ann.read_only_hint, Some(true), "{} should be read-only", tool.name);
                }
                "run_task" | "manage_beads" => {
                    assert_eq!(ann.read_only_hint, Some(false), "{} should NOT be read-only", tool.name);
                }
                _ => panic!("unexpected tool: {}", tool.name),
            }
        }
    }

    #[tokio::test]
    async fn exec_list_agents_empty() {
        let ctx = make_ctx();
        let req = ToolCallRequest {
            name: "list_agents".into(),
            arguments: json!({}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(!result.is_error);
        let text = result.text_content().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["count"], 0);
    }

    #[tokio::test]
    async fn exec_list_agents_with_filter() {
        let ctx = make_ctx();
        {
            let mut agents = ctx.agents.write().await;
            let mut a1 = Agent::new("alpha", AgentRole::Coder, CliType::Claude);
            a1.status = AgentStatus::Active;
            let mut a2 = Agent::new("beta", AgentRole::QaReviewer, CliType::Claude);
            a2.status = AgentStatus::Idle;
            agents.push(a1);
            agents.push(a2);
        }
        let req = ToolCallRequest {
            name: "list_agents".into(),
            arguments: json!({"status_filter": "active"}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(result.text_content().unwrap()).unwrap();
        assert_eq!(parsed["count"], 1);
    }

    #[tokio::test]
    async fn exec_manage_beads_create_and_list() {
        let ctx = make_ctx();

        // Create
        let req = ToolCallRequest {
            name: "manage_beads".into(),
            arguments: json!({"action": "create", "title": "Fix bug #42"}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(!result.is_error);
        let created: serde_json::Value =
            serde_json::from_str(result.text_content().unwrap()).unwrap();
        assert_eq!(created["title"], "Fix bug #42");
        assert_eq!(created["status"], "backlog");

        // List
        let req = ToolCallRequest {
            name: "manage_beads".into(),
            arguments: json!({"action": "list"}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(result.text_content().unwrap()).unwrap();
        assert_eq!(parsed["count"], 1);
    }

    #[tokio::test]
    async fn exec_manage_beads_get_not_found() {
        let ctx = make_ctx();
        let req = ToolCallRequest {
            name: "manage_beads".into(),
            arguments: json!({
                "action": "get",
                "bead_id": "00000000-0000-0000-0000-000000000001"
            }),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn exec_manage_beads_update_status() {
        let ctx = make_ctx();
        let bead = Bead::new("Test bead", Lane::Standard);
        let bead_id = bead.id.to_string();
        ctx.beads.write().await.push(bead);

        let req = ToolCallRequest {
            name: "manage_beads".into(),
            arguments: json!({
                "action": "update_status",
                "bead_id": bead_id,
                "status": "hooked"
            }),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value =
            serde_json::from_str(result.text_content().unwrap()).unwrap();
        assert_eq!(parsed["status"], "hooked");
    }

    #[tokio::test]
    async fn exec_manage_beads_invalid_transition() {
        let ctx = make_ctx();
        let bead = Bead::new("Test bead", Lane::Standard);
        let bead_id = bead.id.to_string();
        ctx.beads.write().await.push(bead);

        // Backlog -> Done is invalid
        let req = ToolCallRequest {
            name: "manage_beads".into(),
            arguments: json!({
                "action": "update_status",
                "bead_id": bead_id,
                "status": "done"
            }),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn exec_run_task_not_found() {
        let ctx = make_ctx();
        let req = ToolCallRequest {
            name: "run_task".into(),
            arguments: json!({"task_id": "00000000-0000-0000-0000-000000000001"}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(result.is_error);
        assert!(result.text_content().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn exec_run_task_success() {
        let ctx = make_ctx();
        let bead = Bead::new("parent", Lane::Standard);
        let task = Task::new(
            "Implement feature",
            bead.id,
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Medium,
        );
        let task_id = task.id.to_string();
        ctx.tasks.write().await.push(task);

        let req = ToolCallRequest {
            name: "run_task".into(),
            arguments: json!({"task_id": task_id}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value =
            serde_json::from_str(result.text_content().unwrap()).unwrap();
        assert_eq!(parsed["status"], "started");
    }

    #[tokio::test]
    async fn exec_run_task_already_running() {
        let ctx = make_ctx();
        let bead = Bead::new("parent", Lane::Standard);
        let mut task = Task::new(
            "Running task",
            bead.id,
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Medium,
        );
        task.phase = TaskPhase::Coding;
        let task_id = task.id.to_string();
        ctx.tasks.write().await.push(task);

        let req = ToolCallRequest {
            name: "run_task".into(),
            arguments: json!({"task_id": task_id}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn exec_get_build_status_all_active() {
        let ctx = make_ctx();
        let bead = Bead::new("b", Lane::Standard);
        let mut t1 = Task::new(
            "Active task",
            bead.id,
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Medium,
        );
        t1.phase = TaskPhase::Coding;
        let mut t2 = Task::new(
            "Done task",
            bead.id,
            TaskCategory::Feature,
            TaskPriority::Low,
            TaskComplexity::Low,
        );
        t2.phase = TaskPhase::Complete;
        ctx.tasks.write().await.extend(vec![t1, t2]);

        let req = ToolCallRequest {
            name: "get_build_status".into(),
            arguments: json!({}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value =
            serde_json::from_str(result.text_content().unwrap()).unwrap();
        assert_eq!(parsed["count"], 1);
    }

    #[tokio::test]
    async fn exec_get_task_logs_empty() {
        let ctx = make_ctx();
        let bead = Bead::new("b", Lane::Standard);
        let task = Task::new(
            "My task",
            bead.id,
            TaskCategory::BugFix,
            TaskPriority::High,
            TaskComplexity::Low,
        );
        let task_id = task.id.to_string();
        ctx.tasks.write().await.push(task);

        let req = ToolCallRequest {
            name: "get_task_logs".into(),
            arguments: json!({"task_id": task_id}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value =
            serde_json::from_str(result.text_content().unwrap()).unwrap();
        assert_eq!(parsed["total"], 0);
    }

    #[tokio::test]
    async fn unknown_tool_returns_none() {
        let ctx = make_ctx();
        let req = ToolCallRequest {
            name: "nonexistent_tool".into(),
            arguments: json!({}),
        };
        assert!(execute_builtin_tool(&ctx, &req).await.is_none());
    }

    #[tokio::test]
    async fn exec_manage_beads_missing_action() {
        let ctx = make_ctx();
        let req = ToolCallRequest {
            name: "manage_beads".into(),
            arguments: json!({}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn exec_run_task_invalid_uuid() {
        let ctx = make_ctx();
        let req = ToolCallRequest {
            name: "run_task".into(),
            arguments: json!({"task_id": "not-a-uuid"}),
        };
        let result = execute_builtin_tool(&ctx, &req).await.unwrap();
        assert!(result.is_error);
        assert!(result.text_content().unwrap().contains("invalid UUID"));
    }
}
