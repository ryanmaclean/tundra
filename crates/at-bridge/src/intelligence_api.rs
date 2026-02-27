//! Intelligence API endpoints.
//!
//! Exposes the `at-intelligence` engines (Insights, Ideation, Roadmap,
//! Changelog, Memory) over HTTP/JSON using Axum.

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::Datelike;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use at_core::context_engine::ProjectContextLoader;
use at_intelligence::{
    ideation::{EffortLevel, IdeaCategory},
    insights::ChatRole,
    memory::{MemoryCategory, MemoryEntry},
    roadmap::{FeatureStatus, RoadmapFeature},
};

use crate::http_api::{simulate_planning_poker_for_bead, ApiState, SimulatePlanningPokerRequest};

// ---------------------------------------------------------------------------
// Request / query types
// ---------------------------------------------------------------------------

/// Request body for creating a new insights chat session.
///
/// **Example:**
/// ```json
/// {
///   "title": "Performance Analysis Q1 2026",
///   "model": "gpt-4"
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub title: String,
    pub model: String,
}

/// Request body for adding a user message to an insights session.
///
/// **Example:**
/// ```json
/// {
///   "content": "What are the top performance bottlenecks in our codebase?"
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct AddMessageRequest {
    pub content: String,
}

/// Request body for AI-powered idea generation.
///
/// Specify a category and optional context to generate relevant ideas.
/// Falls back to deterministic generation if no LLM provider is configured.
///
/// **Example:**
/// ```json
/// {
///   "category": "CodeImprovement",
///   "context": "We have slow database queries in the user service"
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct GenerateIdeasRequest {
    pub category: IdeaCategory,
    pub context: String,
}

/// Request body for creating a new roadmap.
///
/// **Example:**
/// ```json
/// {
///   "name": "Q1 2026 Product Roadmap"
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct CreateRoadmapRequest {
    pub name: String,
}

/// Request body for AI-powered roadmap generation from codebase analysis.
///
/// **Example:**
/// ```json
/// {
///   "analysis": "Feature: Auth system | Description: OAuth2 integration | Priority: 1\nFeature: Dashboard | Description: Admin dashboard | Priority: 2"
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct GenerateRoadmapRequest {
    pub analysis: String,
}

/// Request body for adding a feature to a specific roadmap.
///
/// Priority is 1-5 where 1 is highest priority.
///
/// **Example:**
/// ```json
/// {
///   "title": "Real-time Notifications",
///   "description": "WebSocket-based notification system",
///   "priority": 2
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct AddFeatureRequest {
    pub title: String,
    pub description: String,
    pub priority: u8,
}

/// Request body for updating a roadmap feature's status.
///
/// **Example:**
/// ```json
/// {
///   "status": "InProgress"
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct UpdateFeatureStatusRequest {
    pub status: FeatureStatus,
}

/// Request body for adding a feature to the latest roadmap.
///
/// Simplified version of AddFeatureRequest with string-based priority
/// that accepts both numeric (1-5) and textual values (critical, high, medium, low, lowest).
///
/// **Example:**
/// ```json
/// {
///   "title": "User Profile Page",
///   "description": "Customizable user profile with avatar upload",
///   "status": "Planned",
///   "priority": "high"
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct AddFeatureToLatestRequest {
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: String,
}

/// Request body for storing a memory entry.
///
/// Memory entries are key-value pairs with categorization and source tracking
/// for project knowledge management.
///
/// **Example:**
/// ```json
/// {
///   "key": "database_connection_string_format",
///   "value": "postgresql://user:pass@host:port/dbname",
///   "category": "Technical",
///   "source": "onboarding-doc"
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct AddMemoryRequest {
    pub key: String,
    pub value: String,
    pub category: MemoryCategory,
    pub source: String,
}

/// Query parameters for memory search.
///
/// **Example:**
/// ```
/// GET /api/memory/search?q=database
/// ```
#[derive(Debug, Deserialize)]
pub struct MemorySearchQuery {
    pub q: String,
}

