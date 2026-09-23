//! Merge-gate flow shared by the execute pipeline and the merge endpoints.
//!
//! - [`run_merge_phase`]: the pipeline's Merging phase. Runs the gate on the
//!   task worktree (verify-only by default, merging only with
//!   `merge_mode: "auto"`), loops Merging -> Fixing -> Qa -> Merging up to
//!   `[merge_gate] max_fix_iterations` times on refusal, and always ends the
//!   task in `Complete` or `Error`.
//! - [`record_gate_outcome`]: stores the report on the task, writes build-log
//!   lines and publishes the task update, `MergeResult` and the stable event.
//! - `POST /api/tasks/{id}/merge` ([`merge_task`]) and
//!   `GET /api/tasks/{id}/merge-gate` ([`get_task_merge_gate`]).
//!
//! Every gate run and merge holds [`ApiState::merge_lock`], so two gates never
//! race on the main checkout's `index.lock` or run `sh -c` in the same
//! worktree at once. The tasks lock is never held across a gate run: the task
//! is snapshotted, the lock dropped, and results written back afterwards.
//!
//! Stable event types (`EventPayload.event_type`): `merge_gate_failed`
//! (message = report summary), `merge_success`, `merge_conflict`.

use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use at_api_types::merge_gate::{
    EVENT_MERGE_CONFLICT, EVENT_MERGE_GATE_FAILED, EVENT_MERGE_SUCCESS, MERGE_GATE_SCHEMA_ID,
};
use at_core::merge_gate::{MergeGateConfig, MergeGateReport};
use at_core::types::{BuildLogEntry, BuildStream, CliType, Task, TaskPhase};
use at_core::worktree::WorktreeInfo;
use at_core::worktree_manager::{GatedMerge, MergeResult, WorktreeManager, WorktreeManagerError};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::json;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::state::ApiState;
use super::types::{MergeMode, TaskMergeRequest};
use crate::event_bus::EventBus;
use crate::protocol::{BridgeMessage, EventPayload};

/// Shared task map.
pub(crate) type TaskStore = Arc<RwLock<HashMap<Uuid, Task>>>;

// ---------------------------------------------------------------------------
// Gate outcome
// ---------------------------------------------------------------------------

/// Result of one gate run.
#[derive(Debug, Clone)]
pub(crate) enum GateOutcome {
    /// The gate refused the branch; nothing was merged.
    Refused(MergeGateReport),
    /// The gate passed and, by request, nothing was merged (verify mode).
    Verified(MergeGateReport),
    /// The gate passed and the merge was attempted.
    Merged {
        report: MergeGateReport,
        result: MergeResult,
    },
}

impl From<GatedMerge> for GateOutcome {
    fn from(g: GatedMerge) -> Self {
        match g {
            GatedMerge::Refused { report } => GateOutcome::Refused(report),
            GatedMerge::Attempted { report, result } => GateOutcome::Merged { report, result },
        }
    }
}

impl GateOutcome {
    pub(crate) fn report(&self) -> &MergeGateReport {
        match self {
            GateOutcome::Refused(r) | GateOutcome::Verified(r) => r,
            GateOutcome::Merged { report, .. } => report,
        }
    }

    /// Wire status: `gate_failed`, `verified`, `success`, `nothing_to_merge`
    /// or `conflict`.
    pub(crate) fn status(&self) -> &'static str {
        match self {
            GateOutcome::Refused(_) => "gate_failed",
            GateOutcome::Verified(_) => "verified",
            GateOutcome::Merged { result, .. } => match result {
                MergeResult::Success => "success",
                MergeResult::NothingToMerge => "nothing_to_merge",
                MergeResult::Conflict(_) => "conflict",
            },
        }
    }

    fn conflict_files(&self) -> Vec<String> {
        match self {
            GateOutcome::Merged {
                result: MergeResult::Conflict(files),
                ..
            } => files.clone(),
            _ => Vec::new(),
        }
    }

    /// HTTP code and `ApiMergeResponse` body for the merge endpoints.
    pub(crate) fn response(&self) -> (StatusCode, Json<serde_json::Value>) {
        let report = self.report();
        let status = self.status();
        let mut body = json!({
            "status": status,
            "branch": report.branch,
            "gate": report,
        });
        let code = match self {
            GateOutcome::Refused(r) => {
                body["error"] = json!(r.summary());
                StatusCode::CONFLICT
            }
            _ => StatusCode::OK,
        };
        if status == "conflict" {
            body["files"] = json!(self.conflict_files());
        }
        (code, Json(body))
    }
}

