use thiserror::Error;

/// Errors that can occur when performing read-only git operations.
///
/// These errors are returned by implementations of [`GitReadAdapter`] and
/// cover both low-level I/O failures and git-specific command errors.
#[derive(Debug, Error)]
pub enum GitReadError {
    /// A git command returned a non-zero exit code.
    ///
    /// This typically occurs when:
    /// - The repository path is invalid or not a git repository
    /// - The requested branch or ref doesn't exist
    /// - Git binary is not installed or not in PATH
    /// - Git command encountered an error (stderr is captured in the message)
    #[error("git command failed: {0}")]
    Command(String),

    /// Failed to read from or write to the filesystem during git operations.
    ///
    /// This typically occurs when:
    /// - Repository directory is inaccessible
    /// - Insufficient file permissions
    /// - Disk I/O errors
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Git command output contained invalid UTF-8.
    ///
    /// This is rare but can occur if:
    /// - File paths in the repository use non-UTF-8 encoding
    /// - Git binary produces unexpected output
    #[error("utf8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Read-only git operations used by orchestration/UI status flows.
///
/// This is intentionally narrow and excludes write operations (merge/rebase/push).
/// Write-paths remain on the existing shell-based runner until parity testing is done.
pub trait GitReadAdapter: Send + Sync {
    fn current_branch(&self, repo_dir: &str) -> Result<String, GitReadError>;
    fn status_porcelain(&self, repo_dir: &str) -> Result<Vec<String>, GitReadError>;
    fn diff_stat(&self, repo_dir: &str, base: &str, head: &str) -> Result<String, GitReadError>;
    fn conflict_files(&self, repo_dir: &str) -> Result<Vec<String>, GitReadError>;
}

/// Shell-based read adapter. This is the baseline behavior for migration.
pub struct ShellGitReadAdapter;

impl ShellGitReadAdapter {
    fn run_git(repo_dir: &str, args: &[&str]) -> Result<String, GitReadError> {
        let output = std::process::Command::new("git")
            .current_dir(repo_dir)
            .args(args)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8(output.stderr)
                .unwrap_or_else(|_| "git returned non-utf8 stderr".to_string());
            return Err(GitReadError::Command(stderr.trim().to_string()));
        }

        let stdout = String::from_utf8(output.stdout)?;
        Ok(stdout.trim().to_string())
    }
}

impl GitReadAdapter for ShellGitReadAdapter {
    fn current_branch(&self, repo_dir: &str) -> Result<String, GitReadError> {
        Self::run_git(repo_dir, &["rev-parse", "--abbrev-ref", "HEAD"])
    }

    fn status_porcelain(&self, repo_dir: &str) -> Result<Vec<String>, GitReadError> {
        let out = Self::run_git(repo_dir, &["status", "--porcelain"])?;
        let lines = out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        Ok(lines)
    }

    fn diff_stat(&self, repo_dir: &str, base: &str, head: &str) -> Result<String, GitReadError> {
        Self::run_git(repo_dir, &["diff", "--stat", base, head])
    }

    fn conflict_files(&self, repo_dir: &str) -> Result<Vec<String>, GitReadError> {
        let out = Self::run_git(repo_dir, &["diff", "--name-only", "--diff-filter=U"])?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(ToOwned::to_owned)
            .collect())
    }
}

// ---------------------------------------------------------------------------
// git2-based read adapter (feature-gated)
// ---------------------------------------------------------------------------

/// Native libgit2 read adapter — ~10-50x faster than shelling out.
///
/// Implements the same `GitReadAdapter` trait as `ShellGitReadAdapter` but
/// uses in-process libgit2 calls via `Git2ReadOps`. This is the intended
/// production adapter when the `libgit2` feature is enabled.
#[cfg(feature = "libgit2")]
pub struct Git2ReadAdapter;

#[cfg(feature = "libgit2")]
impl GitReadAdapter for Git2ReadAdapter {
    fn current_branch(&self, repo_dir: &str) -> Result<String, GitReadError> {
        crate::git2_ops::Git2ReadOps::current_branch(std::path::Path::new(repo_dir))
            .map_err(|e| GitReadError::Command(e.to_string()))
    }

