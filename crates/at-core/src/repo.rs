use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("git command failed: {0}")]
    GitCommand(String),

    #[error("not a git repository: {0}")]
    NotARepo(String),

    #[error("path not found: {0}")]
    PathNotFound(String),

    #[error("invalid repo path: gitdir and workdir mismatch")]
    InvalidRepoPath,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("job cancelled")]
    Cancelled,

    #[error("job failed: {0}")]
    JobFailed(String),
}

pub type Result<T> = std::result::Result<T, RepoError>;

// ---------------------------------------------------------------------------
// RepoPath — gitui-inspired gitdir/workdir separation
// ---------------------------------------------------------------------------

/// Represents a git repository's two fundamental paths:
/// - `gitdir`: the `.git` directory (or bare repo path)
/// - `workdir`: the working directory (checkout)
///
/// Inspired by gitui's `RepoPath` which cleanly separates these concerns,
/// enabling support for worktrees, bare repos, and submodules where the
/// gitdir and workdir diverge.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepoPath {
    gitdir: PathBuf,
    workdir: PathBuf,
}

impl RepoPath {
    /// Create a RepoPath from a working directory, auto-discovering the gitdir.
    pub fn from_workdir(workdir: impl Into<PathBuf>) -> Result<Self> {
        let workdir = workdir.into();
        if !workdir.exists() {
            return Err(RepoError::PathNotFound(workdir.display().to_string()));
        }

        let gitdir = discover_gitdir(&workdir)?;
        Ok(Self { gitdir, workdir })
    }

    /// Create a RepoPath with explicit gitdir and workdir (for worktrees).
    pub fn new(gitdir: impl Into<PathBuf>, workdir: impl Into<PathBuf>) -> Self {
        Self {
            gitdir: gitdir.into(),
            workdir: workdir.into(),
        }
    }

    /// The `.git` directory path.
    pub fn gitdir(&self) -> &Path {
        &self.gitdir
    }

    /// The working directory path.
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// Whether this is a worktree (gitdir differs from workdir/.git).
    pub fn is_worktree(&self) -> bool {
        let expected_gitdir = self.workdir.join(".git");
        self.gitdir != expected_gitdir
    }

    /// Whether this is a bare repository (no workdir checkout).
    pub fn is_bare(&self) -> bool {
        self.gitdir == self.workdir
    }
}

impl std::fmt::Display for RepoPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.workdir.display())
    }
}

/// Discover the gitdir for a working directory.
///
/// When the `libgit2` feature is enabled, uses git2's native discovery
/// (no process spawn). Falls back to `git rev-parse --git-dir` otherwise.
fn discover_gitdir(workdir: &Path) -> Result<PathBuf> {
    #[cfg(feature = "libgit2")]
    {
        match crate::git2_ops::Git2ReadOps::discover_gitdir(workdir) {
            Ok(path) => return Ok(path),
            Err(_) => {
                // Fall through to shell-out as fallback
            }
        }
    }

    discover_gitdir_shell(workdir)
}

/// Shell-out fallback for gitdir discovery (`git rev-parse --git-dir`).
fn discover_gitdir_shell(workdir: &Path) -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(workdir)
        .output()?;

    if !output.status.success() {
        return Err(RepoError::NotARepo(workdir.display().to_string()));
    }

    let gitdir_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let gitdir = Path::new(&gitdir_str);

    // git rev-parse may return relative path
    if gitdir.is_absolute() {
        Ok(gitdir.to_path_buf())
    } else {
        Ok(workdir.join(gitdir))
    }
}

// ---------------------------------------------------------------------------
// AsyncGitJob — gitui-inspired async job pattern
// ---------------------------------------------------------------------------

/// Status of an async git operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Result of a completed git operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitJobResult {
    pub status: GitJobStatus,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

impl GitJobResult {
    pub fn success(&self) -> bool {
        self.status == GitJobStatus::Completed && self.exit_code == Some(0)
    }
}

/// An async git operation that runs in the background via tokio::spawn.
///
/// Inspired by gitui's `AsyncJob` pattern using crossbeam + rayon. We use
/// tokio instead (since auto-tundra is fully async) but the concept is the
/// same: git operations never block the UI thread.
pub struct AsyncGitJob {
    pub id: uuid::Uuid,
    pub description: String,
    pub status: GitJobStatus,
    handle: Option<tokio::task::JoinHandle<Result<GitJobResult>>>,
}

