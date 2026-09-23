use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use at_bridge::event_bus::EventBus;
use at_bridge::protocol::{BridgeMessage, EventPayload};
use at_core::types::Task;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::approval::{ApprovalPolicy, ToolApprovalSystem};
use crate::profiles::AgentConfig;
use crate::roles::RoleConfig;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during agent task execution.
///
/// The executor spawns CLI agent processes in PTY sessions and orchestrates
/// task execution with approval workflows. These errors represent failures
/// in process management, execution timeouts, parsing agent output, or
/// unexpected termination scenarios.
#[derive(Debug, Error)]
pub enum ExecutorError {
    /// An error occurred in the PTY pool when spawning or managing agent processes.
    ///
    /// This typically indicates:
    /// - Failure to allocate a new PTY session
    /// - Process spawn failures (command not found, permission denied)
    /// - I/O errors when communicating with the PTY
    ///
    /// The contained string provides details about the PTY pool failure.
    #[error("pty pool error: {0}")]
    PtyPool(String),

    /// The agent process terminated unexpectedly before completing the task.
    ///
    /// This occurs when:
    /// - The agent CLI crashes or exits with an error
    /// - The process is killed by the system (OOM, signal)
    /// - The process exits without producing expected output
    #[error("agent process exited unexpectedly")]
    ProcessDied,

    /// Task execution exceeded the configured timeout.
    ///
    /// The executor enforces timeouts to prevent runaway agent processes.
    /// The contained value is the timeout duration in seconds that was exceeded.
    #[error("task execution timed out after {0}s")]
    Timeout(u64),

    /// Task execution was explicitly aborted by the user or system.
    ///
    /// This is a clean cancellation, distinct from crashes or timeouts.
    #[error("task was aborted")]
    Aborted,

    /// Failed to parse structured output from the agent process.
    ///
    /// Agents communicate via structured JSON events in their output stream.
    /// This error occurs when the output is malformed or doesn't match the
    /// expected schema. The contained string provides parse error details.
    #[error("parse error: {0}")]
    Parse(String),

    /// An internal executor error occurred.
    ///
    /// This is a catch-all for unexpected failures that don't fit other
    /// categories, such as invariant violations or resource exhaustion.
    /// The contained string provides error details.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Result type for executor operations.
///
/// Alias for `std::result::Result<T, ExecutorError>` used throughout
/// the executor module to indicate operations that may fail with an
/// [`ExecutorError`].
pub type Result<T> = std::result::Result<T, ExecutorError>;

// ---------------------------------------------------------------------------
// ExecutionResult
// ---------------------------------------------------------------------------

/// The result of executing a task phase through a CLI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// The task ID that was executed.
    pub task_id: Uuid,
    /// Whether execution succeeded.
    pub success: bool,
    /// Raw output captured from the agent.
    pub output: String,
    /// Structured events parsed from the output, if any.
    pub events: Vec<AgentEvent>,
    /// Tool use errors encountered during execution.
    pub tool_errors: Vec<ToolUseError>,
    /// Duration of execution in milliseconds.
    pub duration_ms: u64,
    /// Exit code of the agent process, if available.
    pub exit_code: Option<i32>,
}

/// A structured event parsed from agent stdout output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub event_type: String,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// A tool_use_error parsed from LLM agent output.
/// These occur when the agent tries to use a tool that isn't available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseError {
    /// The tool name that was requested but unavailable.
    pub tool_name: String,
    /// The raw error message from the LLM platform.
    pub error_message: String,
    /// The full raw XML tag content.
    pub raw: String,
}

/// Strategy for recovering from a tool_use_error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolErrorRecovery {
    /// Inject a hint telling the agent to use an alternative tool.
    RetryWithHint { hint: String },
    /// Skip this tool call and continue execution.
    Skip,
    /// Abort the task.
    Abort,
}

/// Maps unavailable tools to available alternatives.
pub struct ToolFallbackMap {
    fallbacks: std::collections::HashMap<String, Vec<String>>,
}

impl Default for ToolFallbackMap {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolFallbackMap {
    pub fn new() -> Self {
        let mut fallbacks = std::collections::HashMap::new();
        // Common tool name mismatches in LLM agents
        fallbacks.insert(
            "Bash".into(),
            vec!["bash".into(), "shell".into(), "execute_command".into()],
        );
        fallbacks.insert(
            "bash".into(),
            vec!["Bash".into(), "shell".into(), "execute_command".into()],
        );
        fallbacks.insert(
            "Read".into(),
            vec!["read_file".into(), "cat".into(), "file_read".into()],
        );
        fallbacks.insert(
            "Write".into(),
            vec![
                "write_file".into(),
                "file_write".into(),
                "create_file".into(),
            ],
        );
        fallbacks.insert(
            "Edit".into(),
            vec![
                "edit_file".into(),
                "file_edit".into(),
                "str_replace_editor".into(),
            ],
        );
        fallbacks.insert(
            "Grep".into(),
            vec!["grep".into(), "search".into(), "ripgrep".into()],
        );
        fallbacks.insert(
            "Glob".into(),
            vec!["glob".into(), "find_files".into(), "list_files".into()],
        );
        fallbacks.insert(
            "WebSearch".into(),
            vec!["web_search".into(), "search_web".into()],
        );
        fallbacks.insert(
            "WebFetch".into(),
            vec!["web_fetch".into(), "fetch_url".into(), "curl".into()],
        );
        Self { fallbacks }
    }

    /// Register a fallback for a tool name.
    pub fn add_fallback(&mut self, tool: impl Into<String>, alternatives: Vec<String>) {
        self.fallbacks.insert(tool.into(), alternatives);
    }

