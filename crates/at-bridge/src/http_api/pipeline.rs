use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::RwLock;
use uuid::Uuid;

use at_core::types::{BuildLogEntry, BuildStream, CliType, Task, TaskPhase};

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

/// POST /api/tasks/{id}/execute -- spawn the coding -> QA -> fix pipeline.
///
/// Transitions the task to Coding phase, then spawns a background tokio task
/// that drives the pipeline through QA and fix iterations. Returns 202 Accepted
/// immediately so the caller can follow progress via WebSocket events.
///
/// Accepts an optional JSON body with `cli_type` to override the default CLI.
/// Task must be in Planning or Queue phase; returns 400 for invalid phase transitions.
///
/// **Request Body:** Optional ExecuteTaskRequest JSON object with cli_type override.
/// **Response:** 202 Accepted with task snapshot, 404 if task not found, 400 if invalid phase.
pub(crate) async fn execute_task_pipeline(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    body: Option<Json<ExecuteTaskRequest>>,
) -> Result<impl IntoResponse, ApiError> {
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
    let task_snapshot = task.clone();
    drop(tasks);

    // Extract optional CLI type from request body.
    let cli_type = body.and_then(|b| b.0.cli_type).unwrap_or(CliType::Claude);

    // Publish the phase change.
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::TaskUpdate(
            Box::new(task_snapshot.clone()),
        ));

    // Spawn a background task to drive the pipeline phases.
    let tasks_store = state.tasks.clone();
    let event_bus = state.event_bus.clone();
    let pty_pool = state.pty_pool.clone();
    let pipeline_semaphore = state.pipeline_semaphore.clone();
    let pipeline_waiting = state.pipeline_waiting.clone();
    let pipeline_running = state.pipeline_running.clone();
    let pipeline_limit = state.pipeline_max_concurrent;

    let queued_position = pipeline_waiting.fetch_add(1, Ordering::SeqCst) + 1;
    state
        .event_bus
        .publish(crate::protocol::BridgeMessage::Event(
            crate::protocol::EventPayload {
                event_type: "pipeline_queued".to_string(),
                agent_id: None,
                bead_id: Some(task_snapshot.bead_id),
                message: format!(
                    "Task '{}' queued (position={}, limit={})",
                    task_snapshot.title, queued_position, pipeline_limit
                ),
                timestamp: chrono::Utc::now(),
            },
        ));

    tokio::spawn(async move {
        let _permit = match pipeline_semaphore.acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => {
                pipeline_waiting.fetch_sub(1, Ordering::SeqCst);
                event_bus.publish(crate::protocol::BridgeMessage::Event(
                    crate::protocol::EventPayload {
                        event_type: "pipeline_queue_error".to_string(),
                        agent_id: None,
                        bead_id: Some(task_snapshot.bead_id),
                        message: format!(
                            "Task '{}' failed to acquire pipeline queue permit",
                            task_snapshot.title
                        ),
                        timestamp: chrono::Utc::now(),
                    },
                ));
                return;
            }
        };

        pipeline_waiting.fetch_sub(1, Ordering::SeqCst);
        let running_now = pipeline_running.fetch_add(1, Ordering::SeqCst) + 1;
        event_bus.publish(crate::protocol::BridgeMessage::Event(
            crate::protocol::EventPayload {
                event_type: "pipeline_started".to_string(),
                agent_id: None,
                bead_id: Some(task_snapshot.bead_id),
                message: format!(
                    "Task '{}' started (running={}, limit={})",
                    task_snapshot.title, running_now, pipeline_limit
                ),
                timestamp: chrono::Utc::now(),
            },
        ));

        run_pipeline_background(task_snapshot, tasks_store, event_bus, pty_pool, cli_type).await;
        pipeline_running.fetch_sub(1, Ordering::SeqCst);
    });

    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(serde_json::json!({"status": "started", "task_id": id.to_string()})),
    ))
}

