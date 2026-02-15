# auto-tundra Design Document

**Date:** 2026-02-15
**Status:** Approved
**Type:** New Project (Rust + Tauri + Leptos)

## 1. Overview

auto-tundra is a high-performance, multi-agent terminal orchestrator built in Rust. It replaces gastown (Go) and tundra-dome (Go+Kafka+Airflow) with a single Rust binary that manages Claude Code, Codex CLI, Gemini CLI, and OpenCode agents across multiple projects simultaneously.

**Core capabilities:**
- Task queue runner: feed tasks, spawn agent sessions, monitor, collect results
- Full gastown rewrite: beads, agents, convoy, mail, daemon, refinery in Rust
- Desktop dashboard: Tauri GUI with Leptos WASM for monitoring, managing, approving
- Distributed coordination: optional Kafka bridge for tundra-dome compatibility
- Built-in terminal: PTY pool + zellij web sidecar + alacritty_terminal headless

## 2. Lineage

### 2.1 gastown (Go)
Multi-agent orchestration for Claude Code. Beads (git-backed JSONL+SQLite work units), tmux sessions for agents, git worktrees for polecats, mail system, web dashboard, TUI. Coordinates 20-30 agents. Agent taxonomy: Mayor, Deacon, Witness, Refinery, Polecats, Crew.

### 2.2 tundra-dome (Go+Kafka+Airflow)
Extends gastown with Kafka event streaming (27 topics), Airflow DAGs for policy enforcement (10 Superdome policies), Datadog observability (APM + Metrics + Jobs Monitoring via OpenLineage), Kubernetes deployment. Go CLI (`td`) with Sarama Kafka producer.

### 2.3 rust-harness (Rust)
Multi-agent orchestrator library with circuit breakers, rate limiting, memory backends (SQLite, filesystem), security guardrails (tool call firewall, input sanitization), validation harness, quota management. OpenRouter provider.

### 2.4 Auto-Claude (Electron + TypeScript) - Reference
Autonomous multi-session AI coding framework. Kanban board for task management, up to 12 simultaneous agent terminals, roadmap/ideation/insights panels, context injection into terminals.

### 2.5 ccboard (Rust TUI + Web) - Reference Architecture
9-tab dashboard (Dashboard, Sessions, Config, Hooks, Agents, Costs, History, MCP, Analytics). Dual-mode TUI + Web with 100% feature parity. SQLite caching with 89x speedup. SSE for live updates. Single 5.8MB binary.

## 3. Technology Stack

### 3.1 Core Dependencies (Tier 1)

| Crate | Version | License | Purpose |
|-------|---------|---------|---------|
| `genai` | 0.5.3 | MIT/Apache-2.0 | Multi-provider LLM client (14 providers) |
| `rig-core` | 0.30.0 | MIT | Agent framework (RAG, composable agents, 17 providers) |
| `claude-agent-sdk-rs` | 0.6.4 | MIT | Deep Claude Code integration (hooks, streaming) |
| `alacritty_terminal` | 0.25.1 | Apache-2.0 | Headless terminal emulation |
| `portable-pty` | 0.9.0 | MIT | Cross-platform PTY creation |
| `expectrl` | 0.8.0 | MIT | Terminal automation (expect-style) |
| `ratatui` | 0.30.0 | MIT | TUI framework (CLI mode) |
| `crossterm` | 0.29.0 | MIT | Terminal backend |
| `tauri` | 2.10.2 | MIT/Apache-2.0 | Desktop app framework |
| `leptos` | latest | MIT | Rust WASM frontend |
| `tauri-specta` | 2.0.0-rc | MIT | Type-safe Tauri IPC |
| `tauri-plugin-mcp-bridge` | 0.8.1 | MIT | MCP + Tauri integration |
| `tokio` | 1.x | MIT | Async runtime |
| `axum` | 0.8.x | MIT | HTTP/WebSocket/SSE server |
| `flume` | latest | Apache-2.0/MIT | High-perf channels |
| `dashmap` | latest | MIT | Concurrent HashMap |
| `clap` | 4.x | MIT/Apache-2.0 | CLI framework |
| `indicatif` | 0.18.x | MIT | Progress bars |
| `xterm.js` | latest | MIT | WebGL terminal rendering in Tauri webview |
| `dolt` | latest | Apache-2.0 | Git-for-data database (external binary) |

### 3.2 Enhancement Dependencies (Tier 2)

| Crate | License | Purpose |
|-------|---------|---------|
| `openrouter_api` 0.5.0 | MIT/Apache-2.0 | OpenRouter with MCP support |
| `sandbox-agent` 0.2.1 | - | Sandboxed agent execution |
| `agent-bridge` 0.6.2 | - | Cross-agent context handoff |
| `chatdelta` 0.8.0 | - | Parallel multi-AI streaming |
| `codex-helper` 0.12.1 | - | Usage-aware provider routing |
| `rust_supervisor` | MIT | Erlang-style process supervision |
| `pyo3` 0.28.0 | Apache-2.0/MIT | Python AI library interop |
| `tui-term` | MIT | PTY output in ratatui widgets |

