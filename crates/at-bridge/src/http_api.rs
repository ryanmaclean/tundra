use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

use at_core::types::{
    Agent, Bead, BeadStatus, KpiSnapshot, Lane, Task, TaskCategory, TaskComplexity, TaskPhase,
    TaskPriority,
};
use crate::event_bus::EventBus;

/// Shared application state for all HTTP/WS handlers.
pub struct ApiState {
    pub event_bus: EventBus,
    pub beads: Arc<RwLock<Vec<Bead>>>,
    pub agents: Arc<RwLock<Vec<Agent>>>,
    pub kpi: Arc<RwLock<KpiSnapshot>>,
    pub tasks: Arc<RwLock<Vec<Task>>>,
    pub start_time: std::time::Instant,
}

impl ApiState {
    /// Create a new `ApiState` with empty collections and a fresh event bus.
    pub fn new(event_bus: EventBus) -> Self {
        Self {
            event_bus,
            beads: Arc::new(RwLock::new(Vec::new())),
            agents: Arc::new(RwLock::new(Vec::new())),
            kpi: Arc::new(RwLock::new(KpiSnapshot {
                total_beads: 0,
                backlog: 0,
                hooked: 0,
                slung: 0,
                review: 0,
                done: 0,
                failed: 0,
                escalated: 0,
                active_agents: 0,
                timestamp: chrono::Utc::now(),
            })),
            tasks: Arc::new(RwLock::new(Vec::new())),
            start_time: std::time::Instant::now(),
        }
    }
}

/// Build the full API router with all REST and WebSocket routes.
pub fn api_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/api/status", get(get_status))
        .route("/api/beads", get(list_beads))
        .route("/api/beads", post(create_bead))
        .route("/api/beads/{id}/status", post(update_bead_status))
        .route("/api/agents", get(list_agents))
        .route("/api/kpi", get(get_kpi))
        .route("/api/tasks", get(list_tasks))
        .route("/api/tasks", post(create_task))
        .route("/api/tasks/{id}", get(get_task))
        .route("/api/tasks/{id}/phase", post(update_task_phase))
        .route("/api/tasks/{id}/logs", get(get_task_logs))
        .route("/ws", get(ws_handler))
        .layer(CorsLayer::very_permissive())
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct StatusResponse {
    version: String,
    uptime_seconds: u64,
    agent_count: usize,
    bead_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct CreateBeadRequest {
    pub title: String,
    pub description: Option<String>,
    pub lane: Option<Lane>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBeadStatusRequest {
    pub status: BeadStatus,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub bead_id: Uuid,
    pub category: TaskCategory,
    pub priority: TaskPriority,
    pub complexity: TaskComplexity,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskPhaseRequest {
    pub phase: TaskPhase,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn get_status(State(state): State<Arc<ApiState>>) -> Json<StatusResponse> {
    let beads = state.beads.read().await;
    let agents = state.agents.read().await;
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.start_time.elapsed().as_secs(),
        agent_count: agents.len(),
        bead_count: beads.len(),
    })
}

async fn list_beads(State(state): State<Arc<ApiState>>) -> Json<Vec<Bead>> {
    let beads = state.beads.read().await;
    Json(beads.clone())
}

async fn create_bead(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateBeadRequest>,
) -> impl IntoResponse {
    let lane = req.lane.unwrap_or(Lane::Standard);
    let mut bead = Bead::new(req.title, lane);
    bead.description = req.description;

    let mut beads = state.beads.write().await;
    beads.push(bead.clone());

    // Publish event
    state.event_bus.publish(crate::protocol::BridgeMessage::BeadList(beads.clone()));

    (axum::http::StatusCode::CREATED, Json(bead))
}

async fn update_bead_status(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateBeadStatusRequest>,
) -> impl IntoResponse {
    let mut beads = state.beads.write().await;
    let Some(bead) = beads.iter_mut().find(|b| b.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "bead not found"})),
        );
    };

    if !bead.status.can_transition_to(&req.status) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "invalid transition from {:?} to {:?}",
                    bead.status, req.status
                )
            })),
        );
    }

    bead.status = req.status;
    bead.updated_at = chrono::Utc::now();

    let bead_snapshot = bead.clone();
    state.event_bus.publish(crate::protocol::BridgeMessage::BeadList(beads.clone()));

    (axum::http::StatusCode::OK, Json(serde_json::json!(bead_snapshot)))
}

async fn list_agents(State(state): State<Arc<ApiState>>) -> Json<Vec<Agent>> {
    let agents = state.agents.read().await;
    Json(agents.clone())
}

async fn get_kpi(State(state): State<Arc<ApiState>>) -> Json<KpiSnapshot> {
    let kpi = state.kpi.read().await;
    Json(kpi.clone())
}

// ---------------------------------------------------------------------------
// Task handlers
// ---------------------------------------------------------------------------

async fn list_tasks(State(state): State<Arc<ApiState>>) -> Json<Vec<Task>> {
    let tasks = state.tasks.read().await;
    Json(tasks.clone())
}

async fn create_task(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    let task = Task::new(req.title, req.bead_id, req.category, req.priority, req.complexity);

    let mut tasks = state.tasks.write().await;
    tasks.push(task.clone());

    (axum::http::StatusCode::CREATED, Json(task))
}

async fn get_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };
    (axum::http::StatusCode::OK, Json(serde_json::json!(task)))
}

async fn update_task_phase(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTaskPhaseRequest>,
) -> impl IntoResponse {
    let mut tasks = state.tasks.write().await;
    let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };

    if !task.phase.can_transition_to(&req.phase) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "invalid phase transition from {:?} to {:?}",
                    task.phase, req.phase
                )
            })),
        );
    }

    task.set_phase(req.phase);
    let task_snapshot = task.clone();

    (axum::http::StatusCode::OK, Json(serde_json::json!(task_snapshot)))
}

async fn get_task_logs(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.iter().find(|t| t.id == id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found"})),
        );
    };
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(task.logs)),
    )
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<ApiState>) {
    let rx = state.event_bus.subscribe();
    loop {
        match rx.recv_async().await {
            Ok(msg) => {
                let json = serde_json::to_string(&msg).unwrap_or_default();
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
