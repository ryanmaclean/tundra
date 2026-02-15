use at_core::types::CliType;
use async_trait::async_trait;

use crate::pty_pool::{PtyHandle, PtyPool, Result};

// ---------------------------------------------------------------------------
// CliAdapter trait
// ---------------------------------------------------------------------------

/// Trait for CLI tool adapters that know how to launch and interact with a
/// specific coding-agent CLI.
#[async_trait]
pub trait CliAdapter: Send + Sync {
    /// Which CLI type this adapter handles.
    fn cli_type(&self) -> CliType;

    /// The binary name / path for the CLI tool.
    fn binary_name(&self) -> &str;

    /// Default arguments that are always passed.
    fn default_args(&self) -> Vec<String>;

    /// Spawn the CLI inside a PTY from the given pool.
    async fn spawn(&self, pool: &PtyPool, task: &str, workdir: &str) -> Result<PtyHandle>;

    /// Attempt to extract a human-readable status string from raw CLI output.
    fn parse_status_output(&self, output: &str) -> Option<String>;
}

// ---------------------------------------------------------------------------
// Claude adapter
// ---------------------------------------------------------------------------

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

/// Create the appropriate adapter for a given CLI type.
pub fn adapter_for(cli_type: &CliType) -> Box<dyn CliAdapter> {
    match cli_type {
        CliType::Claude => Box::new(ClaudeAdapter),
        CliType::Codex => Box::new(CodexAdapter),
        CliType::Gemini => Box::new(GeminiAdapter),
        CliType::OpenCode => Box::new(OpenCodeAdapter),
    }
}
