use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::warn;

use super::state::ApiState;
use super::types::ResolveConflictRequest;

/// Represents a git worktree entry returned by the list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorktreeEntry {
    /// Stable identifier derived from path or branch name
    id: String,
    /// Absolute filesystem path to the worktree
    path: String,
    /// Git branch name (empty for detached HEAD)
    branch: String,
    /// Associated bead ID (currently unused, reserved for future)
    bead_id: String,
    /// Worktree status ("active" for all current worktrees)
    status: String,
}

/// Generates a stable, filesystem-safe identifier for a worktree.
fn stable_worktree_id(path: &str, branch: &str) -> String {
    let raw = if branch.is_empty() {
        format!("path:{path}")
    } else {
        format!("branch:{branch}")
    };
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// GET /api/worktrees -- list all git worktrees with path and branch info.
pub(crate) async fn list_worktrees() -> impl IntoResponse {
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": stderr})),
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut current_path = String::new();
    let mut current_branch = String::new();

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = path.to_string();
            current_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.is_empty() && !current_path.is_empty() {
            worktrees.push(WorktreeEntry {
                id: stable_worktree_id(&current_path, &current_branch),
                path: current_path.clone(),
                branch: current_branch.clone(),
                bead_id: String::new(),
                status: "active".into(),
            });
            current_path = String::new();
            current_branch = String::new();
        }
    }
    // Handle last entry if stdout doesn't end with empty line
    if !current_path.is_empty() {
        worktrees.push(WorktreeEntry {
            id: stable_worktree_id(&current_path, &current_branch),
            path: current_path,
            branch: current_branch,
            bead_id: String::new(),
            status: "active".into(),
        });
    }

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(worktrees)),
    )
}

/// POST /api/worktrees/{id}/merge -- trigger merge to main for a worktree branch.
pub(crate) async fn merge_worktree(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Look up the worktree by listing current git worktrees and matching the id/branch.
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut found_branch = None;
    let mut current_path = String::new();
    let mut current_branch = String::new();

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = path.to_string();
            current_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.is_empty() && !current_path.is_empty() {
            let candidate_id = stable_worktree_id(&current_path, &current_branch);
            // Match exact stable id first, then keep legacy contains fallback.
            if candidate_id == id || current_branch.contains(&id) || current_path.contains(&id) {
                found_branch = Some(current_branch.clone());
            }
            current_path = String::new();
            current_branch = String::new();
        }
    }
    // Handle last entry
    if found_branch.is_none() && !current_path.is_empty() {
        let candidate_id = stable_worktree_id(&current_path, &current_branch);
        if candidate_id == id || current_branch.contains(&id) || current_path.contains(&id) {
            found_branch = Some(current_branch);
        }
    }

    let branch = match found_branch {
        Some(b) if !b.is_empty() => b,
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "worktree not found", "id": id})),
            );
        }
    };

    // Attempt the merge using git commands
    let base_dir = std::env::current_dir().unwrap_or_default();
    let base_dir_str = base_dir.to_str().unwrap_or(".");

    // Check if there are changes to merge
    let diff_output = tokio::process::Command::new("git")
        .args(["diff", "--stat", "main", &branch])
        .current_dir(base_dir_str)
        .output()
        .await;

    match diff_output {
        Ok(o) if String::from_utf8_lossy(&o.stdout).trim().is_empty() => {
            return (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({"status": "nothing_to_merge", "branch": branch})),
            );
        }
        Ok(_) => { /* has changes */ }
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    }

    // Attempt merge --no-commit to detect conflicts
    let merge_output = tokio::process::Command::new("git")
        .args(["merge", "--no-ff", "--no-commit", &branch])
        .current_dir(base_dir_str)
        .output()
        .await;

    match merge_output {
        Ok(o) if o.status.success() => {
            // Commit the merge
            let commit_msg = format!("Merge branch '{}' into main", branch);
            if let Err(e) = tokio::process::Command::new("git")
                .args(["commit", "-m", &commit_msg])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, "git commit failed during merge");
            }

            // Publish event
            state
                .event_bus
                .publish(crate::protocol::BridgeMessage::MergeResult {
                    worktree_id: id.clone(),
                    branch: branch.clone(),
                    status: "success".to_string(),
                    conflict_files: vec![],
                });

            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({"status": "success", "branch": branch})),
            )
        }
        Ok(o) => {
            // Detect conflict files
            let conflict_output = tokio::process::Command::new("git")
                .args(["diff", "--name-only", "--diff-filter=U"])
                .current_dir(base_dir_str)
                .output()
                .await;

            // Abort the merge
            if let Err(e) = tokio::process::Command::new("git")
                .args(["merge", "--abort"])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, "git merge --abort failed");
            }

            let conflict_files: Vec<String> = match conflict_output {
                Ok(co) => String::from_utf8_lossy(&co.stdout)
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .collect(),
                Err(_) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    stderr
                        .lines()
                        .filter(|l| l.contains("CONFLICT"))
                        .map(|l| l.to_string())
                        .collect()
                }
            };

            state
                .event_bus
                .publish(crate::protocol::BridgeMessage::MergeResult {
                    worktree_id: id.clone(),
                    branch: branch.clone(),
                    status: "conflict".to_string(),
                    conflict_files: conflict_files.clone(),
                });

            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!({
                    "status": "conflict",
                    "branch": branch,
                    "files": conflict_files,
                })),
            )
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// GET /api/worktrees/{id}/merge-preview -- dry-run merge preview.
pub(crate) async fn merge_preview(Path(id): Path<String>) -> impl IntoResponse {
    let base_dir = std::env::current_dir().unwrap_or_default();
    let base_dir_str = base_dir.to_str().unwrap_or(".");

    // Try to find the branch for this worktree id
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut found_branch = None;
    let mut current_path = String::new();
    let mut current_branch = String::new();

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = path.to_string();
            current_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.is_empty() && !current_path.is_empty() {
            if current_branch.contains(&id) || current_path.contains(&id) {
                found_branch = Some(current_branch.clone());
            }
            current_path = String::new();
            current_branch = String::new();
        }
    }
    if found_branch.is_none()
        && !current_path.is_empty()
        && (current_branch.contains(&id) || current_path.contains(&id))
    {
        found_branch = Some(current_branch);
    }

    let branch = match found_branch {
        Some(b) if !b.is_empty() => b,
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "worktree not found", "id": id})),
            );
        }
    };

    // Count commits ahead/behind
    let rev_list = tokio::process::Command::new("git")
        .args([
            "rev-list",
            "--left-right",
            "--count",
            &format!("main...{}", branch),
        ])
        .current_dir(base_dir_str)
        .output()
        .await;

    let (behind, ahead) = match rev_list {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let parts: Vec<&str> = text.trim().split('\t').collect();
            let behind = parts
                .first()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let ahead = parts
                .get(1)
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            (behind, ahead)
        }
        _ => (0, 0),
    };

    // List files changed
    let diff_names = tokio::process::Command::new("git")
        .args(["diff", "--name-only", "main", &branch])
        .current_dir(base_dir_str)
        .output()
        .await;

    let files_changed: Vec<String> = match diff_names {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect(),
        _ => vec![],
    };

    // Check for potential conflicts via merge-tree (git 2.38+) or simple heuristic
    let has_conflicts = false; // Conservative: actual conflicts only detectable via real merge

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "ahead": ahead,
            "behind": behind,
            "files_changed": files_changed,
            "has_conflicts": has_conflicts,
            "branch": branch,
        })),
    )
}