    /// Given a tool name that failed, suggest alternatives.
    pub fn suggest_alternatives(&self, failed_tool: &str) -> Vec<&str> {
        self.fallbacks
            .get(failed_tool)
            .map(|alts| alts.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Build a recovery strategy for a tool_use_error.
    pub fn recover(&self, error: &ToolUseError, available_tools: &[String]) -> ToolErrorRecovery {
        let alternatives = self.suggest_alternatives(&error.tool_name);

        // Find the first alternative that's actually available
        for alt in &alternatives {
            if available_tools.iter().any(|t| t == alt) {
                return ToolErrorRecovery::RetryWithHint {
                    hint: format!(
                        "The tool '{}' is not available. Use '{}' instead.",
                        error.tool_name, alt
                    ),
                };
            }
        }

        // No alternatives found — skip this call
        ToolErrorRecovery::Skip
    }
}

// ---------------------------------------------------------------------------
// PtySpawner trait (for testability)
// ---------------------------------------------------------------------------

/// Abstraction over PTY spawning so we can mock it in tests.
#[async_trait::async_trait]
pub trait PtySpawner: Send + Sync {
    /// Spawn a process and return a handle for I/O.
    fn spawn(
        &self,
        cmd: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> std::result::Result<SpawnedProcess, String>;

    /// Spawn a process in the given working directory.
    ///
    /// The default implementation ignores `cwd` and delegates to
    /// [`spawn`](PtySpawner::spawn); spawners that run real processes
    /// (e.g. [`PtyPoolSpawner`]) override it and must fail rather than start
    /// the process somewhere else when `cwd` does not exist.
    fn spawn_in(
        &self,
        cmd: &str,
        args: &[&str],
        env: &[(&str, &str)],
        cwd: Option<&Path>,
    ) -> std::result::Result<SpawnedProcess, String> {
        if let Some(dir) = cwd {
            tracing::debug!(cwd = %dir.display(), "spawner does not support cwd; ignoring");
        }
        self.spawn(cmd, args, env)
    }
}

/// Lifecycle control over the real OS process behind a [`SpawnedProcess`].
///
/// Mock spawners can omit this; real spawners provide it so the executor can
/// read the true exit status, kill timed-out or aborted processes, and free
/// pool resources.
pub trait ProcessControl: Send + Sync {
    /// Whether the child is still running (non-blocking).
    fn is_alive(&self) -> bool;
    /// The child's exit code once it has exited (non-blocking).
    fn exit_code(&self) -> Option<i32>;
    /// Terminate the child. Must be safe to call on an exited child.
    fn kill(&self);
    /// Release resources held for the child (e.g. its PTY pool slot).
    /// Called exactly once, after the child has exited or been killed.
    fn release(&self);
}

/// Outcome of a single read from a [`SpawnedProcess`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOutcome {
    /// A chunk of output arrived.
    Chunk(Vec<u8>),
    /// No output arrived within the timeout; the process may still be running.
    Timeout,
    /// The output channel is closed (EOF): no more output will ever arrive.
    Closed,
}

/// A handle to a spawned process, abstracting over PtyHandle.
///
/// Dropping the last reference kills the child if it is still running and
/// releases its resources (see [`ProcessControl`]).
pub struct SpawnedProcess {
    pub id: Uuid,
    pub reader: flume::Receiver<Vec<u8>>,
    pub writer: flume::Sender<Vec<u8>>,
    alive: Arc<std::sync::Mutex<bool>>,
    aborted: AtomicBool,
    cleaned_up: AtomicBool,
    control: Option<Box<dyn ProcessControl>>,
}

impl SpawnedProcess {
    /// Create a new SpawnedProcess with the given channels.
    ///
    /// Without a [`ProcessControl`] the executor cannot observe the real exit
    /// status; use [`with_control`](SpawnedProcess::with_control) for real
    /// processes.
    pub fn new(
        id: Uuid,
        reader: flume::Receiver<Vec<u8>>,
        writer: flume::Sender<Vec<u8>>,
        alive: bool,
    ) -> Self {
        Self {
            id,
            reader,
            writer,
            alive: Arc::new(std::sync::Mutex::new(alive)),
            aborted: AtomicBool::new(false),
            cleaned_up: AtomicBool::new(false),
            control: None,
        }
    }

    /// Create a SpawnedProcess backed by a real process via `control`.
    pub fn with_control(
        id: Uuid,
        reader: flume::Receiver<Vec<u8>>,
        writer: flume::Sender<Vec<u8>>,
        control: Box<dyn ProcessControl>,
    ) -> Self {
        let mut p = Self::new(id, reader, writer, true);
        p.control = Some(control);
        p
    }

    /// Whether this process reports its real lifecycle via [`ProcessControl`].
    pub fn has_control(&self) -> bool {
        self.control.is_some()
    }

    fn alive_flag(&self) -> bool {
        *self.alive.lock().unwrap_or_else(|e| {
            warn!("executor lock was poisoned, recovering");
            e.into_inner()
        })
    }

    /// Check if the process is still alive.
    pub fn is_alive(&self) -> bool {
        self.alive_flag() && self.control.as_ref().is_none_or(|c| c.is_alive())
    }

    /// The real exit code, if the process has exited and a
    /// [`ProcessControl`] is attached.
    pub fn exit_code(&self) -> Option<i32> {
        self.control.as_ref().and_then(|c| c.exit_code())
    }

    /// Whether [`abort`](SpawnedProcess::abort) was called.
    pub fn was_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }

    /// Mark the process as dead (for testing).
    pub fn set_dead(&self) {
        *self.alive.lock().unwrap_or_else(|e| {
            warn!("executor lock was poisoned, recovering");
            e.into_inner()
        }) = false;
    }

    /// Kill the underlying process if it is still running.
    pub fn kill(&self) {
        if let Some(c) = &self.control {
            if c.is_alive() {
                c.kill();
            }
        }
    }

    /// Abort: mark dead and kill the underlying process.
    pub fn abort(&self) {
        self.aborted.store(true, Ordering::SeqCst);
        self.set_dead();
        self.kill();
    }

    /// Kill the process if still running and release its resources.
    /// Idempotent.
    pub fn cleanup(&self) {
        if self.cleaned_up.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(c) = &self.control {
            if c.is_alive() {
                c.kill();
            }
            c.release();
        }
    }

    /// Send a line to the process stdin.
    pub fn send_line(&self, line: &str) -> std::result::Result<(), String> {
        let mut data = line.as_bytes().to_vec();
        data.push(b'\n');
        self.writer
            .send(data)
            .map_err(|e| format!("writer closed: {e}"))
    }

    /// Read with a timeout, returning None on timeout *or* closed channel.
    ///
    /// Prefer [`read_next`](SpawnedProcess::read_next), which distinguishes
    /// the two: a closed channel returns immediately, so looping on `None`
    /// busy-spins once the process has exited.
    pub async fn read_timeout(&self, timeout: Duration) -> Option<Vec<u8>> {
        match self.read_next(timeout).await {
            ReadOutcome::Chunk(c) => Some(c),
            ReadOutcome::Timeout | ReadOutcome::Closed => None,
        }
    }

    /// Read the next chunk, distinguishing timeout from EOF.
    pub async fn read_next(&self, timeout: Duration) -> ReadOutcome {
        match tokio::time::timeout(timeout, self.reader.recv_async()).await {
            Ok(Ok(chunk)) => ReadOutcome::Chunk(chunk),
            Ok(Err(_)) => ReadOutcome::Closed,
            Err(_) => ReadOutcome::Timeout,
        }
    }

    /// Drain all currently available output.
    pub fn try_read_all(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        while let Ok(chunk) = self.reader.try_recv() {
            buf.extend_from_slice(&chunk);
        }
        buf
    }
}

impl Drop for SpawnedProcess {
    fn drop(&mut self) {
        self.cleanup();
    }
}

// ---------------------------------------------------------------------------
// Real PtyPool-based spawner
// ---------------------------------------------------------------------------

/// Wraps the real at-session PtyPool for production use.
pub struct PtyPoolSpawner {
    pool: Arc<at_session::pty_pool::PtyPool>,
}

impl PtyPoolSpawner {
    pub fn new(pool: Arc<at_session::pty_pool::PtyPool>) -> Self {
        Self { pool }
    }
}

/// [`ProcessControl`] over a real PTY child; releases its pool slot.
struct PtyControl {
    pool: Arc<at_session::pty_pool::PtyPool>,
    handle: at_session::pty_pool::PtyHandle,
}

impl ProcessControl for PtyControl {
    fn is_alive(&self) -> bool {
        self.handle.is_alive()
    }

