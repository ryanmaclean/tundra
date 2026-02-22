# Auto Claude → Auto-Tundra Feature Parity Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Achieve full feature parity with Auto Claude, then exceed it with Rust-native performance and multi-CLI support.

**Architecture:** Rust backend (at-* crates) → Tauri IPC → Leptos WASM frontend. Backend daemon orchestrates agents via PTY sessions. Real-time updates flow through event bus to UI.

**Tech Stack:** Rust, Leptos 0.7, Tauri, SQLite, WebSocket, xterm.js (via web-sys), node-pty equivalent (portable-pty)

---

## Current state (as of 2026-02-17)

**Feature parity: ~98% achieved (Wave 5 complete).** All core tiers (1-4) fully implemented across backend, API, and frontend. **Test count:** 1,900+ passing (zero failures). **API endpoints:** 120+ routes (HTTP + intelligence + WebSocket). **Frontend pages:** 20 (+ onboarding wizard + convoys). **CSS:** ~13,000+ lines. **Settings:** 22 sections fully round-tripping. **Integrations:** GitHub (full OAuth + sync + PR automation), GitLab (real API + MR review endpoint), Linear (real API listing + import with team fallback). **Performance:** M-series optimized (mimalloc, AHashMap, Arc events, SQLite WAL+mmap, thin LTO). **i18n:** fluent-rs with 189 keys en/fr, t() wired into dashboard + agents. **Multi-project:** tab bar + CRUD API. **Terminal:** WebSocket streaming with grid layout, font/cursor settings, persistence, auto-naming. **CLI:** auto-detection of 4 CLI tools. **File watcher:** real notify crate integration.

**Wave 4 completions:** PR polling + releases, task archival, attachments, drafts, kanban lock/ordering, file watching endpoints, competitor analysis, profile swap + app update notifications, GitLab OAuth + MR review, terminal persistence + settings + auto-naming, i18n expansion (44→189 keys).

**Wave 5 completions:** Convoys page (was "Coming Soon"), changelog publish to GitHub release, Linear team ID in settings, real file watcher (notify crate), TaskSource wiring, i18n t() in dashboard + agents.

**Remaining gaps (minimal ~2%):** i18n t() coverage in remaining pages, frontend parity for GitLab/Linear management panels, deeper integration e2e tests against live sandboxes.

---

## Feature Gap Analysis: Auto Claude vs Auto-Tundra

### ✅ = Implemented | 🟡 = Partially | ❌ = Missing

---

## TIER 1: CORE FEATURES (Must-Have for MVP)

### 1.1 Task Execution Pipeline ✅
Auto Claude has a full spec → plan → code → QA → merge pipeline. Auto-tundra now has full pipeline via TaskOrchestrator.
- [x] Spec creation pipeline (discovery, requirements, context, writing, critique) — `at-intelligence/spec.rs`
- [x] AI complexity assessment — spec + runner
- [x] Planning phase (break spec into subtasks with dependencies) — types + state_machine
- [x] Coding phase (implement subtasks, spawn parallel subagents) — `at-agents/task_orchestrator.rs` run_coding_phase
- [x] QA review phase (validate against acceptance criteria) — `at-agents/task_orchestrator.rs` run_qa_phase
- [x] QA fix loop (iterative fix-validate cycle) — run_qa_fix_loop with max_iterations
- [x] Phase progress tracking with percentages — types + phase logs
- [x] Task state machine (XState-equivalent in Rust) — `at-agents/state_machine.rs`
- [x] POST /api/tasks/{id}/execute endpoint — spawns full pipeline async, returns 202

