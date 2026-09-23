use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use uuid::Uuid;

use at_core::types::{BuildLogEntry, BuildStream, CliType, Task, TaskPhase};
use at_core::worktree_manager::WorktreeManager;

use super::gate_flow;
use super::state::ApiState;
use super::types::{BuildLogsQuery, BuildStatusSummary, ExecuteTaskRequest, PipelineQueueStatus};
use crate::api_error::ApiError;

/// GET /api/pipeline/queue -- return current pipeline queue status.
pub(crate) async fn get_pipeline_queue_status(
    State(state): State<Arc<ApiState>>,
) -> Json<PipelineQueueStatus> {
    Json(PipelineQueueStatus {
        limit: state.pipeline_max_concurrent,
        waiting: state.pipeline_waiting.load(Ordering::SeqCst),
        running: state.pipeline_running.load(Ordering::SeqCst),
        available_permits: state.pipeline_semaphore.available_permits(),
    })
}

/// POST /api/tasks/{id}/execute -- spawn the coding -> QA -> merge-gate pipeline.
///
/// Transitions the task to Coding phase, then spawns a background tokio task
/// that drives the pipeline through QA and the merge gate
/// ([`super::gate_flow::run_merge_phase`]). The task always ends in
/// `complete` or `error`. Returns 202 Accepted immediately so the caller can
/// follow progress via WebSocket events or by polling `GET /api/tasks/{id}`.
///
/// Accepts an optional JSON body with `cli_type` and `merge_mode`
/// (`verify`, the default, never merges; `auto` merges when the gate passes).
/// Task must be in Planning or Queue phase; returns 400 for invalid phase transitions.
///
/// **Response:** 202 `ExecuteTaskResponse`:
/// ```json
/// {"status": "started", "task_id": "...", "merge": "gated",
///  "merge_mode": "verify", "acceptance_criteria_count": 1,
///  "acceptance_criteria_sha256": "...", "warnings": []}
/// ```
/// `merge` is `gated` (a worktree is or will be bound and the gate runs),
/// `skipped_no_worktree` (no repo root and no criteria: ends Complete without
/// a gate) or `blocked_no_worktree` (criteria but no repo root: ends Error).
/// The criteria are frozen at this point: they cannot change until the task
/// leaves the pipeline, and the gate re-checks `acceptance_criteria_sha256`.
pub(crate) async fn execute_task_pipeline(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    body: Option<Json<ExecuteTaskRequest>>,
) -> Result<impl IntoResponse, ApiError> {
    let req = body.map(|b| b.0);
    let cli_type = req
        .as_ref()
        .and_then(|b| b.cli_type.clone())
        .unwrap_or(CliType::Claude);
    let merge_mode = req.as_ref().and_then(|b| b.merge_mode).unwrap_or_default();

    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.get_mut(&id) else {
        return Err(ApiError::NotFound("task not found".into()));
    };

    // The task must be in a phase that can transition to Coding.
    if !task.phase.can_transition_to(&TaskPhase::Coding) {
        return Err(ApiError::BadRequest(format!(
            "cannot start pipeline: task is in {:?} phase",
            task.phase
        )));
    }

    task.set_phase(TaskPhase::Coding);
    if task.started_at.is_none() {
        task.started_at = Some(chrono::Utc::now());
    }
    let task_snapshot = task.clone();
    drop(tasks);

    let criteria = task_snapshot.acceptance_criteria.clone();
    let criteria_sha256 = gate_flow::criteria_sha256(&criteria);
    let worktree_bound = task_snapshot.worktree_path.is_some() && task_snapshot.git_branch.is_some();
    let merge = if state.repo_root.is_some() || worktree_bound {
        "gated"
    } else if criteria.is_empty() {
        "skipped_no_worktree"
    } else {
        "blocked_no_worktree"
    };
    let mut warnings = Vec::new();
    if criteria.is_empty() {
        warnings.push(
            "no acceptance_criteria: the merge gate only checks for clean trees".to_string(),
        );
    }
    if merge == "blocked_no_worktree" {
        warnings.push(
            "acceptance_criteria set but the daemon has no repo_root ([general] workspace_root): the task will end in error"
                .to_string(),
        );
    }

    // Publish the phase change.
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
            task_snapshot.clone(),
        )));

    let ctx = gate_flow::PipelineCtx {
        tasks: state.tasks.clone(),
        event_bus: state.event_bus.clone(),
        pty_pool: state.pty_pool.clone(),
        cli_type,
        repo_root: state.repo_root.clone(),
        merge_lock: state.merge_lock.clone(),
        gate_config: state.settings_manager.load_or_default().merge_gate,
        merge_mode,
        criteria: criteria.clone(),
        criteria_sha256: criteria_sha256.clone(),
    };

    // Spawn a background task to drive the pipeline phases.
    let pipeline_semaphore = state.pipeline_semaphore.clone();
    let pipeline_waiting = state.pipeline_waiting.clone();
    let pipeline_running = state.pipeline_running.clone();
    let pipeline_limit = state.pipeline_max_concurrent;

    let queued_position = pipeline_waiting.fetch_add(1, Ordering::SeqCst) + 1;
    ctx.emit(
        task_snapshot.bead_id,
        "pipeline_queued",
        format!(
            "Task '{}' queued (position={}, limit={})",
            task_snapshot.title, queued_position, pipeline_limit
        ),
    );

    tokio::spawn(async move {
        let _permit = match pipeline_semaphore.acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => {
                pipeline_waiting.fetch_sub(1, Ordering::SeqCst);
                ctx.emit(
                    task_snapshot.bead_id,
                    "pipeline_queue_error",
                    format!(
                        "Task '{}' failed to acquire pipeline queue permit",
                        task_snapshot.title
                    ),
                );
                return;
            }
        };

        pipeline_waiting.fetch_sub(1, Ordering::SeqCst);
        let running_now = pipeline_running.fetch_add(1, Ordering::SeqCst) + 1;
        ctx.emit(
            task_snapshot.bead_id,
            "pipeline_started",
            format!(
                "Task '{}' started (running={}, limit={})",
                task_snapshot.title, running_now, pipeline_limit
            ),
        );

        run_pipeline_background(&ctx, task_snapshot).await;
        pipeline_running.fetch_sub(1, Ordering::SeqCst);
    });

    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "status": "started",
            "task_id": id.to_string(),
            "merge": merge,
            "merge_mode": merge_mode.as_str(),
            "acceptance_criteria_count": criteria.len(),
            "acceptance_criteria_sha256": criteria_sha256,
            "warnings": warnings,
        })),
    ))
}