    fn exit_code(&self) -> Option<i32> {
        self.handle.exit_code()
    }

    fn kill(&self) {
        if let Err(e) = self.handle.kill() {
            warn!(pty_id = %self.handle.id, error = %e, "failed to kill agent process");
        }
    }

    fn release(&self) {
        self.pool.release(self.handle.id);
    }
}

#[async_trait::async_trait]
impl PtySpawner for PtyPoolSpawner {
    fn spawn(
        &self,
        cmd: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> std::result::Result<SpawnedProcess, String> {
        self.spawn_in(cmd, args, env, None)
    }

    fn spawn_in(
        &self,
        cmd: &str,
        args: &[&str],
        env: &[(&str, &str)],
        cwd: Option<&Path>,
    ) -> std::result::Result<SpawnedProcess, String> {
        let handle = self
            .pool
            .spawn_in(cmd, args, env, cwd)
            .map_err(|e| e.to_string())?;
        let (id, reader, writer) = (handle.id, handle.reader.clone(), handle.writer.clone());

        Ok(SpawnedProcess::with_control(
            id,
            reader,
            writer,
            Box::new(PtyControl {
                pool: Arc::clone(&self.pool),
                handle,
            }),
        ))
    }
}

/// Ensures an execution's process is killed, its resources released, and
/// its `active_tasks` entry removed on every exit path, including early
/// `?` returns and cancellation of the executing future.
struct ActiveTaskGuard {
    task_id: Uuid,
    process: Arc<SpawnedProcess>,
    active_tasks: Arc<Mutex<HashMap<Uuid, Arc<SpawnedProcess>>>>,
}

impl Drop for ActiveTaskGuard {
    fn drop(&mut self) {
        self.process.cleanup();
        let task_id = self.task_id;
        let process = Arc::clone(&self.process);
        let remove = move |active: &mut HashMap<Uuid, Arc<SpawnedProcess>>| {
            if active
                .get(&task_id)
                .is_some_and(|p| Arc::ptr_eq(p, &process))
            {
                active.remove(&task_id);
            }
        };
        if let Ok(mut active) = self.active_tasks.try_lock() {
            remove(&mut active);
        } else if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let active_tasks = Arc::clone(&self.active_tasks);
            rt.spawn(async move { remove(&mut *active_tasks.lock().await) });
        }
    }
}

// ---------------------------------------------------------------------------
// AgentExecutor
// ---------------------------------------------------------------------------

/// The core agent execution engine.
///
/// Takes a Task and AgentConfig, spawns a CLI process via the PTY layer,
/// feeds the task prompt to stdin, parses output events, and publishes
/// them to the EventBus. Handles completion, timeout, and failure.
pub struct AgentExecutor {
    spawner: Arc<dyn PtySpawner>,
    event_bus: EventBus,
    /// Active task handles, keyed by task ID.
    active_tasks: Arc<Mutex<HashMap<Uuid, Arc<SpawnedProcess>>>>,
    /// Tool approval system for gating tool invocations.
    approval_system: Arc<Mutex<ToolApprovalSystem>>,
}

impl AgentExecutor {
    /// Create a new executor with a real PtyPool.
    pub fn new(pty_pool: Arc<at_session::pty_pool::PtyPool>, event_bus: EventBus) -> Self {
        Self {
            spawner: Arc::new(PtyPoolSpawner::new(pty_pool)),
            event_bus,
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            approval_system: Arc::new(Mutex::new(ToolApprovalSystem::new())),
        }
    }

    /// Create an executor with a custom spawner (useful for testing).
    pub fn with_spawner(spawner: Arc<dyn PtySpawner>, event_bus: EventBus) -> Self {
        Self {
            spawner,
            event_bus,
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            approval_system: Arc::new(Mutex::new(ToolApprovalSystem::new())),
        }
    }

    /// Create an executor with a custom spawner and approval system.
    pub fn with_spawner_and_approval(
        spawner: Arc<dyn PtySpawner>,
        event_bus: EventBus,
        approval_system: ToolApprovalSystem,
    ) -> Self {
        Self {
            spawner,
            event_bus,
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            approval_system: Arc::new(Mutex::new(approval_system)),
        }
    }

    /// Get a reference to the approval system.
    pub fn approval_system(&self) -> &Arc<Mutex<ToolApprovalSystem>> {
        &self.approval_system
    }

    /// Execute a task using the given agent configuration and role config.
    ///
    /// This will:
    /// 1. Apply role-specific pre-execution hooks
    /// 2. Build CLI arguments from the AgentConfig
    /// 3. Spawn the CLI process via the PTY pool
    /// 4. Feed the task prompt (with system prompt) to stdin
    /// 5. Collect output, parsing for structured events
    /// 6. Check tool approvals for any tool_call events
    /// 7. Publish progress events to the EventBus
    /// 8. Apply role-specific post-execution hooks
    /// 9. Return the execution result
    pub async fn execute_task_with_role(
        &self,
        task: &Task,
        agent_config: &AgentConfig,
        role_config: &dyn RoleConfig,
    ) -> Result<ExecutionResult> {
        // Apply pre-execute hook
        let pre_hook = role_config.pre_execute(&task.title);
        if let Some(ref preamble) = pre_hook {
            tracing::debug!(task_id = %task.id, preamble_len = preamble.len(), "applied pre-execute hook");
        }

        // Build prompt with system prompt included
        let system_prompt = role_config.system_prompt();
        let base_prompt = build_prompt(task);
        let prompt = if let Some(preamble) = pre_hook {
            format!(
                "System: {}\n\n{}\n\n{}",
                system_prompt, preamble, base_prompt
            )
        } else {
            format!("System: {}\n\n{}", system_prompt, base_prompt)
        };

        let result = self.execute_task_inner(task, agent_config, &prompt).await?;

        // Apply post-execute hook
        if let Some(summary) = role_config.post_execute(&result.output) {
            tracing::info!(task_id = %task.id, summary = %summary, "post-execute hook");
        }

        Ok(result)
    }

