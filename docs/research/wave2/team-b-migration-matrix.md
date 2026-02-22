# Team B — git2-rs Migration Matrix (Draft)

## Current State
- High shell-out usage concentrated in `crates/at-core/src/worktree_manager.rs`.
- Complex write flows (merge/rebase/cleanup) are tightly coupled to CLI semantics.

## Migration Matrix
| Operation | Current Path | Target | Risk | Recommendation |
|---|---|---|---|---|
| branch metadata | shell `git branch/show` | `git2-rs` | Low | Migrate first |
| status summary | shell `git status` | `git2-rs` status API | Low | Migrate first |
| diff stats | shell `git diff --stat` | `git2-rs` diff API | Low | Migrate first |
| worktree listing | shell `git worktree` | `git2-rs` worktree API | Medium | Migrate after read-path parity |
| fetch | shell `git fetch` | shell | Medium | Keep shell for now |
| merge | shell `git merge` | shell | High | Keep shell until exhaustive parity tests |
| rebase | shell `git rebase` | shell | High | Keep shell until explicit conflict-flow testing |
| push/pull | shell | shell | Medium/High | Keep shell for now |

## Proposed Adapter
- Add a read-only `GitReadAdapter` trait in `at-core`.
- Preserve `GitRunner` shell abstraction for write-path commands.
- Route read-only call sites through adapter behind feature flag.

## Next Steps
1. Implement read adapter prototype for `status` + `diff stat`.
2. Add parity tests comparing shell output vs `git2-rs` results on fixture repo.
3. Roll out to one production call path first.
