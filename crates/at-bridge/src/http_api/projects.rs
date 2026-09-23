use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use super::state::ApiState;
use super::types::{Project, ProjectQuery};
use crate::api_error::ApiError;

#[derive(Debug, Deserialize)]
pub(crate) struct CreateProjectRequest {
    name: String,
    path: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateProjectRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

/// Sort key giving creation order: parsed RFC 3339 timestamp, then id as a
/// tie-break. `projects`/`attachments` are HashMaps (random iteration order),
/// so every listing and fallback choice must go through this key.
pub(crate) fn creation_key(
    created_at: &str,
    id: Uuid,
) -> (Option<chrono::DateTime<chrono::FixedOffset>>, Uuid) {
    (chrono::DateTime::parse_from_rfc3339(created_at).ok(), id)
}

/// GET /api/projects -- retrieve all projects, oldest first.
pub(crate) async fn list_projects(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<ProjectQuery>,
) -> Json<Vec<Project>> {
    let projects = state.projects.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);
    let mut ordered: Vec<&Project> = projects.values().collect();
    ordered.sort_by_key(|p| creation_key(&p.created_at, p.id));
    Json(ordered.into_iter().skip(offset).take(limit).cloned().collect())
}

/// POST /api/projects -- create a new project.
pub(crate) async fn create_project(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateProjectRequest>,
) -> impl IntoResponse {
    let project = Project {
        id: Uuid::new_v4(),
        name: req.name,
        path: req.path,
        created_at: chrono::Utc::now().to_rfc3339(),
        is_active: false,
    };
    let mut projects = state.projects.write().await;
    projects.insert(project.id, project.clone());
    (axum::http::StatusCode::CREATED, Json(project))
}

/// PUT /api/projects/{id} -- update a project's name or path.
pub(crate) async fn update_project(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProjectRequest>,
) -> impl IntoResponse {
    let mut projects = state.projects.write().await;
    let Some(project) = projects.get_mut(&id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found"})),
        );
    };
    if let Some(name) = req.name {
        project.name = name;
    }
    if let Some(path) = req.path {
        project.path = path;
    }
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(project.clone())),
    )
}

/// DELETE /api/projects/{id} -- delete a project.
pub(crate) async fn delete_project(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut projects = state.projects.write().await;
    if projects.len() <= 1 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "cannot delete last project"})),
        );
    }
    if projects.remove(&id).is_none() {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found"})),
        );
    }
    if !projects.values().any(|p| p.is_active) {
        // Deterministic fallback: the oldest remaining project.
        if let Some(oldest) = projects
            .values_mut()
            .min_by_key(|p| creation_key(&p.created_at, p.id))
        {
            oldest.is_active = true;
        }
    }
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({"ok": true})),
    )
}

/// POST /api/projects/{id}/activate -- set a project as the active project.
pub(crate) async fn activate_project(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let mut projects = state.projects.write().await;
    if !projects.contains_key(&id) {
        return Err(ApiError::NotFound("project not found".into()));
    }
    for p in projects.values_mut() {
        p.is_active = p.id == id;
    }
    let activated = projects
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound("project not found".into()))?;
    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!(activated)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::EventBus;
    use crate::http_api::types::{AttachmentQuery, Attachment};

    fn ts(i: i64) -> String {
        (chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap()
            + chrono::Duration::seconds(i))
        .to_rfc3339()
    }

    /// Replace the seeded project with `n` projects created one second apart.
    async fn state_with_projects(n: i64) -> (Arc<ApiState>, Vec<Uuid>) {
        let state = Arc::new(ApiState::new(EventBus::new()));
        let mut ids = Vec::new();
        let mut projects = state.projects.write().await;
        projects.clear();
        for i in 0..n {
            let p = Project {
                id: Uuid::new_v4(),
                name: format!("p{i}"),
                path: format!("/p{i}"),
                created_at: ts(i),
                is_active: i == 0,
            };
            ids.push(p.id);
            projects.insert(p.id, p);
        }
        drop(projects);
        (state, ids)
    }

    #[tokio::test]
    async fn list_projects_pages_in_creation_order() {
        let (state, ids) = state_with_projects(40).await;
        let mut seen = Vec::new();
        for page in 0..4 {
            let Json(items) = list_projects(
                State(state.clone()),
                Query(ProjectQuery {
                    limit: Some(10),
                    offset: Some(page * 10),
                }),
            )
            .await;
            seen.extend(items.into_iter().map(|p| p.id));
        }
        assert_eq!(seen, ids);
    }

    #[tokio::test]
    async fn deleting_active_project_activates_the_oldest_remaining() {
        let (state, ids) = state_with_projects(30).await;
        let _ = delete_project(State(state.clone()), Path(ids[0])).await;
        let projects = state.projects.read().await;
        let active: Vec<Uuid> = projects
            .values()
            .filter(|p| p.is_active)
            .map(|p| p.id)
            .collect();
        assert_eq!(active, vec![ids[1]]);
    }

    #[tokio::test]
    async fn list_attachments_pages_in_upload_order() {
        let state = Arc::new(ApiState::new(EventBus::new()));
        let task_id = Uuid::new_v4();
        let mut ids = Vec::new();
        {
            let mut atts = state.attachments.write().await;
            for i in 0..40 {
                let a = Attachment {
                    id: Uuid::new_v4(),
                    task_id,
                    filename: format!("f{i}"),
                    content_type: "text/plain".into(),
                    size_bytes: 1,
                    uploaded_at: ts(i),
                };
                ids.push(a.id);
                atts.insert(a.id, a);
            }
        }
        let mut seen = Vec::new();
        for page in 0..4 {
            let Json(items) = crate::http_api::misc::list_attachments(
                State(state.clone()),
                Path(task_id),
                Query(AttachmentQuery {
                    limit: Some(10),
                    offset: Some(page * 10),
                }),
            )
            .await;
            seen.extend(items.into_iter().map(|a| a.id));
        }
        assert_eq!(seen, ids);
    }
}
