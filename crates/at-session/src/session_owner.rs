//! Single native/session lifetime and output owner.
use crate::native::{NativeSession, SpawnSpec};
use crate::pty_pool::{PtyError, Result};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use uuid::Uuid;

const OUTPUT_LIMIT: usize = 1024 * 1024;
const INPUT_LIMIT: usize = 1024 * 1024;
const QUANTUM: Duration = Duration::from_millis(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Liveness { Alive, Exited, Unknown(String) }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Retirement { Running, Pending, Retired, Unknown(String) }
#[derive(Clone, Debug)]
pub struct OutputChunk { pub sequence: u64, pub bytes: Vec<u8> }

pub(crate) struct State {
    pub liveness: Liveness,
    pub exit_code: Option<i32>,
    pub retirement: Retirement,
    pub retire_requested: bool,
    pub generation: u64,
    pub connected: bool,
    pub grace: Option<(u64, Instant)>,
    pub replay_limit: usize,
    pub replay_enabled: bool,
    pub output: VecDeque<OutputChunk>,
    pub output_bytes: usize,
    pub next_sequence: u64,
    pub acknowledged: u64,
    pub input: VecDeque<(Option<u64>, Arc<Vec<u8>>, usize)>,
    pub input_bytes: usize,
    pub input_pending: bool,
    pub resize: Option<(Option<u64>, u16, u16, flume::Sender<Result<()>>)>,
    pub io_closed: bool,
    pub lost_output_bytes: u64,
}

#[cfg(any(test, feature = "test-helpers"))]
pub(crate) struct Fake {
    pub reader: flume::Receiver<Vec<u8>>,
    pub writer: flume::Sender<Vec<u8>>,
    pub child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    pub master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
}

pub(crate) struct Owner {
    pub id: Uuid,
    pub state: Mutex<State>,
    pub worker: Mutex<Option<JoinHandle<()>>>,
    pub worker_registered: AtomicBool,
    pub leases: AtomicUsize,
    #[cfg(any(test, feature = "test-helpers"))]
    pub fake: Option<Fake>,
}

impl Owner {
    pub fn new(id: Uuid) -> Self {
        Self {
            id,
            state: Mutex::new(State {
                liveness: Liveness::Unknown("launch in progress".into()),
                exit_code: None,
                retirement: Retirement::Running,
                retire_requested: false,
                generation: 0,
                connected: false,
                grace: None,
                replay_limit: OUTPUT_LIMIT,
                replay_enabled: false,
                output: VecDeque::new(), output_bytes: 0, next_sequence: 1,
                acknowledged: 0,
                input: VecDeque::new(), input_bytes: 0, input_pending: false, resize: None,
                io_closed: false, lost_output_bytes: 0,
            }),
            worker: Mutex::new(None),
            worker_registered: AtomicBool::new(false),
            leases: AtomicUsize::new(0),
            #[cfg(any(test, feature = "test-helpers"))]
            fake: None,
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, State> {
        // This private lock never encloses user callbacks or blocking I/O.
        // A poison does not erase native ownership; mark it for retirement.
        self.state.lock().unwrap_or_else(|poison| {
            let mut state = poison.into_inner();
            state.retire_requested = true;
            if state.retirement != Retirement::Retired { state.retirement = Retirement::Unknown("session state poisoned".into()); }
            state
        })
    }

    pub fn liveness(&self) -> Liveness {
        #[cfg(any(test, feature = "test-helpers"))]
        if let Some(fake) = &self.fake {
            return match fake.child.lock() {
                Ok(mut child) => match child.try_wait() {
                    Ok(Some(_)) => Liveness::Exited,
                    Ok(None) => Liveness::Alive,
                    Err(error) => Liveness::Unknown(error.to_string()),
                },
                Err(_) => Liveness::Unknown("fake child lock poisoned".into()),
            };
        }
        self.lock().liveness.clone()
    }

    pub fn request_retire(&self) -> Result<()> {
        #[cfg(any(test, feature = "test-helpers"))]
        if let Some(fake) = &self.fake {
            fake.child.lock().map_err(|_| PtyError::Internal("fake child poisoned".into()))?.kill()?;
        }
        let mut state = self.lock();
        if state.retirement == Retirement::Retired { return Ok(()); }
        state.retire_requested = true;
        state.connected = false;
        state.grace = None;
        state.retirement = Retirement::Pending;
        Ok(())
    }

    pub fn attach(&self) -> Result<u64> {
        let mut state = self.lock();
        if state.retire_requested || state.io_closed || state.liveness == Liveness::Exited {
            return Err(PtyError::Internal("session is retiring or exited".into()));
        }
        // A native submitted write must complete before the generation boundary.
        if state.input_pending { return Err(PtyError::Io(std::io::ErrorKind::WouldBlock.into())); }
        state.generation = state.generation.checked_add(1)
            .ok_or_else(|| PtyError::Internal("connection generation exhausted".into()))?;
        state.connected = true;
        state.grace = None;
        Ok(state.generation)
    }

    pub fn detach(&self, generation: u64, grace: Duration, bytes: usize) -> bool {
        let mut state = self.lock();
        if state.generation != generation || !state.connected || state.retire_requested { return false; }
        state.connected = false;
        state.replay_limit = bytes;
        state.replay_enabled = true;
        state.grace = Some((generation, Instant::now() + grace));
        trim(&mut state, bytes);
        true
    }

    pub fn input(&self, generation: Option<u64>, bytes: Vec<u8>) -> Result<()> {
        if bytes.is_empty() { return Ok(()); }
        if bytes.len() > INPUT_LIMIT { return Err(PtyError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "input frame exceeds bounded capacity"))); }
        #[cfg(any(test, feature = "test-helpers"))]
        if let Some(fake) = &self.fake {
            return fake.writer.try_send(bytes).map_err(|error| match error {
                flume::TrySendError::Full(_) => PtyError::Io(std::io::ErrorKind::WouldBlock.into()),
                flume::TrySendError::Disconnected(_) => PtyError::Internal("writer channel closed".into()),
            });
        }
        let mut state = self.lock();
        if state.retire_requested || state.io_closed { return Err(PtyError::Internal("session input closed".into())); }
        if let Some(generation) = generation {
            if !state.connected || generation != state.generation { return Err(PtyError::Internal("stale connection".into())); }
        }
        if bytes.len() > INPUT_LIMIT.saturating_sub(state.input_bytes) {
            return Err(PtyError::Io(std::io::Error::from(std::io::ErrorKind::WouldBlock)));
        }
        state.input_bytes += bytes.len();
        state.input.push_back((generation, Arc::new(bytes), 0));
        Ok(())
    }

    pub fn peek(&self, generation: Option<u64>) -> Result<Option<OutputChunk>> {
        let state = self.lock();
        if let Some(generation) = generation {
            if !state.connected || generation != state.generation { return Err(PtyError::Internal("stale connection".into())); }
        }
        Ok(state.output.front().cloned())
    }

    pub fn acknowledge(&self, generation: Option<u64>, sequence: u64) -> bool {
        let mut state = self.lock();
        if let Some(generation) = generation {
            if !state.connected || generation != state.generation { return false; }
        }
        if state.output.front().map(|chunk| chunk.sequence) != Some(sequence) { return false; }
        if let Some(chunk) = state.output.pop_front() {
            state.output_bytes -= chunk.bytes.len();
            state.acknowledged = sequence;
        }
        true
    }

    pub fn joined(&self) -> bool {
        #[cfg(any(test, feature = "test-helpers"))]
        if self.fake.is_some() { return self.liveness() == Liveness::Exited; }
        if !self.worker_registered.load(Ordering::Acquire) { return false; }
        let mut worker = self.worker.lock().unwrap_or_else(|p| p.into_inner());
        if worker.as_ref().is_some_and(|worker| worker.is_finished()) {
            if let Some(worker) = worker.take() {
                if worker.join().is_err() {
                    self.lock().retirement = Retirement::Unknown("native supervisor panicked".into());
                }
            }
        }
        worker.is_none() && self.lock().retirement == Retirement::Retired
    }
}

fn trim(state: &mut State, limit: usize) {
    while state.output_bytes > limit {
        let Some(mut front) = state.output.pop_front() else { break; };
        let remove = (state.output_bytes - limit).min(front.bytes.len());
        state.output_bytes -= remove;
        state.lost_output_bytes = state.lost_output_bytes.saturating_add(remove as u64);
        if remove < front.bytes.len() {
            front.bytes.drain(..remove);
            state.output.push_front(front);
        }
    }
}

pub(crate) fn supervise(owner: Arc<Owner>, spec: SpawnSpec, ready: flume::Sender<Result<()>>) {
    let (mut native, startup_error) = match NativeSession::spawn(&spec) {
        Ok(native) => (native, None),
        Err(failure) => match failure.retained {
            Some(native) => (native, Some(failure.error.to_string())),
            None => {
                let mut state = owner.lock();
                state.liveness = Liveness::Exited;
                state.io_closed = true;
                state.retirement = Retirement::Retired;
                let _ = ready.try_send(Err(PtyError::SpawnFailed(failure.error.to_string())));
                return;
            }
        },
    };
    // Native ownership is already outside this unwind boundary before any
    // shared-state publication or channel acknowledgement can panic.
    let acknowledged = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Some(error) = startup_error {
            let mut state = owner.lock();
            state.retire_requested = true;
            state.retirement = Retirement::Unknown(error.clone());
            drop(state);
            let _ = ready.try_send(Err(PtyError::SpawnFailed(error)));
        } else {
            owner.lock().liveness = Liveness::Alive;
            if ready.try_send(Ok(())).is_err() { let _ = owner.request_retire(); }
        }
    }));
    drop(ready);
    if acknowledged.is_err() {
        let mut state = owner.lock();
        state.retire_requested = true;
        state.retirement = Retirement::Unknown("native startup publication panicked; owner retained".into());
    }
    let mut buffer = [0_u8; 4096];
    loop {
        // Keep native ownership outside the unwind boundary. A panicking tick
        // cannot drop an unreaped child and convert a retained slot into a leak.
        let tick = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        match native.try_wait() {
            Ok(Some(status)) => {
                let mut state = owner.lock();
                state.exit_code = status.code();
                state.liveness = Liveness::Exited;
                state.retire_requested = true;
                if state.retirement == Retirement::Running { state.retirement = Retirement::Pending; }
            }
            Ok(None) => owner.lock().liveness = Liveness::Alive,
            Err(error) => owner.lock().liveness = Liveness::Unknown(error.to_string()),
        }
        {
            let mut state = owner.lock();
            if let Some((generation, deadline)) = state.grace {
                if !state.connected && generation == state.generation && Instant::now() >= deadline {
                    state.retire_requested = true;
                    state.retirement = Retirement::Pending;
                    state.grace = None;
                }
            }
        }
        let retiring = owner.lock().retire_requested;
        if retiring {
            if let Err(error) = native.request_termination() {
                owner.lock().retirement = Retirement::Unknown(error.to_string());
            }
        }
        let room = { let state = owner.lock(); retiring || (state.replay_enabled && !state.connected) || state.output_bytes < OUTPUT_LIMIT };
        if room {
            match native.read(&mut buffer) {
                Ok(0) => {},
                Ok(count) => {
                    let mut state = owner.lock();
                    let sequence = state.next_sequence;
                    if let Some(next) = sequence.checked_add(1) {
                        state.next_sequence = next;
                        state.output.push_back(OutputChunk { sequence, bytes: buffer[..count].to_vec() });
                        state.output_bytes += count;
                        if (state.replay_enabled && !state.connected) || retiring {
                            let limit = state.replay_limit;
                            trim(&mut state, limit);
                        }
                    } else {
                        state.retire_requested = true;
                        state.retirement = Retirement::Unknown("output sequence exhausted".into());
                    }
                }
                Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => {},
                Err(error) => {
                    owner.lock().retirement = Retirement::Unknown(error.to_string());
                }
            }
        }
        // Nonblocking native submission and attachment share this private lock.
        // Never replace an overlapped write's front until completion is observed.
        {
            let mut state = owner.lock();
            if retiring {
                // The backend retains any submitted buffer through cancellation.
                state.input.clear(); state.input_bytes = 0;
            } else {
                while !state.input_pending {
                    let stale = state.input.front().is_some_and(|(generation, _, _)| {
                        generation.is_some_and(|generation| !state.connected || generation != state.generation)
                    });
                    if !stale { break; }
                    if let Some((_, bytes, _)) = state.input.pop_front() { state.input_bytes -= bytes.len(); }
                }
                if let Some((_, data, offset)) = state.input.front().cloned() {
                    match native.write(&data[offset..]) {
                        Ok(count) if count > 0 => {
                            if let Some(front) = state.input.front_mut() { front.2 += count; }
                            if state.input.front().is_some_and(|(_, bytes, used)| *used == bytes.len()) {
                                if let Some((_, bytes, _)) = state.input.pop_front() { state.input_bytes -= bytes.len(); }
                            }
                        }
                        Ok(_) => {
                            state.retire_requested = true;
                            state.retirement = Retirement::Unknown("native write made no progress".into());
                        }
                        Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => {},
                        Err(error) => {
                            state.retire_requested = true;
                            state.retirement = Retirement::Unknown(error.to_string());
                        }
                    }
                    state.input_pending = native.has_pending_write();
                }
            }
        }
        {
            let mut state = owner.lock();
            if let Some((generation, rows, cols, reply)) = state.resize.take() {
                let stale = generation.is_some_and(|generation| !state.connected || generation != state.generation);
                let result = if state.retire_requested || stale {
                    Err(PtyError::Internal("session resize revoked".into()))
                } else { native.resize(rows, cols).map_err(PtyError::Io) };
                // Bounded one-message reply never waits for a cancelled caller.
                let _ = reply.try_send(result);
            }
        }
        if retiring {
            match native.poll_close() {
                Ok(true) => {
                    let mut state = owner.lock();
                    if state.liveness == Liveness::Exited {
                        state.io_closed = true;
                        state.retirement = Retirement::Retired;
                        return true;
                    }
                }
                Ok(false) => {},
                Err(error) => owner.lock().retirement = Retirement::Unknown(error.to_string()),
            }
        }
        false
        }));
        match tick {
            Ok(true) => return,
            Ok(false) => {},
            Err(_) => {
                let mut state = owner.lock();
                state.retire_requested = true;
                state.retirement = Retirement::Unknown("native supervisor tick panicked; owner retained".into());
            }
        }
        std::thread::sleep(QUANTUM);
    }
}
