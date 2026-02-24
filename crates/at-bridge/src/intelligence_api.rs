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
pub struct AddFeatureToLatestRequest {
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: String,
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

async fn convert_idea(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let engine = state.ideation_engine.read().await;
    match engine.convert_to_task(&id) {
        Some(bead) => (axum::http::StatusCode::OK, Json(serde_json::json!(bead))),
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

/// POST /api/roadmap/features — add a feature to the most recently created roadmap.
/// Accepts a simpler request shape used by the frontend (title, description, status, priority as strings).
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

/// Update a feature's status by feature ID alone (searches across all roadmaps).
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

#[derive(Debug, Deserialize)]
pub struct ChangelogQuery {
    #[serde(default)]
    pub source: Option<String>,
}

async fn get_changelog(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ChangelogQuery>,
) -> impl IntoResponse {
    // D2: Support source=tasks to generate from task history
    if query.source.as_deref() == Some("tasks") {
        let tasks = state.tasks.read().await;
        let completed_tasks: Vec<_> = tasks
            .iter()
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

#[derive(Debug, Deserialize)]
pub struct ContextQuery {
    pub path: Option<String>,
    #[serde(default = "default_budget")]
    pub budget: usize,
}

fn default_budget() -> usize {
    4000
}

/// D3: GET /api/context?path=&budget= — expose ProjectContextLoader for UI to show project index.
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
        context_summary["agent_definitions"] =
            serde_json::json!(snapshot.agent_definitions.iter().map(|a| &a.name).collect::<Vec<_>>());
        context_summary["skill_definitions"] =
            serde_json::json!(snapshot.skill_definitions.iter().map(|s| &s.name).collect::<Vec<_>>());
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
