//! Agent tools exposed to rig-core agents.
//!
//! Every tool holds a [`ToolContext`] and checks it before it touches the
//! host:
//!
//! - **Workspace root.** Paths (and `shell_exec`'s `cwd`) are resolved
//!   against the root and canonicalized. Anything that resolves outside it,
//!   including through a symlink, is rejected.
//! - **`security.allow_shell_exec`.** `shell_exec` refuses to run unless the
//!   at-core [`SecurityConfig`] allows it (the default is `false`).
//! - **[`ToolApprovalSystem`].** `read_file` is checked as `file_read` and
//!   `shell_exec` as `shell_execute` for the context's [`AgentRole`]. Only
//!   `AutoApprove` runs. `RequireApproval` fails closed because a headless
//!   tool call has nobody to wait on, and `Deny` is refused.
//! - **Resource limits.** `read_file` caps the size it reads, and `shell_exec`
//!   runs on `tokio::process` with a timeout. On timeout the child is killed.
//!
//! # Usage with an Agent
//! ```ignore
//! use rig_core::providers::anthropic;
//! let ctx = ToolContext::new(&worktree, &config.security, executor.approval_system().clone(), AgentRole::Coder)?;
//! let client = anthropic::Client::from_env();
//! let agent = client
//!     .agent("claude-sonnet-4-6")
//!     .tool(ReadFileTool::new(ctx.clone()))
//!     .tool(ShellExecTool::new(ctx))
//!     .build();
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use at_core::config::SecurityConfig;
use at_core::types::AgentRole;
use rig_core::completion::ToolDefinition;
use rig_core::tool::{Tool, ToolError};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::approval::{ApprovalPolicy, ToolApprovalSystem};

/// Default wall-clock limit for one `shell_exec` call.
pub const DEFAULT_SHELL_TIMEOUT: Duration = Duration::from_secs(120);
/// Default largest file `read_file` will return (1 MiB).
pub const DEFAULT_MAX_READ_BYTES: u64 = 1024 * 1024;

fn tool_err(msg: impl Into<String>) -> ToolError {
    ToolError::ToolCallError(msg.into().into())
}

/// Policy and sandbox shared by the agent tools.
#[derive(Clone)]
pub struct ToolContext {
    /// Canonical workspace root; every path must stay under it.
    root: PathBuf,
    allow_shell_exec: bool,
    approval: Arc<Mutex<ToolApprovalSystem>>,
    role: AgentRole,
    shell_timeout: Duration,
    max_read_bytes: u64,
}

impl ToolContext {
    /// Build a context rooted at `workspace_root` (usually the task worktree).
    ///
    /// Fails if the root does not exist or cannot be canonicalized.
    pub fn new(
        workspace_root: impl AsRef<Path>,
        security: &SecurityConfig,
        approval: Arc<Mutex<ToolApprovalSystem>>,
        role: AgentRole,
    ) -> std::io::Result<Self> {
        Ok(Self {
            root: std::fs::canonicalize(workspace_root)?,
            allow_shell_exec: security.allow_shell_exec,
            approval,
            role,
            shell_timeout: DEFAULT_SHELL_TIMEOUT,
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
        })
    }

    /// Override the `shell_exec` timeout.
    pub fn with_shell_timeout(mut self, timeout: Duration) -> Self {
        self.shell_timeout = timeout;
        self
    }

    /// Override the `read_file` size cap.
    pub fn with_max_read_bytes(mut self, max: u64) -> Self {
        self.max_read_bytes = max;
        self
    }

    /// The canonical workspace root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Consult the approval system; only `AutoApprove` may proceed.
    async fn gate(&self, policy_tool: &str) -> Result<(), ToolError> {
        let policy = self
            .approval
            .lock()
            .await
            .check_approval(policy_tool, &self.role);
        match policy {
            ApprovalPolicy::AutoApprove => Ok(()),
            ApprovalPolicy::RequireApproval => Err(tool_err(format!(
                "{policy_tool} requires approval for role {:?}; headless tool calls cannot wait for it",
                self.role
            ))),
            ApprovalPolicy::Deny => Err(tool_err(format!(
                "{policy_tool} is denied for role {:?}",
                self.role
            ))),
        }
    }

    /// Resolve `path` (relative to the root, or absolute) and require that the
    /// canonical result lies inside the root.
    async fn resolve_within(&self, path: &str) -> Result<PathBuf, ToolError> {
        let candidate = Path::new(path);
        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.root.join(candidate)
        };
        let canonical = tokio::fs::canonicalize(&joined)
            .await
            .map_err(|e| tool_err(format!("cannot resolve {path}: {e}")))?;
        if !canonical.starts_with(&self.root) {
            return Err(tool_err(format!(
                "{path} is outside the workspace root {}",
                self.root.display()
            )));
        }
        Ok(canonical)
    }
}

