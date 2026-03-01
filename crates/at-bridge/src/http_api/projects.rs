use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use super::state::ApiState;
use super::types::Project;
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

/// GET /api/projects -- retrieve all projects in the system.
pub(crate) async fn list_projects(State(state): State<Arc<ApiState>>) -> Json<Vec<Project>> {
    let projects = state.projects.read().await;
    Json(projects.clone())
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
    projects.push(project.clone());
    (axum::http::StatusCode::CREATED, Json(project))
}

/// PUT /api/projects/{id} -- update a project's name or path.
pub(crate) async fn update_project(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProjectRequest>,
) -> impl IntoResponse {
    let mut projects = state.projects.write().await;
    let Some(project) = projects.iter_mut().find(|p| p.id == id) else {
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
    let before = projects.len();
    projects.retain(|p| p.id != id);
    if projects.len() == before {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project not found"})),
        );
    }
    if !projects.iter().any(|p| p.is_active) {
        if let Some(first) = projects.first_mut() {
            first.is_active = true;
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
    let exists = projects.iter().any(|p| p.id == id);
    if !exists {
        return Err(ApiError::NotFound("project not found".into()));
    }
    for p in projects.iter_mut() {
        p.is_active = p.id == id;
    }
    let activated = projects.iter().find(|p| p.id == id).cloned()
        .ok_or_else(|| ApiError::NotFound("project not found".into()))?;
    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!(activated)),
    ))
}
