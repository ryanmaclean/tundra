use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

use crate::git_read_adapter::{default_read_adapter, GitReadAdapter};
use crate::merge_gate::{GateTarget, GitGateVcs, MergeGate, MergeGateConfig, MergeGateReport};
use crate::repo::RepoPath;
use crate::types::Task;
use crate::worktree::{WorktreeError, WorktreeInfo};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur in high-level worktree management operations.
///
/// This enum wraps lower-level [`WorktreeError`]s and adds task-specific
/// error conditions for operations like creating worktrees for tasks,
/// merging branches, and cleaning up stale worktrees.
#[derive(Debug, Error)]
pub enum WorktreeManagerError {
    /// An error occurred in the underlying worktree operations.
    ///
    /// This wraps errors from the lower-level `WorktreeManager` implementation,
    /// such as:
    /// - Worktree creation failures
    /// - Worktree deletion failures
    /// - Git worktree command errors
    #[error("worktree error: {0}")]
    Worktree(#[from] WorktreeError),

    /// A git command executed by the manager failed.
    ///
    /// This typically occurs when:
    /// - Git binary is not installed or not in PATH
    /// - Git merge/rebase operations fail
    /// - Branch operations encounter errors
    /// - The git command returned a non-zero exit code
    #[error("git command failed: {0}")]
    GitCommand(String),

    /// Failed to read from or write to the filesystem during manager operations.
    ///
    /// This typically occurs when:
    /// - Worktree directory is inaccessible
    /// - Insufficient file permissions
    /// - Disk I/O errors
    /// - Failed to create directories
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// No worktree exists for the specified task.
    ///
    /// This occurs when:
    /// - Attempting to operate on a worktree that hasn't been created
    /// - The task ID doesn't map to any existing worktree
    /// - The worktree was manually deleted
    ///
    /// The task name or ID is included in the error message.
    #[error("worktree not found for task: {0}")]
    NotFound(String),
}

/// Result type alias for worktree manager operations.
///
/// Equivalent to `std::result::Result<T, WorktreeManagerError>`. Used
/// throughout the high-level worktree management API for consistent error handling.
pub type Result<T> = std::result::Result<T, WorktreeManagerError>;

// ---------------------------------------------------------------------------
// MergeResult
// ---------------------------------------------------------------------------

/// Outcome of attempting to merge a worktree branch back to main.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeResult {
    /// Merge completed successfully.
    Success,
    /// Merge has conflicts in the listed files.
    Conflict(Vec<String>),
    /// The branch has no changes relative to main.
    NothingToMerge,
}

/// Outcome of [`WorktreeManager::merge_to_main_gated`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GatedMerge {
    /// The merge gate failed; nothing was merged.
    Refused { report: MergeGateReport },
    /// The gate passed and the merge was attempted with this result.
    Attempted {
        report: MergeGateReport,
        result: MergeResult,
    },
}

impl GatedMerge {
    pub fn report(&self) -> &MergeGateReport {
        match self {
            GatedMerge::Refused { report } | GatedMerge::Attempted { report, .. } => report,
        }
    }
}

// ---------------------------------------------------------------------------
// GitRunner trait (for testability)
// ---------------------------------------------------------------------------

/// Abstraction over git CLI operations so they can be mocked in tests.
pub trait GitRunner: Send + Sync {
    /// Run a git command in the given directory and return (success, stdout, stderr).
    fn run_git(&self, dir: &str, args: &[&str]) -> std::result::Result<GitOutput, String>;
}

#[derive(Debug, Clone)]
pub struct GitOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Real git runner that shells out to the `git` binary.
pub struct RealGitRunner;

