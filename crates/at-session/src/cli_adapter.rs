//! CLI adapter pattern for spawning coding-agent tools in PTY sessions.
//!
//! This module provides a trait-based adapter pattern for launching different
//! AI coding assistant CLIs (Claude, Codex, Gemini, OpenCode) in PTY sessions.
//! Each adapter knows the specific command-line conventions, arguments, and
//! output patterns for its respective CLI tool.
//!
//! ## Architecture
//!
//! - **[`CliAdapter`]**: Trait defining the adapter interface
//! - **Adapter Implementations**: [`ClaudeAdapter`], [`CodexAdapter`],
//!   [`GeminiAdapter`], [`OpenCodeAdapter`]
//! - **[`adapter_for()`]**: Factory function to get the right adapter for a CLI type
//!
//! ## Adapter Responsibilities
//!
//! Each adapter implementation provides:
//!
//! 1. **Binary name**: The command to execute (e.g., "claude", "codex")
//! 2. **Default arguments**: CLI flags always passed (e.g., permission flags)
//! 3. **Spawn logic**: How to construct the full command with task and workdir
//! 4. **Status parsing**: How to extract completion/error status from output
//!
//! ## CLI Type Support
//!
//! - **Claude**: Uses `--dangerously-skip-permissions` flag and `-p` for prompt
//! - **Codex**: Uses `--approval-mode full-auto -q` for non-interactive mode
//! - **Gemini**: Simple `-p` prompt flag, no special permissions needed
//! - **OpenCode**: Direct task argument, no special flags
//!
//! ## Example
//!
//! ```no_run
//! use at_session::cli_adapter::{adapter_for, CliAdapter};
//! use at_session::pty_pool::PtyPool;
//! use at_core::types::CliType;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let pool = PtyPool::new(10);
//! let adapter = adapter_for(&CliType::Claude);
//!
//! // Spawn a Claude session with a task
//! let handle = adapter.spawn(
//!     &pool,
//!     "Implement feature X",
//!     "/path/to/workdir"
//! ).await?;
//!
//! // Read output and parse status
//! if let Some(output) = handle.read_timeout(std::time::Duration::from_secs(5)).await {
//!     let output_str = String::from_utf8_lossy(&output);
//!     if let Some(status) = adapter.parse_status_output(&output_str) {
//!         println!("Task status: {}", status);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use at_core::types::CliType;

use crate::pty_pool::{PtyHandle, PtyPool, Result};

// ---------------------------------------------------------------------------
// CliAdapter trait
// ---------------------------------------------------------------------------

/// Trait for CLI tool adapters that know how to launch and interact with a
/// specific coding-agent CLI.
///
/// This trait abstracts the differences between AI coding assistant CLIs,
/// allowing the terminal session system to spawn and manage any supported
/// CLI tool through a uniform interface.
///
/// ## Implementations
///
/// - [`ClaudeAdapter`]: Anthropic's Claude CLI
/// - [`CodexAdapter`]: OpenAI Codex CLI
/// - [`GeminiAdapter`]: Google Gemini CLI
/// - [`OpenCodeAdapter`]: OpenCode CLI
///
/// ## Thread Safety
///
/// All adapters must be `Send + Sync` to work with the async runtime.
#[async_trait]
pub trait CliAdapter: Send + Sync {
    /// Returns the CLI type this adapter handles.
    ///
    /// Used for routing and registry lookups.
    fn cli_type(&self) -> CliType;

    /// Returns the binary name or path for the CLI tool.
    ///
    /// This should be the command that would be typed in a shell (e.g., "claude").
    /// The binary must be in the PATH or this should be an absolute path.
    fn binary_name(&self) -> &str;

    /// Returns default arguments that are always passed to the CLI.
    ///
    /// These are prepended to all spawn commands and typically include:
    /// - Permission/approval flags (e.g., `--dangerously-skip-permissions`)
    /// - Output format flags (e.g., `-q` for quiet mode)
    /// - Mode flags (e.g., `--approval-mode full-auto`)
    fn default_args(&self) -> Vec<String>;

    /// Spawns the CLI inside a PTY from the given pool.
    ///
    /// # Arguments
    ///
    /// - `pool`: The PTY pool to spawn in
    /// - `task`: The task description/prompt to pass to the CLI
    /// - `workdir`: The working directory where the CLI should execute
    ///
    /// # Returns
    ///
    /// A [`PtyHandle`] for reading output and sending input to the CLI session.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The pool is at capacity ([`PtyError::AtCapacity`])
    /// - The binary is not found or fails to spawn ([`PtyError::SpawnFailed`])
    ///
    /// [`PtyError::AtCapacity`]: crate::pty_pool::PtyError::AtCapacity
    /// [`PtyError::SpawnFailed`]: crate::pty_pool::PtyError::SpawnFailed
    async fn spawn(&self, pool: &PtyPool, task: &str, workdir: &str) -> Result<PtyHandle>;

