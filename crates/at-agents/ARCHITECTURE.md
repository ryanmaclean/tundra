# at-agents Architecture

**[← Back to Project Handbook](../../docs/PROJECT_HANDBOOK.md)**

> Comprehensive architecture documentation for the Auto-Tundra agent execution system.
> Last updated: 2026-02-23

---

## Table of Contents

1. [Overview](#1-overview)
2. [Agent Lifecycle Flow](#2-agent-lifecycle-flow)
3. [Component Relationships](#3-component-relationships)
4. [Agent State Machine](#4-agent-state-machine)
5. [Task Execution Pipeline](#5-task-execution-pipeline)
6. [Tool Approval System](#6-tool-approval-system)
7. [Integration Points](#7-integration-points)

---

## 1. Overview

The `at-agents` crate is the execution engine for autonomous AI agents in Auto-Tundra. It provides:

- **Agent Lifecycle Management** — Spawn, monitor, pause, resume, and stop agents
- **State Machine** — 7 states with 11 valid transitions ensuring deterministic agent behavior
- **Task Execution** — Multi-phase pipeline from discovery through completion
- **Tool Approval** — Gated tool invocations with per-role policies
- **Context Steering** — Progressive context assembly for optimal LLM prompting
- **Supervision** — Health monitoring, stuck detection, and failure recovery

### Architecture Philosophy

The agent system is designed with **layered responsibility**:

```
┌─────────────────────────────────────────────────────────────┐
│ High-Level: Orchestrator (Task decomposition, RLM patterns)│
└────────────────────┬────────────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────┐
│ Mid-Level: TaskRunner (Phase progression, context assembly)│
└────────────────────┬────────────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────┐
│ Low-Level: Executor (PTY spawning, output parsing, events) │
└────────────────────┬────────────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────┐
│ Foundation: State Machine + Lifecycle + Supervisor         │
└─────────────────────────────────────────────────────────────┘
```

**Separation of Concerns:**
- **Orchestrator** — What tasks to do and in what order
- **TaskRunner** — How to assemble context and prompts for each phase
- **Executor** — How to spawn agents, capture output, and handle tools
- **Supervisor** — Agent health and state transitions
- **State Machine** — Valid states and transitions

---

## 2. Agent Lifecycle Flow

An agent moves through a deterministic lifecycle from creation to termination.

### Full Lifecycle Diagram

```
                ┌──────────────────────────────────────┐
                │  User or Daemon requests new agent   │
                └──────────────┬───────────────────────┘
                               │
                               ▼
                        ┌─────────────┐
                        │    IDLE     │ ◄──────────────┐
                        └──────┬──────┘                │
                               │ Start                 │ Recover
                               ▼                       │
                        ┌─────────────┐                │
                        │  SPAWNING   │                │
                        └──┬───────┬──┘                │
                           │       │                   │
                   Spawned │       │ Fail              │
                           │       │                   │
                           ▼       ▼                   │
                    ┌─────────┐ ┌────────┐            │
                    │ ACTIVE  │ │ FAILED │────────────┘
                    └─┬──┬──┬─┘ └────────┘
                      │  │  │
              Pause   │  │  │ Fail
                      │  │  │
                      ▼  │  │
                  ┌────────┐│
                  │ PAUSED ││
                  └──┬──┬──┘│
                     │  │   │ Stop
          Resume     │  │   │
                     │  ▼   ▼
                     │ ┌──────────┐
                     └►│ STOPPING │
                       └─────┬────┘
                             │ Stop
                             ▼
                       ┌──────────┐
                       │ STOPPED  │
                       └──────────┘
```

### Lifecycle Callbacks

The `AgentLifecycle` trait defines hooks that are called at specific points:

```rust
pub trait AgentLifecycle: Send + Sync {
    fn role(&self) -> AgentRole;
    async fn on_start(&mut self) -> Result<()>;
    async fn on_task_assigned(&mut self, bead: &Bead) -> Result<()>;
    async fn on_task_completed(&mut self, bead_id: Uuid) -> Result<()>;
    async fn on_heartbeat(&mut self) -> Result<()>;
    async fn on_stop(&mut self) -> Result<()>;
}
```

**Lifecycle Flow:**

1. **Creation** — `Supervisor::spawn_agent()` creates agent in `Idle` state
2. **Spawning** — Transition `Start`, call `on_start()`, allocate resources
3. **Active** — Transition `Spawned`, agent ready to receive tasks
4. **Task Assignment** — Call `on_task_assigned()` when bead is hooked
5. **Heartbeat** — Periodic `on_heartbeat()` for health monitoring
6. **Task Completion** — Call `on_task_completed()` when bead is done
7. **Shutdown** — Transition `Stop`, call `on_stop()`, clean up resources
8. **Termination** — Final transition to `Stopped` state

### Failure Recovery

When an agent fails:
1. State transitions to `Failed`
2. Supervisor can call `restart_failed()` to recover
3. Transition `Recover` moves back to `Idle`
4. Agent is re-spawned through normal startup flow

---

## 3. Component Relationships

The agent system is composed of interconnected modules with clear responsibilities.

### Component Interaction Diagram

```
┌────────────────────────────────────────────────────────────────┐
│                       at-daemon                                │
│                    (Bead orchestration)                        │
└───────────────────────────┬────────────────────────────────────┘
                            │
                            │ Creates beads, requests execution
                            ▼
┌────────────────────────────────────────────────────────────────┐
│                      Orchestrator                              │
│  - Task decomposition (RLM patterns)                           │
│  - Context assembly via ContextSteerer                         │
│  - Prompt selection via PromptRegistry                         │
│  - Stuck detection and recovery                                │
└───────────────────────────┬────────────────────────────────────┘
                            │
                            │ Delegates phase execution
                            ▼
┌────────────────────────────────────────────────────────────────┐
│                       TaskRunner                               │
│  - Phase progression (Discovery → QA → Complete)               │
│  - Context steering per phase                                  │
│  - AgentSession communication                                  │
│  - Event publishing to EventBus                                │
└───────────────────────────┬────────────────────────────────────┘
                            │
                            │ Spawns agent and executes task
                            ▼
┌────────────────────────────────────────────────────────────────┐
│                       AgentExecutor                            │
│  - PTY process spawning                                        │
│  - Output parsing (JSON events, progress markers)              │
│  - Tool approval checks                                        │
│  - Timeout and abort handling                                  │
└───────────────┬───────────┬────────────────────────────────────┘
                │           │
                │           └─────────────┐
                │                         │
                ▼                         ▼
┌───────────────────────┐   ┌──────────────────────────┐
│   AgentSupervisor     │   │  ToolApprovalSystem      │
│  - Spawns agents      │   │  - Policy enforcement    │
│  - State transitions  │   │  - Pending approvals     │
│  - Health monitoring  │   │  - Per-role overrides    │
│  - Failure recovery   │   └──────────────────────────┘
└───────────┬───────────┘
            │
            │ Manages state
            ▼
┌──────────────────────────────────────────────────────────────┐
│                    AgentStateMachine                         │
│  - Current state                                             │
│  - Valid transitions                                         │
│  - Transition history                                        │
└──────────────────────────────────────────────────────────────┘
```

### Module Responsibilities

| Module | Responsibility | Key Types |
|--------|---------------|-----------|
| `orchestrator.rs` | High-level task coordination, RLM decomposition, context steering | `Orchestrator`, `TaskExecution`, `OrchestratorConfig` |
| `task_runner.rs` | Phase pipeline orchestration, context assembly, prompt selection | `TaskRunner` |
| `executor.rs` | Agent process spawning, output parsing, tool approval | `AgentExecutor`, `ExecutionResult`, `PtySpawner` |
| `supervisor.rs` | Agent lifecycle management, state transitions, health monitoring | `AgentSupervisor`, `AgentInfo`, `ManagedAgent` |
| `state_machine.rs` | State and event definitions, transition validation | `AgentStateMachine`, `AgentState`, `AgentEvent` |
| `lifecycle.rs` | Lifecycle trait and hooks | `AgentLifecycle` |
| `approval.rs` | Tool approval policies and pending approvals | `ToolApprovalSystem`, `ApprovalPolicy`, `PendingApproval` |
| `roles.rs` | Agent role implementations (Mayor, Deacon, Witness, etc.) | `MayorAgent`, `DeaconAgent`, `CrewAgent`, etc. |
| `prompts.rs` | Prompt template registry | `PromptRegistry`, `PromptTemplate` |
| `registry.rs` | Agent and skill registration | `AgentRegistry`, `SkillRegistry` |

---

## 4. Agent State Machine

The agent state machine defines 7 states and 11 valid transitions, enforcing deterministic lifecycle management.

### The 7 Agent States

```rust
pub enum AgentState {
    Idle,        // Agent created but not started
    Spawning,    // Starting up, allocating resources
    Active,      // Ready and executing tasks
    Paused,      // Temporarily suspended
    Stopping,    // Shutdown initiated
    Stopped,     // Fully terminated
    Failed,      // Error state, requires recovery
}
```

**State Categories:**

- **Initial State:** `Idle` — Agent created but not operational
- **Transitional States:** `Spawning`, `Stopping` — Intermediate states during lifecycle changes
- **Operational States:** `Active`, `Paused` — Agent is running (active) or suspended (paused)
- **Terminal States:** `Stopped`, `Failed` — End states (see detailed explanation below)

### State Machine Diagram

The following diagram shows all 7 states and 11 valid transitions:

```
                    ┌──────────┐
                    │   IDLE   │ ◄─────────────────┐
                    └────┬─────┘                   │
                         │ (1) Start               │
                         ▼                         │ (11) Recover
                    ┌──────────┐                   │
                    │ SPAWNING │                   │
                    └──┬────┬──┘                   │
                       │    │                      │
         (2) Spawned   │    │ (3) Fail             │
                       │    │                      │
                       ▼    ▼                      │
              ┌──────────┐ ┌────────┐             │
              │  ACTIVE  │ │ FAILED │─────────────┘
              └─┬──┬──┬──┘ └────▲───┘
                │  │  │         │
   (4) Pause    │  │  │         │ (6) Fail
                │  │  │         │ (10) Fail
                ▼  │  │         │
            ┌────────┐│         │
            │ PAUSED ││         │
            └──┬──┬──┘│         │
               │  │   │         │
   (7) Resume  │  │   │ (5) Stop│
               │  │   │ (8) Stop│
               │  ▼   ▼         │
               │ ┌──────────┐   │
               └►│ STOPPING │───┘
                 └─────┬────┘
                       │ (9) Stop
                       ▼
                 ┌──────────┐
                 │ STOPPED  │ (Terminal)
                 └──────────┘

Transitions:
(1)  Idle     + Start   → Spawning
(2)  Spawning + Spawned → Active
(3)  Spawning + Fail    → Failed
(4)  Active   + Pause   → Paused
(5)  Active   + Stop    → Stopping
(6)  Active   + Fail    → Failed
(7)  Paused   + Resume  → Active
(8)  Paused   + Stop    → Stopping
(9)  Stopping + Stop    → Stopped
(10) Stopping + Fail    → Failed
(11) Failed   + Recover → Idle
```

### The 11 State Transitions

Transitions are triggered by `AgentEvent` and enforced by the state machine:

```rust
pub enum AgentEvent {
    Start,    // Begin spawning
    Spawned,  // Spawning complete
    Pause,    // Suspend execution
    Resume,   // Resume from pause
    Stop,     // Initiate shutdown
    Fail,     // Error occurred
    Recover,  // Recover from failure
}
```

**Complete Transition Table:**

| # | From State | Event | To State | Description |
|---|-----------|-------|----------|-------------|
| 1 | `Idle` | `Start` | `Spawning` | Initiate agent startup |
| 2 | `Spawning` | `Spawned` | `Active` | Startup complete, agent ready |
| 3 | `Spawning` | `Fail` | `Failed` | Startup failed |
| 4 | `Active` | `Pause` | `Paused` | Temporarily suspend agent |
| 5 | `Active` | `Stop` | `Stopping` | Begin graceful shutdown |
| 6 | `Active` | `Fail` | `Failed` | Execution error |
| 7 | `Paused` | `Resume` | `Active` | Resume from pause |
| 8 | `Paused` | `Stop` | `Stopping` | Shutdown while paused |
| 9 | `Stopping` | `Stop` | `Stopped` | Shutdown complete |
| 10 | `Stopping` | `Fail` | `Failed` | Shutdown failed |
| 11 | `Failed` | `Recover` | `Idle` | Reset to initial state |

### Terminal States

The state machine defines two **terminal states**:

#### 1. Stopped State
- **Meaning:** Clean, graceful shutdown completed
- **Entry:** Via `Stopping + Stop` transition (#9)
- **Characteristics:**
  - No outgoing transitions (true terminal state)
  - Resources fully released
  - Agent removed from active supervision
  - Cannot be recovered or restarted
- **Use Case:** Normal agent lifecycle completion

#### 2. Failed State
- **Meaning:** Error occurred, agent in error state
- **Entry:** Via three possible transitions:
  - `Spawning + Fail` (#3) — Startup failure
  - `Active + Fail` (#6) — Execution failure
  - `Stopping + Fail` (#10) — Shutdown failure
- **Characteristics:**
  - Quasi-terminal (has one outgoing transition)
  - Resources may still be held
  - Agent remains under supervision
  - Can be recovered via `Recover` event
- **Use Case:** Recoverable errors, transient failures

**Key Difference:**
- `Stopped` is **permanent** — agent lifecycle is complete
- `Failed` is **recoverable** — agent can be restarted via recovery path

### Recovery Path from Failed State

When an agent enters `Failed` state, it can be recovered through the following path:

```
┌────────┐  Recover   ┌──────┐  Start   ┌──────────┐  Spawned   ┌────────┐
│ FAILED │ ─────────► │ IDLE │ ───────► │ SPAWNING │ ─────────► │ ACTIVE │
└────────┘            └──────┘          └──────────┘            └────────┘
```

**Recovery Process:**

1. **Detection** — Supervisor detects `Failed` state via health monitoring
2. **Recovery Decision** — Supervisor or user decides to attempt recovery
3. **Recover Transition** — `Failed + Recover → Idle` (transition #11)
4. **Resource Cleanup** — Failed agent's resources are released
5. **Fresh Start** — Agent transitions back to `Idle` state
6. **Re-spawning** — Normal startup flow resumes: `Idle → Spawning → Active`

**Implementation:**

```rust
impl AgentSupervisor {
    pub async fn recover_failed_agent(&mut self, agent_id: Uuid) -> Result<()> {
        let agent = self.get_agent_mut(agent_id)?;

        // Only Failed agents can be recovered
        if agent.state_machine.state() != AgentState::Failed {
            return Err(Error::InvalidState);
        }

        // Transition Failed → Idle
        agent.state_machine.transition(AgentEvent::Recover)?;

        // Clean up any held resources
        agent.cleanup_resources().await?;

        // Agent is now ready to be re-started
        Ok(())
    }
}
```

**Recovery Strategies:**

- **Automatic Recovery** — Supervisor can auto-recover after configurable backoff
- **Manual Recovery** — User explicitly requests recovery via CLI or API
- **Selective Recovery** — Only certain error types trigger auto-recovery
- **Recovery Limits** — Max recovery attempts to prevent infinite loops

### State Machine Properties

**Invariants:**
- Agents always start in `Idle`
- Only `Idle` can transition to `Spawning`
- `Failed` agents can only recover to `Idle`, not directly to `Active`
- `Stopped` is a true terminal state with no outgoing transitions
- `Failed` is a quasi-terminal state with one recovery transition
- All transitions are deterministic and validated

**Transition Validation:**

```rust
impl AgentStateMachine {
    pub fn transition(&mut self, event: AgentEvent) -> Result<AgentState> {
        // Validates transition and returns error if invalid
        // Records transition in history for debugging
    }

    pub fn can_transition(&self, event: AgentEvent) -> bool {
        // Check if transition is valid without applying it
    }
}
```

**Transition History:**

The state machine maintains a complete history of all transitions:

```rust
pub struct AgentStateMachine {
    current: AgentState,
    history: Vec<(AgentState, AgentEvent, AgentState)>,
}
```

This history enables:
- **Debugging** — Trace how an agent reached its current state
- **Auditing** — Verify state transition correctness
- **Monitoring** — Detect patterns in state changes
- **Recovery** — Understand failure context for better recovery decisions

---

## 5. Task Execution Pipeline

Tasks progress through a multi-phase pipeline, with each phase having specific goals and agent roles.

### Task Phase Flow

```
┌──────────────┐
│  Discovery   │ — Gather initial requirements, understand scope
└──────┬───────┘
       │
       ▼
┌──────────────────┐
│ ContextGathering │ — Collect relevant code, docs, context
└──────┬───────────┘
       │
       ▼
┌──────────────┐
│ SpecCreation │ — Write detailed specification
└──────┬───────┘
       │
       ▼
┌──────────────┐
│   Planning   │ — Design implementation approach
└──────┬───────┘
       │
       ▼
┌──────────────┐
│    Coding    │ — Implement the solution
└──────┬───────┘
       │
       ▼
┌──────────────┐
│      QA      │ — Test and validate
└──────┬───────┘
       │
       ├─ Pass ─► Complete
       │
       └─ Fail ─► Fixing ─┐
                          │
       ┌──────────────────┘
       │
       ▼
┌──────────────┐
│   Complete   │ — Task finished
└──────────────┘
```

### Phase Execution Flow

For each phase, the system:

1. **Context Assembly** — `ContextSteerer` assembles relevant context within token budget
2. **Prompt Selection** — `PromptRegistry` loads role-specific template
3. **Agent Spawning** — `Executor` spawns CLI agent via PTY pool
4. **Task Execution** — Agent processes task, produces output
5. **Output Parsing** — Parse structured events (`[PROGRESS]`, JSON events, `<tool_use_error>`)
6. **Tool Approval** — Check `ToolApprovalSystem` for any tool invocations
7. **Event Publishing** — Publish progress events to `EventBus`
8. **Phase Completion** — Determine next phase or terminal state

### TaskRunner API

```rust
impl TaskRunner {
    pub async fn run_phase(
        &mut self,
        task: &mut Task,
        session: &mut AgentSession,
        bus: &EventBus,
    ) -> Result<()> {
        // Execute one phase of the task pipeline
    }

    pub async fn run_full_pipeline(
        &mut self,
        task: &mut Task,
        session: &mut AgentSession,
        bus: &EventBus,
    ) -> Result<()> {
        // Run through all phases until completion or failure
    }
}
```

### Executor Internals

The `AgentExecutor` handles the low-level details:

```rust
impl AgentExecutor {
    pub async fn execute_task(
        &self,
        task: &Task,
        agent_config: &AgentConfig,
    ) -> Result<ExecutionResult> {
        // 1. Build CLI args from AgentConfig
        // 2. Spawn process via PtyPool
        // 3. Send prompt to stdin
        // 4. Collect output with timeout
        // 5. Parse structured events
        // 6. Publish to EventBus
        // 7. Return ExecutionResult
    }
}
```

**ExecutionResult:**

```rust
pub struct ExecutionResult {
    pub task_id: Uuid,
    pub success: bool,
    pub output: String,
    pub events: Vec<AgentEvent>,       // Parsed JSON events
    pub tool_errors: Vec<ToolUseError>, // <tool_use_error> tags
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
}
```

### Stuck Detection and Recovery

The `Orchestrator` uses `StuckDetector` to identify agents that are:
- Looping (same output pattern repeated)
- Stalled (no output for extended period)
- Token-exhausted (exceeding budget without progress)

**Recovery Actions:**
1. **Context Reduction** — Trim context and retry
2. **Prompt Simplification** — Use simpler template
3. **Task Decomposition** — Break into smaller subtasks
4. **Manual Intervention** — Request human input

---

## 6. Tool Approval System

The approval system controls which tools agents can invoke, providing security and human oversight.

### Approval Policies

```rust
pub enum ApprovalPolicy {
    AutoApprove,      // Trusted, no approval needed
    RequireApproval,  // Needs human approval
    Deny,             // Never allowed
}
```

### Default Policies

**Auto-Approved (Safe Read Operations):**
- `file_read`, `list_directory`, `search_files`
- `git_diff`, `git_log`, `git_blame`
- `task_status`

**Require Approval (Write Operations):**
- `file_write`, `file_delete`
- `git_commit`, `git_push`
- `run_command` (arbitrary shell commands)

**Denied (Dangerous Operations):**
- `rm -rf` patterns
- System modifications outside project root
- Network operations to unapproved hosts

### Per-Role Overrides

Different agent roles have different trust levels:

```rust
// Example: Mayor agents can auto-approve git commits
approval_system.add_role_override(
    "git_commit",
    AgentRole::Mayor,
    ApprovalPolicy::AutoApprove,
);
```

### Approval Flow

```
┌──────────────────────────────────────────────────────────┐
│  Agent invokes tool (detected in output parsing)        │
└──────────────────────┬───────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────┐
│  Executor.check_tool_event()                             │
│  - Extract tool name from event                          │
│  - Query ToolApprovalSystem                              │
└──────────────────────┬───────────────────────────────────┘
                       │
            ┌──────────┴──────────┐
            │                     │
            ▼                     ▼
     ┌─────────────┐       ┌─────────────┐
     │ AutoApprove │       │  Require    │
     │             │       │  Approval   │
     └──────┬──────┘       └──────┬──────┘
            │                     │
            ▼                     ▼
     ┌─────────────┐       ┌─────────────────────┐
     │   Execute   │       │ Create PendingApproval│
     │   tool      │       │ Wait for human input │
     └─────────────┘       └──────┬──────────────┘
                                  │
                        ┌─────────┴─────────┐
                        │                   │
                        ▼                   ▼
                  ┌──────────┐       ┌──────────┐
                  │ Approved │       │  Denied  │
                  └─────┬────┘       └─────┬────┘
                        │                  │
                        ▼                  ▼
                  ┌──────────┐       ┌──────────┐
                  │ Execute  │       │  Abort   │
                  └──────────┘       └──────────┘
```

### PendingApproval

```rust
pub struct PendingApproval {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub requested_at: DateTime<Utc>,
    pub status: ApprovalStatus,  // Pending, Approved, Denied
    pub resolved_at: Option<DateTime<Utc>>,
}
```

Approvals are stored in the system and can be queried via:
- CLI: `at-cli approvals list`
- API: `GET /api/approvals`
- Bridge: WebSocket events

---

## 7. Integration Points

The agent system integrates with other Auto-Tundra subsystems.

### at-daemon Integration

**Bead Orchestration:**
- Daemon creates beads and assigns them to agents
- Agents transition beads through states: `slung → hooked → done`
- Daemon monitors agent health via supervisor

```rust
// Daemon creates bead
let bead_id = daemon.sling(title, description)?;

// Supervisor spawns agent
let agent_id = supervisor.spawn_agent("builder", AgentRole::Coder, CliType::Claude).await?;

// Executor runs task
let result = executor.execute_task(&task, &agent_config).await?;

// Daemon marks bead complete
daemon.mark_done(bead_id)?;
```

### at-intelligence Integration

**Model Routing:**
- Agents request LLM calls through `at-intelligence`
- Intelligent routing selects optimal model
- Token usage tracked for cost monitoring

```rust
// AgentConfig specifies model preferences
let agent_config = AgentConfig {
    model: "claude-3-5-sonnet-20241022",
    temperature: 0.7,
    max_tokens: 4096,
    ..Default::default()
};
```

### at-session Integration

**PTY Management:**
- Executor spawns agent CLIs via `PtyPool`
- Full terminal emulation for interactive agents
- Output captured and parsed for events

```rust
// Executor uses PtyPool from at-session
let pty_pool = Arc::new(PtyPool::new(16));
let executor = AgentExecutor::new(pty_pool, event_bus, cache);
```

### at-bridge Integration

**Event Publishing:**
- Agents publish events to `EventBus`
- WebSocket clients receive real-time updates
- HTTP API exposes agent status and approvals

**Event Types:**
- `task_execution_start` — Task began
- `task_execution_complete` — Task succeeded
- `task_execution_failed` — Task failed
- `task_execution_timeout` — Task exceeded timeout
- `agent_output` — Incremental output from agent

### at-core Integration

**Context Steering:**
- Orchestrator uses `ContextSteerer` for progressive context assembly
- Token budget enforcement
- Context relevance ranking

**Types:**
- Agent roles defined in `at_core::types::AgentRole`
- Task types defined in `at_core::types::Task`
- Bead lifecycle defined in `at_core::types::Bead`

---

## Summary

The `at-agents` crate provides a robust, deterministic agent execution system with:

✅ **7-state, 11-transition state machine** for predictable lifecycle management
✅ **Multi-layered architecture** (Orchestrator → TaskRunner → Executor)
✅ **Tool approval system** for security and human oversight
✅ **Context steering** for optimal LLM prompting
✅ **Stuck detection** and automatic recovery
✅ **Event-driven** integration with rest of Auto-Tundra

**Key Design Principles:**
1. **Deterministic State Transitions** — No ambiguous states
2. **Separation of Concerns** — Each layer has clear responsibility
3. **Observability** — Events published for monitoring
4. **Failure Recovery** — Failed agents can be recovered
5. **Security** — Tool approvals prevent dangerous operations

For implementation details, see the source files:
- `state_machine.rs` — State definitions and transitions
- `supervisor.rs` — Agent lifecycle management
- `executor.rs` — Task execution and PTY handling
- `orchestrator.rs` — High-level coordination
- `task_runner.rs` — Phase pipeline
- `approval.rs` — Tool approval system
