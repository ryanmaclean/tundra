//! PTY pool management with capacity limits and async I/O channels.
//!
//! This module provides a pool-based architecture for managing pseudo-terminal
//! (PTY) sessions. Each spawned process runs in its own PTY with dedicated
//! background threads for stdin/stdout, and the pool enforces a configurable
//! capacity limit to prevent resource exhaustion.
//!
//! ## Architecture
//!
//! - **[`PtyPool`]**: Manages up to `max_ptys` concurrent PTY sessions
//! - **[`PtyHandle`]**: Handle to a single PTY with async read/write channels
//! - **Background threads**: Each PTY spawns 2 threads (reader, writer)
//! - **Channel-based I/O**: Uses `flume` for buffered, non-blocking communication
//!
//! ## PTY Lifecycle
//!
//! 1. **Spawn**: [`PtyPool::spawn()`] creates a PTY, spawns the child process,
//!    and starts reader/writer threads. Returns a [`PtyHandle`].
//! 2. **I/O**: Use `handle.send()` to write stdin, `handle.reader.recv()` to
//!    read stdout/stderr (merged in PTY mode).
//! 3. **Cleanup**: Call [`PtyHandle::kill()`] to terminate the process, then
//!    [`PtyPool::release()`] to free the slot in the pool.
//!
//! ## Capacity Management
//!
//! The pool enforces a strict capacity limit. If [`PtyPool::spawn()`] is called
//! when `active_count() >= max_ptys`, it returns [`PtyError::AtCapacity`].
//! Clients must release PTYs explicitly via [`PtyPool::release()`] or
//! [`PtyPool::kill()`].
//!
//! ## Thread Safety
//!
//! All types are thread-safe. The pool uses `Arc<Mutex<_>>` for shared state,
//! with poisoned-lock recovery via `unwrap_or_else()`. PTY handles can be
//! cloned and sent across threads (channels are `Clone + Send`).
//!
//! ## Example
//!
//! ```no_run
//! use at_session::pty_pool::PtyPool;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let pool = PtyPool::new(10);
//!
//! // Spawn a shell process
//! let handle = pool.spawn("bash", &[], &[])?;
//!
//! // Send a command
//! handle.send_line("echo hello")?;
//!
//! // Read output (async)
//! if let Some(output) = handle.read_timeout(std::time::Duration::from_secs(1)).await {
//!     println!("Got: {:?}", String::from_utf8_lossy(&output));
//! }
//!
//! // Cleanup
//! handle.kill()?;
//! pool.release(handle.id);
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::io::{Read as IoRead, Write as IoWrite};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during PTY operations.
#[derive(Debug, Error)]
pub enum PtyError {
    /// The pool has reached its maximum capacity and cannot spawn more PTYs.
    ///
    /// Returned by [`PtyPool::spawn()`] when `active_count() >= max_ptys`.
    /// The caller should either wait for existing PTYs to be released or
    /// increase the pool capacity.
    #[error("pty pool is at capacity ({max})")]
    AtCapacity {
        /// The maximum capacity that was reached
        max: usize
    },

    /// A PTY handle with the given UUID was not found in the pool.
    ///
    /// Returned by [`PtyPool::kill()`] when attempting to kill a handle that
    /// was never registered or was already released.
    #[error("pty handle not found: {0}")]
    HandleNotFound(Uuid),

    /// Failed to spawn a PTY or child process.
    ///
    /// This can occur if:
    /// - The PTY system failed to allocate a pseudo-terminal
    /// - The command binary was not found or not executable
    /// - Insufficient system resources (file descriptors, memory)
    #[error("pty spawn failed: {0}")]
    SpawnFailed(String),

    /// I/O error during PTY read/write operations.
    ///
    /// Wraps standard I/O errors from the underlying PTY system or channels.
    #[error("pty I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Internal PTY system error (lock poisoning, channel closed, etc.).
    ///
    /// Indicates an unexpected internal state. This should not occur during
    /// normal operation but may happen if background threads panic or if
    /// the PTY master is closed unexpectedly.
    #[error("pty internal error: {0}")]
    Internal(String),
}

/// Convenience result type for PTY operations.
pub type Result<T> = std::result::Result<T, PtyError>;

