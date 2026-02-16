# Auto Claude → Auto-Tundra Feature Parity Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Achieve full feature parity with Auto Claude, then exceed it with Rust-native performance and multi-CLI support.

**Architecture:** Rust backend (at-* crates) → Tauri IPC → Leptos WASM frontend. Backend daemon orchestrates agents via PTY sessions. Real-time updates flow through event bus to UI.

**Tech Stack:** Rust, Leptos 0.7, Tauri, SQLite, WebSocket, xterm.js (via web-sys), node-pty equivalent (portable-pty)

---

## Feature Gap Analysis: Auto Claude vs Auto-Tundra

### ✅ = Implemented | 🟡 = Partially | ❌ = Missing

---

## TIER 1: CORE FEATURES (Must-Have for MVP)

### 1.1 Task Execution Pipeline ❌
Auto Claude has a full spec → plan → code → QA → merge pipeline. Auto-tundra has types only.
- [ ] Spec creation pipeline (discovery, requirements, context, writing, critique)
- [ ] AI complexity assessment
- [ ] Planning phase (break spec into subtasks with dependencies)
- [ ] Coding phase (implement subtasks, spawn parallel subagents)
- [ ] QA review phase (validate against acceptance criteria)
- [ ] QA fix loop (iterative fix-validate cycle)
- [ ] Phase progress tracking with percentages
- [ ] Task state machine (XState-equivalent in Rust)

### 1.2 Agent Process Management ❌
Auto Claude spawns real Claude CLI processes. Auto-tundra has PTY pool but no execution.
- [ ] Agent spawning via PTY (Claude, Codex, Gemini, OpenCode)
- [ ] Agent output parsing and structured event extraction
- [ ] Agent health monitoring (heartbeat checks)
- [ ] Agent queue with routing and prioritization
- [ ] Up to 12 parallel agent terminals
- [ ] Agent profiles (model selection, thinking level per phase)
- [ ] Rate limit detection and auto-recovery
- [ ] Multi-account credential management with auto-swap

### 1.3 Git Worktree Isolation ❌
Auto Claude runs every task in an isolated git worktree.
- [ ] Create worktree per task (`.worktrees/{spec-name}/`)
- [ ] Branch management per worktree
- [ ] Worktree cleanup for orphaned/stale worktrees
- [ ] Merge system with conflict detection
- [ ] AI-powered semantic merge conflict resolution
- [ ] Direct mode option (no worktree)

### 1.4 Backend API Layer ❌
Auto-tundra UI uses hardcoded demo data. Needs real API.
- [ ] HTTP/WebSocket API server in at-daemon
- [ ] Real-time event streaming (agent events, task progress, logs)
- [ ] Task CRUD endpoints
- [ ] Agent management endpoints
- [ ] Project management endpoints
- [ ] Settings persistence endpoints

### 1.5 Terminal Emulator 🟡
Auto Claude has full xterm.js terminals. Auto-tundra has agent cards only.
- [ ] Real terminal emulation in browser (xterm.js or equivalent via web-sys)
- [ ] PTY streaming to WebSocket to browser
- [ ] Terminal grid layout with drag-and-drop reordering
- [ ] Task context injection into terminals
- [ ] File drag-and-drop into terminals
- [ ] Terminal session persistence across restarts
- [ ] Terminal font/cursor settings
- [ ] Auto-naming terminals from Claude's first message

---

## TIER 2: IMPORTANT FEATURES (High-Value Additions)

### 2.1 GitHub Integration ❌
- [ ] GitHub OAuth flow
- [ ] Issue listing with filters
- [ ] Issue detail view
- [ ] AI-powered issue investigation
- [ ] Auto-fix button for issues
- [ ] Batch issue review wizard
- [ ] Issue import to tasks
- [ ] Issue triage engine
- [ ] PR listing with filters
- [ ] PR detail view with review findings
- [ ] PR review engine (severity-based findings)
- [ ] Create PR from task worktree
- [ ] PR status polling
- [ ] Release management

### 2.2 Insights (Codebase Chat) ❌
- [ ] AI chat interface for codebase exploration
- [ ] Model selector for insights
- [ ] Chat history sidebar with session management
- [ ] Backend insights executor

### 2.3 Roadmap (AI-Generated) 🟡
Auto-tundra has static phases. Auto Claude has AI generation + competitor analysis.
- [ ] AI roadmap generation with progress tracking
- [ ] Feature cards with drag-and-drop sorting
- [ ] Feature detail panel
- [ ] Add feature dialog
- [ ] Competitor analysis dialog
- [ ] Kanban-style roadmap view

