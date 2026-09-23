use at_core::worktree::WorktreeInfo;
use at_core::worktree_manager::{GatedMerge, MergeResult, WorktreeManager};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::warn;

use super::state::ApiState;
use super::types::{ResolveConflictRequest, WorktreeQuery};

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
pub(crate) async fn list_worktrees(Query(params): Query<WorktreeQuery>) -> impl IntoResponse {
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

    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let paginated: Vec<WorktreeEntry> = worktrees.into_iter().skip(offset).take(limit).collect();

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(paginated)),
    )
}

/// A worktree from `git worktree list --porcelain`.
#[derive(Debug, Clone)]
struct ListedWorktree {
    path: String,
    branch: String,
}

/// Parse `git worktree list --porcelain`. The first entry is the main worktree.
fn parse_worktree_porcelain(stdout: &str) -> Vec<ListedWorktree> {
    let mut out = Vec::new();
    let mut path: Option<String> = None;
    let mut branch = String::new();
    for line in stdout.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(prev) = path.take() {
                out.push(ListedWorktree {
                    path: prev,
                    branch: std::mem::take(&mut branch),
                });
            }
            path = Some(p.to_string());
            branch.clear();
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            branch = b.to_string();
        }
    }
    if let Some(prev) = path {
        out.push(ListedWorktree { path: prev, branch });
    }
    out
}

/// POST /api/worktrees/{id}/merge -- run the merge gate, then merge the
/// worktree branch into `main` via [`WorktreeManager::merge_to_main_gated`].
///
/// **Response:** always carries `gate` (an `at.merge_gate.report/v1`
/// [`at_core::merge_gate::MergeGateReport`]) once the worktree is found.
/// - 200 `{"status": "success" | "nothing_to_merge" | "conflict", "branch", "files"?, "gate"}`
/// - 409 `{"status": "gate_failed", "branch", "error", "gate"}` -- nothing merged
/// - 404 unknown worktree, 400 main worktree, 500 git failure
pub(crate) async fn merge_worktree(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let cwd = std::env::current_dir().unwrap_or_default();
    merge_worktree_in(&state, &cwd, &id).await
}

