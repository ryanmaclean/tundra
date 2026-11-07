//! PTY capacity and leases over one retained native session owner.
use crate::native::SpawnSpec;
use crate::session_owner::{supervise, Owner};
pub use crate::session_owner::{Liveness, OutputChunk, Retirement};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("pty pool is at capacity ({max})")] AtCapacity { max: usize },
    #[error("pty handle not found: {0}")] HandleNotFound(Uuid),
    #[error("pty spawn failed: {0}")] SpawnFailed(String),
    #[error("pty I/O error: {0}")] Io(#[from] std::io::Error),
    #[error("pty internal error: {0}")] Internal(String),
}
pub type Result<T> = std::result::Result<T, PtyError>;

#[derive(Clone)]
pub struct PtyReader { owner: Arc<Owner> }
impl PtyReader {
    pub fn try_recv(&self) -> std::result::Result<Vec<u8>, flume::TryRecvError> {
        #[cfg(any(test, feature = "test-helpers"))]
        if let Some(fake) = &self.owner.fake { return fake.reader.try_recv(); }
        match self.owner.peek(None) {
            Ok(Some(chunk)) if self.owner.acknowledge(None, chunk.sequence) => Ok(chunk.bytes),
            _ if self.owner.lock().io_closed => Err(flume::TryRecvError::Disconnected),
            _ => Err(flume::TryRecvError::Empty),
        }
    }
    pub async fn recv_async(&self) -> std::result::Result<Vec<u8>, flume::RecvError> {
        #[cfg(any(test, feature = "test-helpers"))]
        if let Some(fake) = &self.owner.fake { return fake.reader.recv_async().await; }
        loop {
            match self.try_recv() {
                Ok(bytes) => return Ok(bytes),
                Err(flume::TryRecvError::Disconnected) => return Err(flume::RecvError::Disconnected),
                Err(flume::TryRecvError::Empty) => tokio::time::sleep(Duration::from_millis(5)).await,
            }
        }
    }
}

