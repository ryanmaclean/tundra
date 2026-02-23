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

### Component Hierarchy

The agent execution system follows a **layered architecture** with clear separation of concerns:

```
┌──────────────────────────────────────────────────────────┐
│ Layer 1: Strategic Planning (The "Brain")               │
│ ┌──────────────────────────────────────────────────────┐ │
│ │              ORCHESTRATOR                            │ │
│ │  • Task decomposition (RLM patterns)                 │ │
│ │  • Progressive refinement                            │ │
│ │  • Stuck detection & recovery                        │ │
│ │  • Context steering coordination                     │ │
│ │  • Prompt selection strategy                         │ │
│ └──────────────────────────────────────────────────────┘ │
└──────────────────────┬───────────────────────────────────┘
                       │ Delegates phase execution
                       ▼
┌──────────────────────────────────────────────────────────┐
│ Layer 2: Phase Orchestration                            │
│ ┌──────────────────────────────────────────────────────┐ │
│ │              TASK RUNNER                             │ │
│ │  • Phase progression pipeline                        │ │
│ │  • Context assembly per phase                        │ │
│ │  • Prompt template application                       │ │
│ │  • Event publishing (EventBus)                       │ │
│ │  • AgentSession communication                        │ │
│ └──────────────────────────────────────────────────────┘ │
└──────────────────────┬───────────────────────────────────┘
                       │ Spawns agents & executes tasks
                       ▼
┌──────────────────────────────────────────────────────────┐
│ Layer 3: Process Execution                              │
│ ┌──────────────────────────────────────────────────────┐ │
│ │              EXECUTOR                                │ │
│ │  • PTY process spawning                              │ │
│ │  • stdout/stderr capture & parsing                   │ │
│ │  • Tool approval enforcement                         │ │
│ │  • Timeout & abort handling                          │ │
│ │  • Error recovery (tool fallbacks)                   │ │
│ └──────────────────────────────────────────────────────┘ │
└──────────────────────┬───────────────────────────────────┘
                       │
                       │ Reports state changes
                       ▼
┌──────────────────────────────────────────────────────────┐
│ Layer 0: State & Lifecycle (Foundation)                 │
│ ┌──────────────────────────────────────────────────────┐ │
│ │              SUPERVISOR                              │ │
│ │  • Agent spawning & termination                      │ │
│ │  • State machine management                          │ │
│ │  • Health monitoring (heartbeats)                    │ │
│ │  • Failure detection & recovery                      │ │
│ │  • Lifecycle hook invocation                         │ │
│ └──────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
```

### Orchestrator: The "Brain"

The **Orchestrator** is the high-level decision-making component that sits at the top of the execution hierarchy. It is responsible for:

**Core Responsibilities:**

1. **Task Decomposition** — Uses RLM (Recursive Learning Model) patterns to break complex tasks into subtasks
2. **Context Steering** — Coordinates with `ContextSteerer` to assemble relevant context for each execution phase
3. **Prompt Selection** — Interfaces with `PromptRegistry` to select role-specific and phase-specific prompt templates
4. **Stuck Detection** — Monitors task execution for infinite loops, repeated failures, or excessive token usage
5. **Recovery Strategies** — Implements progressive refinement and synthesis strategies when tasks get stuck
6. **Insight Extraction** — Captures session insights (discoveries, gotchas, patterns) for future learning

**Key Data Structures:**

- `TaskExecution` — Tracks execution state, phase history, token usage, recoveries
- `OrchestratorConfig` — Configuration for token budgets, recursion depth, confidence thresholds
- `PhaseRecord` — Records outcome, duration, and tokens used for each phase
- `RecoveryEvent` — Documents stuck detection reasons and actions taken

**Example Flow:**

```rust
// Start a new task execution
let exec_id = orchestrator.start_task(
    "Implement user authentication",
    "Add JWT-based authentication to the API",
    AgentRole::Coder
);

// Assemble context for the current phase
let context = orchestrator.assemble_context(&exec_id, "planning")?;

// Select appropriate prompt
let prompt = orchestrator.select_prompt(&exec_id, "planning")?;

// Check if task is stuck
if orchestrator.is_stuck(&exec_id)? {
    orchestrator.attempt_recovery(&exec_id)?;
}
```

**Decision-Making Authority:**

The Orchestrator makes strategic decisions about:
- When to decompose tasks into subtasks
- Which context to include (via token budget management)
- Which recovery strategy to apply when stuck
- When to finalize execution (confidence threshold)
- Whether to enable progressive refinement

