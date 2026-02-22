# Agent Harness Architecture

This document explains how the Agent Harness works internally.

## 📚 Table of Contents

1. [High-Level Overview](#high-level-overview)
2. [Core Components](#core-components)
3. [Data Flow](#data-flow)
4. [Key Concepts](#key-concepts)
5. [Module Breakdown](#module-breakdown)
6. [Binary Variants](#binary-variants)
7. [Extension Points](#extension-points)
8. [Auto-Tundra vs Agent-Harness](#auto-tundra-vs-agent-harness)

---

## High-Level Overview

The Agent Harness is a multi-agent orchestration system that routes user requests to specialized AI agents and manages API quota limits.

```
┌─────────────────────────────────────────────────────────────────┐
│                         USER INPUT                              │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                      ORCHESTRATOR                               │
│  - Routes to appropriate agent                                  │
│  - Manages conversation flow                                    │
│  - Coordinates multiple agents                                  │
└────────────────────────────┬────────────────────────────────────┘
                             │
                ┌────────────┴────────────┐
                ▼                         ▼
        ┌──────────────┐         ┌──────────────┐
        │   AGENT 1    │         │   AGENT 2    │
        │ (Researcher) │         │   (Coder)    │
        └──────┬───────┘         └──────┬───────┘
               │                        │
               └────────────┬───────────┘
                            ▼
                ┌─────────────────────┐
                │     PROVIDER        │
                │  - API client       │
                │  - Quota checking   │
                │  - Error handling   │
                └──────────┬──────────┘
                           │
                           ▼
                ┌─────────────────────┐
                │   QUOTA TRACKER     │
                │  - Monitor usage    │
                │  - Enforce limits   │
                │  - Track resets     │
                └──────────┬──────────┘
                           │
                           ▼
                ┌─────────────────────┐
                │    LLM API          │
                │  (OpenRouter)       │
                └──────────┬──────────┘
                           │
                           ▼
                ┌─────────────────────┐
                │     RESPONSE        │
                └─────────────────────┘
```

---

## Core Components

### 1. **Orchestrator**
**File:** `src/orchestrator.rs`

Coordinates multiple agents and routes requests.

**Responsibilities:**
- Select which agent handles a request
- Manage conversation state
- Coordinate multi-agent workflows
- Handle routing strategies

**Routing Strategies:**
- **Keyword-Based**: Routes based on keywords in user input
- **First Available**: Uses the first agent in the list
- **Round-Robin**: Rotates between agents
- **Custom**: User-defined routing logic

### 2. **Agent**
**File:** `src/agent.rs`

Individual AI agent with specific instructions and capabilities.

**Responsibilities:**
- Process user messages
- Execute tool calls
- Maintain conversation context
- Format responses

**Key Properties:**
- `name`: Unique identifier
- `instructions`: System prompt defining behavior
- `tools`: Available tools for the agent
- `memory`: Conversation history storage

### 3. **Provider**
**File:** `src/provider.rs`

Handles communication with LLM APIs.

**Responsibilities:**
- Make API requests to OpenRouter
- Parse responses
- Handle streaming
- Retry on failures
- Format tool calls

**Supported Providers:**
- OpenRouter (primary)
- Extensible for other providers

### 4. **Quota Tracker**
**File:** `src/quota.rs`

Monitors and enforces API usage limits.

**Responsibilities:**
- Track requests per model
- Track tokens per model
- Enforce daily limits
- Calculate reset times
- Provide usage statistics

**Free Tier Limits:**
```
Per Model:
- 100 requests/day
- 10,000 tokens/day
- Resets at midnight UTC
```

### 5. **Memory**
**File:** `src/memory.rs`, `src/memory_backends.rs`

Stores conversation history.

**Backends:**
- **In-Memory**: Fast, temporary storage
- **SQLite**: Persistent, production-ready
- **File System**: Human-readable JSON files

**Operations:**
- Append messages
- Retrieve history
- Clear conversations

### 6. **Tools**
**File:** `src/tool.rs`

Functions that agents can call to perform actions.

**Built-in Tools:**
- `calculate`: Perform mathematical calculations
- Extensible for custom tools

**Tool Execution Flow:**
```
Agent decides to use tool
    ↓
Tool call sent to LLM
    ↓
LLM returns tool arguments
    ↓
Tool executes locally
    ↓
Result sent back to LLM
    ↓
LLM formats final response
```

---

## Data Flow

### Single Request Flow

```
1. USER INPUT
   "What is 2+2?"
        ↓
2. ORCHESTRATOR
   - Analyzes input
   - Selects "assistant" agent
        ↓
3. AGENT
   - Loads conversation history from Memory
   - Adds user message to context
   - Determines tools needed (calculator)
        ↓
4. PROVIDER
   - Checks Quota Tracker (requests: 12/100 ✓)
   - Calls OpenRouter API
        ↓
5. LLM API
   - Processes request
   - Returns tool call: calculate(2+2)
        ↓
6. TOOL EXECUTION
   - Calculator tool executes
   - Returns result: 4
        ↓
7. PROVIDER (second call)
   - Sends tool result back to LLM
   - LLM formats final answer
        ↓
8. RESPONSE
   - Agent saves to Memory
   - Returns to user: "The answer is 4"
```

### Multi-Agent Flow

```
1. USER: "Research quantum computing then write a function"
        ↓
2. ORCHESTRATOR
   - Detects "research" keyword → Researcher agent
   - Detects "write" keyword → Coder agent
   - Plans two-step execution
        ↓
3. RESEARCHER AGENT
   - Gathers information about quantum computing
   - Returns research findings
        ↓
4. CODER AGENT
   - Uses research as context
   - Writes code based on findings
   - Returns function implementation
        ↓
5. ORCHESTRATOR
   - Combines both responses
   - Returns unified answer
```

---

## Key Concepts

### Agents

An **agent** is an AI assistant with:
- Specific instructions (system prompt)
- Available tools
- Conversation memory
- Unique name

**Example:**
```rust
let researcher = Agent::builder()
    .name("researcher")
    .instructions("You research topics and provide factual information.")
    .tools(vec![search_tool])
    .build();
```

### Tools

**Tools** are functions agents can call:
- Defined with name, description, parameters
- Executed locally (not by LLM)
- Results sent back to LLM for interpretation

**Tool Definition:**
```rust
Tool {
    name: "calculate",
    description: "Perform mathematical calculations",
    parameters: {
        "expression": "string"
    }
}
```

### Memory

**Memory** stores conversation history:
- Per-conversation isolation (conversation_id)
- Persistent across sessions (with SQLite/FileSystem)
- Enables context-aware responses

**Memory Flow:**
```
User: "My name is Alice"
  → Stored in Memory
User: "What's my name?"
  → Agent retrieves history
  → Sees previous message
  → Responds: "Alice"
```

### Quota Management

**Why Quota Tracking?**
- Free tier has limits (100 req/day, 10k tokens/day)
- Prevents unexpected API blocks
- Enables graceful degradation

**How It Works:**
```
Before each request:
  Check quota → If available → Make request
               → If exceeded → Suggest alternatives
```

---

## Module Breakdown

### Core Library (`src/lib.rs`)
Main public API and exports.

### Agents (`src/agent.rs`)
Agent creation, message processing, tool execution.

### Orchestrator (`src/orchestrator.rs`)
Multi-agent coordination and routing.

### Provider (`src/provider.rs`)
LLM API client implementation.

### Quota (`src/quota.rs`)
Usage tracking and limit enforcement.

### Memory (`src/memory.rs`, `src/memory_backends.rs`)
Conversation storage and retrieval.

### Tools (`src/tool.rs`)
Tool definition and execution framework.

### Types (`src/types.rs`)
Shared data structures (Message, Response, etc.).

### Stream (`src/stream.rs`)
Streaming response handling.

### Production Features
- `src/circuit_breaker.rs` - Resilience pattern
- `src/security.rs` - API key validation, firewall
- `src/rate_limiter.rs` - Rate limiting
- `src/validation_harness.rs` - Testing framework
- `src/memory_management.rs` - Smart pruning
- `src/health.rs` - Health monitoring

---

## Binary Variants

The project includes three executable binaries:

### 1. **interactive** (`src/bin/interactive.rs`)
**Recommended for beginners**

- Interactive CLI chat interface
- Built-in commands (help, status, quit)
- Real-time conversation
- Quota monitoring

**Use When:**
- Learning the system
- Testing queries
- Daily usage

**Run:**
```bash
cargo run --bin interactive
```

### 2. **demo** (`src/bin/demo.rs`)
**Pre-programmed demonstrations**

- Shows multi-agent orchestration
- Demonstrates tool usage
- Displays quota tracking

**Use When:**
- Understanding capabilities
- Seeing examples
- Presentations

**Run:**
```bash
cargo run --bin demo
```

### 3. **agent-harness** (`src/main.rs`)
**Original implementation**

- Basic request/response
- Minimal interface
- Good for scripting

**Use When:**
- Programmatic usage
- Integration testing
- Batch processing

**Run:**
```bash
cargo run --bin agent-harness
```

---

## Extension Points

### Adding a Custom Tool

```rust
use agent_harness::*;

// 1. Define tool function
async fn weather_tool(args: serde_json::Value) -> Result<String, Error> {
    let location = args["location"].as_str().unwrap();
    // ... fetch weather ...
    Ok(format!("Weather in {}: Sunny", location))
}

// 2. Create tool definition
let tool = Tool {
    name: "get_weather".to_string(),
    description: "Get current weather for a location".to_string(),
    parameters: serde_json::json!({
        "type": "object",
        "properties": {
            "location": { "type": "string" }
        }
    }),
};

// 3. Add to agent
let agent = Agent::builder()
    .name("weather_assistant")
    .tools(vec![tool])
    .build();
```

### Adding a Custom Agent

```rust
let custom_agent = Agent::builder()
    .name("translator")
    .instructions("You translate text between languages.")
    .tools(vec![translation_tool])
    .build();

let orchestrator = Orchestrator::builder()
    .add_agent(custom_agent)
    .strategy(RoutingStrategy::KeywordBased(keywords))
    .build();
```

### Adding a Custom Memory Backend

```rust
use agent_harness::memory::Memory;

struct RedisMemory { /* ... */ }

#[async_trait::async_trait]
impl Memory for RedisMemory {
    async fn append(&self, id: &str, messages: &[Message]) -> Result<(), Error> {
        // Store in Redis
    }

    async fn history(&self, id: &str) -> Vec<Message> {
        // Retrieve from Redis
    }
}
```

### Adding a Custom Provider

```rust
use agent_harness::provider::Provider;

struct AnthropicProvider { /* ... */ }

#[async_trait::async_trait]
impl Provider for AnthropicProvider {
    async fn chat_completion(
        &self,
        messages: Vec<Message>,
        tools: Vec<Tool>
    ) -> Result<Response, Error> {
        // Call Anthropic API
    }
}
```

---

## Auto-Tundra vs Agent-Harness

This repository contains two projects:

### **agent-harness** (Main Project)
- ✅ Beginner-friendly
- ✅ Single binary outputs
- ✅ Simple architecture
- ✅ Good for learning
- ✅ Production-ready features

**Location:** `/Users/studio/rust-harness/`

**Start Here If:**
- You're new to the codebase
- You want to understand multi-agent systems
- You need a working LLM orchestrator

### **auto-tundra** (Advanced Project)
- 🔧 Complex workspace
- 🔧 Multiple crates (11 modules)
- 🔧 TUI and GUI interfaces
- 🔧 Advanced features
- 🔧 Active development

**Location:** `/Users/studio/rust-harness/auto-tundra/`

**Crates:**
- `at-core` - Core functionality
- `at-harness` - Agent harness integration
- `at-session` - Session management
- `at-agents` - Agent implementations
- `at-daemon` - Background service
- `at-telemetry` - Metrics and logging
- `at-cli` - Command-line interface
- `at-bridge` - Integration bridge
- `at-tui` - Terminal UI
- `at-integrations` - External integrations
- `at-intelligence` - AI capabilities
- `app/tauri` - Desktop application

**Use When:**
- You've mastered agent-harness
- You need advanced features
- You want GUI/TUI interfaces

**Recommendation:** Start with `agent-harness`, then explore `auto-tundra` once comfortable.

---

## Performance Characteristics

### Latency
```
Interactive Mode Request:
- Quota check: <1ms
- API call: 500-2000ms (depends on model)
- Tool execution: 1-10ms
- Total: ~500-2000ms
```

### Memory Usage
```
In-Memory Backend: ~1KB per message
SQLite Backend: ~2KB per message (disk)
File System Backend: ~3KB per message (disk)
```

### Scalability
- **Concurrent Requests**: Limited by tokio runtime
- **Conversations**: No hard limit (memory-dependent)
- **Messages per Conversation**: Recommended <1000 (use pruning)

---

## Design Decisions

### Why Async/Await?
- Non-blocking I/O for API calls
- Concurrent request handling
- Efficient resource usage

### Why Multiple Agents?
- Specialization improves quality
- Clear separation of concerns
- Easier to test and maintain

### Why Quota Tracking?
- Free tier has limits
- Prevents service interruption
- Enables cost awareness

### Why Multiple Memory Backends?
- Flexibility for different use cases
- Production (SQLite) vs Testing (In-Memory)
- Human-readable option (FileSystem)

---

## 📚 Further Reading

- **Getting Started**: [GETTING_STARTED.md](GETTING_STARTED.md)
- **Examples**: [EXAMPLES.md](EXAMPLES.md)
- **Troubleshooting**: [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
- **Production**: [PRODUCTION.md](../PRODUCTION.md)

---

**Questions?** Check [TROUBLESHOOTING.md](TROUBLESHOOTING.md) or review the source code with these file references!