#[derive(Clone)]
pub struct PtyWriter { owner: Arc<Owner> }
impl PtyWriter {
    pub fn send(&self, bytes: Vec<u8>) -> Result<()> { self.owner.input(None, bytes) }
    pub async fn send_async(&self, bytes: Vec<u8>) -> Result<()> {
        loop {
            match self.owner.input(None, bytes.clone()) {
                Err(PtyError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                result => return result,
            }
        }
    }
}

pub struct PtyHandle {
    pub id: Uuid,
    pub reader: PtyReader,
    pub writer: PtyWriter,
    owner: Arc<Owner>,
}
impl Drop for PtyHandle {
    fn drop(&mut self) {
        if self.owner.leases.fetch_sub(1, std::sync::atomic::Ordering::AcqRel) == 1 { let _ = self.owner.request_retire(); }
    }
}
impl Clone for PtyHandle {
    fn clone(&self) -> Self { Self::lease(self.owner.clone()) }
}
impl std::fmt::Debug for PtyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyHandle").field("id", &self.id).field("liveness", &self.liveness()).finish()
    }
}
impl PtyHandle {
    fn lease(owner: Arc<Owner>) -> Self {
        owner.leases.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Self { id: owner.id, reader: PtyReader { owner: owner.clone() }, writer: PtyWriter { owner: owner.clone() }, owner }
    }
    pub fn liveness(&self) -> Liveness { self.owner.liveness() }
    pub fn exit_code(&self) -> Option<i32> { self.owner.lock().exit_code }
    /// Conservative compatibility view: an observation error never means dead.
    pub fn is_alive(&self) -> bool { self.liveness() != Liveness::Exited }
    pub fn kill(&self) -> Result<()> { self.owner.request_retire() }
    pub fn retirement(&self) -> Retirement {
        if self.owner.joined() { Retirement::Retired } else {
            match self.owner.lock().retirement.clone() { Retirement::Retired => Retirement::Pending, other => other }
        }
    }
    pub async fn wait_retired(&self, timeout: Duration) -> Result<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.owner.joined() { return Ok(()); }
            if tokio::time::Instant::now() >= deadline {
                return Err(PtyError::Internal(format!("retirement pending for {}: {:?}", self.id, self.retirement())));
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    pub fn send(&self, bytes: &[u8]) -> Result<()> { self.writer.send(bytes.to_vec()) }
    pub fn send_line(&self, line: &str) -> Result<()> {
        let mut bytes = line.as_bytes().to_vec(); bytes.push(b'\n'); self.writer.send(bytes)
    }
    pub async fn send_async(&self, bytes: &[u8]) -> Result<()> { self.writer.send_async(bytes.to_vec()).await }
    pub async fn read_timeout(&self, timeout: Duration) -> Option<Vec<u8>> {
        tokio::time::timeout(timeout, self.reader.recv_async()).await.ok().and_then(std::result::Result::ok)
    }
    pub fn try_read_all(&self) -> Vec<u8> { self.read_all_available() }
    pub fn read_all_available(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        while let Ok(chunk) = self.reader.try_recv() { bytes.extend(chunk); }
        bytes
    }
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        #[cfg(any(test, feature = "test-helpers"))]
        if let Some(fake) = &self.owner.fake {
            return fake.master.lock().map_err(|_| PtyError::Internal("fake master poisoned".into()))?
                .resize(portable_pty::PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
                .map_err(|error| PtyError::Internal(error.to_string()));
        }
        let mut state = self.owner.lock();
        if state.retire_requested { return Err(PtyError::Internal("session retiring".into())); }
        let (reply, receiver) = flume::bounded(1);
        state.resize = Some((None, rows, cols, reply));
        drop(state);
        receiver.recv_timeout(Duration::from_secs(2))
            .map_err(|error| PtyError::Internal(format!("resize acknowledgement unavailable: {error}")))?
    }
    pub async fn resize_connection(&self, generation: u64, cols: u16, rows: u16) -> Result<()> {
        let (reply, receiver) = flume::bounded(1);
        {
            let mut state = self.owner.lock();
            if state.retire_requested || !state.connected || generation != state.generation {
                return Err(PtyError::Internal("stale connection".into()));
            }
            state.resize = Some((Some(generation), rows, cols, reply));
        }
        receiver.recv_async().await.map_err(|_| PtyError::Internal("resize superseded".into()))?
    }
    /// Enable terminal reconnect replay; ordinary agent streams retain backpressure.
    pub fn set_disconnect_buffer(&self, bytes: usize) {
        let mut state = self.owner.lock();
        state.replay_enabled = true;
        state.replay_limit = bytes;
    }
    pub fn attach(&self) -> Result<u64> { self.owner.attach() }
    pub fn detach(&self, generation: u64, grace: Duration, bytes: usize) -> bool {
        self.owner.detach(generation, grace, bytes)
    }
    /// Observe generation identity independently of connection/retirement state.
    pub fn generation_matches(&self, generation: u64) -> bool {
        self.owner.lock().generation == generation
    }
    /// Match ownership rather than trusting a reused external identifier.
    pub fn same_owner(&self, other: &Self) -> bool { Arc::ptr_eq(&self.owner, &other.owner) }
    pub fn connection_current(&self, generation: u64) -> bool {
        let state = self.owner.lock();
        state.connected && state.generation == generation && !state.retire_requested
    }
    pub async fn connection_input(&self, generation: u64, bytes: Vec<u8>) -> Result<()> {
        loop {
            match self.owner.input(Some(generation), bytes.clone()) {
                Err(PtyError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => tokio::time::sleep(Duration::from_millis(5)).await,
                result => return result,
            }
        }
    }
    pub async fn connection_output(&self, generation: u64) -> Result<OutputChunk> {
        loop {
            if let Some(chunk) = self.owner.peek(Some(generation))? { return Ok(chunk); }
            if self.owner.lock().io_closed { return Err(PtyError::Internal("session output closed".into())); }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    pub fn acknowledge_output(&self, generation: u64, sequence: u64) -> bool {
        self.owner.acknowledge(Some(generation), sequence)
    }
    pub fn lost_output_bytes(&self) -> u64 { self.owner.lock().lost_output_bytes }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn from_parts(id: Uuid, reader: flume::Receiver<Vec<u8>>, writer: flume::Sender<Vec<u8>>,
        child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>, master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>) -> Self {
        let mut owner = Owner::new(id);
        owner.fake = Some(crate::session_owner::Fake { reader, writer, child, master });
        Self::lease(Arc::new(owner))
    }
}

struct PoolInner {
    max: usize,
    owners: Mutex<HashMap<Uuid, Arc<Owner>>>,
    accepting: std::sync::atomic::AtomicBool,
}
pub struct PtyPool { inner: Arc<PoolInner> }
impl std::fmt::Debug for PtyPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyPool").field("max_ptys", &self.inner.max).field("active_count", &self.active_count()).finish()
    }
}
impl PtyPool {
    pub fn new(max_ptys: usize) -> Self {
        Self { inner: Arc::new(PoolInner { max: max_ptys, owners: Mutex::new(HashMap::new()), accepting: std::sync::atomic::AtomicBool::new(true) }) }
    }
    pub fn max_ptys(&self) -> usize { self.inner.max }
    pub fn active_count(&self) -> usize {
        let mut owners = self.inner.owners.lock().unwrap_or_else(|p| p.into_inner());
        owners.retain(|_, owner| !owner.joined());
        owners.len()
    }
    pub fn available(&self) -> usize { self.inner.max.saturating_sub(self.active_count()) }
    pub fn get(&self, id: Uuid) -> Option<PtyHandle> {
        self.inner.owners.lock().unwrap_or_else(|p| p.into_inner()).get(&id).cloned().map(PtyHandle::lease)
    }
    pub fn handles(&self) -> Vec<PtyHandle> {
        self.inner.owners.lock().unwrap_or_else(|p| p.into_inner()).values().cloned().map(PtyHandle::lease).collect()
    }
    pub fn spawn(&self, cmd: &str, args: &[&str], env: &[(&str, &str)]) -> Result<PtyHandle> {
        let (handle, ready) = self.begin_spawn(cmd, args, env)?;
        ready.recv().map_err(|error| PtyError::SpawnFailed(error.to_string()))??;
        Ok(handle)
    }
    pub async fn spawn_async(&self, cmd: &str, args: &[&str], env: &[(&str, &str)]) -> Result<PtyHandle> {
        let (handle, ready) = self.begin_spawn(cmd, args, env)?;
        // Cancellation cannot remove the pool-owned preparation/child. A guard
        // requests retirement unless the handle has been delivered successfully.
        struct Pending(Option<PtyHandle>);
        impl Drop for Pending { fn drop(&mut self) { if let Some(handle) = &self.0 { let _ = handle.kill(); } } }
        let mut pending = Pending(Some(handle));
        ready.recv_async().await.map_err(|error| PtyError::SpawnFailed(error.to_string()))??;
        Ok(pending.0.take().expect("pending handle present"))
    }
    fn begin_spawn(&self, cmd: &str, args: &[&str], env: &[(&str, &str)]) -> Result<(PtyHandle, flume::Receiver<Result<()>>)> {
        use std::sync::atomic::Ordering;
        let id = Uuid::new_v4();
        let owner = Arc::new(Owner::new(id));
        let handle = PtyHandle::lease(owner.clone());
        {
            let mut owners = self.inner.owners.lock().map_err(|_| PtyError::Internal("pool lock poisoned".into()))?;
            owners.retain(|_, owner| !owner.joined());
            if !self.inner.accepting.load(Ordering::Acquire) { return Err(PtyError::Internal("pool shutting down".into())); }
            if owners.len() >= self.inner.max { return Err(PtyError::AtCapacity { max: self.inner.max }); }
            owners.insert(id, owner.clone());
        }
        let spec = SpawnSpec { program: cmd.into(), args: args.iter().map(|arg| (*arg).into()).collect(), env: env.iter().map(|(k,v)| ((*k).into(),(*v).into())).collect(), rows: 24, cols: 80 };
        let (ready_tx, ready_rx) = flume::bounded(1);
        let worker_owner = owner.clone();
        let worker = match std::thread::Builder::new().name(format!("pty-{id}")).spawn(move || supervise(worker_owner, spec, ready_tx)) {
            Ok(worker) => worker,
            Err(error) => {
                self.inner.owners.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
                return Err(PtyError::SpawnFailed(error.to_string()));
            }
        };
        *owner.worker.lock().unwrap_or_else(|p| p.into_inner()) = Some(worker);
        owner.worker_registered.store(true, Ordering::Release);
        Ok((handle, ready_rx))
    }
    pub fn kill(&self, id: Uuid) -> Result<()> { self.get(id).ok_or(PtyError::HandleNotFound(id))?.kill() }
    /// Compatibility spelling: release requests retirement, never frees a live slot.
    pub fn release(&self, id: Uuid) { if let Some(handle) = self.get(id) { let _ = handle.kill(); } }
    pub fn stop_accepting(&self) { self.inner.accepting.store(false, std::sync::atomic::Ordering::Release); }
    pub async fn shutdown(&self, timeout: Duration) -> Vec<(Uuid, Retirement)> {
        self.inner.accepting.store(false, std::sync::atomic::Ordering::Release);
        for handle in self.handles() { let _ = handle.kill(); }
        let deadline = tokio::time::Instant::now() + timeout;
        while self.active_count() > 0 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        self.handles().into_iter().map(|handle| (handle.id, handle.retirement())).collect()
    }
}
impl Drop for PoolInner {
    fn drop(&mut self) {
        // Signal only. Explicit shutdown owns the awaited retirement result.
        for owner in self.owners.get_mut().unwrap_or_else(|p| p.into_inner()).values() { let _ = owner.request_retire(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- PtyPool: pure constructor / accessor logic -------------------------

    #[test]
    fn new_pool_starts_empty() {
        let pool = PtyPool::new(5);
        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.max_ptys(), 5);
    }

    #[test]
    fn new_pool_with_zero_capacity_is_valid_construction() {
        let pool = PtyPool::new(0);
        assert_eq!(pool.max_ptys(), 0);
        assert_eq!(pool.active_count(), 0);
    }

    #[test]
    fn spawn_into_zero_capacity_pool_returns_at_capacity() {
        // This exercises the AtCapacity error path *without* needing a real
        // PTY — the capacity check happens before openpty().
        let pool = PtyPool::new(0);
        let result = pool.spawn("/bin/true", &[], &[]);
        match result {
            Err(PtyError::AtCapacity { max }) => assert_eq!(max, 0),
            other => panic!("expected AtCapacity, got: {other:?}"),
        }
    }

    #[test]
    fn release_unknown_handle_is_noop() {
        let pool = PtyPool::new(4);
        // Should not panic, should leave count at 0.
        pool.release(Uuid::new_v4());
        assert_eq!(pool.active_count(), 0);
    }

    #[test]
    fn release_does_not_return_error_for_unknown_id() {
        // Distinguishes release() (idempotent) from kill() (errors).
        let pool = PtyPool::new(4);
        pool.release(Uuid::new_v4());
        // No assertion needed beyond "didn't panic".
    }

    #[test]
    fn kill_unknown_handle_returns_handle_not_found() {
        let pool = PtyPool::new(4);
        let bogus = Uuid::new_v4();
        match pool.kill(bogus) {
            Err(PtyError::HandleNotFound(id)) => assert_eq!(id, bogus),
            other => panic!("expected HandleNotFound, got: {other:?}"),
        }
    }

    #[test]
    fn kill_on_empty_pool_returns_error_not_panic() {
        let pool = PtyPool::new(0);
        assert!(pool.kill(Uuid::new_v4()).is_err());
    }

    // -- Debug formatting ---------------------------------------------------

    #[test]
    fn pool_debug_format_contains_fields() {
        let pool = PtyPool::new(7);
        let s = format!("{pool:?}");
        assert!(s.contains("PtyPool"), "got: {s}");
        assert!(s.contains("max_ptys"), "got: {s}");
        assert!(s.contains("active_count"), "got: {s}");
        assert!(s.contains('7'), "expected capacity in debug, got: {s}");
    }

    // -- PtyError: Display + From + variant construction --------------------

    #[test]
    fn at_capacity_error_display_includes_max() {
        let e = PtyError::AtCapacity { max: 12 };
        let s = e.to_string();
        assert!(s.contains("capacity"), "got: {s}");
        assert!(s.contains("12"), "got: {s}");
    }

    #[test]
    fn handle_not_found_error_display_includes_uuid() {
        let id = Uuid::new_v4();
        let e = PtyError::HandleNotFound(id);
        let s = e.to_string();
        assert!(s.contains(&id.to_string()), "got: {s}");
    }

    #[test]
    fn spawn_failed_error_display_includes_message() {
        let e = PtyError::SpawnFailed("boom".into());
        let s = e.to_string();
        assert!(s.contains("boom"), "got: {s}");
        assert!(s.contains("spawn"), "got: {s}");
    }

    #[test]
    fn internal_error_display_includes_message() {
        let e = PtyError::Internal("locked out".into());
        let s = e.to_string();
        assert!(s.contains("locked out"), "got: {s}");
        assert!(s.contains("internal"), "got: {s}");
    }

    #[test]
    fn io_error_converts_via_from_impl() {
        // Verifies the `#[from] std::io::Error` derive on the Io variant.
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let pty_err: PtyError = io_err.into();
        assert!(matches!(pty_err, PtyError::Io(_)));
        assert!(pty_err.to_string().contains("nope"));
    }

    // -- Result alias is usable --------------------------------------------

    #[test]
    fn result_type_alias_works_with_question_mark() {
        fn helper() -> Result<u32> {
            Err(PtyError::AtCapacity { max: 0 })
        }
        assert!(helper().is_err());
    }
}
