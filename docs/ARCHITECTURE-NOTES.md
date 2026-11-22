# Architecture Notes (Auto-Tundra)

> Moved from the Claude auto-memory index (MEMORY.md, sha256 prefix 0f5cb6d2a028, not a git repo) on 2026-10-24. Content unchanged below; section "Project Structure" through "Library Research". Dated snapshots (Feb 2026), verify against code before relying on counts.

## Project Structure
- Workspace: `/Users/studio/tundra/` (live checkout; see project-live-checkout.md — rust-harness/ is a stub)
- Crates: at-core, at-agents, at-harness, at-intelligence, at-bridge, at-session, at-tui, at-cli, at-daemon, at-tauri, at-integrations, at-telemetry
- Frontend: `app/leptos-ui/` (WASM), `app/tauri/`
- Cargo: `/Users/studio/.cargo/bin/cargo`
- Tests: 1,957 passing as of 2026-02-21 (zero failures)

## Architecture Decisions
- **Context steering**: 4-level progressive disclosure (L0 Identity → L3 Deep) in `at-core/context_steering.rs`
- **RLM patterns**: Context folding, recursive decomposition, progressive refinement, stuck detection in `at-core/rlm.rs`
- **Agent roles**: 35 variants in AgentRole enum (6 core + 29 specialized matching Auto Claude)
- **Prompt templates**: Built-in defaults + `.claude/prompts/*.md` file overrides in `at-agents/prompts.rs`
- **Spec pipeline**: Discovery → Requirements → Writing → Critique → Validation in `at-intelligence/spec.rs`
- **API profiles**: Multi-provider (Anthropic, OpenRouter, OpenAI, Local, Custom) with failover in `at-intelligence/api_profiles.rs`
- **Security**: API keys via env vars only, never stored in config (see commit 23f4da1)

## Key Patterns
- `tempfile` not in workspace deps — use `tempfile = "3"` directly in crate Cargo.toml
- AgentRole enum needs Hash trait for HashMap keys
- `role_config_for()` maps specialized roles to base lifecycle agents (Crew as default)
- WASM build: `cd app/leptos-ui && trunk build` (NOT cargo build directly)
- Frontend test: `cargo nextest run -p at-core -p at-bridge -p at-agents -p at-intelligence -p at-session -p at-harness -p at-daemon -p at-integrations`
- Leptos 0.7 CSR: `class=(move || ...)` MUST have parentheses (not `class=move ||`)
- Leptos closures: use `.iter()` + `.cloned()` not `.into_iter()` (must be FnMut not FnOnce)
- Config has 22 sections: 12 original + 10 new (appearance, language, dev_tools, agent_profile, paths, api_profiles, updates, notifications, debug, memory)
- Task execution pipeline: POST /api/tasks/{id}/execute spawns TaskOrchestrator (coding→QA→fix loop)
- Linter auto-fixes: record_error increments total_requests for correct error_rate calc
- Leptos `Callback` uses `.run()` not `.call()`
- `web_sys::Navigator::clipboard()` returns `Clipboard` directly, NOT `Option<Clipboard>`
- Stacked diffs: Task struct has parent_task_id, stack_position, pr_number fields (at-core/types.rs)
- Stack API: GET /api/stacks, GET /api/tasks/{id}/stack, POST /api/tasks/{id}/parent, POST /api/tasks/{id}/rebase
- Frontend Stacks page: pages/stacks.rs with tree visualization (stack-* CSS classes)
- Agent teams: bypassPermissions mode doesn't fully bypass all Bash permission checks — direct implementation may be faster for small features
- rodio `OutputStream` is `!Send` — use dedicated audio thread with `std::sync::mpsc` channel to make `SoundEngine` `Send + Sync` for Tauri state
- `MemoryWeight` has `#[serde(default)]` on `MemoryEntry.weight` for backward compat with old entries missing the field

## Feature Parity Status (2026-02-21)
- **~97% parity** with Auto Claude achieved (3 waves, 12 agent teams + stacked diffs)
- **69+ API endpoints** including merge, queue, GitLab, Linear, OAuth, pipeline, projects, CLI, stacks
- **17 frontend pages** + onboarding wizard + project tabs (added Stacks page)
- **Integrations**: GitHub (OAuth), GitLab (real API + stub fallback), Linear (real GraphQL + stub fallback)
- **i18n**: fluent-rs with en/fr translations, `t(key)` convenience function
- **Terminal**: WebSocket streaming with grid layout (Single/Double/Quad)
- **Multi-project**: tab bar + CRUD API + activate endpoint
- **CLI detection**: GET /api/cli/available scans for claude, codex, gemini, opencode
- **Performance**: mimalloc, AHashMap, Arc events, SQLite WAL+mmap, thin LTO, target-cpu=native

## RLM Research (Feb 2026)
- MIT paper: Recursive Language Models decompose via Python REPL, sub-LLM calls
- Key insight: "Context management > context length" — fold context recursively
- Synthesis strategies: concatenate, LLM merge, best-of, vote, progressive refine
- Stuck detection: timeout, output loops, budget exhaustion
- See: primeintellect.ai/blog/rlm, jangwook.net/en/blog/en/rlm-recursive-language-model-coding-agent/

## Auto Claude Source Analysis (Feb 22) — see memory/auto-claude-source-feb22.md
- Electron+React frontend, Python backend using `claude_agent_sdk`
- Kanban: NO dots in column headers — uses `border-t-2` top border for color. 6 columns with resize/lock/collapse
- Card dots are **subtask indicators**: green=completed, blue=in_progress, red=failed, gray=pending (PhaseProgressIndicator.tsx)
- 9 execution phases: idle, planning, coding, rate_limit_paused, auth_failure_paused, qa_review, qa_fixing, complete, failed
- MCP server: 6 custom tools (subtask_status, build_progress, record_discovery/gotcha, session_context, qa_status)
- Key patterns: phase-aware tool control, fuzzy file path auto-correction, atomic JSON writes, planning validation loop

## Library Research (Feb 21) — see memory/research-feb21.md
- **git2-rs**: PARTIAL ADOPT — use for read ops (diff, status, worktree list), keep shell-out for rebase/merge/fetch
- **rustic-rs**: DEFER — beta, heavy deps, git already covers worktree safety
- **candle + vllm.rs**: Local inference providers for api_profiles.rs
- **claude-sdk-rs**: Agent session management for at-agents terminals
- **tachyonfx + ratatui-wgpu**: TUI shader effects + GPU backend for at-tui
- **DuckDB WASM**: Client-side analytics complement to SQLite OLTP
- **LocalProvider**: OpenAI-compatible endpoint for local inference (vllm.rs, llama.cpp, Ollama, candle)
- **Claude session manager**: `ClaudeSessionManager` in at-agents for multi-turn conversation state
- **ratatui-wgpu**: Optional `gpu` feature flag on at-tui (`--features gpu`), Builder API needs winit surface
- **GitLab MR review**: Real heuristic analysis of MR diffs (unwrap, panic, unsafe, dbg!, etc.) via GitLab API
- **rodio sound effects**: Procedural audio via Tauri bridge — `SoundEngine` on dedicated audio thread, 6 effects (click, success, error, notify, whoosh, chip), optional `sounds` feature
- **Cortex-mem L0/L1/L2 weighting**: `MemoryWeight` with tier (L0 core/L1 active/L2 archive), temporal decay (7-day half-life), frequency bonus, auto-promote/demote — integrated into `ContextSteerer.assemble()`
- **flume channels**: Project already uses flume (not std::mpsc) for all channel communication in at-daemon