/// Background pipeline driver: worktree -> coding -> QA (+ fix loop) ->
/// merge gate. Always leaves the task in `Complete` or `Error`.
async fn run_pipeline_background(ctx: &gate_flow::PipelineCtx, task: Task) {
    let max_fix_iterations: usize = 3;
    let (task_id, bead_id) = (task.id, task.bead_id);
    let emit = |event_type: &str| {
        ctx.emit(
            bead_id,
            event_type,
            format!("Task '{}': {}", task.title, event_type),
        )
    };

    emit("pipeline_start");

    // -- Coding phase --
    emit("coding_phase_start");
    ctx.build_log(task_id, bead_id, BuildStream::Stdout, "Coding phase started".into())
        .await;

    // Bind a worktree so QA and the merge gate run on the task's own branch
    // (mirrors the daemon orchestrator's start_task).
    if let (Some(root), None) = (ctx.repo_root.as_ref(), task.git_branch.as_ref()) {
        let created = {
            let _guard = ctx.merge_lock.lock().await;
            WorktreeManager::new(root).create_for_task(&task).await
        };
        match created {
            Ok(info) => {
                ctx.update(task_id, |t| {
                    t.worktree_path = Some(info.path.clone());
                    t.git_branch = Some(info.branch.clone());
                })
                .await;
                ctx.build_log(
                    task_id,
                    bead_id,
                    BuildStream::Stdout,
                    format!("Worktree created at {} on {}", info.path, info.branch),
                )
                .await;
                emit("worktree_created");
            }
            Err(e) => {
                ctx.fail(task_id, bead_id, format!("worktree creation failed: {e}"))
                    .await;
                return;
            }
        }
    }

    if ctx.pty_pool.is_some() {
        tracing::info!(task_id = %task_id, cli = ?ctx.cli_type, "PTY pool available; coding phase delegated to agent executor");
        ctx.build_log(
            task_id,
            bead_id,
            BuildStream::Stdout,
            "PTY pool available; delegating to agent executor".into(),
        )
        .await;
    }

    ctx.build_log(task_id, bead_id, BuildStream::Stdout, "Coding phase complete".into())
        .await;
    emit("coding_phase_complete");

    // -- QA phase --
    ctx.set_phase(task_id, TaskPhase::Qa).await;
    emit("qa_phase_start");
    ctx.build_log(task_id, bead_id, BuildStream::Stdout, "QA phase started".into())
        .await;
    let mut qa_ok = gate_flow::run_qa(ctx, task_id, "QA result").await;
    emit("qa_phase_complete");

    // -- QA fix loop --
    let mut iterations = 0usize;
    while !qa_ok && iterations < max_fix_iterations {
        iterations += 1;
        emit(&format!("qa_fix_iteration_{}", iterations));
        ctx.set_phase(task_id, TaskPhase::Fixing).await;
        ctx.build_log(
            task_id,
            bead_id,
            BuildStream::Stderr,
            format!("Fix iteration {} of {}", iterations, max_fix_iterations),
        )
        .await;
        ctx.set_phase(task_id, TaskPhase::Qa).await;
        qa_ok = gate_flow::run_qa(ctx, task_id, "QA re-check result").await;
    }

    if !qa_ok {
        ctx.fail(
            task_id,
            bead_id,
            format!("QA failed after {iterations} fix iteration(s)"),
        )
        .await;
        return;
    }

    // -- Merging phase: merge gate (+ gate fix loop) --
    gate_flow::run_merge_phase(ctx, task_id).await;

    let final_phase = ctx.snapshot(task_id).await.map(|t| t.phase);
    tracing::info!(
        task_id = %task_id,
        phase = ?final_phase,
        qa_fix_iterations = iterations,
        "pipeline background task finished"
    );
}

