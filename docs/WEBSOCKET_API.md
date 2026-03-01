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

**Heartbeat & Pong Handling:**

The server sends heartbeat pings every 30 seconds on `/api/events/ws` only. Clients should handle these appropriately:

**✅ Recommended:**
```javascript
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  // Early return for heartbeat messages
  if (data.type === 'ping') {
    // Optional: Send pong response (not required)
    // ws.send(JSON.stringify({ type: 'pong' }));
    return;
  }

  // Handle actual events
  handleEvent(data);
};
```

**⚠️ Important:**
- **Pong responses are optional** — The server does not require clients to send explicit pong messages. Simply ignoring the ping is sufficient.
- **Do not close the connection** on ping messages — This would break the keepalive mechanism.
- **WebSocket protocol pongs are automatic** — Browser WebSocket API and most libraries automatically respond to protocol-level ping frames. The JSON `{"type":"ping"}` is an application-level heartbeat.

**Reliability Guarantees:**

Auto-Tundra's event streaming provides **at-most-once delivery semantics**:

| Guarantee | `/ws` | `/api/events/ws` | Details |
|-----------|-------|------------------|---------|
| **Message Delivery** | At-most-once | At-most-once | No retries, no persistence |
| **Message Ordering** | ✅ FIFO | ✅ FIFO | Events delivered in order (per connection) |
| **Message Persistence** | ❌ None | ❌ None | Disconnected clients miss events |
| **Backpressure Handling** | Drop connection | Drop connection | Slow subscribers disconnected |
| **Duplicate Prevention** | ✅ No duplicates | ✅ No duplicates | Each event sent once per connection |

**At-Most-Once Delivery:**

Events are streamed directly from the in-memory event bus to connected clients. There is **no queueing, buffering, or persistence layer**:

```
Event Bus ──(live stream)──► WebSocket Client
                              │
                              │ (network failure or slow consumer)
                              │
                              ✗ Event lost (not retried)
```

**Implications:**
1. **No Historical Events**: New connections start receiving events from the moment of connection. Past events are not available.
2. **Disconnection = Data Loss**: If a client disconnects (network failure, crash, etc.), events emitted during disconnection are permanently lost.
3. **No Replay Mechanism**: Unlike Kafka or message queues, there is no way to "rewind" or replay missed events.
4. **Ephemeral Notifications**: While `/api/events/ws` integrates with the notification store, this only stores notifications (not raw events), and is separate from the WebSocket stream.

**When This Matters:**
- ✅ **Acceptable**: Real-time dashboards, live monitoring, status indicators
- ❌ **Not Suitable**: Audit logs, financial transactions, critical event processing requiring guaranteed delivery

**Backpressure & Slow Subscriber Handling:**

If a client cannot keep up with the event rate (slow network, blocked UI thread, resource constraints), the server will **drop the connection**:

**Behavior:**
```rust
// From websocket.rs implementation
if ws_tx.send(Message::Text(json.into())).await.is_err() {
    break;  // Send failed → close connection
}
```

**What Happens:**
1. **Send Timeout**: If the WebSocket send operation blocks (client not reading fast enough), the send will eventually fail
2. **Connection Dropped**: The server immediately closes the connection (no graceful degradation)
3. **No Warning Message**: The client simply receives a close frame (code 1006 Abnormal Closure or 1011 Internal Error)

**Why This Happens:**
- WebSocket send buffers fill up when client is slow to read
- Server cannot block indefinitely (would exhaust resources)
- Protects server from resource exhaustion by slow/dead clients

**How to Avoid:**
```javascript
// ✅ GOOD: Asynchronous, non-blocking handling
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  if (data.type === 'ping') return;

  // Dispatch to async handler (doesn't block onmessage)
  queueMicrotask(() => processEvent(data));
};

// ❌ BAD: Synchronous, blocking operations
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  // Heavy computation blocks the message loop
  for (let i = 0; i < 1000000; i++) { /* ... */ }
  updateUI(data);  // Slow DOM operations
};
```

**Best Practices:**
1. **Keep `onmessage` Fast**: Offload heavy processing to async handlers or workers
2. **Monitor Connection Health**: Track ping intervals to detect degradation early
3. **Implement Reconnection**: Auto-reconnect when dropped (see Reconnection Strategies below)
4. **Rate Limit UI Updates**: Debounce high-frequency events (e.g., build progress updates)

**Reconnection Strategies:**

Since `/api/events/ws` provides no automatic reconnection, clients must implement retry logic. **Recommended pattern**:

```javascript
class ResilientEventStream {
  constructor(url) {
    this.url = url;
    this.ws = null;
    this.reconnectAttempts = 0;
    this.maxReconnectDelay = 30000;  // 30 seconds max
    this.listeners = new Map();
    this.connect();
  }

  connect() {
    this.ws = new WebSocket(this.url);

    this.ws.onopen = () => {
      console.log('[WS] Connected');
      this.reconnectAttempts = 0;  // Reset backoff on success
      this.emit('open');
    };

    this.ws.onmessage = (event) => {
      const data = JSON.parse(event.data);

      // Handle heartbeat
      if (data.type === 'ping') {
        console.log('[WS] Heartbeat:', data.timestamp);
        return;
      }

      this.emit('event', data);
    };

    this.ws.onclose = (event) => {
      console.warn('[WS] Disconnected:', event.code, event.reason);
      this.scheduleReconnect();
    };

    this.ws.onerror = (error) => {
      console.error('[WS] Error:', error);
      // onclose will fire after onerror, triggering reconnect
    };
  }

  scheduleReconnect() {
    // Exponential backoff: 1s, 2s, 4s, 8s, 16s, 30s (max)
    const delay = Math.min(
      1000 * Math.pow(2, this.reconnectAttempts),
      this.maxReconnectDelay
    );

    this.reconnectAttempts++;
    console.log(`[WS] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})...`);

    setTimeout(() => this.connect(), delay);
  }

  on(event, handler) {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, []);
    }
    this.listeners.get(event).push(handler);
  }

  emit(event, data) {
    const handlers = this.listeners.get(event) || [];
    handlers.forEach(handler => handler(data));
  }

  close() {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}

// Usage
const stream = new ResilientEventStream('ws://localhost:3000/api/events/ws');

stream.on('open', () => {
  console.log('Event stream ready');
});

stream.on('event', (event) => {
  console.log('Received event:', event);
});
```