impl AsyncGitJob {
    /// Spawn a new async git command.
    pub fn spawn(
        repo: &RepoPath,
        args: Vec<String>,
        description: impl Into<String>,
    ) -> Self {
        let id = uuid::Uuid::new_v4();
        let desc = description.into();
        let workdir = repo.workdir().to_path_buf();
        let gitdir = repo.gitdir().to_path_buf();

        let handle = tokio::spawn(async move {
            let start = std::time::Instant::now();

            let output = tokio::process::Command::new("git")
                .args(&args)
                .current_dir(&workdir)
                .env("GIT_DIR", &gitdir)
                .output()
                .await
                .map_err(RepoError::Io)?;

            let duration_ms = start.elapsed().as_millis() as u64;
            let status = if output.status.success() {
                GitJobStatus::Completed
            } else {
                GitJobStatus::Failed
            };

            Ok(GitJobResult {
                status,
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code(),
                duration_ms,
            })
        });

        Self {
            id,
            description: desc,
            status: GitJobStatus::Running,
            handle: Some(handle),
        }
    }

    /// Wait for the job to complete and return its result.
    pub async fn wait(mut self) -> Result<GitJobResult> {
        match self.handle.take() {
            Some(handle) => {
                let result = handle.await.map_err(|e| RepoError::JobFailed(e.to_string()))??;
                Ok(result)
            }
            None => Err(RepoError::JobFailed("job already consumed".to_string())),
        }
    }

    /// Cancel the job.
    pub fn cancel(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            self.status = GitJobStatus::Cancelled;
        }
    }
}

// ---------------------------------------------------------------------------
// High-level async git operations
// ---------------------------------------------------------------------------

/// Non-blocking git operations for the UI layer.
pub struct AsyncGitOps;

impl AsyncGitOps {
    /// Get the current branch name.
    pub fn current_branch(repo: &RepoPath) -> AsyncGitJob {
        AsyncGitJob::spawn(
            repo,
            vec!["rev-parse".into(), "--abbrev-ref".into(), "HEAD".into()],
            "get current branch",
        )
    }

    /// Get short status output.
    pub fn status(repo: &RepoPath) -> AsyncGitJob {
        AsyncGitJob::spawn(
            repo,
            vec!["status".into(), "--porcelain".into(), "-b".into()],
            "git status",
        )
    }

    /// Get the log (last N commits).
    pub fn log(repo: &RepoPath, count: u32) -> AsyncGitJob {
        AsyncGitJob::spawn(
            repo,
            vec![
                "log".into(),
                format!("-{}", count),
                "--oneline".into(),
                "--decorate".into(),
            ],
            format!("git log -{}", count),
        )
    }

    /// Fetch from a remote.
    pub fn fetch(repo: &RepoPath, remote: &str) -> AsyncGitJob {
        AsyncGitJob::spawn(
            repo,
            vec!["fetch".into(), remote.into()],
            format!("git fetch {}", remote),
        )
    }

    /// Get diff stats.
    pub fn diff_stat(repo: &RepoPath) -> AsyncGitJob {
        AsyncGitJob::spawn(
            repo,
            vec!["diff".into(), "--stat".into()],
            "git diff --stat",
        )
    }

    /// List branches.
    pub fn branches(repo: &RepoPath) -> AsyncGitJob {
        AsyncGitJob::spawn(
            repo,
            vec!["branch".into(), "-a".into(), "--format=%(refname:short)".into()],
            "list branches",
        )
    }

    /// Stage specific files.
    pub fn add(repo: &RepoPath, files: Vec<String>) -> AsyncGitJob {
        let mut args = vec!["add".into()];
        args.extend(files);
        AsyncGitJob::spawn(repo, args, "git add")
    }

    /// Commit with a message.
    pub fn commit(repo: &RepoPath, message: &str) -> AsyncGitJob {
        AsyncGitJob::spawn(
            repo,
            vec!["commit".into(), "-m".into(), message.into()],
            "git commit",
        )
    }

    /// Get list of stashes.
    pub fn stash_list(repo: &RepoPath) -> AsyncGitJob {
        AsyncGitJob::spawn(
            repo,
            vec!["stash".into(), "list".into()],
            "git stash list",
        )
    }

    /// Get the list of tags.
    pub fn tags(repo: &RepoPath) -> AsyncGitJob {
        AsyncGitJob::spawn(
            repo,
            vec!["tag".into(), "--list".into(), "--sort=-creatordate".into()],
            "list tags",
        )
    }
}

// ---------------------------------------------------------------------------
// DiffEntry — structured diff output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    pub path: String,
    pub status: DiffStatus,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Untracked,
}

