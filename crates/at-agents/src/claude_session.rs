//! Claude session management for agent-driven interactions.
//!
//! Provides a `ClaudeSession` that wraps the Anthropic LLM provider with
//! multi-turn conversation state management: message history, system prompt
//! injection, and session lifecycle. This replaces the need for an external
//! claude-sdk-rs dependency while using the same LLM provider infrastructure.
//!
//! # Architecture
//!
//! ```text
//! ClaudeSessionManager
//!   ├── sessions: HashMap<SessionId, ClaudeSession>
//!   └── provider: AnthropicProvider (from at-intelligence)
//!
//! ClaudeSession
//!   ├── id: Uuid
//!   ├── messages: Vec<LlmMessage>  (conversation history)
//!   ├── config: LlmConfig          (model, temp, max_tokens)
//!   └── metadata: SessionMetadata   (created_at, turn_count, tokens_used)
//! ```
//!
//! # Usage
//!
//! ```no_run
//! use at_agents::claude_session::{ClaudeSessionManager, SessionConfig};
//!
//! let mut manager = ClaudeSessionManager::new("api-key");
//! let session_id = manager.create_session(SessionConfig::default());
//! // manager.send_message(session_id, "Hello").await;
//! ```

use std::collections::HashMap;

use at_intelligence::llm::{
    AnthropicProvider, LlmConfig, LlmError, LlmMessage, LlmProvider, LlmResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Session types
// ---------------------------------------------------------------------------

/// Unique identifier for a Claude session.
pub type SessionId = Uuid;

/// Configuration for creating a new Claude session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Model to use (e.g., "claude-sonnet-4-20250514").
    pub model: String,
    /// System prompt injected at the start of every request.
    pub system_prompt: Option<String>,
    /// Maximum tokens per response.
    pub max_tokens: u32,
    /// Temperature for generation (0.0 = deterministic, 1.0 = creative).
    pub temperature: f32,
    /// Maximum number of messages to keep in history (older messages are
    /// dropped to stay within context limits). 0 = unlimited.
    pub max_history_messages: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            system_prompt: None,
            max_tokens: 4096,
            temperature: 0.3,
            max_history_messages: 50,
        }
    }
}

/// Metadata about a session's lifecycle and usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub turn_count: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

impl SessionMetadata {
    fn new() -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            last_active_at: now,
            turn_count: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    fn record_turn(&mut self, response: &LlmResponse) {
        self.last_active_at = Utc::now();
        self.turn_count += 1;
        self.total_input_tokens += response.input_tokens;
        self.total_output_tokens += response.output_tokens;
    }

    /// Total tokens consumed across all turns.
    pub fn total_tokens(&self) -> u64 {
        self.total_input_tokens + self.total_output_tokens
    }
}

/// A single Claude conversation session with message history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSession {
    pub id: SessionId,
    pub config: SessionConfig,
    pub messages: Vec<LlmMessage>,
    pub metadata: SessionMetadata,
}

impl ClaudeSession {
    /// Create a new session with the given configuration.
    pub fn new(config: SessionConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            config,
            messages: Vec::new(),
            metadata: SessionMetadata::new(),
        }
    }

    /// Add a user message to the conversation history.
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.messages.push(LlmMessage::user(content));
    }

    /// Add an assistant response to the conversation history.
    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.messages.push(LlmMessage::assistant(content));
    }

    /// Build an LlmConfig from the session configuration.
    fn llm_config(&self) -> LlmConfig {
        LlmConfig {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
            system_prompt: self.config.system_prompt.clone(),
        }
    }

    /// Trim history to stay within the configured limit.
    fn trim_history(&mut self) {
        if self.config.max_history_messages > 0
            && self.messages.len() > self.config.max_history_messages
        {
            let excess = self.messages.len() - self.config.max_history_messages;
            self.messages.drain(..excess);
        }
    }

    /// Clear the conversation history while preserving session identity.
    pub fn clear_history(&mut self) {
        self.messages.clear();
    }

    /// Number of messages in the conversation.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}

// ---------------------------------------------------------------------------
// ClaudeSessionManager
// ---------------------------------------------------------------------------

/// Manages multiple Claude conversation sessions.
///
/// Each session maintains its own message history and configuration. The
/// manager holds a shared `AnthropicProvider` for making API calls.
pub struct ClaudeSessionManager {
    sessions: HashMap<SessionId, ClaudeSession>,
    provider: AnthropicProvider,
}

