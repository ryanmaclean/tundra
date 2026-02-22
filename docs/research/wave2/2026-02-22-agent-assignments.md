# Wave 2 Sub-Agent Assignments (2026-02-22)

## Team A — Data & Storage
Scope:
- DuckDB WASM + SQLite split architecture
- persistent-scheduler fit for recurring jobs
- rustic-rs backup/snapshot feasibility
Deliverables:
- docs/research/wave2/team-a-data-storage.md
- architecture recommendation + benchmark plan

## Team B — Git & Workflow
Scope:
- git2-rs migration plan for read-paths
- shell-out git inventory and replacement matrix
- worktree/rebase safety boundaries
Deliverables:
- docs/research/wave2/team-b-git-workflow.md
- migration matrix (read/write, risk, owner)

## Team C — Inference & Agents
Scope:
- claude-sdk-rs integration surface for terminal agents
- local inference provider track (vllm.rs + candle)
- provider failover strategy in at-intelligence
Deliverables:
- docs/research/wave2/team-c-inference-agents.md
- phased implementation plan + API touchpoints

## Team D — Native UX (macOS Tahoe)
Scope:
- native shell strategy in Tauri (AppKit/SwiftUI bridge)
- Apple HIG alignment checklist for 2026 desktop UX
- feasibility of native controls vs web-hybrid
Deliverables:
- docs/research/wave2/team-d-native-ux.md
- implementation track with milestones

## Integration Cadence
- Day 0: research briefs and code touchpoint maps
- Day 1: prototypes (feature-flagged)
- Day 2: integration + validation

## Execution Status
- Team A brief created: `docs/research/wave2/team-a-data-storage.md`
- Team B brief created: `docs/research/wave2/team-b-git-workflow.md`
- Team C brief created: `docs/research/wave2/team-c-inference-agents.md`
- Team D brief created: `docs/research/wave2/team-d-native-ux.md`
