# at-bridge

Bridge layer connecting auto-tundra core to external interfaces.

This crate provides the transport and integration layer for auto-tundra, exposing the core agent system through multiple channels: HTTP REST API with authentication, WebSocket terminal connections, IPC protocol, event bus for system-wide notifications, and intelligence API client for LLM integration.

## Architecture Overview

The crate is built around five core components:

1. **HTTP API** (`http_api`): Axum-based REST API server; per-domain sub-routers plus a self-describing route catalog
2. **Terminal WebSocket** (`terminal_ws`): WebSocket multiplexing for terminal I/O with reconnection support
3. **IPC Protocol** (`ipc`): Inter-process communication for command execution
4. **Authentication** (`auth`): API key authentication middleware
5. **Event Bus** (`event_bus`): Pub/sub event system for real-time notifications

## Route Discovery: `GET /api/catalog`

An agent can discover the whole HTTP API in one call:

```sh
curl -s -H "X-API-Key: $AUTO_TUNDRA_API_KEY" http://localhost:$PORT/api/catalog \
  | jq '.cards[] | {method, path, description, request, response}'
```

`GET /api/catalog` (alias pinned to the schema: `GET /api/v1/catalog`) returns
an `at_api_types::catalog::ApiCatalog`. The top level follows bop's
`schema/catalog.v1.json` (`schema_version`, `generated_at`, `source`,
`cards[]` with `id` + `version`), so `bop-catalog` recipes work unchanged; each
card adds the HTTP fields:

```json
{
  "schema_version": "v1",
  "generated_at": "2026-09-22T12:00:00+00:00",
  "source": "at-bridge",
  "service": { "name": "auto-tundra", "version": "0.1.0" },
  "auth": { "scheme": "api_key", "enforced": true, "header": "x-api-key",
            "bearer": true, "ws_query_param": "api_key", "contract_version": "1.0.0" },
  "cards": [
    { "id": "get-api-agents", "version": "0.1.0", "title": "GET /api/agents",
      "description": "List agents", "domain": "agents", "method": "GET",
      "path": "/api/agents", "auth": "api_key", "response": "Vec<Agent>" },
    { "id": "post-api-beads-id-status", "version": "0.1.0",
      "title": "POST /api/beads/{id}/status",
      "description": "Transition a bead to a new status", "domain": "beads",
      "method": "POST", "path": "/api/beads/{id}/status", "auth": "api_key",
      "request": "UpdateBeadStatusRequest", "response": "Bead", "body_limit": 262144 }
  ]
}
```

- `cards` is sorted by `path`, then `method`; one card per `(method, path)`.
- `id` is derived from method + path (kebab-case) and is stable.
- `auth` is `api_key` for every route today (the auth layer wraps the whole
  router); `auth.enforced` is `false` only in development mode (no key).
- `request` / `response` / `body_limit` are omitted when unknown / default.
- Breaking shape changes bump `schema_version` and add `/api/v2/catalog`.

### Router layout

`http_api/routes.rs` defines one `Domain` per URL prefix (`beads_router()`,
`tasks_router()`, `github_router()`, ...), each nested with
`Router::nest(prefix, ..)`: `catalog` + `system` (`/api`), `beads`, `agents`,
`tasks`, `pipeline`, `terminals`, `settings`, `github`, `gitlab`, `linear`,
`kanban`, `mcp` (`/api/mcp`), `mcp_transport` (`/mcp`), `worktrees`, `queue`,
`notifications`, `metrics`, `sessions`, `projects`, `files`, `events`,
`websocket` (`/ws`), `insights`, `ideation`, `roadmap`, `memory`, `changelog`,
`context`. Handlers stay in their `http_api/*.rs` / `intelligence_api.rs` /
`terminal_ws.rs` modules.

Routes are added only through `Domain::route(RouteSpec, handler)`, which
registers the handler and records its catalog entry in the same call, so the
router and the catalog cannot drift. The shared middleware stack (CORS/origin
allowlist -> auth -> rate limit -> body limit -> security headers -> request id
-> metrics -> compression, outermost first) is applied once in
`api_router_with_auth`. axum 0.8 flattens nested routes, so `MatchedPath`
(used for per-endpoint rate-limit buckets) is still the full template, e.g.
`/api/beads/{id}/status`.