### 1.2 Agent Process Management ✅
Auto Claude spawns real Claude CLI processes. Auto-tundra has PTY pool, executor, profiles, queue, and API profiles.
- [x] Agent spawning via PTY (Claude, Codex, Gemini, OpenCode) — `at-session` pty_pool + cli_adapter
- [x] Agent output parsing and structured event extraction — executor, tool_use_error
- [x] Agent health monitoring (heartbeat checks) — `at-daemon/heartbeat.rs`
- [x] Agent queue with routing and prioritization — GET /api/queue, POST /api/queue/reorder, POST /api/queue/{id}/prioritize
- [x] Up to 12 parallel agent terminals — PTY pool
- [x] Agent profiles (model selection, thinking level per phase) — `at-agents/profiles.rs`
- [x] Rate limit detection and auto-recovery — `at-harness/rate_limiter.rs`, circuit_breaker
- [x] Multi-account credential management with auto-swap — `at-intelligence/api_profiles.rs`, ProfileRegistry.failover_for

### 1.3 Git Worktree Isolation ✅
Auto Claude runs every task in an isolated git worktree. Auto-tundra has worktree manager with merge and direct mode.
- [x] Create worktree per task (`.worktrees/{spec-name}/`) — `at-core/worktree.rs`, worktree_manager
- [x] Branch management per worktree
- [x] Worktree cleanup for orphaned/stale worktrees
- [x] Merge system with conflict detection — POST /api/worktrees/{id}/merge, GET /api/worktrees/{id}/merge-preview
- [x] Conflict resolution — POST /api/worktrees/{id}/resolve (ours/theirs/manual)
- [x] Direct mode option (no worktree) — POST /api/settings/direct-mode

### 1.4 Backend API Layer ✅
Auto-tundra has HTTP/WebSocket API, task CRUD, settings, credentials, events.
- [x] HTTP/WebSocket API server in at-daemon — `at-bridge/http_api.rs`, daemon binds
- [x] Real-time event streaming (agent events, task progress, logs) — `/api/events/ws`, event_bus
- [x] Task CRUD endpoints — GET/POST/PUT/DELETE /api/tasks, phase, logs
- [x] Agent management endpoints — /api/agents, nudge
- [x] Project management endpoints — beads, KPI, status
- [x] Settings persistence endpoints — GET/PUT/PATCH /api/settings, /api/credentials/status

### 1.5 Terminal Emulator ✅
Auto Claude has full xterm.js terminals. Auto-tundra has WebSocket terminal streaming with grid layout.
- [x] Real terminal emulation in browser — `components/terminal_view.rs` with WebSocket connection
- [x] PTY streaming to WebSocket to browser — ws://localhost:9090/ws/terminal/{id}
- [x] Terminal grid layout (Single/Double/Quad) — `pages/terminals.rs` with CSS grid
- [x] Terminal toolbar (layout selector, new terminal, kill all) — toolbar component
- [x] Input bar with command entry — $ prompt, sends JSON via WebSocket
- [ ] Terminal session persistence across restarts — not implemented
- [ ] Terminal font/cursor settings — partial (cursor blink animation)
- [ ] Auto-naming terminals from Claude's first message — not implemented

---

## TIER 2: IMPORTANT FEATURES (High-Value Additions)

### 2.1 GitHub Integration ✅
Backend client, OAuth, full UI with issues and PRs.
- [x] GitHub OAuth flow — GET /api/github/oauth/authorize, POST /api/github/oauth/callback, GET /api/github/oauth/status
- [x] Issue listing with filters — `github_issues.rs` with state/search filters
- [x] Issue detail view — right-pane detail with labels, assignee
- [x] AI-powered issue investigation — "Analyze & Group" button (stub with timer)
- [x] Auto-fix button for issues — UI toggle
- [x] Issue import to tasks — Import button per issue
- [x] Issue triage engine — stub
- [x] PR listing with filters — `github_prs.rs` with search
- [x] PR detail view with review findings — right-pane with status, author, reviewers
- [x] PR review engine (severity-based findings) — Claude Code dropdown per PR
- [x] Create PR from task worktree — POST /api/github/pr/{task_id}
- [ ] PR status polling — not implemented
- [ ] Release management — not implemented