/// [`merge_worktree`] with an explicit directory to discover the repo from.
pub(crate) async fn merge_worktree_in(
    state: &ApiState,
    discover_dir: &std::path::Path,
    id: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    let output = match tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(discover_dir)
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": String::from_utf8_lossy(&o.stderr).trim()})),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            );
        }
    };

    let listed = parse_worktree_porcelain(&String::from_utf8_lossy(&output.stdout));
    let Some(main_wt) = listed.first().cloned() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "git worktree list returned no worktrees"})),
        );
    };

    // Exact stable id first, then the legacy substring match (which never
    // selects the main worktree: its path contains most ids).
    let found = listed
        .iter()
        .find(|w| stable_worktree_id(&w.path, &w.branch) == id)
        .or_else(|| {
            listed
                .iter()
                .skip(1)
                .find(|w| w.branch.contains(id) || w.path.contains(id))
        })
        .cloned();
    let wt = match found {
        Some(w) if !w.branch.is_empty() => w,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "worktree not found", "id": id})),
            );
        }
    };
    if wt.path == main_wt.path {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "cannot merge the main worktree into itself",
                "id": id,
                "branch": wt.branch,
            })),
        );
    }

    // Acceptance criteria come from the task that owns this worktree.
    let (task_id, criteria) = {
        let tasks = state.tasks.read().await;
        tasks
            .values()
            .find(|t| {
                t.worktree_path.as_deref() == Some(wt.path.as_str())
                    || t.git_branch.as_deref() == Some(wt.branch.as_str())
            })
            .map(|t| (Some(t.id), t.acceptance_criteria.clone()))
            .unwrap_or((None, Vec::new()))
    };

    let info = WorktreeInfo {
        path: wt.path.clone(),
        branch: wt.branch.clone(),
        base_branch: "main".to_string(),
        task_name: std::path::Path::new(&wt.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        created_at: chrono::Utc::now(),
    };
    let gate_config = state.settings_manager.load_or_default().merge_gate;
    let manager = WorktreeManager::new(&main_wt.path).with_merge_gate_config(gate_config);

    let outcome = match manager.merge_to_main_gated(&info, &criteria).await {
        Ok(o) => o,
        Err(e) => {
            warn!(error = %e, branch = %wt.branch, "merge failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string(), "branch": wt.branch})),
            );
        }
    };

    if let Some(task_id) = task_id {
        let mut tasks = state.tasks.write().await;
        if let Some(t) = tasks.get_mut(&task_id) {
            t.merge_gate_report = Some(outcome.report().clone());
            t.updated_at = chrono::Utc::now();
        }
    }

    let (code, status, files, error) = match &outcome {
        GatedMerge::Refused { report } => (
            StatusCode::CONFLICT,
            "gate_failed",
            Vec::new(),
            Some(report.summary()),
        ),
        GatedMerge::Attempted { result, .. } => match result {
            MergeResult::Success => (StatusCode::OK, "success", Vec::new(), None),
            MergeResult::NothingToMerge => (StatusCode::OK, "nothing_to_merge", Vec::new(), None),
            MergeResult::Conflict(files) => (StatusCode::OK, "conflict", files.clone(), None),
        },
    };

    if status != "nothing_to_merge" {
        state
            .event_bus
            .publish(crate::protocol::BridgeMessage::MergeResult {
                worktree_id: id.to_string(),
                branch: wt.branch.clone(),
                status: status.to_string(),
                conflict_files: files.clone(),
            });
    }

    let mut body = json!({
        "status": status,
        "branch": wt.branch,
        "gate": outcome.report(),
    });
    if status == "conflict" {
        body["files"] = json!(files);
    }
    if let Some(error) = error {
        body["error"] = json!(error);
    }
    (code, Json(body))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::EventBus;
    use at_core::types::{Task, TaskCategory, TaskComplexity, TaskPriority};
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    fn sh_git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Temp repo on `main` plus a `task/<name>` worktree with one commit.
    struct Fixture {
        root: PathBuf,
        repo: PathBuf,
        wt: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn fixture(name: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!("at-bridge-merge-{}", Uuid::new_v4()));
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        sh_git(&repo, &["init", "-q", "-b", "main"]);
        sh_git(&repo, &["config", "user.name", "t"]);
        sh_git(&repo, &["config", "user.email", "t@example.com"]);
        sh_git(&repo, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("f"), "base\n").unwrap();
        sh_git(&repo, &["add", "f"]);
        sh_git(&repo, &["commit", "-q", "-m", "base"]);

        let wt = root.join("wt").join(name);
        let branch = format!("task/{name}");
        sh_git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &branch,
                wt.to_str().unwrap(),
                "main",
            ],
        );
        std::fs::write(wt.join("g"), "task\n").unwrap();
        sh_git(&wt, &["add", "g"]);
        sh_git(&wt, &["commit", "-q", "-m", "task"]);
        Fixture { root, repo, wt }
    }

    async fn state_with_task(fx: &Fixture, branch: &str, criteria: &[&str]) -> (ApiState, Uuid) {
        let mut state = ApiState::new(EventBus::new());
        // Keep the user's ~/.auto-tundra config out of the test.
        state.settings_manager = Arc::new(at_core::settings::SettingsManager::new(
            fx.root.join("config.toml"),
        ));
        let mut task = Task::new(
            "gate test",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        );
        task.git_branch = Some(branch.to_string());
        task.worktree_path = Some(fx.wt.to_string_lossy().to_string());
        task.acceptance_criteria = criteria.iter().map(|s| s.to_string()).collect();
        let id = task.id;
        state.tasks.write().await.insert(id, task);
        (state, id)
    }

    #[tokio::test]
    async fn merge_returns_409_with_gate_report_when_criteria_fail() {
        let fx = fixture("gated");
        let (state, task_id) = state_with_task(&fx, "task/gated", &["echo nope >&2; exit 7"]).await;
        let main_before = sh_git(&fx.repo, &["rev-parse", "main"]);

        let id = stable_worktree_id(&fx.wt.to_string_lossy(), "task/gated");
        let (code, Json(body)) = merge_worktree_in(&state, &fx.repo, &id).await;

        assert_eq!(code, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["status"], "gate_failed");
        assert_eq!(body["branch"], "task/gated");
        assert!(
            body["error"].as_str().unwrap().contains("exited 7"),
            "{body}"
        );
        let gate = &body["gate"];
        assert_eq!(gate["schema"], at_core::merge_gate::MERGE_GATE_SCHEMA);
        assert_eq!(gate["passed"], false);
        assert_eq!(gate["target"], "main");
        assert_eq!(gate["blocked_by"], json!([]));
        let r = &gate["results"][0];
        assert_eq!(r["cmd"], "echo nope >&2; exit 7");
        assert_eq!(r["exit_code"], 7);
        assert_eq!(r["timed_out"], false);
        assert!(r["duration_ms"].is_u64());
        assert_eq!(r["stdout_tail"], "");
        assert_eq!(r["stderr_tail"], "nope\n");

        // Not merged, and the report is recorded on the task.
        assert_eq!(sh_git(&fx.repo, &["rev-parse", "main"]), main_before);
        let tasks = state.tasks.read().await;
        let stored = tasks[&task_id].merge_gate_report.as_ref().unwrap();
        assert!(!stored.passed);
    }

    #[tokio::test]
    async fn merge_returns_200_with_gate_report_when_merged() {
        let fx = fixture("ok");
        let (state, _) = state_with_task(&fx, "task/ok", &["test -f g"]).await;

        let (code, Json(body)) = merge_worktree_in(&state, &fx.repo, "task/ok").await;

        assert_eq!(code, StatusCode::OK, "{body}");
        assert_eq!(body["status"], "success");
        assert_eq!(body["gate"]["passed"], true);
        assert_eq!(body["gate"]["results"][0]["exit_code"], 0);
        let files = sh_git(&fx.repo, &["ls-tree", "--name-only", "main"]);
        assert!(files.lines().any(|l| l == "g"), "{files}");
    }

    #[tokio::test]
    async fn merge_refuses_main_worktree() {
        let fx = fixture("m");
        let (state, _) = state_with_task(&fx, "task/m", &[]).await;
        let repo = fx.repo.canonicalize().unwrap();
        let id = stable_worktree_id(&repo.to_string_lossy(), "main");
        let (code, Json(body)) = merge_worktree_in(&state, &fx.repo, &id).await;
        assert_eq!(code, StatusCode::BAD_REQUEST, "{body}");
    }

    #[test]
    fn porcelain_parser_keeps_order_and_detached_entries() {
        let text = "worktree /r\nHEAD abc\nbranch refs/heads/main\n\nworktree /r/.w/a\nHEAD def\ndetached\n\nworktree /r/.w/b\nHEAD 123\nbranch refs/heads/task/b\n";
        let parsed = parse_worktree_porcelain(text);
        let got: Vec<_> = parsed
            .iter()
            .map(|w| (w.path.as_str(), w.branch.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![("/r", "main"), ("/r/.w/a", ""), ("/r/.w/b", "task/b")]
        );
    }
}