`tests/route_surface_test.rs` pins the surface: `tests/fixtures/routes.txt` is
the 134 `(method, path)` pairs of the pre-nesting router; the test asserts each
is still served and requires the key, that the catalog lists exactly those
plus the two catalog routes, and that no uncatalogued method is served on a
catalogued path.

## Security

### Request Body Size Limits

All HTTP API endpoints enforce request body size limits to prevent denial-of-service attacks via oversized payloads (CWE-770: Uncontrolled Resource Consumption).

**Default Limit: 2MB**
- Applied globally to all endpoints via `DefaultBodyLimit` middleware layer
- Placed early in the middleware stack (before authentication and CORS) for efficient rejection
- Protects 33 endpoints including task creation, settings updates, GitHub integration, and intelligence API

**Per-Endpoint Overrides:**

| Endpoint Category | Limit | Endpoint Count | Rationale |
|-------------------|-------|----------------|-----------|
| **File Attachments** | 10MB | 1 | Allows reasonable file uploads without excessive resource consumption |
| **Simple CRUD Operations** | 256KB | 20 | Status updates, toggles, and control commands require minimal payload sizes |
| **Standard Operations** | 2MB | 37 | Default limit for task creation, settings, integrations, and complex operations |

**Total Protection:** 58 POST/PUT/PATCH endpoints with explicit body size limits

**Examples:**
- `POST /api/tasks/{task_id}/attachments` → 10MB (file uploads)
- `POST /api/beads/{id}/status` → 256KB (status update)
- `POST /api/tasks` → 2MB (task creation with metadata)

**Configuration:**
- Global default: Adjust `DefaultBodyLimit::max()` in the layer chain of `api_router_with_auth` (`crates/at-bridge/src/http_api/mod.rs`)
- Per-endpoint: `.limit(bytes)` on the route's `RouteSpec` in `crates/at-bridge/src/http_api/routes.rs` (also reported as `body_limit` in the catalog)

**Security Posture:**
- ✅ CWE-770 vulnerability mitigated
- ✅ Early rejection prevents resource exhaustion
- ✅ Comprehensive test coverage (11 tests including DOS attack simulation)
- ✅ Zero security warnings from Clippy linter

### Terminal WebSocket Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    HTTP/WebSocket Server                     │
│  - REST API: POST /api/terminals, GET /api/terminals        │
│  - WebSocket: GET /ws/terminal/{id}                          │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   ▼
      ┌────────────────────────────┐
      │   TerminalRegistry         │
      │  - terminals: HashMap      │◄──── Thread-safe shared state
      │  - status tracking         │
      │  - disconnect buffers      │
      └────────┬───────────────────┘
               │ manages
               ▼
    ┌──────────────────────────────┐
    │  WebSocket Connection        │
    │  - Reader task (PTY → WS)    │
    │  - Writer task (WS → PTY)    │
    │  - Heartbeat task (ping)     │
    └──────────┬───────────────────┘
               │ communicates with
               ▼
    ┌──────────────────────────────┐
    │  PtyHandle (at-session)      │
    │  - reader/writer channels    │
    │  - process handle            │
    └──────────────────────────────┘
