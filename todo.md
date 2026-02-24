# Tundra Backlog (Research + Next Waves)

Last updated: 2026-02-24

## Immediate validation

- [ ] Validate `/Users/studio/rust-harness/.github/workflows/e2e-integration.yml` on `github-hosted` runner end-to-end (daemon + Ollama + `at-tui` e2e suite).
- [ ] Stand up a tagged self-hosted runner (`self-hosted,linux,x64`) for deterministic E2E and compare runtime/cost against GitHub-hosted.

## Immediate performance wave (new)

- [x] Add project-context snapshot caching with fingerprint invalidation and cache stats (`hits/misses/rebuilds`) in `at-core`.
- [x] Expose context-cache stats in CLI doctor output and `/api/context`.
- [x] Add repeatable perf probe script: `/Users/studio/rust-harness/scripts/perf_probe.sh`.
- [x] Parallelize `at-tui` refresh fan-out (`fetch_all`) to remove sequential endpoint waits.
- [ ] Add `/api/bootstrap` endpoint to replace multi-endpoint TUI refresh with one snapshot request.
- [ ] Convert `ApiState` ID-addressed collections from `Vec<T>` to `HashMap<Uuid, T>` for O(1) lookup/update paths.
- [ ] Replace full-list mutation broadcasts (`BridgeMessage::BeadList(beads.clone())`) with incremental events (`BeadUpdated/BeadCreated`).
- [ ] Replace queue-like `Vec::remove(0)` with `VecDeque::pop_front()` in bridge/tui/daemon/intelligence/core queue structures.
- [ ] Consolidate KPI snapshot SQL from multiple COUNT queries into one grouped aggregate query.
- [ ] Move blocking session-store FS hot paths to async-safe flow (`spawn_blocking` or DB-backed index/cache).

## Cache and data-structure track

- [ ] Define app-level cache strategy for 2026 Rust SOTA: TinyLFU/Window-TinyLFU (`moka`) vs scan-resistant alternatives (`quick_cache`) for hot path lookups.
- [ ] Add benchmark harness comparing `std::HashMap`/`hashbrown` vs experimental open-addressing crates (`opthash`, `elastic_hash_rs`) using real Tundra access traces.
- [ ] Evaluate persistence tiers for cache: in-memory L1 + optional persistent L2 (SQLite/DuckDB/WAL-backed), with explicit TTL and invalidation rules.
- [ ] Add Miri-guided UB checks for core crates in CI (selective suite), informed by data-driven perf/robustness workflow.
- [ ] Create a perf triage playbook: `cargo miri`, criterion benches, flamegraph, and regression budgets per crate.

## Terminal + agent orchestration track

- [ ] Evaluate `claude-sdk-rs` integration into terminal agent sessions (session lifecycle, streaming, cancellation, retry semantics).
- [ ] Decide on Zellij integration strategy for terminal tabs/panes vs current PTY pool design.
- [ ] Evaluate embedding Nushell as optional shell backend for task terminals.
- [ ] Formalize multi-agent queueing/backpressure model for shared local LLM + skills execution.

## Local inference and model runtime track

- [ ] Research integration design for `vllm.rs` as local OpenAI-compatible fallback provider.
- [ ] Evaluate `candle` as local model runtime path (Metal/CUDA/WASM) and define where it complements `vllm.rs`.
- [ ] Implement provider failover policy: cloud-first (Claude) -> local (`vllm.rs`) -> secondary provider.

## Platform and UX track

- [x] Define a single bundled-terminal profile as source of truth (no cross-terminal compatibility target).
- [x] Upgrade `/Users/studio/rust-harness/app/leptos-ui/src/components/terminal_view.rs` from line-buffer view to true terminal emulation path (xterm.js runtime bridge) with ANSI/VT fidelity.
- [x] Add bundled card-render profile: fixed tiny font metrics, locked palette, and deterministic rendering assumptions.
- [ ] Evaluate/ship card-capable custom font strategy for bundled terminal (monochrome Unicode vs COLRv1 colored cards).
- [ ] Evaluate Lapce architecture patterns applicable to Tundra editor/terminal UX.
- [ ] Evaluate Redox OS architectural ideas relevant to process isolation and composable services.
- [ ] Evaluate Crossbeam patterns for high-throughput agent event bus and work-stealing execution.
- [ ] Add optional audio feedback (Balatro-style cues) via `rodio` with user-level mute/profile controls.
- [ ] Validate Rayon-on-WASM feasibility for frontend workloads (threading constraints, browser requirements, fallback mode).
- [ ] Add native macOS UX track (Tauri + macOS SDK/Xcode), including 2026 macOS Tahoe UI/HID guideline alignment.

## Integrations and ecosystem track

- [ ] Evaluate `rustic` for backup/snapshot strategy (project state, session snapshots, restore UX).
- [ ] Adopt `git2-rs` for read-heavy git operations (diff/status/worktree inventory) while keeping shell-outs for complex porcelain flows.
- [ ] Evaluate `rustdesk` integration use-cases (remote support / collaborative operator mode) and threat model.
- [ ] Evaluate `cortex-mem` overlap with existing memory/context components and decide merge/borrow/reject path.
- [ ] Evaluate `rig` overlap with current orchestration stack and determine interoperability boundaries.
- [ ] Evaluate codex CLI as an optional front-end surface (license/compliance and UX fit).
- [ ] Evaluate `ParthJadhav/Rust_Search` for fast in-app code/document search.
- [ ] Evaluate `persistent-scheduler` as replacement/augmentation for current scheduling/queue logic.

## Repo sync and governance track

- [ ] Define GitHub as source-of-truth sync policy with GitLab mirror and divergence detection rules.
- [ ] Add guardrails to prevent asymmetric repo state (one-side-only commits/files).
- [ ] Add automated parity report job (branches/tags/default branch HEAD parity + drift alerts).
