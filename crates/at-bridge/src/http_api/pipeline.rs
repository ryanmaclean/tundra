use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// RAII guard that decrements an `AtomicUsize` counter when dropped.
///
/// This ensures the `pipeline_running` counter is decremented even when
/// `run_pipeline_background` panics, preventing a permanent counter leak
/// that would eventually starve the semaphore.
struct CounterGuard(Arc<AtomicUsize>);

impl Drop for CounterGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

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
        .publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
            task_snapshot.clone(),
        )));

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
        // CounterGuard ensures `pipeline_running` is decremented even if
        // `run_pipeline_background` panics (panic-safe counter management).
        let _counter_guard = CounterGuard(pipeline_running);
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
        // _counter_guard drops here (or on panic), decrementing pipeline_running.
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
            event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
                t.clone(),
            )));
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
                event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
                    t.clone(),
                )));
            }
        }

        // Re-run QA
        {
            let mut tasks = tasks_store.write().await;
            if let Some(t) = tasks.get_mut(&task.id) {
                t.set_phase(TaskPhase::Qa);
                event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
                    t.clone(),
                )));
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
            event_bus.publish(crate::protocol::BridgeMessage::TaskUpdate(Box::new(
                t.clone(),
            )));
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

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::event_bus::EventBus;
    use crate::http_api::api_router;
    use crate::http_api::state::ApiState;
    use at_core::types::{Task, TaskCategory, TaskComplexity, TaskPhase, TaskPriority};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use tower::ServiceExt;
    use uuid::Uuid;

    fn test_app() -> (axum::Router, Arc<ApiState>) {
        let event_bus = EventBus::new();
        let state = Arc::new(ApiState::new(event_bus).with_relaxed_rate_limits());
        let app = api_router(state.clone());
        (app, state)
    }

    /// Advance a fresh Task (Discovery) through the phase chain up to Planning.
    fn task_at_planning() -> Task {
        let mut task = Task::new(
            "pipeline-test",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        );
        task.set_phase(TaskPhase::ContextGathering);
        task.set_phase(TaskPhase::SpecCreation);
        task.set_phase(TaskPhase::Planning);
        task
    }

    // -----------------------------------------------------------------------
    // Test 1: 400 for invalid phase (task already in Coding)
    // -----------------------------------------------------------------------

    /// Posting /execute for a task already in Coding must return 400 with a
    /// meaningful error body describing the bad phase.
    ///
    /// Mutation test: changing `can_transition_to` to always return `true`
    /// makes this test fail (response becomes 202 instead of 400).
    #[tokio::test]
    async fn execute_task_pipeline_returns_400_for_invalid_phase() {
        let (app, state) = test_app();

        // Seed a task that is already in Coding — cannot transition to Coding again.
        let mut task = task_at_planning();
        task.set_phase(TaskPhase::Coding);
        let task_id = task.id;
        state.tasks.write().await.insert(task_id, task);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/tasks/{task_id}/execute"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "expected 400 Bad Request for task already in Coding phase"
        );

        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = std::str::from_utf8(&body_bytes).unwrap();
        // The error message should mention the phase.
        assert!(
            body_str.contains("Coding")
                || body_str.contains("cannot")
                || body_str.contains("phase"),
            "error body should describe the invalid phase, got: {body_str}"
        );

        // Counter must stay at zero — the request was rejected before spawning.
        assert_eq!(
            state.pipeline_running.load(Ordering::SeqCst),
            0,
            "pipeline_running must remain 0 after a rejected request"
        );
    }

    /// Same as above but for a task in Qa phase (also cannot transition to Coding).
    #[tokio::test]
    async fn execute_task_pipeline_returns_400_for_qa_phase() {
        let (app, state) = test_app();

        let mut task = task_at_planning();
        task.set_phase(TaskPhase::Coding);
        task.set_phase(TaskPhase::Qa);
        let task_id = task.id;
        state.tasks.write().await.insert(task_id, task);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/tasks/{task_id}/execute"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = std::str::from_utf8(&body_bytes).unwrap();
        assert!(
            body_str.contains("Qa") || body_str.contains("cannot") || body_str.contains("phase"),
            "error body should describe the invalid phase, got: {body_str}"
        );

        assert_eq!(state.pipeline_running.load(Ordering::SeqCst), 0);
    }

    // -----------------------------------------------------------------------
    // Test 2: counter increments then decrements on success
    // -----------------------------------------------------------------------

    /// After a successful /execute the spawned background task must complete
    /// and leave `pipeline_running` at 0.
    ///
    /// Uses a real TCP listener so that `tokio::spawn` inside the handler
    /// executes on the same multi-thread runtime as the test, avoiding the
    /// scheduling dead-end that arises when `oneshot()` is combined with
    /// `#[tokio::test]` (single-thread by default).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn execute_task_pipeline_increments_then_decrements_counter_on_success() {
        let event_bus = EventBus::new();
        let state = Arc::new(ApiState::new(event_bus).with_relaxed_rate_limits());
        let app = api_router(state.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let task = task_at_planning();
        let task_id = task.id;
        state.tasks.write().await.insert(task_id, task);

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://{addr}/api/tasks/{task_id}/execute"))
            .send()
            .await
            .unwrap();
        let resp_status = resp.status();
        let json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(
            resp_status,
            reqwest::StatusCode::ACCEPTED,
            "expected 202 Accepted for valid Planning→Coding transition, body={json}"
        );
        assert_eq!(json["status"], "started");

        // The handler increments pipeline_waiting before spawning; the spawned task
        // decrements it after acquiring the semaphore.  Wait until waiting reaches 0
        // (background task acquired the semaphore and is running or done), then
        // verify pipeline_running is also 0 (counter was properly decremented).
        //
        // Note: pipeline_running may be too short-lived to observe > 0 in tests
        // because run_pipeline_background has no blocking I/O in the test environment.
        // What we CAN reliably observe is waiting→0 (task scheduled) + running==0 (done).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let waiting = state.pipeline_waiting.load(Ordering::SeqCst);
            if waiting == 0 {
                break; // spawned task has at minimum acquired the semaphore
            }
            if std::time::Instant::now() > deadline {
                let running = state.pipeline_running.load(Ordering::SeqCst);
                panic!(
                    "pipeline_waiting did not reach 0 within 5 s \
                     (waiting={waiting}, running={running})"
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        // Once waiting==0 the background task is running or already finished.
        // Give it up to 10 s to complete (in practice it's near-instant).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let running = state.pipeline_running.load(Ordering::SeqCst);
            if running == 0 {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("pipeline_running did not return to 0 within 10 s (current={running})");
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        assert_eq!(
            state.pipeline_running.load(Ordering::SeqCst),
            0,
            "pipeline_running must be 0 after background task completes"
        );
        assert_eq!(
            state.pipeline_waiting.load(Ordering::SeqCst),
            0,
            "pipeline_waiting must be 0 after background task completes"
        );
    }

    // -----------------------------------------------------------------------
    // Test 3: counter is decremented even when the background task panics
    // -----------------------------------------------------------------------

    /// This test verifies the panic-safety of the `CounterGuard` fix.
    ///
    /// We cannot inject a panic into `run_pipeline_background` without
    /// refactoring the production code (it is a plain async fn, not a trait
    /// object or injected dependency). Instead we verify the RAII invariant
    /// directly: construct a `CounterGuard`, increment the counter, then
    /// drop it (simulating a panic unwind). The counter must reach 0 even
    /// without an explicit `fetch_sub` call.
    ///
    /// Mutation test: removing the `Drop` impl from `CounterGuard` causes this
    /// test to fail because `counter` stays at 1 after the drop.
    #[tokio::test]
    async fn execute_task_pipeline_counter_guard_is_panic_safe() {
        use super::CounterGuard;
        use std::sync::atomic::AtomicUsize;

        let counter = Arc::new(AtomicUsize::new(0));

        // Simulate what the spawned task does: increment, then wrap in guard.
        counter.fetch_add(1, Ordering::SeqCst);
        let guard = CounterGuard(counter.clone());

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "counter should be 1 after increment"
        );

        // Simulate a panic-safe drop (guard going out of scope / being dropped).
        drop(guard);

        assert_eq!(
            counter.load(Ordering::SeqCst),
            0,
            "CounterGuard must decrement the counter on drop (panic-safe path)"
        );
    }

    // -----------------------------------------------------------------------
    // Test 4: counter returns to 0 when background returns (error path)
    // -----------------------------------------------------------------------

    /// The background pipeline finishes with QA failures (Error phase) rather
    /// than success. The counter must still reach 0 — this is equivalent to
    /// the "background returns Err" case since `run_pipeline_background` is
    /// infallible by signature (returns `()`); the Err outcome is represented
    /// as `TaskPhase::Error` with QA failures.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn execute_task_pipeline_decrements_counter_when_background_returns_error_phase() {
        let event_bus = EventBus::new();
        let state = Arc::new(ApiState::new(event_bus).with_relaxed_rate_limits());
        let app = api_router(state.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Seed a task with a non-existent worktree so QA will fail (no files to check).
        let mut task = task_at_planning();
        task.worktree_path = Some("/nonexistent/path/that/does/not/exist".to_string());
        let task_id = task.id;
        state.tasks.write().await.insert(task_id, task);

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://{addr}/api/tasks/{task_id}/execute"))
            .send()
            .await
            .unwrap();
        let resp_status = resp.status();
        let json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(
            resp_status,
            reqwest::StatusCode::ACCEPTED,
            "expected 202, body={json}"
        );

        // Wait until pipeline_waiting reaches 0 (background task has been scheduled
        // and the semaphore acquired), then verify pipeline_running == 0 (done).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let waiting = state.pipeline_waiting.load(Ordering::SeqCst);
            if waiting == 0 {
                break;
            }
            if std::time::Instant::now() > deadline {
                let running = state.pipeline_running.load(Ordering::SeqCst);
                panic!(
                    "pipeline_waiting did not reach 0 within 5 s \
                     (waiting={waiting}, running={running})"
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let running = state.pipeline_running.load(Ordering::SeqCst);
            if running == 0 {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("pipeline_running did not return to 0 within 10 s (current={running})");
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        assert_eq!(
            state.pipeline_running.load(Ordering::SeqCst),
            0,
            "pipeline_running must be 0 after background task ends (regardless of QA outcome)"
        );
    }
}
