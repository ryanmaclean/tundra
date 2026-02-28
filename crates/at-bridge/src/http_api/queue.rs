use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use at_core::types::{TaskPhase, TaskPriority};

use crate::api_error::ApiError;
use super::state::ApiState;
use super::types::{PrioritizeRequest, QueueReorderRequest};

/// GET /api/queue -- list queued tasks sorted by priority.
pub(crate) async fn list_queue(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let tasks = state.tasks.read().await;

    // Filter to tasks in Discovery phase (queued/not-yet-started)
    let mut queued: Vec<_> = tasks
        .values()
        .filter(|t| t.phase == TaskPhase::Discovery && t.started_at.is_none())
        .cloned()
        .collect();

    // Sort by priority: Urgent > High > Medium > Low
    queued.sort_by(|a, b| {
        let priority_ord = |p: &TaskPriority| -> u8 {
            match p {
                TaskPriority::Urgent => 0,
                TaskPriority::High => 1,
                TaskPriority::Medium => 2,
                TaskPriority::Low => 3,
            }
        };
        priority_ord(&a.priority).cmp(&priority_ord(&b.priority))
    });

    let result: Vec<serde_json::Value> = queued
        .iter()
        .enumerate()
        .map(|(i, t)| {
            serde_json::json!({
                "task_id": t.id,
                "title": t.title,
                "priority": t.priority,
                "queued_at": t.created_at,
                "position": i + 1,
            })
        })
        .collect();

    Json(serde_json::json!(result))
}

/// POST /api/queue/reorder -- reorder the task queue.
pub(crate) async fn reorder_queue(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<QueueReorderRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if req.task_ids.is_empty() {
        return Err(ApiError::BadRequest("task_ids must not be empty".into()));
    }

    // Validate all task IDs exist
    let tasks = state.tasks.read().await;
    for task_id in &req.task_ids {
        if !tasks.contains_key(task_id) {
            return Err(ApiError::NotFound(format!("task {} not found", task_id)));
        }
    }
    drop(tasks);

    // Publish queue update event
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::QueueUpdate {
            task_ids: req.task_ids,
        });

    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"status": "ok"})),
    ))
}

/// POST /api/queue/{task_id}/prioritize -- bump a task's priority.
pub(crate) async fn prioritize_task(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
    Json(req): Json<PrioritizeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.get_mut(&task_id) else {
        return Err(ApiError::NotFound("task not found".into()));
    };

    task.priority = req.priority;
    task.updated_at = chrono::Utc::now();

    let task_snapshot = task.clone();
    drop(tasks);

    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(
            Box::new(task_snapshot.clone()),
        ));

    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task_snapshot)),
    ))
}