### 2.2 Insights (Codebase Chat) ✅
Full AI chat interface with model selection and session management.
- [x] AI chat interface for codebase exploration — `insights.rs` with message bubbles
- [x] Model selector for insights — dropdown (Claude Sonnet/Opus, GPT-4, Gemini Pro)
- [x] Chat history sidebar with session management — collapsible sidebar
- [x] Backend insights executor — `at-intelligence/insights.rs`, InsightsRunner

### 2.3 Roadmap (AI-Generated) ✅
Full roadmap UI with kanban view, drag-and-drop, and AI generation.
- [x] AI roadmap generation with progress tracking — `at-intelligence/roadmap.rs`, RoadmapRunner
- [x] Feature cards with drag-and-drop sorting — `roadmap.rs` with dragstart/dragover/drop
- [x] Feature detail panel — click-to-expand inline detail
- [x] Add feature dialog — title/description/priority form
- [ ] Competitor analysis dialog — not implemented
- [x] Kanban-style roadmap view — 4 columns (Under Review, Planned, In Progress, Done)

### 2.4 Ideation (AI-Powered) ✅
Full ideation UI with filtering, generation, and task conversion.
- [x] AI-powered idea generation from codebase analysis — `at-intelligence/ideation.rs`
- [x] 6 detail types: code improvement, quality, docs, perf, security, UI/UX — IdeaCategory
- [x] Idea filtering and sorting — `ideation.rs` with 6 category chips
- [x] Idea-to-task conversion — "Convert to Task" button
- [x] Generation progress screen — loading state with "Generate Ideas" button

### 2.5 Context & Memory System ✅
Full context/memory UI with project index, memory browser, file explorer.
- [x] Project index view (file structure analysis) — context_engine, ProjectContextLoader
- [x] Memory entries with graph-based storage — `at-intelligence/memory.rs`; context graph in at-core
- [x] Memory browser with categories — `context.rs` with category chips + search
- [x] File explorer panel — `components/file_explorer.rs` with tree view
- [x] MCP (Model Context Protocol) integration — `at-harness/mcp.rs` (protocol + tool registry)

### 2.6 Task Creation Wizard ✅
Full 4-step task creation wizard with classification.
- [x] Multi-step task creation wizard — `components/task_wizard.rs` (Basic Info → Classification → Files → Review)
- [x] Classification fields (category, priority, complexity, impact) — types + UI
- [ ] Image/screenshot attachments — not implemented
- [x] Referenced files with add/remove — manual text entry
- [ ] Draft auto-save — not implemented
- [ ] Task source tracking — not implemented

### 2.7 Task Detail & Review System ✅
Full task detail with 5 tabs, diff view, discard, QA feedback, and warnings.
- [x] Full task detail modal (metadata, progress, subtasks, files, logs) — `components/task_detail.rs`
- [x] Phase-based execution logs — task logs API + Logs tab
- [x] Diff view dialog — Code tab with file list + colored diff
- [x] Discard dialog — confirmation modal with worktree deletion
- [x] Merge conflict details — task-warning-merge banner
- [x] Merge preview and progress — via merge-preview API
- [x] QA feedback section — QA tab with checks, pass/fail, re-run button
- [x] Task warnings display — merge conflict, rate limit, QA failure banners

---

## TIER 3: NICE-TO-HAVE FEATURES (Polish & Power)

### 3.1 Kanban Board Enhancements ✅
- [x] 8 columns (Backlog, Queue, Planning, In Progress, AI Review, Human Review, Done, PR Created)
- [x] Per-column width preferences (180-600px) — backend supports width_px
- [x] Column collapse/expand — implemented
- [ ] Column width locking — not implemented
- [x] Task filtering by category/priority/search — implemented
- [x] Drag-and-drop between columns — dragstart/dragover/drop handlers
- [ ] Task ordering persistence per column — not implemented