### 3.3 Reference Projects (Tier 3 - study, don't depend)

| Project | What to take |
|---------|-------------|
| ccboard | 9-tab architecture, SQLite caching, SSE live updates, dual TUI+Web |
| tmuxcc | Status glyphs (@ * ! ?), approval workflows, tree views |
| cc-enhanced | Analytics/cost tabs, theme system, todo extraction |
| mprocs | Split-pane focus management, scrollback buffers |
| claude-squad | Git worktree isolation, tmux integration |
| Auto-Claude | Kanban board, multi-terminal grid, context injection |
| tenex | AI-agent-specific terminal multiplexing |

## 4. Architecture

### 4.1 Workspace Structure

```
auto-tundra/
├── Cargo.toml                          # Workspace root
├── crates/
│   ├── at-core/                        # Domain model + Dolt DB
│   ├── at-harness/                     # LLM providers + security
│   ├── at-session/                     # Terminal + agent sessions
│   ├── at-agents/                      # Agent taxonomy
│   ├── at-daemon/                      # Background services
│   ├── at-telemetry/                    # OTel + Vector + business metrics
│   ├── at-cli/                         # `at` CLI binary
│   └── at-bridge/                      # Optional event bridge
├── app/
│   ├── tauri/                          # Tauri 2.x backend
│   └── leptos-ui/                      # Leptos WASM frontend
├── dolt/                               # Dolt DB schema
└── docs/
    └── plans/
```

### 4.2 Crate Details

#### at-core (Domain Model + Dolt DB)

```
at-core/
├── beads.rs              # Bead CRUD (SQL via Dolt MySQL protocol)
├── convoy.rs             # Work batches / convoys
├── agent_model.rs        # Agent taxonomy + lifecycle state machine
├── lanes.rs              # Priority lanes (critical/standard/experimental)
├── mail.rs               # Inter-agent messaging
├── hooks.rs              # GUPP: work-on-hook enforcement
├── dolt.rs               # Dolt client (MySQL protocol + branch/merge/diff)
└── cache.rs              # SQLite caching layer (ccboard pattern, 89x speedup)
```

**Dolt integration model:** Connect via MySQL protocol (Dolt exposes MySQL-compatible server on port 3306). Beads become SQL tables with git-like branch/merge/diff. Per-rig branches for agent isolation. SQLite as a read cache layer for dashboard queries.

#### at-harness (LLM Providers + Security)

```
at-harness/
├── providers/
│   ├── multi.rs            # genai unified client (14 providers)
│   ├── openrouter.rs       # openrouter_api (MCP, preferences)
│   ├── claude_sdk.rs       # claude-agent-sdk-rs (hooks, streaming)
│   └── router.rs           # Smart routing (usage-aware, failover)
├── agents/
│   ├── rig_agent.rs        # rig-core agent (RAG, tools, composable)
│   └── sandbox.rs          # Sandboxed execution
├── circuit_breaker.rs      # From rust-harness
├── rate_limiter.rs         # From rust-harness
├── security.rs             # Tool firewall + OpenClaw rules (35+ rules)
└── memory.rs               # Dolt-backed + SQLite memory backends
```

**Multi-provider strategy:**
- `genai` as unified client for direct API access (14 providers)
- `openrouter_api` for OpenRouter-specific features (MCP, provider preferences)
- `claude-agent-sdk-rs` for deep Claude Code CLI integration
- `rig-core` for agent abstractions (RAG, composable agents, tool calling)
- Smart router with usage-aware switching and automatic failover

#### at-session (Terminal + Agent Sessions)

```
at-session/
├── pty_pool.rs             # portable-pty pool (per-agent PTYs)
├── terminal.rs             # alacritty_terminal headless emulation
├── expect.rs               # expectrl automation for agent CLIs
├── zellij_sidecar.rs       # Zellij web mode (WebSocket bridge)
├── mux.rs                  # tmux/screen fallback for human intervention
├── context_bridge.rs       # Cross-agent context handoff (agent-bridge)
└── cli_adapters/
    ├── mod.rs              # Trait: AgentCLI { spawn, send, read, status }
    ├── claude.rs           # Claude Code CLI adapter
    ├── codex.rs            # Codex CLI adapter
    ├── gemini.rs           # Gemini CLI adapter
    └── opencode.rs         # OpenCode adapter
```

**Three terminal paths:**