**Reconnection Timeline (Exponential Backoff):**
```
Attempt 1:  1 second delay
Attempt 2:  2 seconds delay
Attempt 3:  4 seconds delay
Attempt 4:  8 seconds delay
Attempt 5: 16 seconds delay
Attempt 6: 30 seconds delay (capped)
Attempt 7: 30 seconds delay (capped)
...
```

**Alternative: Linear Backoff with Jitter**
```javascript
scheduleReconnect() {
  // 5 seconds + random jitter (0-2 seconds)
  const delay = 5000 + Math.random() * 2000;

  console.log(`[WS] Reconnecting in ${delay.toFixed(0)}ms...`);
  setTimeout(() => this.connect(), delay);
}
```

**⚠️ Important Considerations:**
1. **No State Recovery**: Reconnected clients start fresh — missed events are gone
2. **Application-Level Sync**: After reconnection, clients may need to poll REST APIs to sync application state
3. **Max Reconnect Attempts**: Consider capping total attempts (e.g., 10 retries) to avoid infinite loops on permanent failures
4. **Network Change Detection**: On mobile, listen for `online` events to trigger immediate reconnection:

```javascript
window.addEventListener('online', () => {
  console.log('[WS] Network restored, reconnecting...');
  this.connect();
});
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

## Subscription Methods (Internal Architecture)

> **🔒 Internal API:** The subscription methods described in this section are server-side implementation details. WebSocket clients **cannot** specify filters when connecting—they always receive all events and must implement client-side filtering (see next section).

The Auto-Tundra event bus provides three subscription methods for internal server components:

### `subscribe()` — Unfiltered Subscription

**Description:** Creates a subscription that receives **all events** published to the event bus.

**Usage (Server-Side):**
```rust
use at_bridge::event_bus::EventBus;

let bus = EventBus::new();
let rx = bus.subscribe();  // Receives ALL events

// Process all events
while let Ok(msg) = rx.recv() {
    println!("Event: {:?}", msg);
}
```

**Characteristics:**
- ✅ Receives every `BridgeMessage` published to the bus
- ✅ No filtering overhead—messages delivered immediately
- ✅ Used by WebSocket handlers (`/ws`, `/api/events/ws`)
- ⚠️ Clients must implement their own filtering logic

**When Used:**
- WebSocket connections (all clients get unfiltered streams)
- System-wide event monitors
- Logging and audit systems
- Debugging tools

---

### `subscribe_filtered(predicate)` — Custom Predicate Filtering

**Description:** Creates a subscription with **server-side filtering** using a custom predicate function.

**Usage (Server-Side):**
```rust
// Only receive GetStatus and StatusUpdate messages
let rx = bus.subscribe_filtered(|msg| {
    matches!(msg, BridgeMessage::GetStatus | BridgeMessage::StatusUpdate(_))
});

// Only receive events with specific payload conditions
let rx = bus.subscribe_filtered(|msg| {
    match msg {
        BridgeMessage::Event(payload) => payload.event_type == "critical",
        _ => false,
    }
});
```

**Characteristics:**
- ✅ Server-side filtering reduces unnecessary message delivery
- ✅ Accepts any `Fn(&BridgeMessage) -> bool` predicate
- ✅ Filtered subscribers are retained even when messages don't match
- ⚠️ **Not exposed to WebSocket clients** (internal API only)

**When Used:**
- Internal service-to-service subscriptions
- Notification system integration (filters for user-specific events)
- Agent-specific event routing
- Performance optimization for high-throughput scenarios

---

### `subscribe_for_agent(agent_id)` — Agent-Specific Filtering

**Description:** Convenience method that filters events targeting a **specific agent UUID**.

**Usage (Server-Side):**
```rust
use uuid::Uuid;

let agent_id = Uuid::parse_str("f0e1d2c3-4567-89ab-cdef-0123456789ab")?;
let rx = bus.subscribe_for_agent(agent_id);

// Only receives messages for this agent
while let Ok(msg) = rx.recv() {
    match msg.as_ref() {
        BridgeMessage::SlingBead { bead_id, .. } => {
            println!("Bead {} assigned to agent", bead_id);
        }
        BridgeMessage::AgentOutput { output, .. } => {
            println!("Agent output: {}", output);
        }
        BridgeMessage::Event(payload) => {
            println!("Agent event: {}", payload.event_type);
        }
        _ => {}
    }
}
```

**Filters on:**
- `BridgeMessage::SlingBead { agent_id, .. }`
- `BridgeMessage::AgentOutput { agent_id, .. }`
- `BridgeMessage::Event(EventPayload { agent_id: Some(...), .. })`

**Characteristics:**
- ✅ Automatically extracts `agent_id` from multiple message variants
- ✅ Implements `subscribe_filtered()` under the hood
- ✅ Simplifies agent-specific event monitoring
- ⚠️ **Not exposed to WebSocket clients** (internal API only)

**When Used:**
- Agent management systems
- Per-agent log aggregation
- Agent-specific notification delivery
- Agent performance monitoring

---

## Filtering Events (Client-Side)

> **💡 Important:** WebSocket clients receive **all events** from the server and must implement client-side filtering. Server-side filtering (described above) is not available to external clients.

Since events are broadcast to all connected WebSocket clients, you must implement filtering in your client application. Here are recommended patterns:

### Basic Event Type Filtering

Filter by event kind to process only relevant events:

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

### Agent-Specific Filtering

To monitor events for a specific agent, filter on `agent_id` fields:

```javascript
const TARGET_AGENT_ID = 'f0e1d2c3-4567-89ab-cdef-0123456789ab';

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  // Skip heartbeats
  if (data.type === 'ping') return;

  // Agent-specific filtering
  const agentId = data.payload?.agent_id;
  if (agentId === TARGET_AGENT_ID) {
    console.log('Event for target agent:', data.kind);
    handleAgentEvent(data);
  }
};
```

### Multi-Criteria Filtering

Combine multiple filter conditions for complex scenarios:

```javascript
const MONITORED_AGENTS = new Set([
  'agent-1-uuid',
  'agent-2-uuid',
  'agent-3-uuid'
]);