### TaskRunner: Phase Pipeline Orchestration

The **TaskRunner** implements the multi-phase task execution pipeline, driving tasks from discovery through completion.

**Core Responsibilities:**

1. **Phase Progression** — Executes the standard pipeline: Discovery → ContextGathering → SpecCreation → Planning → Coding → QA → Merging → Complete
2. **Context Assembly** — Uses `ContextSteerer` to build phase-specific context with token budgets
3. **Prompt Application** — Applies templates from `PromptRegistry` for the current role and phase
4. **Event Publishing** — Publishes `phase_start`, `phase_complete`, `task_complete` events to `EventBus`
5. **Session Communication** — Sends prompts to and receives output from `AgentSession`
6. **Error Handling** — Transitions tasks to `Error` state on phase failures

**Phase Pipeline:**

```
Discovery           → Understand the task, identify requirements
ContextGathering    → Collect relevant files, docs, patterns
SpecCreation        → Generate structured specification
Planning            → Create implementation plan with subtasks
Coding              → Execute implementation
QA                  → Validate correctness, run tests
Merging             → Integrate changes (if needed)
Complete            → Finalize and mark done
```

**Key Operations:**

```rust
// Create a TaskRunner with project context
let mut runner = TaskRunner::with_project("/path/to/project")
    .with_role(AgentRole::Coder)
    .with_timeout(Duration::from_secs(300))
    .with_token_budget(16_000);

// Run the full pipeline
runner.run(&mut task, &session, &bus).await?;
```

**Phase Execution Logic:**

For each phase, the TaskRunner:
1. Transitions the task to the new phase
2. Logs phase start and publishes `phase_start` event
3. Assembles context using `ContextSteerer` (if enabled)
4. Selects prompt template from `PromptRegistry` (if enabled)
5. Sends prompt to agent via `AgentSession`
6. Reads agent output with timeout
7. Parses output for completion markers or errors
8. Updates task logs and publishes `phase_complete` event
9. Proceeds to next phase or transitions to `Error` on failure

**Stuck Detection:**

The TaskRunner integrates with `StuckDetector` to identify:
- Infinite loops (repeated tool calls)
- Timeout violations (phase exceeds time limit)
- Token budget exhaustion (excessive context usage)

### Executor: Low-Level Agent Process Management

The **Executor** handles the actual spawning of agent processes and real-time interaction with them.

**Core Responsibilities:**

1. **Process Spawning** — Spawns CLI agent processes via PTY (pseudo-terminal) with configurable commands
2. **Output Capture** — Reads stdout/stderr from agent processes in real-time
3. **Output Parsing** — Extracts structured events, progress markers, and tool errors from agent output
4. **Tool Approval** — Enforces tool usage policies via `ToolApprovalSystem` before allowing execution
5. **Timeout Handling** — Aborts tasks that exceed configured execution time
6. **Error Recovery** — Implements tool fallback strategies when agents request unavailable tools

**Execution Flow:**

```
1. Executor receives task from TaskRunner
2. Spawns PTY process with agent CLI command
3. Sends initial prompt to agent's stdin
4. Enters output reading loop:
   a. Read line from agent stdout
   b. Parse for JSON events (e.g., {"event": "phase_complete"})
   c. Parse for tool_use_errors
   d. Check for completion markers
   e. Enforce timeout
5. On tool_use_error:
   a. Check ToolFallbackMap for alternatives
   b. Inject hint or skip based on policy
6. On completion or timeout:
   a. Capture final output
   b. Build ExecutionResult
   c. Return to TaskRunner
```

**Key Data Structures:**

- `ExecutionResult` — Contains task_id, success, output, events, tool_errors, duration, exit_code
- `AgentEvent` — Structured event parsed from agent output (event_type, message, data)
- `ToolUseError` — Records unavailable tool requests (tool_name, error_message, raw XML)
- `ToolFallbackMap` — Maps unavailable tools to available alternatives

**Tool Error Recovery:**

The Executor implements intelligent tool fallback:

```rust
// Example: Agent requests unavailable "Bash" tool
// Executor checks fallback map
let alternatives = fallback_map.suggest_alternatives("Bash");
// Returns: ["bash", "shell", "execute_command"]

// If "bash" is available, inject hint:
// "The tool 'Bash' is not available. Use 'bash' instead."
```