/// GET /api/tasks/{id}/build-logs -- return captured build output lines.
pub(crate) async fn get_build_logs(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Query(q): Query<BuildLogsQuery>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.get(&id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };

    let logs: Vec<&BuildLogEntry> = if let Some(ref since_str) = q.since {
        match chrono::DateTime::parse_from_rfc3339(since_str) {
            Ok(since_ts) => {
                let since_utc = since_ts.with_timezone(&chrono::Utc);
                task.build_logs
                    .iter()
                    .filter(|e| e.timestamp > since_utc)
                    .collect()
            }
            Err(_) => {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(
                        serde_json::json!({"error": "invalid 'since' timestamp; use ISO-8601 / RFC-3339"}),
                    ),
                );
            }
        }
    } else {
        task.build_logs.iter().collect()
    };

    (axum::http::StatusCode::OK, Json(serde_json::json!(logs)))
}

/// GET /api/tasks/{id}/build-status -- return a summary of the build.
pub(crate) async fn get_build_status(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.get(&id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };

    let stdout_lines = task
        .build_logs
        .iter()
        .filter(|e| e.stream == BuildStream::Stdout)
        .count();
    let stderr_lines = task
        .build_logs
        .iter()
        .filter(|e| e.stream == BuildStream::Stderr)
        .count();
    let last_line = task.build_logs.last().map(|e| e.line.clone());

    let summary = BuildStatusSummary {
        phase: task.phase.clone(),
        progress_percent: task.progress_percent,
        total_lines: task.build_logs.len(),
        stdout_lines,
        stderr_lines,
        error_count: stderr_lines,
        last_line,
    };

    (axum::http::StatusCode::OK, Json(serde_json::json!(summary)))
}
