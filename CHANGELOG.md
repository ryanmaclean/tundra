## Unreleased — 2026-09-22 review wave

Fixes from the 2026-09-22 multi-agent review. Unverified findings still to triage are listed in `docs/reviews/2026-09-22-unverified-findings.md`; open follow-ups are in `todo.md`.

### Security and auth

- Clients send the daemon API key: CLI (b434777), TUI (d905455), web UI and WebSocket (9c4da2a, 5cfa5c7), Tauri (aff0b04, a93c9e0). CLI and TUI share one discovery path: URL from `~/.auto-tundra/daemon.lock`, key from `AUTO_TUNDRA_API_KEY` or the separate `~/.auto-tundra/daemon.key` (7c1641c, 68b87ae).
- Rate limiter keyed per client (peer address, loopback exempt) and only spends tokens when every tier admits (f318199, 7200e80).
- One origin allowlist for CORS and WebSockets (118eb8e).
- Task descriptions sanitized before `inner_html` (stored XSS) and a Content-Security-Policy for the Tauri webview (d3e915a, df08ca4).
- MCP bead tools go through the REST bead services and their validation; unknown MCP sessions are rejected before a tool runs (374bd5f, e80ae05).
- GitLab MR review fails closed on API errors (7355784).
- `EncryptionKey` bytes are actually zeroized (59e53e3).

### Runtime

- Notifications recorded once per event, not once per WebSocket client (bd254ea).
- `PtyPool` attached in the daemon by default; terminal WebSockets close on a dead client, not on a quiet PTY (477e803).
- PTY children spawn in the requested cwd; pool slots reserved atomically (eed13f6, 1115405, fa54f6a).
- Executor no longer spins, enforces its timeout, and kills and releases agent processes (5600fdb).
- Claude CLI: no more `--thinking-budget`; thinking maps to `--effort` (8f84de6).
- `TokenCache` get/put ABBA deadlock removed (759466b).
- Profile selection: the implicit local profile no longer shadows keyed cloud providers (49dca05).
- Settings saved atomically; settings endpoints return 409 instead of overwriting an invalid settings file (10e74bb, d34debe). The Settings page PATCHes only changed fields (3e9be51, 2c4acb5).
- Context steering loads project rules in every phase (6ebbe79).
- `merge_to_main` targets `main` and handles tasks with no changes (b85de91).
- `at exec --wait` stops on the phases the server actually emits (ca6ee1b).
- Every `AgentExecutor` run now registers a live agent per spawned CLI process and heartbeats `Agent.last_seen` on the event bus (`AgentCreated` → throttled `agent_heartbeat` → `AgentUpdated(Stopped)` on exit/timeout/abort/cancel); the daemon's `agent_registry` applies these to `ApiState.agents`. `[daemon.patrol]` stuck-agent detection is now **enabled by default**, proven by a daemon test that a heartbeating-but-silent executor survives patrol while a non-heartbeating agent is force-killed.

### Integrations and UI

- GitHub issue listing paginates and drops pull requests (8d16bd4).
- TUI: UTF-8-safe, width-aware truncation; every bead/agent/convoy status mapped; new Attention kanban column; unsupported commands report `not_implemented` (9b4d481, 6b40c30, 71ddcfa, 121fc7a).
- Web UI: WebSocket reconnect backoff grows; page intervals cleared on unmount (0d18d0b, 5db15ad).

### Build and tests

- Clippy clean under `-D warnings` (96b2383, 1565284).
- License allow list tightened (50b734b).
- OpenSSL removed: reqwest uses rustls (d624fa5).
- Advisories updated via `cargo update` (5e02b5a).
- at-tui e2e tests skip cleanly when no daemon is running (d1ff75f).
- Test count: 2,944 (nextest, workspace excluding at-tauri and at-leptos-ui).

### Ports (2026-09-23)

Six port branches merged with `--no-ff`: housekeeping (37843d1), scheduler (217b938), merge-gate (65c3ea5), output-security (e0f85b3), api-surface (6258eb0), deps-major (ff79818).