1. **Zellij web sidecar** (primary for interactive agents)
   - Bundle zellij binary as Tauri sidecar
   - Run in web mode on localhost (WebSocket + xterm.js)
   - Provides panes, tabs, floating windows, WASM plugins
   - Session resurrection, multi-client attach
   - Leptos UI connects via WebSocket

2. **alacritty_terminal + portable-pty** (headless agents)
   - For agents that don't need interactive UI
   - Output capture and parsing without rendering overhead
   - Used by Zed editor, egui_term, iced_term
   - `Term<T>` with `renderable_content()` for optional UI

3. **tmux/screen fallback** (human intervention)
   - Attach to live agent sessions from external terminal
   - SSH access for remote intervention
   - Compatible with gastown's tmux workflow

**CLI adapter pattern:**
- Each agent CLI gets an adapter implementing `AgentCLI` trait
- `expectrl` handles expect-style automation (wait for prompt, send command)
- Adapters normalize output format for the orchestrator
- PTY per agent via `portable-pty`

#### at-agents (Agent Taxonomy)

```
at-agents/
├── mayor.rs              # Global coordinator
├── deacon.rs             # System watchdog + patrol loops
├── witness.rs            # Per-rig monitor
├── refinery.rs           # Merge queue manager
├── polecat.rs            # Ephemeral workers (git worktrees)
├── crew.rs               # Long-lived named agents
└── supervisor.rs         # rust_supervisor process management
```

**Gastown concepts preserved:**
- Mayor: global coordinator, assigns work, manages lanes
- Deacon: system watchdog, patrol loops, health enforcement
- Witness: per-rig monitor, watches agent activity
- Refinery: merge queue manager, git operations
- Polecat: ephemeral worker in git worktree, disposable
- Crew: long-lived named agent with persistent state

**Supervisor model:** Erlang-style supervision tree via `rust_supervisor`. Auto-restart crashed agents. Health monitoring with configurable thresholds.

#### at-daemon (Background Services)

```
at-daemon/
├── daemon.rs             # Main daemon loop (tokio select!)
├── patrol.rs             # GUPP violations, health checks
├── heartbeat.rs          # Agent heartbeat monitoring
├── metrics.rs            # Cost/token/usage tracking
├── kpi.rs                # KPI snapshots (tundra-dome pattern)
└── cleanup.rs            # Orphan detection, stale branch cleanup
```

#### at-telemetry (Observability)

```
at-telemetry/
├── otel.rs               # OTel SDK init (traces + metrics + OTLP export)
├── spans.rs              # Pre-defined span builders (agent, bead, provider, convoy)
├── business_metrics.rs   # Custom business metrics (cost, throughput, etc.)
├── kpi.rs                # KPI snapshot generator
├── openlineage.rs        # OpenLineage event emission for Jobs Monitoring
└── logging.rs            # tracing-subscriber setup (JSON, env-filter)
```

All crates depend on `at-telemetry` for instrumented spans and metrics. Vector runs externally, consuming logs from `/var/log/auto-tundra/` and OTel from `localhost:4317`.

#### at-cli (`at` CLI Binary)

```
at-cli/
├── main.rs               # clap entry point
└── commands/
    ├── sling.rs           # Assign work to agent
    ├── hook.rs            # Pin work to agent
    ├── done.rs            # Complete work
    ├── nudge.rs           # Notify agent
    ├── convoy.rs          # Manage convoys
    ├── mail.rs            # Agent messaging
    ├── polecat.rs         # Spawn/manage polecats
    ├── session.rs         # Terminal session management
    ├── tui.rs             # Launch TUI dashboard
    ├── status.rs          # System status
    └── cost.rs            # Cost/usage report
```

Replaces both `gt` (gastown) and `td` (tundra-dome) CLIs.

#### at-bridge (Optional Event Bridge)

```
at-bridge/
├── kafka.rs              # Kafka producer/consumer (tundra-dome compat)
├── webhook.rs            # Webhook integration
└── sse.rs                # SSE event stream (for web UI live updates)
```

### 4.3 Tauri Backend

```
app/tauri/src/
├── main.rs
├── commands/
│   ├── pty.rs             # PTY lifecycle (create, write, resize, close)
│   ├── agents.rs          # Agent CRUD + lifecycle
│   ├── beads.rs           # Bead queries via Dolt
│   ├── sessions.rs        # Session management
│   ├── metrics.rs         # Cost/usage data
│   ├── mcp.rs             # MCP bridge commands
│   └── system.rs          # Health, config, daemon control
└── state.rs               # Tauri managed state
```

Type-safe IPC via `tauri-specta`. MCP integration via `tauri-plugin-mcp-bridge`. Zellij bundled as sidecar binary.

### 4.4 Leptos Frontend