    fn status_porcelain(&self, repo_dir: &str) -> Result<Vec<String>, GitReadError> {
        let entries = crate::git2_ops::Git2ReadOps::status(std::path::Path::new(repo_dir))
            .map_err(|e| GitReadError::Command(e.to_string()))?;

        // Format like `git status --porcelain`: "XY path"
        let lines = entries
            .iter()
            .map(|e| {
                let prefix = match e.status {
                    crate::repo::DiffStatus::Modified => "M ",
                    crate::repo::DiffStatus::Added => "A ",
                    crate::repo::DiffStatus::Deleted => "D ",
                    crate::repo::DiffStatus::Renamed => "R ",
                    crate::repo::DiffStatus::Copied => "C ",
                    crate::repo::DiffStatus::Untracked => "??",
                };
                format!("{} {}", prefix, e.path)
            })
            .collect();

        Ok(lines)
    }

    fn diff_stat(&self, repo_dir: &str, base: &str, head: &str) -> Result<String, GitReadError> {
        let entries = crate::git2_ops::Git2ReadOps::diff_stat_refs(
            std::path::Path::new(repo_dir),
            base,
            head,
        )
        .map_err(|e| GitReadError::Command(e.to_string()))?;

        if entries.is_empty() {
            return Ok(String::new());
        }

        // Format like `git diff --stat`: " path | N +++---"
        let mut lines = Vec::with_capacity(entries.len() + 1);
        let mut total_adds = 0u32;
        let mut total_dels = 0u32;

        for entry in &entries {
            let changes = entry.additions + entry.deletions;
            let plus = "+".repeat(entry.additions as usize);
            let minus = "-".repeat(entry.deletions as usize);
            lines.push(format!(" {} | {} {}{}", entry.path, changes, plus, minus));
            total_adds += entry.additions;
            total_dels += entry.deletions;
        }

        lines.push(format!(
            " {} files changed, {} insertions(+), {} deletions(-)",
            entries.len(),
            total_adds,
            total_dels,
        ));

        Ok(lines.join("\n"))
    }

    fn conflict_files(&self, repo_dir: &str) -> Result<Vec<String>, GitReadError> {
        let repo = crate::git2_ops::Git2ReadOps::open(std::path::Path::new(repo_dir))
            .map_err(|e| GitReadError::Command(e.to_string()))?;

        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(false)
            .recurse_untracked_dirs(false)
            .include_ignored(false);

        let statuses = repo
            .statuses(Some(&mut opts))
            .map_err(|e| GitReadError::Command(e.message().to_string()))?;

        Ok(statuses
            .iter()
            .filter(|s| s.status().contains(git2::Status::CONFLICTED))
            .filter_map(|s| s.path().map(ToOwned::to_owned))
            .collect())
    }
}

/// Create the best available read adapter for the current build.
///
/// Returns `Git2ReadAdapter` when the `libgit2` feature is enabled,
/// otherwise falls back to `ShellGitReadAdapter`.
pub fn default_read_adapter() -> Box<dyn GitReadAdapter> {
    #[cfg(feature = "libgit2")]
    {
        Box::new(Git2ReadAdapter)
    }
    #[cfg(not(feature = "libgit2"))]
    {
        Box::new(ShellGitReadAdapter)
    }
}

#[cfg(test)]
mod tests {
    use super::{default_read_adapter, GitReadAdapter, ShellGitReadAdapter};
    use std::path::Path;

    #[test]
    fn shell_adapter_is_object_safe() {
        let adapter: Box<dyn GitReadAdapter> = Box::new(ShellGitReadAdapter);
        let _ = adapter;
    }

    #[test]
    fn default_adapter_is_object_safe() {
        let adapter = default_read_adapter();
        let _ = adapter;
    }

    fn run_git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("git command should run");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn run_git_output(dir: &Path, args: &[&str]) -> std::process::Output {
        std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("git command should run")
    }

    fn init_fixture_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        run_git(root, &["init"]);
        run_git(root, &["config", "user.email", "dev@example.com"]);
        run_git(root, &["config", "user.name", "Auto Tundra"]);

        std::fs::write(root.join("README.md"), "hello\n").expect("write README");
        run_git(root, &["add", "README.md"]);
        run_git(root, &["commit", "-m", "initial"]);
        run_git(root, &["branch", "-M", "main"]);

        run_git(root, &["checkout", "-b", "feature/adapter-test"]);
        std::fs::write(root.join("README.md"), "hello\nmore\n").expect("update README");
        run_git(root, &["add", "README.md"]);
        run_git(root, &["commit", "-m", "feature change"]);

