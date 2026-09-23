//! Merge gate: verify a task branch before it is merged into its target.
//!
//! Ported from bop's `merge_gate.rs` (MIT, `bop-cli/src/merge_gate.rs`) and
//! adapted to tundra's git worktrees. Before a task branch is merged the gate:
//!
//! 1. refuses when the task worktree or the merge target checkout has
//!    uncommitted tracked changes (what was verified must be what merges, and
//!    a dirty base would be swept into the merge commit),
//! 2. optionally refuses when the branch is more than
//!    [`MergeGateConfig::max_behind_commits`] behind the target (default off),
//! 3. runs the task's acceptance criteria as `sh -c` commands inside the
//!    worktree, one at a time, each bounded by
//!    [`MergeGateConfig::command_timeout_secs`], stopping at the first failure.
//!
//! The outcome is a [`MergeGateReport`] (schema [`MERGE_GATE_SCHEMA`]) that
//! callers return verbatim to agents and API clients.
//!
//! VCS-specific checks go through the [`GateVcs`] trait; [`GitGateVcs`] is the
//! only backend today. The acceptance-criteria runner is VCS-agnostic.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tracing::{info, warn};

use crate::worktree_manager::GitRunner;

/// Versioned schema identifier carried by every [`MergeGateReport`].
pub const MERGE_GATE_SCHEMA: &str = "at.merge_gate.report/v1";

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Merge gate settings (`[merge_gate]` in `config.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MergeGateConfig {
    /// Wall-clock limit for each acceptance-criteria command, in seconds.
    pub command_timeout_secs: u64,
    /// Refuse to merge when the branch is more than this many commits behind
    /// the target; the branch must be rebased first. `None` disables the check.
    pub max_behind_commits: Option<u64>,
    /// Maximum bytes of stdout / stderr kept per command in the report.
    pub output_tail_bytes: usize,
}

impl Default for MergeGateConfig {
    fn default() -> Self {
        Self {
            command_timeout_secs: 600,
            max_behind_commits: None,
            output_tail_bytes: 4096,
        }
    }
}

impl MergeGateConfig {
    pub fn command_timeout(&self) -> Duration {
        Duration::from_secs(self.command_timeout_secs)
    }
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

/// Structured result of running the merge gate for one branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeGateReport {
    /// Always [`MERGE_GATE_SCHEMA`].
    pub schema: String,
    /// `true` only when nothing blocks the merge and every command exited 0.
    pub passed: bool,
    /// Task branch being merged.
    pub branch: String,
    /// Branch it would be merged into.
    pub target: String,
    /// Worktree the acceptance criteria ran in.
    pub worktree: String,
    /// Preconditions that refused the merge before (or instead of) running
    /// commands. Empty when no precondition failed.
    #[serde(default)]
    pub blocked_by: Vec<GateBlock>,
    /// One entry per acceptance-criteria command that ran, in order. The gate
    /// stops at the first failure, so later criteria are absent.
    #[serde(default)]
    pub results: Vec<CommandResult>,
}

impl MergeGateReport {
    fn new(branch: &str, target: &str, worktree: &str) -> Self {
        Self {
            schema: MERGE_GATE_SCHEMA.to_string(),
            passed: false,
            branch: branch.to_string(),
            target: target.to_string(),
            worktree: worktree.to_string(),
            blocked_by: Vec::new(),
            results: Vec::new(),
        }
    }

    /// One-line human summary of why the gate failed (or that it passed).
    pub fn summary(&self) -> String {
        if self.passed {
            return format!("merge gate passed ({} commands)", self.results.len());
        }
        if let Some(block) = self.blocked_by.first() {
            return format!("merge gate refused: {}", block.describe());
        }
        match self.results.iter().find(|r| !r.success()) {
            Some(r) if r.timed_out => format!("merge gate refused: `{}` timed out", r.cmd),
            Some(r) => match r.exit_code {
                Some(code) => format!("merge gate refused: `{}` exited {code}", r.cmd),
                None => format!("merge gate refused: `{}` did not exit normally", r.cmd),
            },
            None => "merge gate refused".to_string(),
        }
    }
}

/// Outcome of one acceptance-criteria command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResult {
    /// The command exactly as given in the task's acceptance criteria.
    pub cmd: String,
    /// Process exit code; `null` when the command timed out, was killed by a
    /// signal, or could not be spawned.
    pub exit_code: Option<i32>,
    /// `true` when the command exceeded the per-command timeout and was killed.
    #[serde(default)]
    pub timed_out: bool,
    pub duration_ms: u64,
    /// Last [`MergeGateConfig::output_tail_bytes`] bytes of stdout.
    pub stdout_tail: String,
    /// Last [`MergeGateConfig::output_tail_bytes`] bytes of stderr (spawn
    /// errors are reported here too).
    pub stderr_tail: String,
}

impl CommandResult {
    pub fn success(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }
}

/// A precondition that refused the merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GateBlock {
    /// Tracked files are modified in the task worktree (`location: "worktree"`)
    /// or in the target checkout (`location: "base"`).
    UncommittedChanges {
        location: String,
        path: String,
        files: Vec<String>,
    },
    /// The branch is further behind the target than allowed; rebase first.
    BehindTarget { behind: u64, max_behind: u64 },
    /// A VCS query needed by the gate failed, so the gate fails closed.
    VcsError { message: String },
}

impl GateBlock {
    pub fn describe(&self) -> String {
        match self {
            GateBlock::UncommittedChanges {
                location,
                path,
                files,
            } => format!(
                "{location} {path} has {} uncommitted tracked change(s)",
                files.len()
            ),
            GateBlock::BehindTarget { behind, max_behind } => {
                format!("branch is {behind} commits behind target (max {max_behind}); rebase first")
            }
            GateBlock::VcsError { message } => format!("vcs error: {message}"),
        }
    }
}

// ---------------------------------------------------------------------------
// VCS backend
// ---------------------------------------------------------------------------

/// VCS queries the gate needs. Implement this to add a backend (e.g. jj).
pub trait GateVcs: Send + Sync {
    /// Tracked paths with uncommitted changes in `dir` (untracked files are
    /// ignored). Empty means clean.
    fn uncommitted_tracked_changes(&self, dir: &str) -> Result<Vec<String>, String>;

    /// Number of commits on `target` that `branch` does not contain.
    fn commits_behind(&self, repo_dir: &str, branch: &str, target: &str) -> Result<u64, String>;
}

/// [`GateVcs`] backed by the `git` CLI through a [`GitRunner`].
pub struct GitGateVcs<'a> {
    git: &'a dyn GitRunner,
}

impl<'a> GitGateVcs<'a> {
    pub fn new(git: &'a dyn GitRunner) -> Self {
        Self { git }
    }

    fn run(&self, dir: &str, args: &[&str]) -> Result<String, String> {
        match self.git.run_git(dir, args) {
            Ok(o) if o.success => Ok(o.stdout),
            Ok(o) => Err(format!("git {}: {}", args.join(" "), o.stderr.trim())),
            Err(e) => Err(format!("git {}: {e}", args.join(" "))),
        }
    }
}

impl GateVcs for GitGateVcs<'_> {
    fn uncommitted_tracked_changes(&self, dir: &str) -> Result<Vec<String>, String> {
        let out = self.run(dir, &["status", "--porcelain", "--untracked-files=no"])?;
        Ok(out
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.to_string())
            .collect())
    }

    fn commits_behind(&self, repo_dir: &str, branch: &str, target: &str) -> Result<u64, String> {
        let range = format!("{branch}..{target}");
        let out = self.run(repo_dir, &["rev-list", "--count", &range])?;
        out.trim()
            .parse()
            .map_err(|_| format!("unexpected `git rev-list --count {range}` output: {out:?}"))
    }
}

// ---------------------------------------------------------------------------
// Gate
// ---------------------------------------------------------------------------

/// What the gate is asked to verify.
#[derive(Debug, Clone, Copy)]
pub struct GateTarget<'a> {
    /// Checkout the branch will be merged into.
    pub repo_dir: &'a str,
    /// Task worktree; acceptance criteria run here.
    pub worktree_dir: &'a str,
    pub branch: &'a str,
    pub target: &'a str,
}

/// Runs preconditions and acceptance criteria for a branch.
pub struct MergeGate<'a> {
    config: &'a MergeGateConfig,
    vcs: &'a dyn GateVcs,
}

impl<'a> MergeGate<'a> {
    pub fn new(config: &'a MergeGateConfig, vcs: &'a dyn GateVcs) -> Self {
        Self { config, vcs }
    }

    /// Evaluate the gate. Never merges anything; the caller does that only
    /// when `report.passed` is `true`.
    pub async fn evaluate(&self, t: GateTarget<'_>, criteria: &[String]) -> MergeGateReport {
        let mut report = MergeGateReport::new(t.branch, t.target, t.worktree_dir);

        self.check_clean("worktree", t.worktree_dir, &mut report);
        if t.repo_dir != t.worktree_dir {
            self.check_clean("base", t.repo_dir, &mut report);
        }
        if let Some(max_behind) = self.config.max_behind_commits {
            match self.vcs.commits_behind(t.repo_dir, t.branch, t.target) {
                Ok(behind) if behind > max_behind => report
                    .blocked_by
                    .push(GateBlock::BehindTarget { behind, max_behind }),
                Ok(_) => {}
                Err(message) => report.blocked_by.push(GateBlock::VcsError { message }),
            }
        }

        if !report.blocked_by.is_empty() {
            warn!(branch = %t.branch, summary = %report.summary(), "merge gate blocked");
            return report;
        }

        for criterion in criteria {
            let result = run_criterion(
                criterion,
                Path::new(t.worktree_dir),
                self.config.command_timeout(),
                self.config.output_tail_bytes,
            )
            .await;
            let ok = result.success();
            report.results.push(result);
            if !ok {
                break;
            }
        }

        report.passed = report.results.iter().all(CommandResult::success);
        if report.passed {
            info!(branch = %t.branch, commands = report.results.len(), "merge gate passed");
        } else {
            warn!(branch = %t.branch, summary = %report.summary(), "merge gate failed");
        }
        report
    }

    fn check_clean(&self, location: &str, dir: &str, report: &mut MergeGateReport) {
        match self.vcs.uncommitted_tracked_changes(dir) {
            Ok(files) if files.is_empty() => {}
            Ok(files) => report.blocked_by.push(GateBlock::UncommittedChanges {
                location: location.to_string(),
                path: dir.to_string(),
                files,
            }),
            Err(message) => report.blocked_by.push(GateBlock::VcsError { message }),
        }
    }
}

// ---------------------------------------------------------------------------
// Command runner
// ---------------------------------------------------------------------------

/// Run one acceptance criterion as `sh -c <cmd>` in `dir`.
///
/// On timeout the whole process group is killed (so `sh -c 'sleep 999 & wait'`
/// cannot outlive the gate) and whatever output was produced is kept.
pub async fn run_criterion(
    cmd: &str,
    dir: &Path,
    timeout: Duration,
    tail_bytes: usize,
) -> CommandResult {
    let started = Instant::now();
    let mut command = tokio::process::Command::new("sh");
    command
        .arg("-c")
        .arg(cmd)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                cmd: cmd.to_string(),
                exit_code: None,
                timed_out: false,
                duration_ms: elapsed_ms(started),
                stdout_tail: String::new(),
                stderr_tail: format!("failed to spawn `sh -c` in {}: {e}", dir.display()),
            };
        }
    };

    let stdout = child.stdout.take().map(spawn_reader);
    let stderr = child.stderr.take().map(spawn_reader);

    let (exit_code, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => (status.code(), false),
        Ok(Err(e)) => {
            warn!(cmd, error = %e, "waiting for acceptance criterion failed");
            (None, false)
        }
        Err(_) => {
            kill_process_group(&mut child).await;
            (None, true)
        }
    };

    // Readers finish once every holder of the pipe is gone; bound the wait in
    // case a descendant escaped the process group.
    let stdout = collect(stdout).await;
    let stderr = collect(stderr).await;

    CommandResult {
        cmd: cmd.to_string(),
        exit_code,
        timed_out,
        duration_ms: elapsed_ms(started),
        stdout_tail: tail(&stdout, tail_bytes),
        stderr_tail: tail(&stderr, tail_bytes),
    }
}

fn spawn_reader<R>(mut pipe: R) -> tokio::task::JoinHandle<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf).await;
        buf
    })
}

async fn collect(reader: Option<tokio::task::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    let Some(handle) = reader else {
        return Vec::new();
    };
    match tokio::time::timeout(Duration::from_secs(5), handle).await {
        Ok(Ok(buf)) => buf,
        _ => Vec::new(),
    }
}

async fn kill_process_group(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: killpg only sends a signal; the child was spawned with
        // process_group(0), so its pgid equals its pid.
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Last `max_bytes` of `bytes` as UTF-8 (lossy), cut on a char boundary.
pub fn tail(bytes: &[u8], max_bytes: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= max_bytes {
        return text.into_owned();
    }
    let mut start = text.len() - max_bytes;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_last_bytes_on_char_boundary() {
        assert_eq!(tail(b"hello", 10), "hello");
        assert_eq!(tail(b"hello world", 5), "world");
        // "é" is two bytes; a cut inside it must move forward.
        let s = "aé".as_bytes();
        assert_eq!(tail(s, 1), "");
        assert_eq!(tail(s, 2), "é");
    }

    #[test]
    fn report_serializes_with_schema_and_block_kind() {
        let mut r = MergeGateReport::new("task/x", "main", "/wt");
        r.blocked_by.push(GateBlock::BehindTarget {
            behind: 5,
            max_behind: 2,
        });
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["schema"], MERGE_GATE_SCHEMA);
        assert_eq!(v["passed"], false);
        assert_eq!(v["blocked_by"][0]["kind"], "behind_target");
        assert!(r.summary().contains("5 commits behind"));
    }

    #[test]
    fn config_defaults_disable_behind_check() {
        let c: MergeGateConfig = toml::from_str("").unwrap();
        assert_eq!(c, MergeGateConfig::default());
        assert_eq!(c.max_behind_commits, None);
        let c: MergeGateConfig = toml::from_str("max_behind_commits = 3").unwrap();
        assert_eq!(c.max_behind_commits, Some(3));
        assert_eq!(c.command_timeout_secs, 600);
    }

    #[tokio::test]
    async fn run_criterion_captures_exit_code_and_output() {
        let dir = std::env::temp_dir();
        let r = run_criterion(
            "echo out; echo err >&2; exit 3",
            &dir,
            Duration::from_secs(10),
            1024,
        )
        .await;
        assert_eq!(r.exit_code, Some(3));
        assert!(!r.timed_out);
        assert_eq!(r.stdout_tail, "out\n");
        assert_eq!(r.stderr_tail, "err\n");
        assert!(!r.success());
    }

    #[tokio::test]
    async fn run_criterion_missing_dir_is_failure() {
        let r = run_criterion(
            "true",
            Path::new("/nonexistent/at-merge-gate"),
            Duration::from_secs(5),
            1024,
        )
        .await;
        assert_eq!(r.exit_code, None);
        assert!(r.stderr_tail.contains("failed to spawn"));
    }
}