```

Each WebSocket connection spawns **3 concurrent tasks**:
- **Reader task**: Reads PTY output → sends to WebSocket (5-minute idle timeout)
- **Writer task**: Reads WebSocket messages → writes to PTY stdin (5-minute idle timeout)
- **Heartbeat task**: Sends Ping frames every 30 seconds to detect half-open connections

## Terminal WebSocket Protocol

The terminal WebSocket endpoint at `GET /ws/terminal/{id}` provides a bidirectional channel for terminal input and output.

### Outgoing Messages (Server → Client)

- **Text Messages**: Terminal output (stdout/stderr) sent as UTF-8 text
- **Ping Messages**: Heartbeat frames sent every 30 seconds to detect stale connections

### Incoming Messages (Client → Server)

The client can send messages in two formats:

#### 1. JSON Command Format (Structured)

JSON-serialized commands for typed operations:

```json
{"type": "input", "data": "ls -la\n"}
```

```json
{"type": "resize", "cols": 120, "rows": 30}
```

**Supported Command Types:**

| Command | JSON Format | Description |
|---------|-------------|-------------|
| **Input** | `{"type": "input", "data": "text"}` | Send keystrokes/text to terminal stdin |
| **Resize** | `{"type": "resize", "cols": 120, "rows": 30}` | Resize terminal dimensions |

#### 2. Plain Text Format (Raw Input)

Any text message that doesn't parse as JSON is treated as raw input and written directly to the PTY stdin. This allows simple clients to send keystrokes without JSON wrapping.

```
ls -la\n
```

### Connection Lifecycle

#### Active Connection

When a WebSocket connection is established:

1. **Terminal status** transitions to `Active`
2. **Buffered output** from any previous disconnection is replayed to restore state
3. **Three concurrent tasks** are spawned:
   - Reader: Reads PTY output and sends to WebSocket (5-minute idle timeout)
   - Writer: Reads WebSocket messages and writes to PTY stdin (5-minute idle timeout)
   - Heartbeat: Sends Ping frames every 30 seconds

**Active Connection Flow:**

```
Client                     WebSocket Handler              PTY Process
  │                              │                             │
  │──── Connect WS ─────────────►│                             │
  │                              │──── Read PTY output ───────►│
  │◄──── Replay buffer ──────────│                             │
  │◄──── Terminal output ────────│◄──── stdout/stderr ─────────│
  │                              │                             │
  │──── {"type":"input"} ────────►│                             │
  │                              │──── Write to stdin ─────────►│
  │                              │                             │
  │──── Ping ────────────────────►│                             │
  │◄──── Pong ───────────────────│                             │
```

#### Disconnection & Reconnection Grace Period

When the WebSocket disconnects (network failure, tab close, browser refresh):

1. **Terminal status** transitions to `Disconnected` with timestamp
2. **PTY process** continues running in the background (not killed)
3. **Output is buffered** (last 64KB) for **30 seconds** (reconnection grace period)
4. **If client reconnects within grace period**:
   - Buffered output is replayed to restore terminal state
   - Session resumes transparently without data loss
5. **If grace period expires** without reconnection:
   - PTY process is killed (SIGTERM)
   - Terminal status transitions to `Dead`
   - Subsequent reconnect attempts receive `410 Gone`

**Disconnection Flow:**

```
┌─────────────────────────────────────────────────────────────┐
│                  Disconnection Timeline                      │
└─────────────────────────────────────────────────────────────┘

t=0s                t=30s                          t=...
│                   │                              │
│ WS Disconnect     │ Grace Period Expires         │
│ Status→Disconnected│ PTY Killed                   │
│ Start buffering   │ Status→Dead                  │
│                   │                              │
│◄─────────────────►│                              │
│  30-second grace  │                              │
│                   │                              │
│ If reconnect:     │ If no reconnect:             │
│ - Replay buffer   │ - Kill PTY                   │
│ - Resume session  │ - 410 Gone                   │
```

**Grace Period Benefits:**

This 30-second grace period ensures that brief network interruptions, page reloads, or browser tab switches don't terminate long-running terminal sessions (e.g., compilation, test runs, large file transfers).

#### Disconnect Buffer

The disconnect buffer captures the last **64KB** of PTY output during disconnection:

- **Ring buffer**: Old data is evicted when limit is reached
- **Replay on reconnect**: Buffer contents are sent to client when WebSocket reconnects
- **Memory bounded**: Prevents unbounded memory growth during prolonged disconnections

### Timeouts

| Timeout | Duration | Purpose |
|---------|----------|---------|
| **Idle Timeout** | 5 minutes | WebSocket closes if no data flows in either direction |
| **Heartbeat Interval** | 30 seconds | Ping frames detect half-open connections |
| **Reconnect Grace** | 30 seconds | Buffer output after disconnect before killing PTY |

**Idle Timeout Behavior:**

If no data is sent or received on the WebSocket for 5 minutes, the connection is automatically closed. This prevents resource leaks from abandoned connections. Note that:
- Ping/Pong frames count as activity
- Terminal output resets the idle timer
- User input resets the idle timer

## REST API Endpoints

### Terminal Lifecycle

#### Create Terminal

```http
POST /api/terminals
Content-Type: application/json

