//! Runtime abstraction for Claude-backed agent sessions.
//!
//! This is the integration seam for future `claude-sdk-rs` transport wiring:
//! the orchestration layer can depend on this trait while implementations can
//! be swapped (native manager, SDK-backed runtime, mock runtime).

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::claude_session::{ClaudeSessionManager, SessionConfig, SessionError, SessionId};
use at_intelligence::llm::LlmResponse;

#[async_trait]
pub trait ClaudeRuntime: Send + Sync {
    async fn start_session(&self, config: SessionConfig) -> Result<SessionId, SessionError>;
    async fn send(&self, session_id: SessionId, message: String) -> Result<LlmResponse, SessionError>;
    async fn close_session(&self, session_id: SessionId) -> bool;
    async fn list_sessions(&self) -> Vec<SessionId>;
}

/// Default runtime backed by the existing `ClaudeSessionManager`.
pub struct ManagerClaudeRuntime {
    manager: Mutex<ClaudeSessionManager>,
}

impl ManagerClaudeRuntime {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            manager: Mutex::new(ClaudeSessionManager::new(api_key)),
        }
    }

    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            manager: Mutex::new(ClaudeSessionManager::with_base_url(api_key, base_url)),
        }
    }
}

#[async_trait]
impl ClaudeRuntime for ManagerClaudeRuntime {
    async fn start_session(&self, config: SessionConfig) -> Result<SessionId, SessionError> {
        let mut mgr = self.manager.lock().await;
        Ok(mgr.create_session(config))
    }

    async fn send(&self, session_id: SessionId, message: String) -> Result<LlmResponse, SessionError> {
        let mut mgr = self.manager.lock().await;
        mgr.send_message(&session_id, message).await
    }

    async fn close_session(&self, session_id: SessionId) -> bool {
        let mut mgr = self.manager.lock().await;
        mgr.remove_session(&session_id).is_some()
    }

    async fn list_sessions(&self) -> Vec<SessionId> {
        let mgr = self.manager.lock().await;
        mgr.list_sessions()
    }
}

#[cfg(test)]
mod tests {
    use super::{ClaudeRuntime, ManagerClaudeRuntime};
    use crate::claude_session::SessionConfig;

    #[tokio::test]
    async fn manager_runtime_starts_and_lists_sessions() {
        let runtime = ManagerClaudeRuntime::new("test-key");
        let id = runtime.start_session(SessionConfig::default()).await.unwrap();
        let sessions = runtime.list_sessions().await;
        assert!(sessions.contains(&id));
    }

    #[tokio::test]
    async fn manager_runtime_close_session() {
        let runtime = ManagerClaudeRuntime::new("test-key");
        let id = runtime.start_session(SessionConfig::default()).await.unwrap();
        assert!(runtime.close_session(id).await);
        assert!(!runtime.close_session(id).await);
    }
}