/// Request body for generating a changelog entry from commit messages.
///
/// Parses conventional commit format (feat:, fix:, etc.) and groups changes
/// into Added, Changed, Fixed, and Security sections.
///
/// **Example:**
/// ```json
/// {
///   "commits": "feat: add user authentication\nfix: resolve null pointer in parser\nsecurity: patch XSS vulnerability",
///   "version": "1.2.0"
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct GenerateChangelogRequest {
    pub commits: String,
    pub version: String,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the intelligence sub-router.
///
/// All routes are mounted under `/api/` — the caller is responsible for
/// nesting or merging this into the top-level router.
pub fn intelligence_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Insights
        .route("/api/insights/sessions", get(list_sessions))
        .route("/api/insights/sessions", post(create_session))
        .route(
            "/api/insights/sessions/{id}",
            axum::routing::delete(delete_session),
        )
        .route(
            "/api/insights/sessions/{id}/messages",
            get(get_session_messages).post(add_message),
        )
        // Ideation
        .route("/api/ideation/ideas", get(list_ideas))
        .route("/api/ideation/generate", post(generate_ideas))
        .route("/api/ideation/ideas/{id}/convert", post(convert_idea))
        // Roadmap
        .route("/api/roadmap", get(list_roadmaps))
        .route("/api/roadmap", post(create_roadmap))
        .route("/api/roadmap/generate", post(generate_roadmap))
        .route("/api/roadmap/features", post(add_feature_to_latest))
        .route("/api/roadmap/{id}/features", post(add_feature))
        .route(
            "/api/roadmap/{id}/features/{fid}",
            patch(update_feature_status),
        )
        .route(
            "/api/roadmap/features/{fid}/status",
            axum::routing::put(update_feature_status_by_id),
        )
        // Memory
        .route("/api/memory", get(list_memory))
        .route("/api/memory", post(add_memory))
        .route("/api/memory/search", get(search_memory))
        .route("/api/memory/{id}", axum::routing::delete(delete_memory))
        // Changelog
        .route("/api/changelog", get(get_changelog))
        .route("/api/changelog/generate", post(generate_changelog))
        // Context
        .route("/api/context", get(get_context))
}

// ---------------------------------------------------------------------------
// Insights handlers
// ---------------------------------------------------------------------------

/// GET /api/insights/sessions -- retrieve all chat sessions.
///
/// Returns an array of all insight chat sessions with their ID, title,
/// model configuration, creation timestamp, and message count.
///
/// **Response:** 200 OK with array of session objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "title": "Performance Analysis Q1 2026",
///     "model": "gpt-4",
///     "created_at": "2026-02-20T14:30:00Z",
///     "messages": []
///   }
/// ]
/// ```
async fn list_sessions(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let engine = state.insights_engine.read().await;
    let sessions = engine.list_sessions().to_vec();
    Json(serde_json::json!(sessions))
}

/// POST /api/insights/sessions -- create a new chat session.
///
/// Creates a new insights session for AI-powered codebase analysis and Q&A.
/// Sessions maintain conversation history and can use different LLM models.
///
/// **Request:** JSON body with title and model.
///
/// **Response:** 201 Created with the new session object.
///
/// **Example Request:**
/// ```json
/// {
///   "title": "Performance Analysis Q1 2026",
///   "model": "gpt-4"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000",
///   "title": "Performance Analysis Q1 2026",
///   "model": "gpt-4",
///   "created_at": "2026-02-27T10:00:00Z",
///   "messages": []
/// }
/// ```
async fn create_session(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let mut engine = state.insights_engine.write().await;
    let session = engine.create_session(&req.title, &req.model).clone();
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(session)),
    )
}

/// DELETE /api/insights/sessions/{id} -- delete a chat session.
///
/// Permanently removes a session and all its message history.
///
/// **Response:** 200 OK if deleted, 404 Not Found if session doesn't exist.
///
/// **Example Success Response:**
/// ```json
/// {
///   "deleted": true
/// }
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "session not found"
/// }
/// ```
async fn delete_session(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut engine = state.insights_engine.write().await;
    if engine.delete_session(&id) {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"deleted": true})),
        )
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "session not found"})),
        )
    }
}

/// GET /api/insights/sessions/{id}/messages -- retrieve session messages.
///
/// Returns all messages in a chat session including user queries and AI responses.
///
/// **Response:** 200 OK with message array, 404 Not Found if session doesn't exist.
///
/// **Example Success Response:**
/// ```json
/// [
///   {
///     "role": "User",
///     "content": "What are the top performance bottlenecks?",
///     "timestamp": "2026-02-27T10:00:00Z"
///   },
///   {
///     "role": "Assistant",
///     "content": "Based on the codebase analysis, the main bottlenecks are...",
///     "timestamp": "2026-02-27T10:00:05Z"
///   }
/// ]
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "session not found"
/// }
/// ```
async fn get_session_messages(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let engine = state.insights_engine.read().await;
    match engine.get_session(&id) {
        Some(session) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!(session.messages)),
        ),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "session not found"})),
        ),
    }
}

