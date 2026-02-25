//! Agent queue and pipeline management API routes.
//!
//! Provides endpoints for managing the agent task queue, including
//! queue listing, reordering, task prioritization, and pipeline status.
//!
//! All routes are prefixed with `/api/queue` or `/api/pipeline` and must be
//! merged into the main router using `.merge()` or `.nest()`.

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{atomic::Ordering, Arc};
use uuid::Uuid;

use at_core::types::{TaskPhase, TaskPriority};

use crate::http_api::ApiState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct QueueReorderRequest {
    pub task_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct PrioritizeRequest {
    pub priority: TaskPriority,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineQueueStatus {
    pub limit: usize,
    pub waiting: usize,
    pub running: usize,
    pub available_permits: usize,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the queue sub-router.
///
/// Includes all queue-related endpoints:
/// - Queue listing and reordering
/// - Task prioritization
/// - Pipeline queue status
///
/// All routes are mounted under `/api/queue` and `/api/pipeline/queue` — the
/// caller is responsible for merging this into the top-level router.
pub fn queue_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Agent Queue
        .route("/api/queue", get(list_queue))
        .route("/api/queue/reorder", post(reorder_queue))
        .route("/api/queue/{task_id}/prioritize", post(prioritize_task))
        // Pipeline Queue
        .route("/api/pipeline/queue", get(get_pipeline_queue_status))
}

// ---------------------------------------------------------------------------
// Agent Queue handlers
// ---------------------------------------------------------------------------

/// GET /api/queue — list queued tasks sorted by priority.
async fn list_queue(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let tasks = state.tasks.read().await;

    // Filter to tasks in Discovery phase (queued/not-yet-started)
    let mut queued: Vec<_> = tasks
        .iter()
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

/// POST /api/queue/reorder — reorder the task queue.
async fn reorder_queue(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<QueueReorderRequest>,
) -> impl IntoResponse {
    if req.task_ids.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "task_ids must not be empty"})),
        );
    }

    // Validate all task IDs exist
    let tasks = state.tasks.read().await;
    for task_id in &req.task_ids {
        if !tasks.iter().any(|t| t.id == *task_id) {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("task {} not found", task_id)})),
            );
        }
    }
    drop(tasks);

    // Publish queue update event
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::QueueUpdate {
            task_ids: req.task_ids,
        });

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"status": "ok"})),
    )
}

/// POST /api/queue/{task_id}/prioritize — bump a task's priority.
async fn prioritize_task(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
    Json(req): Json<PrioritizeRequest>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };

    task.priority = req.priority;
    task.updated_at = chrono::Utc::now();

    let task_snapshot = task.clone();
    drop(tasks);

    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(
            task_snapshot.clone(),
        ));

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task_snapshot)),
    )
}

// ---------------------------------------------------------------------------
// Pipeline Queue handlers
// ---------------------------------------------------------------------------

/// GET /api/pipeline/queue — get current pipeline queue status.
async fn get_pipeline_queue_status(
    State(state): State<Arc<ApiState>>,
) -> Json<PipelineQueueStatus> {
    Json(PipelineQueueStatus {
        limit: state.pipeline_max_concurrent,
        waiting: state.pipeline_waiting.load(Ordering::SeqCst),
        running: state.pipeline_running.load(Ordering::SeqCst),
        available_permits: state.pipeline_semaphore.available_permits(),
    })
}