**Timeout and Abort:**

```rust
let result = executor.execute_task(
    task,
    agent_config,
    timeout_secs
).await?;

// Result includes:
// - success: bool
// - duration_ms: u64
// - tool_errors: Vec<ToolUseError>
// - exit_code: Option<i32>
```

### Supervisor: Agent Lifecycle and Health Monitoring

The **Supervisor** is the foundational component responsible for managing agent lifecycle, state transitions, and health monitoring.

**Core Responsibilities:**

1. **Agent Spawning** — Creates new agents with specified role and CLI type, transitions through `Idle → Spawning → Active`
2. **State Management** — Enforces state machine transitions via `AgentStateMachine`
3. **Lifecycle Hooks** — Invokes `AgentLifecycle` callbacks (`on_start`, `on_stop`, `on_heartbeat`, etc.)
4. **Health Monitoring** — Tracks `last_seen` timestamps and sends periodic heartbeats
5. **Failure Recovery** — Detects failed agents and restarts them via `Recover` transition
6. **Agent Registry** — Maintains list of all managed agents with their current state

**Agent Management Operations:**

```rust
let supervisor = AgentSupervisor::new();

// Spawn a new agent
let agent_id = supervisor.spawn_agent(
    "coder-1",
    AgentRole::Coder,
    CliType::ClaudeCode
).await?;

// List all agents
let agents = supervisor.list_agents().await;
for agent in agents {
    println!("{}: {:?} ({})", agent.name, agent.state, agent.role);
}

// Send heartbeat to all active agents
supervisor.send_heartbeat_all().await?;

// Restart failed agents
let restarted = supervisor.restart_failed().await?;

// Stop an agent
supervisor.stop_agent(agent_id).await?;
```

**State Transition Flow:**

When spawning an agent, the Supervisor:
1. Generates unique `Uuid` for agent
2. Creates `AgentStateMachine` in `Idle` state
3. Transitions `Idle → Spawning` via `Start` event
4. Instantiates `AgentLifecycle` implementation for the role
5. Calls `lifecycle.on_start()` to allocate resources
6. Transitions `Spawning → Active` via `Spawned` event
7. Registers agent in internal `Vec<ManagedAgent>`

**Health Monitoring:**

The Supervisor tracks agent health via:
- `last_seen: DateTime<Utc>` — Updated on heartbeat and state changes
- `on_heartbeat()` — Periodic callback to check agent responsiveness
- State inspection — Identifies `Failed` or unresponsive agents

**Failure Recovery:**

When an agent fails:
1. State transitions to `Failed`
2. `restart_failed()` identifies all agents in `Failed` state
3. For each failed agent:
   - Transition `Failed → Idle` via `Recover` event
   - Transition `Idle → Spawning` via `Start` event
   - Call `on_start()` to re-initialize
   - Transition `Spawning → Active` via `Spawned` event

### Data Flow Between Components

The components communicate through well-defined interfaces and data structures.

**Downward Flow (Orchestrator → TaskRunner → Executor):**

```
┌─────────────────┐
│  Orchestrator   │
└────────┬────────┘
         │ 1. TaskExecution context
         │ 2. AssembledContext (from ContextSteerer)
         │ 3. PromptTemplate (from PromptRegistry)
         ▼
┌─────────────────┐
│   TaskRunner    │
└────────┬────────┘
         │ 4. Task + AgentConfig
         │ 5. Prompt string
         │ 6. Timeout duration
         ▼
┌─────────────────┐
│    Executor     │
└─────────────────┘
```

**Upward Flow (Executor → TaskRunner → Orchestrator):**

```
┌─────────────────┐
│    Executor     │ ExecutionResult:
└────────┬────────┘   - success: bool
         │            - output: String
         │            - events: Vec<AgentEvent>
         │            - tool_errors: Vec<ToolUseError>
         │            - duration_ms: u64
         ▼
┌─────────────────┐
│   TaskRunner    │ Phase completion:
└────────┬────────┘   - phase outcome
         │            - logs
         │            - next phase decision
         ▼
┌─────────────────┐
│  Orchestrator   │ Updates:
└─────────────────┘   - PhaseRecord
                      - RecoveryEvent (if stuck)
                      - TaskExecution state
```

**Supervisor Integration (Horizontal Flow):**

