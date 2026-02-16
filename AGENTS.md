# Agent teams and extension guide

This document describes how the auto-tundra codebase is organized into **agent teams** (logical areas of responsibility) and how to extend the project using `AGENTS.md`, `SKILL.md`, `todo.md`, and related context-steering files.

## Agent teams (crate ownership)

| Team | Crates | Responsibility |
|------|--------|----------------|
| **Core** | `at-core` | Types, config, context engine, workflow DSL, health checks, project context loading (AGENTS.md, SKILL.md, todo.md, CLAUDE.md) |
| **Agents** | `at-agents` | Roles, executor, prompts, registry, lifecycle, task runner, approval, specialized agents (Spec, QA, Build, Utility, Ideation) |
| **Intelligence** | `at-intelligence` | LLM providers, model router, token cache, cost tracker, API profiles, spec pipeline, runners (Spec, Insights, Ideation, Roadmap, Analysis), changelog, memory, insights, roadmap, ideation |
| **Bridge** | `at-bridge` | HTTP API, WebSocket, task CRUD, settings, credentials |
| **Harness** | `at-harness` | Provider trait, rate limiter, circuit breaker, MCP protocol, shutdown, trace context, security |
| **Daemon** | `at-daemon` | Orchestrator, task pipeline, event bus, daemon entrypoint |
| **Session** | `at-session` | PTY terminals, terminal pool |
| **Integrations** | `at-integrations` | GitHub, GitLab, Linear clients |
| **UI** | `at-tui`, `leptos-ui` | TUI and Leptos frontend |

## Context steering (skills, agents, todo)

- **AGENTS.md** — Project-level agent instructions. Loaded by `at_core::context_engine::ProjectContextLoader`. Place at repo root or in `.claude/agents/`.
- **SKILL.md** — Per-skill definitions (agentskills.io style). Stored under `.claude/skills/<name>/SKILL.md`. Frontmatter: `name`, `description`, `allowed_tools`, `references`.
- **todo.md** — Task list / backlog. Loaded as context for prioritization and planning.
- **CLAUDE.md** — Claude-specific project instructions. Loaded when present.

The context engine (`at-core::context_engine`) builds a `ContextGraph` from these files and supports:
- Progressive disclosure by token budget
- BFS subgraph traversal from a seed node
- `collect_context(task_id, budget)` for LLM-injectable context

## Extending with new agents or skills

1. **Markdown-defined agents** — Add `.claude/agents/<name>.md` or drop into `AGENTS.md`. Use `at_agents::registry::AgentRegistry::load_from_project()` to load them; they implement `RoleConfig` via `PluginAgent`.
2. **Markdown-defined skills** — Add `.claude/skills/<name>/SKILL.md` with YAML frontmatter. Load via `AgentRegistry::load_from_project()`; use `PluginSkill::to_prompt()` for injection.
3. **New Rust roles** — Add a variant to `at_core::types::AgentRole`, implement `RoleConfig` in `at-agents::roles`, and add a default prompt in `at-agents::prompts::PromptRegistry`.

## API profiles and multi-provider

- **at-intelligence** exposes `ApiProfile`, `ProfileRegistry`, `ProviderKind` (Anthropic, OpenRouter, OpenAI, Custom).
- Credentials come from **env vars only** (e.g. `ANTHROPIC_API_KEY`, `OPENROUTER_API_KEY`). See `at_core::config::CredentialProvider`.
- Failover: `ProfileRegistry::failover_for(current_id)` returns the next enabled profile with an API key.

## Running tests

```bash
cargo test --workspace
```

All agent-team crates have unit and integration tests. Total test count is 1,483+.

## Related files

- **.cursor/rules/** — Cursor IDE rules for this repo.
- **.claude/skills/** — Skill definitions (SKILL.md per skill).
- **.claude/agents/** — Agent markdown files.
- **.claude/prompts/** — Optional prompt overrides per role.
