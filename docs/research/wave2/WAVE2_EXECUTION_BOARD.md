# Wave 2 Execution Board

Date: 2026-02-22

## Team A — Data & Storage
- **A1 OLTP/OLAP boundary** — In Progress
  - Deliverable: schema ownership + sync contract
  - ETA: 24h
- **A2 benchmark harness** — In Progress
  - Deliverable: DuckDB WASM vs API SQL benchmark spec
  - Draft: `docs/research/wave2/team-a-benchmark-plan.md`
  - ETA: 48h
- **A3 scheduler/backups** — Planned
  - Deliverable: persistent-scheduler + rustic feasibility memo
  - ETA: 48h

## Team B — Git & Workflow
- **B1 shell-out inventory** — Complete (initial)
  - Deliverable: migration matrix draft
- **B2 git2 read adapter** — Complete (first slice)
  - Deliverable: adapter trait + first call-site candidate
  - Initial scaffold: `crates/at-core/src/git_read_adapter.rs`
  - First call-site: `WorktreeManager::merge_to_main` diff read now uses `GitReadAdapter` with fallback to `GitRunner`
  - Second call-site: merge conflict-file detection now uses `GitReadAdapter::conflict_files(...)` with fallback
  - Validation: adapter/fallback tests added in `worktree_manager` test module
  - ETA: 24h
- **B3 parity tests** — In Progress
  - Deliverable: fixture-based comparison tests
  - Added: fixture-based shell adapter tests in `crates/at-core/src/git_read_adapter.rs`
  - ETA: 48h

## Team C — Inference & Agents
- **C1 claude-sdk-rs lifecycle map** — In Progress (runtime scaffold added)
  - Deliverable: session/PTY mapping doc
  - Added: `docs/research/wave2/team-c-session-pty-lifecycle.md`
  - Added: `crates/at-agents/src/claude_runtime.rs` runtime abstraction
  - ETA: 24h
- **C2 local provider profile plan** — Complete (first slice)
  - Deliverable: vllm.rs profile schema proposal
  - Draft: `docs/research/wave2/team-c-local-provider-schema.md`
  - Added: `ApiProfile::local_from_providers(...)`
  - Added: local provider config fields in `ProvidersConfig`
  - Added: local provider no-key-required semantics + tests
  - Added: `ResilientRegistry::from_config(...)` bootstrap path
  - Added: daemon startup profile bootstrap logging (embedded + standalone)
  - ETA: 24h
- **C3 candle track** — Planned
  - Deliverable: embedded-runtime feasibility note
  - ETA: 48h

## Team D — Native UX
- **D1 HIG checklist** — Complete (first slice)
  - Deliverable: Tahoe interaction checklist
  - Added: `docs/research/wave2/team-d-native-hig-checklist.md`
  - ETA: 24h
- **D2 hybrid native shell prototype plan** — In Progress (prototype hooks added)
  - Deliverable: Tauri-native chrome milestones
  - Draft: `docs/research/wave2/team-d-native-shell-milestones.md`
  - Added: `AT_NATIVE_SHELL_MACOS` feature flag in Tauri bootstrap
  - Added: runtime `data-native-shell` injection + native-shell CSS hooks
  - Added: executable verification script `tests/interactive/test_native_shell_mode.sh`
  - ETA: 24h
- **D3 Lapce/Redox watchlist** — Planned
  - Deliverable: monthly watch cadence + signals
  - ETA: 48h

## Next Integrations
1. Convert Team B/B2 into a feature-flagged code PR.
2. Convert Team C/C2 into config + provider profile PR.
3. Convert Team D/D2 into Tauri prototype branch plan.