{
  "agent_id": "uuid",
  "title": "My Terminal",
  "cols": 80,
  "rows": 24,
  "font_size": 14,
  "cursor_style": "block",
  "cursor_blink": true
}
```

**Response:**

```json
{
  "id": "terminal-uuid",
  "title": "My Terminal",
  "status": "idle",
  "cols": 80,
  "rows": 24,
  "font_size": 14,
  "font_family": "monospace",
  "line_height": 1.2,
  "letter_spacing": 0.0,
  "profile": "bundled-card",
  "cursor_style": "block",
  "cursor_blink": true,
  "auto_name": null,
  "persistent": false
}
```

#### List Terminals

```http
GET /api/terminals
```

**Response:**

```json
[
  {
    "id": "terminal-uuid-1",
    "title": "Terminal 1",
    "status": "active",
    ...
  },
  {
    "id": "terminal-uuid-2",
    "title": "Terminal 2",
    "status": "disconnected",
    ...
  }
]
```

#### Delete Terminal

```http
DELETE /api/terminals/{id}
```

Kills the PTY process and removes the terminal from the registry.

**Response:** `204 No Content`

### Terminal Management

#### Rename Terminal

```http
POST /api/terminals/{id}/rename
Content-Type: application/json

{
  "name": "Build Terminal"
}
```

**Response:**

```json
{
  "id": "terminal-uuid",
  "title": "Build Terminal",
  ...
}
```

#### Auto-Name Terminal

```http
POST /api/terminals/{id}/auto-name
Content-Type: application/json

{
  "name": "npm run build"
}
```

Sets the auto-generated name from the first command executed in the terminal.

**Response:**

```json
{
  "id": "terminal-uuid",
  "auto_name": "npm run build",
  ...
}
```

#### Update Terminal Settings

```http
PATCH /api/terminals/{id}/settings
Content-Type: application/json

{
  "font_size": 16,
  "cursor_style": "underline",
  "persistent": true
}
```

**Response:**

```json
{
  "id": "terminal-uuid",
  "font_size": 16,
  "cursor_style": "underline",
  "persistent": true,
  ...
}
```

#### List Persistent Terminals

```http
GET /api/terminals/persistent
```

Returns only terminals marked as persistent (survive server restart).

**Response:**

```json
[
  {
    "id": "terminal-uuid",
    "persistent": true,
    ...
  }
]
```

### WebSocket Endpoint

#### Connect to Terminal

```http
GET /ws/terminal/{id}
Upgrade: websocket
```

Upgrades the HTTP connection to a WebSocket for bidirectional terminal I/O.

**WebSocket Messages:**

See [Terminal WebSocket Protocol](#terminal-websocket-protocol) section above.

## Terminal Status States

Terminals transition through the following lifecycle states:

| Status | Description | Transitions To |
|--------|-------------|----------------|
| **Idle** | Terminal created but no WebSocket connected | Active (on WS connect) |
| **Active** | WebSocket connected, actively processing I/O | Disconnected (on WS close), Closed (on explicit close) |
| **Disconnected** | WebSocket closed, PTY running, buffering output | Active (on reconnect), Dead (grace period expires) |
| **Closed** | User explicitly closed terminal | Dead (immediately) |
| **Dead** | PTY killed, terminal unrecoverable | N/A (terminal removed) |

**State Transition Diagram:**

```
┌──────┐
│ Idle │
└──┬───┘
   │ WS connect
   ▼
┌────────┐
│ Active │◄────────────┐
└───┬────┘             │
    │                  │ Reconnect
    │ WS disconnect    │ (within 30s)
    ▼                  │
┌──────────────┐       │
│ Disconnected │───────┘
└──────┬───────┘
       │ Grace period expires (30s)
       │ OR explicit close
       ▼
┌──────┐
│ Dead │
└──────┘
```

## Origin Validation

The WebSocket endpoint validates the `Origin` header to prevent unauthorized cross-origin connections.

**Default Allowed Origins:**

- `http://localhost:*`
- `https://localhost:*`
- `http://127.0.0.1:*`
- `https://127.0.0.1:*`
- `tauri://localhost`

WebSocket upgrade requests from other origins are rejected with `403 Forbidden`.

## Usage Examples

### Creating a Terminal and Connecting via WebSocket

