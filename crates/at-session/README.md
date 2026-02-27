# at-session

Terminal session management and PTY pooling for auto-tundra agents.

This crate provides persistent terminal sessions with PTY (pseudo-terminal) pooling, CLI adaptation, and state persistence. It enables agents to maintain long-running shell environments across task executions, preserving working directories, environment variables, and command history.

## Architecture Overview

The crate is built around three core components:

1. **PTY Pool** (`pty_pool`): Manages concurrent pseudo-terminal sessions with capacity limits
2. **CLI Adapters** (`cli_adapter`): Trait-based pattern for spawning different AI coding CLIs
3. **Session Management** (`session`): Persistent terminal sessions with state tracking

### PTY Pool Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                          PtyPool                             │
│  - max_ptys: capacity limit                                  │
│  - handles: HashMap<Uuid, PtyHandle>                         │
│  - Thread-safe: Arc<Mutex<_>>                                │
└──────────────────┬──────────────────────────────────────────┘
                   │ spawns
                   ▼
      ┌────────────────────────────┐
      │       PtyHandle            │
      │  - id: Uuid                │
      │  - reader: Receiver<Vec>   │◄──── Background Reader Thread
      │  - writer: Sender<Vec>     │────► Background Writer Thread
      │  - child: ChildHandle      │
      │  - master: PtyMaster       │
      └────────────────────────────┘
                   │
                   │ manages
                   ▼
         ┌─────────────────────┐
         │  Child Process      │
         │  (bash, claude, etc)│
         └─────────────────────┘
```

Each PTY spawns **2 background threads**:
- **Reader thread**: Reads from PTY master → sends to `reader` channel
- **Writer thread**: Receives from `writer` channel → writes to PTY master

## Core Components

### PtyPool

Manages up to `max_ptys` concurrent PTY sessions with strict capacity enforcement.

**Key Features:**
- Capacity-limited pool (prevents resource exhaustion)
- Thread-safe shared state (Arc<Mutex>)
- Poisoned-lock recovery
- Automatic cleanup on drop

**Key Methods:**
- `new(max_ptys)`: Create a pool with capacity limit
- `spawn(cmd, args, env)`: Spawn a process in a PTY (returns `PtyHandle`)
- `release(id)`: Free a PTY slot in the pool
- `kill(id)`: Terminate a process and release its slot
- `active_count()`: Get number of active PTYs
- `available()`: Get number of available slots

### PtyHandle

A handle to a single PTY session with async read/write channels.

**Key Fields:**
- `id: Uuid`: Unique identifier for the PTY
- `reader: Receiver<Vec<u8>>`: Channel for receiving stdout/stderr (merged in PTY mode)
- `writer: Sender<Vec<u8>>`: Channel for sending stdin
- `child: Arc<Mutex<Box<dyn Child>>>`: Handle to the child process
- `master: Arc<Mutex<Box<dyn MasterPty>>>`: PTY master for resizing

**Key Methods:**
- `send(data)`: Send bytes to stdin (via writer channel)
- `send_line(line)`: Send a line with newline appended
- `read_timeout(duration)`: Async read with timeout
- `read_all_available()`: Drain all buffered output
- `is_alive()`: Check if child process is running
- `kill()`: Terminate the child process
- `resize(cols, rows)`: Resize the PTY terminal

### Channel Protocol

The PTY handle uses **flume channels** for non-blocking, buffered I/O:

```
┌──────────────────────────────────────────────────────────┐
│                    Channel Flow                           │
└──────────────────────────────────────────────────────────┘

User Code                Reader Thread              PTY Master
    │                         │                          │
    │◄────────────────────────┤ recv()                   │
    │  reader.recv()          │                          │
    │                         │◄─────────────────────────┤
    │                         │  read from master        │
    │                         │──────────────────────────►│
    │                         │  send to channel         │


User Code                Writer Thread              PTY Master
    │                         │                          │
    │─────────────────────────►│ recv()                  │
    │  writer.send()          │                          │
    │                         │──────────────────────────►│
    │                         │  write to master         │
```

**Channel Characteristics:**
- **Bounded capacity**: 256 messages per channel
- **Backpressure**: Sends block when channel is full
- **Thread-safe**: `Clone + Send + Sync`
- **Non-blocking reads**: Use `try_recv()` or async `read_timeout()`

### PTY Lifecycle

#### 1. Spawn Phase
```rust
let pool = PtyPool::new(10);
let handle = pool.spawn("bash", &[], &[])?;
```
**Actions:**
- Creates PTY master/slave pair
- Spawns child process in PTY
- Starts reader thread (PTY → channel)
- Starts writer thread (channel → PTY)
- Registers handle in pool's HashMap

#### 2. I/O Phase
```rust
// Write to stdin
handle.send_line("echo hello")?;

// Read from stdout/stderr (merged)
if let Some(output) = handle.read_timeout(Duration::from_secs(1)).await {
    println!("Output: {}", String::from_utf8_lossy(&output));
}
```

#### 3. Cleanup Phase
```rust
// Terminate process
handle.kill()?;

