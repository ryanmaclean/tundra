use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

use crate::cache::CacheDb;
use crate::git_read_adapter::{default_read_adapter, GitReadAdapter};
use crate::repo::RepoPath;
use crate::types::Task;
use crate::worktree::{WorktreeError, WorktreeInfo};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum WorktreeManagerError {
    #[error("worktree error: {0}")]
    Worktree(#[from] WorktreeError),
    #[error("git command failed: {0}")]
    GitCommand(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("worktree not found for task: {0}")]
    NotFound(String),
}

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
    #[allow(dead_code)]
    cache: Arc<CacheDb>,
    git: Box<dyn GitRunner>,
    git_read: Box<dyn GitReadAdapter>,
}

impl WorktreeManager {
    /// Create a new WorktreeManager with the real git runner.
    ///
    /// Uses the best available read adapter: `Git2ReadAdapter` when the
    /// `libgit2` feature is enabled, otherwise `ShellGitReadAdapter`.
    pub fn new(base_dir: impl Into<PathBuf>, cache: Arc<CacheDb>) -> Self {
        Self {
            base_dir: base_dir.into(),
            cache,
            git: Box::new(RealGitRunner),
            git_read: default_read_adapter(),
        }
    }

    /// Create a new WorktreeManager with a custom git runner (for testing).
    ///
    /// Still uses the best available read adapter automatically.
    pub fn with_git_runner(
        base_dir: impl Into<PathBuf>,
        cache: Arc<CacheDb>,
        git: Box<dyn GitRunner>,
    ) -> Self {
        Self {
            base_dir: base_dir.into(),
            cache,
            git,
            git_read: default_read_adapter(),
        }
    }