```
┌─────────────────┐          ┌─────────────────┐
│   Supervisor    │◄────────►│    Executor     │
└────────┬────────┘          └─────────────────┘
         │                    Reports state changes
         │ Provides:          (spawned, active, failed)
         │ - Agent ID
         │ - State transitions
         │ - Lifecycle hooks
         ▼
┌──────────────────┐
│ AgentStateMachine│
└──────────────────┘
```

**Event Bus Communication (Broadcast):**

```
TaskRunner ──┐
             │
Executor ────┼──► EventBus ──► Subscribers
             │                  (UI, logging, metrics)
Supervisor ──┘
```

Events published:
- `phase_start:<phase>` — TaskRunner publishes when entering a phase
- `phase_complete:<phase>` — TaskRunner publishes when phase succeeds
- `task_complete` — TaskRunner publishes when task finishes
- `agent_spawned` — Supervisor publishes when agent becomes active
- `agent_failed` — Supervisor publishes when agent enters Failed state
- `tool_approval_pending` — Executor publishes when tool needs approval

**Context Steering Integration:**

```
Orchestrator ─────► ContextSteerer ─────► AssembledContext
     │                    ▲                      │
     │                    │                      │
     │                    │ load_project()       │
     │                    │                      ▼
     └────────────────────┴──────────────► TaskRunner
                                                 │
                                                 ▼
                                           Agent prompt
```

**Complete Request Flow Example:**

```
1. at-daemon creates a new bead (task request)
   └─► Orchestrator.start_task()

2. Orchestrator decomposes task (if complex)
   └─► RLM decomposition patterns

3. Orchestrator delegates to TaskRunner
   └─► TaskRunner.run(task, session, bus)

4. TaskRunner enters Discovery phase
   ├─► Assembles context via ContextSteerer
   ├─► Selects prompt via PromptRegistry
   └─► Sends prompt to AgentSession

5. AgentSession forwards to Executor
   └─► Executor.execute_task()

6. Executor spawns PTY process
   ├─► Supervisor tracks agent state (Spawning → Active)
   ├─► Reads agent output
   ├─► Parses events and tool errors
   └─► Returns ExecutionResult

7. TaskRunner processes result
   ├─► Publishes events to EventBus
   ├─► Transitions to next phase
   └─► Repeats 4-6 for each phase

8. Orchestrator monitors for stuck detection
   └─► Triggers recovery if needed

9. Task completes
   ├─► TaskRunner publishes task_complete
   ├─► Supervisor stops agent (Active → Stopping → Stopped)
   └─► Orchestrator finalizes TaskExecution
```

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

The approval system controls which tools agents can invoke, providing security and human oversight. It sits between the agent executor and the tools layer, intercepting tool invocations and enforcing policies based on tool type and agent role.

### Overview

The `ToolApprovalSystem` manages:
- **Default policies** for each tool (safe reads, dangerous writes, forbidden operations)
- **Role-based overrides** allowing trusted agents elevated privileges
- **Pending approval requests** awaiting human decision
- **Approval history** for auditing and debugging

**Key Design Principle:** Security by default with opt-in trust escalation.

---

### Approval Policies

Three policies govern tool invocations:

```rust
pub enum ApprovalPolicy {
    AutoApprove,      // Trusted tool, always allowed without human intervention
    RequireApproval,  // Potentially dangerous, requires explicit human approval
    Deny,             // Never allowed under any circumstances
}
```

#### 1. AutoApprove

**Purpose:** Allow safe, read-only operations without human intervention.

**Use Cases:**
- Reading files, searching code, listing directories
- Git inspection commands (diff, log, blame)
- Querying task status and system state

**Security Model:** These tools cannot modify state or leak sensitive data.

**Default Auto-Approved Tools:**
```rust
file_read, list_directory, search_files
git_diff, git_log, git_blame
task_status
```

**Behavior:** Tool executes immediately when invoked by any agent role.

---

#### 2. RequireApproval

**Purpose:** Gate dangerous operations that modify state or execute arbitrary code.

**Use Cases:**
- Writing files, committing code, pushing to git
- Executing shell commands
- Spawning or stopping agents
- Task assignment and orchestration

**Security Model:** Human must review arguments and approve before execution.

**Default Require-Approval Tools:**
```rust
file_write, shell_execute
git_add, git_commit, git_push
task_assign, agent_spawn, agent_stop
```

**Behavior:**
1. Agent invokes tool
2. Executor creates `PendingApproval` request
3. Agent pauses execution
4. Human reviews request (CLI, API, or WebSocket)
5. Human approves or denies
6. Agent resumes with result

