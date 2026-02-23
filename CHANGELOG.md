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