/// POST /api/insights/sessions/{id}/messages -- add a user message to a session.
///
/// Adds a user message to the specified chat session. The message is stored
/// with a User role and timestamp.
///
/// **Request:** JSON body with message content.
///
/// **Response:** 201 Created if successful, 404 Not Found if session doesn't exist.
///
/// **Example Request:**
/// ```json
/// {
///   "content": "What are the top performance bottlenecks in our codebase?"
/// }
/// ```
///
/// **Example Success Response:**
/// ```json
/// {
///   "ok": true
/// }
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "session not found"
/// }
/// ```
async fn add_message(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<AddMessageRequest>,
) -> impl IntoResponse {
    let mut engine = state.insights_engine.write().await;
    match engine.add_message(&id, ChatRole::User, &req.content) {
        Ok(()) => (
            axum::http::StatusCode::CREATED,
            Json(serde_json::json!({"ok": true})),
        ),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

// ---------------------------------------------------------------------------
// Ideation handlers
// ---------------------------------------------------------------------------

/// GET /api/ideation/ideas -- retrieve all generated ideas.
///
/// Returns an array of all ideas generated by the ideation engine,
/// including their category, effort estimation, and metadata.
///
/// **Response:** 200 OK with array of idea objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "title": "Implement Database Connection Pooling",
///     "description": "Add connection pooling to reduce database latency",
///     "category": "CodeImprovement",
///     "effort": "Medium",
///     "created_at": "2026-02-27T10:00:00Z",
///     "converted": false
///   }
/// ]
/// ```
async fn list_ideas(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let engine = state.ideation_engine.read().await;
    let ideas = engine.list_ideas().to_vec();
    Json(serde_json::json!(ideas))
}

/// POST /api/ideation/generate -- generate new ideas using AI.
///
/// Generates ideas based on a category and optional context. Uses AI-powered
/// generation when an LLM provider is configured, falls back to deterministic
/// generation in offline mode or tests.
///
/// **Request:** Optional JSON body with category and context. Defaults to
/// CodeImprovement category with empty context if not provided.
///
/// **Response:** 201 Created with generated ideas.
///
/// **Example Request:**
/// ```json
/// {
///   "category": "CodeImprovement",
///   "context": "We have slow database queries in the user service"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "ideas": [
///     {
///       "id": "550e8400-e29b-41d4-a716-446655440000",
///       "title": "Implement Database Query Caching",
///       "description": "Add Redis caching layer for frequently accessed user data",
///       "category": "CodeImprovement",
///       "effort": "Medium",
///       "created_at": "2026-02-27T10:00:00Z",
///       "converted": false
///     }
///   ]
/// }
/// ```
async fn generate_ideas(
    State(state): State<Arc<ApiState>>,
    body: Option<Json<GenerateIdeasRequest>>,
) -> impl IntoResponse {
    let (category, context) = match body {
        Some(Json(req)) => (req.category, req.context),
        None => (IdeaCategory::CodeImprovement, String::new()),
    };
    let mut engine = state.ideation_engine.write().await;
    // Try AI-powered ideation first; fall back to deterministic generation
    // when no LLM provider is configured (e.g. in tests or offline mode).
    let result = match engine.generate_ideas_with_ai(&category, &context).await {
        Ok(result) => result,
        Err(_) => engine.generate_ideas(&category, &context),
    };
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(result)),
    )
}

/// POST /api/ideation/ideas/{id}/convert -- convert an idea to a task (bead).
///
/// Converts an ideation idea into a task bead and adds it to the system.
/// Automatically runs a planning poker simulation to estimate effort based
/// on the idea's effort level.
///
/// **Response:** 200 OK with the created bead and planning poker session,
/// 404 Not Found if idea doesn't exist.
///
/// **Example Success Response:**
/// ```json
/// {
///   "id": "660e8400-e29b-41d4-a716-446655440001",
///   "title": "Implement Database Query Caching",
///   "description": "Add Redis caching layer for frequently accessed user data",
///   "status": "New",
///   "lane": "Standard",
///   "priority": 50,
///   "created_at": "2026-02-27T10:00:00Z",
///   "planning_poker": {
///     "session_id": "770e8400-e29b-41d4-a716-446655440002",
///     "consensus": "5",
///     "votes": [...]
///   }
/// }
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "idea not found"
/// }
/// ```
async fn convert_idea(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let (idea_effort, bead) = {
        let engine = state.ideation_engine.read().await;
        let Some(idea) = engine.get_idea(&id).cloned() else {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "idea not found"})),
            );
        };
        let Some(bead) = engine.convert_to_task(&id) else {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "idea not found"})),
            );
        };
        (idea.effort, bead)
    };

    {
        let mut beads = state.beads.write().await;
        beads.insert(bead.id, bead.clone());
    }

    let simulation = simulate_planning_poker_for_bead(
        &state,
        SimulatePlanningPokerRequest {
            bead_id: bead.id,
            virtual_agents: Vec::new(),
            agent_count: Some(5),
            deck_preset: None,
            custom_deck: None,
            round_duration_seconds: None,
            focus_card: Some(effort_to_focus_card(&idea_effort).to_string()),
            seed: Some(id.as_u128() as u64),
            auto_reveal: true,
        },
    )
    .await;

    let mut payload = serde_json::to_value(&bead).unwrap_or_else(|_| serde_json::json!({}));
    if let serde_json::Value::Object(map) = &mut payload {
        match simulation {
            Ok(session) => {
                map.insert(
                    "planning_poker".to_string(),
                    serde_json::to_value(session).unwrap_or(serde_json::Value::Null),
                );
            }
            Err((status, err)) => {
                map.insert(
                    "planning_poker_error".to_string(),
                    serde_json::json!({
                        "status": status.as_u16(),
                        "details": err
                    }),
                );
            }
        }
    }

    (axum::http::StatusCode::OK, Json(payload))
}

