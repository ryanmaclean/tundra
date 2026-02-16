use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

use crate::cache::CacheDb;
use crate::types::Task;
use crate::worktree::{WorktreeInfo, WorktreeError};

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
}

impl WorktreeManager {
    /// Create a new WorktreeManager with the real git runner.
    pub fn new(base_dir: impl Into<PathBuf>, cache: Arc<CacheDb>) -> Self {
        Self {
            base_dir: base_dir.into(),
            cache,
            git: Box::new(RealGitRunner),
        }
    }

    /// Create a new WorktreeManager with a custom git runner (for testing).
    pub fn with_git_runner(
        base_dir: impl Into<PathBuf>,
        cache: Arc<CacheDb>,
        git: Box<dyn GitRunner>,
    ) -> Self {
        Self {
            base_dir: base_dir.into(),
            cache,
            git,
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
        let _ = self.git.run_git(base_dir_str, &["fetch", "origin"]);

        // 2. Check if there are changes to merge
        let diff_result = self.git.run_git(
            base_dir_str,
            &["diff", "--stat", "main", &worktree.branch],
        );

        match diff_result {
            Ok(output) if output.stdout.trim().is_empty() => {
                info!(branch = %worktree.branch, "nothing to merge");
                return Ok(MergeResult::NothingToMerge);
            }
            Ok(_) => { /* has changes, continue */ }
            Err(e) => return Err(WorktreeManagerError::GitCommand(e)),
        }

        // 3. Attempt merge (using --no-commit first to check)
        let merge_result = self.git.run_git(
            base_dir_str,
            &[
                "merge",
                "--no-ff",
                "--no-commit",
                &worktree.branch,
            ],
        );

        match merge_result {
            Ok(output) if output.success => {
                // Commit the merge
                let commit_msg = format!("Merge branch '{}' into main", worktree.branch);
                let commit_result =
                    self.git
                        .run_git(base_dir_str, &["commit", "-m", &commit_msg]);

                match commit_result {
                    Ok(co) if co.success => {
                        // 5. Clean up worktree
                        let wt_path = &worktree.path;
                        let _ = self.git.run_git(
                            base_dir_str,
                            &["worktree", "remove", "--force", wt_path],
                        );
                        let _ = self.git.run_git(
                            base_dir_str,
                            &["branch", "-d", &worktree.branch],
                        );

                        info!(branch = %worktree.branch, "merge successful");
                        Ok(MergeResult::Success)
                    }
                    Ok(co) => Err(WorktreeManagerError::GitCommand(co.stderr)),
                    Err(e) => Err(WorktreeManagerError::GitCommand(e)),
                }
            }
            Ok(output) => {
                // 4. Detect conflicts
                let conflict_result = self.git.run_git(
                    base_dir_str,
                    &["diff", "--name-only", "--diff-filter=U"],
                );

                // Abort the merge
                let _ = self.git.run_git(base_dir_str, &["merge", "--abort"]);

                let conflicts = match conflict_result {
                    Ok(co) => co
                        .stdout
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(|l| l.to_string())
                        .collect(),
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
    async fn worktree_path_returns_correct_location() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
        let manager = WorktreeManager::new("/project", cache);
        let task = make_test_task();

        let path = manager.worktree_path(&task);
        assert_eq!(path, PathBuf::from("/project/.worktrees/test-feature"));
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

    #[test]
    fn sanitize_name_works() {
        assert_eq!(sanitize_name("My Cool Feature!"), "my-cool-feature-");
        assert_eq!(sanitize_name("fix/bug #42"), "fix-bug--42");
        assert_eq!(sanitize_name("simple"), "simple");
    }

    #[tokio::test]
    async fn cleanup_stale_with_no_worktrees_dir() {
        let cache = Arc::new(CacheDb::new_in_memory().await.unwrap());
        let git = Box::new(MockGitRunner::new(vec![]));
        let manager =
            WorktreeManager::with_git_runner("/nonexistent/path/xyz", cache, git);

        let result = manager
            .cleanup_stale(Duration::from_secs(3600))
            .await
            .unwrap();
        assert!(result.is_empty());
    }
}