const CRITICAL_EVENT_TYPES = new Set([
  'agent.error',
  'task.failed',
  'bead.rejected'
]);

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  if (data.type === 'ping') return;

  // Multi-criteria filter
  const isCritical = CRITICAL_EVENT_TYPES.has(data.kind);
  const isMonitored = MONITORED_AGENTS.has(data.payload?.agent_id);

  if (isCritical && isMonitored) {
    alertCriticalEvent(data);
  }
};
```

### Filter Pattern Registry (Advanced)

For complex applications, use a filter registry pattern:

```javascript
class EventFilterRegistry {
  constructor() {
    this.filters = new Map();
  }

  // Register a named filter
  register(name, predicate) {
    this.filters.set(name, predicate);
  }

  // Test event against all registered filters
  test(event) {
    for (const [name, predicate] of this.filters) {
      if (predicate(event)) {
        return { matched: true, filter: name };
      }
    }
    return { matched: false };
  }
}

// Usage
const registry = new EventFilterRegistry();

registry.register('critical-errors', (event) =>
  event.kind === 'agent.error' && event.payload?.severity === 'critical'
);

registry.register('high-priority-tasks', (event) =>
  event.kind === 'task.created' && event.payload?.priority > 8
);

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  if (data.type === 'ping') return;

  const result = registry.test(data);
  if (result.matched) {
    console.log(`Matched filter: ${result.filter}`);
    handleFilteredEvent(data, result.filter);
  }
};
```

### Performance Considerations

**⚠️ Best Practices:**

1. **Early Return for Heartbeats:** Always check for `type === 'ping'` first to avoid unnecessary processing
2. **Use Sets for Lookup:** When filtering by multiple IDs/types, use `Set` instead of arrays for O(1) lookup
3. **Debounce High-Frequency Events:** If receiving many events per second, debounce UI updates
4. **Consider IndexedDB for History:** Store filtered events in IndexedDB for later analysis
5. **Lazy Deserialization:** Only parse `payload` if the event passes initial filters

**Example: Optimized Filter Chain**
```javascript
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);

  // Fast path: ignore heartbeats (most common)
  if (data.type === 'ping') return;

  // Fast path: check kind before accessing payload
  if (!INTERESTING_KINDS.has(data.kind)) return;

  // Only now access payload (may be large)
  const { agent_id, severity } = data.payload || {};

  if (MONITORED_AGENTS.has(agent_id) && severity === 'high') {
    handleEvent(data);
  }
};
```

### Client-Side Filtering Recommendations

| Scenario | Recommended Approach |
|----------|---------------------|
| **Monitor specific agent** | Filter on `payload.agent_id` |
| **Alert on critical events** | Filter on `kind` + `payload.severity` |
| **Track task lifecycle** | Filter on `kind` starting with `task.` |
| **Debug event flow** | Log all events with conditional filtering |
| **Build event dashboard** | Use filter registry + debounced UI updates |
| **Audit trail** | Store all events, filter on query/display |

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

### Message Direction & Quick Reference

#### Frontend → Backend (Client Commands)

These messages are sent by clients to request data or trigger server actions:

| Message | Purpose | Expects Response |
|---------|---------|------------------|
| `GetStatus` | Request server status | `StatusUpdate` |
| `GetKpi` | Request KPI metrics | `KpiUpdate` |
| `ListBeads` | Request beads (with optional filter) | `BeadList` |
| `ListAgents` | Request all agents | `AgentList` |
| `HookBead` | Create and assign new bead | `BeadCreated`, `BeadUpdated` |
| `SlingBead` | Assign bead to agent | `BeadUpdated` (broadcast) |
| `DoneBead` | Mark bead as completed/failed | `BeadUpdated` (broadcast) |
| `NudgeAgent` | Send message/command to agent | May trigger `AgentOutput` |

#### Backend → Frontend (Server Responses/Events)

These messages are sent by the server in response to requests or as real-time event broadcasts:

| Message | Category | Trigger |
|---------|----------|---------|
| `StatusUpdate` | Response | Reply to `GetStatus` |
| `KpiUpdate` | Response | Reply to `GetKpi` |
| `BeadList` | Response | Reply to `ListBeads` |
| `AgentList` | Response | Reply to `ListAgents` |
| `AgentOutput` | Event | Agent produces output |
| `Error` | Response/Event | Request error or system failure |
| `Event` | Event | Generic system event notification |
| `TaskUpdate` | Event | Task progress/phase change |
| `BeadCreated` | Event | New bead created |
| `BeadUpdated` | Event | Bead state changed |
| `MergeResult` | Event | Git merge completed/conflicted |
| `QueueUpdate` | Event | Task queue reordered |

### Message Examples

#### Unit Variants (No Payload)

**`GetStatus`** — Request server status

**Direction:** Frontend → Backend
**When sent:** Client requests current server status (version, uptime, active agents/beads)
**Response:** Server sends `StatusUpdate` with current metrics

```json
{
  "type": "get_status"
}
```

**`ListAgents`** — Request list of all agents

**Direction:** Frontend → Backend
**When sent:** Client needs to retrieve all registered agents and their current status
**Response:** Server sends `AgentList` with array of agent objects

```json
{
  "type": "list_agents"
}
```

**`GetKpi`** — Request KPI metrics

**Direction:** Frontend → Backend
**When sent:** Client requests system-wide KPI metrics (bead counts by status, active agents)
**Response:** Server sends `KpiUpdate` with current statistics

```json
{
  "type": "get_kpi"
}
```

#### Struct Variants (Named Fields)

**`ListBeads`** — Request beads with optional status filter

**Direction:** Frontend → Backend
**When sent:** Client needs to retrieve beads, optionally filtered by status (backlog, hooked, slung, review, done, failed, escalated)
**Response:** Server sends `BeadList` with array of matching bead objects
**Payload:**
- `status` (optional): Filter by bead status. If `null`, returns all beads.

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

**Direction:** Frontend → Backend
**When sent:** Client assigns a specific bead to an agent for execution (transitions bead from hooked → slung)
**Response:** Server updates bead status and broadcasts `BeadUpdated` event to all connected clients
**Payload:**
- `bead_id`: UUID of the bead to assign
- `agent_id`: UUID of the target agent

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

**Direction:** Frontend → Backend
**When sent:** Client creates a new bead and immediately hooks it to an agent (combines creation + assignment)
**Response:** Server creates bead, broadcasts `BeadCreated` event, and may send `BeadUpdated` when hooked
**Payload:**
- `title`: Human-readable bead title
- `agent_name`: Name of the agent to hook the bead to

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

**Direction:** Frontend → Backend
**When sent:** Client marks a bead as finished (success or failure) after review
**Response:** Server transitions bead to done/failed status and broadcasts `BeadUpdated` event
**Payload:**
- `bead_id`: UUID of the bead to mark as done
- `failed`: `true` if bead failed, `false` if completed successfully

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

**Direction:** Frontend → Backend
**When sent:** Client sends an instruction or message to a running agent (e.g., to trigger a specific action)
**Response:** Agent receives message and may produce `AgentOutput` or state changes
**Payload:**
- `agent_name`: Name of the target agent
- `message`: Text message/command to send to the agent

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

**Direction:** Backend → Frontend
**When sent:** Agent produces stdout/stderr output during task execution
**Trigger:** Real-time streaming of agent console output as it executes commands or processes
**Payload:**
- `agent_id`: UUID of the agent producing output
- `output`: Raw text output (may include ANSI escape codes, newlines)

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

**Direction:** Backend → Frontend
**When sent:** Server encounters an error processing a client request or during internal operations
**Trigger:** Invalid request, missing resources, permission errors, or system failures
**Payload:**
- `code`: Machine-readable error code (e.g., `BEAD_NOT_FOUND`, `INVALID_STATUS_TRANSITION`)
- `message`: Human-readable error description

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

**Direction:** Backend → Frontend
**When sent:** Git merge operation completes (success or conflict) on a worktree branch
**Trigger:** Automated merge attempts, worktree cleanup, or manual merge operations
**Payload:**
- `worktree_id`: Identifier of the git worktree
- `branch`: Branch name involved in the merge
- `status`: Merge result (`"success"`, `"conflict"`, `"failed"`)
- `conflict_files`: Array of file paths with conflicts (empty if no conflicts)

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

**Direction:** Backend → Frontend
**When sent:** Task queue order or priorities change
**Trigger:** Manual reordering, priority adjustments, or automatic queue optimization
**Payload:**
- `task_ids`: Ordered array of task UUIDs representing the new queue sequence

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

**Direction:** Backend → Frontend
**When sent:** Response to `GetStatus` request or periodic server health broadcasts
**Trigger:** Client requests status via `GetStatus` command
**Payload:**
- `version`: Server version string (semver format)
- `uptime_seconds`: Time since server started (in seconds)
- `agents_active`: Number of currently active agents
- `beads_active`: Number of beads currently being processed

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

**Direction:** Backend → Frontend
**When sent:** Response to `ListBeads` request
**Trigger:** Client requests bead list (with optional status filter)
**Payload:** Array of `Bead` objects (see `at_core::types::Bead` structure)

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

**Direction:** Backend → Frontend
**When sent:** Response to `ListAgents` request
**Trigger:** Client requests all registered agents
**Payload:** Array of `Agent` objects (see `at_core::types::Agent` structure)

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

**Direction:** Backend → Frontend
**When sent:** Response to `GetKpi` request or periodic KPI broadcasts
**Trigger:** Client requests metrics via `GetKpi` command
**Payload:**
- `total_beads`: Total number of beads in the system
- `backlog`: Beads in backlog status
- `hooked`: Beads in hooked status (assigned but not started)
- `slung`: Beads in slung status (actively being processed)
- `review`: Beads in review status (awaiting approval)
- `done`: Beads successfully completed
- `failed`: Beads that failed
- `active_agents`: Number of agents currently processing tasks

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

**Direction:** Backend → Frontend
**When sent:** Generic system events that don't fit other specific message types
**Trigger:** Various system events (bead status changes, agent lifecycle events, system notifications)
**Payload:**
- `event_type`: Event category (e.g., `"bead.status_change"`, `"agent.started"`)
- `agent_id`: UUID of related agent (optional, `null` if not agent-specific)
- `bead_id`: UUID of related bead (optional, `null` if not bead-specific)
- `message`: Human-readable event description
- `timestamp`: ISO 8601 timestamp when event occurred

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

**Direction:** Backend → Frontend
**When sent:** Task phase changes, progress updates, or subtask status changes
**Trigger:** Agent reports task progress, phase transitions (planning → implementation → QA), or subtask completions
**Payload:** Complete `Task` object from `at_core::types::Task` (boxed for efficiency)

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

**Direction:** Backend → Frontend
**When sent:** New bead is created in the system
**Trigger:** REST API bead creation, `HookBead` command, or automated bead generation
**Payload:** Complete `Bead` object (see `at_core::types::Bead` structure)

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

**Direction:** Backend → Frontend
**When sent:** Bead properties change (status, agent assignment, metadata, etc.)
**Trigger:** Status transitions (slung, review, done), agent reassignment, or metadata updates
**Payload:** Complete updated `Bead` object with current state

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

## Example 4: TypeScript Client with Type Safety

```typescript
// types.ts - Define event types for type safety
export type EventKind =
  | 'agent.status'
  | 'agent.spawned'
  | 'task.progress'
  | 'bead.done'
  | 'notification.created';

