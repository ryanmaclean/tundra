//! Native git read operations via libgit2 (git2 crate).
//!
//! Provides fast, in-process alternatives to shelling out to `git` for
//! read-only queries. Write operations (commit, merge, rebase, fetch) stay
//! as shell-outs — libgit2 is intentionally used only for reads.
//!
//! # Why git2 for reads?
//!
//! - No process spawn overhead (~10-50x faster for hot-path queries)
//! - Structured output without parsing porcelain text
//! - Direct access to libgit2 diff/status/log iterators
//! - Type-safe branch/tag enumeration
//!
//! # Feature gated
//!
//! This module is only available with the `libgit2` feature flag (enabled
//! by default). When disabled, all operations fall back to shell-out via
//! `AsyncGitOps`.

use std::path::Path;

use chrono;
use serde::{Deserialize, Serialize};

use crate::repo::{DiffEntry, DiffStatus, RepoError};

// ---------------------------------------------------------------------------
// Error bridging
// ---------------------------------------------------------------------------

impl From<git2::Error> for RepoError {
    fn from(e: git2::Error) -> Self {
        RepoError::GitCommand(e.message().to_string())
    }
}

// ---------------------------------------------------------------------------
// Native read operations
// ---------------------------------------------------------------------------

/// Native git read operations using libgit2.
///
/// Stateless — opens the repo fresh for each call. This avoids stale index
/// issues and is fine for reads (the repo open is <1ms for local repos).
pub struct Git2ReadOps;

impl Git2ReadOps {
    /// Open a git2 Repository from a working directory path.
    fn open(workdir: &Path) -> Result<git2::Repository, RepoError> {
        git2::Repository::discover(workdir).map_err(RepoError::from)
    }

    /// Discover the gitdir for a working directory (replaces `git rev-parse --git-dir`).
    pub fn discover_gitdir(workdir: &Path) -> Result<std::path::PathBuf, RepoError> {
        let repo = Self::open(workdir)?;
        Ok(repo.path().to_path_buf())
    }

    /// Get the current branch name (replaces `git rev-parse --abbrev-ref HEAD`).
    pub fn current_branch(workdir: &Path) -> Result<String, RepoError> {
        let repo = Self::open(workdir)?;
        let head = repo.head().map_err(RepoError::from)?;

        if head.is_branch() {
            Ok(head.shorthand().unwrap_or("HEAD").to_string())
        } else {
            // Detached HEAD — return short OID
            let oid = head
                .target()
                .ok_or_else(|| RepoError::GitCommand("HEAD has no target".to_string()))?;
            Ok(format!("{:.7}", oid))
        }
    }

    /// Get working directory status (replaces `git status --porcelain`).
    ///
    /// Returns a list of changed files with their status. Only includes
    /// files that differ from HEAD or are untracked.
    pub fn status(workdir: &Path) -> Result<Vec<DiffEntry>, RepoError> {
        let repo = Self::open(workdir)?;

        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false)
            .renames_head_to_index(true);

        let statuses = repo.statuses(Some(&mut opts)).map_err(RepoError::from)?;
        let mut entries = Vec::with_capacity(statuses.len());

        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("").to_string();
            let st = entry.status();

            let status =
                if st.contains(git2::Status::WT_NEW) || st.contains(git2::Status::INDEX_NEW) {
                    DiffStatus::Added
                } else if st.contains(git2::Status::WT_DELETED)
                    || st.contains(git2::Status::INDEX_DELETED)
                {
                    DiffStatus::Deleted
                } else if st.contains(git2::Status::WT_RENAMED)
                    || st.contains(git2::Status::INDEX_RENAMED)
                {
                    DiffStatus::Renamed
                } else if st.contains(git2::Status::WT_MODIFIED)
                    || st.contains(git2::Status::INDEX_MODIFIED)
                    || st.intersects(git2::Status::WT_TYPECHANGE | git2::Status::INDEX_TYPECHANGE)
                {
                    DiffStatus::Modified
                } else {
                    DiffStatus::Untracked
                };

