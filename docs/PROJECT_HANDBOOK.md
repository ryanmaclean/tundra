# Auto-Tundra Project Handbook

**[← Back to README](../README.md)** | **[Getting Started](../GETTING_STARTED.md)** | **[Contributing](../CONTRIBUTING.md)**

> Single source of truth for architecture, CLI, research, evaluations, observability, and execution status.
> Consolidated from the former root-level docs on 2026-02-21.

---

## 📋 Table of Contents

1. [Architecture](#1-architecture)
2. [CLI Guide](#2-cli-guide)
3. [Observability](#3-observability)
4. [Research & Evaluations](#4-research--evaluations)
5. [macOS Native Integration](#5-macos-native-integration)
6. [Execution History](#6-execution-history)

---

# 1. Architecture

## 1.1 System Overview

Auto-Tundra is a **layered, modular architecture** with clear separation of concerns:

```
┌──────────────────────────────────────────────────────────────┐
│                    USER INTERFACES                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │   CLI    │  │   TUI    │  │   HTTP   │  │  WebSocket│    │
│  │ (at-cli) │  │(at-tui)  │  │  API     │  │    API    │    │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬──────┘    │
└───────┼─────────────┼─────────────┼─────────────┼────────────┘
        │             │             │             │
        └─────────────┴─────────────┴─────────────┘
                            │
┌───────────────────────────▼────────────────────────────────────┐
│                      BRIDGE LAYER                              │
│                   ┌──────────────┐                             │
│                   │  at-bridge   │                             │
│                   │ HTTP/WS APIs │                             │
│                   └──────┬───────┘                             │
└──────────────────────────┼────────────────────────────────────┘
                           │
┌──────────────────────────▼────────────────────────────────────┐
│                   ORCHESTRATION LAYER                          │
│                   ┌──────────────┐                             │
│                   │  at-daemon   │                             │
│                   │ Orchestrator │                             │
│                   │  Event Bus   │                             │
│                   └──────┬───────┘                             │
└──────────────────────────┼────────────────────────────────────┘
                           │
        ┌──────────────────┼──────────────────┐
        │                  │                  │
┌───────▼────────┐  ┌──────▼───────┐  ┌──────▼──────┐
│   at-agents    │  │at-intelligence│  │ at-harness  │
│  Agent Roles   │  │  LLM Calls    │  │  Providers  │
│   Executor     │  │ Model Router  │  │Rate Limiter │
│   Registry     │  │ Cost Tracker  │  │Circuit Break│
└───────┬────────┘  └──────┬───────┘  └──────┬──────┘
        │                  │                  │
        └──────────────────┼──────────────────┘
                           │
┌──────────────────────────▼────────────────────────────────────┐
│                    FOUNDATION LAYER                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐     │
│  │ at-core  │  │at-session│  │at-integr │  │at-telemetry│   │
│  │  Types   │  │   PTY    │  │GitHub/GL │  │  Metrics  │    │
│  │ Config   │  │ Terminal │  │  Linear  │  │  Tracing  │    │
│  │ Context  │  └──────────┘  └──────────┘  └──────────┘     │
│  └──────────┘                                                 │
└───────────────────────────────────────────────────────────────┘
```

**Layer Responsibilities:**

1. **User Interfaces** — Entry points for interaction (CLI, TUI, APIs)
2. **Bridge Layer** — HTTP/WebSocket APIs for external clients
3. **Orchestration Layer** — Task coordination, event distribution
4. **Execution Layer** — Agent execution, LLM calls, provider management
5. **Foundation Layer** — Core types, configuration, integrations

## 1.2 Crate Dependency Graph

```
at-cli ────────┐
at-tui ────────┤
at-bridge ─────┼─────→ at-daemon ─────┬─────→ at-agents ────┐
app/tauri ─────┘                      │                      │
                                      ├─────→ at-intelligence│
                                      │                      │
                                      └─────→ at-harness ────┤
                                                              │
                                                              ├─→ at-core
                                                              ├─→ at-session
                                                              ├─→ at-integrations
                                                              └─→ at-telemetry
```

## 1.3 Crate Details

### `at-core` (Foundation)

Core types, configuration, context engine.

| Module | Purpose |
|--------|---------|
| `types::` | Domain types (`AgentRole`, `BeadState`, etc.) |
| `config::` | Configuration loading and validation |
| `context_engine::` | Context graph, progressive disclosure |
| `workflow::` | Workflow DSL for task orchestration |
| `health::` | Health check system |

**Dependencies:** Minimal (serde, tokio, anyhow)

### `at-agents` (Agent Execution)

Agent roles, execution, registry, lifecycle management.

| Module | Purpose |
|--------|---------|
| `roles::` | Predefined roles (Spec, QA, Build, Utility, Ideation) |
| `executor::` | Agent task execution engine |
| `registry::` | Agent and skill registration/discovery |
| `prompts::` | Prompt templates per role |
| `lifecycle::` | Agent startup, shutdown, state management |
| `claude_runtime::` | Claude SDK session runtime *(Wave 2)* |

**Dependencies:** at-core, at-intelligence, at-harness

### `at-intelligence` (LLM Integration)

LLM provider abstraction, model routing, cost tracking.

| Module | Purpose |
|--------|---------|
| `providers::` | Provider implementations (Anthropic, OpenRouter, OpenAI) |
| `router::` | Intelligent model routing and failover |
| `api_profiles::` | Profile registry with local/cloud providers |
| `cache::` | Token-level caching for cost savings |
| `cost::` | Usage tracking and cost estimation |
| `memory::` | Conversation memory management |

**Provider Support:**

| Provider | Models | Features |
|----------|--------|----------|
| Anthropic | Claude 3/4 | Streaming, tool use, artifacts |
| OpenRouter | 100+ models | Multi-model access, free tier |
| OpenAI | GPT-3.5/4 | Chat, embeddings, function calling |
| Local (vllm.rs/candle) | Various | Offline inference *(Wave 2, in progress)* |

### `at-harness` (Provider Infrastructure)

Rate limiting, circuit breaking, security, MCP protocol.

| Feature | Detail |
|---------|--------|
| Rate Limiting | Token bucket, prevent API abuse |
| Circuit Breaker | Fail fast on provider issues |
| Retries | Exponential backoff with jitter |
| Timeouts | Configurable request timeouts |
| Fallback | Automatic provider failover |

### `at-daemon` (Orchestration)

Main orchestrator, task pipeline, event bus.

**Bead State Machine:**
```
     sling          hook           done
(none) ───→ [slung] ───→ [hooked] ───→ [done]
                           │
                           │ nudge
                           └─→ [retry]
```

**Storage:** SQLite via Rusqlite (transactional daemon state).

### `at-bridge` (API Layer)

HTTP/WebSocket APIs for external clients.

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/beads` | POST | Create bead |
| `/api/beads/:id` | GET | Get bead status |
| `/api/beads/:id/hook` | PUT | Hook bead |
| `/api/beads/:id/done` | PUT | Mark done |
| `/api/status` | GET | System status |
| `/api/gitlab/merge-requests/{iid}/review` | POST | Real MR review *(Wave 4)* |
| `/ws` | WS | Real-time updates |

**[→ WebSocket API Reference](./WEBSOCKET_API.md)** — Comprehensive guide for real-time event streaming, terminal I/O, connection setup, and client examples.

### `at-session` (Terminal Management)

PTY terminal pool for command execution. Full terminal emulation, reusable sessions, output capture, signal handling.

### `at-integrations` (External Systems)

GitHub (PRs, issues), GitLab (MRs, CI/CD), Linear (issue tracking). All env/config-driven since Wave 3.

### `at-telemetry` (Observability)

Structured logging (JSON), distributed tracing (trace ID propagation), metrics (counter/gauge/histogram), Datadog integration. See [§3 Observability](#3-observability).

### `at-cli` (Command-Line Interface)

See [§2 CLI Guide](#2-cli-guide) for full reference.

### `at-tui` (Terminal UI)

Real-time bead board visualization, agent monitoring. Built with ratatui + crossterm.

## 1.4 Agent Lifecycle & Execution

**[→ Detailed Architecture Documentation](../crates/at-agents/ARCHITECTURE.md)**

The `at-agents` crate implements a robust agent execution system with deterministic lifecycle management. Agents progress through a **7-state state machine** with 11 valid transitions:

```
Idle → Spawning → Active → Stopping → Stopped
           ↓          ↓         ↓
         Failed ← ← ← ← ← (recoverable)
           ↓
         Idle (via Recover transition)
```

**Key Components:**

- **AgentSupervisor** — Spawns agents, manages state transitions, monitors health
- **AgentStateMachine** — Enforces valid state transitions with complete history tracking
- **Orchestrator** — High-level task decomposition, context steering, stuck detection
- **TaskRunner** — Multi-phase pipeline (Discovery → Planning → Coding → QA → Complete)
- **AgentExecutor** — PTY process spawning, output parsing, tool approval enforcement
- **ToolApprovalSystem** — Security gates for tool invocations with role-based policies

**Agent Lifecycle Flow:**

1. **Creation** — `Supervisor::spawn_agent()` creates agent in `Idle` state
2. **Spawning** — Transition to `Spawning`, call `on_start()`, allocate resources
3. **Active** — Transition to `Active`, agent ready to receive and execute tasks
4. **Task Execution** — Multi-turn conversation with Claude via `ClaudeRuntime`
5. **Completion** — Transition to `Stopping`, call `on_stop()`, clean up resources
6. **Termination** — Final transition to `Stopped` state

**Failure Recovery:**

When an agent fails, it enters the `Failed` state and can be recovered via the `Recover` transition, which moves it back to `Idle` for a fresh restart. This enables resilient agent management without losing task context.

For comprehensive details on state transitions, component relationships, task execution pipeline, and tool approval policies, see the [at-agents ARCHITECTURE.md](../crates/at-agents/ARCHITECTURE.md).

## 1.5 Data Flow

```
 1. User creates task        →  at sling "Fix bug"
 2. CLI sends HTTP POST      →  POST /api/beads
 3. Bridge forwards          →  daemon.create_bead(title, lane)
 4. Daemon stores            →  SQLite INSERT, state='slung'
 5. User hooks task           →  at hook bead_001
 6. State change             →  slung → hooked
 7. Agent selected           →  registry.select_agent_for_task()
 8. Context loaded           →  context_engine.collect_context()
 9. LLM called               →  provider.chat(messages, tools)
10. Harness gate             →  rate_limiter → circuit_breaker
11. API request              →  POST provider API
12. Response streamed back   →  provider → intelligence → agents → daemon
13. Tools executed           →  executor.execute_tool()
14. User marks complete      →  at done bead_001
```

## 1.6 Context Engine & Progressive Disclosure

Context is loaded incrementally based on token budget:

```rust
let context = engine.collect_context("task-123", 8000)?;
// Priority: task → agent → skills → references
// Returns: most relevant context within budget
```

## 1.7 Multi-Provider Failover

```
Primary (Anthropic) → Secondary (OpenRouter) → Tertiary (OpenAI)
```

Each step is attempted only when the previous provider is rate-limited or errors.

## 1.8 Extension Points

**Adding a new agent role:**
1. Define variant in `at-core/src/types.rs` → `AgentRole::NewRole`
2. Implement `RoleConfig` in `at-agents/src/roles/new_role.rs`
3. Register in `at-agents/src/registry.rs`

**Adding a new provider:**
1. Implement `Provider` trait in `at-intelligence`
2. Add to `ProfileRegistry` via `registry.add_provider()`

---

# 2. CLI Guide

## 2.1 Installation

```bash
cargo build --release --bin at        # Build
cargo install --path crates/at-cli    # Install globally
at --help
```

## 2.2 Global Options

| Option | Default | Purpose |
|--------|---------|---------|
| `--api-url <URL>` | `http://localhost:9090` | Daemon API endpoint |

## 2.3 Command Reference

| Command | Purpose | State Change | Example |
|---------|---------|--------------|---------|
| `status` | Show system state | — | `at status` |
| `sling` | Create task | → slung | `at sling "Fix bug"` |
| `hook` | Start task | slung → hooked | `at hook bead_001` |
| `done` | Complete task | hooked → done | `at done bead_001` |
| `nudge` | Restart agent | retries | `at nudge agent_001` |
| `skill list` | Discover project skills | — | `at skill list -p .` |
| `skill show` | Show one skill body | — | `at skill show -s wave-execution -p .` |
| `run` | Create skill-aware task | backlog + task | `at run -t "Fix OAuth" -s integration-hardening -p .` |
| `agent run` | Role-scoped skill-aware task | backlog + task + execute | `at agent run -r qa-reviewer -t "Audit PR flow" -s wave-execution -p .` |
| `doctor` | Environment/connectivity checks | — | `at doctor -p . -S` |
| `smoke` | Browser runtime smoke (WebGPU + audio cues) | — | `at smoke -p . -S` |

### Core Commands

**`sling`** — Create a new bead (task).
```bash
at sling "Fix login bug"               # Standard priority
at sling "Security patch" --lane critical
at sling "Try new pattern" --lane experimental
```
Lanes: `experimental` (low), `standard` (default), `critical` (high).

**`hook`** — Start processing a queued task.
```bash
at hook bead_a1b2c3d4
```

**`done`** — Mark a bead as complete.
```bash
at done bead_a1b2c3d4
```

**`nudge`** — Send restart signal to a stuck agent. ⚠️ Use sparingly.
```bash
at nudge agent_spec_001
```

### Skill-Aware Commands

```bash
# List all project skills
at skill list -p /path/to/project

# Show a specific skill
at skill show -s integration-hardening -p . -f -j

# Create skill-aware task (no execute)
at run -t "Wire GitLab MR review UX" \
  -s integration-hardening -s wave-execution \
  -p . --no-execute

# Dry-run: preview prompt/payload locally (no daemon calls)
at run -t "Plan wave" -s wave-execution -p . --dry-run --emit-prompt

# Role-scoped execution
at agent run -r qa-reviewer -t "Review auth" -s wave-execution -p . -m sonnet -n 2

# Doctor checks (JSON + strict for CI)
at doctor -p . -j -S

# Browser runtime smoke (auto-serves app/leptos-ui/dist)
at smoke -p . -S
```

`doctor` validates: daemon reachability, env vars, project path, `.claude/skills/*/SKILL.md` count.
`smoke` validates: JS bridges loaded, WebGPU probe callable, AudioWorklet warmup + cue playback.

## 2.4 Workflows

```bash
# Basic flow
at status                       # 1. Check system
at sling "Implement feature X"  # 2. Create task → bead_abc123
at hook bead_abc123             # 3. Start work
at status                       # 4. Monitor
at done bead_abc123             # 5. Complete

# Recovery from stuck agent
at status                       # Identify stuck agent
at nudge agent_spec_001         # Restart
at status                       # Verify recovery
```

## 2.5 Troubleshooting

| Problem | Cause | Solution |
|---------|-------|----------|
| "Connection refused" | Daemon not running | `cargo run --bin at-daemon` first |
| "Bead not found" | Typo or different instance | `at status` to list all beads |
| Agent unresponsive | Stuck processing | `at nudge <agent-id>`, check `RUST_LOG=debug` |
| Rate limit exceeded | Provider throttled | Auto-failover; add `OPENROUTER_API_KEY` |

---

# 3. Observability

## 3.1 Stack Overview

**Primary stack:** OpenTelemetry → Datadog (vendor-neutral, industry standard).

```
Application (Rust, at-telemetry crate)
    ↓
OpenTelemetry SDK (tracing + metrics)
    ↓
Datadog Exporter / OTLP
    ↓
Datadog Agent (localhost:8126)
    ↓
Datadog Cloud
```

## 3.2 Core Dependencies

```toml
[dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
opentelemetry = { version = "0.22", features = ["trace", "metrics"] }
opentelemetry-sdk = { version = "0.22", features = ["rt-tokio"] }
opentelemetry-datadog = "0.10"
tracing-opentelemetry = "0.23"
```

## 3.3 Datadog APM Setup

```rust
use opentelemetry_datadog::DatadogPropagator;

pub fn init_datadog_tracing(service_name: &str) -> Result<()> {
    global::set_text_map_propagator(DatadogPropagator::new());

    let tracer = opentelemetry_datadog::new_pipeline()
        .with_service_name(service_name)
        .with_agent_endpoint("http://localhost:8126")
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let subscriber = Registry::default()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .with(telemetry);

    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
}
```

## 3.4 Instrumentation Patterns

```rust
#[instrument(name = "process_request", skip(request), fields(user_id = %request.user_id))]
async fn process_request(request: Request) -> Result<Response, Error> {
    info!("Processing request");
    let result = fetch_data(&request.user_id).await?;
    Ok(Response::new(result))
}
```

**Database instrumentation:**
- **SQLite (sqlx):** Auto-instrumented when `tracing` feature is enabled.
- **DuckDB:** Requires custom wrapper (see `InstrumentedDuckDB` pattern).

## 3.5 WASM Tracing

```rust
#[cfg(target_arch = "wasm32")]
pub fn init_wasm_tracing() {
    use tracing_wasm::{WASMLayer, WASMLayerConfig};
    let config = WASMLayerConfig::default()
        .set_max_level(tracing::Level::INFO);
    tracing_subscriber::registry()
        .with(WASMLayer::new(config))
        .init();
}
```

## 3.6 Logging Pipeline

**Application** → JSON structured logs → **Vector** → **Datadog Logs**

```toml
# vector.toml
[sources.app_logs]
type = "file"
include = ["/var/log/app/*.log"]

[transforms.parse_json]
type = "remap"
inputs = ["app_logs"]
source = '''
. = parse_json!(.message)
.service = "auto-tundra"
.ddsource = "rust"
'''

[sinks.datadog_logs]
type = "datadog_logs"
inputs = ["parse_json"]
default_api_key = "${DATADOG_API_KEY}"
```

## 3.7 Key Metrics to Track

| Category | Metrics |
|----------|---------|
| APM | Request rate, error rate, P50/P95/P99 latency, Apdex |
| Database | Query duration, pool utilization, slow queries (>100ms) |
| System | CPU, memory, thread count |
| Custom | LLM cost per task, cache hit rate, agent throughput |

## 3.8 Production Checklist

- [ ] OpenTelemetry SDK configured
- [ ] Datadog Agent running (localhost:8126)
- [ ] Service name and version set
- [ ] Environment tags configured
- [ ] Sampling strategy defined
- [ ] Database queries instrumented
- [ ] Error tracking enabled
- [ ] Log aggregation configured (Vector)
- [ ] Profiling enabled (pprof + Datadog continuous profiler)
- [ ] Dashboards and alerts created

## 3.9 Alternatives

| Tool | Rust Support | Trade-off |
|------|-------------|-----------|
| Datadog | ✅ Excellent | Full-featured, expensive |
| Honeycomb | ✅ Excellent | Best high-cardinality, expensive |
| Grafana Cloud | ✅ Good | Open source, complex setup |
| Jaeger | ✅ Excellent | Free/self-hosted, limited features |

---

# 4. Research & Evaluations

## 4.1 Core Architecture Research

### Crossbeam (Concurrency Primitives)
Use `crossbeam-channel` for high-throughput MPMC message passing between daemon, agents, and PTYs.

### Rayon in WASM
**Possible with caveats.** Requires `+atomics` + `+bulk-memory` target features, COOP/COEP headers, and a Web Worker pool via `wasm-bindgen-rayon`. Use case: offloading heavy client-side analytics without blocking UI.

## 4.2 Technology Evaluations

### DuckDB WASM — ✅ Approved for Frontend Analytics

**Decision:** Use DuckDB WASM in the Leptos frontend (`app/leptos-ui`) for client-side analytics and cost rollups.

**Architecture:** SQLite for OLTP daemon state (server-side) + DuckDB WASM for OLAP frontend analytics (client-side). These are separate concerns.

**Pattern:**
1. Leptos fetches raw JSON from backend (`at-bridge`).
2. JSON registered in DuckDB WASM instance.
3. SQL queries run natively in browser.
4. Results bound to Leptos via Signals.

**Action items:**
- [x] Document architecture and integration path
- [ ] Add `js-sys` and `wasm-bindgen-futures` to `app/leptos-ui`
- [ ] Create `duckdb.js` wrapper in `public/`
- [ ] Bind functions in `analytics.rs`

**Open research:** Benchmark dashboard rollups (WASM vs API-side), decide persistence mode (in-memory vs OPFS).

### Zellij Terminal Multiplexer — ❌ Deferred

**Decision: Defer.** Zellij's visual panes interfere with agent screen reading. If multi-pane is needed for human users, implement pane splitting natively in Leptos frontend with separate `at-session` PTYs.

**Why not Zellij sidecar:**
- Adds ~15-20MB binary weight
- Extra latency in PTY → multiplexer → WebSocket → xterm.js pipeline
- Complex ANSI output makes agent raw-text reads unreliable

### Rig Framework — Adopt as LLM Abstraction
Significant overlap with `at-agents` and `at-intelligence`. **Action:** Adopt Rig as a dependency for low-level LLM interactions; focus our crates on higher-level orchestration, Kanban, and UI.

### Nushell Embedding — ✅ Viable
Nu engine crates (`nu-cli`, `nu-engine`, `nu-parser`) can be embedded into agent terminals for structured data-aware shell output (tables/JSON instead of raw text).

### Rodio (Sound Design) — Nice-to-Have
WASM-compatible audio. Play tactile sounds on agent events (task completion, errors). Low priority.

### RustDesk — Enterprise Use Case
Useful for agent-assisted remote pair programming. P2P screen/terminal sharing within Auto-Tundra for enterprise scenarios.

### Codex CLI — MIT, Adoptable
MIT-licensed. Fork/adopt REPL-style multi-turn patterns as a CLI frontend communicating with our daemon API.

## 4.3 Wave 2 Research Tracks

| # | Topic | Status | Details |
|---|-------|--------|---------|
| 13 | Claude SDK RS session runtime | In progress | Map `SessionManager` to `at-session` PTY lifecycle |
| 14 | DuckDB WASM + SQLite split | Approved | See §4.2 above |
| 15 | Local LLM (vllm.rs + candle) | In progress | `ProviderKind::Local` profile, Metal backend |
| 16 | rustic-rs backup/snapshots | Evaluating | vs git-based recovery, nightly `.tundra` snapshots |
| 17 | git2-rs migration | In progress | Read ops via libgit2, shell fallback for writes |
| 18 | macOS Tahoe native UI | In progress | See [§5 macOS](#5-macos-native-integration) |

**Research team mapping:**
- **Team A (Data + Storage):** DuckDB split, persistent-scheduler, rustic-rs
- **Team B (Git + Workflow):** git2-rs migration, worktree parity
- **Team C (Inference + Agents):** Claude SDK RS, vllm.rs + candle local provider
- **Team D (Native UX):** macOS Tahoe native shell, HIG alignment

**Detailed execution artifacts:** `docs/research/wave2/`

## 4.4 Utility Libraries

| Library | Purpose | Integration Point |
|---------|---------|-------------------|
| Cortex-Mem | LLM context memory management | Context pruning/summarization |
| Rust_Search | Fast file search | Agent workspace search (replaces `fd`/`rg` shelling) |
| Persistent-Scheduler | Job scheduling with DB persistence | Daily syncs, nightly context pruning |

---

# 5. macOS Native Integration

## 5.1 Strategy

**Primary approach: CSS-first** with selective native Tauri plugins.

> **Do NOT** adopt cacao, swift-bridge, or pure Rust GUI frameworks. They break cross-platform builds and the Leptos WASM advantage.

**Two-layer strategy:**

| Layer | Scope | Risk |
|-------|-------|------|
| Layer 1: CSS-based UI | SF Pro fonts, dark mode detection, `backdrop-filter` vibrancy, HIG layouts | Low |
| Layer 2: Native Tauri plugins | System appearance detection, menu bar, advanced vibrancy | Medium |

## 5.2 HIG 2026 Compliance (macOS Tahoe)

**Design principles:** Clarity, Consistency, Feedback, Efficiency, Delight.

**Typography:** SF Pro primary (`-apple-system, BlinkMacSystemFont`), Inter fallback.

**Color system:** Light mode (silver/graphite), dark mode (current palette). Support user accent color via `NSAppearance`.

**Spacing:** 8px baseline grid, 16px edge margins, 8-12px component gaps.

**Visual effects:** `backdrop-filter: blur()` vibrancy, 8-12px rounded corners, subtle shadows on floating panels.

**Navigation:** Sidebar (200-280px, collapsible, drag-to-resize), tab bar, toolbar, native context menus.

**Keyboard shortcuts:** Standard macOS (Cmd+Q/W/N/S/,) via Tauri native menu bar or JS event listeners.

**Accessibility:** VoiceOver, Voice Control, high contrast, `prefers-reduced-motion`, ARIA labels.

## 5.3 Tauri Configuration

```json
{
  "app": {
    "windows": [{
      "title": "auto-tundra",
      "transparent": true,
      "decorations": true,
      "titleBarStyle": "overlay",
      "width": 1280, "height": 800,
      "minWidth": 900, "minHeight": 600,
      "backgroundColor": "#0f0a1a"
    }]
  },
  "bundle": {
    "macOS": {
      "minimumSystemVersion": "12.0"
    }
  }
}
```

## 5.4 CSS Enhancements

```css
:root {
  --font-system: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Helvetica Neue', sans-serif;
  --font-mono: 'Menlo', 'Monaco', 'Courier New', monospace;
}

@media (prefers-color-scheme: light) {
  :root { --bg-primary: #ffffff; --bg-secondary: #f5f5f7; }
}
@media (prefers-color-scheme: dark) {
  :root { --bg-primary: #0f0a1a; }
}

.vibrancy-sidebar {
  background-color: rgba(21, 14, 36, 0.95);
  backdrop-filter: blur(20px);
}
```

## 5.5 Native Plugin (Optional, Phase 3)

For system appearance detection (accent color, dark mode) and menu bar integration, create a `tauri-plugin-macos-native` wrapping `objc2`:

```rust
#[tauri::command]
#[cfg(target_os = "macos")]
pub async fn get_system_appearance() -> Result<SystemAppearance> {
    // objc2 calls to NSAppearance
}
```

**Anti-recommendations:** Do **not** use cacao (breaks cross-platform), swift-bridge (immature), or private macOS APIs.

## 5.6 Implementation Roadmap

| Phase | Scope | Effort | Risk |
|-------|-------|--------|------|
| 1. CSS & Layout | Font stack, dark mode, vibrancy, HIG spacing | 1-2 days | Low |
| 2. Tauri Config | Transparent titlebar, window sizing | 1 day | Medium |
| 3. Native Plugin | System appearance, menu bar, deep vibrancy | 2-3 days | Higher |
| 4. Leptos Components | Collapsible sidebar, keyboard shortcuts, a11y | 3-5 days | Medium |

---

# 6. Execution History

## 6.1 Phase 4: UI Polish & Wire-up

### Frontend Team
| Task | Location | Goal |
|------|----------|------|
| Task Detail Polish | `task_detail.rs` | Right sidebar metadata layout |
| Subtask Checklist | `task_detail.rs`, `style.css` | Interactive checklist, file tree diff |
| Wire UI Elements | `edit_task_modal.rs`, `api.rs` | `api::update_task`, EditTaskModal save |

### Backend Team
| Task | Location | Goal |
|------|----------|------|
| Expand Task API | `http_api.rs`, `types.rs` | `PUT /api/tasks/{id}` with tags, agent_profile, model |
| Persist Subtasks | Rusqlite | Replace `demo_subtasks` mock with real persistence |
| Execution Logs | `http_api.rs` | Stream/poll tokio tracing logs per `task_id` |

## 6.2 Wave 2 (Completed)

| Lane | Scope | Key Deliverables |
|------|-------|-----------------|
| B — Git Read Migration | Adapter-first conflict detection | `GitReadAdapter::conflict_files`, shell fallback |
| C — Local Profile Bootstrap | Wire local provider to startup | `ResilientRegistry::from_config`, profile selection logging |
| D — Native Shell Verification | Executable HIG checklist | `tests/interactive/test_native_shell_mode.sh` |

## 6.3 Wave 3 (Completed)

| Lane | Scope | Key Deliverables |
|------|-------|-----------------|
| A — Skill Guardrails | Project-local skills | `.claude/skills/wave-execution/SKILL.md`, `.claude/skills/integration-hardening/SKILL.md` |
| B — Integration Hardening | Remove stub tokens | GitLab/Linear env/config-driven, real `MrReviewEngine` |
| C — Demo/Live Bootstrap | Auto-hydrate state | Startup async fetch, `is_demo` auto-flip |
| D — Git2 Read Migration | `list_worktrees` via libgit2 | libgit2-first with shell fallback |

## 6.4 Wave 4 (Completed)

| Lane | Scope | Key Deliverables |
|------|-------|-----------------|
| A — GitLab MR Review UI | Frontend MR review | `fetch_gitlab_merge_requests()`, `Review MR` button |
| B — Env-Driven Test Coverage | Handler tests | GitLab/Linear missing-token/project tests |
| C — Contract Tightening | Docs + runtime alignment | 400 on missing `team_id`, API contract doc |

## 6.5 Wave 5 (Completed)

| Lane | Scope | Key Deliverables |
|------|-------|-----------------|
| A — Skill CLI Surface | Scriptable skill commands | `at skill list`, `at skill show`, JSON output |
| B — Skill-Aware Task Runner | CLI task execution | `at run -t ... -s ...`, `at agent run` |
| C — Doctor + CLI Docs | Operational checks | `at doctor`, `--strict`, CLI_GUIDE updated |

## 6.6 Wave 6 (Completed)

| Lane | Scope | Key Deliverables |
|------|-------|-----------------|
| A — Dry-Run Prompt Tooling | Preview mode | `--dry-run`, `--emit-prompt` |
| B — CLI Robustness | Parsing hardening | Non-JSON fallback, improved error messages |
| C — Regression Tests | Unit tests | 5 `at-cli` tests for new commands |

## 6.7 Wave 2 Research Execution Artifacts

All research execution docs are in `docs/research/wave2/`:

| Team | Artifacts |
|------|-----------|
| A (Data) | `team-a-data-storage.md`, `team-a-oltp-olap-boundary.md`, `team-a-benchmark-plan.md` |
| B (Git) | `team-b-git-workflow.md`, `team-b-migration-matrix.md`, `team-b-integration-api-contract.md` |
| C (Inference) | `team-c-inference-agents.md`, `team-c-integration-touchpoints.md`, `team-c-local-provider-schema.md`, `team-c-session-pty-lifecycle.md` |
| D (Native UX) | `team-d-native-ux.md`, `team-d-native-hig-checklist.md`, `team-d-native-shell-milestones.md` |

---

## 📚 Related Documentation

- **[README.md](../README.md)** — Project overview
- **[GETTING_STARTED.md](../GETTING_STARTED.md)** — Initial setup
- **[CONTRIBUTING.md](../CONTRIBUTING.md)** — Development guide
- **[AGENTS.md](../AGENTS.md)** — Agent configuration
- **[docs/research/wave2/](research/wave2/)** — Detailed research artifacts

---

**Document Version:** 2.0 (2026-02-21)
**Consolidated from:** ARCHITECTURE.md, CLI_GUIDE.md, OBSERVABILITY.md, RESEARCH_TODOS.md, DUCKDB_WASM_EVALUATION.md, ZELLIJ_EVALUATION.md, MACOS_NATIVE_INTEGRATION.md, AGENT_ASSIGNMENTS.md
