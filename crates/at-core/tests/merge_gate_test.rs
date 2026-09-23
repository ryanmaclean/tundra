//! Real-git tests for the merge gate in front of `WorktreeManager::merge_to_main`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use at_core::merge_gate::{GateBlock, MergeGateConfig, MERGE_GATE_SCHEMA};
use at_core::worktree::WorktreeInfo;
use at_core::worktree_manager::{GatedMerge, MergeResult, WorktreeManager};
use chrono::Utc;

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

fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    sh_git(p, &["init", "-q", "-b", "main"]);
    sh_git(p, &["config", "user.name", "t"]);
    sh_git(p, &["config", "user.email", "t@example.com"]);
    sh_git(p, &["config", "commit.gpgsign", "false"]);
    std::fs::write(p.join("f"), "base\n").unwrap();
    std::fs::write(p.join(".gitignore"), ".worktrees/\n").unwrap();
    sh_git(p, &["add", "f", ".gitignore"]);
    sh_git(p, &["commit", "-q", "-m", "base"]);
    dir
}

/// Worktree `task/<name>` with one commit adding file `g`.
fn task_worktree(repo: &Path, name: &str) -> WorktreeInfo {
    let wt_path = repo.join(".worktrees").join(name);
    let branch = format!("task/{name}");
    sh_git(
        repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &branch,
            wt_path.to_str().unwrap(),
            "main",
        ],
    );
    std::fs::write(wt_path.join("g"), "task work\n").unwrap();
    sh_git(&wt_path, &["add", "g"]);
    sh_git(&wt_path, &["commit", "-q", "-m", "task work"]);
    WorktreeInfo {
        path: wt_path.to_string_lossy().to_string(),
        branch,
        base_branch: "main".to_string(),
        task_name: name.to_string(),
        created_at: Utc::now(),
    }
}

fn main_has(repo: &Path, file: &str) -> bool {
    sh_git(repo, &["ls-tree", "--name-only", "main"])
        .lines()
        .any(|l| l == file)
}

fn criteria(cmds: &[&str]) -> Vec<String> {
    cmds.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn gate_passes_then_merges() {
    let repo = init_repo();
    let p = repo.path();
    let wt = task_worktree(p, "pass");

    let manager = WorktreeManager::new(p);
    let outcome = manager
        .merge_to_main_gated(&wt, &criteria(&["test -f g", "echo checked"]))
        .await
        .unwrap();

    let GatedMerge::Attempted { report, result } = outcome else {
        panic!("expected merge attempt, got {outcome:?}");
    };
    assert_eq!(result, MergeResult::Success);
    assert!(report.passed);
    assert_eq!(report.schema, MERGE_GATE_SCHEMA);
    assert!(report.blocked_by.is_empty());
    assert_eq!(report.results.len(), 2);
    assert_eq!(report.results[0].exit_code, Some(0));
    assert_eq!(report.results[1].stdout_tail, "checked\n");
    assert!(main_has(p, "g"));
}

#[tokio::test]
async fn failing_criterion_refuses_merge_and_stops() {
    let repo = init_repo();
    let p = repo.path();
    let wt = task_worktree(p, "fail");
    let main_before = sh_git(p, &["rev-parse", "main"]);

    let manager = WorktreeManager::new(p);
    let outcome = manager
        .merge_to_main_gated(
            &wt,
            &criteria(&["echo ok", "echo boom >&2; exit 3", "echo never-run"]),
        )
        .await
        .unwrap();

    let GatedMerge::Refused { report } = outcome else {
        panic!("expected refusal, got {outcome:?}");
    };
    assert!(!report.passed);
    assert_eq!(report.results.len(), 2, "stops at first failure");
    let failed = &report.results[1];
    assert_eq!(failed.cmd, "echo boom >&2; exit 3");
    assert_eq!(failed.exit_code, Some(3));
    assert!(!failed.timed_out);
    assert_eq!(failed.stderr_tail, "boom\n");
    assert!(report.summary().contains("exited 3"));

    // Nothing merged; worktree and branch left for the fix loop.
    assert_eq!(sh_git(p, &["rev-parse", "main"]), main_before);
    assert!(!main_has(p, "g"));
    assert!(Path::new(&wt.path).exists());
}

#[tokio::test]
async fn criterion_timeout_kills_command_and_refuses() {
    let repo = init_repo();
    let p = repo.path();
    let wt = task_worktree(p, "slow");

    let manager = WorktreeManager::new(p).with_merge_gate_config(MergeGateConfig {
        command_timeout_secs: 1,
        ..MergeGateConfig::default()
    });
    let started = Instant::now();
    // Background child + wait: only a process-group kill stops this in time.
    let outcome = manager
        .merge_to_main_gated(&wt, &criteria(&["echo started; sleep 30 & wait"]))
        .await
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "timeout not enforced: {:?}",
        started.elapsed()
    );

    let GatedMerge::Refused { report } = outcome else {
        panic!("expected refusal, got {outcome:?}");
    };
    let r = &report.results[0];
    assert!(r.timed_out);
    assert_eq!(r.exit_code, None);
    assert!(r.duration_ms >= 1000);
    assert_eq!(r.stdout_tail, "started\n", "partial output kept");
    assert!(!main_has(p, "g"));
}