/// Parse `git diff --numstat` output into DiffEntry list.
pub fn parse_numstat(output: &str) -> Vec<DiffEntry> {
    output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let additions = parts[0].parse::<u32>().unwrap_or(0);
                let deletions = parts[1].parse::<u32>().unwrap_or(0);
                let path = parts[2].to_string();
                Some(DiffEntry {
                    path,
                    status: DiffStatus::Modified,
                    additions,
                    deletions,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Parse `git status --porcelain` output into DiffEntry list.
pub fn parse_porcelain_status(output: &str) -> Vec<DiffEntry> {
    output
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            let status_code = &line[..2];
            let path = line[3..].to_string();

            let status = match status_code.trim() {
                "A" | "A " | " A" => DiffStatus::Added,
                "M" | "M " | " M" | "MM" => DiffStatus::Modified,
                "D" | "D " | " D" => DiffStatus::Deleted,
                "R" | "R " => DiffStatus::Renamed,
                "C" | "C " => DiffStatus::Copied,
                "??" => DiffStatus::Untracked,
                _ => DiffStatus::Modified,
            };

            Some(DiffEntry {
                path,
                status,
                additions: 0,
                deletions: 0,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_path_new() {
        let rp = RepoPath::new("/repo/.git", "/repo");
        assert_eq!(rp.gitdir(), Path::new("/repo/.git"));
        assert_eq!(rp.workdir(), Path::new("/repo"));
        assert!(!rp.is_worktree());
        assert!(!rp.is_bare());
    }

    #[test]
    fn repo_path_worktree_detection() {
        let rp = RepoPath::new("/repo/.git/worktrees/feat", "/repo/.worktrees/feat");
        assert!(rp.is_worktree());
        assert!(!rp.is_bare());
    }

    #[test]
    fn repo_path_bare_detection() {
        let rp = RepoPath::new("/repo.git", "/repo.git");
        assert!(rp.is_bare());
    }

    #[test]
    fn repo_path_display() {
        let rp = RepoPath::new("/repo/.git", "/repo");
        assert_eq!(rp.to_string(), "/repo");
    }

    #[test]
    fn repo_path_serialize() {
        let rp = RepoPath::new("/repo/.git", "/repo");
        let json = serde_json::to_string(&rp).unwrap();
        let back: RepoPath = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rp);
    }

    #[test]
    fn parse_numstat_output() {
        let output = "10\t2\tsrc/main.rs\n5\t0\tsrc/lib.rs\n";
        let entries = parse_numstat(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "src/main.rs");
        assert_eq!(entries[0].additions, 10);
        assert_eq!(entries[0].deletions, 2);
        assert_eq!(entries[1].path, "src/lib.rs");
        assert_eq!(entries[1].additions, 5);
    }

    #[test]
    fn parse_numstat_empty() {
        assert!(parse_numstat("").is_empty());
        assert!(parse_numstat("\n").is_empty());
    }

    #[test]
    fn parse_porcelain_status_output() {
        let output = " M src/main.rs\nA  src/new.rs\n?? untracked.txt\n D  deleted.rs\n";
        let entries = parse_porcelain_status(output);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].status, DiffStatus::Modified);
        assert_eq!(entries[0].path, "src/main.rs");
        assert_eq!(entries[1].status, DiffStatus::Added);
        assert_eq!(entries[2].status, DiffStatus::Untracked);
        assert_eq!(entries[3].status, DiffStatus::Deleted);
    }

    #[test]
    fn parse_porcelain_status_empty() {
        assert!(parse_porcelain_status("").is_empty());
    }

    #[test]
    fn diff_status_serialize() {
        let json = serde_json::to_string(&DiffStatus::Added).unwrap();
        assert_eq!(json, "\"added\"");
        let back: DiffStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, DiffStatus::Added);
    }

    #[test]
    fn git_job_result_success_check() {
        let result = GitJobResult {
            status: GitJobStatus::Completed,
            stdout: "ok".to_string(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 100,
        };
        assert!(result.success());

        let failed = GitJobResult {
            status: GitJobStatus::Failed,
            stdout: String::new(),
            stderr: "error".to_string(),
            exit_code: Some(1),
            duration_ms: 50,
        };
        assert!(!failed.success());
    }

    #[test]
    fn git_job_status_serialize() {
        let json = serde_json::to_string(&GitJobStatus::Running).unwrap();
        assert_eq!(json, "\"running\"");
    }

    #[test]
    fn git_job_result_serialize() {
        let result = GitJobResult {
            status: GitJobStatus::Completed,
            stdout: "main".to_string(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 42,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: GitJobResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.stdout, "main");
        assert_eq!(back.duration_ms, 42);
    }

    #[test]
    fn repo_path_hash() {
        use std::collections::HashSet;
        let rp1 = RepoPath::new("/a/.git", "/a");
        let rp2 = RepoPath::new("/b/.git", "/b");
        let rp3 = RepoPath::new("/a/.git", "/a");
        let mut set = HashSet::new();
        set.insert(rp1);
        set.insert(rp2);
        set.insert(rp3);
        assert_eq!(set.len(), 2);
    }

    #[tokio::test]
    async fn async_git_job_cancel() {
        let rp = RepoPath::new("/tmp/.git", "/tmp");
        let mut job = AsyncGitJob::spawn(&rp, vec!["version".into()], "test cancel");
        job.cancel();
        assert_eq!(job.status, GitJobStatus::Cancelled);
    }
}
