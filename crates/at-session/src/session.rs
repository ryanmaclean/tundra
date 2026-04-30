use std::time::Duration;

use at_core::types::CliType;
use tracing::{debug, info};
use uuid::Uuid;

use crate::cli_adapter::{adapter_for, CliAdapter};
use crate::pty_pool::{PtyHandle, PtyPool, Result};

// ---------------------------------------------------------------------------
// AgentSession
// ---------------------------------------------------------------------------

/// Ties together an agent identity, its PTY handle, and the CLI adapter used
/// to interact with the underlying coding-agent process.
pub struct AgentSession {
    /// The agent ID from at-core (mirrors `Agent::id`).
    pub agent_id: Uuid,
    /// The PTY handle for this session.
    pub handle: PtyHandle,
    /// The CLI adapter used to interpret output and manage the process.
    adapter: Box<dyn CliAdapter>,
}

impl AgentSession {
    /// Spawn a new agent session using the given pool.
    pub async fn spawn(
        pool: &PtyPool,
        agent_id: Uuid,
        cli_type: &CliType,
        task: &str,
        workdir: &str,
    ) -> Result<Self> {
        let adapter = adapter_for(cli_type);
        info!(
            %agent_id,
            cli = adapter.binary_name(),
            "spawning agent session"
        );
        let handle = adapter.spawn(pool, task, workdir).await?;
        Ok(Self {
            agent_id,
            handle,
            adapter,
        })
    }

    /// Send a command string to the agent process (appends newline).
    pub fn send_command(&self, cmd: &str) -> Result<()> {
        debug!(%self.agent_id, cmd, "sending command to agent");
        self.handle.send_line(cmd)
    }

    /// Send raw bytes to the agent process stdin.
    pub fn send_raw(&self, data: &[u8]) -> Result<()> {
        self.handle.send(data)
    }

    /// Read all currently buffered output from the agent.
    pub fn read_output(&self) -> Vec<u8> {
        self.handle.try_read_all()
    }

    /// Read output with a timeout, returning `None` if nothing arrives.
    pub async fn read_output_timeout(&self, timeout: Duration) -> Option<Vec<u8>> {
        self.handle.read_timeout(timeout).await
    }

    /// Check whether the agent process is still running.
    pub fn is_alive(&self) -> bool {
        self.handle.is_alive()
    }

    /// Kill the underlying process.
    pub fn kill(&self) -> Result<()> {
        info!(%self.agent_id, "killing agent session");
        self.handle.kill()
    }

    /// Attempt to parse the latest output into a status string.
    pub fn parse_status(&self, output: &str) -> Option<String> {
        self.adapter.parse_status_output(output)
    }

    /// The CLI type for this session.
    pub fn cli_type(&self) -> CliType {
        self.adapter.cli_type()
    }

    /// The binary name for this session's CLI.
    pub fn binary_name(&self) -> &str {
        self.adapter.binary_name()
    }

    /// The PTY handle ID.
    pub fn handle_id(&self) -> Uuid {
        self.handle.id
    }

    /// Build an `AgentSession` directly from a pre-constructed `PtyHandle`
    /// and adapter.
    ///
    /// `pub(crate)` and gated `#[cfg(test)]` so unit tests in this crate can
    /// inject fake handles/adapters without spawning a real CLI process.
    /// Production code should always use [`AgentSession::spawn`].
    #[cfg(test)]
    pub(crate) fn from_parts(
        agent_id: Uuid,
        handle: PtyHandle,
        adapter: Box<dyn CliAdapter>,
    ) -> Self {
        Self {
            agent_id,
            handle,
            adapter,
        }
    }
}

