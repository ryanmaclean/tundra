//! Acceptance criteria end to end over the HTTP API, against real git repos:
//! author criteria on a task, run `POST /api/tasks/{id}/execute`, and watch
//! the live pipeline create the task worktree, run the merge gate, loop
//! through Fixing, and end in `complete` or `error` -- plus the task-scoped
//! merge endpoint, the report endpoint and the schema contract.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use at_api_types::merge_gate::{ApiMergeGateReport, ApiMergeResponse, MERGE_GATE_SCHEMA_ID};
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_bridge::protocol::BridgeMessage;
use at_core::merge_gate::{GateBlock, MergeGateReport};
use at_core::types::{Task, TaskPhase};
use at_core::worktree_manager::WorktreeManager;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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

/// Temp dir holding a git repo on `main` (`repo/`) and a daemon settings
/// file (`config.toml`).
struct Fixture {
    root: PathBuf,
    repo: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture(max_fix_iterations: usize) -> Fixture {
    let root = std::env::temp_dir().join(format!("at-accept-e2e-{}", Uuid::new_v4()));
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    sh_git(&repo, &["init", "-q", "-b", "main"]);
    sh_git(&repo, &["config", "user.name", "t"]);
    sh_git(&repo, &["config", "user.email", "t@example.com"]);
    sh_git(&repo, &["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.join("f"), "base\n").unwrap();
    sh_git(&repo, &["add", "f"]);
    sh_git(&repo, &["commit", "-q", "-m", "base"]);
    std::fs::write(
        root.join("config.toml"),
        format!(
            "[merge_gate]\nmax_fix_iterations = {max_fix_iterations}\ncommand_timeout_secs = 30\n"
        ),
    )
    .unwrap();
    Fixture { root, repo }
}

fn state_for(fx: &Fixture) -> Arc<ApiState> {
    let mut state = ApiState::new(EventBus::new()).with_relaxed_rate_limits();
    state.settings_manager = Arc::new(at_core::settings::SettingsManager::new(
        fx.root.join("config.toml"),
    ));
    state.repo_root = Some(fx.repo.clone());
    Arc::new(state)
}

async fn send(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            b = b.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(b.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Create a task over the API and move it to Planning so it can execute.
async fn create_task(app: &Router, state: &ApiState, title: &str, criteria: &[&str]) -> Uuid {
    let (code, body) = send(
        app,
        "POST",
        "/api/tasks",
        Some(json!({
            "title": title,
            "bead_id": Uuid::new_v4(),
            "category": "feature",
            "priority": "medium",
            "complexity": "small",
            "acceptance_criteria": criteria,
        })),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let id: Uuid = body["id"].as_str().unwrap().parse().unwrap();
    state
        .tasks
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .set_phase(TaskPhase::Planning);
    id
}

/// Bind a worktree to the task (as the pipeline would) with `files`
/// committed on its branch.
async fn bind_worktree_with_commit(fx: &Fixture, state: &ApiState, id: Uuid, files: &[&str]) {
    let task = state.tasks.read().await[&id].clone();
    let info = WorktreeManager::new(&fx.repo)
        .create_for_task(&task)
        .await
        .expect("create worktree");
    let wt = Path::new(&info.path);
    for f in files {
        std::fs::write(wt.join(f), format!("{f}\n")).unwrap();
        sh_git(wt, &["add", f]);
    }
    sh_git(wt, &["commit", "-q", "-m", "task work"]);
    let mut tasks = state.tasks.write().await;
    let t = tasks.get_mut(&id).unwrap();
    t.worktree_path = Some(info.path.clone());
    t.git_branch = Some(info.branch.clone());
}

async fn wait_terminal(app: &Router, id: Uuid) -> Value {
    for _ in 0..1500 {
        let (code, task) = send(app, "GET", &format!("/api/tasks/{id}"), None).await;
        assert_eq!(code, StatusCode::OK);
        if matches!(
            task["phase"].as_str(),
            Some("complete" | "error" | "stopped")
        ) {
            return task;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("task {id} never reached a terminal phase");
}

fn main_has(repo: &Path, file: &str) -> bool {
    sh_git(repo, &["ls-tree", "--name-only", "main"])
        .lines()
        .any(|l| l == file)
}

fn drain_events(rx: &flume::Receiver<Arc<BridgeMessage>>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        if let BridgeMessage::Event(p) = &*msg {
            out.push((p.event_type.clone(), p.message.clone()));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// (1) Refused gate: worktree created, fix loop, ends in error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refused_gate_loops_then_errors_with_report() {
    let fx = fixture(1);
    let state = state_for(&fx);
    let app = api_router(state.clone());
    let rx = state.event_bus.subscribe();
    let main_before = sh_git(&fx.repo, &["rev-parse", "main"]);

    let id = create_task(&app, &state, "Refused gate", &["test -f ok"]).await;
    let (code, body) = send(
        &app,
        "POST",
        &format!("/api/tasks/{id}/execute"),
        Some(json!({"merge_mode": "auto"})),
    )
    .await;
    assert_eq!(code, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["merge"], "gated");
    assert_eq!(body["acceptance_criteria_count"], 1);

    let task = wait_terminal(&app, id).await;
    assert_eq!(task["phase"], "error", "{task}");
    let report: MergeGateReport =
        serde_json::from_value(task["merge_gate_report"].clone()).unwrap();
    assert!(!report.passed);
    assert_eq!(task["error"], report.summary());
    assert_eq!(report.results[0].cmd, "test -f ok");
    assert_eq!(report.results[0].exit_code, Some(1));
    assert_eq!(report.criteria, vec!["test -f ok".to_string()]);
    assert!(report.head.is_some(), "report attests the verified commit");

    // The pipeline bound a worktree on a task branch.
    let wt = task["worktree_path"].as_str().unwrap();
    assert!(Path::new(wt).is_dir(), "{wt}");
    assert_eq!(task["git_branch"], "task/refused-gate");

    // Gate ran max_fix_iterations + 1 times; each refusal was announced.
    let events = drain_events(&rx);
    let refused: Vec<_> = events
        .iter()
        .filter(|(t, _)| t == "merge_gate_failed")
        .collect();
    assert_eq!(refused.len(), 2, "{events:?}");
    assert_eq!(refused[0].1, report.summary());
    assert!(events
        .iter()
        .any(|(t, _)| t == "merge_gate_fix_iteration_1"));
    assert!(!events.iter().any(|(t, _)| t == "merge_success"));

    // The fix prompt was logged for the agent, and nothing reached main.
    let logs: Vec<String> = task["build_logs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["line"].as_str().unwrap().to_string())
        .collect();
    assert!(
        logs.iter()
            .any(|l| l.contains("Failing acceptance criterion: `test -f ok`")),
        "{logs:?}"
    );
    assert_eq!(sh_git(&fx.repo, &["rev-parse", "main"]), main_before);

    // The report endpoint agrees.
    let (code, gate) = send(&app, "GET", &format!("/api/tasks/{id}/merge-gate"), None).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(gate["state"], "failing");
    assert_eq!(gate["report"]["schema"], MERGE_GATE_SCHEMA_ID);
}

// ---------------------------------------------------------------------------
// (2) Passing gate: auto merges; verify does not
// ---------------------------------------------------------------------------

#[tokio::test]
async fn passing_gate_in_auto_mode_merges_and_completes() {
    let fx = fixture(1);
    let state = state_for(&fx);
    let app = api_router(state.clone());
    let rx = state.event_bus.subscribe();

    let id = create_task(&app, &state, "Auto merge", &["test -f ok"]).await;
    bind_worktree_with_commit(&fx, &state, id, &["ok"]).await;

    let (code, body) = send(
        &app,
        "POST",
        &format!("/api/tasks/{id}/execute"),
        Some(json!({"merge_mode": "auto"})),
    )
    .await;
    assert_eq!(code, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["merge_mode"], "auto");

    let task = wait_terminal(&app, id).await;
    assert_eq!(task["phase"], "complete", "{task}");
    assert_eq!(task["merge_gate_report"]["passed"], true);
    assert!(task["merged_at"].is_string());
    assert!(task["completed_at"].is_string());
    assert!(main_has(&fx.repo, "ok"), "the task commit reached main");
    assert!(drain_events(&rx).iter().any(|(t, _)| t == "merge_success"));

    let (_, gate) = send(&app, "GET", &format!("/api/tasks/{id}/merge-gate"), None).await;
    assert_eq!(gate["state"], "merged");
}

#[tokio::test]
async fn passing_gate_in_default_verify_mode_leaves_main_alone_until_merged() {
    let fx = fixture(1);
    let state = state_for(&fx);
    let app = api_router(state.clone());

    let id = create_task(&app, &state, "Verify only", &["test -f ok"]).await;
    bind_worktree_with_commit(&fx, &state, id, &["ok"]).await;

    let (code, body) = send(&app, "POST", &format!("/api/tasks/{id}/execute"), None).await;
    assert_eq!(code, StatusCode::ACCEPTED);
    assert_eq!(body["merge_mode"], "verify");

    let task = wait_terminal(&app, id).await;
    assert_eq!(task["phase"], "complete", "{task}");
    assert_eq!(task["merge_gate_report"]["passed"], true);
    assert!(task["merged_at"].is_null());
    assert!(!main_has(&fx.repo, "ok"), "verify mode never merges");

    let (_, gate) = send(&app, "GET", &format!("/api/tasks/{id}/merge-gate"), None).await;
    assert_eq!(gate["state"], "passing");
    let head = gate["report"]["head"].as_str().unwrap().to_string();
    assert!(gate["links"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["rel"] == "merge"));

    // A stale expected_head is refused without merging.
    let (code, body) = send(
        &app,
        "POST",
        &format!("/api/tasks/{id}/merge"),
        Some(json!({"expected_head": "0000000000000000000000000000000000000000"})),
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["status"], "stale_head");
    assert_eq!(body["head"], head.as_str());
    assert!(!main_has(&fx.repo, "ok"));

    // The reviewed head merges.
    let (code, body) = send(
        &app,
        "POST",
        &format!("/api/tasks/{id}/merge"),
        Some(json!({"expected_head": head})),
    )
    .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "success");
    assert!(main_has(&fx.repo, "ok"));
    let (_, gate) = send(&app, "GET", &format!("/api/tasks/{id}/merge-gate"), None).await;
    assert_eq!(gate["state"], "merged");
}

// ---------------------------------------------------------------------------
// (3) Task-scoped merge returns the 409 gate report
// ---------------------------------------------------------------------------

#[tokio::test]
async fn task_merge_returns_409_with_versioned_gate_report() {
    let fx = fixture(1);
    let state = state_for(&fx);
    let app = api_router(state.clone());
    let id = create_task(&app, &state, "Manual merge", &["test -f ok"]).await;
    bind_worktree_with_commit(&fx, &state, id, &["not-ok"]).await;

    let (code, body) = send(&app, "POST", &format!("/api/tasks/{id}/merge"), None).await;

    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["status"], "gate_failed");
    assert_eq!(body["gate"]["schema"], MERGE_GATE_SCHEMA_ID);
    assert_eq!(body["branch"], "task/manual-merge");
    // The wasm-safe client type parses the 409 body.
    let parsed: ApiMergeResponse = serde_json::from_value(body.clone()).unwrap();
    let gate = parsed.gate.unwrap();
    assert!(!gate.passed);
    assert_eq!(Some(gate.summary()), parsed.error);
    assert!(!main_has(&fx.repo, "not-ok"));
    // Stored on the task for the UI.
    assert_eq!(
        state.tasks.read().await[&id]
            .merge_gate_report
            .as_ref()
            .map(|r| r.passed),
        Some(false)
    );

    // A running pipeline owns the merge.
    state
        .tasks
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .set_phase(TaskPhase::Coding);
    let (code, body) = send(&app, "POST", &format!("/api/tasks/{id}/merge"), None).await;
    assert_eq!(code, StatusCode::CONFLICT);
    assert_eq!(body["status"], "pipeline_running");
}

// ---------------------------------------------------------------------------
// (4) Concurrent pipelines in one repo
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_executes_in_one_repo_do_not_race_on_git() {
    let fx = fixture(0);
    let mut state = ApiState::new(EventBus::new()).with_relaxed_rate_limits();
    state.settings_manager = Arc::new(at_core::settings::SettingsManager::new(
        fx.root.join("config.toml"),
    ));
    state.repo_root = Some(fx.repo.clone());
    state.pipeline_semaphore = Arc::new(tokio::sync::Semaphore::new(4));
    state.pipeline_max_concurrent = 4;
    let state = Arc::new(state);
    let app = api_router(state.clone());

    let a = create_task(&app, &state, "Concurrent a", &["test -f a"]).await;
    let b = create_task(&app, &state, "Concurrent b", &["test -f b"]).await;
    bind_worktree_with_commit(&fx, &state, a, &["a"]).await;
    bind_worktree_with_commit(&fx, &state, b, &["b"]).await;

    let exec = |id: Uuid| {
        let app = app.clone();
        async move {
            send(
                &app,
                "POST",
                &format!("/api/tasks/{id}/execute"),
                Some(json!({"merge_mode": "auto"})),
            )
            .await
        }
    };
    let ((ca, _), (cb, _)) = tokio::join!(exec(a), exec(b));
    assert_eq!((ca, cb), (StatusCode::ACCEPTED, StatusCode::ACCEPTED));

    let (ta, tb) = tokio::join!(wait_terminal(&app, a), wait_terminal(&app, b));
    for t in [&ta, &tb] {
        assert_eq!(t["phase"], "complete", "{t}");
        assert!(
            !t.to_string().contains("index.lock"),
            "git lock contention: {t}"
        );
    }
    assert!(main_has(&fx.repo, "a") && main_has(&fx.repo, "b"));
}

// ---------------------------------------------------------------------------
// Schema contract
// ---------------------------------------------------------------------------

fn full_report() -> MergeGateReport {
    let mut r = MergeGateReport::new("task/x", "main", "/r/.worktrees/x");
    r.criteria = vec!["test -f ok".into()];
    r.head = Some("abc123".into());
    r.blocked_by.push(GateBlock::UncommittedChanges {
        location: "worktree".into(),
        path: "/r/.worktrees/x".into(),
        files: vec![" M f".into()],
    });
    r
}

#[test]
fn schema_properties_match_serialized_report() {
    let schema: Value =
        serde_json::from_str(at_api_types::merge_gate::MERGE_GATE_REPORT_SCHEMA_JSON).unwrap();
    let report = serde_json::to_value(full_report()).unwrap();
    let keys: std::collections::BTreeSet<&str> = report
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let props: std::collections::BTreeSet<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, props, "schema drift");
    for req in schema["required"].as_array().unwrap() {
        assert!(keys.contains(req.as_str().unwrap()), "{req}");
    }
    let result_props: std::collections::BTreeSet<&str> = schema["$defs"]["CommandResult"]
        ["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let result = serde_json::to_value(at_core::merge_gate::CommandResult {
        cmd: "c".into(),
        exit_code: Some(0),
        timed_out: false,
        duration_ms: 1,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
    })
    .unwrap();
    let result_keys: std::collections::BTreeSet<&str> = result
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(result_keys, result_props);
}

#[test]
fn api_report_round_trips_the_core_report() {
    let core = full_report();
    let api: ApiMergeGateReport =
        serde_json::from_value(serde_json::to_value(&core).unwrap()).unwrap();
    assert_eq!(api.schema, MERGE_GATE_SCHEMA_ID);
    assert_eq!(api.summary(), core.summary());
    assert_eq!(api.blocked_by[0].kind, "uncommitted_changes");
    assert_eq!(api.criteria, core.criteria);
    assert_eq!(api.head, core.head);
    // And back: the core type accepts what the client re-serializes.
    let back: MergeGateReport =
        serde_json::from_value(serde_json::to_value(&api).unwrap()).unwrap();
    assert_eq!(back.blocked_by, core.blocked_by);
    assert_eq!(back.summary(), core.summary());
}

#[test]
fn task_serializes_criteria_and_report_for_clients() {
    let mut t = Task::new(
        "t",
        Uuid::new_v4(),
        at_core::types::TaskCategory::Feature,
        at_core::types::TaskPriority::Medium,
        at_core::types::TaskComplexity::Small,
    );
    t.acceptance_criteria = vec!["true".into()];
    t.merge_gate_report = Some(full_report());
    let v = serde_json::to_value(&t).unwrap();
    assert_eq!(v["acceptance_criteria"], json!(["true"]));
    assert_eq!(v["merge_gate_report"]["schema"], MERGE_GATE_SCHEMA_ID);
    assert!(v["merged_at"].is_null());
}