**Unknown Tools:** Any tool not explicitly configured defaults to `RequireApproval` for safety.

---

#### 3. Deny

**Purpose:** Prevent catastrophic operations that should never be allowed.

**Use Cases:**
- Destructive file operations outside project scope
- Force-pushing to protected branches
- System-level modifications

**Security Model:** Hard block, no human override possible.

**Default Denied Tools:**
```rust
delete, file_delete, force_push
```

**Behavior:** Tool invocation immediately fails with `ApprovalError::Denied`.

---

### Approval Statuses

Every approval request progresses through a lifecycle tracked by `ApprovalStatus`:

```rust
pub enum ApprovalStatus {
    Pending,   // Awaiting human decision
    Approved,  // Human granted permission
    Denied,    // Human rejected request
}
```

#### Status Lifecycle

```
┌─────────────────────────────────────────┐
│  ToolApprovalSystem::request_approval() │
│  Creates PendingApproval                │
└──────────────────┬──────────────────────┘
                   │
                   ▼
            ┌─────────────┐
            │   PENDING   │ ◄────────┐
            └──────┬──────┘          │
                   │                 │
         Human     │                 │ Already Resolved?
         Decision  │                 │ (Error)
                   │                 │
       ┌───────────┴───────────┐     │
       │                       │     │
       ▼                       ▼     │
┌────────────┐          ┌───────────┐│
│  APPROVED  │          │  DENIED   ││
└────────────┘          └───────────┘│
       │                       │     │
       │                       │     │
       └───────────┬───────────┘     │
                   │                 │
                   ▼                 │
              ┌─────────┐            │
              │ Resolved│────────────┘
              │ (Final) │
              └─────────┘
```

#### 1. Pending Status

**Meaning:** Request awaiting human review.

**Properties:**
- `status`: `ApprovalStatus::Pending`
- `resolved_at`: `None`
- Agent execution is paused
- Request visible in approval queue

**Duration:** Indefinite (until human acts or timeout).

**Queries:**
```rust
// List all pending approvals
let pending = approval_system.list_pending();

// Check specific approval
if let Some(approval) = approval_system.get_approval(id) {
    assert_eq!(approval.status, ApprovalStatus::Pending);
}
```

---

#### 2. Approved Status

**Meaning:** Human granted permission to proceed.

**Properties:**
- `status`: `ApprovalStatus::Approved`
- `resolved_at`: `Some(timestamp)`
- Tool execution proceeds
- Request removed from pending queue

**API:**
```rust
approval_system.approve(approval_id)?;
assert!(approval_system.is_approved(approval_id));
```

**Error Handling:** Attempting to approve an already-resolved request returns `ApprovalError::AlreadyResolved`.

---

#### 3. Denied Status

**Meaning:** Human rejected the request.

**Properties:**
- `status`: `ApprovalStatus::Denied`
- `resolved_at`: `Some(timestamp)`
- Tool execution aborted
- Agent receives error response

**API:**
```rust
approval_system.deny(approval_id)?;
```

**Agent Impact:** Agent's tool invocation fails, must handle error and adjust strategy.

---

### End-to-End Approval Workflow

Complete flow from tool invocation to execution:

#### Step 1: Agent Invokes Tool

Agent's LLM output includes tool invocation:

```json
{
  "type": "tool_use",
  "tool": "file_write",
  "arguments": {
    "path": "src/main.rs",
    "content": "fn main() { println!(\"Hello\"); }"
  }
}
```

#### Step 2: Executor Detects Tool Use

`AgentExecutor` parses agent output and detects tool invocation:

```rust
impl AgentExecutor {
    async fn handle_output_event(&mut self, event: OutputEvent) -> Result<()> {
        match event {
            OutputEvent::ToolUse { tool_name, arguments } => {
                self.check_and_execute_tool(tool_name, arguments).await?;
            }
            // ... other events
        }
        Ok(())
    }
}
```

#### Step 3: Policy Resolution

Executor queries `ToolApprovalSystem` with tool name and agent role:

```rust
let policy = approval_system.check_approval(&tool_name, &agent_role);
```

**Resolution Order:**
1. **Role-specific override** (if configured)
2. **Default tool policy** (if known tool)
3. **RequireApproval** (fallback for unknown tools)