        tmp
    }

    #[test]
    fn shell_adapter_current_branch_fixture() {
        let tmp = init_fixture_repo();
        let adapter = ShellGitReadAdapter;
        let branch = adapter
            .current_branch(tmp.path().to_str().unwrap())
            .unwrap();
        assert_eq!(branch, "feature/adapter-test");
    }

    #[test]
    fn shell_adapter_status_porcelain_fixture() {
        let tmp = init_fixture_repo();
        std::fs::write(tmp.path().join("README.md"), "hello\nmore\ndirty\n").unwrap();

        let adapter = ShellGitReadAdapter;
        let lines = adapter
            .status_porcelain(tmp.path().to_str().unwrap())
            .unwrap();
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|l| l.contains("README.md")));
    }

    #[test]
    fn shell_adapter_diff_stat_fixture() {
        let tmp = init_fixture_repo();
        let adapter = ShellGitReadAdapter;
        let stat = adapter
            .diff_stat(tmp.path().to_str().unwrap(), "main", "feature/adapter-test")
            .unwrap();
        assert!(stat.contains("README.md"));
    }

    #[test]
    fn shell_adapter_conflict_files_fixture() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        run_git(root, &["init"]);
        run_git(root, &["config", "user.email", "dev@example.com"]);
        run_git(root, &["config", "user.name", "Auto Tundra"]);

        std::fs::write(root.join("conflict.txt"), "base\n").expect("write base");
        run_git(root, &["add", "conflict.txt"]);
        run_git(root, &["commit", "-m", "base"]);
        run_git(root, &["branch", "-M", "main"]);

        run_git(root, &["checkout", "-b", "feature/conflict"]);
        std::fs::write(root.join("conflict.txt"), "feature\n").expect("write feature");
        run_git(root, &["commit", "-am", "feature edit"]);

        run_git(root, &["checkout", "main"]);
        std::fs::write(root.join("conflict.txt"), "main\n").expect("write main");
        run_git(root, &["commit", "-am", "main edit"]);

        let merge = run_git_output(root, &["merge", "feature/conflict"]);
        assert!(!merge.status.success(), "expected merge conflict");

        let adapter = ShellGitReadAdapter;
        let conflicts = adapter.conflict_files(root.to_str().unwrap()).unwrap();
        assert!(
            conflicts.iter().any(|p| p.ends_with("conflict.txt")),
            "expected conflict.txt in conflict file list, got: {conflicts:?}"
        );

        let _ = run_git_output(root, &["merge", "--abort"]);
    }

    #[cfg(feature = "libgit2")]
    mod git2_adapter_tests {
        use super::super::{Git2ReadAdapter, GitReadAdapter};
        use std::path::PathBuf;

        fn workspace_root() -> PathBuf {
            let manifest = env!("CARGO_MANIFEST_DIR");
            PathBuf::from(manifest)
                .parent()
                .and_then(|p| p.parent())
                .expect("workspace root")
                .to_path_buf()
        }

        #[test]
        fn git2_adapter_is_object_safe() {
            let adapter: Box<dyn GitReadAdapter> = Box::new(Git2ReadAdapter);
            let _ = adapter;
        }

        #[test]
        fn git2_adapter_current_branch() {
            let root = workspace_root();
            let adapter = Git2ReadAdapter;
            let branch = adapter.current_branch(root.to_str().unwrap()).unwrap();
            assert!(!branch.is_empty());
        }

        #[test]
        fn git2_adapter_status_porcelain() {
            let root = workspace_root();
            let adapter = Git2ReadAdapter;
            // Should not error, even if clean
            let _lines = adapter.status_porcelain(root.to_str().unwrap()).unwrap();
        }

        #[test]
        fn git2_adapter_diff_stat_same_ref() {
            let root = workspace_root();
            let adapter = Git2ReadAdapter;
            let branch = adapter.current_branch(root.to_str().unwrap()).unwrap();
            // Diff a branch against itself — should be empty
            let stat = adapter
                .diff_stat(root.to_str().unwrap(), &branch, &branch)
                .unwrap();
            assert!(stat.is_empty());
        }

        #[test]
        fn git2_adapter_conflict_files_smoke() {
            let root = workspace_root();
            let adapter = Git2ReadAdapter;
            let _ = adapter.conflict_files(root.to_str().unwrap()).unwrap();
        }
    }
}