fn effort_to_focus_card(effort: &EffortLevel) -> &'static str {
    match effort {
        EffortLevel::Trivial => "1",
        EffortLevel::Small => "3",
        EffortLevel::Medium => "5",
        EffortLevel::Large => "8",
        EffortLevel::Massive => "13",
    }
}

// ---------------------------------------------------------------------------
// Roadmap handlers
// ---------------------------------------------------------------------------

/// GET /api/roadmap -- retrieve all roadmaps.
///
/// Returns an array of all product roadmaps with their features, priorities,
/// and status information.
///
/// **Response:** 200 OK with array of roadmap objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "name": "Q1 2026 Product Roadmap",
///     "created_at": "2026-02-20T14:30:00Z",
///     "features": [
///       {
///         "id": "660e8400-e29b-41d4-a716-446655440001",
///         "title": "Real-time Notifications",
///         "description": "WebSocket-based notification system",
///         "priority": 2,
///         "status": "Planned",
///         "created_at": "2026-02-20T15:00:00Z"
///       }
///     ]
///   }
/// ]
/// ```
async fn list_roadmaps(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let engine = state.roadmap_engine.read().await;
    let roadmaps = engine.list_roadmaps().to_vec();
    Json(serde_json::json!(roadmaps))
}

/// POST /api/roadmap -- create a new roadmap.
///
/// Creates an empty roadmap with the specified name. Features can be added
/// later via the add feature endpoints.
///
/// **Request:** JSON body with roadmap name.
///
/// **Response:** 201 Created with the new roadmap object.
///
/// **Example Request:**
/// ```json
/// {
///   "name": "Q1 2026 Product Roadmap"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000",
///   "name": "Q1 2026 Product Roadmap",
///   "created_at": "2026-02-27T10:00:00Z",
///   "features": []
/// }
/// ```
async fn create_roadmap(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateRoadmapRequest>,
) -> impl IntoResponse {
    let mut engine = state.roadmap_engine.write().await;
    let roadmap = engine.create_roadmap(&req.name).clone();
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(roadmap)),
    )
}

/// POST /api/roadmap/generate -- generate a roadmap from codebase analysis.
///
/// Parses a structured analysis text (typically from AI) and generates a
/// roadmap with features. Expects lines in format:
/// `- Feature: <title> | Description: <desc> | Priority: <num>`
///
/// **Request:** Optional JSON body with analysis text. Defaults to empty
/// analysis if not provided, creating an empty "Generated Roadmap".
///
/// **Response:** 201 Created with the generated roadmap.
///
/// **Example Request:**
/// ```json
/// {
///   "analysis": "- Feature: Auth system | Description: OAuth2 integration | Priority: 1\n- Feature: Dashboard | Description: Admin dashboard | Priority: 2"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000",
///   "name": "Generated Roadmap",
///   "created_at": "2026-02-27T10:00:00Z",
///   "features": [
///     {
///       "id": "660e8400-e29b-41d4-a716-446655440001",
///       "title": "Auth system",
///       "description": "OAuth2 integration",
///       "priority": 1,
///       "status": "Planned",
///       "created_at": "2026-02-27T10:00:00Z"
///     }
///   ]
/// }
/// ```
async fn generate_roadmap(
    State(state): State<Arc<ApiState>>,
    body: Option<Json<GenerateRoadmapRequest>>,
) -> impl IntoResponse {
    let analysis = match body {
        Some(Json(req)) => req.analysis,
        None => String::new(),
    };
    let mut engine = state.roadmap_engine.write().await;
    let roadmap = engine.generate_from_codebase(&analysis).clone();
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(roadmap)),
    )
}