impl GitRunner for RealGitRunner {
    fn run_git(&self, dir: &str, args: &[&str]) -> std::result::Result<GitOutput, String> {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .map_err(|e| e.to_string())?;

        Ok(GitOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// WorktreeManager
// ---------------------------------------------------------------------------

/// High-level manager for git worktrees used in task execution.
///
/// Builds on the lower-level `at_core::worktree` module to provide
/// task-oriented operations: creating worktrees for tasks, cleaning up
/// stale ones, and merging completed work back to the main branch.
pub struct WorktreeManager {
    base_dir: PathBuf,
    git: Box<dyn GitRunner>,
    git_read: Box<dyn GitReadAdapter>,
    gate_config: MergeGateConfig,
}

impl WorktreeManager {
    /// Create a new WorktreeManager with the real git runner.
    ///
    /// Uses the best available read adapter: `Git2ReadAdapter` when the
    /// `libgit2` feature is enabled, otherwise `ShellGitReadAdapter`.
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            git: Box::new(RealGitRunner),
            git_read: default_read_adapter(),
            gate_config: MergeGateConfig::default(),
        }
    }

    /// Create a new WorktreeManager with a custom git runner (for testing).
    ///
    /// Still uses the best available read adapter automatically.
    pub fn with_git_runner(base_dir: impl Into<PathBuf>, git: Box<dyn GitRunner>) -> Self {
        Self {
            base_dir: base_dir.into(),
            git,
            git_read: default_read_adapter(),
            gate_config: MergeGateConfig::default(),
        }
    }

    /// Create a manager with fully custom adapters.
    ///
    /// Intended for staged migration/testing where read-path git calls can use
    /// a different implementation (`git2-rs`, mock adapters, etc.) while
    /// write-paths remain on the existing `GitRunner`.
    pub fn with_adapters(
        base_dir: impl Into<PathBuf>,
        git: Box<dyn GitRunner>,
        git_read: Box<dyn GitReadAdapter>,
    ) -> Self {
        Self {
            base_dir: base_dir.into(),
            git,
            git_read,
            gate_config: MergeGateConfig::default(),
        }
    }

    /// Replace the merge-gate settings used by [`merge_to_main_gated`](Self::merge_to_main_gated).
    pub fn with_merge_gate_config(mut self, config: MergeGateConfig) -> Self {
        self.gate_config = config;
        self
    }

    /// Merge-gate settings in effect.
    pub fn merge_gate_config(&self) -> &MergeGateConfig {
        &self.gate_config
    }

    /// Run the merge gate for `worktree`, and merge it into its base branch
    /// only if the gate passes.
    ///
    /// `acceptance_criteria` are shell commands (normally
    /// [`Task::acceptance_criteria`]) run in the worktree; see
    /// [`crate::merge_gate`] for the full set of checks. This is the entry
    /// point every caller that merges task work should use.
    pub async fn merge_to_main_gated(
        &self,
        worktree: &WorktreeInfo,
        acceptance_criteria: &[String],
    ) -> Result<GatedMerge> {
        let report = self.verify_gate(worktree, acceptance_criteria).await;
        if !report.passed {
            return Ok(GatedMerge::Refused { report });
        }
        let result = self.merge_to_main(worktree).await?;
        Ok(GatedMerge::Attempted { report, result })
    }

    /// Run the merge gate for `worktree` without merging anything
    /// ("verify" mode). Same checks and report as
    /// [`merge_to_main_gated`](Self::merge_to_main_gated).
    pub async fn verify_gate(
        &self,
        worktree: &WorktreeInfo,
        acceptance_criteria: &[String],
    ) -> MergeGateReport {
        let base_dir_str = self.base_dir.to_str().unwrap_or(".");
        let vcs = GitGateVcs::new(self.git.as_ref());
        let gate = MergeGate::new(&self.gate_config, &vcs);
        gate.evaluate(
            GateTarget {
                repo_dir: base_dir_str,
                worktree_dir: &worktree.path,
                branch: &worktree.branch,
                target: merge_target(worktree),
            },
            acceptance_criteria,
        )
        .await
    }

    /// Commit currently checked out in `worktree` (`git rev-parse HEAD`).
    pub fn worktree_head(&self, worktree: &WorktreeInfo) -> Result<String> {
        self.git_ok(&worktree.path, &["rev-parse", "HEAD"])
            .map(|s| s.trim().to_string())
    }

    /// Create a worktree for a task.
    ///
    /// The worktree is placed at `{base_dir}/.worktrees/{sanitized-title}/`
    /// with a branch named `task/{sanitized-title}` based off `main`.
    pub async fn create_for_task(&self, task: &Task) -> Result<WorktreeInfo> {
        let sanitized = sanitize_name(&task.title);
        let branch_name = format!("task/{sanitized}");
        let wt_path = self.worktree_path_for_name(&sanitized);

        info!(
            task_id = %task.id,
            worktree = %wt_path.display(),
            branch = %branch_name,
            "creating worktree for task"
        );

        // Ensure parent directory exists
        let parent = wt_path.parent().expect(".worktrees parent");
        tokio::fs::create_dir_all(parent).await?;

        // Check if already exists
        match tokio::fs::try_exists(&wt_path).await {
            Ok(true) => {
                return Err(WorktreeManagerError::Worktree(
                    WorktreeError::AlreadyExists(wt_path.display().to_string()),
                ));
            }
            Err(e) => return Err(WorktreeManagerError::Io(e)),
            Ok(false) => {}
        }

        let base_dir_str = self.base_dir.to_str().unwrap_or(".");
        let wt_path_str = wt_path.to_str().unwrap_or(".");

        // git worktree add -b task/xxx <path> main
        let result = self.git.run_git(
            base_dir_str,
            &["worktree", "add", "-b", &branch_name, wt_path_str, "main"],
        );

        match result {
            Ok(output) if output.success => {
                let info = WorktreeInfo {
                    path: wt_path.display().to_string(),
                    branch: branch_name,
                    base_branch: "main".to_string(),
                    task_name: sanitized,
                    created_at: Utc::now(),
                };
                Ok(info)
            }
            Ok(output) => Err(WorktreeManagerError::GitCommand(output.stderr)),
            Err(e) => Err(WorktreeManagerError::GitCommand(e)),
        }
    }

    /// Clean up worktrees that are older than `max_age`.
    ///
    /// Returns the list of paths that were removed.
    pub async fn cleanup_stale(&self, max_age: Duration) -> Result<Vec<PathBuf>> {
        let worktrees_dir = self.base_dir.join(".worktrees");
        let mut removed = Vec::new();

        match tokio::fs::try_exists(&worktrees_dir).await {
            Ok(false) => return Ok(removed),
            Err(e) => return Err(WorktreeManagerError::Io(e)),
            Ok(true) => {}
        }

        let mut read_dir = tokio::fs::read_dir(&worktrees_dir).await?;
        let cutoff = std::time::SystemTime::now()
            .checked_sub(max_age)
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // Check modification time
            let metadata = tokio::fs::metadata(&path).await?;
            let modified = metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            if modified < cutoff {
                let path_str = path.to_str().unwrap_or("");
                let base_dir_str = self.base_dir.to_str().unwrap_or(".");

                info!(path = %path.display(), "removing stale worktree");

                let result = self
                    .git
                    .run_git(base_dir_str, &["worktree", "remove", "--force", path_str]);

                match result {
                    Ok(output) if output.success => {
                        removed.push(path);
                    }
                    Ok(output) => {
                        warn!(
                            path = %path.display(),
                            stderr = %output.stderr,
                            "failed to remove stale worktree"
                        );
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "failed to remove stale worktree");
                    }
                }
            }
        }