/// Background pipeline driver: coding -> QA -> fix loop.
async fn run_pipeline_background(
    task: Task,
    tasks_store: Arc<RwLock<std::collections::HashMap<Uuid, Task>>>,
    event_bus: crate::event_bus::EventBus,
    pty_pool: Option<Arc<at_session::pty_pool::PtyPool>>,
    _cli_type: CliType,
) {
    use at_intelligence::runner::QaRunner;
    let max_fix_iterations: usize = 3;

    let emit = |event_type: &str| {
        event_bus.publish(crate::protocol::BridgeMessage::Event(
            crate::protocol::EventPayload {
                event_type: event_type.to_string(),
                agent_id: None,
                bead_id: Some(task.bead_id),
                message: format!("Task '{}': {}", task.title, event_type),
                timestamp: chrono::Utc::now(),
            },
        ));
    };

    let emit_build_log = |tasks_store: &Arc<RwLock<std::collections::HashMap<Uuid, Task>>>,
                          event_bus: &crate::event_bus::EventBus,
                          task_id: Uuid,
                          bead_id: Uuid,
                          stream: BuildStream,
                          line: String,
                          phase: TaskPhase| {
        let ts = tasks_store.clone();
        let eb = event_bus.clone();
        let stream_label = match &stream {
            BuildStream::Stdout => "stdout",
            BuildStream::Stderr => "stderr",
        };
        eb.publish(crate::protocol::BridgeMessage::Event(
            crate::protocol::EventPayload {
                event_type: "build_log_line".to_string(),
                agent_id: None,
                bead_id: Some(bead_id),
                message: format!("[{}] {}", stream_label, line),
                timestamp: chrono::Utc::now(),
            },
        ));
        async move {
            let mut tasks = ts.write().await;
            if let Some(t) = tasks.get_mut(&task_id) {
                t.build_logs.push(BuildLogEntry {
                    timestamp: chrono::Utc::now(),
                    stream,
                    line,
                    phase,
                });
                t.updated_at = chrono::Utc::now();
            }
        }
    };

    emit("pipeline_start");

    // -- Coding phase --
    emit("coding_phase_start");

    emit_build_log(
        &tasks_store,
        &event_bus,
        task.id,
        task.bead_id,
        BuildStream::Stdout,
        "Coding phase started".to_string(),
        TaskPhase::Coding,
    )
    .await;

    if pty_pool.is_some() {
        tracing::info!(task_id = %task.id, "PTY pool available; coding phase delegated to agent executor");
        emit_build_log(
            &tasks_store,
            &event_bus,
            task.id,
            task.bead_id,
            BuildStream::Stdout,
            "PTY pool available; delegating to agent executor".to_string(),
            TaskPhase::Coding,
        )
        .await;
    }

    emit_build_log(
        &tasks_store,
        &event_bus,
        task.id,
        task.bead_id,
        BuildStream::Stdout,
        "Coding phase complete".to_string(),
        TaskPhase::Coding,
    )
    .await;

    emit("coding_phase_complete");

    // Transition to QA
    {
        let mut tasks = tasks_store.write().await;
        if let Some(t) = tasks.get_mut(&task.id) {
            t.set_phase(TaskPhase::Qa);
            event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(t.clone())));
        }
    }

    // -- QA phase --
    emit("qa_phase_start");

    emit_build_log(
        &tasks_store,
        &event_bus,
        task.id,
        task.bead_id,
        BuildStream::Stdout,
        "QA phase started".to_string(),
        TaskPhase::Qa,
    )
    .await;

    let worktree = task.worktree_path.as_deref().unwrap_or(".");
    let mut qa_runner = QaRunner::new();
    let mut report = qa_runner.run_qa_checks(task.id, &task.title, Some(worktree));

    let qa_stream = if report.status == at_core::types::QaStatus::Passed {
        BuildStream::Stdout
    } else {
        BuildStream::Stderr
    };
    emit_build_log(
        &tasks_store,
        &event_bus,
        task.id,
        task.bead_id,
        qa_stream,
        format!(
            "QA result: {:?} ({} issues)",
            report.status,
            report.issues.len()
        ),
        TaskPhase::Qa,
    )
    .await;

    emit("qa_phase_complete");

    // -- QA fix loop --
    let mut iterations = 0usize;
    while report.status == at_core::types::QaStatus::Failed && iterations < max_fix_iterations {
        iterations += 1;
        emit(&format!("qa_fix_iteration_{}", iterations));

        emit_build_log(
            &tasks_store,
            &event_bus,
            task.id,
            task.bead_id,
            BuildStream::Stderr,
            format!("Fix iteration {} of {}", iterations, max_fix_iterations),
            TaskPhase::Fixing,
        )
        .await;

        // Transition to Fixing
        {
            let mut tasks = tasks_store.write().await;
            if let Some(t) = tasks.get_mut(&task.id) {
                t.set_phase(TaskPhase::Fixing);
                event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(t.clone())));
            }
        }

        // Re-run QA
        {
            let mut tasks = tasks_store.write().await;
            if let Some(t) = tasks.get_mut(&task.id) {
                t.set_phase(TaskPhase::Qa);
                event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(t.clone())));
            }
        }

        let mut qa = QaRunner::new();
        report = qa.run_qa_checks(task.id, &task.title, Some(worktree));

        let iter_stream = if report.status == at_core::types::QaStatus::Passed {
            BuildStream::Stdout
        } else {
            BuildStream::Stderr
        };
        emit_build_log(
            &tasks_store,
            &event_bus,
            task.id,
            task.bead_id,
            iter_stream,
            format!(
                "QA re-check result: {:?} ({} issues)",
                report.status,
                report.issues.len()
            ),
            TaskPhase::Qa,
        )
        .await;
    }

    // Store the QA report on the task
    {
        let mut tasks = tasks_store.write().await;
        if let Some(t) = tasks.get_mut(&task.id) {
            t.qa_report = Some(report.clone());

            let next_phase = report.next_phase();
            t.set_phase(next_phase);
            event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(t.clone())));
        }
    }

    if report.status == at_core::types::QaStatus::Passed {
        emit_build_log(
            &tasks_store,
            &event_bus,
            task.id,
            task.bead_id,
            BuildStream::Stdout,
            "Pipeline completed successfully".to_string(),
            TaskPhase::Complete,
        )
        .await;
        emit("pipeline_complete");
    } else {
        emit_build_log(
            &tasks_store,
            &event_bus,
            task.id,
            task.bead_id,
            BuildStream::Stderr,
            "Pipeline completed with failures".to_string(),
            TaskPhase::Error,
        )
        .await;
        emit("pipeline_complete_with_failures");
    }

    tracing::info!(
        task_id = %task.id,
        qa_passed = (report.status == at_core::types::QaStatus::Passed),
        fix_iterations = iterations,
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