/// Run the gate (and, with `merge`, the merge) for `info` against the main
/// checkout at `repo_root`, holding `merge_lock` for the whole run.
pub(crate) async fn run_gate(
    merge_lock: &tokio::sync::Mutex<()>,
    repo_root: &FsPath,
    gate_config: MergeGateConfig,
    info: &WorktreeInfo,
    criteria: &[String],
    merge: bool,
) -> Result<GateOutcome, WorktreeManagerError> {
    let _guard = merge_lock.lock().await;
    let manager = WorktreeManager::new(repo_root).with_merge_gate_config(gate_config);
    if merge {
        Ok(manager.merge_to_main_gated(info, criteria).await?.into())
    } else {
        let report = manager.verify_gate(info, criteria).await;
        Ok(if report.passed {
            GateOutcome::Verified(report)
        } else {
            GateOutcome::Refused(report)
        })
    }
}

/// Record a gate outcome: store the report on the task (when one owns the
/// worktree), write build-log lines, and publish `TaskUpdate`, `MergeResult`
/// and the stable `merge_gate_failed` / `merge_success` / `merge_conflict`
/// event. Shared by the pipeline and both merge endpoints.
pub(crate) async fn record_gate_outcome(
    tasks: &TaskStore,
    bus: &EventBus,
    task_id: Option<Uuid>,
    worktree_id: &str,
    outcome: &GateOutcome,
) {
    let report = outcome.report();
    let summary = report.summary();
    let merged = matches!(
        outcome,
        GateOutcome::Merged {
            result: MergeResult::Success,
            ..
        }
    );

    let mut bead_id = None;
    if let Some(id) = task_id {
        let snapshot = {
            let mut tasks = tasks.write().await;
            tasks.get_mut(&id).map(|t| {
                t.merge_gate_report = Some(report.clone());
                let stream = if report.passed {
                    BuildStream::Stdout
                } else {
                    BuildStream::Stderr
                };
                t.add_build_log(stream, summary.clone());
                if let Some(failed) = report.results.iter().find(|r| !r.success()) {
                    t.add_build_log(
                        BuildStream::Stderr,
                        format!("failing criterion: {}", failed.cmd),
                    );
                    for line in failed.stderr_tail.lines() {
                        t.add_build_log(BuildStream::Stderr, line.to_string());
                    }
                }
                match outcome {
                    GateOutcome::Merged { result, .. } => match result {
                        MergeResult::Success => {
                            t.merged_at = Some(chrono::Utc::now());
                            t.add_build_log(
                                BuildStream::Stdout,
                                format!("merged {} into {}", report.branch, report.target),
                            );
                        }
                        MergeResult::NothingToMerge => {
                            t.add_build_log(BuildStream::Stdout, "nothing to merge".to_string());
                        }
                        MergeResult::Conflict(files) => {
                            t.add_build_log(
                                BuildStream::Stderr,
                                format!("merge conflicts in: {}", files.join(", ")),
                            );
                        }
                    },
                    GateOutcome::Verified(_) => t.add_build_log(
                        BuildStream::Stdout,
                        format!("verified only (merge_mode=verify); {} not merged", report.branch),
                    ),
                    GateOutcome::Refused(_) => {}
                }
                t.clone()
            })
        };
        if let Some(t) = snapshot {
            bead_id = Some(t.bead_id);
            bus.publish(BridgeMessage::TaskUpdate(Box::new(t)));
        }
    }

    let status = outcome.status();
    if matches!(status, "gate_failed" | "success" | "conflict") {
        bus.publish(BridgeMessage::MergeResult {
            worktree_id: worktree_id.to_string(),
            branch: report.branch.clone(),
            status: status.to_string(),
            conflict_files: outcome.conflict_files(),
        });
    }
    let event = match outcome {
        GateOutcome::Refused(_) => Some((EVENT_MERGE_GATE_FAILED, summary)),
        _ if merged => Some((
            EVENT_MERGE_SUCCESS,
            format!("merged {} into {}", report.branch, report.target),
        )),
        GateOutcome::Merged {
            result: MergeResult::Conflict(files),
            ..
        } => Some((
            EVENT_MERGE_CONFLICT,
            format!("merge conflicts in: {}", files.join(", ")),
        )),
        _ => None,
    };
    if let Some((event_type, message)) = event {
        bus.publish(BridgeMessage::Event(EventPayload {
            event_type: event_type.to_string(),
            agent_id: None,
            bead_id,
            message,
            timestamp: chrono::Utc::now(),
        }));
    }
}

