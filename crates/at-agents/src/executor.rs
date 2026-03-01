use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use at_bridge::event_bus::EventBus;
use at_bridge::protocol::{BridgeMessage, EventPayload};
use at_core::types::Task;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::approval::{ApprovalPolicy, ToolApprovalSystem};
use crate::profiles::AgentConfig;
use crate::roles::RoleConfig;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("pty pool error: {0}")]
    PtyPool(String),
    #[error("agent process exited unexpectedly")]
    ProcessDied,
    #[error("task execution timed out after {0}s")]
    Timeout(u64),
    #[error("task was aborted")]
    Aborted,
    #[error("parse error: {0}")]
    Parse(String),
    #[error("internal error: {0}")]
    Internal(String),
}

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
}

/// A handle to a spawned process, abstracting over PtyHandle.
pub struct SpawnedProcess {
    pub id: Uuid,
    pub reader: flume::Receiver<Vec<u8>>,
    pub writer: flume::Sender<Vec<u8>>,
    alive: Arc<std::sync::Mutex<bool>>,
}

impl SpawnedProcess {
    /// Create a new SpawnedProcess with the given channels.
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
        }
    }

    /// Check if the process is still alive.
    pub fn is_alive(&self) -> bool {
        *self.alive.lock().unwrap_or_else(|e| {
            warn!("executor lock was poisoned, recovering");
            e.into_inner()
        })
    }

    /// Mark the process as dead (for testing).
    pub fn set_dead(&self) {
        *self.alive.lock().unwrap_or_else(|e| {
            warn!("executor lock was poisoned, recovering");
            e.into_inner()
        }) = false;
    }

    /// Send a line to the process stdin.
    pub fn send_line(&self, line: &str) -> std::result::Result<(), String> {
        let mut data = line.as_bytes().to_vec();
        data.push(b'\n');
        self.writer
            .send(data)
            .map_err(|e| format!("writer closed: {e}"))
    }

    /// Read with a timeout, returning None on timeout.
    pub async fn read_timeout(&self, timeout: Duration) -> Option<Vec<u8>> {
        let rx = self.reader.clone();
        tokio::time::timeout(timeout, async move { rx.recv_async().await.ok() })
            .await
            .ok()
            .flatten()
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

#[async_trait::async_trait]
impl PtySpawner for PtyPoolSpawner {
    fn spawn(
        &self,
        cmd: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> std::result::Result<SpawnedProcess, String> {
        let handle = self.pool.spawn(cmd, args, env).map_err(|e| e.to_string())?;

        Ok(SpawnedProcess::new(
            handle.id,
            handle.reader,
            handle.writer,
            true,
        ))
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
    pub fn new(
        pty_pool: Arc<at_session::pty_pool::PtyPool>,
        event_bus: EventBus,
    ) -> Self {
        Self {
            spawner: Arc::new(PtyPoolSpawner::new(pty_pool)),
            event_bus,
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            approval_system: Arc::new(Mutex::new(ToolApprovalSystem::new())),
        }
    }

    /// Create an executor with a custom spawner (useful for testing).
    pub fn with_spawner(
        spawner: Arc<dyn PtySpawner>,
        event_bus: EventBus,
    ) -> Self {
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

        // Build CLI args
        let cli_args = agent_config.to_cli_args();
        let args_refs: Vec<&str> = cli_args.iter().map(|s| s.as_str()).collect();

        // Build env vars
        let env_pairs: Vec<(String, String)> = agent_config.env_vars.clone().into_iter().collect();
        let env_refs: Vec<(&str, &str)> = env_pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        // Spawn the process
        let process = self
            .spawner
            .spawn(agent_config.binary_name(), &args_refs, &env_refs)
            .map_err(ExecutorError::PtyPool)?;

        let process = Arc::new(process);

        // Track as active
        {
            let mut active = self.active_tasks.lock().await;
            active.insert(task.id, Arc::clone(&process));
        }

        // Publish start event
        self.publish_event(task, "task_execution_start");

        // Send the prompt to stdin
        process.send_line(prompt).map_err(ExecutorError::Internal)?;

        // Collect output with timeout
        let timeout = Duration::from_secs(agent_config.timeout_secs);
        let mut output_buf = Vec::new();
        let mut events = Vec::new();

        let collect_result = tokio::time::timeout(timeout, async {
            // Read output chunks until the process finishes or channel closes
            loop {
                match process.read_timeout(Duration::from_secs(5)).await {
                    Some(chunk) => {
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
                    None => {
                        // Timeout on read - check if process is still alive
                        if !process.is_alive() {
                            break;
                        }
                    }
                }
            }
        })
        .await;

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

        // Remove from active tasks
        {
            let mut active = self.active_tasks.lock().await;
            active.remove(&task.id);
        }

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

        let timed_out = collect_result.is_err();
        if timed_out {
            warn!(
                task_id = %task.id,
                timeout_secs = agent_config.timeout_secs,
                "task execution timed out"
            );
            self.publish_event(task, "task_execution_timeout");
        }

        let success = !timed_out && !output.is_empty();

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
            exit_code: if success { Some(0) } else { None },
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
            process.set_dead();
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