```
app/leptos-ui/src/
├── app.rs
├── pages/                  # 9-tab layout
│   ├── dashboard.rs        # Overview: agents, beads, costs, KPIs
│   ├── agents.rs           # Agent grid with live terminals
│   ├── beads.rs            # Kanban board
│   ├── sessions.rs         # Session browser + live stats
│   ├── convoys.rs          # Convoy progress tracker
│   ├── costs.rs            # Cost breakdown, forecast, budgets
│   ├── analytics.rs        # Heatmaps, trends, insights
│   ├── config.rs           # 4-column config diff
│   └── mcp.rs              # MCP server management
└── components/
    ├── terminal.rs         # xterm.js wrapper (WebGL)
    ├── agent_card.rs       # Status glyphs: @ * ! ?
    ├── bead_card.rs        # Bead detail with drag-drop
    ├── kanban.rs           # Kanban board component
    ├── metrics_chart.rs    # Token/cost/usage charts
    ├── heatmap.rs          # Activity heatmap
    ├── tree_view.rs        # Session/pane hierarchy
    ├── diff_viewer.rs      # Git diff display
    ├── log_viewer.rs       # Filterable log stream
    ├── approval_panel.rs   # Agent approval workflow
    ├── command_palette.rs  # `:` command palette
    ├── search.rs           # `/` full-text search
    └── help_modal.rs       # `?` keyboard shortcuts
```

## 5. UI Design

### 5.1 Tab Layout (9 tabs)

| # | Tab | Description |
|---|-----|-------------|
| 1 | Dashboard | Overview: agent count/status, bead count/status, convoy progress, cost summary, KPI cards |
| 2 | Agents | Agent list + live terminal grid (xterm.js), approval workflow, status glyphs |
| 3 | Beads | Kanban board: Backlog -> Hooked -> Slung -> Review -> Done, drag-drop |
| 4 | Sessions | Session browser with CPU/RAM/tokens, chronological timeline, search |
| 5 | Convoys | Convoy progress tracker, molecule templates, batch work management |
| 6 | Costs | Cost breakdown by model/project, 30-day forecast, budget alerts, billing windows |
| 7 | Analytics | Activity heatmaps, temporal patterns, usage trends, insights |
| 8 | Config | 4-column config diff (default/global/project/local), YAML editing |
| 9 | MCP | MCP server management, status detection, env masking |

### 5.2 Status Glyph System

| Glyph | Meaning | Usage |
|-------|---------|-------|
| `@` | Processing/Active | Agent running |
| `*` | Idle | Agent waiting |
| `!` | Pending approval | Needs human input |
| `?` | Unknown | State unclear |
| ` | Active bead | Work in progress |
| ` | Inactive bead | Not started |
| ` | Completed | Done |
| ` | Failed | Error |
| ` | High priority | Critical lane |
| ` | Medium priority | Standard lane |
| ` | Low priority | Experimental lane |

### 5.3 Keyboard Navigation (vim-first)

| Key | Action |
|-----|--------|
| `1-9` | Direct tab jump |
| `j/k` | Navigate up/down |
| `h/l` | Navigate left/right |
| `Tab` | Cycle panels |
| `Enter` | Select/activate |
| `y/n` | Approve/reject agent action |
| `a` | Approve all pending |
| `f` | Focus terminal pane |
| `z` | Zoom/fullscreen terminal |
| `s` | Split view |
| `/` | Search |
| `:` | Command palette |
| `?` | Help modal |
| `r` | Refresh |
| `t` | Cycle theme |
| `q` | Quit |

### 5.4 Dashboard Layout

```
+-------------------------------------------------------------+
|  auto-tundra                                  [5 agents]     |
+-------------------------------------------------------------+
| [1.Dashboard] [2.Agents] [3.Beads] ... [9.MCP]             |
+-------------------------------------------------------------+
|                                                              |
|  +------------+------------+------------+------------+       |
|  | Agents  5  | Beads  23  | Convoys 3  | Cost $47   |       |
|  | @3 *1 !1   | .12 .8 .3  | 67%        | +12% today |       |
|  +------------+------------+------------+------------+       |
|                                                              |
|  +---------------------------+-----------------------------+ |
|  | Active Agents             | Recent Activity             | |
|  | @ mayor    opus-4.6  12% | 14:32 polecat-1 done #42   | |
|  | @ deacon   sonnet    8%  | 14:28 refinery merged #38   | |
|  | * witness  haiku     0%  | 14:15 mayor assigned #43    | |
|  | ! polecat1 codex    45%  | 14:12 deacon patrol OK      | |
|  | @ polecat2 gemini   22%  | 14:01 polecat-2 slung #44  | |
|  +---------------------------+-----------------------------+ |
|                                                              |
+-------------------------------------------------------------+
| [/] Search [:] Command [?] Help [q] Quit  CPU 12% MEM 2G   |
+-------------------------------------------------------------+
```

### 5.5 Agent Grid (Tab 2)

```
+-------------------------------------------------------------+
| Agent List              | Live Terminal (xterm.js)           |
| +--------------------+  | +-------------------------------+  |
| | @ mayor [opus]     |<-| | $ claude code --resume sess1  |  |
| | @ deacon [sonnet]  |  | | Analyzing codebase...         |  |
| | * witness [haiku]  |  | | Found 3 files to modify       |  |
| | ! polecat-1 [codex]|  | | > Apply changes? [y/n]        |  |
| | @ polecat-2 [gem]  |  | |                               |  |
| +--------------------+  | +-------------------------------+  |
| [y] Approve [n] Reject  | [f] Focus [z] Zoom [s] Split     |
+-------------------------------------------------------------+
```

### 5.6 Kanban Board (Tab 3)

```
+----------+----------+----------+----------+----------+
| Backlog  | Hooked   | Slung    | Review   | Done     |
| #44      | #43      | #41      | #38      | #35 .    |
| #45      | @mayor   | @plct-2  | @refnry  | #36 .    |
| #46      |          | 45%      | 2 files  | #37 .    |
+----------+----------+----------+----------+----------+
```

## 6. Data Architecture

### 6.1 Dolt DB Schema

```sql
-- Beads (work units)
CREATE TABLE beads (
    id          VARCHAR(36) PRIMARY KEY,
    title       VARCHAR(255) NOT NULL,
    description TEXT,
    status      ENUM('backlog','hooked','slung','review','done','failed','escalated') NOT NULL,
    lane        ENUM('critical','standard','experimental') DEFAULT 'standard',
    priority    INT DEFAULT 0,
    agent_id    VARCHAR(36),
    convoy_id   VARCHAR(36),
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    hooked_at   TIMESTAMP NULL,
    slung_at    TIMESTAMP NULL,
    done_at     TIMESTAMP NULL,
    git_branch  VARCHAR(255),
    metadata    JSON
);