impl ClaudeSessionManager {
    /// Create a new session manager with the given Anthropic API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            sessions: HashMap::new(),
            provider: AnthropicProvider::new(api_key),
        }
    }

    /// Create a session manager with a custom base URL (for testing or proxies).
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            sessions: HashMap::new(),
            provider: AnthropicProvider::new(api_key).with_base_url(base_url),
        }
    }

    /// Create a new conversation session and return its ID.
    pub fn create_session(&mut self, config: SessionConfig) -> SessionId {
        let session = ClaudeSession::new(config);
        let id = session.id;
        self.sessions.insert(id, session);
        id
    }

    /// Get a reference to a session by ID.
    pub fn get_session(&self, id: &SessionId) -> Option<&ClaudeSession> {
        self.sessions.get(id)
    }

    /// Get a mutable reference to a session by ID.
    pub fn get_session_mut(&mut self, id: &SessionId) -> Option<&mut ClaudeSession> {
        self.sessions.get_mut(id)
    }

    /// List all active session IDs.
    pub fn list_sessions(&self) -> Vec<SessionId> {
        self.sessions.keys().copied().collect()
    }

    /// Remove a session by ID.
    pub fn remove_session(&mut self, id: &SessionId) -> Option<ClaudeSession> {
        self.sessions.remove(id)
    }

    /// Send a user message to a session and get the assistant's response.
    ///
    /// This appends the user message, calls the Anthropic API with the full
    /// conversation history, appends the assistant's response, and returns it.
    pub async fn send_message(
        &mut self,
        session_id: &SessionId,
        message: impl Into<String>,
    ) -> Result<LlmResponse, SessionError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or(SessionError::NotFound(*session_id))?;

        // Add user message to history.
        session.add_user_message(message);

        // Build request config.
        let config = session.llm_config();

        // Call the provider with full history.
        let response = self
            .provider
            .complete(&session.messages, &config)
            .await
            .map_err(SessionError::Llm)?;

        // Append assistant response to history.
        session.add_assistant_message(&response.content);

        // Record usage metrics.
        session.metadata.record_turn(&response);

        // Trim history if needed.
        session.trim_history();

        Ok(response)
    }

    /// Number of active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during session operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("session not found: {0}")]
    NotFound(SessionId),

    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use at_intelligence::llm::LlmRole;

    #[test]
    fn session_config_default() {
        let config = SessionConfig::default();
        assert_eq!(config.model, "claude-sonnet-4-20250514");
        assert_eq!(config.max_tokens, 4096);
        assert!(config.temperature > 0.0);
        assert!(config.system_prompt.is_none());
        assert_eq!(config.max_history_messages, 50);
    }

    #[test]
    fn session_creation() {
        let session = ClaudeSession::new(SessionConfig::default());
        assert_eq!(session.message_count(), 0);
        assert_eq!(session.metadata.turn_count, 0);
        assert_eq!(session.metadata.total_tokens(), 0);
    }

    #[test]
    fn session_add_messages() {
        let mut session = ClaudeSession::new(SessionConfig::default());
        session.add_user_message("Hello");
        session.add_assistant_message("Hi there!");
        session.add_user_message("How are you?");

        assert_eq!(session.message_count(), 3);
        assert_eq!(session.messages[0].role, LlmRole::User);
        assert_eq!(session.messages[0].content, "Hello");
        assert_eq!(session.messages[1].role, LlmRole::Assistant);
        assert_eq!(session.messages[2].role, LlmRole::User);
    }

    #[test]
    fn session_clear_history() {
        let mut session = ClaudeSession::new(SessionConfig::default());
        session.add_user_message("Hello");
        session.add_assistant_message("Hi");
        assert_eq!(session.message_count(), 2);

        session.clear_history();
        assert_eq!(session.message_count(), 0);
    }

    #[test]
    fn session_trim_history() {
        let mut session = ClaudeSession::new(SessionConfig {
            max_history_messages: 3,
            ..SessionConfig::default()
        });

        for i in 0..5 {
            session.add_user_message(format!("msg-{}", i));
        }
        assert_eq!(session.message_count(), 5);

        session.trim_history();
        assert_eq!(session.message_count(), 3);
        // Oldest messages should be dropped.
        assert_eq!(session.messages[0].content, "msg-2");
        assert_eq!(session.messages[1].content, "msg-3");
        assert_eq!(session.messages[2].content, "msg-4");
    }

    #[test]
    fn session_trim_history_unlimited() {
        let mut session = ClaudeSession::new(SessionConfig {
            max_history_messages: 0, // unlimited
            ..SessionConfig::default()
        });

        for i in 0..100 {
            session.add_user_message(format!("msg-{}", i));
        }
        session.trim_history();
        assert_eq!(session.message_count(), 100); // nothing trimmed
    }

    #[test]
    fn session_llm_config() {
        let session = ClaudeSession::new(SessionConfig {
            model: "claude-opus-4-20250514".into(),
            system_prompt: Some("Be helpful".into()),
            max_tokens: 2048,
            temperature: 0.5,
            max_history_messages: 20,
        });

        let config = session.llm_config();
        assert_eq!(config.model, "claude-opus-4-20250514");
        assert_eq!(config.max_tokens, 2048);
        assert!((config.temperature - 0.5).abs() < f32::EPSILON);
        assert_eq!(config.system_prompt.as_deref(), Some("Be helpful"));
    }

    #[test]
    fn session_metadata_recording() {
        let mut metadata = SessionMetadata::new();
        assert_eq!(metadata.turn_count, 0);

        let response = LlmResponse {
            content: "test".into(),
            model: "test".into(),
            input_tokens: 100,
            output_tokens: 50,
            finish_reason: "end_turn".into(),
        };
        metadata.record_turn(&response);

        assert_eq!(metadata.turn_count, 1);
        assert_eq!(metadata.total_input_tokens, 100);
        assert_eq!(metadata.total_output_tokens, 50);
        assert_eq!(metadata.total_tokens(), 150);
    }

    #[test]
    fn manager_create_and_get_session() {
        let mut manager = ClaudeSessionManager::new("test-key");
        assert_eq!(manager.session_count(), 0);

        let id = manager.create_session(SessionConfig::default());
        assert_eq!(manager.session_count(), 1);

        let session = manager.get_session(&id).unwrap();
        assert_eq!(session.id, id);
    }

    #[test]
    fn manager_list_sessions() {
        let mut manager = ClaudeSessionManager::new("test-key");
        let id1 = manager.create_session(SessionConfig::default());
        let id2 = manager.create_session(SessionConfig::default());

        let ids = manager.list_sessions();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[test]
    fn manager_remove_session() {
        let mut manager = ClaudeSessionManager::new("test-key");
        let id = manager.create_session(SessionConfig::default());

        let removed = manager.remove_session(&id);
        assert!(removed.is_some());
        assert_eq!(manager.session_count(), 0);
        assert!(manager.get_session(&id).is_none());
    }

    #[test]
    fn manager_remove_nonexistent() {
        let mut manager = ClaudeSessionManager::new("test-key");
        let fake_id = Uuid::new_v4();
        assert!(manager.remove_session(&fake_id).is_none());
    }

    #[test]
    fn manager_with_base_url() {
        let manager = ClaudeSessionManager::with_base_url("test-key", "http://localhost:9999");
        assert_eq!(manager.session_count(), 0);
    }

    #[test]
    fn session_serialization_roundtrip() {
        let mut session = ClaudeSession::new(SessionConfig {
            model: "test-model".into(),
            system_prompt: Some("Be helpful".into()),
            max_tokens: 1024,
            temperature: 0.7,
            max_history_messages: 10,
        });
        session.add_user_message("Hello");
        session.add_assistant_message("Hi!");

        let json = serde_json::to_string(&session).unwrap();
        let deser: ClaudeSession = serde_json::from_str(&json).unwrap();

        assert_eq!(deser.id, session.id);
        assert_eq!(deser.config.model, "test-model");
        assert_eq!(deser.message_count(), 2);
        assert_eq!(deser.messages[0].content, "Hello");
        assert_eq!(deser.messages[1].content, "Hi!");
    }

    #[test]
    fn session_error_display() {
        let id = Uuid::new_v4();
        let err = SessionError::NotFound(id);
        assert!(err.to_string().contains("session not found"));

        let llm_err = SessionError::Llm(LlmError::Timeout);
        assert!(llm_err.to_string().contains("timed out"));
    }
}