/// POST /api/roadmap/{id}/features -- add a feature to a specific roadmap.
///
/// Adds a new feature to the roadmap identified by the path parameter.
/// Priority is 1-5 where 1 is highest priority.
///
/// **Request:** JSON body with feature details.
///
/// **Response:** 201 Created with the new feature, 404 Not Found if roadmap doesn't exist.
///
/// **Example Request:**
/// ```json
/// {
///   "title": "Real-time Notifications",
///   "description": "WebSocket-based notification system",
///   "priority": 2
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "660e8400-e29b-41d4-a716-446655440001",
///   "title": "Real-time Notifications",
///   "description": "WebSocket-based notification system",
///   "priority": 2,
///   "status": "Planned",
///   "created_at": "2026-02-27T10:00:00Z"
/// }
/// ```
async fn add_feature(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<AddFeatureRequest>,
) -> impl IntoResponse {
    let mut engine = state.roadmap_engine.write().await;
    let feature = RoadmapFeature::new(&req.title, &req.description, req.priority);
    let feature_json = serde_json::json!(feature);
    match engine.add_feature(&id, feature) {
        Ok(()) => (axum::http::StatusCode::CREATED, Json(feature_json)),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// POST /api/roadmap/features -- add a feature to the most recently created roadmap.
///
/// Simplified endpoint that adds a feature to the latest roadmap. If no roadmap exists,
/// automatically creates a "Default Roadmap". Accepts both numeric (1-5) and textual
/// priority values (critical, high, medium, low, lowest).
///
/// **Request:** JSON body with feature details using string-based priority.
///
/// **Response:** 201 Created with the new feature, 404 Not Found on error.
///
/// **Example Request:**
/// ```json
/// {
///   "title": "User Profile Page",
///   "description": "Customizable user profile with avatar upload",
///   "status": "Planned",
///   "priority": "high"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "660e8400-e29b-41d4-a716-446655440001",
///   "title": "User Profile Page",
///   "description": "Customizable user profile with avatar upload",
///   "priority": 2,
///   "status": "Planned",
///   "created_at": "2026-02-27T10:00:00Z"
/// }
/// ```
async fn add_feature_to_latest(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<AddFeatureToLatestRequest>,
) -> impl IntoResponse {
    let mut engine = state.roadmap_engine.write().await;
    let roadmaps = engine.list_roadmaps();
    let roadmap_id = match roadmaps.last() {
        Some(r) => r.id,
        None => {
            // Auto-create a default roadmap if none exists
            let r = engine.create_roadmap("Default Roadmap");
            r.id
        }
    };
    let priority =
        req.priority
            .parse::<u8>()
            .unwrap_or_else(|_| match req.priority.to_lowercase().as_str() {
                "critical" | "highest" => 1,
                "high" => 2,
                "medium" | "normal" => 3,
                "low" => 4,
                "lowest" => 5,
                _ => 3,
            });
    let feature = RoadmapFeature::new(&req.title, &req.description, priority);
    let feature_json = serde_json::json!(feature);
    match engine.add_feature(&roadmap_id, feature) {
        Ok(()) => (axum::http::StatusCode::CREATED, Json(feature_json)),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// PATCH /api/roadmap/{id}/features/{fid} -- update a feature's status.
///
/// Updates the status of a specific feature within a specific roadmap.
/// Requires both roadmap ID and feature ID.
///
/// **Request:** JSON body with new status.
///
/// **Response:** 200 OK if updated, 404 Not Found if roadmap or feature doesn't exist.
///
/// **Example Request:**
/// ```json
/// {
///   "status": "InProgress"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "updated": true
/// }
/// ```
async fn update_feature_status(
    State(state): State<Arc<ApiState>>,
    Path((id, fid)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateFeatureStatusRequest>,
) -> impl IntoResponse {
    let mut engine = state.roadmap_engine.write().await;
    match engine.update_feature_status(&id, &fid, req.status) {
        Ok(()) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"updated": true})),
        ),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// PUT /api/roadmap/features/{fid}/status -- update a feature's status by feature ID.
///
/// Updates a feature's status by searching across all roadmaps for the feature ID.
/// Useful when you know the feature ID but not which roadmap it belongs to.
///
/// **Request:** JSON body with new status.
///
/// **Response:** 200 OK if updated, 404 Not Found if feature doesn't exist in any roadmap.
///
/// **Example Request:**
/// ```json
/// {
///   "status": "InProgress"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "updated": true
/// }
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "feature not found in any roadmap"
/// }
/// ```
async fn update_feature_status_by_id(
    State(state): State<Arc<ApiState>>,
    Path(fid): Path<Uuid>,
    Json(req): Json<UpdateFeatureStatusRequest>,
) -> impl IntoResponse {
    let mut engine = state.roadmap_engine.write().await;
    // Find the roadmap that contains this feature
    let roadmap_id = engine
        .list_roadmaps()
        .iter()
        .find(|r| r.features.iter().any(|f| f.id == fid))
        .map(|r| r.id);

    match roadmap_id {
        Some(rid) => match engine.update_feature_status(&rid, &fid, req.status) {
            Ok(()) => (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({"updated": true})),
            ),
            Err(e) => (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": e.to_string()})),
            ),
        },
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "feature not found in any roadmap"})),
        ),
    }
}

// ---------------------------------------------------------------------------
// Memory handlers
// ---------------------------------------------------------------------------

/// GET /api/memory -- retrieve all memory entries.
///
/// Returns all stored memory entries including key, value, category, source,
/// and timestamps. Memory entries store project knowledge, patterns, and
/// important information for future reference.
///
/// **Response:** 200 OK with array of memory entry objects.
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "key": "database_connection_string_format",
///     "value": "postgresql://user:pass@host:port/dbname",
///     "category": "Technical",
///     "source": "onboarding-doc",
///     "created_at": "2026-02-20T14:30:00Z",
///     "accessed_at": "2026-02-27T10:00:00Z",
///     "access_count": 5
///   }
/// ]
/// ```
async fn list_memory(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let store = state.memory_store.read().await;
    // search("") matches every entry because every string contains "".
    let entries: Vec<_> = store.search("").into_iter().cloned().collect();
    Json(serde_json::json!(entries))
}