-- Agents
CREATE TABLE agents (
    id          VARCHAR(36) PRIMARY KEY,
    name        VARCHAR(255) NOT NULL UNIQUE,
    role        ENUM('mayor','deacon','witness','refinery','polecat','crew') NOT NULL,
    cli_type    ENUM('claude','codex','gemini','opencode') NOT NULL,
    model       VARCHAR(255),
    status      ENUM('active','idle','pending','unknown','stopped') NOT NULL,
    rig         VARCHAR(255),
    pid         INT,
    session_id  VARCHAR(255),
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    last_seen   TIMESTAMP,
    metadata    JSON
);

-- Convoys (work batches)
CREATE TABLE convoys (
    id          VARCHAR(36) PRIMARY KEY,
    name        VARCHAR(255) NOT NULL,
    description TEXT,
    lane        ENUM('critical','standard','experimental') DEFAULT 'standard',
    status      ENUM('pending','active','completed','failed') NOT NULL,
    progress    FLOAT DEFAULT 0.0,
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    started_at  TIMESTAMP NULL,
    completed_at TIMESTAMP NULL,
    metadata    JSON
);

-- Mail (inter-agent messaging)
CREATE TABLE mail (
    id          VARCHAR(36) PRIMARY KEY,
    from_agent  VARCHAR(36) NOT NULL,
    to_agent    VARCHAR(36) NOT NULL,
    subject     VARCHAR(255),
    body        TEXT NOT NULL,
    read_at     TIMESTAMP NULL,
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    metadata    JSON
);