// ---------------------------------------------------------------------------
// PtyHandle
// ---------------------------------------------------------------------------

/// A handle to a single PTY session with async read/write channels.
///
/// Each `PtyHandle` represents a running process inside a pseudo-terminal.
/// The handle provides:
/// - **Async I/O**: Non-blocking channels for reading output and writing input
/// - **Process control**: Methods to check liveness, kill, and resize the PTY
/// - **Thread management**: Two background threads (reader, writer) that run
///   until the process exits or the handle is dropped
///
/// ## Channel Protocol
///
/// - **`reader`**: Receives chunks of stdout/stderr (merged in PTY mode).
///   Background thread reads from PTY master and sends to this channel.
///   Channel is bounded (256 messages) to apply backpressure.
/// - **`writer`**: Sends chunks of stdin to the PTY. Background thread receives
///   from this channel and writes to PTY master. Sending blocks if the channel
///   is full (backpressure).
///
/// ## Lifetime
///
/// The PTY remains active as long as:
/// 1. The child process is running, OR
/// 2. The reader channel still has buffered data
///
/// When the child exits, the reader thread drains remaining output and then
/// stops. The writer thread stops when the writer channel is closed (dropped).
///
/// ## Cleanup
///
/// To properly clean up a PTY:
/// 1. Call [`kill()`](PtyHandle::kill) to terminate the child process
/// 2. Drain the reader channel to consume remaining output
/// 3. Call [`PtyPool::release()`] to remove the handle from the pool
/// 4. Drop the handle to release resources and join threads
///
/// ## Example
///
/// ```no_run
/// # use at_session::pty_pool::{PtyPool, PtyHandle};
/// # async fn example(handle: PtyHandle) -> Result<(), Box<dyn std::error::Error>> {
/// // Check if process is still running
/// if handle.is_alive() {
///     // Send input
///     handle.send_line("ls -la")?;
///
///     // Read output with timeout
///     if let Some(output) = handle.read_timeout(std::time::Duration::from_secs(2)).await {
///         println!("Output: {}", String::from_utf8_lossy(&output));
///     }
///
///     // Resize terminal
///     handle.resize(120, 40)?;
/// }
/// # Ok(())
/// # }
/// ```
pub struct PtyHandle {
    /// Unique identifier for this PTY session.
    pub id: Uuid,

    /// Channel for receiving output from the PTY (stdout/stderr merged).
    ///
    /// The background reader thread sends chunks of data here as they arrive.
    /// This is a bounded channel (256 messages) to prevent unbounded buffering.
    pub reader: flume::Receiver<Vec<u8>>,

    /// Channel for sending input to the PTY (stdin).
    ///
    /// The background writer thread receives from this channel and writes to
    /// the PTY master. Sends block if the channel is full (backpressure).
    pub writer: flume::Sender<Vec<u8>>,

    /// Handle to the child process for lifecycle management.
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,

    /// Handle to the PTY master for resize operations.
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,

    /// Background thread that reads from PTY master and sends to `reader` channel.
    _reader_thread: Option<std::thread::JoinHandle<()>>,

    /// Background thread that receives from `writer` channel and writes to PTY master.
    _writer_thread: Option<std::thread::JoinHandle<()>>,
}

impl PtyHandle {
    /// Check whether the underlying child process is still running.
    ///
    /// Returns `true` if the process has not exited, `false` if it has exited
    /// or if the status check failed. This is a non-blocking call that uses
    /// `try_wait()` to poll the process status.
    ///
    /// Note: This does not check if output is still buffered in the reader
    /// channel. A process may have exited but output may still be available
    /// for reading.
    pub fn is_alive(&self) -> bool {
        let mut child = self.child.lock().unwrap_or_else(|e| {
            warn!("child lock was poisoned, recovering");
            e.into_inner()
        });
        match child.try_wait() {
            Ok(Some(_status)) => false,
            Ok(None) => true,
            Err(_) => false,
        }
    }