```rust
use reqwest;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    // 1. Create terminal via REST API
    let response = client
        .post("http://localhost:3000/api/terminals")
        .json(&serde_json::json!({
            "agent_id": "00000000-0000-0000-0000-000000000000",
            "title": "Example Terminal",
            "cols": 80,
            "rows": 24
        }))
        .send()
        .await?;

    let terminal: serde_json::Value = response.json().await?;
    let terminal_id = terminal["id"].as_str().unwrap();
    println!("Created terminal: {}", terminal_id);

    // 2. Connect to WebSocket
    let ws_url = format!("ws://localhost:3000/ws/terminal/{}", terminal_id);
    let (mut ws_stream, _) = connect_async(ws_url).await?;
    println!("Connected to WebSocket");

    // 3. Send input (JSON format)
    let input_msg = serde_json::json!({
        "type": "input",
        "data": "echo 'Hello from WebSocket'\n"
    });
    ws_stream.send(Message::Text(input_msg.to_string())).await?;

    // 4. Receive output
    while let Some(msg) = ws_stream.next().await {
        match msg? {
            Message::Text(text) => {
                println!("Terminal output: {}", text);
            }
            Message::Ping(_) => {
                // Pong is sent automatically
            }
            Message::Close(_) => {
                println!("WebSocket closed");
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
```

### Sending Raw Input (Plain Text)

```rust
// Instead of JSON, send raw text directly
ws_stream.send(Message::Text("ls -la\n".to_string())).await?;
```

Any text message that doesn't parse as JSON is treated as raw input.

### Resizing Terminal

```rust
// Send resize command
let resize_msg = serde_json::json!({
    "type": "resize",
    "cols": 120,
    "rows": 30
});
ws_stream.send(Message::Text(resize_msg.to_string())).await?;
```

### Handling Reconnection

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn connect_with_retry(terminal_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let ws_url = format!("ws://localhost:3000/ws/terminal/{}", terminal_id);

    loop {
        match connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                println!("Connected!");
                // Use ws_stream...
                break;
            }
            Err(e) => {
                eprintln!("Connection failed: {}, retrying in 1s...", e);
                sleep(Duration::from_secs(1)).await;
            }
        }
    }

    Ok(())
}
```

If the WebSocket disconnects and you reconnect within the 30-second grace period, the terminal session resumes with buffered output replayed.

### Listing Active Terminals

```rust
use reqwest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let response = client
        .get("http://localhost:3000/api/terminals")
        .send()
        .await?;

    let terminals: Vec<serde_json::Value> = response.json().await?;

    for terminal in terminals {
        println!(
            "Terminal {}: {} (status: {})",
            terminal["id"].as_str().unwrap(),
            terminal["title"].as_str().unwrap(),
            terminal["status"].as_str().unwrap()
        );
    }

    Ok(())
}
```

### Cleaning Up Terminals

```rust
use reqwest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let terminal_id = "terminal-uuid";

    // Delete terminal (kills PTY and removes from registry)
    client
        .delete(&format!("http://localhost:3000/api/terminals/{}", terminal_id))
        .send()
        .await?;

    println!("Terminal deleted");

    Ok(())
}
```

## Event Bus

The event bus provides a pub/sub system for real-time notifications across the system.

**Key Features:**
- Topic-based subscriptions
- Async event handlers
- Thread-safe shared state
- Automatic cleanup of closed subscribers

**Common Event Topics:**
- `terminal.output` — Terminal output events
- `terminal.resize` — Terminal resize events
- `terminal.close` — Terminal close events
- `agent.status` — Agent status updates
- `task.progress` — Task progress notifications

See `event_bus` module documentation for detailed API.

## Authentication

The `auth` module provides API key authentication middleware for securing HTTP endpoints.

**Key Features:**
- Header-based authentication (`Authorization: Bearer <token>`)
- Configurable key validation
- Per-route opt-in/opt-out
- 401 Unauthorized for invalid/missing keys

See `auth` module documentation for integration details.

## IPC Protocol

The `ipc` module provides inter-process communication for command execution and control.

**Key Features:**
- Command registry for typed commands
- Request/response pattern
- Async command handlers
- Serialization via serde_json

See `ipc` module documentation for command registration and dispatch.

## Implementation Details

### WebSocket Task Lifecycle

When a client connects to `GET /ws/terminal/{id}`, the handler spawns three concurrent tasks:

**Reader Task:**
```rust
loop {
    select! {
        // Read from PTY
        msg = pty_handle.reader.recv_async() => {
            if let Ok(data) = msg {
                ws.send(Message::Text(String::from_utf8_lossy(&data))).await?;
                last_activity = Instant::now();
            }
        }
        // Idle timeout
        _ = sleep_until(last_activity + IDLE_TIMEOUT) => {
            break;  // Close connection
        }
    }
}
```

**Writer Task:**
```rust
loop {
    select! {
        // Read from WebSocket
        msg = ws.recv() => {
            match msg {
                Message::Text(text) => {
                    // Try parse as JSON command, fallback to raw input
                    if let Ok(cmd) = serde_json::from_str::<WsIncoming>(&text) {
                        handle_command(cmd).await?;
                    } else {
                        pty_handle.send(text.as_bytes())?;
                    }
                    last_activity = Instant::now();
                }
                _ => {}
            }
        }
        // Idle timeout
        _ = sleep_until(last_activity + IDLE_TIMEOUT) => {
            break;  // Close connection
        }
    }
}
```

**Heartbeat Task:**
```rust
loop {
    sleep(HEARTBEAT_INTERVAL).await;
    if ws.send(Message::Ping(vec![])).await.is_err() {
        break;  // Connection closed
    }
}
```

### Disconnect Buffer Implementation

The disconnect buffer uses a ring buffer (VecDeque) to capture PTY output:

```rust
pub struct DisconnectBuffer {
    buffer: VecDeque<u8>,
    max_size: usize,  // 64KB
}

