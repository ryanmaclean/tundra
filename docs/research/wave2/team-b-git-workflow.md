# Team B — Git & Workflow

## Scope
- `git2-rs` migration plan for read-path operations
- Shell-out git inventory and replacement matrix
- Worktree/rebase safety boundaries

## Deliverables
- Migration matrix (read/write/risk/owner)
- First candidate PR plan for low-risk read-paths
- Regression test checklist for worktree flows

## Sub-Agent Breakdown
- **B1 (Inventory):** enumerate shell-out git operations and classify read/write.
- **B2 (Adapter):** draft `git2-rs` read adapter trait and call-site mapping.
- **B3 (Parity Tests):** build macOS worktree edge-case parity tests.

## Kickoff Findings
- Current codebase shells out heavily in `worktree_manager` and merge/rebase paths.
- Best migration path is read-first (`status`, diff stats, branch metadata), preserving shell-out for complex writes initially.
- `git2-rs` should reduce parse fragility and improve typed error handling on read paths.

## Immediate Tasks
1. Inventory shell-out git usage:
   - `status`, `diff --stat`, branch metadata, worktree list
2. Mark operations by migration risk:
   - Low risk: read-only queries
   - High risk: merge/rebase/fetch/push write paths
3. Draft `git2-rs` adapter interface for read-paths.
4. Define fallback strategy:
   - Keep shell-out for rebase/merge until parity and test coverage are proven.

## Acceptance Criteria
- Operation-by-operation migration table
- Test plan for branch/worktree edge cases on macOS
- Rollback plan if libgit2 behavior diverges

## Status
- In Progress