export interface BaseEvent<K extends EventKind, P = unknown> {
  kind: K;
  payload: P;
  timestamp: string;
  metadata?: Record<string, unknown>;
}

export interface AgentStatusPayload {
  agent_id: string;
  status: 'idle' | 'spawning' | 'active' | 'stopping' | 'stopped';
}

export interface TaskProgressPayload {
  task_id: string;
  progress: number;
  message?: string;
}

export interface BeadDonePayload {
  bead_id: string;
  result: 'success' | 'failure';
  error?: string;
}

export type AgentStatusEvent = BaseEvent<'agent.status', AgentStatusPayload>;
export type TaskProgressEvent = BaseEvent<'task.progress', TaskProgressPayload>;
export type BeadDoneEvent = BaseEvent<'bead.done', BeadDonePayload>;
export type AutoTundraEvent = AgentStatusEvent | TaskProgressEvent | BeadDoneEvent;

export interface HeartbeatMessage {
  type: 'ping';
  timestamp: string;
}

export type WebSocketMessage = AutoTundraEvent | HeartbeatMessage;

// client.ts - Type-safe WebSocket client
export class TypedEventClient {
  private ws: WebSocket | null = null;
  private handlers = new Map<EventKind, Set<(payload: any) => void>>();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 10;
  private reconnectDelay = 1000; // Start at 1 second
  private heartbeatInterval: number | null = null;
  private connectionState: 'connecting' | 'connected' | 'disconnected' = 'disconnected';

