# Team A — OLTP/OLAP Boundary Draft

## Decision
- **SQLite (daemon)** remains source-of-truth for transactional state.
- **DuckDB WASM (frontend)** is proposed for analytics and ad-hoc aggregation.

## Ownership Split
| Domain | System | Write Path | Read Pattern |
|---|---|---|---|
| Tasks, agents, sessions | SQLite | daemon/API | low-latency CRUD |
| Settings, credentials refs | SQLite | daemon/API | config reads |
| Cost rollups, activity aggregates | DuckDB WASM | browser import | analytical scans/group-bys |
| Insights snapshots | DuckDB WASM | browser import | chart/table aggregations |

## Sync Contract (Proposed)
1. Daemon exports analytics snapshots via API endpoints (append-only/event-window).
2. Frontend imports snapshots into DuckDB tables.
3. UI queries DuckDB for heavy aggregations; falls back to API if unavailable.

## Risk Notes
- Browser memory pressure for very large datasets.
- OPFS persistence tradeoffs across browsers.
- Consistency lag between daemon writes and browser analytics imports.

## Next Steps
- Define dataset schemas for `cost_events`, `task_events`, `agent_activity`.
- Create benchmark harness for 10k / 100k / 1M row scenarios.
