# Tundra Backlog (Research + Next Waves)

Last updated: 2026-09-22

## Immediate validation

- [ ] Validate `/Users/studio/rust-harness/.github/workflows/e2e-integration.yml` on `github-hosted` runner end-to-end (daemon + Ollama + `at-tui` e2e suite).
- [ ] Stand up a tagged self-hosted runner (`self-hosted,linux,x64`) for deterministic E2E and compare runtime/cost against GitHub-hosted.

## Immediate performance wave (new)

- [x] Add project-context snapshot caching with fingerprint invalidation and cache stats (`hits/misses/rebuilds`) in `at-core`. (done 2026-02-23, 50dbbfc: `at-core/src/context_engine.rs`)
- [x] Expose context-cache stats in CLI doctor output and `/api/context`. (done 2026-02-23, 46206db: `at-cli/src/commands/doctor.rs`, `at-bridge/src/intelligence_api.rs` `/api/context`)
- [x] Add repeatable perf probe script: `scripts/perf_probe.sh`. (done 2026-02-23, 46206db)
- [x] Parallelize `at-tui` refresh fan-out (`fetch_all`) to remove sequential endpoint waits. (done 2026-02-23, 46206db: `std::thread::scope` fan-out in `at-tui/src/api_client.rs`)
- [x] Add `/api/bootstrap` endpoint to replace multi-endpoint TUI refresh with one snapshot request. (done 2026-09-22, 9d2ac84: route in `at-bridge/src/http_api/mod.rs`, TUI `fetch_bootstrap` fast path)
- [x] Convert `ApiState` ID-addressed collections from `Vec<T>` to `HashMap<Uuid, T>` for O(1) lookup/update paths. (done 2026-02-27, 3381f46; finished 2026-09-22, 9d2ac84)
- [x] Replace full-list mutation broadcasts (`BridgeMessage::BeadList(beads.clone())`) with incremental events (`BeadUpdated/BeadCreated`). (done 2026-02-27, 03b0cec; 2026-09-22, 9d2ac84: `BeadCreated`/`BeadUpdated` in `at-bridge/src/protocol.rs`)
- [x] Replace queue-like `Vec::remove(0)` with `VecDeque::pop_front()` in bridge/tui/daemon/intelligence/core queue structures. (done 2026-02-25, 0859068)
- [x] N/A — KPI is in-memory, no SQL exists (removed SQL consolidation item)
- [x] Move blocking session-store FS hot paths to async-safe flow (`spawn_blocking` or DB-backed index/cache). (done 2026-02-24, 3504363: `at-core/src/session_store.rs` uses `tokio::fs` plus an LRU read cache)

## Cache and data-structure track

- [ ] Define app-level cache strategy for 2026 Rust SOTA: TinyLFU/Window-TinyLFU (`moka`) vs scan-resistant alternatives (`quick_cache`) for hot path lookups.
- [ ] Add benchmark harness comparing `std::HashMap`/`hashbrown` vs experimental open-addressing crates (`opthash`, `elastic_hash_rs`) using real Tundra access traces.
- [ ] Evaluate persistence tiers for cache: in-memory L1 + optional persistent L2 (SQLite/DuckDB/WAL-backed), with explicit TTL and invalidation rules.
- [ ] Add Miri-guided UB checks for core crates in CI (selective suite), informed by data-driven perf/robustness workflow.
- [ ] Create a perf triage playbook: `cargo miri`, criterion benches, flamegraph, and regression budgets per crate.

## Terminal + agent orchestration track

- [x] N/A — claude-sdk-rs evaluation superseded by cc-sdk/rig-core decisions (see library research notes).
- [ ] Decide on Zellij integration strategy for terminal tabs/panes vs current PTY pool design.
  - Answered by sibling code: `/Users/studio/bop/crates/bop-cli/src/workspace.rs` + `/Users/studio/bop/zellij/bop.kdl` (layout-driven panes per card) and `/Users/studio/rust-town/elixir-gastown/lib/elixir_gastown/zellij_manager.ex` (session/pane lifecycle). Port or borrow instead of designing from scratch.
- [ ] Evaluate embedding Nushell as optional shell backend for task terminals.
  - Answered by sibling code: `/Users/studio/bop/adapters/*.nu` calling convention (`adapter.nu <workdir> <prompt_file> <stdout_log> <stderr_log> [memory_out]`, see `bop/adapters/README.md`). Shelling out to `nu` scripts avoids embedding the Nushell crates.
- [ ] Formalize multi-agent queueing/backpressure model for shared local LLM + skills execution.
  - Answered by sibling code: `/Users/studio/rust-town/elixir-gastown/lib/elixir_gastown/inference_semaphore.ex` (counting semaphore, default 8 permits) plus bop per-provider cooldowns (`cooldown_seconds` in `/Users/studio/bop/crates/bop-core/src/config.rs`).

## Local inference and model runtime track

- [ ] Research integration design for `vllm.rs` as local OpenAI-compatible fallback provider.
- [ ] Evaluate `candle` as local model runtime path (Metal/CUDA/WASM) and define where it complements `vllm.rs`.
- [ ] Implement provider failover policy: cloud-first (Claude) -> local (`vllm.rs`) -> secondary provider.
  - Partially answered in-tree: `ProfileRegistry::failover_for` exists in `crates/at-intelligence/src/api_profiles.rs`. What is missing is the ordering policy (which tier comes next and when to fall back); bop's `default_provider_chain` in `/Users/studio/bop/crates/bop-core/src/config.rs` is a model for it.

