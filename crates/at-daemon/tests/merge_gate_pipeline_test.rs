//! The daemon pipeline's Merging phase goes through the merge gate: a refused
//! gate sends the task back through Fixing -> Qa -> Merging instead of merging.
//! Uses a real git repo; only the agent CLI is mocked.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use at_agents::executor::{AgentExecutor, PtySpawner, SpawnedProcess};
use at_bridge::event_bus::EventBus;
use at_bridge::protocol::BridgeMessage;
use at_core::types::*;
use at_core::worktree_manager::WorktreeManager;
use at_daemon::orchestrator::{OrchestratorError, TaskOrchestrator};
use uuid::Uuid;

/// Agent stand-in. When asked to fix a merge-gate failure it "fixes" the
/// branch by committing a `fixed` file in its working directory.
struct FixerSpawner {
    fix_by_committing: bool,
    fix_prompts: Mutex<usize>,
    _write_rxs: Mutex<Vec<flume::Receiver<Vec<u8>>>>,
}

impl FixerSpawner {
    fn new(fix_by_committing: bool) -> Self {
        Self {
            fix_by_committing,
            fix_prompts: Mutex::new(0),
            _write_rxs: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl PtySpawner for FixerSpawner {
    fn spawn(
        &self,
        cmd: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<SpawnedProcess, String> {
        self.spawn_in(cmd, args, env, None)
    }

    fn spawn_in(
        &self,
        _cmd: &str,
        args: &[&str],
        _env: &[(&str, &str)],
        cwd: Option<&Path>,
    ) -> Result<SpawnedProcess, String> {
        if args.iter().any(|a| a.contains("merge gate refused")) {
            *self.fix_prompts.lock().unwrap() += 1;
            if let (true, Some(dir)) = (self.fix_by_committing, cwd) {
                std::fs::write(dir.join("fixed"), "fixed\n").unwrap();
                sh_git(dir, &["add", "fixed"]);
                sh_git(dir, &["commit", "-q", "-m", "fix: satisfy merge gate"]);
            }
        }
        let (read_tx, read_rx) = flume::bounded(16);
        let (write_tx, write_rx) = flume::bounded::<Vec<u8>>(16);
        self._write_rxs.lock().unwrap().push(write_rx);
        let _ = read_tx.send(b"ok\n".to_vec());
        drop(read_tx);
        Ok(SpawnedProcess::new(
            Uuid::new_v4(),
            read_rx,
            write_tx,
            false,
        ))
    }
}

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

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn init_repo() -> Repo {
    let p = std::env::temp_dir().join(format!("at-daemon-gate-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&p).unwrap();
    sh_git(&p, &["init", "-q", "-b", "main"]);
    sh_git(&p, &["config", "user.name", "t"]);
    sh_git(&p, &["config", "user.email", "t@example.com"]);
    sh_git(&p, &["config", "commit.gpgsign", "false"]);
    std::fs::write(p.join("f"), "base\n").unwrap();
    sh_git(&p, &["add", "f"]);
    sh_git(&p, &["commit", "-q", "-m", "base"]);
    Repo(p)
}

fn orchestrator(
    repo: &Path,
    spawner: Arc<FixerSpawner>,
    max_fix: usize,
) -> (TaskOrchestrator, flume::Receiver<Arc<BridgeMessage>>) {
    let bus = EventBus::new();
    let rx = bus.subscribe();
    let executor = AgentExecutor::with_spawner(spawner, bus.clone());
    let orch = TaskOrchestrator::new(executor, WorktreeManager::new(repo), bus)
        .with_max_gate_fix_iterations(max_fix);
    (orch, rx)
}

fn task_with_criteria(criteria: &[&str]) -> Task {
    let mut task = Task::new(
        "Gate Me",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );
    task.acceptance_criteria = criteria.iter().map(|s| s.to_string()).collect();
    task
}

fn events(rx: &flume::Receiver<Arc<BridgeMessage>>) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        if let BridgeMessage::Event(p) = &*msg {
            out.push(p.event_type.clone());
        }
    }
    out
}

#[tokio::test]
async fn failed_gate_enters_fix_loop_then_merges_once_fixed() {
    let repo = init_repo();
    let spawner = Arc::new(FixerSpawner::new(true));
    let (orch, rx) = orchestrator(&repo.0, spawner.clone(), 3);
    let mut task = task_with_criteria(&["test -f fixed"]);

    orch.start_task(&mut task)
        .await
        .expect("pipeline completes");

    assert_eq!(task.phase, TaskPhase::Complete);
    assert_eq!(*spawner.fix_prompts.lock().unwrap(), 1, "one fix iteration");
    let report = task.merge_gate_report.as_ref().expect("gate report stored");
    assert!(report.passed);
    assert_eq!(report.results[0].exit_code, Some(0));

    let ev = events(&rx);
    let gate_failed = ev.iter().position(|e| e == "merge_gate_failed").unwrap();
    let fixing = ev.iter().position(|e| e == "phase_start:Fixing").unwrap();
    let merged = ev.iter().position(|e| e == "merge_success").unwrap();
    assert!(gate_failed < fixing && fixing < merged, "{ev:?}");

    // The fix commit reached main only after the gate passed.
    let files = sh_git(&repo.0, &["ls-tree", "--name-only", "main"]);
    assert!(files.lines().any(|l| l == "fixed"), "{files}");
}

#[tokio::test]
async fn gate_that_never_passes_errors_without_merging() {
    let repo = init_repo();
    let main_before = sh_git(&repo.0, &["rev-parse", "main"]);
    let spawner = Arc::new(FixerSpawner::new(false));
    let (orch, rx) = orchestrator(&repo.0, spawner.clone(), 2);
    let mut task = task_with_criteria(&["echo still broken >&2; exit 1"]);

    let err = orch.start_task(&mut task).await.unwrap_err();

    match err {
        OrchestratorError::MergeGateFailed(report) => {
            assert!(!report.passed);
            assert_eq!(report.results[0].stderr_tail, "still broken\n");
        }
        other => panic!("expected MergeGateFailed, got {other:?}"),
    }
    assert_eq!(task.phase, TaskPhase::Error);
    assert!(task
        .error
        .as_deref()
        .unwrap()
        .contains("after 2 fix iteration"));
    assert_eq!(*spawner.fix_prompts.lock().unwrap(), 2);
    assert!(task.completed_at.is_none());

    let ev = events(&rx);
    assert_eq!(ev.iter().filter(|e| *e == "merge_gate_failed").count(), 3);
    assert!(!ev
        .iter()
        .any(|e| e == "merge_success" || e == "task_complete"));
    // Nothing reached main.
    assert_eq!(sh_git(&repo.0, &["rev-parse", "main"]), main_before);
}

#[tokio::test]
async fn config_max_fix_iterations_is_honoured_without_override() {
    use at_core::merge_gate::MergeGateConfig;
    let repo = init_repo();
    let spawner = Arc::new(FixerSpawner::new(false));
    let bus = EventBus::new();
    let executor = AgentExecutor::with_spawner(spawner.clone(), bus.clone());
    let wm = WorktreeManager::new(&repo.0).with_merge_gate_config(MergeGateConfig {
        max_fix_iterations: 1,
        ..MergeGateConfig::default()
    });
    // No with_max_gate_fix_iterations(): the config value applies.
    let orch = TaskOrchestrator::new(executor, wm, bus);
    let mut task = task_with_criteria(&["exit 1"]);

    let err = orch.start_task(&mut task).await.unwrap_err();
    assert!(matches!(err, OrchestratorError::MergeGateFailed(_)), "{err:?}");
    assert_eq!(*spawner.fix_prompts.lock().unwrap(), 1);
    assert!(task
        .error
        .as_deref()
        .unwrap()
        .contains("after 1 fix iteration"));
}

#[tokio::test]
async fn criteria_without_worktree_fail_closed() {
    // Not a git repo: worktree creation fails, so no branch is bound.
    let dir = std::env::temp_dir().join(format!("at-daemon-nogit-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let _cleanup = Repo(dir.clone());
    let spawner = Arc::new(FixerSpawner::new(false));
    let (orch, rx) = orchestrator(&dir, spawner, 3);
    let mut task = task_with_criteria(&["true"]);

    let err = orch.start_task(&mut task).await.unwrap_err();

    assert!(matches!(err, OrchestratorError::InvalidState(_)), "{err:?}");
    assert_eq!(task.phase, TaskPhase::Error);
    assert!(task.error.as_deref().unwrap().contains("no worktree"));
    let ev = events(&rx);
    assert!(!ev.iter().any(|e| e == "task_complete"), "{ev:?}");
}

#[tokio::test]
async fn no_criteria_and_no_worktree_still_completes() {
    let dir = std::env::temp_dir().join(format!("at-daemon-nogit-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let _cleanup = Repo(dir.clone());
    let (orch, _rx) = orchestrator(&dir, Arc::new(FixerSpawner::new(false)), 3);
    let mut task = task_with_criteria(&[]);
    orch.start_task(&mut task).await.expect("completes");
    assert_eq!(task.phase, TaskPhase::Complete);
}

/// Git stand-in whose `rev-list` fails, so the gate passes (clean, no
/// criteria) but the merge itself errors.
struct RevListFails;

impl at_core::worktree_manager::GitRunner for RevListFails {
    fn run_git(
        &self,
        _dir: &str,
        args: &[&str],
    ) -> Result<at_core::worktree_manager::GitOutput, String> {
        let fail = args.first() == Some(&"rev-list");
        Ok(at_core::worktree_manager::GitOutput {
            success: !fail,
            stdout: if args.first() == Some(&"rev-parse") {
                "0123456789abcdef\n".into()
            } else {
                String::new()
            },
            stderr: if fail { "boom".into() } else { String::new() },
        })
    }
}

#[tokio::test]
async fn merge_error_ends_in_error_not_ok() {
    let dir = std::env::temp_dir().join(format!("at-daemon-mergeerr-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let _cleanup = Repo(dir.clone());
    let bus = EventBus::new();
    let executor = AgentExecutor::with_spawner(Arc::new(FixerSpawner::new(false)), bus.clone());
    let wm = WorktreeManager::with_git_runner(&dir, Box::new(RevListFails));
    let orch = TaskOrchestrator::new(executor, wm, bus);
    let mut task = task_with_criteria(&[]);

    let err = orch.start_task(&mut task).await.unwrap_err();

    assert!(matches!(err, OrchestratorError::Worktree(_)), "{err:?}");
    assert_eq!(task.phase, TaskPhase::Error);
    assert!(task.error.as_deref().unwrap().contains("Merge failed"));
    assert!(task.completed_at.is_none());
}