// ---------------------------------------------------------------------------
// read_file
// ---------------------------------------------------------------------------

/// Arguments for [`ReadFileTool`].
#[derive(Debug, Deserialize)]
pub struct ReadFileArgs {
    /// Path relative to the workspace root (absolute paths must be inside it).
    pub path: String,
}

/// Read a UTF-8 file inside the workspace root.
#[derive(Clone)]
pub struct ReadFileTool {
    ctx: ToolContext,
}

impl ReadFileTool {
    pub fn new(ctx: ToolContext) -> Self {
        Self { ctx }
    }
}

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    type Error = ToolError;
    type Args = ReadFileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Read a UTF-8 file from the workspace".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the workspace root"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<String, ToolError> {
        self.ctx.gate("file_read").await?;
        let path = self.ctx.resolve_within(&args.path).await?;
        let meta = tokio::fs::metadata(&path)
            .await
            .map_err(|e| tool_err(format!("read_file failed for {}: {e}", args.path)))?;
        if !meta.is_file() {
            return Err(tool_err(format!("{} is not a regular file", args.path)));
        }
        if meta.len() > self.ctx.max_read_bytes {
            return Err(tool_err(format!(
                "{} is {} bytes, over the {} byte limit",
                args.path,
                meta.len(),
                self.ctx.max_read_bytes
            )));
        }
        tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| tool_err(format!("read_file failed for {}: {e}", args.path)))
    }
}

// ---------------------------------------------------------------------------
// shell_exec
// ---------------------------------------------------------------------------

/// Arguments for [`ShellExecTool`].
#[derive(Debug, Deserialize)]
pub struct ShellExecArgs {
    /// The shell command to run (`sh -c`).
    pub command: String,
    /// Working directory inside the workspace (defaults to the root).
    #[serde(default)]
    pub cwd: Option<String>,
}

/// Run a shell command inside the workspace root, gated by
/// `security.allow_shell_exec` and the approval system, with a timeout.
#[derive(Clone)]
pub struct ShellExecTool {
    ctx: ToolContext,
}

impl ShellExecTool {
    pub fn new(ctx: ToolContext) -> Self {
        Self { ctx }
    }
}