impl std::fmt::Debug for AgentSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentSession")
            .field("agent_id", &self.agent_id)
            .field("handle_id", &self.handle.id)
            .field("cli", &self.adapter.binary_name())
            .field("alive", &self.is_alive())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty_pool::PtyError;

    // -- Spawn error path: capacity-exhausted pool --------------------------

    #[tokio::test]
    async fn spawn_into_zero_capacity_pool_propagates_at_capacity() {
        let pool = PtyPool::new(0);
        let agent = Uuid::new_v4();
        let result =
            AgentSession::spawn(&pool, agent, &CliType::Claude, "task", "/tmp").await;
        assert!(matches!(
            result,
            Err(PtyError::AtCapacity { max: 0 })
        ));
    }

    #[tokio::test]
    async fn spawn_for_each_cli_type_propagates_capacity_error() {
        // Verifies the error is returned regardless of which adapter is
        // chosen — exercises the `adapter_for` -> `adapter.spawn` path for
        // every CliType variant.
        for cli in [
            CliType::Claude,
            CliType::Codex,
            CliType::Gemini,
            CliType::OpenCode,
        ] {
            let pool = PtyPool::new(0);
            let agent = Uuid::new_v4();
            let result =
                AgentSession::spawn(&pool, agent, &cli, "task", "/tmp").await;
            assert!(
                matches!(result, Err(PtyError::AtCapacity { max: 0 })),
                "expected AtCapacity for {cli:?}"
            );
        }
    }

    // -- parse_status delegation: each CliType --------------------------

    // We need an `AgentSession` to call parse_status, but constructing one
    // requires a real PTY. Instead, verify by direct adapter access through
    // the public API: `adapter_for` is in cli_adapter, so we know the
    // delegation in parse_status is a 1:1 forward. The cli_adapter tests
    // already cover all the regex paths.
    //
    // The remaining session-specific surface is the `cli_type()`,
    // `binary_name()`, and `handle_id()` accessors plus the `Debug` impl —
    // all of which require a real PTY handle to instantiate. We cover the
    // adapter-selection path (the only branching logic in `spawn`) above.

    #[test]
    fn cli_type_round_trips_through_adapter_for() {
        // Sanity check: the adapter-selection logic that AgentSession::spawn
        // relies on returns an adapter whose cli_type() matches the input.
        // This indirectly verifies the contract that `session.cli_type()`
        // returns the requested CliType.
        for cli in [
            CliType::Claude,
            CliType::Codex,
            CliType::Gemini,
            CliType::OpenCode,
        ] {
            let adapter = crate::cli_adapter::adapter_for(&cli);
            assert_eq!(adapter.cli_type(), cli);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests using fake portable_pty types (unix-only — the fakes implement
// MasterPty, which has unix-specific methods like as_raw_fd).
// ---------------------------------------------------------------------------

#[cfg(all(test, unix))]
mod unix_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};

    use crate::cli_adapter::CliAdapter;
    use crate::pty_pool::{PtyHandle, PtyPool, Result as PtyResult};

    // -- Fake portable_pty::Child / ChildKiller ----------------------------

    #[derive(Debug, Clone)]
    struct FakeChild {
        alive: Arc<Mutex<bool>>,
        kill_should_fail: bool,
    }

    impl FakeChild {
        fn alive() -> Self {
            Self {
                alive: Arc::new(Mutex::new(true)),
                kill_should_fail: false,
            }
        }

        fn dead() -> Self {
            Self {
                alive: Arc::new(Mutex::new(false)),
                kill_should_fail: false,
            }
        }
    }

    impl ChildKiller for FakeChild {
        fn kill(&mut self) -> std::io::Result<()> {
            if self.kill_should_fail {
                return Err(std::io::Error::other("fake kill failure"));
            }
            *self.alive.lock().unwrap() = false;
            Ok(())
        }

        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(self.clone())
        }
    }

    impl Child for FakeChild {
        fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
            if *self.alive.lock().unwrap() {
                Ok(None)
            } else {
                Ok(Some(ExitStatus::with_exit_code(0)))
            }
        }

        fn wait(&mut self) -> std::io::Result<ExitStatus> {
            *self.alive.lock().unwrap() = false;
            Ok(ExitStatus::with_exit_code(0))
        }

        fn process_id(&self) -> Option<u32> {
            Some(0)
        }
    }

    // -- Fake portable_pty::MasterPty --------------------------------------

    #[derive(Debug)]
    struct FakeMaster;

    impl MasterPty for FakeMaster {
        fn resize(&self, _size: PtySize) -> std::result::Result<(), anyhow::Error> {
            Ok(())
        }

        fn get_size(&self) -> std::result::Result<PtySize, anyhow::Error> {
            Ok(PtySize::default())
        }

        fn try_clone_reader(
            &self,
        ) -> std::result::Result<Box<dyn std::io::Read + Send>, anyhow::Error> {
            Ok(Box::new(std::io::empty()))
        }

        fn take_writer(
            &self,
        ) -> std::result::Result<Box<dyn std::io::Write + Send>, anyhow::Error> {
            Ok(Box::new(std::io::sink()))
        }

        fn process_group_leader(&self) -> Option<i32> {
            None
        }

        fn as_raw_fd(&self) -> Option<portable_pty::unix::RawFd> {
            None
        }
    }

    // -- Fake CliAdapter ---------------------------------------------------

    struct TestAdapter {
        cli: CliType,
        name: &'static str,
        status: Option<String>,
    }

    #[async_trait]
    impl CliAdapter for TestAdapter {
        fn cli_type(&self) -> CliType {
            self.cli.clone()
        }
        fn binary_name(&self) -> &str {
            self.name
        }
        fn default_args(&self) -> Vec<String> {
            vec![]
        }
        async fn spawn(
            &self,
            _pool: &PtyPool,
            _task: &str,
            _workdir: &str,
        ) -> PtyResult<PtyHandle> {
            // Tests should construct PtyHandle via PtyHandle::from_parts and
            // never go through the adapter's spawn path.
            unreachable!("TestAdapter::spawn should not be invoked in unit tests");
        }
        fn parse_status_output(&self, output: &str) -> Option<String> {
            if let Some(s) = &self.status {
                if output.contains("STATUS_TRIGGER") {
                    return Some(s.clone());
                }
            }
            None
        }
    }

    // -- Helpers -----------------------------------------------------------

    /// Build an AgentSession that uses the FakeChild and FakeMaster — no real
    /// PTY, no spawned process.
    fn make_session(
        adapter: Box<dyn CliAdapter>,
        child: FakeChild,
    ) -> (
        AgentSession,
        flume::Sender<Vec<u8>>,   // we hold the producing end of `reader`
        flume::Receiver<Vec<u8>>, // we hold the consuming end of `writer`
        Uuid,                     // the handle id
    ) {
        let (read_tx, read_rx) = flume::bounded::<Vec<u8>>(256);
        let (write_tx, write_rx) = flume::bounded::<Vec<u8>>(256);
        let handle_id = Uuid::new_v4();
        let handle = PtyHandle::from_parts(
            handle_id,
            read_rx,
            write_tx,
            Arc::new(Mutex::new(
                Box::new(child) as Box<dyn Child + Send + Sync>
            )),
            Arc::new(Mutex::new(Box::new(FakeMaster) as Box<dyn MasterPty + Send>)),
        );
        let agent_id = Uuid::new_v4();
        let session = AgentSession::from_parts(agent_id, handle, adapter);
        (session, read_tx, write_rx, handle_id)
    }

    fn test_adapter(cli: CliType, name: &'static str) -> Box<dyn CliAdapter> {
        Box::new(TestAdapter {
            cli,
            name,
            status: None,
        })
    }

    fn test_adapter_with_status(
        cli: CliType,
        name: &'static str,
        status: &str,
    ) -> Box<dyn CliAdapter> {
        Box::new(TestAdapter {
            cli,
            name,
            status: Some(status.to_string()),
        })
    }

    // -- Accessor tests ----------------------------------------------------

    #[test]
    fn cli_type_returns_adapter_value() {
        let (s, _r, _w, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        assert!(matches!(s.cli_type(), CliType::Claude));
    }

    #[test]
    fn binary_name_returns_adapter_value() {
        let (s, _r, _w, _id) = make_session(
            test_adapter(CliType::Codex, "codex-test"),
            FakeChild::alive(),
        );
        assert_eq!(s.binary_name(), "codex-test");
    }

    #[test]
    fn handle_id_returns_pty_handle_id() {
        let (s, _r, _w, expected) = make_session(
            test_adapter(CliType::Gemini, "gemini-test"),
            FakeChild::alive(),
        );
        assert_eq!(s.handle_id(), expected);
    }

    #[test]
    fn debug_impl_includes_key_fields() {
        let (s, _r, _w, _id) = make_session(
            test_adapter(CliType::OpenCode, "opencode-test"),
            FakeChild::alive(),
        );
        let dbg = format!("{:?}", s);
        assert!(!dbg.is_empty());
        assert!(dbg.contains("AgentSession"));
        assert!(dbg.contains("opencode-test"));
        assert!(dbg.contains("agent_id"));
        assert!(dbg.contains("handle_id"));
    }

    // -- parse_status tests -----------------------------------------------

    #[test]
    fn parse_status_returns_some_when_adapter_recognises_output() {
        let (s, _r, _w, _id) = make_session(
            test_adapter_with_status(CliType::Claude, "claude-test", "completed"),
            FakeChild::alive(),
        );
        assert_eq!(s.parse_status("STATUS_TRIGGER bla"), Some("completed".into()));
    }

    #[test]
    fn parse_status_returns_none_when_output_does_not_match() {
        let (s, _r, _w, _id) = make_session(
            test_adapter_with_status(CliType::Claude, "claude-test", "completed"),
            FakeChild::alive(),
        );
        assert_eq!(s.parse_status("nothing interesting here"), None);
    }

    #[test]
    fn parse_status_passes_through_error_status_from_adapter() {
        let (s, _r, _w, _id) = make_session(
            test_adapter_with_status(CliType::Codex, "codex-test", "error"),
            FakeChild::alive(),
        );
        assert_eq!(s.parse_status("STATUS_TRIGGER explosion"), Some("error".into()));
    }

    // -- send_command / send_raw / read_output ----------------------------

    #[test]
    fn send_command_appends_newline_and_writes_to_writer_channel() {
        let (s, _r, write_rx, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        s.send_command("hello").expect("send_command failed");
        let buf = write_rx.recv().expect("nothing on writer channel");
        assert_eq!(buf, b"hello\n".to_vec());
    }

    #[test]
    fn send_raw_writes_exact_bytes_to_writer_channel() {
        let (s, _r, write_rx, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        s.send_raw(b"\x1b[A").expect("send_raw failed");
        let buf = write_rx.recv().expect("nothing on writer channel");
        assert_eq!(buf, b"\x1b[A".to_vec());
    }

    #[test]
    fn send_command_errors_when_writer_channel_closed() {
        let (s, _r, write_rx, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        // Drop the receiver: the bounded sender will fail with a closed
        // channel error, which the production code maps to PtyError::Internal.
        drop(write_rx);
        let err = s.send_command("nope").expect_err("expected error");
        let msg = format!("{err}");
        assert!(msg.contains("writer channel closed"), "got: {msg}");
    }

    #[test]
    fn send_raw_errors_when_writer_channel_closed() {
        let (s, _r, write_rx, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        drop(write_rx);
        let err = s.send_raw(b"x").expect_err("expected error");
        assert!(format!("{err}").contains("writer channel closed"));
    }

    #[test]
    fn read_output_drains_buffered_chunks_in_order() {
        let (s, read_tx, _w, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        read_tx.send(b"hello ".to_vec()).unwrap();
        read_tx.send(b"world\n".to_vec()).unwrap();
        let out = s.read_output();
        assert_eq!(out, b"hello world\n".to_vec());
    }

    #[test]
    fn read_output_returns_empty_when_no_data_buffered() {
        let (s, _r, _w, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        assert!(s.read_output().is_empty());
    }

    #[tokio::test]
    async fn read_output_timeout_returns_some_when_data_arrives() {
        let (s, read_tx, _w, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        read_tx.send(b"chunk".to_vec()).unwrap();
        let got = s.read_output_timeout(Duration::from_millis(200)).await;
        assert_eq!(got.as_deref(), Some(&b"chunk"[..]));
    }

    #[tokio::test]
    async fn read_output_timeout_returns_none_on_timeout() {
        let (s, _read_tx, _w, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        let got = s.read_output_timeout(Duration::from_millis(20)).await;
        assert!(got.is_none());
    }

    // -- is_alive / kill --------------------------------------------------

    #[test]
    fn is_alive_returns_true_for_alive_child() {
        let (s, _r, _w, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        assert!(s.is_alive());
    }

    #[test]
    fn is_alive_returns_false_for_dead_child() {
        let (s, _r, _w, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::dead(),
        );
        assert!(!s.is_alive());
    }

    #[test]
    fn kill_flips_child_to_not_alive() {
        let (s, _r, _w, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            FakeChild::alive(),
        );
        assert!(s.is_alive());
        s.kill().expect("kill should succeed on fake child");
        assert!(!s.is_alive());
    }

    #[test]
    fn kill_propagates_internal_error_when_killer_fails() {
        let mut child = FakeChild::alive();
        child.kill_should_fail = true;
        let (s, _r, _w, _id) = make_session(
            test_adapter(CliType::Claude, "claude-test"),
            child,
        );
        let err = s.kill().expect_err("expected kill error");
        let msg = format!("{err}");
        // PtyError::Internal wraps the underlying io::Error message.
        assert!(msg.contains("fake kill failure"), "got: {msg}");
    }
}