        Ok(removed)
    }

    /// Attempt to merge a worktree branch back to its base branch (`main`)
    /// **without** running the merge gate. Callers merging task work should
    /// use [`merge_to_main_gated`](Self::merge_to_main_gated).
    ///
    /// The merge flow:
    /// 1. Fetch latest (best effort)
    /// 2. Count commits unique to the task branch (`rev-list --count
    ///    <base>..<branch>`); zero means [`MergeResult::NothingToMerge`], even
    ///    if the base branch has since advanced
    /// 3. Refuse if `base_dir` has uncommitted tracked changes (they would be
    ///    swept into the merge commit)
    /// 4. Check out the base branch explicitly if something else is checked
    ///    out, so the merge never lands on an unrelated branch
    /// 5. Merge with `--no-ff --no-commit`, then commit; detect conflicts
    /// 6. Clean up the worktree and task branch on success
    /// 7. Restore the previously checked-out branch if step 4 switched
    pub async fn merge_to_main(&self, worktree: &WorktreeInfo) -> Result<MergeResult> {
        let base_dir_str = self.base_dir.to_str().unwrap_or(".");
        let target = merge_target(worktree);

        info!(
            branch = %worktree.branch,
            target = %target,
            "attempting merge to main"
        );

        // 1. Fetch latest
        if let Err(e) = self.git.run_git(base_dir_str, &["fetch", "origin"]) {
            warn!(error = %e, "git fetch failed, proceeding with local state");
        }

        // 2. Commits on the task branch that the target does not have yet.
        let range = format!("{target}..{}", worktree.branch);
        let ahead = self.git_ok(base_dir_str, &["rev-list", "--count", &range])?;
        let ahead: u64 = ahead.trim().parse().map_err(|_| {
            WorktreeManagerError::GitCommand(format!(
                "unexpected `git rev-list --count {range}` output: {ahead:?}"
            ))
        })?;
        if ahead == 0 {
            info!(branch = %worktree.branch, "nothing to merge");
            return Ok(MergeResult::NothingToMerge);
        }

        // 3. Refuse to merge on top of uncommitted tracked changes.
        let dirty = self.git_ok(
            base_dir_str,
            &["status", "--porcelain", "--untracked-files=no"],
        )?;
        if !dirty.trim().is_empty() {
            return Err(WorktreeManagerError::GitCommand(format!(
                "refusing to merge {} into {target}: {} has uncommitted changes",
                worktree.branch,
                self.base_dir.display()
            )));
        }

        // 4. Make sure the target branch is what we merge into.
        let current = self.git_ok(base_dir_str, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let switched = current.trim() != target;
        if switched {
            info!(from = %current.trim(), to = %target, "checking out merge target");
            self.git_ok(base_dir_str, &["checkout", target])?;
        }

        let result = self.merge_into_checked_out(base_dir_str, worktree, target);

        // 7. Restore whatever the developer had checked out.
        if switched {
            match self.git.run_git(base_dir_str, &["checkout", "-"]) {
                Ok(o) if o.success => {}
                Ok(o) => warn!(stderr = %o.stderr, "failed to restore previous branch"),
                Err(e) => warn!(error = %e, "failed to restore previous branch"),
            }
        }

        result
    }

    /// Run a git command and return stdout, turning a spawn failure or a
    /// non-zero exit status into [`WorktreeManagerError::GitCommand`].
    fn git_ok(&self, dir: &str, args: &[&str]) -> Result<String> {
        match self.git.run_git(dir, args) {
            Ok(o) if o.success => Ok(o.stdout),
            Ok(o) => Err(WorktreeManagerError::GitCommand(format!(
                "git {}: {}",
                args.join(" "),
                o.stderr.trim()
            ))),
            Err(e) => Err(WorktreeManagerError::GitCommand(e)),
        }
    }

    /// Steps 5-6 of [`merge_to_main`](Self::merge_to_main): merge the task
    /// branch into the currently checked-out `target`.
    fn merge_into_checked_out(
        &self,
        base_dir_str: &str,
        worktree: &WorktreeInfo,
        target: &str,
    ) -> Result<MergeResult> {
        // 5. Attempt merge (using --no-commit first to check)
        let merge_result = self.git.run_git(
            base_dir_str,
            &["merge", "--no-ff", "--no-commit", &worktree.branch],
        );

        match merge_result {
            Ok(output) if output.success => {
                if output.stdout.contains("Already up to date") {
                    info!(branch = %worktree.branch, "nothing to merge (already up to date)");
                    return Ok(MergeResult::NothingToMerge);
                }

                // Commit the merge
                let commit_msg = format!("Merge branch '{}' into {target}", worktree.branch);
                let commit_result = self
                    .git
                    .run_git(base_dir_str, &["commit", "-m", &commit_msg]);

                match commit_result {
                    Ok(co) if co.success => {
                        // 6. Clean up worktree
                        let wt_path = &worktree.path;
                        if let Err(e) = self
                            .git
                            .run_git(base_dir_str, &["worktree", "remove", "--force", wt_path])
                        {
                            warn!(error = %e, "git worktree cleanup failed");
                        }
                        if let Err(e) = self
                            .git
                            .run_git(base_dir_str, &["branch", "-d", &worktree.branch])
                        {
                            warn!(error = %e, "git branch cleanup failed");
                        }

                        info!(branch = %worktree.branch, "merge successful");
                        Ok(MergeResult::Success)
                    }
                    Ok(co)
                        if co.stdout.contains("nothing to commit")
                            || co.stderr.contains("nothing to commit") =>
                    {
                        info!(branch = %worktree.branch, "nothing to merge (nothing to commit)");
                        Ok(MergeResult::NothingToMerge)
                    }
                    Ok(co) => {
                        // Leave the repo clean rather than mid-merge.
                        let _ = self.git.run_git(base_dir_str, &["merge", "--abort"]);
                        Err(WorktreeManagerError::GitCommand(co.stderr))
                    }
                    Err(e) => Err(WorktreeManagerError::GitCommand(e)),
                }
            }
            Ok(output) => {
                // 5b. Detect conflicts
                let conflict_result = match self.git_read.conflict_files(base_dir_str) {
                    Ok(files) => Ok(files),
                    Err(e) => {
                        warn!(
                            error = %e,
                            branch = %worktree.branch,
                            "git read adapter failed for conflict files; falling back to GitRunner"
                        );
                        match self
                            .git
                            .run_git(base_dir_str, &["diff", "--name-only", "--diff-filter=U"])
                        {
                            Ok(co) => Ok(co
                                .stdout
                                .lines()
                                .filter(|l| !l.is_empty())
                                .map(|l| l.to_string())
                                .collect()),
                            Err(err) => Err(err),
                        }
                    }
                };

                // Abort the merge
                if let Err(e) = self.git.run_git(base_dir_str, &["merge", "--abort"]) {
                    warn!(error = %e, "git merge --abort failed");
                }

                let conflicts = match conflict_result {
                    Ok(files) => files,
                    Err(_) => {
                        // Parse conflicts from the merge stderr
                        output
                            .stderr
                            .lines()
                            .filter(|l| l.contains("CONFLICT"))
                            .map(|l| l.to_string())
                            .collect()
                    }
                };

                warn!(branch = %worktree.branch, conflicts = ?conflicts, "merge conflicts detected");
                Ok(MergeResult::Conflict(conflicts))
            }
            Err(e) => Err(WorktreeManagerError::GitCommand(e)),
        }
    }

    /// Create a `RepoPath` for a worktree, linking the main gitdir to the
    /// worktree's working directory.
    ///
    /// This bridges the gitui-inspired `RepoPath` with the worktree system,
    /// enabling async git ops to target a specific worktree.
    pub fn repo_path_for_worktree(&self, worktree: &WorktreeInfo) -> RepoPath {
        let gitdir = self
            .base_dir
            .join(".git")
            .join("worktrees")
            .join(&worktree.task_name);
        RepoPath::new(gitdir, PathBuf::from(&worktree.path))
    }

    /// Create a `RepoPath` for the main repository (not a worktree).
    pub fn repo_path(&self) -> RepoPath {
        RepoPath::new(self.base_dir.join(".git"), self.base_dir.clone())
    }

    /// Get the filesystem path where a task's worktree would be located.
    pub fn worktree_path(&self, task: &Task) -> PathBuf {
        let sanitized = sanitize_name(&task.title);
        self.worktree_path_for_name(&sanitized)
    }

    /// Internal helper to compute worktree path from a sanitized name.
    fn worktree_path_for_name(&self, sanitized_name: &str) -> PathBuf {
        self.base_dir.join(".worktrees").join(sanitized_name)
    }
}

