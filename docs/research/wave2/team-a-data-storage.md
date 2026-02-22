# Team A — Data & Storage

## Scope
- DuckDB WASM + SQLite split architecture
- `persistent-scheduler` fit for recurring jobs
- `rustic-rs` backup/snapshot strategy

## Deliverables
- Data plane proposal for analytics vs transactional state
- Benchmark plan (`DuckDB WASM` vs API-side SQL rollups)
- Backup/restore design note for task/worktree metadata

## Sub-Agent Breakdown
- **A1 (OLTP/OLAP Boundary):** define schema ownership and synchronization contracts.
- **A2 (Bench & Query Suite):** implement benchmark harness and query corpus.
- **A3 (Scheduler/Backups):** evaluate `persistent-scheduler` + `rustic-rs` recovery track.

## Kickoff Findings
- `DuckDB WASM` is production-ready for browser analytics execution and fits client-side OLAP usage.
- Existing daemon SQLite is already optimized for transactional updates and should remain system-of-record.
- `rustic-rs` remains high-capability but adds significant dependency/operational overhead; treat as optional backup module.

## Immediate Tasks
1. Define source-of-truth boundaries:
   - `SQLite`: daemon transactional state (`tasks`, `agents`, `sessions`, config)
   - `DuckDB WASM`: frontend OLAP (`costs`, `activity`, `insights` rollups)
2. Draft ingestion model:
   - API export endpoints for analytics snapshots
   - Browser-side import into DuckDB
3. Evaluate scheduler requirements:
   - nightly snapshots
   - periodic sync jobs
4. Validate `rustic-rs` overhead and restoration UX.

## Acceptance Criteria
- Clear architecture table with read/write ownership
- P50/P95 benchmark plan with datasets and query suite
- Risk list with fallback path (SQLite-only mode)

## Status
- In Progress