### 2.4 Ideation (AI-Powered) 🟡
Auto-tundra has basic form. Auto Claude has 6 analysis types.
- [ ] AI-powered idea generation from codebase analysis
- [ ] 6 detail types: code improvement, quality, docs, perf, security, UI/UX
- [ ] Idea filtering and sorting
- [ ] Idea-to-task conversion
- [ ] Generation progress screen

### 2.5 Context & Memory System ❌
- [ ] Project index view (file structure analysis)
- [ ] Memory entries with graph-based storage
- [ ] Service detection (API routes, DB, deps, env, external services)
- [ ] Codebase pattern discovery
- [ ] Keyword extraction
- [ ] MCP (Model Context Protocol) integration

### 2.6 Task Creation Wizard ❌
Auto-tundra has basic modal. Auto Claude has multi-step wizard.
- [ ] Multi-step task creation wizard
- [ ] Classification fields (category, priority, complexity, impact)
- [ ] Image/screenshot attachments
- [ ] Referenced files with autocomplete
- [ ] Draft auto-save
- [ ] Task source tracking

### 2.7 Task Detail & Review System ❌
- [ ] Full task detail modal (metadata, progress, subtasks, files, logs)
- [ ] Phase-based execution logs
- [ ] Diff view dialog
- [ ] Discard dialog
- [ ] Merge conflict details
- [ ] Merge preview and progress
- [ ] QA feedback section
- [ ] Task warnings display

---

## TIER 3: NICE-TO-HAVE FEATURES (Polish & Power)

### 3.1 Kanban Board Enhancements 🟡
- [ ] 8 columns (add Backlog, Queue, PR Created, Error)
- [ ] Per-column width preferences (180-600px)
- [ ] Column collapse/expand
- [ ] Column width locking
- [ ] Task filtering by category/priority/complexity/impact
- [ ] Task ordering persistence per column

### 3.2 Settings System 🟡
Auto-tundra shows hardcoded TOML. Auto Claude has full settings UI.
- [ ] General settings (theme, language, scale, beta updates)
- [ ] Display/theme settings (7 color themes, light/dark/system)
- [ ] Agent configuration settings
- [ ] Terminal font settings (family, size, weight, cursor style)
- [ ] Security settings
- [ ] Integration settings (GitHub, GitLab, Linear)
- [ ] Debug/developer settings
- [ ] Environment variable configuration per project
- [ ] IDE and terminal preference selection

### 3.3 Changelog Generation ❌
- [ ] Changelog view
- [ ] AI changelog generation from task history
- [ ] Changelog filtering
- [ ] GitHub release creation from changelog
- [ ] Task archival

### 3.4 Project Management ❌
- [ ] Multi-project support with tabs
- [ ] Drag-and-drop project tab reordering
- [ ] File explorer panel with tree view
- [ ] Project initialization and detection
- [ ] File watching for live updates

### 3.5 GitLab Integration ❌
- [ ] GitLab OAuth
- [ ] Issue management
- [ ] Merge request management
- [ ] MR review engine

### 3.6 Linear Integration ❌
- [ ] Linear task import modal
- [ ] Team/project selection
- [ ] Issue filtering and bulk import
- [ ] Bidirectional sync

### 3.7 Onboarding Wizard ❌
- [ ] Multi-step onboarding
- [ ] Auth method selection
- [ ] Tool/IDE preferences
- [ ] Memory system setup
- [ ] First task creation

### 3.8 Notifications System ❌
- [ ] Toast notifications
- [ ] Profile swap notifications
- [ ] App update notifications
- [ ] Download progress indicators

### 3.9 Internationalization ❌
- [ ] i18n framework (fluent-rs for Rust)
- [ ] English + French translations
- [ ] Translation key system across all UI text

---

## TIER 4: AUTO-TUNDRA EXCLUSIVE (Superset Features)

### 4.1 Multi-CLI Support (Already Designed) 🟡
- [ ] Claude CLI adapter (fully implement)
- [ ] OpenAI Codex CLI adapter
- [ ] Google Gemini CLI adapter
- [ ] OpenCode CLI adapter
- [ ] CLI auto-detection and configuration

### 4.2 Rust Performance Advantages
- [ ] Zero-copy event streaming via channels
- [ ] Lock-free concurrent agent management
- [ ] SQLite with rusqlite for fast persistence
- [ ] WASM-native UI with no JS framework overhead

### 4.3 Advanced Agent Roles (Already Designed) 🟡
- [ ] Mayor agent (coordination)
- [ ] Deacon agent (task distribution)
- [ ] Witness agent (monitoring)
- [ ] Refinery agent (code quality)
- [ ] Polecat agent (security)
- [ ] Crew agent (general purpose)

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