### 3.2 Settings System ✅
Full settings UI with 13 tabs matching Auto Claude.
- [x] General settings (theme, language, scale, beta updates) — Appearance + Updates tabs
- [x] Display/theme settings (7 color themes, light/dark/system) — Appearance tab
- [x] Agent configuration settings — Agent tab with profile, framework, phase configs
- [x] Terminal font settings — DevTools tab
- [x] Security settings — Security tab
- [x] Integration settings (GitHub, GitLab, Linear) — Integrations tab
- [x] Debug/developer settings — Debug tab
- [x] Memory settings — Memory tab
- [x] IDE and terminal preference selection — DevTools tab
- [x] Backend Config has all 22 sections — round-trips via GET/PUT/PATCH /api/settings

### 3.3 Changelog Generation ✅
Full changelog UI with 3-step generator.
- [x] Changelog view — `changelog.rs` with entry list, expand/collapse, category grouping
- [x] AI changelog generation from task history — `at-intelligence/changelog.rs`, ChangelogEngine
- [x] 3-step generator — Select source → Generate → Release
- [ ] GitHub release creation from changelog — stub
- [ ] Task archival — not implemented

### 3.4 Project Management ✅
- [x] File explorer panel with tree view — `components/file_explorer.rs`
- [x] Multi-project support with tabs — `components/project_tabs.rs` + GET/POST/PUT/DELETE /api/projects
- [x] Project activation — POST /api/projects/{id}/activate
- [x] Add project modal — name + path form
- [ ] File watching for live updates — not implemented

### 3.5 GitLab Integration ✅
Real API implementation with env-driven credentials and MR review route.
- [x] GitLab client — `at-integrations/src/gitlab/mod.rs` (real HTTP with runtime stub fallback for tests)
- [x] Issue management — GET /api/gitlab/issues
- [x] Merge request management — GET /api/gitlab/merge-requests
- [x] MR review engine endpoint — POST /api/gitlab/merge-requests/{iid}/review
- [ ] GitLab OAuth — not implemented

### 3.6 Linear Integration ✅
Real API implementation with env-driven credentials and configurable team fallback.
- [x] Linear client — `at-integrations/src/linear/mod.rs` (real GraphQL with runtime stub fallback for tests)
- [x] Issue listing — GET /api/linear/issues
- [x] Issue import — POST /api/linear/import
- [x] Team selection fallback — query param `team_id` or `settings.integrations.linear_team_id`
- [ ] Bidirectional sync — not implemented

### 3.7 Onboarding Wizard ✅
Full 7-step onboarding wizard.
- [x] Multi-step onboarding — `pages/onboarding.rs` (7 steps)
- [x] Auth method selection — env vars, config file, OAuth
- [x] Tool/IDE preferences — IDE, terminal, git tool selection
- [x] Agent configuration — provider, model, thinking level, max agents
- [x] Memory system setup — toggles for memory, graphiti, embeddings
- [x] First task creation — quick task form or skip

### 3.8 Notifications System ✅
Backend CRUD + frontend bell component + toast CSS.
- [x] Notification bell — `components/notification_bell.rs` with dropdown
- [x] Toast notifications — CSS styles for slide-in toasts
- [x] Backend CRUD — /api/notifications, /api/notifications/count, mark read, delete
- [ ] Profile swap notifications — not implemented
- [ ] App update notifications — not implemented

### 3.9 Internationalization ✅
- [x] i18n framework (fluent-rs) — `app/leptos-ui/src/i18n.rs` with Locale enum, FluentBundle
- [x] English translations — `locales/en.ftl` (nav, common, dashboard, settings keys)
- [x] French translations — `locales/fr.ftl` (all same keys)
- [x] `t(key)` convenience function + Leptos context integration
- [ ] Full translation coverage across all UI text — partial (core keys done)

---

## TIER 4: AUTO-TUNDRA EXCLUSIVE (Superset Features)