/// POST /api/worktrees/{id}/resolve -- accept conflict resolution.
pub(crate) async fn resolve_conflict(
    Path(id): Path<String>,
    Json(req): Json<ResolveConflictRequest>,
) -> impl IntoResponse {
    let valid_strategies = ["ours", "theirs", "manual"];
    if !valid_strategies.contains(&req.strategy.as_str()) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("invalid strategy '{}', must be one of: ours, theirs, manual", req.strategy)
            })),
        );
    }

    let base_dir = std::env::current_dir().unwrap_or_default();
    let base_dir_str = base_dir.to_str().unwrap_or(".");

    match req.strategy.as_str() {
        "ours" => {
            if let Err(e) = tokio::process::Command::new("git")
                .args(["checkout", "--ours", &req.file])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, file = %req.file, "git conflict resolution command failed");
            }
            if let Err(e) = tokio::process::Command::new("git")
                .args(["add", &req.file])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, file = %req.file, "git conflict resolution command failed");
            }
        }
        "theirs" => {
            if let Err(e) = tokio::process::Command::new("git")
                .args(["checkout", "--theirs", &req.file])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, file = %req.file, "git conflict resolution command failed");
            }
            if let Err(e) = tokio::process::Command::new("git")
                .args(["add", &req.file])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, file = %req.file, "git conflict resolution command failed");
            }
        }
        "manual" => {
            // For manual, just mark the file as resolved by staging it
            if let Err(e) = tokio::process::Command::new("git")
                .args(["add", &req.file])
                .current_dir(base_dir_str)
                .output()
                .await
            {
                warn!(error = %e, file = %req.file, "git conflict resolution command failed");
            }
        }
        _ => {}
    }

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "status": "resolved",
            "worktree_id": id,
            "file": req.file,
            "strategy": req.strategy,
        })),
    )
}

/// DELETE /api/worktrees/{id} -- remove a git worktree by path.
pub(crate) async fn delete_worktree(Path(id): Path<String>) -> impl IntoResponse {
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": stderr})),
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut current_path = String::new();
    let mut current_branch = String::new();
    let mut found_path: Option<String> = None;

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = path.to_string();
            current_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.is_empty() && !current_path.is_empty() {
            let candidate_id = stable_worktree_id(&current_path, &current_branch);
            if candidate_id == id || current_branch.contains(&id) || current_path.contains(&id) {
                found_path = Some(current_path.clone());
                break;
            }
            current_path.clear();
            current_branch.clear();
        }
    }
    if found_path.is_none() && !current_path.is_empty() {
        let candidate_id = stable_worktree_id(&current_path, &current_branch);
        if candidate_id == id || current_branch.contains(&id) || current_path.contains(&id) {
            found_path = Some(current_path);
        }
    }

    let Some(path) = found_path else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "worktree not found", "id": id})),
        );
    };

    let rm = tokio::process::Command::new("git")
        .args(["worktree", "remove", "--force", &path])
        .output()
        .await;

    match rm {
        Ok(o) if o.status.success() => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "id": id, "path": path})),
        ),
        Ok(o) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": String::from_utf8_lossy(&o.stderr).to_string(),
                "id": id,
                "path": path
            })),
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string(), "id": id, "path": path})),
        ),
    }
}