/// POST /api/memory -- add a new memory entry.
///
/// Stores a new key-value pair in the memory system with categorization
/// and source tracking. Useful for recording project patterns, decisions,
/// and important technical details.
///
/// **Request:** JSON body with key, value, category, and source.
///
/// **Response:** 201 Created with the new memory entry ID.
///
/// **Example Request:**
/// ```json
/// {
///   "key": "database_connection_string_format",
///   "value": "postgresql://user:pass@host:port/dbname",
///   "category": "Technical",
///   "source": "onboarding-doc"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000"
/// }
/// ```
async fn add_memory(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<AddMemoryRequest>,
) -> impl IntoResponse {
    let mut store = state.memory_store.write().await;
    let entry = MemoryEntry::new(req.key, req.value, req.category, req.source);
    let id = store.add_entry(entry);
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({"id": id})),
    )
}

/// GET /api/memory/search?q={query} -- search memory entries.
///
/// Searches memory entries for matches in keys, values, categories, or sources.
/// Case-insensitive substring matching.
///
/// **Query Parameters:**
/// - `q`: Search query string (required)
///
/// **Response:** 200 OK with array of matching memory entries.
///
/// **Example Request:**
/// ```
/// GET /api/memory/search?q=database
/// ```
///
/// **Example Response:**
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "key": "database_connection_string_format",
///     "value": "postgresql://user:pass@host:port/dbname",
///     "category": "Technical",
///     "source": "onboarding-doc",
///     "created_at": "2026-02-20T14:30:00Z",
///     "accessed_at": "2026-02-27T10:00:00Z",
///     "access_count": 5
///   }
/// ]
/// ```
async fn search_memory(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MemorySearchQuery>,
) -> impl IntoResponse {
    let store = state.memory_store.read().await;
    let results: Vec<_> = store.search(&q.q).into_iter().cloned().collect();
    Json(serde_json::json!(results))
}

/// DELETE /api/memory/{id} -- delete a memory entry.
///
/// Permanently removes a memory entry from the system.
///
/// **Response:** 200 OK if deleted, 404 Not Found if entry doesn't exist.
///
/// **Example Success Response:**
/// ```json
/// {
///   "deleted": true
/// }
/// ```
///
/// **Example Error Response:**
/// ```json
/// {
///   "error": "memory entry not found"
/// }
/// ```
async fn delete_memory(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut store = state.memory_store.write().await;
    if store.delete_entry(&id) {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"deleted": true})),
        )
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "memory entry not found"})),
        )
    }
}

// ---------------------------------------------------------------------------
// Changelog handlers
// ---------------------------------------------------------------------------

/// Query parameters for changelog retrieval.
///
/// **Example:**
/// ```
/// GET /api/changelog?source=tasks
/// ```
#[derive(Debug, Deserialize)]
pub struct ChangelogQuery {
    #[serde(default)]
    pub source: Option<String>,
}

/// GET /api/changelog -- retrieve changelog entries or generate from tasks.
///
/// Returns changelog entries. When `source=tasks` query parameter is provided,
/// generates a changelog from completed tasks instead of returning stored entries.
/// Automatically categorizes tasks into feat, fix, refactor, docs, security, etc.
///
/// **Query Parameters:**
/// - `source`: Optional. Set to "tasks" to generate from completed tasks.
///
/// **Response:** 200 OK with changelog entries or generated markdown.
///
/// **Example Request (list mode):**
/// ```
/// GET /api/changelog
/// ```
///
/// **Example Response (list mode):**
/// ```json
/// [
///   {
///     "version": "1.2.0",
///     "date": "2026-02-27",
///     "sections": [
///       {
///         "category": "Added",
///         "items": ["User authentication system"]
///       }
///     ]
///   }
/// ]
/// ```
///
/// **Example Request (task-generated mode):**
/// ```
/// GET /api/changelog?source=tasks
/// ```
///
/// **Example Response (task-generated mode):**
/// ```json
/// {
///   "markdown": "# Changelog\n\n## [2026.2.27]\n\n### Added\n- feat: User authentication system\n",
///   "entries": [...]
/// }
/// ```
async fn get_changelog(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ChangelogQuery>,
) -> impl IntoResponse {
    // D2: Support source=tasks to generate from task history
    if query.source.as_deref() == Some("tasks") {
        let tasks = state.tasks.read().await;
        let completed_tasks: Vec<_> = tasks
            .values()
            .filter(|t| t.phase == at_core::types::TaskPhase::Complete)
            .collect();

        if completed_tasks.is_empty() {
            return (
                axum::http::StatusCode::OK,
                Json(
                    serde_json::json!({"markdown": "# Changelog\n\nNo completed tasks found.\n", "entries": []}),
                ),
            );
        }

        // Generate changelog entries from completed tasks
        let mut engine = state.changelog_engine.write().await;
        let mut commits = String::new();
        for task in &completed_tasks {
            let category = match task.category {
                at_core::types::TaskCategory::Feature => "feat",
                at_core::types::TaskCategory::BugFix => "fix",
                at_core::types::TaskCategory::Refactoring => "refactor",
                at_core::types::TaskCategory::Documentation => "docs",
                at_core::types::TaskCategory::Security => "security",
                at_core::types::TaskCategory::Performance => "perf",
                at_core::types::TaskCategory::Infrastructure => "infra",
                at_core::types::TaskCategory::Testing => "test",
                at_core::types::TaskCategory::UiUx => "ui",
            };
            commits.push_str(&format!("{}: {}\n", category, task.title));
        }
        let version = format!(
            "{}.{}.{}",
            chrono::Utc::now().year(),
            chrono::Utc::now().month(),
            chrono::Utc::now().day()
        );
        let entry = engine.generate_from_commits(&commits, &version);
        let markdown = engine.generate_markdown();
        drop(engine);

        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "markdown": markdown,
                "entries": vec![entry]
            })),
        )
    } else {
        // Default: list existing entries
        let engine = state.changelog_engine.read().await;
        let entries = engine.list_entries().to_vec();
        (axum::http::StatusCode::OK, Json(serde_json::json!(entries)))
    }
}