  constructor(
    private url: string = 'ws://localhost:3000/api/events/ws',
    private onStateChange?: (state: 'connecting' | 'connected' | 'disconnected') => void
  ) {}

  connect(): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      console.warn('Already connected');
      return;
    }

    this.updateState('connecting');
    this.ws = new WebSocket(this.url);

    this.ws.onopen = () => {
      console.log('WebSocket connected');
      this.reconnectAttempts = 0;
      this.reconnectDelay = 1000;
      this.updateState('connected');
      this.startHeartbeatMonitor();
    };

    this.ws.onmessage = (event) => {
      try {
        const message: WebSocketMessage = JSON.parse(event.data);

        // Handle heartbeat
        if ('type' in message && message.type === 'ping') {
          console.debug('Heartbeat received:', message.timestamp);
          return;
        }

        // Handle events
        const autoEvent = message as AutoTundraEvent;
        const handlers = this.handlers.get(autoEvent.kind);

        if (handlers) {
          handlers.forEach(handler => {
            try {
              handler(autoEvent.payload);
            } catch (error) {
              console.error(`Error in handler for ${autoEvent.kind}:`, error);
            }
          });
        }
      } catch (error) {
        console.error('Failed to parse WebSocket message:', error);
      }
    };

    this.ws.onerror = (error) => {
      console.error('WebSocket error:', error);
    };

    this.ws.onclose = (event) => {
      console.log('WebSocket closed:', event.code, event.reason);
      this.updateState('disconnected');
      this.stopHeartbeatMonitor();
      this.attemptReconnect();
    };
  }

  private updateState(state: 'connecting' | 'connected' | 'disconnected'): void {
    this.connectionState = state;
    this.onStateChange?.(state);
  }

  private startHeartbeatMonitor(): void {
    // Monitor for missed heartbeats (should arrive every 30s)
    this.heartbeatInterval = window.setInterval(() => {
      if (this.ws?.readyState !== WebSocket.OPEN) {
        this.stopHeartbeatMonitor();
      }
    }, 35000); // Check every 35s (5s grace period)
  }

  private stopHeartbeatMonitor(): void {
    if (this.heartbeatInterval !== null) {
      clearInterval(this.heartbeatInterval);
      this.heartbeatInterval = null;
    }
  }

  private attemptReconnect(): void {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      console.error('Max reconnection attempts reached');
      return;
    }

    this.reconnectAttempts++;

    // Exponential backoff: 1s, 2s, 4s, 8s, 16s, 32s (capped at 32s)
    const delay = Math.min(this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1), 32000);

    console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);

    setTimeout(() => this.connect(), delay);
  }

  on<K extends EventKind>(
    kind: K,
    handler: (payload: Extract<AutoTundraEvent, { kind: K }>['payload']) => void
  ): void {
    if (!this.handlers.has(kind)) {
      this.handlers.set(kind, new Set());
    }
    this.handlers.get(kind)!.add(handler);
  }

  off<K extends EventKind>(
    kind: K,
    handler: (payload: Extract<AutoTundraEvent, { kind: K }>['payload']) => void
  ): void {
    this.handlers.get(kind)?.delete(handler);
  }

  disconnect(): void {
    this.stopHeartbeatMonitor();
    if (this.ws) {
      this.ws.close(1000, 'Client disconnect');
      this.ws = null;
    }
    this.updateState('disconnected');
  }

  getState(): 'connecting' | 'connected' | 'disconnected' {
    return this.connectionState;
  }
}

// Usage
const client = new TypedEventClient(
  'ws://localhost:3000/api/events/ws',
  (state) => console.log('Connection state:', state)
);

client.on('agent.status', (payload) => {
  // TypeScript knows payload is AgentStatusPayload
  console.log(`Agent ${payload.agent_id}: ${payload.status}`);
});

client.on('task.progress', (payload) => {
  // TypeScript knows payload is TaskProgressPayload
  const percentage = Math.round(payload.progress * 100);
  console.log(`Task ${payload.task_id}: ${percentage}%`);
});

client.connect();
```

## Example 5: Advanced Error Handling & Reconnection

```typescript
// reconnection-manager.ts - Advanced reconnection strategy
export interface ReconnectionConfig {
  maxAttempts: number;
  initialDelay: number;
  maxDelay: number;
  backoffMultiplier: number;
  jitter: boolean;
}

export class RobustWebSocketClient {
  private ws: WebSocket | null = null;
  private reconnectAttempts = 0;
  private reconnectTimeout: number | null = null;
  private intentionalClose = false;
  private messageQueue: string[] = [];
  private maxQueueSize = 100;

  private config: ReconnectionConfig = {
    maxAttempts: 15,
    initialDelay: 1000,
    maxDelay: 60000,
    backoffMultiplier: 2,
    jitter: true
  };

  constructor(
    private url: string,
    private onMessage: (data: any) => void,
    private onStateChange?: (state: 'connecting' | 'connected' | 'disconnected' | 'error') => void,
    config?: Partial<ReconnectionConfig>
  ) {
    this.config = { ...this.config, ...config };
  }

  connect(): void {
    if (this.ws?.readyState === WebSocket.OPEN) return;

    this.intentionalClose = false;
    this.onStateChange?.('connecting');

    try {
      this.ws = new WebSocket(this.url);
      this.setupEventHandlers();
    } catch (error) {
      console.error('Failed to create WebSocket:', error);
      this.onStateChange?.('error');
      this.scheduleReconnect();
    }
  }

  private setupEventHandlers(): void {
    if (!this.ws) return;

    this.ws.onopen = () => {
      console.log('✓ WebSocket connected');
      this.reconnectAttempts = 0;
      this.onStateChange?.('connected');
      this.flushMessageQueue();
    };

    this.ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);

        // Ignore heartbeats at this layer
        if (data.type === 'ping') return;

