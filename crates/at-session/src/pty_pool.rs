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

#[derive(Debug, Error)]
pub enum PtyError {
    #[error("pty pool is at capacity ({max})")]
    AtCapacity { max: usize },

    #[error("pty handle not found: {0}")]
    HandleNotFound(Uuid),

    #[error("pty spawn failed: {0}")]
    SpawnFailed(String),

    #[error("pty I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("pty internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, PtyError>;

// ---------------------------------------------------------------------------
// PtyHandle
// ---------------------------------------------------------------------------

/// A handle to a single PTY session with async read/write channels.
pub struct PtyHandle {
    pub id: Uuid,
    pub reader: flume::Receiver<Vec<u8>>,
    pub writer: flume::Sender<Vec<u8>>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    _reader_thread: Option<std::thread::JoinHandle<()>>,
    _writer_thread: Option<std::thread::JoinHandle<()>>,
}

impl PtyHandle {
    /// Check whether the underlying child process is still running.
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

    /// Kill the child process.
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

    /// Read all currently available output (non-blocking drain of the channel).
    pub fn try_read_all(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        while let Ok(chunk) = self.reader.try_recv() {
            buf.extend_from_slice(&chunk);
        }
        buf
    }

    /// Read output with an async timeout.
    pub async fn read_timeout(&self, timeout: std::time::Duration) -> Option<Vec<u8>> {
        let rx = self.reader.clone();
        tokio::time::timeout(timeout, async move { rx.recv_async().await.ok() })
            .await
            .ok()
            .flatten()
    }

    /// Send bytes to the PTY stdin.
    pub fn send(&self, data: &[u8]) -> Result<()> {
        self.writer
            .send(data.to_vec())
            .map_err(|e| PtyError::Internal(format!("writer channel closed: {e}")))?;
        Ok(())
    }

    /// Send a string followed by a newline.
    pub fn send_line(&self, line: &str) -> Result<()> {
        let mut data = line.as_bytes().to_vec();
        data.push(b'\n');
        self.send(&data)
    }

    /// Resize the PTY to the given dimensions.
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

/// Manages a pool of PTY sessions up to a configured capacity.
pub struct PtyPool {
    max_ptys: usize,
    handles: Arc<Mutex<HashMap<Uuid, ()>>>,
}

impl PtyPool {
    /// Create a new pool with the given maximum number of concurrent PTYs.
    pub fn new(max_ptys: usize) -> Self {
        info!(max_ptys, "creating PtyPool");
        Self {
            max_ptys,
            handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Number of currently active PTY sessions tracked by this pool.
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
    pub fn max_ptys(&self) -> usize {
        self.max_ptys
    }

    /// Spawn a new process inside a PTY.
    ///
    /// Returns a `PtyHandle` that provides async channels for reading stdout
    /// and writing to stdin of the spawned process.
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

    /// Remove a handle from the pool tracking (e.g. after the process exits).
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