/// `sha256` (lower-case hex) of the canonical JSON array of `criteria`. The
/// pipeline freezes this at execute time and re-checks it before every gate
/// run.
pub(crate) fn criteria_sha256(criteria: &[String]) -> String {
    at_harness::audit_chain::entry_hash("", &json!(criteria))
}

/// Main checkout of the repository `worktree` belongs to (the first entry of
/// `git worktree list --porcelain`).
pub(crate) async fn main_checkout_of(worktree: &str) -> Result<PathBuf, String> {
    let out = tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(worktree)
        .output()
        .await
        .map_err(|e| format!("git worktree list in {worktree}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git worktree list in {worktree}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    super::worktrees::parse_worktree_porcelain(&String::from_utf8_lossy(&out.stdout))
        .first()
        .map(|w| PathBuf::from(&w.path))
        .ok_or_else(|| format!("no worktrees listed for {worktree}"))
}

/// [`WorktreeInfo`] for a task's bound worktree.
fn worktree_info(path: &str, branch: &str) -> WorktreeInfo {
    WorktreeInfo {
        path: path.to_string(),
        branch: branch.to_string(),
        base_branch: "main".to_string(),
        task_name: FsPath::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        created_at: chrono::Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Pipeline context and helpers
// ---------------------------------------------------------------------------

/// Everything the background execute pipeline needs, snapshotted when
/// `POST /api/tasks/{id}/execute` accepted the task.
pub(crate) struct PipelineCtx {
    pub tasks: TaskStore,
    pub event_bus: EventBus,
    pub pty_pool: Option<Arc<at_session::pty_pool::PtyPool>>,
    pub cli_type: CliType,
    pub repo_root: Option<PathBuf>,
    pub merge_lock: Arc<tokio::sync::Mutex<()>>,
    pub gate_config: MergeGateConfig,
    pub merge_mode: MergeMode,
    /// Acceptance criteria frozen at execute time; the gate runs exactly
    /// these.
    pub criteria: Vec<String>,
    /// [`criteria_sha256`] of `criteria`.
    pub criteria_sha256: String,
}

impl PipelineCtx {
    /// Publish a pipeline event for the task's bead.
    pub(crate) fn emit(&self, bead_id: Uuid, event_type: &str, message: String) {
        self.event_bus.publish(BridgeMessage::Event(EventPayload {
            event_type: event_type.to_string(),
            agent_id: None,
            bead_id: Some(bead_id),
            message,
            timestamp: chrono::Utc::now(),
        }));
    }

    /// Append a build-log line to the task and publish `build_log_line`.
    pub(crate) async fn build_log(
        &self,
        task_id: Uuid,
        bead_id: Uuid,
        stream: BuildStream,
        line: String,
    ) {
        let label = match stream {
            BuildStream::Stdout => "stdout",
            BuildStream::Stderr => "stderr",
        };
        self.emit(bead_id, "build_log_line", format!("[{label}] {line}"));
        let mut tasks = self.tasks.write().await;
        if let Some(t) = tasks.get_mut(&task_id) {
            t.build_logs.push(BuildLogEntry {
                timestamp: chrono::Utc::now(),
                stream,
                line,
                phase: t.phase.clone(),
            });
            t.updated_at = chrono::Utc::now();
        }
    }

    /// Apply `f` to the task, publish `TaskUpdate`, return the new snapshot.
    pub(crate) async fn update(&self, task_id: Uuid, f: impl FnOnce(&mut Task)) -> Option<Task> {
        let snapshot = {
            let mut tasks = self.tasks.write().await;
            let t = tasks.get_mut(&task_id)?;
            f(t);
            t.updated_at = chrono::Utc::now();
            t.clone()
        };
        self.event_bus
            .publish(BridgeMessage::TaskUpdate(Box::new(snapshot.clone())));
        Some(snapshot)
    }

    pub(crate) async fn set_phase(&self, task_id: Uuid, phase: TaskPhase) -> Option<Task> {
        self.update(task_id, |t| t.set_phase(phase)).await
    }

    pub(crate) async fn snapshot(&self, task_id: Uuid) -> Option<Task> {
        self.tasks.read().await.get(&task_id).cloned()
    }

    /// End the task in `Error` with `message` (never a silent skip).
    pub(crate) async fn fail(&self, task_id: Uuid, bead_id: Uuid, message: String) {
        tracing::warn!(task_id = %task_id, error = %message, "pipeline failed");
        self.build_log(task_id, bead_id, BuildStream::Stderr, message.clone())
            .await;
        self.update(task_id, |t| {
            t.set_phase(TaskPhase::Error);
            t.error = Some(message.clone());
        })
        .await;
        self.emit(
            bead_id,
            "pipeline_complete_with_failures",
            format!("Task {task_id}: {message}"),
        );
    }

    /// End the task in `Complete`.
    pub(crate) async fn complete(&self, task_id: Uuid, bead_id: Uuid) {
        self.update(task_id, |t| {
            t.set_phase(TaskPhase::Complete);
            t.completed_at = Some(chrono::Utc::now());
            t.error = None;
        })
        .await;
        self.build_log(
            task_id,
            bead_id,
            BuildStream::Stdout,
            "Pipeline completed successfully".to_string(),
        )
        .await;
        self.emit(bead_id, "pipeline_complete", format!("Task {task_id}: pipeline_complete"));
    }
}

/// Run QA once in the task's worktree, store the report, and log the result.
/// Returns `true` unless QA reported `Failed` (the placeholder runner reports
/// `Pending` when it only found minor issues, which does not block).
pub(crate) async fn run_qa(ctx: &PipelineCtx, task_id: Uuid, label: &str) -> bool {
    use at_intelligence::runner::QaRunner;
    let Some(task) = ctx.snapshot(task_id).await else {
        return false;
    };
    let worktree = task.worktree_path.clone().unwrap_or_else(|| ".".to_string());
    let report = QaRunner::new().run_qa_checks(task.id, &task.title, Some(&worktree));
    let failed = report.status == at_core::types::QaStatus::Failed;
    let stream = if failed {
        BuildStream::Stderr
    } else {
        BuildStream::Stdout
    };
    ctx.build_log(
        task_id,
        task.bead_id,
        stream,
        format!(
            "{label}: {:?} ({} issues)",
            report.status,
            report.issues.len()
        ),
    )
    .await;
    ctx.update(task_id, |t| t.qa_report = Some(report)).await;
    !failed
}

/// The pipeline's Merging phase; always leaves the task in `Complete` or
/// `Error`.
///
/// - No worktree bound: `Complete` ("merge skipped: no worktree") when the
///   task has no criteria, otherwise `Error` -- criteria are never skipped.
/// - Criteria changed since execute (sha256 mismatch): `Error`.
/// - Gate passes: `Complete` (merged when `merge_mode` is `auto`).
/// - Merge conflict or git failure: `Error`.
/// - Gate refused: Merging -> Fixing (fix prompt logged) -> Qa -> Merging and
///   retry, up to `max_fix_iterations` times; then `Error` with
///   `task.error` = the report summary.
pub(crate) async fn run_merge_phase(ctx: &PipelineCtx, task_id: Uuid) {
    let Some(task) = ctx.set_phase(task_id, TaskPhase::Merging).await else {
        return;
    };
    let bead_id = task.bead_id;
    ctx.emit(bead_id, "merge_phase_start", format!("Task '{}': merge_phase_start", task.title));

    if criteria_sha256(&task.acceptance_criteria) != ctx.criteria_sha256 {
        ctx.fail(
            task_id,
            bead_id,
            "acceptance criteria changed after execute (sha256 mismatch)".to_string(),
        )
        .await;
        return;
    }

    let (Some(path), Some(branch)) = (task.worktree_path.clone(), task.git_branch.clone()) else {
        if ctx.criteria.is_empty() {
            ctx.build_log(
                task_id,
                bead_id,
                BuildStream::Stdout,
                "merge skipped: no worktree".to_string(),
            )
            .await;
            ctx.complete(task_id, bead_id).await;
        } else {
            ctx.fail(
                task_id,
                bead_id,
                "acceptance criteria set but no worktree bound (daemon repo_root unset)"
                    .to_string(),
            )
            .await;
        }
        return;
    };

    let repo_root = match &ctx.repo_root {
        Some(root) => root.clone(),
        None => match main_checkout_of(&path).await {
            Ok(root) => root,
            Err(e) => {
                ctx.fail(task_id, bead_id, format!("cannot locate main checkout: {e}"))
                    .await;
                return;
            }
        },
    };
    let info = worktree_info(&path, &branch);
    let worktree_id = super::worktrees::stable_worktree_id(&path, &branch);
    let merge = ctx.merge_mode == MergeMode::Auto;
    let max_fix = ctx.gate_config.max_fix_iterations;

    let mut iterations = 0usize;
    loop {
        ctx.build_log(
            task_id,
            bead_id,
            BuildStream::Stdout,
            format!(
                "merge gate: {} criteria, merge_mode={}, criteria_sha256={}",
                ctx.criteria.len(),
                ctx.merge_mode.as_str(),
                ctx.criteria_sha256
            ),
        )
        .await;
        let outcome = match run_gate(
            &ctx.merge_lock,
            &repo_root,
            ctx.gate_config.clone(),
            &info,
            &ctx.criteria,
            merge,
        )
        .await
        {
            Ok(o) => o,
            Err(e) => {
                ctx.fail(task_id, bead_id, format!("merge failed: {e}")).await;
                return;
            }
        };
        record_gate_outcome(&ctx.tasks, &ctx.event_bus, Some(task_id), &worktree_id, &outcome)
            .await;

        match outcome {
            GateOutcome::Verified(_)
            | GateOutcome::Merged {
                result: MergeResult::Success | MergeResult::NothingToMerge,
                ..
            } => {
                ctx.complete(task_id, bead_id).await;
                return;
            }
            GateOutcome::Merged {
                result: MergeResult::Conflict(files),
                ..
            } => {
                ctx.fail(
                    task_id,
                    bead_id,
                    format!("merge conflicts in: {}", files.join(", ")),
                )
                .await;
                return;
            }
            GateOutcome::Refused(report) => {
                if iterations >= max_fix {
                    ctx.fail(task_id, bead_id, report.summary()).await;
                    return;
                }
                iterations += 1;
                ctx.set_phase(task_id, TaskPhase::Fixing).await;
                ctx.emit(
                    bead_id,
                    &format!("merge_gate_fix_iteration_{iterations}"),
                    format!("Task {task_id}: merge gate fix iteration {iterations} of {max_fix}"),
                );
                ctx.build_log(
                    task_id,
                    bead_id,
                    BuildStream::Stderr,
                    format!("merge gate fix iteration {iterations} of {max_fix}"),
                )
                .await;
                // The coding step of this pipeline is delegated to the agent
                // executor; the prompt it gets is logged here verbatim.
                for line in report.fix_prompt().lines() {
                    ctx.build_log(
                        task_id,
                        bead_id,
                        BuildStream::Stderr,
                        format!("fix prompt: {line}"),
                    )
                    .await;
                }
                ctx.set_phase(task_id, TaskPhase::Qa).await;
                run_qa(ctx, task_id, "QA re-check result").await;
                ctx.set_phase(task_id, TaskPhase::Merging).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

/// `state` of `GET /api/tasks/{id}/merge-gate`.
fn gate_state(task: &Task) -> &'static str {
    match (&task.merge_gate_report, task.merged_at) {
        (None, _) => "unverified",
        (Some(r), Some(_)) if r.passed => "merged",
        (Some(r), _) if r.passed => "passing",
        (Some(_), _) => "failing",
    }
}

fn gate_links(task: &Task) -> Vec<serde_json::Value> {
    let id = task.id;
    let mut links = vec![
        json!({"rel": "self", "method": "GET", "href": format!("/api/tasks/{id}/merge-gate")}),
        json!({"rel": "schema", "method": "GET", "href": at_api_types::schemas::path_for(MERGE_GATE_SCHEMA_ID)}),
    ];
    if !super::tasks::criteria_locked(&task.phase) {
        links.push(json!({"rel": "update_criteria", "method": "PUT", "href": format!("/api/tasks/{id}")}));
        if task.worktree_path.is_some() && task.git_branch.is_some() && task.merged_at.is_none() {
            links.push(json!({"rel": "merge", "method": "POST", "href": format!("/api/tasks/{id}/merge")}));
        }
    }
    if task.phase.can_transition_to(&TaskPhase::Coding) {
        links.push(json!({"rel": "execute", "method": "POST", "href": format!("/api/tasks/{id}/execute")}));
    }
    links
}

/// GET /api/tasks/{id}/merge-gate -- the last merge-gate report for a task.
///
/// **Response:** 200 `ApiMergeGateState` `{task_id, state, acceptance_criteria,
/// report, links}` with `state` one of `passing`, `failing`, `merged`;
/// 404 `{"error": "no merge gate run", "state": "unverified", ...}` when the
/// gate never ran; 404 `{"error": "task not found"}` for an unknown task.
pub(crate) async fn get_task_merge_gate(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> (StatusCode, Json<serde_json::Value>) {
    let tasks = state.tasks.read().await;
    let Some(task) = tasks.get(&id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "task not found"})),
        );
    };
    let body = json!({
        "task_id": task.id,
        "state": gate_state(task),
        "acceptance_criteria": task.acceptance_criteria,
        "report": task.merge_gate_report,
        "links": gate_links(task),
    });
    if task.merge_gate_report.is_none() {
        let mut body = body;
        body["error"] = json!("no merge gate run");
        return (StatusCode::NOT_FOUND, Json(body));
    }
    (StatusCode::OK, Json(body))
}

/// POST /api/tasks/{id}/merge -- gated merge of the task's own worktree.
///
/// Resolves the worktree from `task.worktree_path` / `task.git_branch` (never
/// by substring), runs the gate with the task's criteria and merges only if
/// it passes. Same body and codes as `POST /api/worktrees/{id}/merge`:
/// - 200 `{"status": "success" | "nothing_to_merge" | "conflict", "branch", "files"?, "gate"}`
/// - 409 `{"status": "gate_failed", "branch", "error", "gate"}`
/// - 409 `{"status": "stale_head", "expected_head", "head"}` when
///   `expected_head` does not match the worktree
/// - 409 `{"status": "pipeline_running", "phase"}` while the pipeline owns it
/// - 404 unknown task or no worktree bound, 500 git failure
pub(crate) async fn merge_task(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
    body: Option<Json<TaskMergeRequest>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let req = body.map(|b| b.0).unwrap_or_default();
    let task = match state.tasks.read().await.get(&id) {
        Some(t) => t.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "task not found"})),
            )
        }
    };
    if super::tasks::criteria_locked(&task.phase) {
        return (
            StatusCode::CONFLICT,
            Json(json!({"status": "pipeline_running", "phase": task.phase,
                "error": "the execute pipeline owns this task's merge"})),
        );
    }
    let (Some(path), Some(branch)) = (task.worktree_path.clone(), task.git_branch.clone()) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no worktree bound to task", "task_id": id})),
        );
    };
    let repo_root = match &state.repo_root {
        Some(root) => root.clone(),
        None => match main_checkout_of(&path).await {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e, "branch": branch})),
                )
            }
        },
    };
    let info = worktree_info(&path, &branch);

    if let Some(expected) = req.expected_head.as_deref() {
        let head = WorktreeManager::new(&repo_root).worktree_head(&info);
        match head {
            Ok(head) if head == expected => {}
            Ok(head) => {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({"status": "stale_head", "branch": branch,
                        "expected_head": expected, "head": head,
                        "error": "worktree moved since the report the caller reviewed"})),
                )
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string(), "branch": branch})),
                )
            }
        }
    }

    let gate_config = state.settings_manager.load_or_default().merge_gate;
    let outcome = match run_gate(
        &state.merge_lock,
        &repo_root,
        gate_config,
        &info,
        &task.acceptance_criteria,
        true,
    )
    .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(error = %e, branch = %branch, "task merge failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string(), "branch": branch})),
            );
        }
    };
    let worktree_id = super::worktrees::stable_worktree_id(&path, &branch);
    record_gate_outcome(&state.tasks, &state.event_bus, Some(id), &worktree_id, &outcome).await;
    outcome.response()
}