/// Branch a worktree merges into: its recorded base, or `main`.
fn merge_target(worktree: &WorktreeInfo) -> &str {
    if worktree.base_branch.trim().is_empty() {
        "main"
    } else {
        worktree.base_branch.as_str()
    }
}

/// Sanitize a task name for use as a directory / branch name.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_read_adapter::GitReadError;
    use crate::types::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    /// A mock git runner that records commands and returns canned responses.
    struct MockGitRunner {
        /// Canned responses: for each call in order, return this.
        responses: Mutex<VecDeque<GitOutput>>,
        /// Record of all commands that were run.
        commands: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl MockGitRunner {
        fn new(responses: Vec<GitOutput>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
                commands: Mutex::new(Vec::new()),
            }
        }

        fn commands(&self) -> Vec<(String, Vec<String>)> {
            self.commands.lock().unwrap().clone()
        }
    }

    struct SharedMockGitRunner(Arc<MockGitRunner>);

    impl GitRunner for SharedMockGitRunner {
        fn run_git(&self, dir: &str, args: &[&str]) -> std::result::Result<GitOutput, String> {
            self.0.run_git(dir, args)
        }
    }

    struct MockReadAdapter {
        diff_result: std::result::Result<String, String>,
        conflict_result: std::result::Result<Vec<String>, String>,
    }

    impl crate::git_read_adapter::GitReadAdapter for MockReadAdapter {
        fn current_branch(&self, _repo_dir: &str) -> std::result::Result<String, GitReadError> {
            Ok("main".to_string())
        }

        fn status_porcelain(
            &self,
            _repo_dir: &str,
        ) -> std::result::Result<Vec<String>, GitReadError> {
            Ok(Vec::new())
        }

        fn diff_stat(
            &self,
            _repo_dir: &str,
            _base: &str,
            _head: &str,
        ) -> std::result::Result<String, GitReadError> {
            match &self.diff_result {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(GitReadError::Command(e.clone())),
            }
        }

        fn conflict_files(
            &self,
            _repo_dir: &str,
        ) -> std::result::Result<Vec<String>, GitReadError> {
            match &self.conflict_result {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(GitReadError::Command(e.clone())),
            }
        }
    }

    impl GitRunner for MockGitRunner {
        fn run_git(&self, dir: &str, args: &[&str]) -> std::result::Result<GitOutput, String> {
            self.commands.lock().unwrap().push((
                dir.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
            ));

            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                Ok(GitOutput {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            } else {
                Ok(responses.pop_front().unwrap())
            }
        }
    }

    fn make_test_task() -> Task {
        Task::new(
            "Test Feature",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        )
    }

    #[tokio::test]
    async fn create_for_task_builds_correct_path() {
        let tmp = std::env::temp_dir().join("at-wm-test-create");
        // Clean up from previous runs
        let _ = tokio::fs::remove_dir_all(&tmp).await;

        let git = Box::new(MockGitRunner::new(vec![GitOutput {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
        }]));

        let manager = WorktreeManager::with_git_runner(tmp.clone(), git);
        let task = make_test_task();

        let result = manager.create_for_task(&task).await.unwrap();
        assert!(result.path.contains(".worktrees"));
        assert!(result.path.contains("test-feature"));
        assert_eq!(result.branch, "task/test-feature");
        assert_eq!(result.base_branch, "main");

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn create_for_task_rejects_duplicate() {
        let tmp = std::env::temp_dir().join("at-wm-test-dup");
        let _ = tokio::fs::remove_dir_all(&tmp).await;

        // Pre-create the worktree directory to simulate duplicate
        let wt_dir = tmp.join(".worktrees").join("test-feature");
        tokio::fs::create_dir_all(&wt_dir).await.unwrap();

        let git = Box::new(MockGitRunner::new(vec![]));
        let manager = WorktreeManager::with_git_runner(tmp.clone(), git);
        let task = make_test_task();

        let result = manager.create_for_task(&task).await;
        assert!(result.is_err());

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn repo_path_for_worktree_sets_correct_paths() {
        let manager = WorktreeManager::new("/project");

        let wt = WorktreeInfo {
            path: "/project/.worktrees/my-task".to_string(),
            branch: "task/my-task".to_string(),
            base_branch: "main".to_string(),
            task_name: "my-task".to_string(),
            created_at: Utc::now(),
        };

        let rp = manager.repo_path_for_worktree(&wt);
        assert_eq!(
            rp.gitdir(),
            std::path::Path::new("/project/.git/worktrees/my-task")
        );
        assert_eq!(
            rp.workdir(),
            std::path::Path::new("/project/.worktrees/my-task")
        );
        assert!(rp.is_worktree());
    }

    #[tokio::test]
    async fn repo_path_main_not_worktree() {
        let manager = WorktreeManager::new("/project");
        let rp = manager.repo_path();
        assert_eq!(rp.gitdir(), std::path::Path::new("/project/.git"));
        assert_eq!(rp.workdir(), std::path::Path::new("/project"));
        assert!(!rp.is_worktree());
    }

    #[tokio::test]
    async fn cleanup_stale_with_no_worktrees_dir() {
        let git = Box::new(MockGitRunner::new(vec![]));
        let manager = WorktreeManager::with_git_runner("/nonexistent/path/xyz", git);

        let result = manager
            .cleanup_stale(Duration::from_secs(3600))
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    fn out(stdout: &str) -> GitOutput {
        GitOutput {
            success: true,
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    fn fail(stderr: &str) -> GitOutput {
        GitOutput {
            success: false,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }

    fn test_wt() -> WorktreeInfo {
        WorktreeInfo {
            path: "/project/.worktrees/test".to_string(),
            branch: "task/test".to_string(),
            base_branch: "main".to_string(),
            task_name: "test".to_string(),
            created_at: Utc::now(),
        }
    }

    fn mock_manager(responses: Vec<GitOutput>) -> (WorktreeManager, Arc<MockGitRunner>) {
        let shared = Arc::new(MockGitRunner::new(responses));
        let manager = WorktreeManager::with_adapters(
            "/project",
            Box::new(SharedMockGitRunner(shared.clone())),
            Box::new(MockReadAdapter {
                diff_result: Err("diff_stat must not decide merges".to_string()),
                conflict_result: Ok(vec!["file.rs".to_string()]),
            }),
        );
        (manager, shared)
    }

    fn args(cmds: &[(String, Vec<String>)]) -> Vec<String> {
        cmds.iter().map(|(_, a)| a.join(" ")).collect()
    }

    #[tokio::test]
    async fn merge_to_main_success_on_main() {
        let (manager, git) = mock_manager(vec![
            out(""),       // fetch
            out("2\n"),    // rev-list --count main..task/test
            out(""),       // status (clean)
            out("main\n"), // rev-parse --abbrev-ref HEAD
            out(""),       // merge
            out(""),       // commit
            out(""),       // worktree remove
            out(""),       // branch -d
        ]);

        let result = manager.merge_to_main(&test_wt()).await.unwrap();
        assert_eq!(result, MergeResult::Success);
        assert_eq!(
            args(&git.commands()),
            vec![
                "fetch origin",
                "rev-list --count main..task/test",
                "status --porcelain --untracked-files=no",
                "rev-parse --abbrev-ref HEAD",
                "merge --no-ff --no-commit task/test",
                "commit -m Merge branch 'task/test' into main",
                "worktree remove --force /project/.worktrees/test",
                "branch -d task/test",
            ]
        );
    }

    #[tokio::test]
    async fn merge_checks_out_main_and_restores_previous_branch() {
        let (manager, git) = mock_manager(vec![
            out(""),              // fetch
            out("1\n"),           // rev-list
            out(""),              // status
            out("feature/foo\n"), // rev-parse: developer is on another branch
            out(""),              // checkout main
            out(""),              // merge
            out(""),              // commit
            out(""),              // worktree remove
            out(""),              // branch -d
            out(""),              // checkout -
        ]);

        let result = manager.merge_to_main(&test_wt()).await.unwrap();
        assert_eq!(result, MergeResult::Success);
        let cmds = args(&git.commands());
        let checkout_main = cmds.iter().position(|c| c == "checkout main").unwrap();
        let merge = cmds
            .iter()
            .position(|c| c.starts_with("merge --no-ff"))
            .unwrap();
        assert!(
            checkout_main < merge,
            "must check out main before merging: {cmds:?}"
        );
        assert_eq!(cmds.last().unwrap(), "checkout -");
    }

    #[tokio::test]
    async fn merge_fails_when_checkout_of_main_fails() {
        let (manager, git) = mock_manager(vec![
            out(""),
            out("1\n"),
            out(""),
            out("feature/foo\n"),
            fail("error: pathspec 'main' did not match"),
        ]);

        let err = manager.merge_to_main(&test_wt()).await.unwrap_err();
        assert!(matches!(err, WorktreeManagerError::GitCommand(_)));
        assert!(!args(&git.commands()).iter().any(|c| c.starts_with("merge")));
    }

    #[tokio::test]
    async fn merge_refuses_dirty_worktree() {
        let (manager, git) = mock_manager(vec![
            out(""),
            out("1\n"),
            out(" M src/lib.rs\n"), // status: tracked change
        ]);

        let err = manager.merge_to_main(&test_wt()).await.unwrap_err();
        assert!(err.to_string().contains("uncommitted changes"), "{err}");
        let cmds = args(&git.commands());
        assert!(!cmds
            .iter()
            .any(|c| c.starts_with("merge") || c.starts_with("checkout")));
    }

    #[tokio::test]
    async fn merge_to_main_nothing_to_merge_when_branch_has_no_commits() {
        // main advanced past the task branch: the tree diff is non-empty but
        // the branch has no commits of its own.
        let (manager, git) = mock_manager(vec![out(""), out("0\n")]);

        let result = manager.merge_to_main(&test_wt()).await.unwrap();
        assert_eq!(result, MergeResult::NothingToMerge);
        assert_eq!(
            git.commands().len(),
            2,
            "no checkout/merge/commit attempted"
        );
    }

    #[tokio::test]
    async fn merge_commit_nothing_to_commit_is_nothing_to_merge() {
        let (manager, _git) = mock_manager(vec![
            out(""),
            out("1\n"),
            out(""),
            out("main\n"),
            out(""), // merge
            GitOutput {
                success: false,
                stdout: "nothing to commit, working tree clean\n".to_string(),
                stderr: String::new(),
            },
        ]);
        let result = manager.merge_to_main(&test_wt()).await.unwrap();
        assert_eq!(result, MergeResult::NothingToMerge);
    }

    #[tokio::test]
    async fn merge_rev_list_failure_is_error() {
        let (manager, _git) = mock_manager(vec![out(""), fail("fatal: bad revision")]);
        assert!(manager.merge_to_main(&test_wt()).await.is_err());
    }

    #[tokio::test]
    async fn merge_to_main_conflict_uses_read_adapter_and_aborts() {
        let (manager, git) = mock_manager(vec![
            out(""),
            out("3\n"),
            out(""),
            out("main\n"),
            fail("CONFLICT (content): Merge conflict in file.rs\n"), // merge
            out(""),                                                 // merge --abort
        ]);

        let result = manager.merge_to_main(&test_wt()).await.unwrap();
        assert_eq!(result, MergeResult::Conflict(vec!["file.rs".to_string()]));
        assert_eq!(args(&git.commands()).last().unwrap(), "merge --abort");
    }

    // -- real git ---------------------------------------------------------

    fn sh_git(dir: &std::path::Path, args: &[&str]) -> String {
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
        sh_git(p, &["add", "f"]);
        sh_git(p, &["commit", "-q", "-m", "base"]);
        dir
    }

    fn real_wt(repo: &std::path::Path, name: &str) -> WorktreeInfo {
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
        WorktreeInfo {
            path: wt_path.to_string_lossy().to_string(),
            branch,
            base_branch: "main".to_string(),
            task_name: name.to_string(),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn real_git_merges_into_main_not_checked_out_branch() {
        let repo = init_repo();
        let p = repo.path();
        let wt = real_wt(p, "x");
        let wt_dir = std::path::PathBuf::from(&wt.path);
        std::fs::write(wt_dir.join("g"), "task work\n").unwrap();
        sh_git(&wt_dir, &["add", "g"]);
        sh_git(&wt_dir, &["commit", "-q", "-m", "task work"]);

        // Developer has an unrelated branch checked out in the project root.
        sh_git(p, &["checkout", "-q", "-b", "feature/foo"]);
        let foo_before = sh_git(p, &["rev-parse", "feature/foo"]);

        let manager = WorktreeManager::new(p);
        let result = manager.merge_to_main(&wt).await.unwrap();
        assert_eq!(result, MergeResult::Success);

        // Task commit landed on main; feature/foo untouched and restored.
        let main_files = sh_git(p, &["ls-tree", "--name-only", "main"]);
        assert!(main_files.lines().any(|l| l == "g"), "{main_files}");
        assert_eq!(sh_git(p, &["rev-parse", "feature/foo"]), foo_before);
        assert_eq!(
            sh_git(p, &["rev-parse", "--abbrev-ref", "HEAD"]),
            "feature/foo"
        );
        let msg = sh_git(p, &["log", "-1", "--format=%s", "main"]);
        assert_eq!(msg, "Merge branch 'task/x' into main");
    }

    #[tokio::test]
    async fn real_git_no_change_task_after_main_advanced_is_nothing_to_merge() {
        let repo = init_repo();
        let p = repo.path();
        let wt = real_wt(p, "y"); // no commits on task/y

        // Another task merged first: main advances.
        std::fs::write(p.join("f"), "changed\n").unwrap();
        sh_git(p, &["commit", "-q", "-am", "main advanced"]);

        let manager = WorktreeManager::new(p);
        let result = manager.merge_to_main(&wt).await.unwrap();
        assert_eq!(result, MergeResult::NothingToMerge);
    }
}