// Free slot in pool
pool.release(handle.id);

// Drop handle (joins threads)
drop(handle);
```

**Cleanup Order:**
1. `kill()`: Sends SIGTERM to child, drains output
2. `release()`: Removes from pool's HashMap
3. `drop()`: Joins background threads, closes channels

### PtyError Types

Comprehensive error handling for all PTY operations:

| Error Variant | Cause | Recovery |
|---------------|-------|----------|
| `AtCapacity { max }` | Pool is full (`active_count >= max_ptys`) | Wait for release or increase capacity |
| `HandleNotFound(Uuid)` | PTY not found in pool | Check UUID or already released |
| `SpawnFailed(String)` | Failed to create PTY or spawn process | Check binary path, permissions, resources |
| `Io(io::Error)` | I/O error during read/write | Check process state, retry |
| `Internal(String)` | Lock poisoning, channel closed | Unexpected state, may need restart |

### CLI Adapters

Trait-based pattern for spawning AI coding assistant CLIs in PTY sessions.

#### CliAdapter Trait

Abstracts CLI-specific conventions:

```rust
#[async_trait]
pub trait CliAdapter: Send + Sync {
    fn cli_type(&self) -> CliType;
    fn binary_name(&self) -> &str;
    fn default_args(&self) -> Vec<String>;
    async fn spawn(&self, pool: &PtyPool, task: &str, workdir: &str) -> Result<PtyHandle>;
    fn parse_status_output(&self, output: &str) -> Option<String>;
}
```

#### Supported CLIs

| CLI | Binary | Default Args | Prompt Flag |
|-----|--------|--------------|-------------|
| **Claude** | `claude` | `--dangerously-skip-permissions` | `-p` |
| **Codex** | `codex` | `--approval-mode full-auto -q` | `-p` |
| **Gemini** | `gemini` | (none) | `-p` |
| **OpenCode** | `opencode` | (none) | (direct arg) |

#### Adapter Responsibilities

Each adapter implementation handles:

1. **Binary name**: Command to execute (must be in PATH)
2. **Default arguments**: CLI flags for non-interactive mode
3. **Spawn logic**: Construct full command with task + workdir
4. **Status parsing**: Extract completion/error status from output

#### Factory Function

```rust
pub fn adapter_for(cli_type: &CliType) -> Box<dyn CliAdapter>
```

Returns the appropriate adapter for a CLI type.

## Thread Safety

All types are thread-safe and can be shared across async tasks:

- **PtyPool**: Uses `Arc<Mutex<HashMap>>` for shared state
  - Poisoned-lock recovery via `unwrap_or_else()`
  - Safe to clone and share across threads

- **PtyHandle**: All fields are thread-safe
  - Channels are `Clone + Send + Sync`
  - Child/Master wrapped in `Arc<Mutex<_>>`
  - Can be cloned and sent across threads

- **CliAdapter**: Required to be `Send + Sync`
  - All implementations are stateless
  - Safe to use from any thread

## Usage Examples

### Basic PTY Spawn

```rust
use at_session::pty_pool::PtyPool;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PtyPool::new(10);

    // Spawn a bash shell
    let handle = pool.spawn("bash", &[], &[])?;

    // Send commands
    handle.send_line("ls -la")?;
    handle.send_line("pwd")?;

    // Read output with timeout
    tokio::time::sleep(Duration::from_millis(100)).await;
    if let Some(output) = handle.read_timeout(Duration::from_secs(2)).await {
        println!("Output:\n{}", String::from_utf8_lossy(&output));
    }

    // Cleanup
    handle.kill()?;
    pool.release(handle.id);

    Ok(())
}
```

### Using CLI Adapters

```rust
use at_session::cli_adapter::{adapter_for, CliAdapter};
use at_session::pty_pool::PtyPool;
use at_core::types::CliType;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PtyPool::new(5);
    let adapter = adapter_for(&CliType::Claude);

    // Spawn Claude with a task
    let handle = adapter.spawn(
        &pool,
        "Add error handling to the main function",
        "/path/to/project"
    ).await?;

    // Monitor output
    loop {
        if let Some(output) = handle.read_timeout(Duration::from_secs(1)).await {
            let output_str = String::from_utf8_lossy(&output);
            println!("{}", output_str);

            // Check for completion
            if let Some(status) = adapter.parse_status_output(&output_str) {
                println!("Task status: {}", status);
                if status == "completed" || status == "error" {
                    break;
                }
            }
        }

        if !handle.is_alive() {
            break;
        }
    }

    // Cleanup
    pool.kill(handle.id)?;

    Ok(())
}
```

### Capacity Management

```rust
use at_session::pty_pool::{PtyPool, PtyError};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PtyPool::new(2);  // Small capacity for demo

    // Spawn first two PTYs
    let h1 = pool.spawn("bash", &[], &[])?;
    let h2 = pool.spawn("bash", &[], &[])?;

    println!("Active: {}/{}", pool.active_count(), 2);

    // Third spawn fails
    match pool.spawn("bash", &[], &[]) {
        Err(PtyError::AtCapacity { max }) => {
            println!("Pool full! Max capacity: {}", max);
        }
        _ => unreachable!(),
    }

    // Release one slot
    pool.release(h1.id);

    // Now we can spawn again
    let h3 = pool.spawn("bash", &[], &[])?;
    println!("Spawned after release: {}", h3.id);

    // Cleanup
    pool.kill(h2.id)?;
    pool.kill(h3.id)?;

    Ok(())
}
```

### Resizing PTY

```rust
use at_session::pty_pool::PtyPool;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PtyPool::new(5);
    let handle = pool.spawn("vim", &[], &[])?;

    // Resize to 120x40 (common terminal size)
    handle.resize(120, 40)?;

    // Now vim sees a 120-column, 40-row terminal
    // ...

    handle.kill()?;
    pool.release(handle.id);

    Ok(())
}
```

### Error Handling

```rust
use at_session::pty_pool::{PtyPool, PtyError};