    /// Kill the child process immediately.
    ///
    /// Sends `SIGKILL` (or platform equivalent) to terminate the process
    /// without allowing graceful shutdown. After calling this, [`is_alive()`]
    /// will return `false` (after a brief delay).
    ///
    /// The reader channel may still contain buffered output after the process
    /// is killed. Call [`try_read_all()`] to drain remaining data.
    ///
    /// # Errors
    ///
    /// Returns [`PtyError::Internal`] if the kill operation fails (rare).
    ///
    /// [`is_alive()`]: PtyHandle::is_alive
    /// [`try_read_all()`]: PtyHandle::try_read_all
    pub fn kill(&self) -> Result<()> {
        let mut child = self.child.lock().unwrap_or_else(|e| {
            warn!("child lock was poisoned, recovering");
            e.into_inner()
        });
        child
            .kill()
            .map_err(|e| PtyError::Internal(e.to_string()))?;
        Ok(())
    }

    /// Read all currently available output without blocking.
    ///
    /// Drains the reader channel using `try_recv()` in a loop until no more
    /// data is available. Returns immediately with whatever data was buffered.
    ///
    /// This is useful for:
    /// - Collecting all output after a process exits
    /// - Polling for output without blocking
    /// - Draining the channel before calling [`kill()`](PtyHandle::kill)
    ///
    /// Returns an empty `Vec<u8>` if no data is currently available.
    pub fn try_read_all(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        while let Ok(chunk) = self.reader.try_recv() {
            buf.extend_from_slice(&chunk);
        }
        buf
    }

    /// Read the next chunk of output with a timeout.
    ///
    /// Waits up to `timeout` for the next chunk of data from the reader channel.
    /// Returns `Some(data)` if data arrives within the timeout, or `None` if
    /// the timeout expires or the channel is closed.
    ///
    /// Unlike [`try_read_all()`](PtyHandle::try_read_all), this returns only
    /// a single chunk (one `recv()` call). To read all available data, call
    /// this in a loop or use `try_read_all()`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use at_session::pty_pool::PtyHandle;
    /// # async fn example(handle: PtyHandle) {
    /// use std::time::Duration;
    ///
    /// // Wait up to 5 seconds for output
    /// if let Some(chunk) = handle.read_timeout(Duration::from_secs(5)).await {
    ///     println!("Received: {}", String::from_utf8_lossy(&chunk));
    /// } else {
    ///     println!("Timeout or channel closed");
    /// }
    /// # }
    /// ```
    pub async fn read_timeout(&self, timeout: std::time::Duration) -> Option<Vec<u8>> {
        let rx = self.reader.clone();
        tokio::time::timeout(timeout, async move { rx.recv_async().await.ok() })
            .await
            .ok()
            .flatten()
    }

    /// Send raw bytes to the PTY's stdin.
    ///
    /// Writes the given bytes to the writer channel, which the background
    /// writer thread will forward to the PTY master. Blocks if the channel
    /// is full (bounded at 256 messages).
    ///
    /// # Errors
    ///
    /// Returns [`PtyError::Internal`] if the writer channel is closed (this
    /// happens if the writer thread panicked or the PTY master was closed).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use at_session::pty_pool::PtyHandle;
    /// # fn example(handle: PtyHandle) -> Result<(), Box<dyn std::error::Error>> {
    /// // Send raw bytes (no newline)
    /// handle.send(b"echo hello")?;
    /// handle.send(b"\n")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn send(&self, data: &[u8]) -> Result<()> {
        self.writer
            .send(data.to_vec())
            .map_err(|e| PtyError::Internal(format!("writer channel closed: {e}")))?;
        Ok(())
    }

    /// Send a string followed by a newline (`\n`).
    ///
    /// Convenience method that appends a newline to the string and sends it
    /// via [`send()`](PtyHandle::send). Equivalent to `send(format!("{}\n", line))`.
    ///
    /// # Errors
    ///
    /// Returns [`PtyError::Internal`] if the writer channel is closed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use at_session::pty_pool::PtyHandle;
    /// # fn example(handle: PtyHandle) -> Result<(), Box<dyn std::error::Error>> {
    /// // Execute a shell command
    /// handle.send_line("ls -la")?;
    /// handle.send_line("echo $HOME")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn send_line(&self, line: &str) -> Result<()> {
        let mut data = line.as_bytes().to_vec();
        data.push(b'\n');
        self.send(&data)
    }

    /// Resize the PTY to the given dimensions (columns x rows).
    ///
    /// Updates the terminal size that the child process sees via `SIGWINCH`.
    /// This is important for full-screen terminal applications (vim, tmux, etc.)
    /// that need to know the terminal size.
    ///
    /// # Arguments
    ///
    /// - `cols`: Number of columns (characters per line), typically 80-200
    /// - `rows`: Number of rows (lines), typically 24-60
    ///
    /// # Errors
    ///
    /// Returns [`PtyError::Internal`] if the resize operation fails (rare).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use at_session::pty_pool::PtyHandle;
    /// # fn example(handle: PtyHandle) -> Result<(), Box<dyn std::error::Error>> {
    /// // Resize to standard 80x24 terminal
    /// handle.resize(80, 24)?;
    ///
    /// // Resize to wide terminal for modern editors
    /// handle.resize(120, 40)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let master = self.master.lock().unwrap_or_else(|e| {
            warn!("master lock was poisoned, recovering");
            e.into_inner()
        });
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Internal(format!("resize failed: {e}")))?;
        debug!(cols, rows, "PTY resized");
        Ok(())
    }
}