    /// Execute a task using the given agent configuration (without role config).
    ///
    /// This will:
    /// 1. Build CLI arguments from the AgentConfig
    /// 2. Spawn the CLI process via the PTY pool
    /// 3. Feed the task prompt to stdin
    /// 4. Collect output, parsing for structured events
    /// 5. Publish progress events to the EventBus
    /// 6. Return the execution result
    pub async fn execute_task(
        &self,
        task: &Task,
        agent_config: &AgentConfig,
    ) -> Result<ExecutionResult> {
        let prompt = build_prompt(task);
        self.execute_task_inner(task, agent_config, &prompt).await
    }

    /// Internal task execution implementation.
    async fn execute_task_inner(
        &self,
        task: &Task,
        agent_config: &AgentConfig,
        prompt: &str,
    ) -> Result<ExecutionResult> {
        let start = std::time::Instant::now();

        info!(
            task_id = %task.id,
            cli = agent_config.binary_name(),
            model = %agent_config.model,
            "executing task"
        );

        // Build CLI args. CLIs in print mode (claude -p) do not read a prompt
        // from a TTY stdin, so for those the prompt goes on the command line.
        let mut cli_args = agent_config.to_cli_args();
        let prompt_in_args = agent_config.prompt_in_args();
        if prompt_in_args {
            cli_args.push("--".to_string());
            cli_args.push(prompt.to_string());
        }
        let args_refs: Vec<&str> = cli_args.iter().map(|s| s.as_str()).collect();

        // Build env vars
        let env_pairs: Vec<(String, String)> = agent_config.env_vars.clone().into_iter().collect();
        let env_refs: Vec<(&str, &str)> = env_pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        // Run the agent inside the task's worktree when it has one.
        let cwd = task.worktree_path.as_deref().map(Path::new);

        // Spawn the process
        let process = self
            .spawner
            .spawn_in(agent_config.binary_name(), &args_refs, &env_refs, cwd)
            .map_err(ExecutorError::PtyPool)?;

        let process = Arc::new(process);

        // Track as active; the guard kills/releases the process and removes
        // the entry on every exit path (including `?` and cancellation).
        {
            let mut active = self.active_tasks.lock().await;
            active.insert(task.id, Arc::clone(&process));
        }
        let _guard = ActiveTaskGuard {
            task_id: task.id,
            process: Arc::clone(&process),
            active_tasks: Arc::clone(&self.active_tasks),
        };

        // Publish start event
        self.publish_event(task, "task_execution_start");

        // Send the prompt to stdin
        if !prompt_in_args {
            process.send_line(prompt).map_err(ExecutorError::Internal)?;
        }

        // Collect output with timeout
        let timeout = Duration::from_secs(agent_config.timeout_secs);
        let mut output_buf = Vec::new();
        let mut events = Vec::new();

        let collect_result = tokio::time::timeout(timeout, async {
            // Read output chunks until the channel closes (EOF) or the
            // process is no longer alive.
            loop {
                match process.read_next(Duration::from_secs(1)).await {
                    ReadOutcome::Chunk(chunk) => {
                        let text = String::from_utf8_lossy(&chunk);
                        // Try to parse structured events from each line
                        for line in text.lines() {
                            if let Some(event) = parse_agent_event(line) {
                                events.push(event);
                            }
                        }
                        output_buf.extend_from_slice(&chunk);

                        // Publish incremental output
                        self.event_bus.publish(BridgeMessage::AgentOutput {
                            agent_id: task.id,
                            output: text.to_string(),
                        });
                    }
                    // EOF: the PTY reader thread exits when the child does.
                    ReadOutcome::Closed => break,
                    ReadOutcome::Timeout => {
                        if !process.is_alive() {
                            break;
                        }
                    }
                }
            }
        })
        .await;

        let timed_out = collect_result.is_err();
        let aborted = process.was_aborted();

        if timed_out {
            warn!(
                task_id = %task.id,
                timeout_secs = agent_config.timeout_secs,
                "task execution timed out; killing agent process"
            );
            process.kill();
            self.publish_event(task, "task_execution_timeout");
        } else if process.has_control() && !aborted {
            // Output EOF can precede the child being reapable; give it a
            // moment so we report the real exit status.
            let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
            while process.exit_code().is_none() && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }

        // Drain any remaining buffered output
        let remaining = process.try_read_all();
        if !remaining.is_empty() {
            let text = String::from_utf8_lossy(&remaining);
            for line in text.lines() {
                if let Some(event) = parse_agent_event(line) {
                    events.push(event);
                }
            }
            output_buf.extend_from_slice(&remaining);
        }

        let exit_code = process.exit_code();

        // Kill (if still running), release the pool slot, and remove from
        // active tasks.
        drop(_guard);

        let duration_ms = start.elapsed().as_millis() as u64;
        let output = String::from_utf8_lossy(&output_buf).to_string();

        // Parse tool_use_errors from the accumulated output
        let tool_errors = parse_tool_use_errors(&output);
        if !tool_errors.is_empty() {
            warn!(
                task_id = %task.id,
                error_count = tool_errors.len(),
                tools = ?tool_errors.iter().map(|e| &e.tool_name).collect::<Vec<_>>(),
                "tool_use_errors detected in agent output"
            );
        }

        // Success comes from the real exit status. Only spawners that cannot
        // report one (no ProcessControl, e.g. test mocks) fall back to
        // "produced output".
        let success = !timed_out
            && !aborted
            && match exit_code {
                Some(code) => code == 0,
                None => !process.has_control() && !output.is_empty(),
            };

        // Publish completion event
        self.publish_event(
            task,
            if success {
                "task_execution_complete"
            } else {
                "task_execution_failed"
            },
        );

        info!(
            task_id = %task.id,
            success,
            ?exit_code,
            timed_out,
            aborted,
            duration_ms,
            events_count = events.len(),
            tool_errors = tool_errors.len(),
            "task execution finished"
        );

        Ok(ExecutionResult {
            task_id: task.id,
            success,
            output,
            events,
            tool_errors,
            duration_ms,
            exit_code,
        })
    }

