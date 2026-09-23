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