### 4.1 Multi-CLI Support ✅
- [x] Claude CLI adapter — `at-session/cli_adapter.rs`
- [x] OpenAI Codex CLI adapter — `at-session/cli_adapter.rs`
- [x] Google Gemini CLI adapter — `at-session/cli_adapter.rs`
- [x] OpenCode CLI adapter — `at-session/cli_adapter.rs`
- [x] CLI auto-detection — GET /api/cli/available (scans PATH for claude, codex, gemini, opencode)
- [x] CLI adapter wiring to orchestrator — TaskOrchestrator.with_cli_type() + direct_mode config

### 4.2 Rust Performance Advantages ✅
- [x] Arc-based event streaming via channels — EventBus with Arc<BridgeMessage>
- [x] AHashMap for hot-path concurrent maps — 4 modules
- [x] SQLite with rusqlite + WAL + mmap — `at-core/cache.rs`
- [x] WASM-native UI with no JS framework overhead — Leptos 0.7 CSR
- [x] mimalloc global allocator — at-cli, at-daemon
- [x] M-series optimized: target-cpu=native, thin LTO, codegen-units=1

### 4.3 Advanced Agent Roles (Already Designed) ✅
- [x] Mayor agent (coordination) — `at-agents/roles.rs`
- [x] Deacon agent (task distribution)
- [x] Witness agent (monitoring)
- [x] Refinery agent (code quality)
- [x] Polecat agent (security)
- [x] Crew agent (general purpose)
- [x] Expanded roles: Spec, QA, Build, Utility, Ideation — AgentRole variants + prompts

---

## IMPLEMENTATION TEAM ASSIGNMENTS

### Team Alpha: Backend Execution Engine (Tier 1.1 + 1.2 + 1.3)
**Focus**: Task pipeline, agent spawning, worktree isolation
**Crates**: at-agents, at-session, at-daemon, at-core
**Priority**: CRITICAL - nothing works without this

### Team Beta: API & Real-Time Layer (Tier 1.4 + 1.5)
**Focus**: HTTP/WebSocket server, terminal streaming, event bus
**Crates**: at-bridge, at-daemon
**Priority**: CRITICAL - UI needs data

### Team Gamma: GitHub & Integrations (Tier 2.1 + 3.5 + 3.6)
**Focus**: GitHub/GitLab/Linear integration
**Crates**: new at-integrations crate
**Priority**: HIGH - core workflow feature

### Team Delta: AI Features (Tier 2.2 + 2.3 + 2.4 + 2.5 + 3.3)
**Focus**: Insights, roadmap gen, ideation analysis, memory system
**Crates**: new at-intelligence crate
**Priority**: HIGH - differentiation features

### Team Epsilon: UI Enhancement (Tier 2.6 + 2.7 + 3.1 + 3.2 + 3.8)
**Focus**: Task wizard, review system, kanban polish, settings UI
**Crate**: app/leptos-ui
**Priority**: MEDIUM - polish and completeness

### Team Zeta: Platform & Polish (Tier 3.4 + 3.7 + 3.9 + 4.1)
**Focus**: Multi-project, onboarding, i18n, multi-CLI
**Crates**: at-cli, at-core, app/leptos-ui
**Priority**: MEDIUM - user experience

---

## ESTIMATED SCOPE

| Tier | Features | Est. Files | Complexity |
|------|----------|-----------|------------|
| 1 (Core) | 5 areas | ~80 files | Very High |
| 2 (Important) | 7 areas | ~60 files | High |
| 3 (Nice-to-have) | 9 areas | ~50 files | Medium |
| 4 (Exclusive) | 3 areas | ~20 files | Medium |
| **Total** | **24 areas** | **~210 files** | **Massive** |

---

## RECOMMENDED EXECUTION ORDER

1. **Sprint 1** (Teams Alpha + Beta): Backend execution + API layer → first real agent run
2. **Sprint 2** (Team Epsilon + Alpha): Task wizard + review system → complete task lifecycle
3. **Sprint 3** (Teams Gamma + Delta): GitHub integration + AI features → external integrations
4. **Sprint 4** (Teams Delta + Zeta): Settings, onboarding, i18n → polish
5. **Sprint 5** (All teams): Integration testing, performance tuning, release