    /// Create a manager with fully custom adapters.
    ///
    /// Intended for staged migration/testing where read-path git calls can use
    /// a different implementation (`git2-rs`, mock adapters, etc.) while
    /// write-paths remain on the existing `GitRunner`.
    pub fn with_adapters(
        base_dir: impl Into<PathBuf>,
        cache: Arc<CacheDb>,
        git: Box<dyn GitRunner>,
        git_read: Box<dyn GitReadAdapter>,
    ) -> Self {
        Self {
            base_dir: base_dir.into(),
            cache,
            git,
            git_read,
        }
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
        std::fs::create_dir_all(parent)?;

        // Check if already exists
        if wt_path.exists() {
            return Err(WorktreeManagerError::Worktree(
                WorktreeError::AlreadyExists(wt_path.display().to_string()),
            ));
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

        if !worktrees_dir.exists() {
            return Ok(removed);
        }

        let entries = std::fs::read_dir(&worktrees_dir)?;
        let cutoff = std::time::SystemTime::now()
            .checked_sub(max_age)
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // Check modification time
            let metadata = std::fs::metadata(&path)?;
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

    /// Attempt to merge a worktree branch back to main.
    ///
    /// The merge flow:
    /// 1. Fetch latest main
    /// 2. Check if there are any changes to merge
    /// 3. Attempt the merge
    /// 4. Detect conflicts
    /// 5. Clean up worktree on success
    pub async fn merge_to_main(&self, worktree: &WorktreeInfo) -> Result<MergeResult> {
        let base_dir_str = self.base_dir.to_str().unwrap_or(".");

        info!(
            branch = %worktree.branch,
            "attempting merge to main"
        );

        // 1. Fetch latest
        if let Err(e) = self.git.run_git(base_dir_str, &["fetch", "origin"]) {
            warn!(error = %e, "git fetch failed, proceeding with local state");
        }

        // 2. Check if there are changes to merge
        let diff_stdout = match self
            .git_read
            .diff_stat(base_dir_str, "main", &worktree.branch)
        {
            Ok(stdout) => stdout,
            Err(e) => {
                warn!(
                    error = %e,
                    branch = %worktree.branch,
                    "git read adapter failed for diff --stat; falling back to GitRunner"
                );
                match self
                    .git
                    .run_git(base_dir_str, &["diff", "--stat", "main", &worktree.branch])
                {
                    Ok(output) => output.stdout,
                    Err(err) => return Err(WorktreeManagerError::GitCommand(err)),
                }
            }
        };

        match diff_stdout.trim() {
            "" => {
                info!(branch = %worktree.branch, "nothing to merge");
                return Ok(MergeResult::NothingToMerge);
            }
            _ => { /* has changes, continue */ }
        }

        // 3. Attempt merge (using --no-commit first to check)
        let merge_result = self.git.run_git(
            base_dir_str,
            &["merge", "--no-ff", "--no-commit", &worktree.branch],
        );

        match merge_result {
            Ok(output) if output.success => {
                // Commit the merge
                let commit_msg = format!("Merge branch '{}' into main", worktree.branch);
                let commit_result = self
                    .git
                    .run_git(base_dir_str, &["commit", "-m", &commit_msg]);

                match commit_result {
                    Ok(co) if co.success => {
                        // 5. Clean up worktree
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
                    Ok(co) => Err(WorktreeManagerError::GitCommand(co.stderr)),
                    Err(e) => Err(WorktreeManagerError::GitCommand(e)),
                }
            }
            Ok(output) => {
                // 4. Detect conflicts
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
    use std::sync::Mutex;
    use uuid::Uuid;

    /// A mock git runner that records commands and returns canned responses.
    struct MockGitRunner {
        /// Canned responses: for each call in order, return this.
        responses: Mutex<Vec<GitOutput>>,
        /// Record of all commands that were run.
        commands: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl MockGitRunner {
        fn new(responses: Vec<GitOutput>) -> Self {
            Self {
                responses: Mutex::new(responses),
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
                Ok(responses.remove(0))
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
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
        let tmp = std::env::temp_dir().join("at-wm-test-create");
        // Clean up from previous runs
        let _ = std::fs::remove_dir_all(&tmp);

        let git = Box::new(MockGitRunner::new(vec![GitOutput {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
        }]));

        let manager = WorktreeManager::with_git_runner(tmp.clone(), cache, git);
        let task = make_test_task();

        let result = manager.create_for_task(&task).await.unwrap();
        assert!(result.path.contains(".worktrees"));
        assert!(result.path.contains("test-feature"));
        assert_eq!(result.branch, "task/test-feature");
        assert_eq!(result.base_branch, "main");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn create_for_task_rejects_duplicate() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
        let tmp = std::env::temp_dir().join("at-wm-test-dup");
        let _ = std::fs::remove_dir_all(&tmp);

        // Pre-create the worktree directory to simulate duplicate
        let wt_dir = tmp.join(".worktrees").join("test-feature");
        std::fs::create_dir_all(&wt_dir).unwrap();

        let git = Box::new(MockGitRunner::new(vec![]));
        let manager = WorktreeManager::with_git_runner(tmp.clone(), cache, git);
        let task = make_test_task();

        let result = manager.create_for_task(&task).await;
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn merge_to_main_success() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());

        // Responses: fetch, diff (has changes), merge (success), commit (success),
        // worktree remove, branch delete
        let git = Box::new(MockGitRunner::new(vec![
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // fetch
            GitOutput {
                success: true,
                stdout: "file.rs | 5 ++---\n".to_string(),
                stderr: String::new(),
            }, // diff
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // merge
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // commit
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // worktree remove
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // branch delete
        ]));

        let manager = WorktreeManager::with_git_runner("/project", cache, git);
        let wt = WorktreeInfo {
            path: "/project/.worktrees/test".to_string(),
            branch: "task/test".to_string(),
            base_branch: "main".to_string(),
            task_name: "test".to_string(),
            created_at: Utc::now(),
        };

        let result = manager.merge_to_main(&wt).await.unwrap();
        assert_eq!(result, MergeResult::Success);
    }

    #[tokio::test]
    async fn merge_to_main_nothing_to_merge() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());

        let git = Box::new(MockGitRunner::new(vec![
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // fetch
            GitOutput {
                success: true,
                stdout: "".to_string(), // no diff
                stderr: String::new(),
            },
        ]));

        let manager = WorktreeManager::with_git_runner("/project", cache, git);
        let wt = WorktreeInfo {
            path: "/project/.worktrees/test".to_string(),
            branch: "task/test".to_string(),
            base_branch: "main".to_string(),
            task_name: "test".to_string(),
            created_at: Utc::now(),
        };

        let result = manager.merge_to_main(&wt).await.unwrap();
        assert_eq!(result, MergeResult::NothingToMerge);
    }

    #[tokio::test]
    async fn merge_to_main_conflict() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());

        let git = Box::new(MockGitRunner::new(vec![
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // fetch
            GitOutput {
                success: true,
                stdout: "file.rs | 5 ++---\n".to_string(),
                stderr: String::new(),
            }, // diff (has changes)
            GitOutput {
                success: false,
                stdout: String::new(),
                stderr: "CONFLICT (content): Merge conflict in file.rs\n".to_string(),
            }, // merge fails
            GitOutput {
                success: true,
                stdout: "file.rs\n".to_string(),
                stderr: String::new(),
            }, // diff --name-only
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // merge --abort
        ]));

        let manager = WorktreeManager::with_git_runner("/project", cache, git);
        let wt = WorktreeInfo {
            path: "/project/.worktrees/test".to_string(),
            branch: "task/test".to_string(),
            base_branch: "main".to_string(),
            task_name: "test".to_string(),
            created_at: Utc::now(),
        };

        let result = manager.merge_to_main(&wt).await.unwrap();
        match result {
            MergeResult::Conflict(files) => {
                assert!(!files.is_empty());
                assert!(files[0].contains("file.rs"));
            }
            other => panic!("Expected Conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn repo_path_for_worktree_sets_correct_paths() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
        let manager = WorktreeManager::new("/project", cache);

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
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
        let manager = WorktreeManager::new("/project", cache);
        let rp = manager.repo_path();
        assert_eq!(rp.gitdir(), std::path::Path::new("/project/.git"));
        assert_eq!(rp.workdir(), std::path::Path::new("/project"));
        assert!(!rp.is_worktree());
    }

    #[tokio::test]
    async fn cleanup_stale_with_no_worktrees_dir() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
        let git = Box::new(MockGitRunner::new(vec![]));
        let manager = WorktreeManager::with_git_runner("/nonexistent/path/xyz", cache, git);

        let result = manager
            .cleanup_stale(Duration::from_secs(3600))
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn merge_uses_git_read_adapter_for_diff_check() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
        let shared = Arc::new(MockGitRunner::new(vec![GitOutput {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
        }])); // fetch only

        let manager = WorktreeManager::with_adapters(
            "/project",
            cache,
            Box::new(SharedMockGitRunner(shared.clone())),
            Box::new(MockReadAdapter {
                diff_result: Ok(String::new()), // no changes
                conflict_result: Ok(Vec::new()),
            }),
        );

        let wt = WorktreeInfo {
            path: "/project/.worktrees/test".to_string(),
            branch: "task/test".to_string(),
            base_branch: "main".to_string(),
            task_name: "test".to_string(),
            created_at: Utc::now(),
        };

        let result = manager.merge_to_main(&wt).await.unwrap();
        assert_eq!(result, MergeResult::NothingToMerge);

        let commands = shared.commands();
        // Only fetch should be executed by GitRunner (diff came from read adapter).
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].1,
            vec!["fetch".to_string(), "origin".to_string()]
        );
    }

    #[tokio::test]
    async fn merge_uses_git_read_adapter_for_conflict_detection() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
        let shared = Arc::new(MockGitRunner::new(vec![
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // fetch
            GitOutput {
                success: false,
                stdout: String::new(),
                stderr: "CONFLICT (content): Merge conflict in file.rs\n".to_string(),
            }, // merge fails
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // merge --abort
        ]));

        let manager = WorktreeManager::with_adapters(
            "/project",
            cache,
            Box::new(SharedMockGitRunner(shared.clone())),
            Box::new(MockReadAdapter {
                diff_result: Ok("file.rs | 5 ++---\n".to_string()),
                conflict_result: Ok(vec!["file.rs".to_string()]),
            }),
        );

        let wt = WorktreeInfo {
            path: "/project/.worktrees/test".to_string(),
            branch: "task/test".to_string(),
            base_branch: "main".to_string(),
            task_name: "test".to_string(),
            created_at: Utc::now(),
        };

        let result = manager.merge_to_main(&wt).await.unwrap();
        assert_eq!(result, MergeResult::Conflict(vec!["file.rs".to_string()]));

        let commands = shared.commands();
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[0].1,
            vec!["fetch".to_string(), "origin".to_string()]
        );
        assert_eq!(
            commands[1].1,
            vec![
                "merge".to_string(),
                "--no-ff".to_string(),
                "--no-commit".to_string(),
                "task/test".to_string()
            ]
        );
        assert_eq!(
            commands[2].1,
            vec!["merge".to_string(), "--abort".to_string()]
        );
    }

    #[tokio::test]
    async fn merge_falls_back_to_git_runner_when_read_adapter_fails() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
        let shared = Arc::new(MockGitRunner::new(vec![
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // fetch
            GitOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }, // fallback diff --stat (empty)
        ]));

        let manager = WorktreeManager::with_adapters(
            "/project",
            cache,
            Box::new(SharedMockGitRunner(shared.clone())),
            Box::new(MockReadAdapter {
                diff_result: Err("adapter failed".to_string()),
                conflict_result: Ok(Vec::new()),
            }),
        );

        let wt = WorktreeInfo {
            path: "/project/.worktrees/test".to_string(),
            branch: "task/test".to_string(),
            base_branch: "main".to_string(),
            task_name: "test".to_string(),
            created_at: Utc::now(),
        };

        let result = manager.merge_to_main(&wt).await.unwrap();
        assert_eq!(result, MergeResult::NothingToMerge);

        let commands = shared.commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].1,
            vec!["fetch".to_string(), "origin".to_string()]
        );
        assert_eq!(
            commands[1].1,
            vec![
                "diff".to_string(),
                "--stat".to_string(),
                "main".to_string(),
                "task/test".to_string()
            ]
        );
    }
}