        this.onMessage(data);
      } catch (error) {
        console.error('Failed to parse message:', error);
      }
    };

    this.ws.onerror = (error) => {
      console.error('WebSocket error:', error);
      this.onStateChange?.('error');
    };

    this.ws.onclose = (event) => {
      console.log(`WebSocket closed: ${event.code} ${event.reason || '(no reason)'}`);
      this.onStateChange?.('disconnected');

      // Don't reconnect if closed intentionally
      if (!this.intentionalClose) {
        this.scheduleReconnect();
      }
    };
  }

  private calculateReconnectDelay(): number {
    const { initialDelay, maxDelay, backoffMultiplier, jitter } = this.config;

    // Exponential backoff
    let delay = initialDelay * Math.pow(backoffMultiplier, this.reconnectAttempts);
    delay = Math.min(delay, maxDelay);

    // Add jitter to prevent thundering herd
    if (jitter) {
      const jitterAmount = delay * 0.3; // ±30% jitter
      delay += (Math.random() * 2 - 1) * jitterAmount;
    }

    return Math.floor(delay);
  }

  private scheduleReconnect(): void {
    if (this.reconnectAttempts >= this.config.maxAttempts) {
      console.error('✗ Max reconnection attempts reached. Giving up.');
      this.onStateChange?.('error');
      return;
    }

    if (this.reconnectTimeout !== null) {
      clearTimeout(this.reconnectTimeout);
    }

    const delay = this.calculateReconnectDelay();
    this.reconnectAttempts++;

    console.log(
      `⟳ Reconnecting in ${(delay / 1000).toFixed(1)}s ` +
      `(attempt ${this.reconnectAttempts}/${this.config.maxAttempts})`
    );

    this.reconnectTimeout = window.setTimeout(() => {
      this.reconnectTimeout = null;
      this.connect();
    }, delay);
  }

  send(message: string | object): boolean {
    const data = typeof message === 'string' ? message : JSON.stringify(message);

    // If connected, send immediately
    if (this.ws?.readyState === WebSocket.OPEN) {
      try {
        this.ws.send(data);
        return true;
      } catch (error) {
        console.error('Failed to send message:', error);
        this.queueMessage(data);
        return false;
      }
    }

    // Otherwise queue for later
    this.queueMessage(data);
    return false;
  }

  private queueMessage(data: string): void {
    if (this.messageQueue.length >= this.maxQueueSize) {
      console.warn('Message queue full, dropping oldest message');
      this.messageQueue.shift();
    }
    this.messageQueue.push(data);
  }

  private flushMessageQueue(): void {
    while (this.messageQueue.length > 0 && this.ws?.readyState === WebSocket.OPEN) {
      const message = this.messageQueue.shift()!;
      try {
        this.ws.send(message);
      } catch (error) {
        console.error('Failed to send queued message:', error);
        this.messageQueue.unshift(message); // Put it back
        break;
      }
    }
  }

  disconnect(): void {
    this.intentionalClose = true;

    if (this.reconnectTimeout !== null) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }

    if (this.ws) {
      this.ws.close(1000, 'Client disconnect');
      this.ws = null;
    }

    this.messageQueue = [];
    this.onStateChange?.('disconnected');
  }

  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  getQueueSize(): number {
    return this.messageQueue.length;
  }
}

// Usage
const client = new RobustWebSocketClient(
  'ws://localhost:3000/api/events/ws',
  (event) => {
    console.log('Event:', event.kind, event.payload);
  },
  (state) => {
    console.log('Connection state:', state);
    document.getElementById('status')!.textContent = state;
  }
);

client.connect();

// Messages sent while disconnected are queued and sent on reconnection
client.send({ type: 'subscribe', topics: ['agent.*', 'task.*'] });
```

## Example 6: React Integration

```typescript
// useWebSocket.ts - React Hook for WebSocket connection
import { useEffect, useRef, useState, useCallback } from 'react';

interface UseWebSocketOptions {
  url: string;
  onMessage?: (event: any) => void;
  onError?: (error: Event) => void;
  autoReconnect?: boolean;
  reconnectInterval?: number;
}

interface WebSocketState {
  isConnected: boolean;
  isConnecting: boolean;
  error: Error | null;
  lastMessage: any | null;
}

export function useWebSocket(options: UseWebSocketOptions) {
  const {
    url,
    onMessage,
    onError,
    autoReconnect = true,
    reconnectInterval = 5000
  } = options;

  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<number | null>(null);
  const [state, setState] = useState<WebSocketState>({
    isConnected: false,
    isConnecting: false,
    error: null,
    lastMessage: null
  });

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return;

    setState(prev => ({ ...prev, isConnecting: true, error: null }));

    const ws = new WebSocket(url);

    ws.onopen = () => {
      console.log('WebSocket connected');
      setState(prev => ({ ...prev, isConnected: true, isConnecting: false }));
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);

        // Ignore heartbeats
        if (data.type === 'ping') return;

        setState(prev => ({ ...prev, lastMessage: data }));
        onMessage?.(data);
      } catch (error) {
        console.error('Failed to parse message:', error);
      }
    };

    ws.onerror = (error) => {
      console.error('WebSocket error:', error);
      setState(prev => ({
        ...prev,
        error: new Error('WebSocket connection error')
      }));
      onError?.(error);
    };

    ws.onclose = () => {
      console.log('WebSocket closed');
      setState(prev => ({
        ...prev,
        isConnected: false,
        isConnecting: false
      }));

      // Auto-reconnect if enabled
      if (autoReconnect && reconnectTimeoutRef.current === null) {
        reconnectTimeoutRef.current = window.setTimeout(() => {
          reconnectTimeoutRef.current = null;
          connect();
        }, reconnectInterval);
      }
    };

    wsRef.current = ws;
  }, [url, onMessage, onError, autoReconnect, reconnectInterval]);

  const disconnect = useCallback(() => {
    if (reconnectTimeoutRef.current !== null) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }

    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
  }, []);

  const sendMessage = useCallback((message: any) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(message));
      return true;
    }
    return false;
  }, []);

  useEffect(() => {
    connect();

    return () => {
      disconnect();
    };
  }, [connect, disconnect]);

  return {
    ...state,
    sendMessage,
    reconnect: connect,
    disconnect
  };
}

// EventDashboard.tsx - React component using the hook
import React from 'react';
import { useWebSocket } from './useWebSocket';

interface Event {
  kind: string;
  payload: any;
  timestamp: string;
}