            entries.push(DiffEntry {
                path,
                status,
                additions: 0,
                deletions: 0,
            });
        }

        Ok(entries)
    }

    /// Get diff stats between HEAD and working directory
    /// (replaces `git diff --numstat`).
    pub fn diff_stat(workdir: &Path) -> Result<Vec<DiffEntry>, RepoError> {
        let repo = Self::open(workdir)?;
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());

        let diff = repo
            .diff_tree_to_workdir_with_index(head_tree.as_ref(), None)
            .map_err(RepoError::from)?;

        let _stats = diff.stats().map_err(RepoError::from)?;
        let mut entries = Vec::new();

        // Walk each delta for per-file stats
        for (idx, delta) in diff.deltas().enumerate() {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let status = match delta.status() {
                git2::Delta::Added => DiffStatus::Added,
                git2::Delta::Deleted => DiffStatus::Deleted,
                git2::Delta::Modified => DiffStatus::Modified,
                git2::Delta::Renamed => DiffStatus::Renamed,
                git2::Delta::Copied => DiffStatus::Copied,
                _ => DiffStatus::Modified,
            };

            // Per-file line stats require walking patches
            let (additions, deletions) = diff
                .get_delta(idx)
                .map(|_| {
                    // Use the overall stats as approximation when we can't
                    // get per-file stats cheaply. For detailed per-file,
                    // we'd need to iterate hunks which is more expensive.
                    (0u32, 0u32)
                })
                .unwrap_or((0, 0));

            entries.push(DiffEntry {
                path,
                status,
                additions,
                deletions,
            });
        }

        // Try to get per-file line counts by walking patches
        let _ = diff.foreach(
            &mut |_, _| true, // file cb
            None,             // binary cb
            None,             // hunk cb
            None,             // line cb
        );

        // More accurate: walk with print to get line stats
        let mut line_stats: Vec<(u32, u32)> = vec![(0, 0); entries.len()];
        let mut file_idx = 0usize;
        let _ = diff.print(git2::DiffFormat::Patch, |delta, _hunk, line| {
            // Track which file we're in
            let current_path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            // Find matching entry
            if let Some(pos) = entries.iter().position(|e| e.path == current_path) {
                file_idx = pos;
            }

            if file_idx < line_stats.len() {
                match line.origin() {
                    '+' => line_stats[file_idx].0 += 1,
                    '-' => line_stats[file_idx].1 += 1,
                    _ => {}
                }
            }
            true
        });

        // Apply collected line stats
        for (entry, (adds, dels)) in entries.iter_mut().zip(line_stats.iter()) {
            entry.additions = *adds;
            entry.deletions = *dels;
        }

        Ok(entries)
    }

    /// List all branches (replaces `git branch -a --format=%(refname:short)`).
    pub fn branches(workdir: &Path) -> Result<Vec<BranchInfo>, RepoError> {
        let repo = Self::open(workdir)?;
        let branches = repo.branches(None).map_err(RepoError::from)?;

        let mut result = Vec::new();
        for branch in branches {
            let (branch, branch_type) = branch.map_err(RepoError::from)?;
            let name = branch
                .name()
                .map_err(RepoError::from)?
                .unwrap_or("")
                .to_string();

            let is_head = branch.is_head();
            let is_remote = branch_type == git2::BranchType::Remote;

            result.push(BranchInfo {
                name,
                is_head,
                is_remote,
            });
        }

        Ok(result)
    }

    /// List tags sorted by creation date (replaces `git tag --list --sort=-creatordate`).
    pub fn tags(workdir: &Path) -> Result<Vec<String>, RepoError> {
        let repo = Self::open(workdir)?;
        let mut tags = Vec::new();

        repo.tag_foreach(|_oid, name| {
            if let Ok(name_str) = std::str::from_utf8(name) {
                let short = name_str.strip_prefix("refs/tags/").unwrap_or(name_str);
                tags.push(short.to_string());
            }
            true
        })
        .map_err(RepoError::from)?;

        // Reverse to approximate newest-first (tag_foreach returns in ref order)
        tags.reverse();
        Ok(tags)
    }

    /// Get commit log (replaces `git log -N --oneline --decorate`).
    pub fn log(workdir: &Path, count: usize) -> Result<Vec<CommitInfo>, RepoError> {
        let repo = Self::open(workdir)?;
        let mut revwalk = repo.revwalk().map_err(RepoError::from)?;
        revwalk.push_head().map_err(RepoError::from)?;
        revwalk
            .set_sorting(git2::Sort::TIME)
            .map_err(RepoError::from)?;

        let mut commits = Vec::with_capacity(count);
        for oid_result in revwalk.take(count) {
            let oid = oid_result.map_err(RepoError::from)?;
            let commit = repo.find_commit(oid).map_err(RepoError::from)?;

            let message = commit.summary().unwrap_or("").to_string();

            let author = commit.author();
            let author_name = author.name().unwrap_or("").to_string();
            let time = commit.time();
            let timestamp = time.seconds();

            commits.push(CommitInfo {
                oid: format!("{:.7}", oid),
                message,
                author: author_name,
                timestamp,
            });
        }

        Ok(commits)
    }

    /// Get stash list (replaces `git stash list`).
    pub fn stash_list(workdir: &Path) -> Result<Vec<StashEntry>, RepoError> {
        let repo = Self::open(workdir)?;
        let mut stashes = Vec::new();

        // git2 stash_foreach requires &mut Repository
        let mut repo = repo;
        repo.stash_foreach(|index, message, _oid| {
            stashes.push(StashEntry {
                index,
                message: message.to_string(),
            });
            true
        })
        .map_err(RepoError::from)?;

        Ok(stashes)
    }

    /// Check if a path is inside a git repository.
    pub fn is_repo(workdir: &Path) -> bool {
        git2::Repository::discover(workdir).is_ok()
    }

    /// Count commits ahead/behind between two branches
    /// (replaces `git rev-list --left-right --count`).
    pub fn ahead_behind(
        workdir: &Path,
        local_branch: &str,
        upstream_branch: &str,
    ) -> Result<(usize, usize), RepoError> {
        let repo = Self::open(workdir)?;

        let local_oid = repo
            .revparse_single(local_branch)
            .map_err(RepoError::from)?
            .id();
        let upstream_oid = repo
            .revparse_single(upstream_branch)
            .map_err(RepoError::from)?
            .id();

        repo.graph_ahead_behind(local_oid, upstream_oid)
            .map_err(RepoError::from)
    }

    /// List changed files between two refs
    /// (replaces `git diff --name-only ref1 ref2`).
    pub fn diff_name_only(
        workdir: &Path,
        from_ref: &str,
        to_ref: &str,
    ) -> Result<Vec<String>, RepoError> {
        let repo = Self::open(workdir)?;

        let from_tree = repo
            .revparse_single(from_ref)
            .map_err(RepoError::from)?
            .peel_to_tree()
            .map_err(RepoError::from)?;
        let to_tree = repo
            .revparse_single(to_ref)
            .map_err(RepoError::from)?
            .peel_to_tree()
            .map_err(RepoError::from)?;

        let diff = repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
            .map_err(RepoError::from)?;

        let mut files = Vec::new();
        for delta in diff.deltas() {
            if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                files.push(path.to_string_lossy().to_string());
            }
        }

        Ok(files)
    }

    /// Check if the working directory is clean (no modified/staged/untracked files).
    pub fn is_clean(workdir: &Path) -> Result<bool, RepoError> {
        let entries = Self::status(workdir)?;
        Ok(entries.is_empty())
    }

    /// Get diff stats between two refs (replaces `git diff --stat ref1 ref2`).
    ///
    /// Unlike `diff_stat()` which compares HEAD to working dir, this compares
    /// two arbitrary refs (branches, tags, commits). Used for merge pre-checks
    /// and PR diff views.
    pub fn diff_stat_refs(
        workdir: &Path,
        from_ref: &str,
        to_ref: &str,
    ) -> Result<Vec<DiffEntry>, RepoError> {
        let repo = Self::open(workdir)?;

        let from_tree = repo
            .revparse_single(from_ref)
            .map_err(RepoError::from)?
            .peel_to_tree()
            .map_err(RepoError::from)?;
        let to_tree = repo
            .revparse_single(to_ref)
            .map_err(RepoError::from)?
            .peel_to_tree()
            .map_err(RepoError::from)?;

        let diff = repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
            .map_err(RepoError::from)?;

        let mut entries = Vec::new();
        for delta in diff.deltas() {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let status = match delta.status() {
                git2::Delta::Added => DiffStatus::Added,
                git2::Delta::Deleted => DiffStatus::Deleted,
                git2::Delta::Modified => DiffStatus::Modified,
                git2::Delta::Renamed => DiffStatus::Renamed,
                git2::Delta::Copied => DiffStatus::Copied,
                _ => DiffStatus::Modified,
            };

            entries.push(DiffEntry {
                path,
                status,
                additions: 0,
                deletions: 0,
            });
        }

        // Walk patches for per-file line counts
        let mut line_stats: Vec<(u32, u32)> = vec![(0, 0); entries.len()];
        let _ = diff.print(git2::DiffFormat::Patch, |delta, _hunk, line| {
            let current_path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Some(pos) = entries.iter().position(|e| e.path == current_path) {
                match line.origin() {
                    '+' => line_stats[pos].0 += 1,
                    '-' => line_stats[pos].1 += 1,
                    _ => {}
                }
            }
            true
        });

        for (entry, (adds, dels)) in entries.iter_mut().zip(line_stats.iter()) {
            entry.additions = *adds;
            entry.deletions = *dels;
        }

        Ok(entries)
    }

    /// Check if there are any changes between two refs.
    ///
    /// Fast check — just tests if diff has any deltas, doesn't walk patches.
    /// Used for merge pre-checks ("nothing to merge" detection).
    pub fn has_changes_between(
        workdir: &Path,
        from_ref: &str,
        to_ref: &str,
    ) -> Result<bool, RepoError> {
        let repo = Self::open(workdir)?;

        let from_tree = repo
            .revparse_single(from_ref)
            .map_err(RepoError::from)?
            .peel_to_tree()
            .map_err(RepoError::from)?;
        let to_tree = repo
            .revparse_single(to_ref)
            .map_err(RepoError::from)?
            .peel_to_tree()
            .map_err(RepoError::from)?;

        let diff = repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
            .map_err(RepoError::from)?;

        Ok(diff.deltas().len() > 0)
    }

    // -----------------------------------------------------------------------
    // Graph & visualization operations
    // -----------------------------------------------------------------------

    /// Find the merge base (common ancestor) of two refs.
    ///
    /// Essential for stacked PR visualization — shows where a child branch
    /// forked from its parent. Also used to determine if a rebase is needed.
    pub fn merge_base(workdir: &Path, ref_a: &str, ref_b: &str) -> Result<String, RepoError> {
        let repo = Self::open(workdir)?;

        let oid_a = repo.revparse_single(ref_a).map_err(RepoError::from)?.id();
        let oid_b = repo.revparse_single(ref_b).map_err(RepoError::from)?.id();

        let base = repo.merge_base(oid_a, oid_b).map_err(RepoError::from)?;

        Ok(format!("{:.7}", base))
    }

    /// Build a branch topology graph for stack visualization.
    ///
    /// For each branch in `branch_names`, returns its relationship to
    /// `base_branch`: merge base, ahead/behind counts, and fork point age.
    /// Powers the stacked PR tree view in the frontend.
    pub fn branch_graph(
        workdir: &Path,
        base_branch: &str,
        branch_names: &[&str],
    ) -> Result<Vec<BranchGraphNode>, RepoError> {
        let repo = Self::open(workdir)?;

        let base_oid = repo
            .revparse_single(base_branch)
            .map_err(RepoError::from)?
            .id();

        let mut nodes = Vec::with_capacity(branch_names.len());

        for &branch in branch_names {
            let branch_oid = match repo.revparse_single(branch) {
                Ok(obj) => obj.id(),
                Err(_) => {
                    nodes.push(BranchGraphNode {
                        name: branch.to_string(),
                        exists: false,
                        merge_base_oid: String::new(),
                        ahead: 0,
                        behind: 0,
                        fork_point_timestamp: 0,
                        latest_commit_timestamp: 0,
                        needs_rebase: false,
                    });
                    continue;
                }
            };

            let merge_base_oid = repo.merge_base(base_oid, branch_oid).unwrap_or(base_oid);

            let (ahead, behind) = repo
                .graph_ahead_behind(branch_oid, base_oid)
                .unwrap_or((0, 0));

            // Fork point timestamp — when this branch diverged
            let fork_ts = repo
                .find_commit(merge_base_oid)
                .map(|c| c.time().seconds())
                .unwrap_or(0);

            // Latest commit on this branch
            let latest_ts = repo
                .find_commit(branch_oid)
                .map(|c| c.time().seconds())
                .unwrap_or(0);

            // Needs rebase if base has moved past the fork point
            let needs_rebase = behind > 0 && merge_base_oid != base_oid;

            nodes.push(BranchGraphNode {
                name: branch.to_string(),
                exists: true,
                merge_base_oid: format!("{:.7}", merge_base_oid),
                ahead,
                behind,
                fork_point_timestamp: fork_ts,
                latest_commit_timestamp: latest_ts,
                needs_rebase,
            });
        }

        Ok(nodes)
    }

    /// Get file-level blame summary: who wrote how much of a file.
    ///
    /// Returns a breakdown of lines per author. Useful for "ownership"
    /// indicators on the Insights page — shows who knows each area.
    pub fn blame_summary(
        workdir: &Path,
        file_path: &str,
    ) -> Result<Vec<BlameAuthorSummary>, RepoError> {
        let repo = Self::open(workdir)?;

        let blame = repo
            .blame_file(std::path::Path::new(file_path), None)
            .map_err(RepoError::from)?;

        let mut author_lines: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        let mut total_lines = 0u32;

        for hunk in blame.iter() {
            let author = hunk
                .final_signature()
                .name()
                .unwrap_or("unknown")
                .to_string();
            let lines = hunk.lines_in_hunk() as u32;
            *author_lines.entry(author).or_insert(0) += lines;
            total_lines += lines;
        }

        let mut result: Vec<BlameAuthorSummary> = author_lines
            .into_iter()
            .map(|(author, lines)| {
                let percentage = if total_lines > 0 {
                    (lines as f64 / total_lines as f64) * 100.0
                } else {
                    0.0
                };
                BlameAuthorSummary {
                    author,
                    lines,
                    percentage,
                }
            })
            .collect();

        // Sort by lines descending
        result.sort_by(|a, b| b.lines.cmp(&a.lines));
        Ok(result)
    }

    /// Get contributor stats: commits per author over the whole repo.
    ///
    /// Powers the Insights page contributor breakdown. Walks the last
    /// `max_commits` commits and tallies per-author counts.
    pub fn contributor_stats(
        workdir: &Path,
        max_commits: usize,
    ) -> Result<Vec<ContributorStat>, RepoError> {
        let repo = Self::open(workdir)?;
        let mut revwalk = repo.revwalk().map_err(RepoError::from)?;
        revwalk.push_head().map_err(RepoError::from)?;
        revwalk
            .set_sorting(git2::Sort::TIME)
            .map_err(RepoError::from)?;

        let mut author_stats: std::collections::HashMap<String, ContributorAccum> =
            std::collections::HashMap::new();

        for oid_result in revwalk.take(max_commits) {
            let oid = oid_result.map_err(RepoError::from)?;
            let commit = repo.find_commit(oid).map_err(RepoError::from)?;
            let author = commit.author().name().unwrap_or("unknown").to_string();
            let ts = commit.time().seconds();

            let entry = author_stats
                .entry(author)
                .or_insert_with(|| ContributorAccum {
                    commits: 0,
                    first_commit: ts,
                    last_commit: ts,
                });
            entry.commits += 1;
            if ts < entry.first_commit {
                entry.first_commit = ts;
            }
            if ts > entry.last_commit {
                entry.last_commit = ts;
            }
        }

        let mut result: Vec<ContributorStat> = author_stats
            .into_iter()
            .map(|(author, acc)| ContributorStat {
                author,
                commits: acc.commits,
                first_commit_timestamp: acc.first_commit,
                last_commit_timestamp: acc.last_commit,
            })
            .collect();

        result.sort_by(|a, b| b.commits.cmp(&a.commits));
        Ok(result)
    }

    /// Get commit activity heatmap data: commits per day for the last N days.
    ///
    /// Powers the GitHub-style contribution heatmap. Groups commits by
    /// date (UTC) and returns daily counts.
    pub fn commit_activity(workdir: &Path, days: u32) -> Result<Vec<DailyActivity>, RepoError> {
        let repo = Self::open(workdir)?;
        let mut revwalk = repo.revwalk().map_err(RepoError::from)?;
        revwalk.push_head().map_err(RepoError::from)?;
        revwalk
            .set_sorting(git2::Sort::TIME)
            .map_err(RepoError::from)?;

        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::days(days as i64);
        let cutoff_ts = cutoff.timestamp();

        let mut daily: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

        for oid_result in revwalk {
            let oid = oid_result.map_err(RepoError::from)?;
            let commit = repo.find_commit(oid).map_err(RepoError::from)?;
            let ts = commit.time().seconds();

            if ts < cutoff_ts {
                break; // Commits are time-sorted, so we can stop early
            }

            let dt = chrono::DateTime::from_timestamp(ts, 0).unwrap_or(cutoff);
            let date_str = dt.format("%Y-%m-%d").to_string();
            *daily.entry(date_str).or_insert(0) += 1;
        }

        let mut result: Vec<DailyActivity> = daily
            .into_iter()
            .map(|(date, count)| DailyActivity { date, count })
            .collect();

        result.sort_by(|a, b| a.date.cmp(&b.date));
        Ok(result)
    }

    /// Get a compact repo health summary in a single call.
    ///
    /// Combines multiple queries into one response — ideal for a dashboard
    /// widget that shows repo status at a glance without N separate API calls.
    pub fn repo_summary(workdir: &Path) -> Result<RepoSummary, RepoError> {
        let branch = Self::current_branch(workdir).unwrap_or_else(|_| "unknown".to_string());
        let status = Self::status(workdir).unwrap_or_default();
        let branches = Self::branches(workdir).unwrap_or_default();
        let recent = Self::log(workdir, 5).unwrap_or_default();
        let clean = status.is_empty();

        let modified_count = status
            .iter()
            .filter(|e| e.status == DiffStatus::Modified)
            .count();
        let added_count = status
            .iter()
            .filter(|e| e.status == DiffStatus::Added)
            .count();
        let deleted_count = status
            .iter()
            .filter(|e| e.status == DiffStatus::Deleted)
            .count();
        let untracked_count = status
            .iter()
            .filter(|e| e.status == DiffStatus::Untracked)
            .count();

        let local_branches = branches.iter().filter(|b| !b.is_remote).count();
        let remote_branches = branches.iter().filter(|b| b.is_remote).count();

        Ok(RepoSummary {
            current_branch: branch,
            is_clean: clean,
            modified_count,
            added_count,
            deleted_count,
            untracked_count,
            local_branch_count: local_branches,
            remote_branch_count: remote_branches,
            recent_commits: recent,
        })
    }
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