impl std::fmt::Debug for PtyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyHandle")
            .field("id", &self.id)
            .field("alive", &self.is_alive())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// PtyPool
// ---------------------------------------------------------------------------

/// Manages a pool of PTY sessions with strict capacity enforcement.
///
/// The `PtyPool` tracks active PTY handles and prevents spawning more than
/// `max_ptys` concurrent sessions. This prevents resource exhaustion from
/// unbounded PTY creation (each PTY consumes file descriptors, memory, and
/// two background threads).
///
/// ## Capacity Enforcement
///
/// - [`spawn()`] checks the active count before creating a PTY
/// - Returns [`PtyError::AtCapacity`] if `active_count() >= max_ptys`
/// - Caller must [`release()`] handles to free capacity
///
/// ## Thread Safety
///
/// The pool is thread-safe and can be shared via `Arc<PtyPool>`. All methods
/// use interior mutability with poisoned-lock recovery.
///
/// ## Resource Cleanup
///
/// The pool does NOT automatically clean up PTYs. Callers are responsible for:
/// 1. Killing the process via [`PtyHandle::kill()`]
/// 2. Releasing the handle via [`release()`] or [`kill()`]
///
/// ## Example
///
/// ```no_run
/// use at_session::pty_pool::PtyPool;
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create a pool with capacity for 10 PTYs
/// let pool = Arc::new(PtyPool::new(10));
///
/// // Spawn multiple shells
/// let handles: Vec<_> = (0..5)
///     .map(|_| pool.spawn("bash", &[], &[]))
///     .collect::<Result<_, _>>()?;
///
/// // Use the PTYs...
/// for handle in &handles {
///     handle.send_line("echo hello")?;
/// }
///
/// // Cleanup
/// for handle in handles {
///     handle.kill()?;
///     pool.release(handle.id);
/// }
/// # Ok(())
/// # }
/// ```
///
/// [`spawn()`]: PtyPool::spawn
/// [`release()`]: PtyPool::release
/// [`kill()`]: PtyPool::kill
pub struct PtyPool {
    max_ptys: usize,
    handles: Arc<Mutex<HashMap<Uuid, ()>>>,
}