    /// Attempts to extract a human-readable status string from raw CLI output.
    ///
    /// Parses common status indicators like "completed", "error", "done", etc.
    /// from the CLI's stdout/stderr output. Each adapter knows its CLI's
    /// specific output conventions.
    ///
    /// # Arguments
    ///
    /// - `output`: Raw text output from the CLI (stdout/stderr merged in PTY)
    ///
    /// # Returns
    ///
    /// - `Some(status)`: A normalized status string ("completed", "error", etc.)
    /// - `None`: No recognizable status found in the output
    ///
    /// # Example Status Strings
    ///
    /// - `"completed"`: Task finished successfully
    /// - `"error"`: Task encountered an error
    fn parse_status_output(&self, output: &str) -> Option<String>;
}

// ---------------------------------------------------------------------------
// Claude adapter
// ---------------------------------------------------------------------------

/// Adapter for Anthropic's Claude CLI.
///
/// Spawns the `claude` command with the following conventions:
/// - Binary: `claude`
/// - Default flags: `--dangerously-skip-permissions` (skips interactive approval)
/// - Prompt flag: `-p` followed by the task description
/// - Working directory: Set via `PWD` environment variable
///
/// ## Status Parsing
///
/// Recognizes these patterns in Claude's output:
/// - `"Task complete"` or `"Done!"` → `"completed"`
/// - `"Error"` or `"error:"` → `"error"`
///
/// ## Example Command
///
/// ```text
/// claude --dangerously-skip-permissions -p "Implement feature X"
/// ```
pub struct ClaudeAdapter;

#[async_trait]
impl CliAdapter for ClaudeAdapter {
    fn cli_type(&self) -> CliType {
        CliType::Claude
    }

    fn binary_name(&self) -> &str {
        "claude"
    }

    fn default_args(&self) -> Vec<String> {
        vec!["--dangerously-skip-permissions".into()]
    }

    async fn spawn(&self, pool: &PtyPool, task: &str, workdir: &str) -> Result<PtyHandle> {
        let args_owned = self.default_args();
        let mut args: Vec<&str> = args_owned.iter().map(|s| s.as_str()).collect();
        args.push("-p");
        args.push(task);
        let env = [("PWD", workdir)];
        pool.spawn(self.binary_name(), &args, &env)
    }