    /// Check tool approval for a tool_call event.
    ///
    /// When the agent output contains a tool_call event, this method checks
    /// the approval system to determine if the tool is allowed. Returns the
    /// approval policy so callers can decide how to proceed.
    pub async fn check_tool_event(
        &self,
        event: &AgentEvent,
        agent_role: &at_core::types::AgentRole,
        agent_id: Uuid,
    ) -> ApprovalPolicy {
        if event.event_type != "tool_call" {
            return ApprovalPolicy::AutoApprove;
        }

        let tool_name = &event.message;
        let approval_system = self.approval_system.lock().await;
        let policy = approval_system.check_approval(tool_name, agent_role);

        match policy {
            ApprovalPolicy::AutoApprove => {
                tracing::debug!(tool = %tool_name, "tool auto-approved");
            }
            ApprovalPolicy::RequireApproval => {
                tracing::warn!(
                    tool = %tool_name,
                    agent_id = %agent_id,
                    "tool requires approval"
                );
            }
            ApprovalPolicy::Deny => {
                tracing::error!(
                    tool = %tool_name,
                    agent_id = %agent_id,
                    "tool invocation DENIED by policy"
                );
            }
        }

        policy
    }

    /// Abort a running task by its ID.
    pub async fn abort_task(&self, task_id: Uuid) -> Result<()> {
        let mut active = self.active_tasks.lock().await;
        if let Some(process) = active.remove(&task_id) {
            info!(%task_id, "aborting task execution");
            // Kill the real process; the executing future observes the abort,
            // reports failure, and releases the pool slot.
            process.abort();
            Ok(())
        } else {
            warn!(%task_id, "task not found in active tasks");
            Err(ExecutorError::Internal(format!(
                "task {task_id} not found in active tasks"
            )))
        }
    }

    /// Publish an event to the bus.
    fn publish_event(&self, task: &Task, event_type: &str) {
        self.event_bus.publish(BridgeMessage::Event(EventPayload {
            event_type: event_type.to_string(),
            agent_id: None,
            bead_id: Some(task.bead_id),
            message: format!("Task '{}': {}", task.title, event_type),
            timestamp: Utc::now(),
        }));
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the prompt string to feed to the agent CLI.
fn build_prompt(task: &Task) -> String {
    let desc = task.description.as_deref().unwrap_or("No description");
    format!(
        "Task: {}\nDescription: {}\nPhase: {:?}\nPriority: {:?}\nComplexity: {:?}",
        task.title, desc, task.phase, task.priority, task.complexity
    )
}

/// Try to parse a line of agent output as a structured JSON event.
///
/// Expected format: `{"event":"<type>","message":"...","data":{...}}`
/// or progress markers like `[PROGRESS] 50%`
fn parse_agent_event(line: &str) -> Option<AgentEvent> {
    let trimmed = line.trim();

    // Try JSON parse first
    if trimmed.starts_with('{') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(event_type) = val.get("event").and_then(|v| v.as_str()) {
                return Some(AgentEvent {
                    event_type: event_type.to_string(),
                    message: val
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    data: val.get("data").cloned(),
                });
            }
        }
    }

    // Try progress marker: [PROGRESS] NN%
    if trimmed.starts_with("[PROGRESS]") {
        let rest = trimmed.trim_start_matches("[PROGRESS]").trim();
        return Some(AgentEvent {
            event_type: "progress".to_string(),
            message: rest.to_string(),
            data: None,
        });
    }

    // Try error marker: [ERROR] ...
    if trimmed.starts_with("[ERROR]") {
        let rest = trimmed.trim_start_matches("[ERROR]").trim();
        return Some(AgentEvent {
            event_type: "error".to_string(),
            message: rest.to_string(),
            data: None,
        });
    }

