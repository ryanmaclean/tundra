# WebSocket API Reference

**[← Back to Project Handbook](./PROJECT_HANDBOOK.md)** | **[Security Documentation](./SECURITY_WEBSOCKET_ORIGIN.md)**

> Real-time event streaming and terminal I/O over WebSocket connections.
> Comprehensive guide for integrating with Auto-Tundra's WebSocket endpoints.

---

## 📋 Table of Contents

1. [Overview](#1-overview)
2. [Endpoints Comparison](#2-endpoints-comparison)
3. [Connection Setup](#3-connection-setup)
4. [Origin Header Requirements](#4-origin-header-requirements)
5. [Event Streaming API](#5-event-streaming-api)
6. [Terminal WebSocket API](#6-terminal-websocket-api)
7. [Client Examples](#7-client-examples)
8. [Troubleshooting](#8-troubleshooting)

---

# 1. Overview

Auto-Tundra provides **three WebSocket endpoints** for real-time communication:

| Endpoint | Purpose | Use Case |
|----------|---------|----------|
| **`/ws`** | Legacy event streaming | Backward compatibility, simple event monitoring |
| **`/api/events/ws`** | Modern event streaming | Production use with heartbeat and notifications |
| **`/ws/terminal/{id}`** | Terminal I/O | Interactive shell access, command execution |

## Key Characteristics

- **Bidirectional Communication**: Full-duplex channels for client ↔ server messaging
- **Real-Time Updates**: Events pushed to clients as they occur (no polling)
- **Origin Validation**: CSRF protection via Origin header checking
- **Automatic Reconnection**: Grace periods for seamless reconnection after network failures
- **WebSocket Protocol**: RFC 6455 compliant (ws:// for HTTP, wss:// for HTTPS)

## When to Use Each Endpoint

```
┌──────────────────────────────────────────────────────────────┐
│                    Use Case Decision Tree                     │
└──────────────────────────────────────────────────────────────┘

Need real-time system events?
│
├─ YES ──→ Production application?
│          │
│          ├─ YES ──→ Use /api/events/ws
│          │          ✓ Heartbeat keeps connection alive
│          │          ✓ Notification integration
│          │          ✓ Better error handling
│          │
│          └─ NO ───→ Use /ws
│                     ✓ Simple setup
│                     ✓ Minimal protocol
│
└─ NO ───→ Need terminal I/O?
           │
           └─ YES ──→ Use /ws/terminal/{id}
                      ✓ Interactive shell
                      ✓ Bidirectional I/O
                      ✓ Automatic buffering
```

---

# 2. Endpoints Comparison

## `/ws` — Legacy Event Streaming

**Description:** Original WebSocket endpoint for system events. Simple, fire-and-forget event stream.

**Protocol:**
- **Server → Client:** JSON-serialized events
- **Client → Server:** Not supported (one-way only)

**Features:**
- ✅ Basic event streaming
- ✅ Automatic JSON serialization
- ❌ No heartbeat/keepalive
- ❌ No client-to-server messaging
- ❌ No notification integration

**Connection Lifecycle:**
```
Client                    Server
  │                         │
  │─── Upgrade WS ─────────►│
  │                         │
  │                         │ Subscribe to event bus
  │◄──── Event ─────────────│
  │◄──── Event ─────────────│
  │◄──── Event ─────────────│
  │                         │
  │─── Close ───────────────►│
  │                         │ Unsubscribe
```

**Use Cases:**
- Quick prototyping
- Debugging event flow
- Legacy integrations
- Simple monitoring dashboards

---

## `/api/events/ws` — Modern Event Streaming

**Description:** Production-grade event streaming with heartbeat, bidirectional messaging, and notification integration.

**Protocol:**
- **Server → Client:** JSON-serialized events + heartbeat pings
- **Client → Server:** Pong responses, close frames

**Features:**
- ✅ Full event streaming
- ✅ 30-second heartbeat (prevents connection timeouts)
- ✅ Notification store integration
- ✅ Bidirectional messaging support
- ✅ Connection health monitoring

**Connection Lifecycle:**
```
Client                    Server
  │                         │
  │─── Upgrade WS ─────────►│
  │                         │
  │                         │ Subscribe to event bus
  │                         │ Start 30s heartbeat timer
  │◄──── Event ─────────────│
  │◄──── Event ─────────────│
  │                         │
  │◄──── Ping ──────────────│ (every 30s)
  │──── Pong ──────────────►│
  │                         │
  │◄──── Event ─────────────│
  │                         │
  │─── Close ───────────────►│
  │                         │ Unsubscribe, stop heartbeat
```

**Heartbeat Message Format:**
```json
{
  "type": "ping",
  "timestamp": "2026-03-01T12:34:56.789Z"
}
```

**Use Cases:**
- Production web applications
- Long-lived connections
- Mobile/desktop clients
- Real-time dashboards with high uptime requirements

---

## `/ws/terminal/{id}` — Terminal I/O WebSocket

**Description:** Interactive terminal I/O over WebSocket with resilient reconnection and automatic buffering.

**Protocol:**
- **Server → Client:** Terminal output (UTF-8 text) + Ping frames
- **Client → Server:** Terminal input (JSON commands or raw text) + Pong responses

**Features:**
- ✅ Full bidirectional terminal I/O
- ✅ 30-second reconnection grace period
- ✅ 64KB disconnect buffer (output replay on reconnect)
- ✅ JSON command protocol (input, resize)
- ✅ Plain text fallback (raw stdin)
- ✅ 5-minute idle timeout
- ✅ 30-second heartbeat

**Connection Lifecycle:**
```
Client                    Terminal                  PTY Process
  │                         │                             │
  │─── Upgrade WS ─────────►│                             │
  │                         │ Status → Active             │
  │◄──── Buffered output ───│                             │
  │                         │──── Read stdout ───────────►│
  │◄──── Terminal output ───│◄──── Data ──────────────────│
  │                         │                             │
  │─── {"type":"input"} ────►│                             │
  │                         │──── Write stdin ───────────►│
  │                         │                             │
  │──X  (Disconnect)        │ Status → Disconnected       │
  │                         │ Start 30s grace timer       │
  │                         │ Buffer output (64KB)        │
  │                         │◄──── Data ──────────────────│
  │                         │ (buffering...)              │
  │                         │                             │
  │─── Reconnect ──────────►│ Status → Active             │
  │◄──── Replay buffer ─────│                             │
  │◄──── Terminal output ───│◄──── Data ──────────────────│
```

**Reconnection Grace Period:**

If the WebSocket disconnects (network failure, page reload, tab switch), the terminal session survives:

1. **Disconnection (t=0s)**: Status → `Disconnected`, start buffering
2. **Grace Period (0-30s)**: PTY continues running, output buffered (64KB ring buffer)
3. **Reconnect Before 30s**: Buffer replayed, session resumes transparently
4. **Grace Expires (t=30s)**: PTY killed (SIGTERM), status → `Dead`, buffer dropped

**Benefits:**
- Page reloads don't kill long-running commands (builds, tests, downloads)
- Brief network interruptions are transparent to users
- No data loss during temporary disconnections

**Use Cases:**
- Interactive terminal emulators
- Remote command execution
- Build/test output streaming
- SSH-like terminal access

---

# 3. Connection Setup

## Prerequisites

1. **Running at-bridge server** on `http://localhost:{port}` (default 3000)
2. **Valid Origin header** (see [Section 4](#4-origin-header-requirements))
3. **WebSocket client library** (browser WebSocket API, ws/tungstenite for Rust, etc.)

## Connection URL Format

```
ws://localhost:{port}{endpoint}
```

**Examples:**
```
ws://localhost:3000/ws
ws://localhost:3000/api/events/ws
ws://localhost:3000/ws/terminal/a1b2c3d4-5678-90ab-cdef-1234567890ab
```

## Basic Connection Flow

### 1. Establish Connection

**JavaScript (Browser):**
```javascript
const ws = new WebSocket('ws://localhost:3000/api/events/ws');

ws.onopen = () => {
  console.log('Connected to event stream');
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

ws.onclose = (event) => {
  console.log('Connection closed:', event.code, event.reason);
};
```

**Rust (tungstenite):**
```rust
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};

let ws_url = "ws://localhost:3000/api/events/ws";
let (mut ws_stream, _response) = connect_async(ws_url)
    .await
    .expect("Failed to connect");

println!("Connected to event stream");
```

**Python (websockets):**
```python
import asyncio
import websockets

async def connect():
    uri = "ws://localhost:3000/api/events/ws"
    async with websockets.connect(uri) as ws:
        print("Connected to event stream")
        async for message in ws:
            print(f"Received: {message}")

asyncio.run(connect())
```

### 2. Handle Messages

**JavaScript (Browser):**
```javascript
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  // Handle heartbeat
  if (data.type === 'ping') {
    console.log('Heartbeat received at', data.timestamp);
    return;
  }

  // Handle system events
  console.log('Event:', data);
};
```

**Rust:**
```rust
while let Some(msg) = ws_stream.next().await {
    match msg? {
        Message::Text(text) => {
            let event: serde_json::Value = serde_json::from_str(&text)?;

            // Handle heartbeat
            if event.get("type").and_then(|v| v.as_str()) == Some("ping") {
                println!("Heartbeat: {}", event["timestamp"]);
                continue;
            }

            // Handle system events
            println!("Event: {:?}", event);
        }
        Message::Ping(_) => {
            // Pong sent automatically by library
        }
        Message::Close(_) => {
            println!("Server closed connection");
            break;
        }
        _ => {}
    }
}
```

### 3. Handle Disconnection

**Automatic Reconnection Pattern (JavaScript):**
```javascript
class ResilientWebSocket {
  constructor(url, options = {}) {
    this.url = url;
    this.reconnectDelay = options.reconnectDelay || 1000;
    this.maxReconnectDelay = options.maxReconnectDelay || 30000;
    this.reconnectAttempts = 0;
    this.connect();
  }

  connect() {
    this.ws = new WebSocket(this.url);

    this.ws.onopen = () => {
      console.log('Connected');
      this.reconnectAttempts = 0;
      this.reconnectDelay = 1000;
      if (this.onopen) this.onopen();
    };

    this.ws.onmessage = (event) => {
      if (this.onmessage) this.onmessage(event);
    };

    this.ws.onerror = (error) => {
      console.error('WebSocket error:', error);
    };

    this.ws.onclose = () => {
      console.log('Connection closed, reconnecting...');
      this.reconnect();
    };
  }

  reconnect() {
    this.reconnectAttempts++;
    const delay = Math.min(
      this.reconnectDelay * Math.pow(2, this.reconnectAttempts),
      this.maxReconnectDelay
    );

    console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);
    setTimeout(() => this.connect(), delay);
  }

  send(data) {
    if (this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(data);
    } else {
      console.warn('WebSocket not open, message queued');
    }
  }

  close() {
    this.ws.close();
  }
}

// Usage
const ws = new ResilientWebSocket('ws://localhost:3000/api/events/ws');
ws.onmessage = (event) => {
  console.log('Event:', JSON.parse(event.data));
};
```

---

# 4. Origin Header Requirements

## Security Model

All WebSocket endpoints validate the **Origin header** to prevent cross-site WebSocket hijacking attacks. This is a **critical security feature** that protects against remote code execution vulnerabilities.

**Why Origin Validation Matters:**

Unlike HTTP requests protected by CORS, WebSocket connections **bypass browser CORS restrictions**. Without server-side Origin validation, any malicious website could:

1. Open a WebSocket connection to your local at-bridge daemon
2. Send commands to your terminal sessions
3. Execute arbitrary code with your user privileges

See [SECURITY_WEBSOCKET_ORIGIN.md](./SECURITY_WEBSOCKET_ORIGIN.md) for detailed vulnerability analysis.

## Default Allowed Origins

By default, **only localhost origins** are permitted:

```rust
const DEFAULT_ALLOWED_ORIGINS: &[&str] = &[
    "http://localhost",
    "https://localhost",
    "http://127.0.0.1",
    "https://127.0.0.1",
    "http://[::1]",
    "https://[::1]",
];
```

**Matching Rules:**

- **Exact match**: `http://localhost` ✅
- **Prefix match with port**: `http://localhost:3000` ✅
- **Subdomain**: `http://sub.localhost` ❌
- **Different protocol**: `ws://localhost` ❌
- **With path**: `http://localhost/path` ❌
- **External domain**: `http://evil.com` ❌

## Client-Side Implementation

### Browser (Automatic)

Modern browsers **automatically** send the Origin header for WebSocket connections:

```javascript
// Browser automatically sets:
// Origin: http://localhost:3000
const ws = new WebSocket('ws://localhost:3000/ws');
```

**No manual configuration needed** for same-origin connections.

### Cross-Origin Connections (Blocked by Default)

If you're connecting from a web page hosted on a different domain:

```javascript
// Page: http://example.com
// WebSocket: ws://localhost:3000/ws
// Origin: http://example.com ❌ REJECTED

const ws = new WebSocket('ws://localhost:3000/ws');
// Result: 403 Forbidden
```

**Solution:** Configure allowed origins on the at-bridge server (beyond scope of this document).

### Native Clients (Rust, Python, etc.)

Native WebSocket clients must **manually set the Origin header**:

**Rust (tungstenite):**
```rust
use tokio_tungstenite::{connect_async, tungstenite::http::Request};

let ws_url = "ws://localhost:3000/ws";
let request = Request::builder()
    .uri(ws_url)
    .header("Origin", "http://localhost")
    .body(())
    .unwrap();

let (ws_stream, _) = connect_async(request).await?;
```

**Python (websockets):**
```python
import websockets

async def connect():
    extra_headers = {
        "Origin": "http://localhost"
    }

    async with websockets.connect(
        "ws://localhost:3000/ws",
        extra_headers=extra_headers
    ) as ws:
        # Use websocket...
        pass
```

**curl (testing):**
```bash
curl -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Origin: http://localhost" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: x3JJHMbDL1EzLkh9GBhXDw==" \
  http://localhost:3000/ws
```

## Error Responses

### Missing Origin Header

**Request:**
```http
GET /ws HTTP/1.1
Host: localhost:3000
Upgrade: websocket
Connection: Upgrade
(no Origin header)
```

**Response:**
```http
HTTP/1.1 403 Forbidden
Content-Length: 18

origin not allowed
```

### Invalid Origin

**Request:**
```http
GET /ws HTTP/1.1
Host: localhost:3000
Origin: http://evil.com
Upgrade: websocket
Connection: Upgrade
```

**Response:**
```http
HTTP/1.1 403 Forbidden
Content-Length: 18

origin not allowed
```

### Valid Origin

**Request:**
```http
GET /ws HTTP/1.1
Host: localhost:3000
Origin: http://localhost:3000
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Version: 13
Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==
```

**Response:**
```http
HTTP/1.1 101 Switching Protocols
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=
```

---

# 5. Event Streaming API

## Event Message Format

All events streamed via `/ws` and `/api/events/ws` follow a consistent JSON structure:

```typescript
interface Event {
  id: string;           // UUID
  kind: string;         // Event type (e.g., "agent.status", "task.progress")
  source: string;       // Event source (e.g., "at-daemon", "at-agents")
  payload: any;         // Event-specific data
  timestamp: string;    // ISO 8601 timestamp
}
```

**Example:**
```json
{
  "id": "a1b2c3d4-5678-90ab-cdef-1234567890ab",
  "kind": "agent.status",
  "source": "at-daemon",
  "payload": {
    "agent_id": "f0e1d2c3-4567-89ab-cdef-0123456789ab",
    "status": "running",
    "message": "Processing task"
  },
  "timestamp": "2026-03-01T12:34:56.789Z"
}
```

## Common Event Types

### Agent Events

**`agent.status`** — Agent state change
```json
{
  "kind": "agent.status",
  "payload": {
    "agent_id": "uuid",
    "status": "running" | "paused" | "stopped",
    "message": "Status description"
  }
}
```

**`agent.progress`** — Agent task progress
```json
{
  "kind": "agent.progress",
  "payload": {
    "agent_id": "uuid",
    "progress": 0.75,
    "message": "Building project (75%)"
  }
}
```

### Task Events

**`task.created`** — New task created
```json
{
  "kind": "task.created",
  "payload": {
    "task_id": "uuid",
    "title": "Implement feature X",
    "status": "pending"
  }
}
```

**`task.status`** — Task status change
```json
{
  "kind": "task.status",
  "payload": {
    "task_id": "uuid",
    "old_status": "in_progress",
    "new_status": "completed"
  }
}
```

### Bead Events

**`bead.slung`** — Bead slung (task queued)
```json
{
  "kind": "bead.slung",
  "payload": {
    "bead_id": "uuid",
    "title": "Fix bug #123"
  }
}
```

**`bead.hooked`** — Bead hooked (agent assigned)
```json
{
  "kind": "bead.hooked",
  "payload": {
    "bead_id": "uuid",
    "agent_id": "uuid"
  }
}
```

**`bead.done`** — Bead completed
```json
{
  "kind": "bead.done",
  "payload": {
    "bead_id": "uuid",
    "result": "success" | "failure"
  }
}
```

## Filtering Events (Client-Side)

Since events are broadcast to all connected clients, implement client-side filtering:

```javascript
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  // Ignore heartbeats
  if (data.type === 'ping') return;

  // Filter by event kind
  if (data.kind === 'agent.status') {
    handleAgentStatus(data.payload);
  } else if (data.kind === 'task.progress') {
    handleTaskProgress(data.payload);
  }
};
```

## BridgeMessage Protocol

The BridgeMessage enum defines the structured message protocol for bidirectional WebSocket communication between frontend and backend. Messages are serialized as **tagged unions** using Serde's adjacently-tagged format.

### Tagged Union Format

All BridgeMessage variants are serialized with a **discriminator field** (`type`) and optional **content field** (`payload`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum BridgeMessage {
    // Message variants...
}
```

**Serialization Behavior:**

- **`tag = "type"`**: The enum variant name becomes the `type` field (in `snake_case`)
- **`content = "payload"`**: The variant's data becomes the `payload` field
- **Unit variants** (no data): Serialized as `{"type": "variant_name"}` (no `payload` field)
- **Struct variants** (named fields): Serialized as `{"type": "variant_name", "payload": {...}}`
- **Tuple variants** (unnamed data): Serialized as `{"type": "variant_name", "payload": ...}`

### Message Direction

**Frontend → Backend** (Client Commands):
- `GetStatus`, `ListBeads`, `ListAgents`, `SlingBead`, `HookBead`, `DoneBead`, `NudgeAgent`, `GetKpi`

**Backend → Frontend** (Server Responses/Events):
- `StatusUpdate`, `BeadList`, `AgentList`, `KpiUpdate`, `AgentOutput`, `Error`, `Event`, `TaskUpdate`, `MergeResult`, `QueueUpdate`, `BeadCreated`, `BeadUpdated`

### Message Examples

#### Unit Variants (No Payload)

**`GetStatus`** — Request server status

```json
{
  "type": "get_status"
}
```

**`ListAgents`** — Request list of all agents

```json
{
  "type": "list_agents"
}
```

**`GetKpi`** — Request KPI metrics

```json
{
  "type": "get_kpi"
}
```

#### Struct Variants (Named Fields)

**`ListBeads`** — Request beads with optional status filter

```json
{
  "type": "list_beads",
  "payload": {
    "status": "hooked"
  }
}
```

```json
{
  "type": "list_beads",
  "payload": {
    "status": null
  }
}
```

**`SlingBead`** — Assign bead to agent

```json
{
  "type": "sling_bead",
  "payload": {
    "bead_id": "a1b2c3d4-5678-90ab-cdef-1234567890ab",
    "agent_id": "f0e1d2c3-4567-89ab-cdef-0123456789ab"
  }
}
```

**`HookBead`** — Create and assign new bead

```json
{
  "type": "hook_bead",
  "payload": {
    "title": "Implement user authentication",
    "agent_name": "auth-agent"
  }
}
```

**`DoneBead`** — Mark bead as completed

```json
{
  "type": "done_bead",
  "payload": {
    "bead_id": "a1b2c3d4-5678-90ab-cdef-1234567890ab",
    "failed": false
  }
}
```

**`NudgeAgent`** — Send message to agent

```json
{
  "type": "nudge_agent",
  "payload": {
    "agent_name": "build-agent",
    "message": "Restart build process"
  }
}
```

**`AgentOutput`** — Agent execution output

```json
{
  "type": "agent_output",
  "payload": {
    "agent_id": "f0e1d2c3-4567-89ab-cdef-0123456789ab",
    "output": "Build completed successfully\n"
  }
}
```

**`Error`** — Error response

```json
{
  "type": "error",
  "payload": {
    "code": "BEAD_NOT_FOUND",
    "message": "Bead with ID a1b2c3d4-5678-90ab-cdef-1234567890ab does not exist"
  }
}
```

**`MergeResult`** — Git merge completion/conflict notification

```json
{
  "type": "merge_result",
  "payload": {
    "worktree_id": "task-123",
    "branch": "feature/auth",
    "status": "conflict",
    "conflict_files": [
      "src/auth.rs",
      "Cargo.toml"
    ]
  }
}
```

**`QueueUpdate`** — Task queue reordering

```json
{
  "type": "queue_update",
  "payload": {
    "task_ids": [
      "a1b2c3d4-5678-90ab-cdef-1234567890ab",
      "b2c3d4e5-6789-01bc-def0-123456789abc",
      "c3d4e5f6-789a-12cd-ef01-23456789abcd"
    ]
  }
}
```

#### Tuple Variants (Single Wrapped Object)

**`StatusUpdate`** — Server status information

```json
{
  "type": "status_update",
  "payload": {
    "version": "0.1.0",
    "uptime_seconds": 3600,
    "agents_active": 3,
    "beads_active": 5
  }
}
```

**`BeadList`** — List of beads

```json
{
  "type": "bead_list",
  "payload": [
    {
      "id": "a1b2c3d4-5678-90ab-cdef-1234567890ab",
      "title": "Fix authentication bug",
      "status": "hooked",
      "agent_id": "f0e1d2c3-4567-89ab-cdef-0123456789ab",
      "created_at": "2026-03-01T10:30:00Z"
    },
    {
      "id": "b2c3d4e5-6789-01bc-def0-123456789abc",
      "title": "Add unit tests",
      "status": "backlog",
      "agent_id": null,
      "created_at": "2026-03-01T11:00:00Z"
    }
  ]
}
```

**`AgentList`** — List of agents

```json
{
  "type": "agent_list",
  "payload": [
    {
      "id": "f0e1d2c3-4567-89ab-cdef-0123456789ab",
      "name": "auth-agent",
      "status": "active",
      "current_bead_id": "a1b2c3d4-5678-90ab-cdef-1234567890ab"
    },
    {
      "id": "e1f2a3b4-5678-90ab-cdef-0123456789ab",
      "name": "build-agent",
      "status": "idle",
      "current_bead_id": null
    }
  ]
}
```

**`KpiUpdate`** — KPI metrics

```json
{
  "type": "kpi_update",
  "payload": {
    "total_beads": 100,
    "backlog": 20,
    "hooked": 5,
    "slung": 15,
    "review": 10,
    "done": 45,
    "failed": 5,
    "active_agents": 3
  }
}
```

**`Event`** — System event notification

```json
{
  "type": "event",
  "payload": {
    "event_type": "bead.status_change",
    "agent_id": "f0e1d2c3-4567-89ab-cdef-0123456789ab",
    "bead_id": "a1b2c3d4-5678-90ab-cdef-1234567890ab",
    "message": "Bead moved to review status",
    "timestamp": "2026-03-01T12:34:56.789Z"
  }
}
```

**`TaskUpdate`** — Real-time task progress update

```json
{
  "type": "task_update",
  "payload": {
    "id": "a1b2c3d4-5678-90ab-cdef-1234567890ab",
    "title": "Implement user authentication",
    "phase": "implementation",
    "progress": 0.65,
    "subtasks": [
      {
        "id": "subtask-1",
        "title": "Create user model",
        "status": "completed"
      },
      {
        "id": "subtask-2",
        "title": "Add authentication middleware",
        "status": "in_progress"
      }
    ]
  }
}
```

**`BeadCreated`** — New bead created event

```json
{
  "type": "bead_created",
  "payload": {
    "id": "c3d4e5f6-789a-12cd-ef01-23456789abcd",
    "title": "Optimize database queries",
    "status": "backlog",
    "agent_id": null,
    "created_at": "2026-03-01T14:00:00Z"
  }
}
```

**`BeadUpdated`** — Bead updated event

```json
{
  "type": "bead_updated",
  "payload": {
    "id": "a1b2c3d4-5678-90ab-cdef-1234567890ab",
    "title": "Fix authentication bug",
    "status": "review",
    "agent_id": "f0e1d2c3-4567-89ab-cdef-0123456789ab",
    "created_at": "2026-03-01T10:30:00Z",
    "updated_at": "2026-03-01T15:45:00Z"
  }
}
```

### Client Implementation Example

**JavaScript/TypeScript:**

```typescript
// Type definitions for type safety
type BridgeMessage =
  | { type: 'get_status' }
  | { type: 'list_beads'; payload: { status?: string } }
  | { type: 'status_update'; payload: StatusPayload }
  | { type: 'error'; payload: { code: string; message: string } }
  // ... other variants

// Sending messages
function sendCommand(ws: WebSocket, message: BridgeMessage) {
  ws.send(JSON.stringify(message));
}

// Examples
sendCommand(ws, { type: 'get_status' });
sendCommand(ws, { type: 'list_beads', payload: { status: 'hooked' } });
sendCommand(ws, {
  type: 'sling_bead',
  payload: {
    bead_id: 'a1b2c3d4-5678-90ab-cdef-1234567890ab',
    agent_id: 'f0e1d2c3-4567-89ab-cdef-0123456789ab'
  }
});

// Receiving messages
ws.onmessage = (event) => {
  const message: BridgeMessage = JSON.parse(event.data);

  switch (message.type) {
    case 'status_update':
      console.log('Server status:', message.payload);
      break;
    case 'bead_list':
      console.log('Beads:', message.payload);
      break;
    case 'error':
      console.error(`Error ${message.payload.code}:`, message.payload.message);
      break;
    case 'task_update':
      console.log('Task progress:', message.payload.progress);
      break;
    // ... handle other message types
  }
};
```

**Rust:**

```rust
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

// Use the BridgeMessage enum from at_bridge::protocol
use at_bridge::protocol::BridgeMessage;

// Sending messages
async fn send_command(ws: &mut WebSocketStream, msg: BridgeMessage) -> Result<()> {
    let json = serde_json::to_string(&msg)?;
    ws.send(Message::Text(json)).await?;
    Ok(())
}

// Examples
send_command(&mut ws, BridgeMessage::GetStatus).await?;
send_command(&mut ws, BridgeMessage::ListBeads { status: Some("hooked".to_string()) }).await?;
send_command(&mut ws, BridgeMessage::SlingBead {
    bead_id: uuid!("a1b2c3d4-5678-90ab-cdef-1234567890ab"),
    agent_id: uuid!("f0e1d2c3-4567-89ab-cdef-0123456789ab"),
}).await?;

// Receiving messages
while let Some(msg) = ws.next().await {
    match msg? {
        Message::Text(text) => {
            let message: BridgeMessage = serde_json::from_str(&text)?;

            match message {
                BridgeMessage::StatusUpdate(status) => {
                    println!("Server uptime: {}s", status.uptime_seconds);
                }
                BridgeMessage::BeadList(beads) => {
                    println!("Received {} beads", beads.len());
                }
                BridgeMessage::Error { code, message } => {
                    eprintln!("Error {}: {}", code, message);
                }
                BridgeMessage::TaskUpdate(task) => {
                    println!("Task progress: {:.0}%", task.progress * 100.0);
                }
                // ... handle other message types
                _ => {}
            }
        }
        _ => {}
    }
}
```

### Serialization Details

**Serde Attributes:**

- **`#[serde(tag = "type", content = "payload")]`**: Adjacently-tagged enum representation
  - Creates two separate JSON fields: `type` (discriminator) and `payload` (content)
  - Allows for clean, predictable JSON structure

- **`#[serde(rename_all = "snake_case")]`**: Converts Rust variant names from PascalCase to snake_case
  - `GetStatus` → `"get_status"`
  - `StatusUpdate` → `"status_update"`
  - `BeadList` → `"bead_list"`

- **`#[allow(clippy::large_enum_variant)]`**: Suppresses warnings about enum size variance
  - Some variants like `TaskUpdate(Box<Task>)` are large, but that's acceptable for this use case

**Important Notes:**

1. **No `payload` field for unit variants**: Messages like `GetStatus` serialize to `{"type": "get_status"}` without a `payload` field. Clients should handle the absence of this field gracefully.

2. **Boxed payloads**: Large payloads like `TaskUpdate` use `Box<T>` to reduce enum size, but this is transparent in JSON serialization.

3. **Null handling**: Optional fields in payloads (like `status: Option<String>`) serialize as `null` in JSON when `None`.

4. **UUID serialization**: UUIDs serialize as hyphenated strings: `"a1b2c3d4-5678-90ab-cdef-1234567890ab"`.

5. **DateTime serialization**: Timestamps use ISO 8601 format: `"2026-03-01T12:34:56.789Z"`.

---

# 6. Terminal WebSocket API

## Creating a Terminal Session

Before connecting to `/ws/terminal/{id}`, create a terminal via REST API:

**Request:**
```http
POST /api/terminals HTTP/1.1
Content-Type: application/json

{
  "agent_id": "00000000-0000-0000-0000-000000000000",
  "title": "My Terminal",
  "cols": 80,
  "rows": 24
}
```

**Response:**
```json
{
  "id": "a1b2c3d4-5678-90ab-cdef-1234567890ab",
  "title": "My Terminal",
  "status": "idle",
  "cols": 80,
  "rows": 24,
  "font_size": 14,
  "cursor_style": "block",
  "cursor_blink": true
}
```

**Save the `id` field** — this is your WebSocket connection identifier.

## Connecting to Terminal

```javascript
const terminalId = 'a1b2c3d4-5678-90ab-cdef-1234567890ab';
const ws = new WebSocket(`ws://localhost:3000/ws/terminal/${terminalId}`);

ws.onopen = () => {
  console.log('Terminal connected');
  // Terminal status → Active
  // Buffered output replayed (if reconnecting)
};

ws.onmessage = (event) => {
  // Terminal output (UTF-8 text)
  console.log('Terminal output:', event.data);
};
```

## Sending Input to Terminal

The terminal WebSocket supports **two input formats**:

### 1. JSON Command Format (Structured)

**Input Command:**
```json
{
  "type": "input",
  "data": "ls -la\n"
}
```

```javascript
// Send command to terminal
ws.send(JSON.stringify({
  type: 'input',
  data: 'echo "Hello World"\n'
}));
```

**Resize Command:**
```json
{
  "type": "resize",
  "cols": 120,
  "rows": 30
}
```

```javascript
// Resize terminal window
ws.send(JSON.stringify({
  type: 'resize',
  cols: 120,
  rows: 30
}));
```

### 2. Plain Text Format (Raw Input)

Any message that **doesn't parse as JSON** is treated as raw terminal input:

```javascript
// Send raw keystrokes
ws.send('ls -la\n');
ws.send('cd /tmp\n');
ws.send('pwd\n');
```

**This allows simple clients** to send input without JSON wrapping.

## Receiving Output from Terminal

**All terminal output is sent as plain UTF-8 text** (not JSON):

```javascript
ws.onmessage = (event) => {
  // event.data contains raw terminal output
  // Example: "total 48\ndrwxr-xr-x  6 user  staff   192 Mar  1 12:34 .\n..."

  // Display in terminal emulator
  terminal.write(event.data);
};
```

**Note:** Output includes **ANSI escape codes** for colors, cursor movement, etc.

## Handling Disconnection & Reconnection

### Graceful Disconnection

The terminal survives brief disconnections (network failures, page reloads):

```javascript
let ws;
const terminalId = 'a1b2c3d4-5678-90ab-cdef-1234567890ab';

function connect() {
  ws = new WebSocket(`ws://localhost:3000/ws/terminal/${terminalId}`);

  ws.onopen = () => {
    console.log('Terminal connected');
    // If reconnecting within 30s, buffered output is replayed
  };

  ws.onclose = () => {
    console.log('Terminal disconnected, reconnecting...');
    // Reconnect within 30 seconds to resume session
    setTimeout(connect, 1000);
  };

  ws.onmessage = (event) => {
    terminal.write(event.data);
  };
}

connect();
```

### Reconnection Timeline

```
t=0s: Disconnect
      ↓
      Terminal status → Disconnected
      PTY continues running
      Output buffered (64KB ring buffer)

t=1s: Reconnect attempt 1
      ↓
      Connection established
      Buffered output replayed
      Session resumes ✅

--- OR ---

t=0s: Disconnect
      ↓
      Terminal status → Disconnected
      PTY continues running
      Output buffered (64KB)

t=31s: Reconnect attempt (too late)
      ↓
      PTY killed at t=30s
      Status → Dead
      Connection rejected: 410 Gone ❌
```

### Detecting Terminal Death

If you reconnect after the 30-second grace period, you'll receive an error:

```javascript
ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

ws.onclose = (event) => {
  if (event.code === 1008) { // Policy violation
    console.error('Terminal session expired (grace period exceeded)');
    // Create a new terminal instead of reconnecting
  }
};
```

## Terminal State Machine

```
┌──────┐
│ Idle │  (Terminal created, no WebSocket)
└──┬───┘
   │ WebSocket connect
   ▼
┌────────┐
│ Active │  (WebSocket connected, I/O flowing)
└───┬────┘
    │
    │ WebSocket disconnect
    ▼
┌──────────────┐
│ Disconnected │  (Buffering output, 30s grace period)
└──────┬───────┘
       │
       ├─ Reconnect within 30s ──→ Active (resume session)
       │
       └─ Grace expires (30s) ──→ Dead (PTY killed, session lost)
```

## Timeouts

| Timeout | Duration | Behavior |
|---------|----------|----------|
| **Idle Timeout** | 5 minutes | WebSocket closes if no data flows |
| **Heartbeat Interval** | 30 seconds | Ping frames sent to detect half-open connections |
| **Reconnect Grace** | 30 seconds | PTY survives disconnection, buffering output |

**Idle Timeout Example:**

If no input is sent and no output is received for 5 minutes, the connection automatically closes:

```javascript
// Keep connection alive by sending periodic input
setInterval(() => {
  if (ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: 'input', data: '' })); // Empty input
  }
}, 60000); // Every 60 seconds
```

---

# 7. Client Examples

## Example 1: Event Monitor (JavaScript)

```javascript
class EventMonitor {
  constructor(url = 'ws://localhost:3000/api/events/ws') {
    this.url = url;
    this.handlers = new Map();
    this.connect();
  }

  connect() {
    this.ws = new WebSocket(this.url);

    this.ws.onopen = () => {
      console.log('Event monitor connected');
    };

    this.ws.onmessage = (event) => {
      const data = JSON.parse(event.data);

      // Ignore heartbeats
      if (data.type === 'ping') {
        console.log(`Heartbeat: ${data.timestamp}`);
        return;
      }

      // Dispatch to registered handlers
      const handler = this.handlers.get(data.kind);
      if (handler) {
        handler(data.payload, data);
      } else {
        console.log('Unhandled event:', data.kind, data);
      }
    };

    this.ws.onerror = (error) => {
      console.error('WebSocket error:', error);
    };

    this.ws.onclose = () => {
      console.log('Connection closed, reconnecting in 5s...');
      setTimeout(() => this.connect(), 5000);
    };
  }

  on(eventKind, handler) {
    this.handlers.set(eventKind, handler);
  }

  off(eventKind) {
    this.handlers.delete(eventKind);
  }
}

// Usage
const monitor = new EventMonitor();

monitor.on('agent.status', (payload) => {
  console.log(`Agent ${payload.agent_id}: ${payload.status}`);
});

monitor.on('task.progress', (payload) => {
  console.log(`Task progress: ${payload.progress * 100}%`);
});

monitor.on('bead.done', (payload) => {
  console.log(`Bead ${payload.bead_id} completed: ${payload.result}`);
});
```

## Example 2: Terminal Client (Rust)

```rust
use tokio_tungstenite::{connect_async, tungstenite::{Message, http::Request}};
use futures_util::{StreamExt, SinkExt};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create terminal via REST API
    let client = reqwest::Client::new();
    let response = client
        .post("http://localhost:3000/api/terminals")
        .json(&json!({
            "agent_id": "00000000-0000-0000-0000-000000000000",
            "title": "Rust Terminal Client",
            "cols": 120,
            "rows": 30
        }))
        .send()
        .await?;

    let terminal: serde_json::Value = response.json().await?;
    let terminal_id = terminal["id"].as_str().unwrap();
    println!("Created terminal: {}", terminal_id);

    // 2. Connect to terminal WebSocket
    let ws_url = format!("ws://localhost:3000/ws/terminal/{}", terminal_id);
    let request = Request::builder()
        .uri(&ws_url)
        .header("Origin", "http://localhost")
        .body(())
        .unwrap();

    let (mut ws_stream, _) = connect_async(request).await?;
    println!("Connected to terminal WebSocket");

    // 3. Send commands
    let commands = vec![
        "echo 'Hello from Rust!'\n",
        "pwd\n",
        "ls -la\n",
    ];

    for cmd in commands {
        let msg = json!({
            "type": "input",
            "data": cmd
        });
        ws_stream.send(Message::Text(msg.to_string())).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // 4. Read output
    while let Some(msg) = ws_stream.next().await {
        match msg? {
            Message::Text(text) => {
                print!("{}", text);
            }
            Message::Ping(_) => {
                // Pong sent automatically
            }
            Message::Close(_) => {
                println!("\nTerminal closed");
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
```

## Example 3: Event Stream (Python)

```python
import asyncio
import websockets
import json
from datetime import datetime

class EventStream:
    def __init__(self, url='ws://localhost:3000/api/events/ws'):
        self.url = url
        self.handlers = {}

    def on(self, event_kind, handler):
        self.handlers[event_kind] = handler

    async def connect(self):
        extra_headers = {
            'Origin': 'http://localhost'
        }

        async with websockets.connect(self.url, extra_headers=extra_headers) as ws:
            print(f"Connected to {self.url}")

            async for message in ws:
                data = json.loads(message)

                # Handle heartbeat
                if data.get('type') == 'ping':
                    timestamp = data.get('timestamp')
                    print(f"Heartbeat: {timestamp}")
                    continue

                # Dispatch to handlers
                event_kind = data.get('kind')
                handler = self.handlers.get(event_kind)

                if handler:
                    await handler(data.get('payload'), data)
                else:
                    print(f"Unhandled event: {event_kind}")

# Usage
async def handle_agent_status(payload, event):
    print(f"Agent {payload['agent_id']}: {payload['status']}")

async def handle_task_progress(payload, event):
    print(f"Task progress: {payload.get('progress', 0) * 100}%")

async def main():
    stream = EventStream()
    stream.on('agent.status', handle_agent_status)
    stream.on('task.progress', handle_task_progress)

    await stream.connect()

asyncio.run(main())
```

---

# 8. Troubleshooting

## Connection Refused

**Symptom:** `WebSocket connection failed: Connection refused`

**Causes:**
- at-bridge server not running
- Wrong port number
- Firewall blocking connection

**Solutions:**
```bash
# Check if server is running
curl http://localhost:3000/api/status

# Start at-daemon (which starts at-bridge)
at-daemon start

# Check firewall settings
# macOS
sudo pfctl -sr | grep 3000

# Linux
sudo iptables -L | grep 3000
```

## 403 Forbidden (Origin Validation Failed)

**Symptom:** `HTTP 403 Forbidden: origin not allowed`

**Causes:**
- Missing Origin header
- Origin not in allowlist
- Invalid Origin format

**Solutions:**

**Browser (same-origin):**
```javascript
// ✅ CORRECT: Connect from localhost page
// URL: http://localhost:3000/index.html
const ws = new WebSocket('ws://localhost:3000/ws');
```

**Native client:**
```rust
// ✅ CORRECT: Set Origin header
let request = Request::builder()
    .uri("ws://localhost:3000/ws")
    .header("Origin", "http://localhost")
    .body(())
    .unwrap();
```

**Verify Origin with curl:**
```bash
curl -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Origin: http://localhost" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: $(openssl rand -base64 16)" \
  http://localhost:3000/ws
```

## Connection Timeout

**Symptom:** Connection hangs or times out after 5 minutes

**Causes:**
- No data flow (idle timeout)
- Heartbeat not working

**Solutions:**

**Ensure heartbeat messages are handled:**
```javascript
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  // ✅ CORRECT: Don't close connection on heartbeat
  if (data.type === 'ping') {
    return; // Keep connection alive
  }

  // Handle other events...
};
```

**Send periodic keepalive (terminal WebSocket):**
```javascript
setInterval(() => {
  if (ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: 'input', data: '' }));
  }
}, 60000); // Every 60 seconds
```

## Terminal Session Lost (410 Gone)

**Symptom:** Reconnection fails with `410 Gone` or `1008 Policy Violation`

**Cause:** Reconnected after 30-second grace period expired

**Solution:**

```javascript
ws.onclose = (event) => {
  if (event.code === 1008) {
    console.error('Terminal session expired');

    // ❌ WRONG: Try to reconnect to dead terminal
    // setTimeout(reconnect, 1000);

    // ✅ CORRECT: Create new terminal
    createNewTerminal();
  }
};

async function createNewTerminal() {
  const response = await fetch('http://localhost:3000/api/terminals', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      agent_id: '00000000-0000-0000-0000-000000000000',
      title: 'Recovered Terminal',
      cols: 80,
      rows: 24
    })
  });

  const terminal = await response.json();
  connectToTerminal(terminal.id);
}
```

## Missing Terminal Output

**Symptom:** Terminal output not appearing, but commands execute

**Causes:**
- Not handling UTF-8 text messages
- Filtering out non-JSON messages

**Solutions:**

**✅ CORRECT:**
```javascript
ws.onmessage = (event) => {
  // Terminal output is plain text, NOT JSON
  terminal.write(event.data);
};
```

**❌ WRONG:**
```javascript
ws.onmessage = (event) => {
  // This breaks terminal output!
  const data = JSON.parse(event.data); // SyntaxError: Unexpected token
};
```

## Message Parsing Errors

**Symptom:** `SyntaxError: Unexpected token` when parsing messages

**Cause:** Mixing event stream format with terminal output format

**Solution:**

**Event endpoints (`/ws`, `/api/events/ws`):**
```javascript
// ✅ Events are always JSON
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  handleEvent(data);
};
```

**Terminal endpoint (`/ws/terminal/{id}`):**
```javascript
// ✅ Terminal output is plain text
ws.onmessage = (event) => {
  terminal.write(event.data); // No JSON parsing
};
```

---

## Quick Reference

### Endpoint Summary

| Endpoint | Protocol | Use Case |
|----------|----------|----------|
| `/ws` | JSON events | Legacy event monitoring |
| `/api/events/ws` | JSON events + heartbeat | Production event streaming |
| `/ws/terminal/{id}` | Text I/O + JSON commands | Interactive terminal |

### Connection Checklist

- [ ] at-bridge server running (`curl http://localhost:3000/api/status`)
- [ ] Correct WebSocket URL (`ws://localhost:3000/...`)
- [ ] Origin header set (native clients only)
- [ ] Origin in allowlist (localhost by default)
- [ ] Message format matches endpoint type (JSON vs text)
- [ ] Heartbeat messages handled (don't close connection)
- [ ] Reconnection logic implemented (exponential backoff)

### Security Checklist

- [ ] Origin validation enabled (default)
- [ ] Connecting from allowed origin (localhost)
- [ ] Using secure WebSocket (wss://) in production
- [ ] Not exposing terminal endpoints publicly
- [ ] Authentication enabled if exposing API externally

---

**For more information:**
- [Project Handbook](./PROJECT_HANDBOOK.md) — System architecture and overview
- [Security Documentation](./SECURITY_WEBSOCKET_ORIGIN.md) — Origin validation security details
- [at-bridge README](../crates/at-bridge/README.md) — Implementation details

**Questions or issues?** Open an issue on GitHub or check the troubleshooting section above.