- API surface: routes nested into per-domain sub-routers with a route catalog at `GET /api/catalog` and `GET /api/v1/catalog` (269a1bb, 0e1364e, d104284, 7fa3940). The catalog is served without the API key but rate limited (loopback exempt) and reports `auth: none` for itself; every other route still requires the key (fae6cfe). Unused `command_registry` and `commands` modules removed (2c636fc).
- Merge gate in front of `WorktreeManager::merge_to_main`: acceptance criteria produce a `MergeGateReport`, and the worktree merge endpoint returns 409 with the report when the gate refuses (6d3cee2, b429685). The orchestrator gates the Merging phase and sends gate failures to the fix loop (39be659).
- Scheduler ranks backlog beads by a priority score (780f0db). Patrol detects and force-kills stuck agents via `[daemon.patrol]`, disabled by default (9da5e86, d3fd0ba, a291334, f7cdfdc).
- Output guard (credential redaction, prompt-injection blocking) and a hash-chained audit log for approval decisions (b27c3ca, 1c10f7d). Outbound PR, MR, issue and Linear payloads are screened, and notifications are redacted before they are recorded (32c938b, 6e168b1). The docs scan runs with no allowlist (1111a98).
- Dependencies: reqwest 0.13 on rustls with aws-lc-rs and post-quantum key exchange (06769d5, a0d6ca0); git2 0.21, octocrab 0.54, tauri 2.11; RustSec advisories cleared (c42bafa). `cargo deny` passes. The lru 0.12.5 that ratatui 0.29 pulls in stays ignored: RUSTSEC-2026-0002 was already ignored, RUSTSEC-2026-0253 is new.
- Datadog API key removed from the profiling docs and scripts (6662e99).
- Test count after the ports: 3,024 (nextest, workspace excluding at-tauri and at-leptos-ui).

## 1.0.0 - Agent Orchestration & Comprehensive Testing

### New Features

- Orchestrator with graph memory and steered task runner for advanced agent coordination

- RLM (Recursive Language Model) patterns enabling sophisticated agent orchestration workflows

- Agent teams with API profiles, spec pipeline, runners, MCP integration, and context steering capabilities

- Context engine, agent registry, graceful shutdown, and trace propagation for system reliability

- Comprehensive error handling with tool_use_error management, token optimization, model routing, and LETS metrics

- LLM observability suite with Datadog profiling integration for production monitoring

- Local provider settings and GitHub auto-fix toggle with MCP agent grid styling

- Ollama integration with CLI workflow, skill queueing, and tundra CLI wrapper

- Terminal emulator with settings persistence and GitHub synchronization

- Task wizard, settings UI, and agent executor runtime

- AI-powered intelligence features with LLM provider abstraction and intelligence API

- Redesigned Leptos UI with interactive kanban board, sidebar navigation, and TUI dashboard

- Full auto-tundra workspace setup with 8 crates and 116 passing tests

- End-to-end testing framework with review_bead command

### Improvements

- Dynamic port allocation with lockfile-based discovery for multi-instance support

- Standardized UI patterns across all frontend pages for consistency

- Issue stat icons for enhanced visual feedback

- Agent page banner styling and layout refinements

- Isolated test server settings to prevent cross-test race conditions

- Visual parity fixes for kanban, insights, and CSS components

### Bug Fixes

- Ideation API fallback when no LLM provider is configured

- WebSocket connection error handling and stability improvements

- UI page styling corrections and ideation test compilation issues

- Dashboard role formatting and unused import cleanup

### Testing

- Wave 1-4 exhaustive test coverage with 827 new tests spanning agents, LLM, notifications, pipeline, roadmap, changelog, worktrees, GitHub, settings, auth, task details, API, lifecycle, files, and QA workflows

### Security

- Removed all API key storage from configuration; now uses environment variables exclusively

---

## What's Changed

