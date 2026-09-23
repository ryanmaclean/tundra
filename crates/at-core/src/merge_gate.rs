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
//! callers return verbatim to agents and API clients. The report doubles as
//! an attestation: it records the criteria it ran (`criteria`), the verified
//! worktree commit (`head`) and when it ran (`generated_at`).
//!
//! # Authoring criteria
//!
//! Acceptance criteria are authored explicitly (task create/update API, the
//! MCP `create_bead` tool, the UI task form) and validated with
//! [`validate_criteria`]: at most [`MAX_CRITERIA`] single-line commands of at
//! most [`MAX_CRITERION_BYTES`] bytes each.
//!
//! Name clash: `at_intelligence::spec::AcceptanceCriterion` is prose written
//! by the spec pipeline, not a shell command. It is never copied into
//! `Task::acceptance_criteria`, and neither are GitHub / GitLab / Linear issue
//! bodies: only a human or agent that means "run this command" sets them.
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

/// Maximum number of acceptance criteria on one task.
pub const MAX_CRITERIA: usize = 32;

/// Maximum length of one acceptance criterion, in bytes.
pub const MAX_CRITERION_BYTES: usize = 1024;

/// Validate acceptance criteria before they are stored on a task or bead.
///
/// Rejects more than [`MAX_CRITERIA`] entries, and any entry that is empty or
/// whitespace-only, longer than [`MAX_CRITERION_BYTES`], or contains a control
/// character other than `\t` (NUL and newlines included: one criterion is one
/// command line). The error names the offending index, e.g.
/// `acceptance_criteria[2]: exceeds 1024 bytes`. Shared by the HTTP API and
/// the MCP tools so both reject exactly the same input.
pub fn validate_criteria(criteria: &[String]) -> Result<(), String> {
    if criteria.len() > MAX_CRITERIA {
        return Err(format!(
            "acceptance_criteria: at most {MAX_CRITERIA} entries allowed (got {})",
            criteria.len()
        ));
    }
    for (i, c) in criteria.iter().enumerate() {
        if c.trim().is_empty() {
            return Err(format!("acceptance_criteria[{i}]: must not be empty"));
        }
        if c.len() > MAX_CRITERION_BYTES {
            return Err(format!(
                "acceptance_criteria[{i}]: exceeds {MAX_CRITERION_BYTES} bytes"
            ));
        }
        if c.contains('\0') {
            return Err(format!("acceptance_criteria[{i}]: contains NUL"));
        }
        if c.contains(['\n', '\r']) {
            return Err(format!(
                "acceptance_criteria[{i}]: contains a newline (one command per entry)"
            ));
        }
        if c.chars().any(|ch| ch.is_control() && ch != '\t') {
            return Err(format!(
                "acceptance_criteria[{i}]: contains a control character"
            ));
        }
    }
    Ok(())
}

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
    /// Fixing -> Qa -> Merging iterations a task gets after the gate refuses
    /// it before the task moves to Error (0 = fail on the first refusal).
    /// Used by both the daemon orchestrator and the HTTP execute pipeline.
    pub max_fix_iterations: usize,
}

/// Default for [`MergeGateConfig::max_fix_iterations`].
pub const DEFAULT_MAX_FIX_ITERATIONS: usize = 3;