impl PtyPool {
    /// Create a new pool with the given maximum number of concurrent PTYs.
    ///
    /// The pool starts empty and can grow up to `max_ptys` active sessions.
    /// Choose `max_ptys` based on your system resources:
    /// - Each PTY uses ~2 file descriptors + 2 threads + memory for buffers
    /// - Typical limits: 10-100 for desktop apps, 100-1000 for servers
    ///
    /// # Example
    ///
    /// ```
    /// use at_session::pty_pool::PtyPool;
    ///
    /// // Small pool for interactive use
    /// let pool = PtyPool::new(10);
    /// assert_eq!(pool.max_ptys(), 10);
    /// assert_eq!(pool.active_count(), 0);
    /// ```
    pub fn new(max_ptys: usize) -> Self {
        info!(max_ptys, "creating PtyPool");
        Self {
            max_ptys,
            handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Number of currently active PTY sessions tracked by this pool.
    ///
    /// This count includes all PTYs that have been spawned but not yet released,
    /// regardless of whether the underlying process is still alive. To free
    /// capacity, call [`release()`](PtyPool::release) or [`kill()`](PtyPool::kill).
    pub fn active_count(&self) -> usize {
        self.handles
            .lock()
            .unwrap_or_else(|e| {
                warn!("PtyPool lock was poisoned, recovering");
                e.into_inner()
            })
            .len()
    }

    /// Maximum capacity of the pool.
    ///
    /// This is the `max_ptys` value passed to [`new()`](PtyPool::new).
    pub fn max_ptys(&self) -> usize {
        self.max_ptys
    }

    /// Spawn a new process inside a PTY with the given command, arguments, and environment.
    ///
    /// Creates a pseudo-terminal, spawns the child process, and starts two
    /// background threads for I/O. Returns a [`PtyHandle`] that provides async
    /// channels for reading output and writing input.
    ///
    /// ## Thread Behavior
    ///
    /// Two background threads are spawned for each PTY:
    ///
    /// - **Reader thread**: Reads from PTY master (4KB chunks) and sends to the
    ///   `handle.reader` channel. Stops when:
    ///   - The child process exits and EOF is reached
    ///   - The reader channel is closed (dropped)
    ///   - An unrecoverable I/O error occurs
    ///
    /// - **Writer thread**: Receives from `handle.writer` channel and writes to
    ///   PTY master. Stops when:
    ///   - The writer channel is closed (all senders dropped)
    ///   - An unrecoverable I/O error occurs
    ///
    /// ## Initial PTY Size
    ///
    /// The PTY is created with a default size of 80 columns × 24 rows.
    /// Use [`PtyHandle::resize()`] to change it after spawning.
    ///
    /// ## Capacity Check
    ///
    /// This method enforces the pool capacity limit. If `active_count() >= max_ptys`,
    /// it returns [`PtyError::AtCapacity`] without spawning a process.
    ///
    /// # Arguments
    ///
    /// - `cmd`: Command to execute (e.g., `"bash"`, `"python3"`)
    /// - `args`: Command arguments (e.g., `&["-c", "echo hello"]`)
    /// - `env`: Environment variables to set (e.g., `&[("HOME", "/tmp")]`)
    ///
    /// # Errors
    ///
    /// - [`PtyError::AtCapacity`]: Pool is full
    /// - [`PtyError::SpawnFailed`]: PTY creation or process spawn failed
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use at_session::pty_pool::PtyPool;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let pool = PtyPool::new(5);
    ///
    /// // Spawn a shell
    /// let shell = pool.spawn("bash", &[], &[])?;
    ///
    /// // Spawn Python with environment
    /// let python = pool.spawn(
    ///     "python3",
    ///     &["-u", "-i"],  // unbuffered, interactive
    ///     &[("PYTHONPATH", "/custom/path")],
    /// )?;
    ///
    /// // Use the handles...
    /// shell.send_line("echo $SHELL")?;
    /// python.send_line("import sys; print(sys.version)")?;
    ///
    /// // Cleanup
    /// shell.kill()?;
    /// python.kill()?;
    /// pool.release(shell.id);
    /// pool.release(python.id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn spawn(&self, cmd: &str, args: &[&str], env: &[(&str, &str)]) -> Result<PtyHandle> {
        // Capacity check
        {
            let handles = self.handles.lock().unwrap_or_else(|e| {
                warn!("PtyPool lock was poisoned, recovering");
                e.into_inner()
            });
            if handles.len() >= self.max_ptys {
                return Err(PtyError::AtCapacity { max: self.max_ptys });
            }
        }

        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::SpawnFailed(e.to_string()))?;

        let mut command = CommandBuilder::new(cmd);
        for arg in args {
            command.arg(*arg);
        }
        for (k, v) in env {
            command.env(*k, *v);
        }

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| PtyError::SpawnFailed(e.to_string()))?;

        debug!(cmd, ?args, "spawned PTY process");