export function EventDashboard() {
  const [events, setEvents] = React.useState<Event[]>([]);

  const { isConnected, isConnecting, error, lastMessage } = useWebSocket({
    url: 'ws://localhost:3000/api/events/ws',
    onMessage: (event) => {
      setEvents(prev => [event, ...prev].slice(0, 100)); // Keep last 100 events
    }
  });

  return (
    <div className="dashboard">
      <header>
        <h1>Auto-Tundra Events</h1>
        <div className="status">
          {isConnecting && <span className="badge connecting">Connecting...</span>}
          {isConnected && <span className="badge connected">Connected</span>}
          {error && <span className="badge error">Error: {error.message}</span>}
        </div>
      </header>

      <div className="events-list">
        {events.length === 0 ? (
          <p className="empty">No events yet</p>
        ) : (
          events.map((event, index) => (
            <div key={index} className="event-card">
              <div className="event-header">
                <span className="event-kind">{event.kind}</span>
                <span className="event-time">
                  {new Date(event.timestamp).toLocaleTimeString()}
                </span>
              </div>
              <pre className="event-payload">
                {JSON.stringify(event.payload, null, 2)}
              </pre>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
```

## Example 7: Vue Integration

```typescript
// useWebSocket.ts - Vue 3 Composable
import { ref, onMounted, onUnmounted, Ref } from 'vue';

interface UseWebSocketOptions {
  url: string;
  autoConnect?: boolean;
  reconnect?: boolean;
  reconnectInterval?: number;
}

export function useWebSocket(options: UseWebSocketOptions) {
  const { url, autoConnect = true, reconnect = true, reconnectInterval = 5000 } = options;

  const ws: Ref<WebSocket | null> = ref(null);
  const isConnected = ref(false);
  const isConnecting = ref(false);
  const lastMessage: Ref<any> = ref(null);
  const error: Ref<Error | null> = ref(null);

  let reconnectTimeout: number | null = null;

  const connect = () => {
    if (ws.value?.readyState === WebSocket.OPEN) {
      console.warn('Already connected');
      return;
    }

    isConnecting.value = true;
    error.value = null;

    const websocket = new WebSocket(url);

    websocket.onopen = () => {
      console.log('WebSocket connected');
      isConnected.value = true;
      isConnecting.value = false;
      ws.value = websocket;
    };

    websocket.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);

        // Ignore heartbeats
        if (data.type === 'ping') return;

        lastMessage.value = data;
      } catch (err) {
        console.error('Failed to parse message:', err);
      }
    };

    websocket.onerror = (event) => {
      console.error('WebSocket error:', event);
      error.value = new Error('WebSocket connection error');
    };

    websocket.onclose = () => {
      console.log('WebSocket closed');
      isConnected.value = false;
      isConnecting.value = false;
      ws.value = null;

      // Auto-reconnect if enabled
      if (reconnect && reconnectTimeout === null) {
        reconnectTimeout = window.setTimeout(() => {
          reconnectTimeout = null;
          connect();
        }, reconnectInterval);
      }
    };
  };

  const disconnect = () => {
    if (reconnectTimeout !== null) {
      clearTimeout(reconnectTimeout);
      reconnectTimeout = null;
    }

    if (ws.value) {
      ws.value.close();
      ws.value = null;
    }

    isConnected.value = false;
  };

  const send = (message: any): boolean => {
    if (ws.value?.readyState === WebSocket.OPEN) {
      ws.value.send(JSON.stringify(message));
      return true;
    }
    console.warn('Cannot send message: WebSocket not connected');
    return false;
  };

  onMounted(() => {
    if (autoConnect) {
      connect();
    }
  });

  onUnmounted(() => {
    disconnect();
  });

  return {
    ws,
    isConnected,
    isConnecting,
    lastMessage,
    error,
    connect,
    disconnect,
    send
  };
}

// EventMonitor.vue - Vue component using the composable
<template>
  <div class="event-monitor">
    <div class="header">
      <h2>Auto-Tundra Event Stream</h2>
      <div class="connection-status">
        <span v-if="isConnecting" class="badge connecting">
          Connecting...
        </span>
        <span v-else-if="isConnected" class="badge connected">
          Connected
        </span>
        <span v-else class="badge disconnected">
          Disconnected
        </span>
      </div>
    </div>

    <div v-if="error" class="error-message">
      {{ error.message }}
    </div>

    <div class="events-container">
      <div
        v-for="(event, index) in events"
        :key="index"
        :class="['event-item', `event-${event.kind.split('.')[0]}`]"
      >
        <div class="event-meta">
          <span class="event-kind">{{ event.kind }}</span>
          <span class="event-timestamp">
            {{ formatTime(event.timestamp) }}
          </span>
        </div>
        <div class="event-payload">
          <pre>{{ JSON.stringify(event.payload, null, 2) }}</pre>
        </div>
      </div>

      <div v-if="events.length === 0" class="empty-state">
        <p>Waiting for events...</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { useWebSocket } from './useWebSocket';

interface Event {
  kind: string;
  payload: any;
  timestamp: string;
}

const events = ref<Event[]>([]);
const maxEvents = 100;

const { isConnected, isConnecting, lastMessage, error } = useWebSocket({
  url: 'ws://localhost:3000/api/events/ws',
  autoConnect: true,
  reconnect: true
});

// Watch for new messages and add to events list
watch(lastMessage, (newMessage) => {
  if (newMessage) {
    events.value.unshift(newMessage);

    // Keep only last 100 events
    if (events.value.length > maxEvents) {
      events.value = events.value.slice(0, maxEvents);
    }
  }
});

function formatTime(timestamp: string): string {
  return new Date(timestamp).toLocaleTimeString();
}
</script>

<style scoped>
.event-monitor {
  padding: 20px;
  font-family: system-ui, -apple-system, sans-serif;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.badge {
  padding: 4px 12px;
  border-radius: 12px;
  font-size: 14px;
  font-weight: 500;
}

.badge.connected {
  background: #d4edda;
  color: #155724;
}

.badge.connecting {
  background: #fff3cd;
  color: #856404;
}

.badge.disconnected {
  background: #f8d7da;
  color: #721c24;
}

.event-item {
  margin-bottom: 12px;
  padding: 12px;
  border: 1px solid #ddd;
  border-radius: 8px;
  background: #fff;
}

.event-meta {
  display: flex;
  justify-content: space-between;
  margin-bottom: 8px;
}

.event-kind {
  font-weight: 600;
  color: #2c3e50;
}

.event-timestamp {
  color: #6c757d;
  font-size: 14px;
}

.event-payload pre {
  margin: 0;
  padding: 8px;
  background: #f8f9fa;
  border-radius: 4px;
  overflow-x: auto;
}

.empty-state {
  text-align: center;
  padding: 40px;
  color: #6c757d;
}
</style>
```

## Example 8: Rust Client with tokio-tungstenite

```rust
// Full-featured Rust client with error handling and reconnection
use tokio_tungstenite::{connect_async, tungstenite::{Message, http::Request}};
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use anyhow::{Result, Context};

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ServerMessage {
    #[serde(rename = "ping")]
    Ping { timestamp: String },

    #[serde(untagged)]
    Event(Event),
}

#[derive(Debug, Deserialize)]
struct Event {
    kind: String,
    payload: serde_json::Value,
    timestamp: String,
}

#[derive(Debug, Serialize)]
struct TerminalInput {
    r#type: String,
    data: String,
}

pub struct AutoTundraClient {
    base_url: String,
    max_reconnect_attempts: u32,
}

impl AutoTundraClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            max_reconnect_attempts: 10,
        }
    }

    /// Connect to event stream with automatic reconnection
    pub async fn subscribe_events<F>(&self, mut handler: F) -> Result<()>
    where
        F: FnMut(Event) + Send,
    {
        let mut reconnect_attempts = 0;
        let mut reconnect_delay = Duration::from_secs(1);

        loop {
            match self.try_connect_events(&mut handler).await {
                Ok(()) => {
                    // Clean disconnect, exit loop
                    break;
                }
                Err(e) => {
                    reconnect_attempts += 1;

                    if reconnect_attempts > self.max_reconnect_attempts {
                        eprintln!("✗ Max reconnection attempts reached");
                        return Err(e);
                    }

                    eprintln!(
                        "⟳ Connection lost: {}. Reconnecting in {:?} (attempt {}/{})",
                        e, reconnect_delay, reconnect_attempts, self.max_reconnect_attempts
                    );

                    tokio::time::sleep(reconnect_delay).await;

                    // Exponential backoff (cap at 32 seconds)
                    reconnect_delay = std::cmp::min(reconnect_delay * 2, Duration::from_secs(32));
                }
            }
        }

        Ok(())
    }

    async fn try_connect_events<F>(&self, handler: &mut F) -> Result<()>
    where
        F: FnMut(Event) + Send,
    {
        let ws_url = format!("{}/api/events/ws", self.base_url.replace("http", "ws"));

        let request = Request::builder()
            .uri(&ws_url)
            .header("Origin", "http://localhost")
            .body(())
            .context("Failed to build WebSocket request")?;

        let (mut ws_stream, _) = connect_async(request)
            .await
            .context("Failed to connect to WebSocket")?;

        println!("✓ Connected to event stream");

        while let Some(msg) = ws_stream.next().await {
            match msg? {
                Message::Text(text) => {
                    match serde_json::from_str::<ServerMessage>(&text) {
                        Ok(ServerMessage::Ping { timestamp }) => {
                            // Heartbeat received, connection is alive
                            eprintln!("❤ Heartbeat: {}", timestamp);
                        }
                        Ok(ServerMessage::Event(event)) => {
                            handler(event);
                        }
                        Err(e) => {
                            eprintln!("Failed to parse message: {}", e);
                        }
                    }
                }
                Message::Ping(_) => {
                    // Pong sent automatically by tokio-tungstenite
                }
                Message::Close(frame) => {
                    println!("✗ Server closed connection: {:?}", frame);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Create a terminal and interact with it
    pub async fn terminal_session(&self, commands: Vec<String>) -> Result<()> {
        let client = reqwest::Client::new();

        // 1. Create terminal
        let response = client
            .post(format!("{}/api/terminals", self.base_url))
            .json(&serde_json::json!({
                "agent_id": "00000000-0000-0000-0000-000000000000",
                "title": "Rust Terminal Session",
                "cols": 120,
                "rows": 30
            }))
            .send()
            .await
            .context("Failed to create terminal")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to create terminal: {}", response.status());
        }

        let terminal: serde_json::Value = response.json().await?;
        let terminal_id = terminal["id"]
            .as_str()
            .context("Terminal ID not found")?;

        println!("✓ Created terminal: {}", terminal_id);

        // 2. Connect to terminal WebSocket
        let ws_url = format!(
            "{}/ws/terminal/{}",
            self.base_url.replace("http", "ws"),
            terminal_id
        );

        let request = Request::builder()
            .uri(&ws_url)
            .header("Origin", "http://localhost")
            .body(())
            .context("Failed to build terminal WebSocket request")?;

        let (mut ws_stream, _) = connect_async(request)
            .await
            .context("Failed to connect to terminal WebSocket")?;

        println!("✓ Connected to terminal WebSocket");

        // 3. Send commands
        for cmd in commands {
            let input = TerminalInput {
                r#type: "input".to_string(),
                data: format!("{}\n", cmd),
            };

            ws_stream
                .send(Message::Text(serde_json::to_string(&input)?))
                .await
                .context("Failed to send command")?;

            println!("> {}", cmd);

            // Wait a bit for output
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // 4. Read output for a while
        let timeout = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                Some(msg) = ws_stream.next() => {
                    match msg? {
                        Message::Text(text) => {
                            print!("{}", text);
                        }
                        Message::Close(_) => {
                            println!("\n✗ Terminal closed");
                            break;
                        }
                        _ => {}
                    }
                }
                _ = &mut timeout => {
                    println!("\n✓ Timeout reached, closing terminal");
                    break;
                }
            }
        }

        // 5. Close WebSocket
        ws_stream.close(None).await?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = AutoTundraClient::new("http://localhost:3000");

    // Example 1: Subscribe to events
    println!("=== Event Stream Example ===");
    tokio::spawn(async move {
        let _ = client.subscribe_events(|event| {
            println!("[{}] {}: {:?}", event.timestamp, event.kind, event.payload);
        }).await;
    });

    // Example 2: Terminal session
    println!("\n=== Terminal Session Example ===");
    let terminal_client = AutoTundraClient::new("http://localhost:3000");
    terminal_client.terminal_session(vec![
        "echo 'Hello from Rust!'".to_string(),
        "pwd".to_string(),
        "ls -la".to_string(),
    ]).await?;

    Ok(())
}
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