impl Tool for ShellExecTool {
    const NAME: &'static str = "shell_exec";
    type Error = ToolError;
    type Args = ShellExecArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Run a shell command in the agent workspace".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to run"
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Working directory relative to the workspace root (defaults to the root)"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<String, ToolError> {
        if !self.ctx.allow_shell_exec {
            return Err(tool_err(
                "shell_exec is disabled (security.allow_shell_exec = false)",
            ));
        }
        self.ctx.gate("shell_execute").await?;
        let cwd = self
            .ctx
            .resolve_within(args.cwd.as_deref().unwrap_or("."))
            .await?;

        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg(&args.command)
            .current_dir(&cwd)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let command = &args.command;
        // Dropping the `output()` future on timeout drops the child, and
        // `kill_on_drop` kills it.
        let output = tokio::time::timeout(self.ctx.shell_timeout, cmd.output())
            .await
            .map_err(|_| {
                tool_err(format!(
                    "shell_exec '{command}' timed out after {:?}",
                    self.ctx.shell_timeout
                ))
            })?
            .map_err(|e| tool_err(format!("shell_exec failed to spawn '{command}': {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if output.status.success() {
            Ok(stdout)
        } else {
            Err(tool_err(format!(
                "shell_exec '{command}' exited with {}: {stderr}",
                output.status
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn security(allow_shell_exec: bool) -> SecurityConfig {
        SecurityConfig {
            allow_shell_exec,
            ..SecurityConfig::default()
        }
    }

    /// Context whose approval system auto-approves both tools.
    fn ctx(root: &Path, allow_shell_exec: bool) -> ToolContext {
        let mut approval = ToolApprovalSystem::new();
        approval.set_policy("file_read", ApprovalPolicy::AutoApprove);
        approval.set_policy("shell_execute", ApprovalPolicy::AutoApprove);
        ToolContext::new(
            root,
            &security(allow_shell_exec),
            Arc::new(Mutex::new(approval)),
            AgentRole::Coder,
        )
        .unwrap()
    }

    fn read(path: &str) -> ReadFileArgs {
        ReadFileArgs { path: path.into() }
    }

    fn sh(command: &str, cwd: Option<&str>) -> ShellExecArgs {
        ShellExecArgs {
            command: command.into(),
            cwd: cwd.map(Into::into),
        }
    }

    #[tokio::test]
    async fn read_file_reads_inside_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello tools").unwrap();
        let tool = ReadFileTool::new(ctx(dir.path(), false));
        assert_eq!(tool.call(read("test.txt")).await.unwrap(), "hello tools");
        let abs = dir.path().join("test.txt");
        assert_eq!(
            tool.call(read(&abs.to_string_lossy())).await.unwrap(),
            "hello tools"
        );
    }

    #[tokio::test]
    async fn read_file_missing_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ReadFileTool::new(ctx(dir.path(), false));
        assert!(tool.call(read("nope.txt")).await.is_err());
    }

    #[tokio::test]
    async fn read_file_rejects_paths_outside_root() {
        let outer = tempfile::tempdir().unwrap();
        std::fs::write(outer.path().join("secret.txt"), "secret").unwrap();
        let root = outer.path().join("ws");
        std::fs::create_dir(&root).unwrap();
        let tool = ReadFileTool::new(ctx(&root, false));

        let dotdot = tool.call(read("../secret.txt")).await.unwrap_err();
        assert!(
            dotdot.to_string().contains("outside the workspace"),
            "{dotdot}"
        );

        let abs = outer.path().join("secret.txt");
        let abs_err = tool.call(read(&abs.to_string_lossy())).await.unwrap_err();
        assert!(abs_err.to_string().contains("outside the workspace"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn read_file_rejects_symlink_escape() {
        let outer = tempfile::tempdir().unwrap();
        std::fs::write(outer.path().join("secret.txt"), "secret").unwrap();
        let root = outer.path().join("ws");
        std::fs::create_dir(&root).unwrap();
        std::os::unix::fs::symlink(outer.path().join("secret.txt"), root.join("link")).unwrap();
        let tool = ReadFileTool::new(ctx(&root, false));
        assert!(tool.call(read("link")).await.is_err());
    }

    #[tokio::test]
    async fn read_file_enforces_size_cap() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("big.txt"), "0123456789").unwrap();
        let tool = ReadFileTool::new(ctx(dir.path(), false).with_max_read_bytes(4));
        let err = tool.call(read("big.txt")).await.unwrap_err();
        assert!(err.to_string().contains("limit"), "{err}");
    }

    #[tokio::test]
    async fn read_file_honours_deny_policy() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        let mut approval = ToolApprovalSystem::new();
        approval.set_policy("file_read", ApprovalPolicy::Deny);
        let ctx = ToolContext::new(
            dir.path(),
            &security(false),
            Arc::new(Mutex::new(approval)),
            AgentRole::Coder,
        )
        .unwrap();
        let err = ReadFileTool::new(ctx)
            .call(read("a.txt"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("denied"), "{err}");
    }

    #[tokio::test]
    async fn shell_exec_refused_when_allow_shell_exec_false() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("ran");
        let tool = ShellExecTool::new(ctx(dir.path(), false));
        let err = tool
            .call(sh(&format!("touch {}", marker.display()), None))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("allow_shell_exec"), "{err}");
        assert!(!marker.exists(), "command must not have run");
    }

    #[tokio::test]
    async fn shell_exec_default_policy_requires_approval_and_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ToolContext::new(
            dir.path(),
            &security(true),
            Arc::new(Mutex::new(ToolApprovalSystem::new())),
            AgentRole::Coder,
        )
        .unwrap();
        let err = ShellExecTool::new(ctx)
            .call(sh("echo hi", None))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("requires approval"), "{err}");
    }

    #[tokio::test]
    async fn shell_exec_success_runs_in_root() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ShellExecTool::new(ctx(dir.path(), true));
        let out = tool.call(sh("pwd -P", None)).await.unwrap();
        assert_eq!(
            Path::new(out.trim()),
            std::fs::canonicalize(dir.path()).unwrap()
        );
        assert_eq!(
            tool.call(sh("echo hello", None)).await.unwrap().trim(),
            "hello"
        );
    }

    #[tokio::test]
    async fn shell_exec_failure() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ShellExecTool::new(ctx(dir.path(), true));
        assert!(tool.call(sh("exit 1", None)).await.is_err());
    }

    #[tokio::test]
    async fn shell_exec_rejects_cwd_outside_root() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("ws");
        std::fs::create_dir(&root).unwrap();
        let tool = ShellExecTool::new(ctx(&root, true));
        let err = tool.call(sh("pwd", Some(".."))).await.unwrap_err();
        assert!(err.to_string().contains("outside the workspace"), "{err}");
        let err = tool.call(sh("pwd", Some("/"))).await.unwrap_err();
        assert!(err.to_string().contains("outside the workspace"), "{err}");
    }

    #[tokio::test]
    async fn shell_exec_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ShellExecTool::new(
            ctx(dir.path(), true).with_shell_timeout(Duration::from_millis(200)),
        );
        let start = std::time::Instant::now();
        let err = tool.call(sh("sleep 30", None)).await.unwrap_err();
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn definitions_name_the_tools() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), false);
        assert_eq!(
            ReadFileTool::new(c.clone())
                .definition(String::new())
                .await
                .name,
            "read_file"
        );
        assert_eq!(
            ShellExecTool::new(c).definition(String::new()).await.name,
            "shell_exec"
        );
    }
}