    fn parse_status_output(&self, output: &str) -> Option<String> {
        // Look for common Claude CLI status patterns
        if output.contains("Task complete") || output.contains("Done!") {
            Some("completed".into())
        } else if output.contains("Error") || output.contains("error:") {
            Some("error".into())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Codex adapter
// ---------------------------------------------------------------------------

/// Adapter for OpenAI's Codex CLI.
///
/// Spawns the `codex` command with the following conventions:
/// - Binary: `codex`
/// - Default flags: `--approval-mode full-auto -q` (non-interactive, quiet)
/// - Task argument: Passed directly without a flag
/// - Working directory: Set via `PWD` environment variable
///
/// ## Status Parsing
///
/// Recognizes these patterns in Codex's output:
/// - `"completed"` or `"finished"` → `"completed"`
/// - `"error"` → `"error"`
///
/// ## Example Command
///
/// ```text
/// codex --approval-mode full-auto -q "Implement feature X"
/// ```
pub struct CodexAdapter;

#[async_trait]
impl CliAdapter for CodexAdapter {
    fn cli_type(&self) -> CliType {
        CliType::Codex
    }

    fn binary_name(&self) -> &str {
        "codex"
    }

    fn default_args(&self) -> Vec<String> {
        vec!["--approval-mode".into(), "full-auto".into(), "-q".into()]
    }

    async fn spawn(&self, pool: &PtyPool, task: &str, workdir: &str) -> Result<PtyHandle> {
        let args_owned = self.default_args();
        let mut args: Vec<&str> = args_owned.iter().map(|s| s.as_str()).collect();
        args.push(task);
        let env = [("PWD", workdir)];
        pool.spawn(self.binary_name(), &args, &env)
    }

    fn parse_status_output(&self, output: &str) -> Option<String> {
        if output.contains("completed") || output.contains("finished") {
            Some("completed".into())
        } else if output.contains("error") {
            Some("error".into())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Gemini adapter
// ---------------------------------------------------------------------------

/// Adapter for Google's Gemini CLI.
///
/// Spawns the `gemini` command with the following conventions:
/// - Binary: `gemini`
/// - Default flags: None
/// - Prompt flag: `-p` followed by the task description
/// - Working directory: Set via `PWD` environment variable
///
/// ## Status Parsing
///
/// Recognizes these patterns in Gemini's output:
/// - `"Done"` or `"Complete"` → `"completed"`
/// - `"Error"` → `"error"`
///
/// ## Example Command
///
/// ```text
/// gemini -p "Implement feature X"
/// ```
pub struct GeminiAdapter;

#[async_trait]
impl CliAdapter for GeminiAdapter {
    fn cli_type(&self) -> CliType {
        CliType::Gemini
    }

    fn binary_name(&self) -> &str {
        "gemini"
    }

    fn default_args(&self) -> Vec<String> {
        vec![]
    }

    async fn spawn(&self, pool: &PtyPool, task: &str, workdir: &str) -> Result<PtyHandle> {
        let args: Vec<&str> = vec!["-p", task];
        let env = [("PWD", workdir)];
        pool.spawn(self.binary_name(), &args, &env)
    }

    fn parse_status_output(&self, output: &str) -> Option<String> {
        if output.contains("Done") || output.contains("Complete") {
            Some("completed".into())
        } else if output.contains("Error") {
            Some("error".into())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// OpenCode adapter
// ---------------------------------------------------------------------------

/// Adapter for the OpenCode CLI.
///
/// Spawns the `opencode` command with the following conventions:
/// - Binary: `opencode`
/// - Default flags: None
/// - Task argument: Passed directly without a flag
/// - Working directory: Set via `PWD` environment variable
///
/// ## Status Parsing
///
/// Recognizes these patterns in OpenCode's output:
/// - `"done"` or `"complete"` → `"completed"`
/// - `"error"` or `"Error"` → `"error"`
///
/// ## Example Command
///
/// ```text
/// opencode "Implement feature X"
/// ```
pub struct OpenCodeAdapter;

#[async_trait]
impl CliAdapter for OpenCodeAdapter {
    fn cli_type(&self) -> CliType {
        CliType::OpenCode
    }

    fn binary_name(&self) -> &str {
        "opencode"
    }

    fn default_args(&self) -> Vec<String> {
        vec![]
    }

    async fn spawn(&self, pool: &PtyPool, task: &str, workdir: &str) -> Result<PtyHandle> {
        let args: Vec<&str> = vec![task];
        let env = [("PWD", workdir)];
        pool.spawn(self.binary_name(), &args, &env)
    }

    fn parse_status_output(&self, output: &str) -> Option<String> {
        if output.contains("done") || output.contains("complete") {
            Some("completed".into())
        } else if output.contains("error") || output.contains("Error") {
            Some("error".into())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Factory helper
// ---------------------------------------------------------------------------

/// Creates the appropriate adapter for a given CLI type.
///
/// This factory function returns a boxed trait object that implements
/// [`CliAdapter`] for the specified CLI type. Use this instead of
/// constructing adapter structs directly.
///
/// # Arguments
///
/// - `cli_type`: The type of coding assistant CLI to get an adapter for
///
/// # Returns
///
/// A boxed [`CliAdapter`] implementation for the specified CLI type.
///
/// # Example
///
/// ```
/// use at_session::cli_adapter::adapter_for;
/// use at_core::types::CliType;
///
/// let claude_adapter = adapter_for(&CliType::Claude);
/// assert_eq!(claude_adapter.binary_name(), "claude");
///
/// let codex_adapter = adapter_for(&CliType::Codex);
/// assert_eq!(codex_adapter.binary_name(), "codex");
/// ```
pub fn adapter_for(cli_type: &CliType) -> Box<dyn CliAdapter> {
    match cli_type {
        CliType::Claude => Box::new(ClaudeAdapter),
        CliType::Codex => Box::new(CodexAdapter),
        CliType::Gemini => Box::new(GeminiAdapter),
        CliType::OpenCode => Box::new(OpenCodeAdapter),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Factory ------------------------------------------------------------

    #[test]
    fn adapter_for_claude_returns_claude_adapter() {
        let adapter = adapter_for(&CliType::Claude);
        assert_eq!(adapter.binary_name(), "claude");
        assert_eq!(adapter.cli_type(), CliType::Claude);
    }

    #[test]
    fn adapter_for_codex_returns_codex_adapter() {
        let adapter = adapter_for(&CliType::Codex);
        assert_eq!(adapter.binary_name(), "codex");
        assert_eq!(adapter.cli_type(), CliType::Codex);
    }

    #[test]
    fn adapter_for_gemini_returns_gemini_adapter() {
        let adapter = adapter_for(&CliType::Gemini);
        assert_eq!(adapter.binary_name(), "gemini");
        assert_eq!(adapter.cli_type(), CliType::Gemini);
    }

    #[test]
    fn adapter_for_opencode_returns_opencode_adapter() {
        let adapter = adapter_for(&CliType::OpenCode);
        assert_eq!(adapter.binary_name(), "opencode");
        assert_eq!(adapter.cli_type(), CliType::OpenCode);
    }

    // -- Default args -------------------------------------------------------

    #[test]
    fn claude_default_args_include_skip_permissions() {
        let args = ClaudeAdapter.default_args();
        assert_eq!(args, vec!["--dangerously-skip-permissions".to_string()]);
    }

    #[test]
    fn codex_default_args_include_full_auto_and_quiet() {
        let args = CodexAdapter.default_args();
        assert_eq!(
            args,
            vec![
                "--approval-mode".to_string(),
                "full-auto".to_string(),
                "-q".to_string(),
            ]
        );
    }

    #[test]
    fn gemini_default_args_are_empty() {
        let args = GeminiAdapter.default_args();
        assert!(args.is_empty());
    }

    #[test]
    fn opencode_default_args_are_empty() {
        let args = OpenCodeAdapter.default_args();
        assert!(args.is_empty());
    }

    // -- Status parsing: Claude ---------------------------------------------

    #[test]
    fn claude_parses_task_complete_as_completed() {
        let status = ClaudeAdapter.parse_status_output("All work finished. Task complete.");
        assert_eq!(status.as_deref(), Some("completed"));
    }

    #[test]
    fn claude_parses_done_bang_as_completed() {
        let status = ClaudeAdapter.parse_status_output("Done!");
        assert_eq!(status.as_deref(), Some("completed"));
    }

    #[test]
    fn claude_parses_error_capital_as_error() {
        let status = ClaudeAdapter.parse_status_output("Error: something blew up");
        assert_eq!(status.as_deref(), Some("error"));
    }

    #[test]
    fn claude_parses_error_lowercase_as_error() {
        let status = ClaudeAdapter.parse_status_output("note: error: bad input");
        assert_eq!(status.as_deref(), Some("error"));
    }

    #[test]
    fn claude_parses_unrelated_output_as_none() {
        let status = ClaudeAdapter.parse_status_output("just some normal output here");
        assert!(status.is_none());
    }

    #[test]
    fn claude_parses_empty_output_as_none() {
        let status = ClaudeAdapter.parse_status_output("");
        assert!(status.is_none());
    }

    // -- Status parsing: Codex ----------------------------------------------

    #[test]
    fn codex_parses_completed_as_completed() {
        let status = CodexAdapter.parse_status_output("Job completed successfully");
        assert_eq!(status.as_deref(), Some("completed"));
    }

    #[test]
    fn codex_parses_finished_as_completed() {
        let status = CodexAdapter.parse_status_output("All steps finished.");
        assert_eq!(status.as_deref(), Some("completed"));
    }

    #[test]
    fn codex_parses_error_as_error() {
        let status = CodexAdapter.parse_status_output("there was an error here");
        assert_eq!(status.as_deref(), Some("error"));
    }

    #[test]
    fn codex_parses_unrelated_output_as_none() {
        let status = CodexAdapter.parse_status_output("Working on it...");
        assert!(status.is_none());
    }

    // -- Status parsing: Gemini ---------------------------------------------

    #[test]
    fn gemini_parses_done_as_completed() {
        let status = GeminiAdapter.parse_status_output("Done with task.");
        assert_eq!(status.as_deref(), Some("completed"));
    }

    #[test]
    fn gemini_parses_complete_as_completed() {
        let status = GeminiAdapter.parse_status_output("Complete.");
        assert_eq!(status.as_deref(), Some("completed"));
    }

    #[test]
    fn gemini_parses_error_capital_as_error() {
        let status = GeminiAdapter.parse_status_output("Error happened");
        assert_eq!(status.as_deref(), Some("error"));
    }

    #[test]
    fn gemini_parses_unrelated_output_as_none() {
        let status = GeminiAdapter.parse_status_output("hello world");
        assert!(status.is_none());
    }

    // -- Status parsing: OpenCode -------------------------------------------

    #[test]
    fn opencode_parses_done_as_completed() {
        let status = OpenCodeAdapter.parse_status_output("done");
        assert_eq!(status.as_deref(), Some("completed"));
    }

    #[test]
    fn opencode_parses_complete_as_completed() {
        let status = OpenCodeAdapter.parse_status_output("step is complete now");
        assert_eq!(status.as_deref(), Some("completed"));
    }

    #[test]
    fn opencode_parses_error_lowercase_as_error() {
        let status = OpenCodeAdapter.parse_status_output("error: oh no");
        assert_eq!(status.as_deref(), Some("error"));
    }

    #[test]
    fn opencode_parses_error_capital_as_error() {
        let status = OpenCodeAdapter.parse_status_output("Error: caught it");
        assert_eq!(status.as_deref(), Some("error"));
    }

    #[test]
    fn opencode_parses_unrelated_output_as_none() {
        let status = OpenCodeAdapter.parse_status_output("running...");
        assert!(status.is_none());
    }

    // -- Precedence: completion checks before error -------------------------

    #[test]
    fn claude_completion_takes_priority_over_error() {
        // Claude's parser checks "Task complete" or "Done!" first; if both
        // markers are present, the implementation prefers "completed".
        let status = ClaudeAdapter.parse_status_output("Done! No Error here.");
        assert_eq!(status.as_deref(), Some("completed"));
    }

    #[test]
    fn codex_completion_takes_priority_over_error() {
        let status = CodexAdapter.parse_status_output("completed with no error");
        assert_eq!(status.as_deref(), Some("completed"));
    }

    // -- Spawn error path: spawning into a full pool returns AtCapacity -----
    //
    // We can exercise the *capacity* path of every adapter without ever
    // touching a real PTY: fill the pool with sentinel UUIDs by spawning
    // into a 0-capacity pool, which fails before openpty() is called.

    #[tokio::test]
    async fn claude_spawn_into_full_pool_returns_at_capacity() {
        let pool = PtyPool::new(0);
        let result = ClaudeAdapter.spawn(&pool, "task", "/tmp").await;
        assert!(matches!(
            result,
            Err(crate::pty_pool::PtyError::AtCapacity { max: 0 })
        ));
    }

    #[tokio::test]
    async fn codex_spawn_into_full_pool_returns_at_capacity() {
        let pool = PtyPool::new(0);
        let result = CodexAdapter.spawn(&pool, "task", "/tmp").await;
        assert!(matches!(
            result,
            Err(crate::pty_pool::PtyError::AtCapacity { max: 0 })
        ));
    }

    #[tokio::test]
    async fn gemini_spawn_into_full_pool_returns_at_capacity() {
        let pool = PtyPool::new(0);
        let result = GeminiAdapter.spawn(&pool, "task", "/tmp").await;
        assert!(matches!(
            result,
            Err(crate::pty_pool::PtyError::AtCapacity { max: 0 })
        ));
    }

    #[tokio::test]
    async fn opencode_spawn_into_full_pool_returns_at_capacity() {
        let pool = PtyPool::new(0);
        let result = OpenCodeAdapter.spawn(&pool, "task", "/tmp").await;
        assert!(matches!(
            result,
            Err(crate::pty_pool::PtyError::AtCapacity { max: 0 })
        ));
    }

    // -- Spawn error path: nonexistent binary returns SpawnFailed -----------
    //
    // We make this hermetic by clearing PATH for the duration of the test so
    // the spawn cannot resolve `claude` (or any other binary) to an
    // executable. Without this, on a host where `claude` happens to be on
    // PATH the test would silently spawn a real interactive CLI inside a
    // PTY that can hang or leak processes.

    /// Serialize tests that mutate process-global env vars (PATH).
    static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // Holding a std::sync::MutexGuard across `.await` is intentional here —
    // the lock guards a process-wide mutation (PATH) that must remain in
    // effect while the adapter spawns. This test is sequential (no
    // concurrent awaits inside it) so the lint's deadlock concern does not
    // apply.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn claude_spawn_with_missing_binary_fails_cleanly() {
        let _guard = PATH_LOCK.lock().unwrap();
        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", "/nonexistent-path-for-cli-adapter-test");

        let pool = PtyPool::new(4);
        let result = ClaudeAdapter.spawn(&pool, "task", "/tmp").await;

        // Restore PATH before any panicking assertion.
        match original_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }

        let err = result.expect_err("expected SpawnFailed when binary is unreachable");
        assert!(
            matches!(err, crate::pty_pool::PtyError::SpawnFailed(_)),
            "expected SpawnFailed, got: {err:?}"
        );
    }
}