impl Default for MergeGateConfig {
    fn default() -> Self {
        Self {
            command_timeout_secs: 600,
            max_behind_commits: None,
            output_tail_bytes: 4096,
            max_fix_iterations: DEFAULT_MAX_FIX_ITERATIONS,
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
    /// Every criterion the gate was asked to run, in order (additive in v1;
    /// absent in reports written before it existed).
    #[serde(default)]
    pub criteria: Vec<String>,
    /// Commit checked out in the worktree when the gate ran (`null` when it
    /// could not be read). What passed is what merges only if this matches
    /// the branch tip that was merged.
    #[serde(default)]
    pub head: Option<String>,
    /// When the gate ran (RFC 3339, UTC).
    #[serde(default)]
    pub generated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl MergeGateReport {
    /// An empty, not-yet-passed report for `branch` -> `target`.
    pub fn new(branch: &str, target: &str, worktree: &str) -> Self {
        Self {
            schema: MERGE_GATE_SCHEMA.to_string(),
            passed: false,
            branch: branch.to_string(),
            target: target.to_string(),
            worktree: worktree.to_string(),
            blocked_by: Vec::new(),
            results: Vec::new(),
            criteria: Vec::new(),
            head: None,
            generated_at: Some(chrono::Utc::now()),
        }
    }

    /// Instructions for a fixing agent after the gate refused the branch:
    /// the summary, every blocking precondition, and the first failing
    /// command with its output tails. Shared by the daemon orchestrator and
    /// the HTTP execute pipeline so both prompt the same way.
    pub fn fix_prompt(&self) -> String {
        let mut prompt = format!("The merge gate refused this branch: {}", self.summary());
        for block in &self.blocked_by {
            prompt.push_str(&format!("\n- {}", block.describe()));
        }
        if let Some(failed) = self.results.iter().find(|r| !r.success()) {
            prompt.push_str(&format!(
                "\nFailing acceptance criterion: `{}`\nstdout (tail):\n{}\nstderr (tail):\n{}",
                failed.cmd, failed.stdout_tail, failed.stderr_tail
            ));
        }
        prompt.push_str("\nFix the problem and commit the changes on the task branch.");
        prompt
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
    /// A block kind this build does not know (written by a newer version).
    /// Treated as blocking: an unrecognised refusal never reads as a pass.
    #[serde(other)]
    Unknown,
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
            GateBlock::Unknown => "unknown precondition (treated as blocking)".to_string(),
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

    /// Commit id checked out in `dir`, recorded in the report as `head`.
    /// Backends that cannot answer cheaply keep the default (`Err`), which
    /// leaves `head` unset without failing the gate.
    fn head(&self, _dir: &str) -> Result<String, String> {
        Err("head not supported by this backend".to_string())
    }
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

    fn head(&self, dir: &str) -> Result<String, String> {
        let out = self.run(dir, &["rev-parse", "HEAD"])?;
        let sha = out.trim();
        if sha.is_empty() {
            Err("empty `git rev-parse HEAD` output".to_string())
        } else {
            Ok(sha.to_string())
        }
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
        report.criteria = criteria.to_vec();
        report.head = self.vcs.head(t.worktree_dir).ok();

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

/// Environment variables passed through to `sh -c <criterion>`. Everything
/// else -- API keys (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, ...), tokens and
/// any other secret the daemon process holds -- is cleared, because
/// `stdout_tail`/`stderr_tail` are broadcast over the WebSocket, stored on
/// the task, and can end up in a PR body.
const CRITERION_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "SHELL",
];

/// Run one acceptance criterion as `sh -c <cmd>` in `dir`.
///
/// On timeout the whole process group is killed (so `sh -c 'sleep 999 & wait'`
/// cannot outlive the gate) and whatever output was produced is kept. The
/// child's environment is cleared to [`CRITERION_ENV_ALLOWLIST`] so secrets
/// held by the daemon process (API keys, tokens) can never appear in the
/// command's output.
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
        .env_clear()
        .envs(
            CRITERION_ENV_ALLOWLIST
                .iter()
                .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v))),
        )
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

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn validate_criteria_accepts_normal_commands() {
        assert!(validate_criteria(&[]).is_ok());
        assert!(validate_criteria(&v(&["cargo test -p at-core", "test -f ok\t# tab ok"])).is_ok());
        let max: Vec<String> = (0..MAX_CRITERIA).map(|i| format!("true {i}")).collect();
        assert!(validate_criteria(&max).is_ok());
        assert!(validate_criteria(&["x".repeat(MAX_CRITERION_BYTES)]).is_ok());
    }

    #[test]
    fn validate_criteria_rejects_bad_entries() {
        let err = validate_criteria(&v(&["true", "  "])).unwrap_err();
        assert_eq!(err, "acceptance_criteria[1]: must not be empty");
        assert!(validate_criteria(&v(&[""])).is_err());
        assert!(validate_criteria(&v(&["true\0false"]))
            .unwrap_err()
            .contains("NUL"));
        assert!(validate_criteria(&v(&["true\nrm -rf /"]))
            .unwrap_err()
            .contains("newline"));
        assert!(validate_criteria(&v(&["true\r"])).is_err());
        assert!(validate_criteria(&v(&["echo \u{1b}[31m"]))
            .unwrap_err()
            .contains("control"));
        let err = validate_criteria(&[
            "ok".to_string(),
            "ok".to_string(),
            "x".repeat(MAX_CRITERION_BYTES + 1),
        ])
        .unwrap_err();
        assert_eq!(err, "acceptance_criteria[2]: exceeds 1024 bytes");
        let too_many: Vec<String> = (0..=MAX_CRITERIA).map(|_| "true".to_string()).collect();
        assert!(validate_criteria(&too_many)
            .unwrap_err()
            .contains("at most 32"));
    }

    #[test]
    fn fix_prompt_names_failing_command_and_stderr() {
        let mut r = MergeGateReport::new("task/x", "main", "/wt");
        r.results.push(CommandResult {
            cmd: "cargo test".into(),
            exit_code: Some(101),
            timed_out: false,
            duration_ms: 5,
            stdout_tail: "running 3 tests".into(),
            stderr_tail: "thread panicked at src/lib.rs".into(),
        });
        let p = r.fix_prompt();
        assert!(p.starts_with("The merge gate refused this branch: merge gate refused: `cargo test` exited 101"), "{p}");
        assert!(p.contains("Failing acceptance criterion: `cargo test`"), "{p}");
        assert!(p.contains("thread panicked at src/lib.rs"), "{p}");
        assert!(p.ends_with("commit the changes on the task branch."), "{p}");

        let mut blocked = MergeGateReport::new("task/x", "main", "/wt");
        blocked.blocked_by.push(GateBlock::VcsError {
            message: "boom".into(),
        });
        assert!(blocked.fix_prompt().contains("\n- vcs error: boom"));
    }

    #[test]
    fn unknown_block_kind_deserializes_as_blocking() {
        let b: GateBlock = serde_json::from_str(r#"{"kind":"from_the_future","x":1}"#).unwrap();
        assert_eq!(b, GateBlock::Unknown);
        assert!(b.describe().contains("blocking"));
    }

    #[test]
    fn report_without_attestation_fields_still_deserializes() {
        let old = serde_json::json!({
            "schema": MERGE_GATE_SCHEMA, "passed": true, "branch": "b",
            "target": "main", "worktree": "/w", "blocked_by": [], "results": []
        });
        let r: MergeGateReport = serde_json::from_value(old).unwrap();
        assert!(r.criteria.is_empty());
        assert_eq!(r.head, None);
        assert_eq!(r.generated_at, None);
    }

    #[test]
    fn config_without_max_fix_iterations_defaults_to_three() {
        let c: MergeGateConfig = toml::from_str("command_timeout_secs = 5").unwrap();
        assert_eq!(c.max_fix_iterations, 3);
        let c: MergeGateConfig = toml::from_str("max_fix_iterations = 1").unwrap();
        assert_eq!(c.max_fix_iterations, 1);
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

    /// Secrets held by the daemon process must never leak into acceptance
    /// criteria output: it is broadcast over the WebSocket, stored on the
    /// task and can land in a PR body.
    #[tokio::test]
    async fn run_criterion_does_not_leak_env_secrets() {
        // SAFETY: single-threaded within this test's tokio runtime; no other
        // test reads this variable.
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "sk-super-secret-value");
        }
        let dir = std::env::temp_dir();
        let r = run_criterion(
            "echo \"key=[$ANTHROPIC_API_KEY]\"",
            &dir,
            Duration::from_secs(10),
            1024,
        )
        .await;
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }
        assert_eq!(r.exit_code, Some(0));
        assert_eq!(r.stdout_tail, "key=[]\n");
        assert!(!r.stdout_tail.contains("sk-super-secret-value"));
    }

    #[tokio::test]
    async fn run_criterion_keeps_allowlisted_path() {
        let dir = std::env::temp_dir();
        // `sh` itself must still be resolvable via PATH after env_clear.
        let r = run_criterion("echo -n \"$PATH\" | wc -c", &dir, Duration::from_secs(10), 64)
            .await;
        assert_eq!(r.exit_code, Some(0));
        assert_ne!(r.stdout_tail.trim(), "0", "PATH must survive env_clear");
    }
}
