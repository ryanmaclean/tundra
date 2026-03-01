use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur when managing git worktrees.
///
/// These errors cover worktree creation, deletion, listing, and discovery
/// operations performed by [`WorktreeManager`].
#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    /// A git worktree command returned a non-zero exit code.
    ///
    /// This typically occurs when:
    /// - Git binary is not installed or not in PATH
    /// - The git worktree command encountered an error (stderr is captured)
    /// - Invalid branch or path arguments were provided
    #[error("git command failed: {0}")]
    GitCommand(String),

    /// Attempted to create a worktree that already exists.
    ///
    /// This occurs when:
    /// - A worktree already exists at the target path
    /// - The task name maps to an existing worktree directory
    ///
    /// The worktree path is included in the error message.
    #[error("worktree already exists: {0}")]
    AlreadyExists(String),

    /// The specified worktree path does not exist.
    ///
    /// This typically occurs when:
    /// - Attempting to delete a worktree that has already been removed
    /// - The worktree path was mistyped or is invalid
    /// - The worktree was manually deleted outside of the manager
    #[error("worktree not found: {0}")]
    NotFound(String),

    /// Failed to read from or write to the filesystem during worktree operations.
    ///
    /// This typically occurs when:
    /// - Worktree directory is inaccessible
    /// - Insufficient file permissions
    /// - Disk I/O errors
    /// - Failed to create `.worktrees/` parent directory
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for worktree operations.
///
/// Equivalent to `std::result::Result<T, WorktreeError>`. Used throughout
/// the worktree management API for consistent error handling.
pub type Result<T> = std::result::Result<T, WorktreeError>;

// ---------------------------------------------------------------------------
// WorktreeInfo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub path: String,
    pub branch: String,
    pub base_branch: String,
    pub task_name: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// WorktreeManager
// ---------------------------------------------------------------------------

/// Manages git worktrees for isolated task execution.
///
/// Each task gets its own worktree under `.worktrees/{task-name}/` relative to
/// the project directory. This keeps work isolated while sharing the same git
/// history.
pub struct WorktreeManager;

impl WorktreeManager {
    /// Create a new worktree for the given task.
    ///
    /// The worktree is placed at `{project_dir}/.worktrees/{task_name}/` and
    /// a new branch `task/{task_name}` is created from `base_branch`.
    pub fn create_worktree(
        task_name: &str,
        base_branch: &str,
        project_dir: &str,
    ) -> Result<WorktreeInfo> {
        let sanitized = sanitize_name(task_name);
        let worktree_dir = worktree_path(project_dir, &sanitized);
        let branch_name = format!("task/{sanitized}");

        if worktree_dir.exists() {
            return Err(WorktreeError::AlreadyExists(
                worktree_dir.display().to_string(),
            ));
        }

        // Ensure the .worktrees parent directory exists.
        let parent = worktree_dir.parent().expect(".worktrees parent");
        std::fs::create_dir_all(parent)?;

        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &branch_name,
                worktree_dir.to_str().unwrap(),
                base_branch,
            ])
            .current_dir(project_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitCommand(stderr.to_string()));
        }

        Ok(WorktreeInfo {
            path: worktree_dir.display().to_string(),
            branch: branch_name,
            base_branch: base_branch.to_string(),
            task_name: sanitized,
            created_at: Utc::now(),
        })
    }

    /// Delete an existing worktree by its path.
    pub fn delete_worktree(path: &str, project_dir: &str) -> Result<()> {
        let wt_path = Path::new(path);
        if !wt_path.exists() {
            return Err(WorktreeError::NotFound(path.to_string()));
        }

        let output = Command::new("git")
            .args(["worktree", "remove", "--force", path])
            .current_dir(project_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitCommand(stderr.to_string()));
        }

        Ok(())
    }

    /// List all worktrees managed under `.worktrees/` in the project.
    pub fn list_worktrees(project_dir: &str) -> Result<Vec<WorktreeInfo>> {
        #[cfg(feature = "libgit2")]
        {
            if let Ok(results) = Self::list_worktrees_git2(project_dir) {
                return Ok(results);
            }
        }

        Self::list_worktrees_shell(project_dir)
    }

    fn list_worktrees_shell(project_dir: &str) -> Result<Vec<WorktreeInfo>> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(project_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitCommand(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let worktrees_prefix = format!(
            "{}/.worktrees/",
            Path::new(project_dir)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(project_dir))
                .display()
        );

        let mut results = Vec::new();
        let mut current_path: Option<String> = None;
        let mut current_branch: Option<String> = None;

        for line in stdout.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                current_path = Some(path.to_string());
                current_branch = None;
            } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                current_branch = Some(branch.to_string());
            } else if line.is_empty() {
                if let (Some(ref path), Some(ref branch)) = (&current_path, &current_branch) {
                    if path.contains("/.worktrees/") || path.starts_with(&worktrees_prefix) {
                        let task_name = Path::new(path)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        results.push(WorktreeInfo {
                            path: path.clone(),
                            branch: branch.clone(),
                            base_branch: String::new(), // not available from porcelain output
                            task_name,
                            created_at: Utc::now(), // approximate; git doesn't track this
                        });
                    }
                }
                current_path = None;
                current_branch = None;
            }
        }

        Ok(results)
    }

    #[cfg(feature = "libgit2")]
    fn list_worktrees_git2(project_dir: &str) -> Result<Vec<WorktreeInfo>> {
        let repo = git2::Repository::discover(project_dir)
            .map_err(|e| WorktreeError::GitCommand(e.message().to_string()))?;
        let names = repo
            .worktrees()
            .map_err(|e| WorktreeError::GitCommand(e.message().to_string()))?;

        let project_canon = Path::new(project_dir)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(project_dir));
        let worktrees_prefix = format!("{}/.worktrees/", project_canon.display());

        let mut results = Vec::new();
        for name in names.iter().flatten() {
            let wt = match repo.find_worktree(name) {
                Ok(w) => w,
                Err(_) => continue,
            };
            let path = wt.path().display().to_string();
            if !(path.contains("/.worktrees/") || path.starts_with(&worktrees_prefix)) {
                continue;
            }

            let task_name = Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let branch = crate::git2_ops::Git2ReadOps::current_branch(Path::new(&path))
                .unwrap_or_else(|_| format!("task/{task_name}"));

            results.push(WorktreeInfo {
                path,
                branch,
                base_branch: String::new(),
                task_name,
                created_at: Utc::now(),
            });
        }

        results.sort_by(|a, b| a.task_name.cmp(&b.task_name));
        Ok(results)
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