/// POST /api/changelog/generate -- generate a changelog entry from commit messages.
///
/// Parses conventional commit messages (feat:, fix:, etc.) and generates
/// a structured changelog entry grouped into Added, Changed, Fixed, and
/// Security sections.
///
/// **Request:** JSON body with commit messages and version string.
///
/// **Response:** 201 Created with the generated changelog entry.
///
/// **Example Request:**
/// ```json
/// {
///   "commits": "feat: add user authentication\nfix: resolve null pointer in parser\nsecurity: patch XSS vulnerability",
///   "version": "1.2.0"
/// }
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "version": "1.2.0",
///   "date": "2026-02-27",
///   "sections": [
///     {
///       "category": "Added",
///       "items": ["add user authentication"]
///     },
///     {
///       "category": "Fixed",
///       "items": ["resolve null pointer in parser"]
///     },
///     {
///       "category": "Security",
///       "items": ["patch XSS vulnerability"]
///     }
///   ]
/// }
/// ```
async fn generate_changelog(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<GenerateChangelogRequest>,
) -> impl IntoResponse {
    let mut engine = state.changelog_engine.write().await;
    let entry = engine.generate_from_commits(&req.commits, &req.version);
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(entry)),
    )
}

/// Query parameters for project context retrieval.
///
/// **Example:**
/// ```
/// GET /api/context?path=/path/to/project&budget=4000
/// ```
#[derive(Debug, Deserialize)]
pub struct ContextQuery {
    pub path: Option<String>,
    #[serde(default = "default_budget")]
    pub budget: usize,
}

fn default_budget() -> usize {
    4000
}