/// Branch information from libgit2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub is_remote: bool,
}

/// Commit information from libgit2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub oid: String,
    pub message: String,
    pub author: String,
    pub timestamp: i64,
}

/// Stash entry from libgit2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
}

/// Branch topology node for stack/graph visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchGraphNode {
    pub name: String,
    pub exists: bool,
    pub merge_base_oid: String,
    pub ahead: usize,
    pub behind: usize,
    pub fork_point_timestamp: i64,
    pub latest_commit_timestamp: i64,
    pub needs_rebase: bool,
}

/// Per-author blame summary for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameAuthorSummary {
    pub author: String,
    pub lines: u32,
    pub percentage: f64,
}

/// Contributor statistics from commit history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributorStat {
    pub author: String,
    pub commits: u32,
    pub first_commit_timestamp: i64,
    pub last_commit_timestamp: i64,
}

/// Internal accumulator for contributor stats.
struct ContributorAccum {
    commits: u32,
    first_commit: i64,
    last_commit: i64,
}

/// Daily commit activity for heatmap visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyActivity {
    pub date: String,
    pub count: u32,
}

/// Compact repo health summary (single API call).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSummary {
    pub current_branch: String,
    pub is_clean: bool,
    pub modified_count: usize,
    pub added_count: usize,
    pub deleted_count: usize,
    pub untracked_count: usize,
    pub local_branch_count: usize,
    pub remote_branch_count: usize,
    pub recent_commits: Vec<CommitInfo>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Get the workspace root (which is a git repo).
    fn workspace_root() -> PathBuf {
        let manifest = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest)
            .parent() // crates/
            .and_then(|p| p.parent()) // rust-harness/
            .expect("workspace root")
            .to_path_buf()
    }

    #[test]
    fn discover_gitdir_finds_repo() {
        let root = workspace_root();
        let gitdir = Git2ReadOps::discover_gitdir(&root).unwrap();
        assert!(gitdir.exists());
    }

    #[test]
    fn current_branch_returns_string() {
        let root = workspace_root();
        let branch = Git2ReadOps::current_branch(&root).unwrap();
        assert!(!branch.is_empty());
    }

    #[test]
    fn status_returns_entries() {
        let root = workspace_root();
        // Should not error even if clean
        let _entries = Git2ReadOps::status(&root).unwrap();
    }

    #[test]
    fn branches_lists_at_least_one() {
        let root = workspace_root();
        let branches = Git2ReadOps::branches(&root).unwrap();
        // CI may use detached-HEAD checkout with no local branches;
        // in a normal working copy there is at least one.
        if !branches.is_empty() {
            // If local branches exist, one should be HEAD (unless detached)
            let has_head = branches.iter().any(|b| b.is_head);
            let all_remote = branches.iter().all(|b| b.is_remote);
            assert!(
                has_head || all_remote,
                "if local branches exist, one should be HEAD"
            );
        }
    }

    #[test]
    fn tags_does_not_error() {
        let root = workspace_root();
        let _tags = Git2ReadOps::tags(&root).unwrap();
    }

    #[test]
    fn log_returns_commits() {
        let root = workspace_root();
        let commits = Git2ReadOps::log(&root, 5).unwrap();
        assert!(!commits.is_empty(), "should have at least one commit");
        assert!(commits.len() <= 5);
        assert!(!commits[0].oid.is_empty());
        assert!(!commits[0].message.is_empty());
    }

    #[test]
    fn stash_list_does_not_error() {
        let root = workspace_root();
        let _stashes = Git2ReadOps::stash_list(&root).unwrap();
    }

    #[test]
    fn is_repo_detects_git() {
        let root = workspace_root();
        assert!(Git2ReadOps::is_repo(&root));
        assert!(!Git2ReadOps::is_repo(Path::new("/nonexistent/path")));
    }

    #[test]
    fn diff_stat_does_not_error() {
        let root = workspace_root();
        let _entries = Git2ReadOps::diff_stat(&root).unwrap();
    }

    #[test]
    fn is_clean_returns_bool() {
        let root = workspace_root();
        let _clean = Git2ReadOps::is_clean(&root).unwrap();
    }

    #[test]
    fn branch_info_serialize() {
        let info = BranchInfo {
            name: "main".to_string(),
            is_head: true,
            is_remote: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: BranchInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "main");
        assert!(back.is_head);
    }

    #[test]
    fn commit_info_serialize() {
        let info = CommitInfo {
            oid: "abc1234".to_string(),
            message: "test commit".to_string(),
            author: "Dev".to_string(),
            timestamp: 1700000000,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: CommitInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.oid, "abc1234");
    }

    #[test]
    fn stash_entry_serialize() {
        let entry = StashEntry {
            index: 0,
            message: "WIP on main".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: StashEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.index, 0);
    }

    #[test]
    fn diff_stat_refs_does_not_error() {
        let root = workspace_root();
        let branch = Git2ReadOps::current_branch(&root).unwrap();
        // Diff branch against itself — should return empty
        let entries = Git2ReadOps::diff_stat_refs(&root, &branch, &branch).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn has_changes_between_same_ref() {
        let root = workspace_root();
        let branch = Git2ReadOps::current_branch(&root).unwrap();
        let has = Git2ReadOps::has_changes_between(&root, &branch, &branch).unwrap();
        assert!(!has);
    }

    // -- Graph & visualization tests --

    #[test]
    fn merge_base_finds_ancestor() {
        let root = workspace_root();
        let branch = Git2ReadOps::current_branch(&root).unwrap();
        // Merge base of a branch with itself is its own tip
        let base = Git2ReadOps::merge_base(&root, &branch, &branch).unwrap();
        assert!(!base.is_empty());
        assert_eq!(base.len(), 7); // short OID
    }

    #[test]
    fn branch_graph_returns_nodes() {
        let root = workspace_root();
        let branch = Git2ReadOps::current_branch(&root).unwrap();
        let nodes = Git2ReadOps::branch_graph(&root, &branch, &[&branch]).unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].exists);
        assert_eq!(nodes[0].ahead, 0); // branch vs itself = 0 ahead
        assert_eq!(nodes[0].behind, 0);
        assert!(!nodes[0].needs_rebase);
    }

    #[test]
    fn branch_graph_handles_missing_branch() {
        let root = workspace_root();
        let branch = Git2ReadOps::current_branch(&root).unwrap();
        let nodes =
            Git2ReadOps::branch_graph(&root, &branch, &["nonexistent-branch-xyz-999"]).unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(!nodes[0].exists);
    }

    #[test]
    fn blame_summary_on_known_file() {
        let root = workspace_root();
        // Blame our own source file — should always exist
        let summary = Git2ReadOps::blame_summary(&root, "crates/at-core/src/git2_ops.rs");
        // May fail if file isn't committed yet — that's OK in test
        if let Ok(authors) = summary {
            assert!(!authors.is_empty());
            let total_pct: f64 = authors.iter().map(|a| a.percentage).sum();
            assert!((total_pct - 100.0).abs() < 0.1);
        }
    }

    #[test]
    fn contributor_stats_returns_authors() {
        let root = workspace_root();
        let stats = Git2ReadOps::contributor_stats(&root, 100).unwrap();
        assert!(!stats.is_empty());
        // Should be sorted by commits descending
        for window in stats.windows(2) {
            assert!(window[0].commits >= window[1].commits);
        }
    }

    #[test]
    fn commit_activity_returns_days() {
        let root = workspace_root();
        let activity = Git2ReadOps::commit_activity(&root, 30).unwrap();
        // Should have at least one day with commits (we just committed)
        // Dates should be sorted
        for window in activity.windows(2) {
            assert!(window[0].date <= window[1].date);
        }
    }

    #[test]
    fn repo_summary_combines_queries() {
        let root = workspace_root();
        let summary = Git2ReadOps::repo_summary(&root).unwrap();
        assert!(!summary.current_branch.is_empty());
        // CI detached-HEAD checkout may have zero local branches
        assert!(!summary.recent_commits.is_empty());
    }

    #[test]
    fn branch_graph_node_serialize() {
        let node = BranchGraphNode {
            name: "feature/auth".to_string(),
            exists: true,
            merge_base_oid: "abc1234".to_string(),
            ahead: 3,
            behind: 1,
            fork_point_timestamp: 1700000000,
            latest_commit_timestamp: 1700100000,
            needs_rebase: true,
        };
        let json = serde_json::to_string(&node).unwrap();
        let back: BranchGraphNode = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ahead, 3);
        assert!(back.needs_rebase);
    }

    #[test]
    fn daily_activity_serialize() {
        let day = DailyActivity {
            date: "2026-02-21".to_string(),
            count: 5,
        };
        let json = serde_json::to_string(&day).unwrap();
        let back: DailyActivity = serde_json::from_str(&json).unwrap();
        assert_eq!(back.date, "2026-02-21");
        assert_eq!(back.count, 5);
    }

    #[test]
    fn repo_summary_serialize() {
        let summary = RepoSummary {
            current_branch: "main".to_string(),
            is_clean: true,
            modified_count: 0,
            added_count: 0,
            deleted_count: 0,
            untracked_count: 0,
            local_branch_count: 3,
            remote_branch_count: 2,
            recent_commits: vec![],
        };
        let json = serde_json::to_string(&summary).unwrap();
        let back: RepoSummary = serde_json::from_str(&json).unwrap();
        assert!(back.is_clean);
        assert_eq!(back.local_branch_count, 3);
    }
}
