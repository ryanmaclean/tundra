# Team A — Benchmark Plan (A2)

## Goal
Compare frontend analytical query performance:
- DuckDB WASM in-browser
- API-side SQL aggregation (SQLite daemon endpoints)

## Dataset Sizes
- S: 10k events
- M: 100k events
- L: 1M events

## Query Suite
1. Time-window grouped counts (`hour/day/week`)
2. Top-N agents by completed tasks
3. Cost rollup by provider/model/time range
4. Error-rate trend (success/failure time series)

## Metrics
- Query latency (P50, P95, max)
- Browser memory usage
- Payload transfer size from daemon to browser
- Cold-start and warm-cache timings

## Success Criteria
- DuckDB WASM provides lower P95 for multi-aggregation dashboard queries.
- Memory remains within acceptable UI budget for M dataset.
- Fallback path remains available when browser analytics is disabled.