**Example:**
```rust
// Crew agent invoking file_write
approval_system.check_approval("file_write", &AgentRole::Crew)
// Returns: ApprovalPolicy::RequireApproval

// Mayor agent with override
approval_system.set_role_override(
    "file_write",
    AgentRole::Mayor,
    ApprovalPolicy::AutoApprove,
);
approval_system.check_approval("file_write", &AgentRole::Mayor)
// Returns: ApprovalPolicy::AutoApprove
```

#### Step 4: Policy Enforcement

Based on resolved policy:

##### 4a. AutoApprove Path

```rust
match policy {
    ApprovalPolicy::AutoApprove => {
        // Execute immediately
        let result = tools.execute(&tool_name, &arguments).await?;
        return Ok(result);
    }
    // ...
}
```

##### 4b. RequireApproval Path

```rust
ApprovalPolicy::RequireApproval => {
    // Create pending approval
    let pending = approval_system.request_approval(
        agent_id,
        tool_name,
        arguments.clone(),
    );

    // Publish event to notify human
    event_bus.publish(Event::ApprovalRequested {
        approval_id: pending.id,
        agent_id,
        tool_name: pending.tool_name.clone(),
        arguments: pending.arguments.clone(),
    }).await?;

    // Pause agent execution and wait
    self.await_approval(pending.id).await?;

    // Check result
    if approval_system.is_approved(pending.id) {
        let result = tools.execute(&tool_name, &arguments).await?;
        Ok(result)
    } else {
        Err(Error::ApprovalDenied(pending.id))
    }
}
```

##### 4c. Deny Path

```rust
ApprovalPolicy::Deny => {
    Err(ApprovalError::Denied(tool_name.to_string()))
}
```

#### Step 5: Human Review

Human receives approval request via:

**CLI:**
```bash
$ at-cli approvals list
[1] Agent crew-42 requests: file_write
    Arguments: {"path": "src/main.rs", ...}
    Time: 2026-02-23 14:30:00 UTC

$ at-cli approvals approve 1
✓ Approval granted
```

**API:**
```bash
GET /api/approvals
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "agent_id": "...",
    "tool_name": "file_write",
    "arguments": {...},
    "status": "pending"
  }
]

POST /api/approvals/550e8400-e29b-41d4-a716-446655440000/approve
{"status": "approved"}
```

**WebSocket Event:**
```json
{
  "type": "approval_requested",
  "approval_id": "550e8400-e29b-41d4-a716-446655440000",
  "agent_id": "crew-42",
  "tool_name": "file_write",
  "arguments": {"path": "src/main.rs", ...}
}
```

#### Step 6: Resolution

Human approves or denies:

```rust
// Approve
approval_system.approve(approval_id)?;

// Deny
approval_system.deny(approval_id)?;
```

**Validation:** Cannot resolve already-resolved requests (error: `ApprovalError::AlreadyResolved`).

#### Step 7: Agent Resumes

Agent unblocks and receives result:
- **Approved:** Tool executes, agent receives tool output
- **Denied:** Agent receives error, must adjust strategy

---

### Role-Based Policy Overrides

Different agent roles have different trust levels. The system supports per-role policy overrides.

#### Trust Hierarchy

```
Mayor (highest trust) — Can auto-approve orchestration tools
  │
Deacon                — Can auto-approve task management
  │
Crew                  — Standard permissions
  │
Witness (lowest)      — Read-only, strict approval requirements
```

#### Configuring Overrides

```rust
// Mayor can auto-approve git commits
approval_system.set_role_override(
    "git_commit",
    AgentRole::Mayor,
    ApprovalPolicy::AutoApprove,
);

// Deacon can auto-approve task assignment
approval_system.set_role_override(
    "task_assign",
    AgentRole::Deacon,
    ApprovalPolicy::AutoApprove,
);

// Witness cannot write files even with approval
approval_system.set_role_override(
    "file_write",
    AgentRole::Witness,
    ApprovalPolicy::Deny,
);
```

#### Policy Resolution Order

When checking approval, the system resolves in this order:

```rust
pub fn check_approval(&self, tool_name: &str, agent_role: &AgentRole) -> ApprovalPolicy {
    // 1. Check role-specific override first (highest priority)
    if let Some((_, _, policy)) = self
        .role_overrides
        .iter()
        .find(|(t, r, _)| t == tool_name && r == agent_role)
    {
        return *policy;
    }

    // 2. Fall back to default tool policy (medium priority)
    if let Some(policy) = self.policies.get(tool_name) {
        return *policy;
    }

    // 3. Unknown tools require approval by default (lowest priority)
    ApprovalPolicy::RequireApproval
}
```