- feat: implement comprehensive LLM observability with Datadog profiling by @ryanlmacLean in 44bec23
- feat: add local provider settings, GitHub auto-fix toggle, and MCP agent grid styles by @ryanlmacLean in 913bb54
- feat: orchestrator, graph memory, and steered task runner by @ryanlmacLean in aa9c3ff
- feat: add RLM (Recursive Language Model) patterns for agent orchestration by @ryanlmacLean in 24ae00f
- feat: agent teams, API profiles, spec pipeline, runners, MCP, context steering, AGENTS.md by @ryanlmacLean in 320e14c
- feat: add context engine, agent registry, graceful shutdown, and trace propagation by @ryanlmacLean in a1a5664
- feat: add tool_use_error handling, token optimization, model routing, and LETS metrics by @ryanlmacLean in 08118b9
- security: remove all API key storage from config, use env vars only by @ryanlmacLean in 23f4da1
- test: add Wave 4 exhaustive tests — Task detail, API, lifecycle, files, QA (211 new) by @ryanlmacLean in 028a152
- test: add Wave 3 exhaustive tests — agents, LLM, notifications, pipeline, harness (253 new) by @ryanlmacLean in 1f1c7fb
- test: add Wave 2 exhaustive tests — Roadmap, Changelog, Worktrees, GitHub, Settings, Auth (184 new) by @ryanlmacLean in 872042e
- test: add Wave 1 exhaustive tests — Kanban, Terminals, Insights, Ideation (189 new tests) by @ryanlmacLean in fcf9932
- feat: implement Sprint 9 — agent roles, tool approval, notifications, telemetry, session persistence by @ryanlmacLean in d25eab8
- feat: implement Sprint 8 — LLM type unification, CLI commands, API auth, frontend wiring by @ryanlmacLean in 27f5efe
- feat: implement Sprint 7 — LLM provider abstraction, AI-powered intelligence, intelligence API by @ryanlmacLean in a8cc677
- feat: implement Sprint 6 — terminal emulator, settings persistence, GitHub sync by @ryanlmacLean in a42f63f
- feat: implement Sprint 5 — task wizard, settings UI, agent executor runtime by @ryanlmacLean in e5436de
- feat: implement Sprint 4 — AI intelligence, platform features, live dashboard by @ryanlmacLean in 5b474c6
- feat: implement Sprint 1-3 — task pipeline, HTTP/WS API, GitHub integration, UI enhancements by @ryanlmacLean in 2d4fa3d
- feat: redesign Leptos UI with Auto Claude-style kanban board and interactive sidebar by @ryanlmacLean in 50446ca
- feat: add end-to-end tests and review_bead command by @ryanlmacLean in 23f8354
- chore: fix gitignore to exclude all target dirs and remove tracked build artifacts by @ryanlmacLean in 12d8358
- feat: add TUI dashboard, Tauri backend, and Leptos WASM frontend (Phase 4+5) by @ryanlmacLean in c259b9b
- feat: implement full auto-tundra workspace - 8 crates, 116 tests passing by @ryanlmacLean in f2f5e7e
- chore: initialize auto-tundra workspace with 8 crates by @ryanlmacLean in 941aec4
- fix: update dashboard role formatting and remove unused import by @ryanlmacLean in 377dab2
- fix: ideation API to fall back when no LLM provider configured by @ryanlmacLean in 292858f
- fix: UI page styles and ideation test compilation by @ryanlmacLean in 2d95fbe
- fix: websocket connection errors by @ryanlmacLean in 36d5760
- fix: address websocket connection errors by @ryanlmacLean in a459888
- fix: visual parity fixes for kanban, insights, and CSS by @ryanlmacLean in cf30d5f
- Dynamic port allocation with lockfile-based discovery by @ryanlmacLean in d2ab140
- Integrate Ollama CLI workflow by @ryanlmacLean in 310ca9d
- Configure Ollama skill workflow by @ryanlmacLean in 56b61b6
- Add Ollama skill queueing workflow by @ryanlmacLean in 622bb23
- Configure CLI skill with Ollama by @ryanlmacLean in 6a0eef2
- Add skill wrapper for tundra CLI by @ryanlmacLean in b820657
- Isolate settings per test server to avoid cross-test races by @ryanlmacLean in d3a112c
- Update leptos UI pages by @ryanlmacLean in 8312087
- Standardize UI patterns across all frontend pages by @ryanlmacLean in 7aeb6c9
- Style agents page banners by @ryanlmacLean in 8c3c090
- Add issue stat icons by @ryanlmacLean in 52fdf5d
- Review listed file changes by @ryanlmacLean in 039fd92
- Review recent leptos-ui changes by @ryanlmacLean in a304f4c
- Review datadog profiling updates by @ryanlmacLean in 8128aa1
- Summarize recent change requests by @ryanlmacLean in a6d0e0f
- Summarize recent repo updates by @ryanlmacLean in 176c89a