#[tokio::test]
async fn uncommitted_changes_in_worktree_refuse_without_running_criteria() {
    let repo = init_repo();
    let p = repo.path();
    let wt = task_worktree(p, "dirty");
    std::fs::write(PathBuf::from(&wt.path).join("g"), "edited, not committed\n").unwrap();
    let marker = p.join("criterion-ran");

    let manager = WorktreeManager::new(p);
    let cmd = format!("touch {}", marker.display());
    let outcome = manager
        .merge_to_main_gated(&wt, &criteria(&[&cmd]))
        .await
        .unwrap();

    let GatedMerge::Refused { report } = outcome else {
        panic!("expected refusal, got {outcome:?}");
    };
    assert!(report.results.is_empty());
    assert!(!marker.exists(), "criteria must not run when blocked");
    match &report.blocked_by[..] {
        [GateBlock::UncommittedChanges {
            location, files, ..
        }] => {
            assert_eq!(location, "worktree");
            assert_eq!(files.len(), 1);
            assert!(files[0].ends_with('g'), "{files:?}");
        }
        other => panic!("unexpected blocks: {other:?}"),
    }
    assert!(!main_has(p, "g"));
}

#[tokio::test]
async fn uncommitted_changes_in_base_refuse() {
    let repo = init_repo();
    let p = repo.path();
    let wt = task_worktree(p, "basedirty");
    std::fs::write(p.join("f"), "local edit\n").unwrap();

    let outcome = WorktreeManager::new(p)
        .merge_to_main_gated(&wt, &[])
        .await
        .unwrap();
    let GatedMerge::Refused { report } = outcome else {
        panic!("expected refusal, got {outcome:?}");
    };
    assert!(matches!(
        &report.blocked_by[..],
        [GateBlock::UncommittedChanges { location, .. }] if location == "base"
    ));
}

#[tokio::test]
async fn behind_check_is_off_by_default_and_enforced_when_configured() {
    let repo = init_repo();
    let p = repo.path();
    let wt = task_worktree(p, "behind");
    for i in 0..2 {
        std::fs::write(p.join("f"), format!("main {i}\n")).unwrap();
        sh_git(p, &["commit", "-q", "-am", &format!("main {i}")]);
    }

    let strict = WorktreeManager::new(p).with_merge_gate_config(MergeGateConfig {
        max_behind_commits: Some(1),
        ..MergeGateConfig::default()
    });
    let GatedMerge::Refused { report } = strict.merge_to_main_gated(&wt, &[]).await.unwrap() else {
        panic!("expected refusal when 2 behind with max 1");
    };
    assert_eq!(
        report.blocked_by,
        vec![GateBlock::BehindTarget {
            behind: 2,
            max_behind: 1
        }]
    );

    // Default config: no behind limit, merge proceeds.
    let outcome = WorktreeManager::new(p)
        .merge_to_main_gated(&wt, &[])
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        GatedMerge::Attempted {
            result: MergeResult::Success,
            ..
        }
    ));
    assert!(main_has(p, "g"));
}