/// GET /api/context -- retrieve project context information.
///
/// Exposes the ProjectContextLoader for the UI to show project index,
/// agent definitions, skill definitions, and cached context files like
/// CLAUDE.md, AGENTS.md, and TODO.md.
///
/// **Query Parameters:**
/// - `path`: Optional project root path. Defaults to current directory.
/// - `budget`: Token budget for response size (default 4000). Values > 2000
///   include full agent and skill definition lists.
///
/// **Response:** 200 OK with project context summary.
///
/// **Example Request:**
/// ```
/// GET /api/context?path=/workspace/myproject&budget=4000
/// ```
///
/// **Example Response:**
/// ```json
/// {
///   "claude_md": "Project overview and guidelines...",
///   "agents_md": "Agent definitions...",
///   "todo_md": "- [ ] Task 1\n- [ ] Task 2",
///   "agent_definitions_count": 5,
///   "skill_definitions_count": 12,
///   "cache": {
///     "hits": 42,
///     "misses": 3,
///     "rebuilds": 1
///   },
///   "budget": 4000,
///   "agent_definitions": ["coder", "reviewer", "qa"],
///   "skill_definitions": ["git", "npm", "cargo"]
/// }
/// ```
async fn get_context(Query(query): Query<ContextQuery>) -> impl IntoResponse {
    let project_root = query
        .path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));

    let loader = ProjectContextLoader::new(project_root);
    let snapshot = loader.load_snapshot_cached();
    let cache_stats = ProjectContextLoader::context_cache_stats();

    // Load project context files
    let mut context_summary = serde_json::json!({
        "claude_md": snapshot.claude_md,
        "agents_md": snapshot.agents_md,
        "todo_md": snapshot.todo_md,
        "agent_definitions_count": snapshot.agent_definitions.len(),
        "skill_definitions_count": snapshot.skill_definitions.len(),
        "cache": {
            "hits": cache_stats.hits,
            "misses": cache_stats.misses,
            "rebuilds": cache_stats.rebuilds,
        },
        "budget": query.budget,
    });

    // If budget allows, include more details
    if query.budget > 2000 {
        context_summary["agent_definitions"] = serde_json::json!(snapshot
            .agent_definitions
            .iter()
            .map(|a| &a.name)
            .collect::<Vec<_>>());
        context_summary["skill_definitions"] = serde_json::json!(snapshot
            .skill_definitions
            .iter()
            .map(|s| &s.name)
            .collect::<Vec<_>>());
    }

    Json(context_summary)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use at_intelligence::{changelog::ChangelogEngine, roadmap::RoadmapEngine};

    #[test]
    fn test_roadmap_generate_from_codebase() {
        let mut engine = RoadmapEngine::new();
        let analysis = "\
- Feature: Auth system | Description: OAuth2 integration | Priority: 1
- Feature: Dashboard | Description: Admin dashboard | Priority: 2
- Feature: Search | Description: Full-text search | Priority: 3
not a feature line
- Feature: No desc line | Priority: 4
";
        let roadmap = engine.generate_from_codebase(analysis);

        assert_eq!(roadmap.name, "Generated Roadmap");
        assert_eq!(roadmap.features.len(), 4);

        assert_eq!(roadmap.features[0].title, "Auth system");
        assert_eq!(roadmap.features[0].description, "OAuth2 integration");
        assert_eq!(roadmap.features[0].priority, 1);

        assert_eq!(roadmap.features[1].title, "Dashboard");
        assert_eq!(roadmap.features[1].priority, 2);

        assert_eq!(roadmap.features[3].title, "No desc line");
        assert_eq!(roadmap.features[3].description, "");
        assert_eq!(roadmap.features[3].priority, 4);
    }

    #[test]
    fn test_roadmap_generate_empty_input() {
        let mut engine = RoadmapEngine::new();
        let roadmap = engine.generate_from_codebase("");
        assert!(roadmap.features.is_empty());
    }

    #[test]
    fn test_changelog_generate_from_commits() {
        let mut engine = ChangelogEngine::new();
        let commits = "\
feat: add user authentication
feat(api): new /health endpoint
fix: resolve null pointer in parser
fix(ui): button alignment issue
perf: optimize database queries
refactor: extract helper module
docs: update README
security: patch XSS vulnerability
some random commit message
";
        let entry = engine.generate_from_commits(commits, "1.2.0");

        assert_eq!(entry.version, "1.2.0");

        // Collect categories
        let categories: Vec<_> = entry.sections.iter().map(|s| &s.category).collect();

        // Should have Added, Changed, Fixed, Security sections
        use at_intelligence::changelog::ChangeCategory;
        assert!(categories.contains(&&ChangeCategory::Added));
        assert!(categories.contains(&&ChangeCategory::Changed));
        assert!(categories.contains(&&ChangeCategory::Fixed));
        assert!(categories.contains(&&ChangeCategory::Security));

        // Check Added items (feat + fallback)
        let added = entry
            .sections
            .iter()
            .find(|s| s.category == ChangeCategory::Added)
            .unwrap();
        assert_eq!(added.items.len(), 3); // 2 feat + 1 fallback

        // Check Fixed items
        let fixed = entry
            .sections
            .iter()
            .find(|s| s.category == ChangeCategory::Fixed)
            .unwrap();
        assert_eq!(fixed.items.len(), 2);

        // Check Changed items (perf + refactor + docs = 3)
        let changed = entry
            .sections
            .iter()
            .find(|s| s.category == ChangeCategory::Changed)
            .unwrap();
        assert_eq!(changed.items.len(), 3);

        // Check Security items
        let security = entry
            .sections
            .iter()
            .find(|s| s.category == ChangeCategory::Security)
            .unwrap();
        assert_eq!(security.items.len(), 1);

        // The entry should also be stored in the engine
        assert_eq!(engine.list_entries().len(), 1);
    }

    #[test]
    fn test_changelog_generate_empty_commits() {
        let mut engine = ChangelogEngine::new();
        let entry = engine.generate_from_commits("", "0.0.1");
        assert_eq!(entry.version, "0.0.1");
        assert!(entry.sections.is_empty());
    }

    #[test]
    fn test_api_state_has_intelligence_engines() {
        use crate::event_bus::EventBus;
        let event_bus = EventBus::new();
        let state = ApiState::new(event_bus);

        // Verify all intelligence engines are accessible and default-initialized.
        // We just need to confirm they are constructed without panic.
        let _insights = &state.insights_engine;
        let _ideation = &state.ideation_engine;
        let _roadmap = &state.roadmap_engine;
        let _memory = &state.memory_store;
        let _changelog = &state.changelog_engine;
    }
}