-- Events (audit log)
CREATE TABLE events (
    id          VARCHAR(36) PRIMARY KEY,
    event_type  VARCHAR(50) NOT NULL,
    actor       VARCHAR(36),
    bead_id     VARCHAR(36),
    agent_id    VARCHAR(36),
    rig         VARCHAR(255),
    lane        VARCHAR(50),
    payload     JSON,
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Metrics (cost/token tracking)
CREATE TABLE metrics (
    id          VARCHAR(36) PRIMARY KEY,
    agent_id    VARCHAR(36),
    model       VARCHAR(255),
    input_tokens  BIGINT DEFAULT 0,
    output_tokens BIGINT DEFAULT 0,
    cost_usd    DECIMAL(10,6) DEFAULT 0,
    duration_ms BIGINT DEFAULT 0,
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- KPI Snapshots
CREATE TABLE kpi_snapshots (
    id          VARCHAR(36) PRIMARY KEY,
    beads_active      INT,
    beads_completed   INT,
    beads_failed      INT,
    agents_active     INT,
    convoys_active    INT,
    total_cost_today  DECIMAL(10,2),
    total_tokens_today BIGINT,
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

### 6.2 Dolt Branch Strategy

- `main` branch: source of truth, all completed work
- `rig/{rig-name}` branches: per-machine state
- `agent/{agent-name}` branches: per-agent working state
- Agents commit to their branch, refinery merges to main
- `dolt diff` for change review, `dolt merge` for integration

### 6.3 SQLite Cache Layer

Following ccboard's 89x speedup pattern:
- SQLite file alongside Dolt for read-heavy dashboard queries
- TTL-based cache invalidation
- Dirty-state checking before cache refresh
- Lazy loading for expensive aggregations
- File watcher (500ms debounce) for Dolt changes

## 7. Security

### 7.1 Tool Call Firewall

From rust-harness + OpenClaw patterns:
- 35+ built-in security rules
- Blocks: `rm -rf /`, SSH key theft, API key exposure, crypto wallet access
- 3 rule types: Regex, Keyword, Template
- Two modes: Enforce or Monitor
- Custom rules via YAML config

### 7.2 Agent Sandboxing

- Each agent CLI runs in its own PTY (process isolation)
- Git worktrees for polecat filesystem isolation
- Rate limiting per agent (token bucket algorithm)
- Circuit breaker per provider (prevent cascade failures)
- Input sanitization for prompt injection detection

### 7.3 PTY Limits

- Configurable max PTY count (default: 20)
- PTY pool with lifecycle management (create, reuse, destroy)
- Graceful degradation when limit reached (queue, not crash)
- Per-agent timeout enforcement

## 8. Performance Targets

| Metric | Target |
|--------|--------|
| Startup time | < 200ms (100+ sessions) |
| Memory baseline | < 20MB |
| CPU idle | < 1% |
| UI latency | < 50ms |
| Rendering | 60fps |
| Cache hit rate | > 99% |
| Dashboard query | < 250ms (from Dolt via cache) |
| Agent spawn | < 500ms |
| Binary size | < 10MB (single binary) |

## 9. Observability & Telemetry

### 9.1 OpenTelemetry Instrumentation

Full OTel stack for traces, metrics, and logs:

| Crate | License | Purpose |
|-------|---------|---------|
| `opentelemetry` | Apache-2.0 | Core OTel API |
| `opentelemetry-otlp` | Apache-2.0 | OTLP exporter (gRPC + HTTP) |
| `opentelemetry-sdk` | Apache-2.0 | SDK for traces + metrics |
| `tracing-opentelemetry` | MIT | Bridge tracing crate to OTel |
| `tracing` | MIT | Structured logging/tracing |
| `tracing-subscriber` | MIT | Log formatting + filtering |

**Trace spans for every critical path:**
- `agent.spawn` -> `agent.session.create` -> `agent.pty.open`
- `bead.lifecycle` -> `bead.hook` -> `bead.sling` -> `bead.done`
- `provider.request` -> `provider.stream` -> `provider.complete`
- `convoy.start` -> `convoy.progress` -> `convoy.complete`
- `daemon.patrol` -> `daemon.heartbeat` -> `daemon.cleanup`

**Span attributes:**
```
agent.name, agent.role, agent.cli_type, agent.model
bead.id, bead.status, bead.lane, bead.priority
provider.name, provider.model, provider.tokens.input, provider.tokens.output
convoy.id, convoy.progress
rig.name, rig.hostname
```

### 9.2 Business Metrics

Custom metrics exported via OTel Metrics SDK:

**Agent Metrics:**
- `at.agents.active` (gauge) - Active agent count by role/cli_type
- `at.agents.idle` (gauge) - Idle agent count
- `at.agents.pending_approval` (gauge) - Agents waiting for human input
- `at.agents.spawn_duration_ms` (histogram) - Agent spawn latency
- `at.agents.session_duration_s` (histogram) - Agent session length
- `at.agents.restarts` (counter) - Agent restart count by role

**Bead Metrics:**
- `at.beads.total` (gauge) - Total beads by status/lane
- `at.beads.cycle_time_s` (histogram) - Time from hook to done
- `at.beads.throughput` (counter) - Beads completed per interval
- `at.beads.failed` (counter) - Bead failures by lane
- `at.beads.escalated` (counter) - Escalations by severity

**Cost Metrics:**
- `at.cost.total_usd` (counter) - Total cost by provider/model
- `at.cost.tokens.input` (counter) - Input tokens by provider/model
- `at.cost.tokens.output` (counter) - Output tokens by provider/model
- `at.cost.per_bead_usd` (histogram) - Cost per bead completion
- `at.cost.burn_rate_usd_per_hour` (gauge) - Current burn rate

**Convoy Metrics:**
- `at.convoy.active` (gauge) - Active convoys
- `at.convoy.progress` (gauge) - Convoy completion percentage
- `at.convoy.duration_s` (histogram) - Convoy total duration

**System Metrics:**
- `at.pty.active` (gauge) - Active PTY count
- `at.pty.pool_utilization` (gauge) - PTY pool usage percentage
- `at.daemon.patrol_duration_ms` (histogram) - Patrol loop latency
- `at.cache.hit_rate` (gauge) - SQLite cache hit rate
- `at.dolt.query_duration_ms` (histogram) - Dolt query latency

### 9.3 Logging with Vector

**Architecture:**
```
auto-tundra (tracing crate)
    |
    +-> stdout (JSON structured logs)
    +-> file (/var/log/auto-tundra/*.jsonl)
    |
    v
Vector (sidecar or agent)
    |
    +-> Datadog Logs API
    +-> Datadog Metrics (from log-derived metrics)
    +-> S3/local archive (cold storage)
    +-> stdout (development)
```

**Vector configuration (vector.toml):**
```toml
[sources.at_logs]
type = "file"
include = ["/var/log/auto-tundra/*.jsonl"]
read_from = "beginning"

[sources.at_otel]
type = "opentelemetry"
grpc.address = "0.0.0.0:4317"
http.address = "0.0.0.0:4318"

[transforms.parse_json]
type = "remap"
inputs = ["at_logs"]
source = '. = parse_json!(.message)'

[transforms.enrich]
type = "remap"
inputs = ["parse_json"]
source = '''
.service = "auto-tundra"
.rig = get_env_var("AT_RIG") ?? "unknown"
.env = get_env_var("AT_ENV") ?? "local"
'''

[transforms.redact]
type = "remap"
inputs = ["enrich"]
source = '''
# Redact API keys and secrets
.message = redact(.message, filters: ["pattern"], patterns: ["sk-[a-zA-Z0-9-_]+", "ghp_[a-zA-Z0-9]+"])
'''

[sinks.datadog_logs]
type = "datadog_logs"
inputs = ["redact"]
default_api_key = "${DD_API_KEY}"
site = "datadoghq.com"

[sinks.datadog_metrics]
type = "datadog_metrics"
inputs = ["at_otel"]
default_api_key = "${DD_API_KEY}"

[sinks.archive]
type = "file"
inputs = ["redact"]
path = "/var/log/auto-tundra/archive/%Y-%m-%d.jsonl"
encoding.codec = "json"
```

**Structured log format:**
```json
{
  "ts": "2026-02-15T14:32:00.123Z",
  "level": "INFO",
  "target": "at_agents::mayor",
  "span": { "agent": "mayor", "bead": "bead-42", "rig": "mbp_m1" },
  "message": "Assigned bead to polecat-1",
  "fields": {
    "agent.role": "mayor",
    "bead.id": "bead-42",
    "bead.lane": "critical",
    "action": "sling"
  }
}
```

### 9.4 Datadog Integration

Carries forward tundra-dome's Datadog stack:
- **APM:** Distributed traces via OTel -> Datadog
- **Metrics:** Custom business metrics via OTel -> Datadog
- **Logs:** Structured JSON via Vector -> Datadog
- **Jobs Monitoring:** OpenLineage events for bead lifecycle (START/COMPLETE/FAIL)
- **Dashboards:** Pre-built Datadog dashboards for auto-tundra KPIs

**OpenLineage mapping (from tundra-dome):**
- `bead.hook` -> OpenLineage START
- `bead.done` -> OpenLineage COMPLETE
- `bead.done --fail` -> OpenLineage FAIL
- Namespace: `auto-tundra:{rig}`
- RunId: deterministic from `namespace:bead_id`

### 9.5 KPI Snapshots

Periodic KPI snapshots (every 5 minutes) written to Dolt + exported as metrics:

```json
{
  "ts": "2026-02-15T14:30:00Z",
  "beads_active": 12,
  "beads_completed_15m": 3,
  "beads_failed_15m": 0,
  "agents_active": 5,
  "agents_idle": 1,
  "convoys_active": 2,
  "cost_today_usd": 47.20,
  "tokens_today": 2350000,
  "burn_rate_usd_hour": 5.90,
  "patrol_ok": true,
  "pty_utilization": 0.25
}
```

Used by daemon patrol loops for threshold-based auto-escalation (tundra Superdome pattern).

### 9.6 Crate Additions for Observability

| Crate | License | Purpose |
|-------|---------|---------|
| `opentelemetry` | Apache-2.0 | Core OTel API |
| `opentelemetry-otlp` | Apache-2.0 | OTLP exporter |
| `opentelemetry-sdk` | Apache-2.0 | Traces + metrics SDK |
| `opentelemetry-semantic-conventions` | Apache-2.0 | Standard attribute names |
| `tracing-opentelemetry` | MIT | Bridge tracing -> OTel |
| `tracing` | MIT | Structured logging |
| `tracing-subscriber` | MIT | Subscriber + env-filter |
| `metrics` | MIT | Metrics facade (alternative to OTel metrics) |
| `openlineage-rust` | Apache-2.0 | OpenLineage event emission |

Vector runs as external sidecar (not embedded), configured via `vector.toml`.

## 10. Build Sequence

### Phase 1: Foundation (at-core + at-cli)
1. Cargo workspace scaffolding
2. Dolt DB schema + client
3. Bead CRUD operations
4. `at` CLI with sling/hook/done/status
5. SQLite cache layer

### Phase 2: Agent Sessions (at-session + at-harness)
1. PTY pool with portable-pty
2. CLI adapters (claude, codex, gemini, opencode)
3. expectrl automation
4. genai + rig-core provider integration
5. Circuit breaker + rate limiter from rust-harness

### Phase 3: Agent Taxonomy (at-agents + at-daemon)
1. Agent state machine
2. Mayor, Deacon, Witness, Refinery, Polecat, Crew
3. rust_supervisor process supervision
4. Daemon with patrol loops
5. Heartbeat monitoring

### Phase 4: TUI Dashboard (ratatui)
1. 9-tab layout
2. Agent list with status glyphs
3. Bead kanban board
4. Live terminal embedding (tui-term)
5. Cost/analytics displays

### Phase 5: Tauri Desktop App
1. Tauri 2.x scaffolding
2. Leptos WASM frontend
3. xterm.js terminal components
4. Zellij sidecar integration
5. tauri-specta type-safe IPC
6. MCP bridge

### Phase 6: Advanced Features
1. Convoy management
2. Kafka bridge (tundra-dome compat)
3. SSE live updates
4. Approval workflows
5. Cross-agent context bridge
6. Heatmaps + analytics

## 10. CLI Reference

```
at                          # System status
at sling <bead> <agent>     # Assign work to agent
at hook <bead> <agent>      # Pin work to agent
at done <bead> [--fail]     # Complete work
at nudge <agent> -m <msg>   # Notify agent
at convoy create <name>     # Create convoy
at convoy start <name>      # Start convoy
at mail send <to> -m <msg>  # Send mail
at polecat spawn <name>     # Spawn polecat worker
at polecat list             # List polecats
at session list             # List terminal sessions
at session attach <id>      # Attach to session
at tui                      # Launch TUI dashboard
at status                   # System overview
at cost                     # Cost report
at cost forecast            # 30-day forecast
```

## 11. Configuration

```toml
# ~/.auto-tundra/config.toml

[general]
rig = "mbp_m1"
default_lane = "standard"
max_agents = 20
max_ptys = 20

[dolt]
host = "127.0.0.1"
port = 3306
database = "auto_tundra"
data_dir = "~/.auto-tundra/dolt"

[cache]
sqlite_path = "~/.auto-tundra/cache.db"
ttl_seconds = 30

[providers]
default = "openrouter"

[providers.openrouter]
api_key_env = "OPENROUTER_API_KEY"
default_model = "anthropic/claude-sonnet-4"

[providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
default_model = "claude-sonnet-4-20250514"

[providers.openai]
api_key_env = "OPENAI_API_KEY"
default_model = "gpt-4o"

[providers.google]
api_key_env = "GOOGLE_API_KEY"
default_model = "gemini-2.5-pro"

[agents.claude]
binary = "claude"
args = ["--dangerously-skip-permissions"]
timeout_seconds = 600

[agents.codex]
binary = "codex"
args = ["--approval-mode", "full-auto"]
timeout_seconds = 600

[agents.gemini]
binary = "gemini"
args = []
timeout_seconds = 600

[agents.opencode]
binary = "opencode"
args = []
timeout_seconds = 600

[security]
tool_firewall = true
input_sanitization = true
max_tool_calls_per_turn = 10

[daemon]
heartbeat_interval_seconds = 30
patrol_interval_seconds = 60
kpi_snapshot_interval_seconds = 300

[ui]
theme = "dark"
refresh_interval_ms = 2000
terminal_scrollback_lines = 5000

[bridge]
kafka_enabled = false
kafka_brokers = "localhost:9092"
webhook_enabled = false
```

## 12. Open Questions

1. **Dolt server management:** Should auto-tundra start/stop a local Dolt SQL server, or expect it to be running?
   - Recommendation: Auto-start on first run, managed as a sidecar process.

2. **Agent CLI version pinning:** How to handle breaking changes in Claude Code / Codex / Gemini CLI APIs?
   - Recommendation: Version detection in CLI adapters with fallback behavior.

3. **Multi-rig coordination:** How do multiple auto-tundra instances on different machines coordinate?
   - Recommendation: Dolt remote for data sync, Kafka bridge for real-time events.

4. **WASM plugin system:** Should we support Zellij-style WASM plugins for extensibility?
   - Recommendation: Phase 7 feature. Use Zellij's plugin system via sidecar first.