impl DisconnectBuffer {
    pub fn push(&mut self, data: &[u8]) {
        for &byte in data {
            if self.buffer.len() >= self.max_size {
                self.buffer.pop_front();  // Evict oldest
            }
            self.buffer.push_back(byte);
        }
    }

    pub fn drain(&mut self) -> Vec<u8> {
        self.buffer.drain(..).collect()
    }
}
```

When the WebSocket disconnects:
1. Allocate a `DisconnectBuffer` for the terminal
2. Continue reading PTY output and push to buffer
3. On reconnect, drain buffer and send to WebSocket
4. If grace period expires, kill PTY and drop buffer

### Thread Safety

All types are thread-safe and can be shared across async tasks:

- **TerminalRegistry**: Uses `Arc<RwLock<HashMap>>` for shared state
  - Read-write lock allows concurrent reads
  - Safe to clone and share across threads

- **ApiState**: Wraps all shared state in Arc
  - Immutable after initialization
  - Can be cloned and sent across threads

- **Event Bus**: Uses `Arc<Mutex<HashMap>>` for subscriptions
  - Interior mutability for pub/sub
  - Safe concurrent access

## Performance Characteristics

| Operation | Time Complexity | Notes |
|-----------|-----------------|-------|
| `create_terminal` | O(1) + PTY spawn | HashMap insert, PTY spawn |
| `list_terminals` | O(n) | Iterate all terminals |
| `delete_terminal` | O(1) + SIGTERM | HashMap remove, kill PTY |
| WebSocket send | O(1) | Channel send (may block if full) |
| WebSocket recv | O(1) | Channel receive (blocking) |
| Buffer replay | O(n) | n = buffer size (max 64KB) |

**Memory Usage:**
- Each terminal: ~64KB disconnect buffer (when disconnected)
- WebSocket connections: ~8KB per channel
- Registry overhead: O(n) where n = number of terminals

## Testing

The crate includes comprehensive integration tests:

```bash
cargo test -p at-bridge
```

Key test scenarios:
- Terminal lifecycle (create, list, delete)
- WebSocket connection/disconnection
- Reconnection within grace period
- Grace period expiration
- Message formats (JSON and plain text)
- Origin validation
- Authentication middleware
- Event bus pub/sub

## Dependencies

- `axum`: Web framework for HTTP/WebSocket server
- `tower`: Middleware and service abstractions
- `tower-http`: HTTP middleware (CORS, tracing)
- `tokio`: Async runtime
- `serde`/`serde_json`: Serialization
- `uuid`: Unique identifiers
- `futures-util`: Stream/Sink utilities for WebSocket
- `flume`: MPMC channels for event bus
- `at-session`: PTY pool and session management
- `at-core`: Core types and utilities

## Platform Support

The crate works on all platforms supported by `at-session`:
- **Linux**: Native PTY support
- **macOS**: Native PTY support
- **Windows**: ConPTY (Windows 10+)

## License

See workspace LICENSE file.