## Thanks to all contributors

@ryanlmacLean
## Unverified-findings sweep (2026-09-23)

Merged reviewed branches from the 2026-09-22 unverified-findings list (see `docs/reviews/2026-09-22-unverified-findings.md`) after per-branch approval. Fixed:

- #3 — WebSocket reconnect backoff never grows: every close retries after 1s, forever
- #4 — Timer closures outlive their pages: beads auto-refresh keeps polling and writing global state after navigating away
- #9 — New public rig-core `shell_exec` tool runs arbitrary `sh -c` with no gate, is unused, and pulls rig-core into daemon/tauri
- #10 — at-api-types defines ApiStack but the bridge has no /api/stacks route; UI silently shows demo stacks
- #12 — MCP SSE sessions are force-closed after exactly 1 hour, and abandoned sessions stay in memory for that hour
- #13 — MCP transport: tools run before the session is checked, and the shipped Claude Code config cannot authenticate
- #14 — MCP SSE tools/call skips bead lifecycle checks, input sanitization and event publishing that the REST API enforces
- #15 — Projects changed from Vec to HashMap, so list order and pagination are no longer stable and deleting the active project activates an arbitrary one
- #16 — Vec to HashMap conversion makes project and attachment listing and pagination order random
- #17 — Worktree merge and resolve endpoints ignore git exit codes and report success; delete matches by substring and runs `git worktree remove --force`
- #18 — POST /api/ideation/generate holds the ideation_engine write lock across the LLM network call
- #19 — GET /api/changelog?source=tasks mutates state: every call appends a duplicate entry and returns ever-growing markdown
- #20 — Blocking flume::Sender::send on the bounded (256) PTY stdin channel inside an async task can wedge a tokio worker and the terminal
- #30 — Orchestrator slices agent output at byte 1000 and panics on multi-byte UTF-8
- #31 — Circuit breaker HalfOpen state admits unlimited concurrent calls
- #32 — RateLimitConfig with a zero rate panics on the first rejected check (Duration::from_secs_f64(inf))
- #33 — No HTTP timeouts on any GitHub, GitLab or Linear client, so a stalled upstream hangs request handlers indefinitely
- #35 — Linear list_issues is hard-capped at 50 with no pagination, so bridge offset/limit can't reach past item 50
- #36 — Linear sync push overwrites remote issue titles with their own IDs and descriptions with a fixed string
- #37 — Cost lookup needs an exact model-name match and silently returns $0; the renamed pricing rows also keep the old, wrong prices
- #38 — Ideation's text fallback turns truncated or non-JSON LLM output into junk ideas like "{" and "\"ideas\": ["
- #39 — The cloud provider HTTP clients have no timeout, so a stalled Anthropic or OpenAI connection hangs the caller forever
- #48 — Uncommitted bootstrap change runs fetches one after another instead of in parallel

Skipped (blocked, worktrees left in place for inspection — see report): findings surfaced on `unv/at-bridge` (worktree delete `force=false` default not wired to the only caller; `terminal_conns` leak on direct terminal delete) and `unv/at-tui` (bootstrap-success path still fires the 3 fallback fetches unconditionally, defeating the fast path).

Verification after merge: `cargo check --workspace --exclude at-tauri --exclude at-leptos-ui` clean; `cargo clippy --workspace --exclude at-tauri --exclude at-leptos-ui -- -D warnings` clean; `cargo nextest run --workspace --exclude at-tauri --exclude at-leptos-ui` — 3096 passed, 0 failed, 0 skipped; `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok; `cargo check -p at-leptos-ui --target wasm32-unknown-unknown` clean.