        let child = Arc::new(Mutex::new(child));
        let handle_id = Uuid::new_v4();

        // -- stdout reader thread --
        let (read_tx, read_rx) = flume::bounded::<Vec<u8>>(256);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::SpawnFailed(e.to_string()))?;
        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if read_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        // On macOS, EIO is expected when the child exits
                        if e.kind() != std::io::ErrorKind::Other {
                            debug!("pty reader error: {e}");
                        }
                        break;
                    }
                }
            }
        });

        // -- stdin writer thread --
        let (write_tx, write_rx) = flume::bounded::<Vec<u8>>(256);
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::SpawnFailed(e.to_string()))?;
        let writer_thread = std::thread::spawn(move || {
            while let Ok(data) = write_rx.recv() {
                if writer.write_all(&data).is_err() {
                    break;
                }
                let _ = writer.flush();
            }
        });

        // Track in pool
        {
            let mut handles = self.handles.lock().unwrap_or_else(|e| {
                warn!("PtyPool lock was poisoned, recovering");
                e.into_inner()
            });
            handles.insert(handle_id, ());
        }

        Ok(PtyHandle {
            id: handle_id,
            reader: read_rx,
            writer: write_tx,
            child,
            master: Arc::new(Mutex::new(pair.master)),
            _reader_thread: Some(reader_thread),
            _writer_thread: Some(writer_thread),
        })
    }

    /// Kill a PTY session by handle ID and remove it from the pool.
    ///
    /// This is a convenience method that removes the handle from pool tracking.
    /// The caller is still responsible for calling [`PtyHandle::kill()`] to
    /// terminate the actual process.
    ///
    /// Typically you would:
    /// 1. Call [`PtyHandle::kill()`] to terminate the process
    /// 2. Call this method to free the pool slot
    ///
    /// # Errors
    ///
    /// Returns [`PtyError::HandleNotFound`] if the given UUID is not in the pool
    /// (either never spawned or already released).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use at_session::pty_pool::PtyPool;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let pool = PtyPool::new(5);
    /// let handle = pool.spawn("bash", &[], &[])?;
    ///
    /// // Terminate process and free pool slot
    /// handle.kill()?;
    /// pool.kill(handle.id)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn kill(&self, handle_id: Uuid) -> Result<()> {
        let mut handles = self.handles.lock().unwrap_or_else(|e| {
            warn!("PtyPool lock was poisoned, recovering");
            e.into_inner()
        });
        if handles.remove(&handle_id).is_some() {
            info!(%handle_id, "removed PTY handle from pool");
            Ok(())
        } else {
            warn!(%handle_id, "PTY handle not found in pool");
            Err(PtyError::HandleNotFound(handle_id))
        }
    }

    /// Remove a handle from the pool tracking (e.g., after the process exits).
    ///
    /// This frees a slot in the pool, allowing new PTYs to be spawned.
    /// Unlike [`kill()`](PtyPool::kill), this does not return an error if
    /// the handle is not found (idempotent cleanup).
    ///
    /// Call this after:
    /// - The process has exited (naturally or via [`PtyHandle::kill()`])
    /// - You've drained any remaining output from the reader channel
    /// - You're done with the [`PtyHandle`]
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use at_session::pty_pool::PtyPool;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let pool = PtyPool::new(5);
    /// let handle = pool.spawn("bash", &[], &[])?;
    ///
    /// // Use the PTY...
    /// handle.send_line("exit")?;
    ///
    /// // Wait for exit and drain output
    /// while handle.is_alive() {
    ///     tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    /// }
    /// let _remaining = handle.try_read_all();
    ///
    /// // Release the pool slot
    /// pool.release(handle.id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn release(&self, handle_id: Uuid) {
        let mut handles = self.handles.lock().unwrap_or_else(|e| {
            warn!("PtyPool lock was poisoned, recovering");
            e.into_inner()
        });
        handles.remove(&handle_id);
        debug!(%handle_id, "released PTY handle from pool");
    }
}

impl std::fmt::Debug for PtyPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyPool")
            .field("max_ptys", &self.max_ptys)
            .field("active_count", &self.active_count())
            .finish()
    }
}
