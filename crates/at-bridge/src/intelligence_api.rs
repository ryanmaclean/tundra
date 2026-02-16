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
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use at_intelligence::{
    ideation::IdeaCategory,
    insights::ChatRole,
    memory::{MemoryCategory, MemoryEntry},
    roadmap::{FeatureStatus, RoadmapFeature},
};

use crate::http_api::ApiState;

// ---------------------------------------------------------------------------
// Request / query types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub title: String,
    pub model: String,
}

#[derive(Debug, Deserialize)]
pub struct AddMessageRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateIdeasRequest {
    pub category: IdeaCategory,
    pub context: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRoadmapRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateRoadmapRequest {
    pub analysis: String,
}

#[derive(Debug, Deserialize)]
pub struct AddFeatureRequest {
    pub title: String,
    pub description: String,
    pub priority: u8,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFeatureStatusRequest {
    pub status: FeatureStatus,
}

#[derive(Debug, Deserialize)]
pub struct AddMemoryRequest {
    pub key: String,
    pub value: String,
    pub category: MemoryCategory,
    pub source: String,
}

#[derive(Debug, Deserialize)]
pub struct MemorySearchQuery {
    pub q: String,
}

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
            post(add_message),
        )
        // Ideation
        .route("/api/ideation/ideas", get(list_ideas))
        .route("/api/ideation/generate", post(generate_ideas))
        .route("/api/ideation/ideas/{id}/convert", post(convert_idea))
        // Roadmap
        .route("/api/roadmap", get(list_roadmaps))
        .route("/api/roadmap", post(create_roadmap))
        .route("/api/roadmap/generate", post(generate_roadmap))
        .route("/api/roadmap/{id}/features", post(add_feature))
        .route(
            "/api/roadmap/{id}/features/{fid}",
            patch(update_feature_status),
        )
        // Memory
        .route("/api/memory", get(list_memory))
        .route("/api/memory", post(add_memory))
        .route("/api/memory/search", get(search_memory))
        .route("/api/memory/{id}", axum::routing::delete(delete_memory))
        // Changelog
        .route("/api/changelog", get(list_changelog))
        .route("/api/changelog/generate", post(generate_changelog))
}

// ---------------------------------------------------------------------------
// Insights handlers
// ---------------------------------------------------------------------------

async fn list_sessions(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let engine = state.insights_engine.read().await;
    let sessions = engine.list_sessions().to_vec();
    Json(serde_json::json!(sessions))
}

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

async fn list_ideas(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let engine = state.ideation_engine.read().await;
    let ideas = engine.list_ideas().to_vec();
    Json(serde_json::json!(ideas))
}

async fn generate_ideas(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<GenerateIdeasRequest>,
) -> impl IntoResponse {
    let mut engine = state.ideation_engine.write().await;
    let result = engine.generate_ideas(&req.category, &req.context);
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(result)),
    )
}

async fn convert_idea(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let engine = state.ideation_engine.read().await;
    match engine.convert_to_task(&id) {
        Some(bead) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!(bead)),
        ),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "idea not found"})),
        ),
    }
}

// ---------------------------------------------------------------------------
// Roadmap handlers
// ---------------------------------------------------------------------------

async fn list_roadmaps(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let engine = state.roadmap_engine.read().await;
    let roadmaps = engine.list_roadmaps().to_vec();
    Json(serde_json::json!(roadmaps))
}

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

async fn generate_roadmap(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<GenerateRoadmapRequest>,
) -> impl IntoResponse {
    let mut engine = state.roadmap_engine.write().await;
    let roadmap = engine.generate_from_codebase(&req.analysis).clone();
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(roadmap)),
    )
}

async fn add_feature(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<AddFeatureRequest>,
) -> impl IntoResponse {
    let mut engine = state.roadmap_engine.write().await;
    let feature = RoadmapFeature::new(&req.title, &req.description, req.priority);
    let feature_json = serde_json::json!(feature);
    match engine.add_feature(&id, feature) {
        Ok(()) => (
            axum::http::StatusCode::CREATED,
            Json(feature_json),
        ),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

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

// ---------------------------------------------------------------------------
// Memory handlers
// ---------------------------------------------------------------------------

async fn list_memory(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let store = state.memory_store.read().await;
    // search("") matches every entry because every string contains "".
    let entries: Vec<_> = store.search("").into_iter().cloned().collect();
    Json(serde_json::json!(entries))
}

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

async fn search_memory(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<MemorySearchQuery>,
) -> impl IntoResponse {
    let store = state.memory_store.read().await;
    let results: Vec<_> = store.search(&q.q).into_iter().cloned().collect();
    Json(serde_json::json!(results))
}

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

async fn list_changelog(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let engine = state.changelog_engine.read().await;
    let entries = engine.list_entries().to_vec();
    Json(serde_json::json!(entries))
}

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use at_intelligence::{
        changelog::ChangelogEngine,
        roadmap::RoadmapEngine,
    };

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
