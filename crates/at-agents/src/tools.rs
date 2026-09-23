//! Agent tool definitions using rig-core `#[rig_tool]` macro.
//!
//! rig-core 0.38.1 uses a function-level attribute macro (`#[rig_tool]`) rather
//! than a struct-level derive. Each function annotated with `#[rig_tool]` is
//! transformed into a type that implements `rig::tool::Tool`, with its JSON
//! schema generated at compile time from the function's parameter types via
//! `schemars`. This replaces hand-written JSON schema with compile-time-verified
//! schema generation.
//!
//! # Usage with an Agent
//! ```ignore
//! use rig_core::providers::anthropic;
//! let client = anthropic::Client::from_env();
//! let agent = client
//!     .agent("claude-sonnet-4-6")
//!     .tool(ReadFileTool)
//!     .tool(ShellExecTool)
//!     .build();
//! ```

use rig_core::tool::ToolError;

// Re-export the macro so call-sites can use `tools::rig_tool` if desired.
pub use rig_core::tool_macro;

/// Read a file from the agent sandbox filesystem.
///
/// Returns the UTF-8 contents of the file at `path`, or a `ToolError` if the
/// file cannot be read.
#[rig_core::tool_macro(
    description = "Read a file from the workspace",
    params(path = "Absolute path to the file to read")
)]
pub fn read_file(path: String) -> Result<String, ToolError> {
    std::fs::read_to_string(&path)
        .map_err(|e| ToolError::ToolCallError(format!("read_file failed for {path}: {e}").into()))
}

/// Execute a shell command in the agent sandbox.
///
/// Runs `command` in a subprocess, optionally with the working directory set
/// to `cwd` (defaults to the process's current directory). Returns combined
/// stdout+stderr, or a `ToolError` on non-zero exit or spawn failure.
#[rig_core::tool_macro(
    description = "Run a shell command in the agent workspace",
    params(
        command = "The shell command to run",
        cwd = "Working directory (defaults to the process current directory)"
    )
)]
pub fn shell_exec(command: String, cwd: Option<String>) -> Result<String, ToolError> {
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(&command);
    if let Some(dir) = &cwd {
        cmd.current_dir(dir);
    }
    let output = cmd.output().map_err(|e| {
        ToolError::ToolCallError(format!("shell_exec failed to spawn '{command}': {e}").into())
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        Err(ToolError::ToolCallError(
            format!(
                "shell_exec '{command}' exited with {}: {stderr}",
                output.status
            )
            .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_file_missing_returns_error() {
        let result = read_file("/nonexistent/path/that/does/not/exist".into());
        assert!(result.is_err());
    }

    #[test]
    fn read_file_reads_existing() {
        // Write a temp file and read it back.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "hello tools").unwrap();
        let result = read_file(path.to_string_lossy().into());
        assert_eq!(result.unwrap(), "hello tools");
    }

    #[test]
    fn shell_exec_success() {
        let result = shell_exec("echo hello".into(), None);
        assert_eq!(result.unwrap().trim(), "hello");
    }

    #[test]
    fn shell_exec_failure() {
        let result = shell_exec("exit 1".into(), None);
        assert!(result.is_err());
    }

    #[test]
    fn shell_exec_with_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let result = shell_exec("pwd".into(), Some(dir.path().to_string_lossy().into()));
        assert!(result.is_ok());
    }
}