    None
}

/// Parse `<tool_use_error>` XML tags from agent output.
///
/// These tags appear when an LLM platform rejects a tool call because
/// the tool doesn't exist in the available tool set. Common examples:
/// - `<tool_use_error>Error: No such tool available: Bash</tool_use_error>`
/// - `<tool_use_error>Tool "Read" is not available</tool_use_error>`
pub fn parse_tool_use_errors(output: &str) -> Vec<ToolUseError> {
    let mut errors = Vec::new();
    let open_tag = "<tool_use_error>";
    let close_tag = "</tool_use_error>";

    let mut search_from = 0;
    while let Some(start) = output[search_from..].find(open_tag) {
        let abs_start = search_from + start;
        let content_start = abs_start + open_tag.len();

        if let Some(end) = output[content_start..].find(close_tag) {
            let content = &output[content_start..content_start + end];
            let raw = &output[abs_start..content_start + end + close_tag.len()];

            let tool_name = extract_tool_name(content);
            errors.push(ToolUseError {
                tool_name,
                error_message: content.trim().to_string(),
                raw: raw.to_string(),
            });

            search_from = content_start + end + close_tag.len();
        } else {
            break;
        }
    }

    errors
}

/// Extract the tool name from a tool_use_error message.
///
/// Handles patterns like:
/// - "Error: No such tool available: Bash"
/// - "Tool \"Read\" is not available"
/// - "Unknown tool: Write"
fn extract_tool_name(error_msg: &str) -> String {
    let msg = error_msg.trim();

    // Pattern: "No such tool available: <ToolName>"
    if let Some(idx) = msg.rfind(": ") {
        let after = msg[idx + 2..].trim();
        if !after.is_empty() && after.chars().next().is_some_and(|c| c.is_alphabetic()) {
            return after.to_string();
        }
    }

    // Pattern: "Tool \"<ToolName>\" is not available" or "Tool '<ToolName>'"
    if msg.contains("Tool") {
        let cleaned = msg.replace(['"', '\''], "");
        for word in cleaned.split_whitespace() {
            // Tool names are typically PascalCase or lowercase
            if word != "Tool"
                && word != "is"
                && word != "not"
                && word != "available"
                && word != "Error:"
                && word != "Unknown"
                && word.chars().next().is_some_and(|c| c.is_alphabetic())
            {
                return word.to_string();
            }
        }
    }

    // Fallback: return the whole message
    msg.to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use at_core::types::*;

    // -- Mock spawner for testing --

    struct MockSpawner {
        /// Pre-canned output chunks to send through the reader.
        output_chunks: Vec<Vec<u8>>,
        /// Whether the process should report as alive.
        starts_alive: bool,
        /// Holds write receivers to prevent channel from closing.
        _write_rxs: std::sync::Mutex<Vec<flume::Receiver<Vec<u8>>>>,
    }

    impl MockSpawner {
        fn new(output_chunks: Vec<Vec<u8>>, starts_alive: bool) -> Self {
            Self {
                output_chunks,
                starts_alive,
                _write_rxs: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl PtySpawner for MockSpawner {
        fn spawn(
            &self,
            _cmd: &str,
            _args: &[&str],
            _env: &[(&str, &str)],
        ) -> std::result::Result<SpawnedProcess, String> {
            let (read_tx, read_rx) = flume::bounded(256);
            let (write_tx, write_rx) = flume::bounded::<Vec<u8>>(256);

            // Keep write_rx alive so send_line doesn't fail
            self._write_rxs.lock().unwrap().push(write_rx);

            // Send pre-canned output
            for chunk in &self.output_chunks {
                let _ = read_tx.send(chunk.clone());
            }
            // Drop sender to signal EOF
            drop(read_tx);

            Ok(SpawnedProcess::new(
                Uuid::new_v4(),
                read_rx,
                write_tx,
                self.starts_alive,
            ))
        }
    }

    // -- Mock with a real-process-like lifecycle (ProcessControl) --

    #[derive(Default)]
    struct ControlState {
        alive: AtomicBool,
        exit_code: std::sync::Mutex<Option<i32>>,
        killed: AtomicBool,
        released: std::sync::atomic::AtomicUsize,
    }

    struct MockControl(Arc<ControlState>);

    impl ProcessControl for MockControl {
        fn is_alive(&self) -> bool {
            self.0.alive.load(Ordering::SeqCst)
        }
        fn exit_code(&self) -> Option<i32> {
            *self.0.exit_code.lock().unwrap()
        }
        fn kill(&self) {
            self.0.killed.store(true, Ordering::SeqCst);
            self.0.alive.store(false, Ordering::SeqCst);
            *self.0.exit_code.lock().unwrap() = Some(137);
        }
        fn release(&self) {
            self.0.released.fetch_add(1, Ordering::SeqCst);
        }
    }

    type KeptChannels = (flume::Sender<Vec<u8>>, flume::Receiver<Vec<u8>>);

    /// Spawner whose process exits with `exit_code` after emitting `output`
    /// (reader closed), or, when `exit_code` is None, keeps running with the
    /// reader open until killed.
    struct ControlSpawner {
        output: Vec<u8>,
        exit_code: Option<i32>,
        state: Arc<ControlState>,
        cwd: std::sync::Mutex<Option<std::path::PathBuf>>,
        keep: std::sync::Mutex<Vec<KeptChannels>>,
    }

    impl ControlSpawner {
        fn new(output: &[u8], exit_code: Option<i32>) -> Self {
            Self {
                output: output.to_vec(),
                exit_code,
                state: Arc::new(ControlState::default()),
                cwd: std::sync::Mutex::new(None),
                keep: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl PtySpawner for ControlSpawner {
        fn spawn(
            &self,
            cmd: &str,
            args: &[&str],
            env: &[(&str, &str)],
        ) -> std::result::Result<SpawnedProcess, String> {
            self.spawn_in(cmd, args, env, None)
        }

        fn spawn_in(
            &self,
            _cmd: &str,
            _args: &[&str],
            _env: &[(&str, &str)],
            cwd: Option<&Path>,
        ) -> std::result::Result<SpawnedProcess, String> {
            *self.cwd.lock().unwrap() = cwd.map(Path::to_path_buf);
            let (read_tx, read_rx) = flume::bounded(256);
            let (write_tx, write_rx) = flume::bounded::<Vec<u8>>(256);
            if !self.output.is_empty() {
                read_tx.send(self.output.clone()).unwrap();
            }
            match self.exit_code {
                Some(code) => {
                    *self.state.exit_code.lock().unwrap() = Some(code);
                    self.state.alive.store(false, Ordering::SeqCst);
                    drop(read_tx);
                    self.keep.lock().unwrap().push((write_tx.clone(), write_rx));
                }
                None => {
                    self.state.alive.store(true, Ordering::SeqCst);
                    // Keep the reader open: a hung process that prints nothing.
                    self.keep.lock().unwrap().push((read_tx, write_rx));
                }
            }
            Ok(SpawnedProcess::with_control(
                Uuid::new_v4(),
                read_rx,
                write_tx,
                Box::new(MockControl(Arc::clone(&self.state))),
            ))
        }
    }

    #[tokio::test]
    async fn execute_task_returns_when_reader_closes_even_if_reported_alive() {
        // Regression: a closed reader made read_timeout return None instantly,
        // the loop saw is_alive()==true, and spun forever at 100% CPU.
        let spawner = Arc::new(MockSpawner::new(vec![b"done\n".to_vec()], true));
        let executor = AgentExecutor::with_spawner(spawner, EventBus::new());
        let task = make_test_task();
        let mut config = make_config();
        config.timeout_secs = 600;

        let result = tokio::time::timeout(
            Duration::from_secs(5),
            executor.execute_task(&task, &config),
        )
        .await
        .expect("execute_task must return once the output channel closes")
        .unwrap();
        assert!(result.output.contains("done"));
    }

    #[tokio::test]
    async fn execute_task_nonzero_exit_is_failure_with_real_exit_code() {
        let spawner = Arc::new(ControlSpawner::new(
            b"error: unknown option '--thinking-budget'\n",
            Some(1),
        ));
        let state = Arc::clone(&spawner.state);
        let executor = AgentExecutor::with_spawner(spawner, EventBus::new());
        let task = make_test_task();
        let mut config = make_config();
        config.timeout_secs = 5;

        let result = executor.execute_task(&task, &config).await.unwrap();
        assert!(!result.success, "non-empty output must not imply success");
        assert_eq!(result.exit_code, Some(1));
        assert_eq!(state.released.load(Ordering::SeqCst), 1);
        assert!(executor.active_tasks.lock().await.is_empty());
    }

    #[tokio::test]
    async fn execute_task_zero_exit_is_success_and_releases_slot() {
        let spawner = Arc::new(ControlSpawner::new(b"all good\n", Some(0)));
        let state = Arc::clone(&spawner.state);
        let executor = AgentExecutor::with_spawner(spawner, EventBus::new());
        let task = make_test_task();
        let mut config = make_config();
        config.timeout_secs = 5;

        let result = executor.execute_task(&task, &config).await.unwrap();
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert!(!state.killed.load(Ordering::SeqCst));
        assert_eq!(state.released.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn execute_task_timeout_fires_and_kills_process() {
        let spawner = Arc::new(ControlSpawner::new(b"", None));
        let state = Arc::clone(&spawner.state);
        let executor = AgentExecutor::with_spawner(spawner, EventBus::new());
        let task = make_test_task();
        let mut config = make_config();
        config.timeout_secs = 1;

        let result = tokio::time::timeout(
            Duration::from_secs(10),
            executor.execute_task(&task, &config),
        )
        .await
        .expect("overall timeout must fire")
        .unwrap();
        assert!(!result.success);
        assert!(state.killed.load(Ordering::SeqCst), "timed-out process must be killed");
        assert_eq!(state.released.load(Ordering::SeqCst), 1);
        assert!(executor.active_tasks.lock().await.is_empty());
    }

    #[tokio::test]
    async fn abort_task_kills_running_process() {
        let spawner = Arc::new(ControlSpawner::new(b"", None));
        let state = Arc::clone(&spawner.state);
        let executor = Arc::new(AgentExecutor::with_spawner(spawner, EventBus::new()));
        let task = make_test_task();
        let task_id = task.id;
        let mut config = make_config();
        config.timeout_secs = 60;

        let exec = Arc::clone(&executor);
        let run = tokio::spawn(async move { exec.execute_task(&task, &config).await });

        for _ in 0..100 {
            if executor.active_tasks.lock().await.contains_key(&task_id) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        executor.abort_task(task_id).await.unwrap();

        let result = tokio::time::timeout(Duration::from_secs(10), run)
            .await
            .expect("aborted execution must finish")
            .unwrap()
            .unwrap();
        assert!(!result.success);
        assert!(state.killed.load(Ordering::SeqCst), "abort must kill the process");
        assert_eq!(state.released.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancelled_execution_still_kills_and_releases() {
        let spawner = Arc::new(ControlSpawner::new(b"", None));
        let state = Arc::clone(&spawner.state);
        let executor = AgentExecutor::with_spawner(spawner, EventBus::new());
        let task = make_test_task();
        let mut config = make_config();
        config.timeout_secs = 60;

        // Drop the future mid-execution (e.g. an outer timeout).
        let _ = tokio::time::timeout(
            Duration::from_millis(200),
            executor.execute_task(&task, &config),
        )
        .await;
        assert!(state.killed.load(Ordering::SeqCst));
        assert_eq!(state.released.load(Ordering::SeqCst), 1);
        assert!(executor.active_tasks.lock().await.is_empty());
    }

    #[tokio::test]
    async fn execute_task_runs_in_task_worktree() {
        let spawner = Arc::new(ControlSpawner::new(b"ok\n", Some(0)));
        let executor = AgentExecutor::with_spawner(Arc::clone(&spawner) as _, EventBus::new());
        let mut task = make_test_task();
        task.worktree_path = Some("/tmp/some-worktree".to_string());
        let config = make_config();

        executor.execute_task(&task, &config).await.unwrap();
        assert_eq!(
            spawner.cwd.lock().unwrap().as_deref(),
            Some(Path::new("/tmp/some-worktree"))
        );
    }

    #[tokio::test]
    async fn pty_pool_spawner_reports_exit_code_cwd_and_frees_slot() {
        let pool = Arc::new(at_session::pty_pool::PtyPool::new(1));
        let spawner = PtyPoolSpawner::new(Arc::clone(&pool));
        let dir = std::env::temp_dir().join(format!("at-agents-cwd-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let expected = dir.canonicalize().unwrap();

        let process = spawner
            .spawn_in("/bin/sh", &["-c", "pwd -P; exit 3"], &[], Some(&dir))
            .unwrap();
        let mut out = Vec::new();
        loop {
            match process.read_next(Duration::from_secs(5)).await {
                ReadOutcome::Chunk(c) => out.extend_from_slice(&c),
                ReadOutcome::Closed => break,
                ReadOutcome::Timeout => panic!("real PTY reader never closed"),
            }
        }
        let mut code = None;
        for _ in 0..100 {
            code = process.exit_code();
            if code.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(code, Some(3));
        assert_eq!(String::from_utf8_lossy(&out).trim(), expected.to_string_lossy());

        assert_eq!(pool.active_count(), 1);
        drop(process);
        assert_eq!(pool.active_count(), 0, "dropping the process must free its slot");

        // A long-running child is killed on abort and its slot freed.
        let process = spawner.spawn("/bin/sleep", &["30"], &[]).unwrap();
        assert!(process.is_alive());
        process.abort();
        assert!(!process.is_alive());
        drop(process);
        assert_eq!(pool.active_count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn make_test_task() -> Task {
        Task::new(
            "Test task",
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        )
    }

    fn make_config() -> AgentConfig {
        AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding)
    }

    #[tokio::test]
    async fn execute_task_produces_result_with_output() {
        let spawner = Arc::new(MockSpawner::new(
            vec![b"Hello from agent\n".to_vec()],
            false,
        ));
        let bus = EventBus::new();

        let executor = AgentExecutor::with_spawner(spawner, bus);
        let task = make_test_task();
        let mut config = make_config();
        config.timeout_secs = 2;

        let result = executor.execute_task(&task, &config).await.unwrap();
        assert_eq!(result.task_id, task.id);
        assert!(result.output.contains("Hello from agent"));
        assert!(result.success);
    }

    #[tokio::test]
    async fn execute_task_parses_json_events() {
        let json_event =
            r#"{"event":"tool_call","message":"Reading file","data":{"file":"src/main.rs"}}"#;
        let output = format!("{json_event}\nsome normal output\n");

        let spawner = Arc::new(MockSpawner::new(vec![output.into_bytes()], false));
        let bus = EventBus::new();

        let executor = AgentExecutor::with_spawner(spawner, bus);
        let task = make_test_task();
        let mut config = make_config();
        config.timeout_secs = 2;

        let result = executor.execute_task(&task, &config).await.unwrap();
        assert!(!result.events.is_empty());
        assert_eq!(result.events[0].event_type, "tool_call");
        assert_eq!(result.events[0].message, "Reading file");
    }

    #[tokio::test]
    async fn execute_task_parses_progress_markers() {
        let output = b"[PROGRESS] 50% complete\n".to_vec();

        let spawner = Arc::new(MockSpawner::new(vec![output], false));
        let bus = EventBus::new();

        let executor = AgentExecutor::with_spawner(spawner, bus);
        let task = make_test_task();
        let mut config = make_config();
        config.timeout_secs = 2;

        let result = executor.execute_task(&task, &config).await.unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].event_type, "progress");
        assert!(result.events[0].message.contains("50%"));
    }

    #[tokio::test]
    async fn execute_task_publishes_events_to_bus() {
        let spawner = Arc::new(MockSpawner::new(vec![b"output\n".to_vec()], false));
        let bus = EventBus::new();
        let rx = bus.subscribe();

        let executor = AgentExecutor::with_spawner(spawner, bus);
        let task = make_test_task();
        let mut config = make_config();
        config.timeout_secs = 2;

        let _ = executor.execute_task(&task, &config).await.unwrap();

        // Should have received start event
        let mut found_start = false;
        let mut found_complete = false;
        while let Ok(msg) = rx.try_recv() {
            if let BridgeMessage::Event(payload) = &*msg {
                if payload.event_type == "task_execution_start" {
                    found_start = true;
                }
                if payload.event_type == "task_execution_complete" {
                    found_complete = true;
                }
            }
        }
        assert!(found_start, "should have published start event");
        assert!(found_complete, "should have published complete event");
    }

    #[tokio::test]
    async fn abort_task_removes_from_active() {
        let (read_tx, read_rx) = flume::bounded(256);
        let (write_tx, _write_rx) = flume::bounded::<Vec<u8>>(256);

        let task_id = Uuid::new_v4();
        let process = Arc::new(SpawnedProcess::new(Uuid::new_v4(), read_rx, write_tx, true));

        let bus = EventBus::new();
        let spawner: Arc<dyn PtySpawner> = Arc::new(MockSpawner::new(vec![], true));

        let executor = AgentExecutor::with_spawner(spawner, bus);

        // Manually insert a task as active
        {
            let mut active = executor.active_tasks.lock().await;
            active.insert(task_id, process);
        }

        // Abort it
        let result = executor.abort_task(task_id).await;
        assert!(result.is_ok());

        // Should no longer be active
        let active = executor.active_tasks.lock().await;
        assert!(!active.contains_key(&task_id));

        // Keep read_tx alive to avoid compile warning
        drop(read_tx);
    }

    #[test]
    fn parse_agent_event_json() {
        let line = r#"{"event":"tool_call","message":"test","data":null}"#;
        let event = parse_agent_event(line).unwrap();
        assert_eq!(event.event_type, "tool_call");
        assert_eq!(event.message, "test");
    }

    #[test]
    fn parse_agent_event_progress() {
        let event = parse_agent_event("[PROGRESS] 75%").unwrap();
        assert_eq!(event.event_type, "progress");
        assert!(event.message.contains("75%"));
    }

    #[test]
    fn parse_agent_event_error() {
        let event = parse_agent_event("[ERROR] something failed").unwrap();
        assert_eq!(event.event_type, "error");
        assert!(event.message.contains("something failed"));
    }

    #[test]
    fn parse_agent_event_plain_text_returns_none() {
        assert!(parse_agent_event("just some normal output").is_none());
        assert!(parse_agent_event("").is_none());
    }

    #[test]
    fn build_prompt_includes_task_info() {
        let task = make_test_task();
        let prompt = build_prompt(&task);
        assert!(prompt.contains("Test task"));
        assert!(prompt.contains("Discovery"));
        assert!(prompt.contains("Medium"));
    }

    // -- tool_use_error parsing tests --

    #[test]
    fn parse_tool_use_error_no_such_tool() {
        let output = r#"some output
<tool_use_error>Error: No such tool available: Bash</tool_use_error>
more output"#;
        let errors = parse_tool_use_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].tool_name, "Bash");
        assert!(errors[0].error_message.contains("No such tool"));
    }

    #[test]
    fn parse_tool_use_error_multiple() {
        let output = r#"<tool_use_error>Error: No such tool available: Bash</tool_use_error>
text in between
<tool_use_error>Error: No such tool available: Read</tool_use_error>"#;
        let errors = parse_tool_use_errors(output);
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].tool_name, "Bash");
        assert_eq!(errors[1].tool_name, "Read");
    }

    #[test]
    fn parse_tool_use_error_none() {
        let output = "normal output without any errors";
        let errors = parse_tool_use_errors(output);
        assert!(errors.is_empty());
    }

    #[test]
    fn parse_tool_use_error_unclosed_tag() {
        let output = "<tool_use_error>Error: No such tool available: Bash";
        let errors = parse_tool_use_errors(output);
        assert!(errors.is_empty());
    }

    #[test]
    fn parse_tool_use_error_tool_quoted() {
        let output = r#"<tool_use_error>Tool "Write" is not available</tool_use_error>"#;
        let errors = parse_tool_use_errors(output);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].tool_name, "Write");
    }

    #[test]
    fn tool_fallback_map_suggests_alternatives() {
        let map = ToolFallbackMap::new();
        let alts = map.suggest_alternatives("Bash");
        assert!(!alts.is_empty());
        assert!(alts.contains(&"bash"));
    }

    #[test]
    fn tool_fallback_map_unknown_tool() {
        let map = ToolFallbackMap::new();
        let alts = map.suggest_alternatives("NonExistentTool");
        assert!(alts.is_empty());
    }

    #[test]
    fn tool_fallback_map_recover_with_available() {
        let map = ToolFallbackMap::new();
        let error = ToolUseError {
            tool_name: "Bash".into(),
            error_message: "No such tool".into(),
            raw: "<tool_use_error>No such tool</tool_use_error>".into(),
        };
        let available = vec!["bash".to_string(), "read_file".to_string()];
        let recovery = map.recover(&error, &available);
        match recovery {
            ToolErrorRecovery::RetryWithHint { hint } => {
                assert!(hint.contains("bash"), "hint should suggest 'bash': {hint}");
            }
            other => panic!("expected RetryWithHint, got {other:?}"),
        }
    }

    #[test]
    fn tool_fallback_map_recover_no_available() {
        let map = ToolFallbackMap::new();
        let error = ToolUseError {
            tool_name: "Bash".into(),
            error_message: "No such tool".into(),
            raw: "".into(),
        };
        let available = vec!["some_other_tool".to_string()];
        let recovery = map.recover(&error, &available);
        assert_eq!(recovery, ToolErrorRecovery::Skip);
    }

    #[test]
    fn tool_fallback_map_custom_fallback() {
        let mut map = ToolFallbackMap::new();
        map.add_fallback("MyCustomTool", vec!["alt_tool".to_string()]);
        let alts = map.suggest_alternatives("MyCustomTool");
        assert_eq!(alts, vec!["alt_tool"]);
    }

    #[tokio::test]
    async fn execute_task_detects_tool_use_errors() {
        let output_with_error = b"some output\n<tool_use_error>Error: No such tool available: Bash</tool_use_error>\nmore output\n".to_vec();

        let spawner = Arc::new(MockSpawner::new(vec![output_with_error], false));
        let bus = EventBus::new();

        let executor = AgentExecutor::with_spawner(spawner, bus);
        let task = make_test_task();
        let mut config = make_config();
        config.timeout_secs = 2;

        let result = executor.execute_task(&task, &config).await.unwrap();
        assert_eq!(result.tool_errors.len(), 1);
        assert_eq!(result.tool_errors[0].tool_name, "Bash");
    }
}