async fn spawn_with_retry(pool: &PtyPool) -> Result<(), Box<dyn std::error::Error>> {
    match pool.spawn("my-cli", &[], &[]) {
        Ok(handle) => {
            println!("Spawned: {}", handle.id);
            Ok(())
        }
        Err(PtyError::AtCapacity { max }) => {
            eprintln!("Pool at capacity ({}), waiting...", max);
            // Wait and retry
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            spawn_with_retry(pool).await
        }
        Err(PtyError::SpawnFailed(msg)) => {
            eprintln!("Spawn failed: {}", msg);
            // Check binary path, permissions, etc.
            Err(msg.into())
        }
        Err(e) => Err(e.into()),
    }
}
```

## Implementation Details

### Background Thread Behavior

**Reader Thread:**
```rust
loop {
    let mut buf = [0u8; 8192];
    match master.read(&mut buf) {
        Ok(0) => break,  // EOF
        Ok(n) => {
            let _ = reader_tx.send(buf[..n].to_vec());
        }
        Err(e) if would_block(&e) => continue,
        Err(_) => break,
    }
}
```
- Reads in 8KB chunks
- Sends to channel (non-blocking)
- Stops on EOF or error

**Writer Thread:**
```rust
loop {
    match writer_rx.recv() {
        Ok(data) => {
            let _ = master.write_all(&data);
        }
        Err(_) => break,  // Channel closed
    }
}
```
- Blocks on channel receive
- Writes to PTY master
- Stops when channel closes (handle dropped)

### Poisoned Lock Recovery

The pool uses `unwrap_or_else()` to recover from poisoned locks:

```rust
let guard = self.handles.lock().unwrap_or_else(|e| {
    warn!("PtyPool lock poisoned, recovering");
    e.into_inner()
});
```

This ensures that a panic in one thread doesn't permanently break the pool.

### Channel Backpressure

Channels have a capacity of 256 messages. When full:
- **Reader thread**: Blocks on send (applies backpressure to PTY)
- **User code**: Blocks on `writer.send()` (applies backpressure to caller)

This prevents unbounded memory growth when output is produced faster than it's consumed.

## Performance Characteristics

| Operation | Time Complexity | Notes |
|-----------|-----------------|-------|
| `spawn()` | O(1) + process spawn | HashMap insert, thread spawn |
| `release()` | O(1) | HashMap remove |
| `kill()` | O(1) + SIGTERM | Process signal, HashMap remove |
| `active_count()` | O(1) | HashMap len() |
| `send()` | O(1) | Channel send (may block if full) |
| `read_timeout()` | O(n) | n = number of messages in channel |

**Memory Usage:**
- Each PTY: ~8KB per channel (256 × 32 bytes avg per message)
- Pool overhead: O(n) where n = number of active PTYs

## Testing

The crate includes comprehensive integration tests:

```bash
cargo test -p at-session
```

Key test scenarios:
- Basic spawn/kill lifecycle
- Capacity limits and AtCapacity errors
- Channel I/O (send/receive)
- CLI adapter spawning
- Concurrent access (thread safety)
- Error handling and recovery

## Dependencies

- `portable-pty`: Cross-platform PTY implementation
- `flume`: Fast MPMC channels for async I/O
- `tokio`: Async runtime for timeout/sleep
- `uuid`: Unique PTY identifiers
- `tracing`: Structured logging
- `thiserror`: Error type derivation
- `async-trait`: Async trait methods

## Platform Support

The crate works on all platforms supported by `portable-pty`:
- **Linux**: Native PTY support via `/dev/ptmx`
- **macOS**: Native PTY support via `/dev/ptmx`
- **Windows**: ConPTY (Windows 10+) or WinPTY fallback

## License

See workspace LICENSE file.
