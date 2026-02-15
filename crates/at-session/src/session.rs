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