**Example Resolution:**

```rust
// Setup
approval_system.set_policy("file_write", ApprovalPolicy::RequireApproval);
approval_system.set_role_override(
    "file_write",
    AgentRole::Mayor,
    ApprovalPolicy::AutoApprove,
);

// Resolution
approval_system.check_approval("file_write", &AgentRole::Crew)
// → RequireApproval (default policy)

approval_system.check_approval("file_write", &AgentRole::Mayor)
// → AutoApprove (role override takes precedence)

approval_system.check_approval("unknown_tool", &AgentRole::Crew)
// → RequireApproval (safe default for unknown tools)
```

---

### Integration with Executor

The `AgentExecutor` integrates approval checks into its execution pipeline.

#### Executor Architecture

```
┌────────────────────────────────────────────┐
│           AgentExecutor                    │
│                                            │
│  ┌──────────────────────────────────────┐ │
│  │  1. Parse agent output               │ │
│  └──────────┬───────────────────────────┘ │
│             │                              │
│  ┌──────────▼───────────────────────────┐ │
│  │  2. Detect tool_use events           │ │
│  └──────────┬───────────────────────────┘ │
│             │                              │
│  ┌──────────▼───────────────────────────┐ │
│  │  3. Query ToolApprovalSystem         │ │──────► ToolApprovalSystem
│  └──────────┬───────────────────────────┘ │
│             │                              │
│  ┌──────────▼───────────────────────────┐ │
│  │  4. Enforce policy                   │ │
│  │     - AutoApprove → Execute          │ │
│  │     - RequireApproval → Wait         │ │
│  │     - Deny → Error                   │ │
│  └──────────┬───────────────────────────┘ │
│             │                              │
│  ┌──────────▼───────────────────────────┐ │
│  │  5. Execute tool (if approved)       │ │──────► Tools Layer
│  └──────────┬───────────────────────────┘ │
│             │                              │
│  ┌──────────▼───────────────────────────┐ │
│  │  6. Return result to agent           │ │
│  └──────────────────────────────────────┘ │
└────────────────────────────────────────────┘
```

#### Executor Implementation

```rust
impl AgentExecutor {
    pub async fn execute_task(
        &self,
        task: &Task,
        agent_config: &AgentConfig,
        approval_system: &mut ToolApprovalSystem,
    ) -> Result<ExecutionResult> {
        // Spawn agent process
        let mut pty = self.pty_pool.acquire().await?;
        pty.spawn(&agent_config.cli_args).await?;

        // Send task prompt
        pty.write(task.prompt.as_bytes()).await?;

        // Collect and process output
        loop {
            let output = pty.read().await?;
            let events = self.parse_output(&output)?;

            for event in events {
                match event {
                    OutputEvent::ToolUse { tool_name, arguments } => {
                        // Check approval
                        let policy = approval_system.check_approval(
                            &tool_name,
                            &agent_config.role,
                        );

                        match policy {
                            ApprovalPolicy::AutoApprove => {
                                // Execute immediately
                                let result = self.tools.execute(
                                    &tool_name,
                                    &arguments,
                                ).await?;
                                self.send_tool_result(&mut pty, result).await?;
                            }

                            ApprovalPolicy::RequireApproval => {
                                // Create approval request
                                let pending = approval_system.request_approval(
                                    agent_config.agent_id,
                                    tool_name.clone(),
                                    arguments.clone(),
                                );

                                // Notify human
                                self.event_bus.publish(Event::ApprovalRequested {
                                    approval_id: pending.id,
                                    agent_id: agent_config.agent_id,
                                    tool_name,
                                    arguments,
                                }).await?;

                                // Wait for resolution
                                self.await_approval(pending.id, approval_system).await?;

                                // Check result and execute if approved
                                if approval_system.is_approved(pending.id) {
                                    let result = self.tools.execute(
                                        &tool_name,
                                        &arguments,
                                    ).await?;
                                    self.send_tool_result(&mut pty, result).await?;
                                } else {
                                    // Send denial to agent
                                    self.send_tool_error(
                                        &mut pty,
                                        "Tool use denied by human",
                                    ).await?;
                                }
                            }

                            ApprovalPolicy::Deny => {
                                // Immediate rejection
                                self.send_tool_error(
                                    &mut pty,
                                    format!("Tool '{}' is denied by policy", tool_name),
                                ).await?;
                            }
                        }
                    }

                    OutputEvent::TaskComplete => break,
                    _ => { /* handle other events */ }
                }
            }
        }

        Ok(ExecutionResult { /* ... */ })
    }

    async fn await_approval(
        &self,
        approval_id: Uuid,
        approval_system: &ToolApprovalSystem,
    ) -> Result<()> {
        // Poll until resolved or timeout
        let timeout = Duration::from_secs(300); // 5 minutes
        let start = Instant::now();

        loop {
            if let Some(approval) = approval_system.get_approval(approval_id) {
                if approval.status != ApprovalStatus::Pending {
                    return Ok(());
                }
            }

            if start.elapsed() > timeout {
                return Err(Error::ApprovalTimeout(approval_id));
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
```

#### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    #[error("approval request not found: {0}")]
    NotFound(Uuid),

    #[error("approval request already resolved: {0}")]
    AlreadyResolved(Uuid),

    #[error("tool denied by policy: {0}")]
    Denied(String),
}
```

**Agent Impact:**
- `NotFound`: Internal error, should not happen in normal operation
- `AlreadyResolved`: Attempt to approve/deny twice, returns error
- `Denied`: Tool invocation fails, agent must handle gracefully

---

### API Reference

#### ToolApprovalSystem Methods

```rust
impl ToolApprovalSystem {
    /// Create system with default policies
    pub fn new() -> Self;

    /// Create permissive system (testing only)
    pub fn permissive() -> Self;

    /// Set default policy for a tool
    pub fn set_policy(&mut self, tool_name: impl Into<String>, policy: ApprovalPolicy);

    /// Set role-specific override
    pub fn set_role_override(
        &mut self,
        tool_name: impl Into<String>,
        role: AgentRole,
        policy: ApprovalPolicy,
    );

    /// Check policy for tool invocation by role
    pub fn check_approval(&self, tool_name: &str, agent_role: &AgentRole) -> ApprovalPolicy;

    /// Create pending approval request
    pub fn request_approval(
        &mut self,
        agent_id: Uuid,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> &PendingApproval;

    /// Approve pending request
    pub fn approve(&mut self, approval_id: Uuid) -> Result<()>;

    /// Deny pending request
    pub fn deny(&mut self, approval_id: Uuid) -> Result<()>;

    /// List pending approvals
    pub fn list_pending(&self) -> Vec<&PendingApproval>;

    /// List all approvals (including resolved)
    pub fn list_all(&self) -> &[PendingApproval];

    /// Get specific approval by ID
    pub fn get_approval(&self, id: Uuid) -> Option<&PendingApproval>;

    /// Check if approval is approved
    pub fn is_approved(&self, approval_id: Uuid) -> bool;
}
```

#### PendingApproval Structure

```rust
pub struct PendingApproval {
    pub id: Uuid,                         // Unique approval ID
    pub agent_id: Uuid,                   // Agent requesting approval
    pub tool_name: String,                // Tool being invoked
    pub arguments: serde_json::Value,     // Tool arguments for review
    pub requested_at: DateTime<Utc>,      // When request was created
    pub status: ApprovalStatus,           // Current status (Pending/Approved/Denied)
    pub resolved_at: Option<DateTime<Utc>>, // When resolved (None if pending)
}
```

---

### Usage Examples

#### Querying Approvals

```bash
# CLI
$ at-cli approvals list
$ at-cli approvals approve <id>
$ at-cli approvals deny <id>

# API
GET /api/approvals
GET /api/approvals/<id>
POST /api/approvals/<id>/approve
POST /api/approvals/<id>/deny

# WebSocket (subscribe to events)
{
  "type": "subscribe",
  "events": ["approval_requested", "approval_resolved"]
}
```

---

### Security Considerations

**Defense in Depth:**
1. **Default deny for unknown tools** — Safe fallback
2. **Explicit policy configuration** — No implicit trust
3. **Role-based overrides** — Granular privilege escalation
4. **Immutable resolved approvals** — Cannot change decision after resolution
5. **Audit trail** — All approvals logged with timestamps

**Best Practices:**
- Review approval arguments carefully before approving
- Use role overrides sparingly for trusted automation
- Monitor approval patterns for anomalies
- Set timeouts for pending approvals to prevent agent deadlock

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