/// Build the worktree path under `.worktrees/`.
fn worktree_path(project_dir: &str, sanitized_name: &str) -> PathBuf {
    Path::new(project_dir)
        .join(".worktrees")
        .join(sanitized_name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_name_replaces_special_chars() {
        assert_eq!(sanitize_name("My Cool Feature!"), "my-cool-feature-");
        assert_eq!(sanitize_name("fix/bug #42"), "fix-bug--42");
        assert_eq!(sanitize_name("simple"), "simple");
        assert_eq!(sanitize_name("UPPER_case"), "upper_case");
    }

    #[test]
    fn worktree_path_construction() {
        let p = worktree_path("/project", "my-task");
        assert_eq!(p, PathBuf::from("/project/.worktrees/my-task"));
    }

    #[test]
    fn worktree_info_serialization() {
        let info = WorktreeInfo {
            path: "/tmp/.worktrees/test".to_string(),
            branch: "task/test".to_string(),
            base_branch: "main".to_string(),
            task_name: "test".to_string(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let back: WorktreeInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.path, info.path);
        assert_eq!(back.branch, info.branch);
        assert_eq!(back.task_name, info.task_name);
    }

    #[test]
    fn create_worktree_rejects_duplicate_path() {
        // Use a temp directory that already exists to simulate a duplicate
        let tmp = std::env::temp_dir();
        let existing_name = "at-worktree-test-exists";
        let existing_path = tmp.join(".worktrees").join(existing_name);
        std::fs::create_dir_all(&existing_path).ok();

        let result = WorktreeManager::create_worktree(existing_name, "main", tmp.to_str().unwrap());
        assert!(result.is_err());
        if let Err(WorktreeError::AlreadyExists(_)) = result {
            // expected
        } else {
            panic!("Expected AlreadyExists error");
        }

        // cleanup
        std::fs::remove_dir_all(tmp.join(".worktrees").join(existing_name)).ok();
    }

    #[test]
    fn delete_worktree_rejects_nonexistent() {
        let result = WorktreeManager::delete_worktree("/nonexistent/path/xyz", "/tmp");
        assert!(result.is_err());
        if let Err(WorktreeError::NotFound(_)) = result {
            // expected
        } else {
            panic!("Expected NotFound error");
        }
    }
}