## Platform and UX track

- [x] Define a single bundled-terminal profile as source of truth (no cross-terminal compatibility target).
- [x] Upgrade `/Users/studio/rust-harness/app/leptos-ui/src/components/terminal_view.rs` from line-buffer view to true terminal emulation path (xterm.js runtime bridge) with ANSI/VT fidelity.
- [x] Add bundled card-render profile: fixed tiny font metrics, locked palette, and deterministic rendering assumptions.
- [ ] Research `tui-cards` + Ratatui integration for planning-poker card rendering in bundled terminal mode.
- [x] Evaluate card-game interaction patterns from `csol` and `poker-planning-cli` for planning-poker UX flow (join/vote/reveal rounds). (done, marked 2026-09-22: `/api/kanban/poker/{start,vote,reveal,simulate,{bead_id}}` in `crates/at-bridge/src/http_api/kanban.rs`)
- [ ] Evaluate animation/perf budget for card effects in terminal stack (`ratatui`, `crossterm`, `console`, `indicatif`, `termimad`) under tiny-font profile.
- [ ] Evaluate/ship card-capable custom font strategy for bundled terminal (monochrome Unicode vs COLRv1 colored cards).
- [ ] Evaluate Lapce architecture patterns applicable to Tundra editor/terminal UX.
- [ ] Evaluate Redox OS architectural ideas relevant to process isolation and composable services.
- [ ] Evaluate Crossbeam patterns for high-throughput agent event bus and work-stealing execution.
- [ ] Add optional audio feedback (Balatro-style cues) via `rodio` with user-level mute/profile controls.
- [ ] Validate Rayon-on-WASM feasibility for frontend workloads (threading constraints, browser requirements, fallback mode).
- [ ] Add native macOS UX track (Tauri + macOS SDK/Xcode), including 2026 macOS Tahoe UI/HID guideline alignment.

## Integrations and ecosystem track

- [ ] Evaluate `rustic` for backup/snapshot strategy (project state, session snapshots, restore UX).
- [x] Adopt `git2-rs` for read-heavy git operations (diff/status/worktree inventory) while keeping shell-outs for complex porcelain flows. (done 2026-02-21, b820657; marked 2026-09-22: `crates/at-core/src/git2_ops.rs`, `git_read_adapter.rs`, `libgit2` default feature in `crates/at-core/Cargo.toml`)
- [ ] Evaluate `rustdesk` integration use-cases (remote support / collaborative operator mode) and threat model.
- [ ] Evaluate `cortex-mem` overlap with existing memory/context components and decide merge/borrow/reject path.
- [ ] Evaluate `rig` overlap with current orchestration stack and determine interoperability boundaries.
- [x] Evaluate codex CLI as an optional front-end surface (license/compliance and UX fit). (done, marked 2026-09-22: `CodexAdapter` in `crates/at-session/src/cli_adapter.rs`, detected by `GET /api/cli/available`; codex CLI is Apache-2.0)
- [ ] Evaluate `ParthJadhav/Rust_Search` for fast in-app code/document search.
- [ ] Evaluate `persistent-scheduler` as replacement/augmentation for current scheduling/queue logic.
  - Answered by sibling code: `/Users/studio/bop/crates/bop-cli/src/lock.rs` run lease (`RunLease` with PID + start time, heartbeat every 5s, stale after 30s) gives crash-safe ownership without a new dependency.

## Repo sync and governance track

- [ ] Define GitHub as source-of-truth sync policy with GitLab mirror and divergence detection rules.
  - Answered by sibling code: `/Users/studio/rust-town/fleet-ops/scripts/gh-tuple-mirror.nu`.
- [ ] Add guardrails to prevent asymmetric repo state (one-side-only commits/files).
  - Answered by sibling code: `/Users/studio/rust-town/fleet-ops/scripts/gh-tuple-mirror.nu`.
- [ ] Add automated parity report job (branches/tags/default branch HEAD parity + drift alerts).
  - Answered by sibling code: `/Users/studio/rust-town/fleet-ops/scripts/gh-tuple-mirror.nu`. Note the fleet source of truth is Gitea, not GitHub, so the source-of-truth item above needs revisiting.

## 2026-09-22 review follow-ups

- [ ] Upgrade `reqwest` to 0.13 and evaluate post-quantum (hybrid ML-KEM) TLS key exchange.
- [ ] Major bumps: `git2` and `lru` (semver-major; check API changes in `at-core`).
- [ ] Sign off on 5 MPL-2.0 transitive crates (tauri-utils pulls the `cssparser` family; `dirs` pulls `option-ext`) or replace them. MPL-2.0 is outside the MIT/BSD/Apache allow list.
- [ ] Task halt/resume: stop an executing task and resume it from its last phase.
- [ ] `TerminalView` (leptos-ui) reconnect after WebSocket drop, with growing backoff.
- [ ] `PtyHandle` `Drop`: kill and reap the child process when the handle is dropped.
- [ ] Circuit breaker half-open state: allow a limited number of probe calls before closing.
- [ ] at-bridge worktrees merge dedupe (in progress on another branch).
- [ ] Gitea client in `at-integrations` (GitHub/GitLab/Linear exist; the fleet source of truth is Gitea).
- [ ] Triage the 52 unverified high/medium review findings in `docs/reviews/2026-09-22-unverified-findings.md`.
