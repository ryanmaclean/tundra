//! Managed Agents executor — cloud-offload via Anthropic's hosted agent runtime.
//!
//! Implements `PtySpawner` so it can slot in as an alternative backend to the
//! local PTY executor for tasks requiring network egress or cloud containers.
//!
//! # Status
//! STUB — not wired into production routing. Enable per-task via
//! `AgentConfig.executor_kind = ExecutorKind::Managed` (not yet defined).
//!
//! # Activation
//! Set env var `AT_MANAGED_AGENTS_TOKEN` to an Anthropic API key with the
//! `managed-agents-2026-04-01` beta feature enabled.

use std::fmt;

use crate::executor::{ExecutorError, PtySpawner, SpawnedProcess};

/// Cloud-offload executor. Implements `PtySpawner` so `AgentExecutor`
/// can use it without changes to the orchestration layer.
pub struct ManagedAgentsExecutor {
    /// Anthropic API key with managed-agents-2026-04-01 beta enabled.
    /// Never derived via Debug — use masked_key() for display.
    api_key: String,
    /// Target model for the managed session.
    model: String,
}

impl fmt::Debug for ManagedAgentsExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagedAgentsExecutor")
            .field("api_key", &self.masked_key())
            .field("model", &self.model)
            .finish()
    }
}

impl ManagedAgentsExecutor {
    /// Create a new executor. Reads `AT_MANAGED_AGENTS_TOKEN` from env.
    pub fn from_env() -> Result<Self, ExecutorError> {
        let api_key = std::env::var("AT_MANAGED_AGENTS_TOKEN").map_err(|_| {
            ExecutorError::PtyPool(
                "AT_MANAGED_AGENTS_TOKEN not set — Managed Agents requires an Anthropic API key \
                 with the managed-agents-2026-04-01 beta feature enabled"
                    .into(),
            )
        })?;
        Ok(Self {
            api_key,
            model: "claude-sonnet-4-6".into(),
        })
    }

    /// Return the configured model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Return the configured API key (masked for display).
    pub fn masked_key(&self) -> String {
        if self.api_key.len() <= 8 {
            "***".into()
        } else {
            format!("{}***", &self.api_key[..4])
        }
    }
}

impl PtySpawner for ManagedAgentsExecutor {
    fn spawn(
        &self,
        _cmd: &str,
        _args: &[&str],
        _env: &[(&str, &str)],
    ) -> std::result::Result<SpawnedProcess, String> {
        // STUB — real implementation will POST to Anthropic's managed-agents
        // endpoint using self.api_key, creating a remote agent session and
        // bridging its stdio over channels returned as SpawnedProcess.
        Err("Managed Agents stub — not yet implemented".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Serialise all env-var tests through a process-wide mutex.
    // std::env::set_var / remove_var mutate shared global state — without this
    // guard, parallel test threads race and produce spurious failures.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn from_env_errors_without_token() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::remove_var("AT_MANAGED_AGENTS_TOKEN");
        let result = ManagedAgentsExecutor::from_env();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("AT_MANAGED_AGENTS_TOKEN"));
    }

    #[test]
    fn from_env_succeeds_with_token() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::set_var("AT_MANAGED_AGENTS_TOKEN", "sk-ant-test1234567890");
        let exec = ManagedAgentsExecutor::from_env().expect("should succeed with token set");
        assert_eq!(exec.model(), "claude-sonnet-4-6");
        assert!(exec.masked_key().ends_with("***"));
        // Debug output must NOT contain the raw key
        let dbg = format!("{:?}", exec);
        assert!(
            !dbg.contains("sk-ant-test1234567890"),
            "api_key leaked in Debug: {dbg}"
        );
        std::env::remove_var("AT_MANAGED_AGENTS_TOKEN");
    }

    #[test]
    fn spawn_returns_stub_error() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::set_var("AT_MANAGED_AGENTS_TOKEN", "sk-ant-test1234567890");
        let exec = ManagedAgentsExecutor::from_env().unwrap();
        let result = exec.spawn("claude", &["--version"], &[]);
        let err_msg = result.map(|_| ()).unwrap_err();
        assert!(
            err_msg.contains("Managed Agents stub"),
            "unexpected: {err_msg}"
        );
        std::env::remove_var("AT_MANAGED_AGENTS_TOKEN");
    }

    #[test]
    fn debug_output_masks_api_key() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::set_var("AT_MANAGED_AGENTS_TOKEN", "sk-ant-supersecret12345");
        let exec = ManagedAgentsExecutor::from_env().unwrap();
        let debug_str = format!("{exec:?}");
        assert!(
            !debug_str.contains("supersecret"),
            "key leaked: {debug_str}"
        );
        assert!(debug_str.contains("***"), "mask missing: {debug_str}");
        std::env::remove_var("AT_MANAGED_AGENTS_TOKEN");
    }
}
