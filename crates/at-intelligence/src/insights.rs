use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::IntelligenceError;

// ---------------------------------------------------------------------------
// LLM provider abstraction
// ---------------------------------------------------------------------------
// When Team Xi delivers crate::llm, replace this block with:
//   use crate::llm::{LlmProvider, LlmMessage, LlmResponse};
//
// For now we define a compatible local copy so the engine compiles and tests
// can exercise the AI path with a MockProvider.
// ---------------------------------------------------------------------------

/// Role for an LLM message (mirrors crate::llm::LlmMessage).
#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
}

/// Response from an LLM provider (mirrors crate::llm::LlmResponse).
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub model: String,
    pub usage: Option<LlmUsage>,
}

#[derive(Debug, Clone)]
pub struct LlmUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Trait implemented by LLM backends (mirrors crate::llm::LlmProvider).
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync + std::fmt::Debug {
    async fn complete(
        &self,
        messages: Vec<LlmMessage>,
        model: Option<&str>,
    ) -> Result<LlmResponse, Box<dyn std::error::Error + Send + Sync>>;
}

// ---------------------------------------------------------------------------
// ChatRole
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

// ---------------------------------------------------------------------------
// ChatMessage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// InsightsSession
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightsSession {
    pub id: Uuid,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub model: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// InsightsEngine
// ---------------------------------------------------------------------------

pub struct InsightsEngine {
    sessions: Vec<InsightsSession>,
    provider: Option<Arc<dyn LlmProvider>>,
}

// Manual Debug impl because `dyn LlmProvider` already requires Debug but
// Arc<dyn …> doesn't auto-derive Debug in all contexts.
impl std::fmt::Debug for InsightsEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InsightsEngine")
            .field("sessions", &self.sessions)
            .field("has_provider", &self.provider.is_some())
            .finish()
    }
}

impl InsightsEngine {
    /// Create an engine **without** an LLM provider.
    /// All sync methods work; AI-powered methods will return an error.
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            provider: None,
        }
    }

    /// Create an engine **with** an LLM provider for AI-powered chat.
    pub fn with_provider(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            sessions: Vec::new(),
            provider: Some(provider),
        }
    }

    pub fn create_session(&mut self, title: &str, model: &str) -> &InsightsSession {
        let session = InsightsSession {
            id: Uuid::new_v4(),
            title: title.to_string(),
            messages: Vec::new(),
            model: model.to_string(),
            created_at: Utc::now(),
        };
        self.sessions.push(session);
        self.sessions.last().unwrap()
    }

    pub fn list_sessions(&self) -> &[InsightsSession] {
        &self.sessions
    }

    pub fn get_session(&self, id: &Uuid) -> Option<&InsightsSession> {
        self.sessions.iter().find(|s| s.id == *id)
    }

    pub fn add_message(
        &mut self,
        session_id: &Uuid,
        role: ChatRole,
        content: &str,
    ) -> Result<(), IntelligenceError> {
        let session = self
            .sessions
            .iter_mut()
            .find(|s| s.id == *session_id)
            .ok_or(IntelligenceError::NotFound {
                entity: "session".into(),
                id: *session_id,
            })?;

        session.messages.push(ChatMessage {
            role,
            content: content.to_string(),
            timestamp: Utc::now(),
        });
        Ok(())
    }

    pub fn delete_session(&mut self, id: &Uuid) -> bool {
        let len_before = self.sessions.len();
        self.sessions.retain(|s| s.id != *id);
        self.sessions.len() < len_before
    }

    // -----------------------------------------------------------------------
    // AI-powered methods
    // -----------------------------------------------------------------------

    /// Send a user message and get an AI assistant response.
    ///
    /// This adds the user message to the session, builds the full
    /// conversation history, calls the LLM provider, and appends the
    /// assistant reply.  Returns the assistant's `ChatMessage`.
    pub async fn send_message_with_ai(
        &mut self,
        session_id: &Uuid,
        content: &str,
    ) -> Result<ChatMessage, IntelligenceError> {
        let provider = self
            .provider
            .as_ref()
            .ok_or_else(|| {
                IntelligenceError::InvalidOperation(
                    "No LLM provider configured – use InsightsEngine::with_provider()".into(),
                )
            })?
            .clone();

        // 1. Add the user message.
        self.add_message(session_id, ChatRole::User, content)?;

        // 2. Build the conversation history as LlmMessages.
        let session = self
            .sessions
            .iter()
            .find(|s| s.id == *session_id)
            .ok_or(IntelligenceError::NotFound {
                entity: "session".into(),
                id: *session_id,
            })?;

        let system_prompt = "You are an expert codebase exploration assistant. \
            Help the user understand code structure, patterns, dependencies, and \
            potential improvements. Be concise and precise.";

        let mut llm_messages = vec![LlmMessage {
            role: "system".to_string(),
            content: system_prompt.to_string(),
        }];

        for msg in &session.messages {
            let role = match msg.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::System => "system",
            };
            llm_messages.push(LlmMessage {
                role: role.to_string(),
                content: msg.content.clone(),
            });
        }

        // 3. Call the LLM.
        let model_hint = session.model.clone();
        let response = provider
            .complete(llm_messages, Some(&model_hint))
            .await
            .map_err(|e| IntelligenceError::InvalidOperation(format!("LLM call failed: {e}")))?;

        // 4. Append the assistant reply.
        let assistant_msg = ChatMessage {
            role: ChatRole::Assistant,
            content: response.content.clone(),
            timestamp: Utc::now(),
        };

        let session_mut = self
            .sessions
            .iter_mut()
            .find(|s| s.id == *session_id)
            .ok_or(IntelligenceError::NotFound {
                entity: "session".into(),
                id: *session_id,
            })?;
        session_mut.messages.push(assistant_msg.clone());

        Ok(assistant_msg)
    }
}

impl Default for InsightsEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // ---- MockProvider --------------------------------------------------------

    #[derive(Debug)]
    struct MockProvider {
        /// The canned response the mock returns.
        response: String,
        /// Captured calls for assertions.
        calls: Mutex<Vec<Vec<LlmMessage>>>,
    }

    impl MockProvider {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn captured_calls(&self) -> Vec<Vec<LlmMessage>> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(
            &self,
            messages: Vec<LlmMessage>,
            _model: Option<&str>,
        ) -> Result<LlmResponse, Box<dyn std::error::Error + Send + Sync>> {
            self.calls.lock().unwrap().push(messages);
            Ok(LlmResponse {
                content: self.response.clone(),
                model: "mock".to_string(),
                usage: None,
            })
        }
    }

    // ---- Tests ---------------------------------------------------------------

    #[tokio::test]
    async fn send_message_with_ai_builds_conversation_history() {
        let mock = Arc::new(MockProvider::new("I can help with that codebase."));
        let mut engine = InsightsEngine::with_provider(mock.clone());

        let session_id = engine.create_session("AI Chat", "claude-3").id;

        // First exchange
        let reply = engine
            .send_message_with_ai(&session_id, "Explain the module structure")
            .await
            .unwrap();

        assert_eq!(reply.role, ChatRole::Assistant);
        assert_eq!(reply.content, "I can help with that codebase.");

        // Verify the session now has user + assistant messages
        let session = engine.get_session(&session_id).unwrap();
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, ChatRole::User);
        assert_eq!(session.messages[0].content, "Explain the module structure");
        assert_eq!(session.messages[1].role, ChatRole::Assistant);

        // Second exchange – history should accumulate
        let _reply2 = engine
            .send_message_with_ai(&session_id, "Tell me more about errors")
            .await
            .unwrap();

        let session = engine.get_session(&session_id).unwrap();
        assert_eq!(session.messages.len(), 4);

        // Verify the LLM was called with the full conversation each time
        let calls = mock.captured_calls();
        assert_eq!(calls.len(), 2);

        // First call: system + 1 user message
        assert_eq!(calls[0].len(), 2); // system + user
        assert_eq!(calls[0][0].role, "system");
        assert_eq!(calls[0][1].role, "user");

        // Second call: system + user + assistant + user
        assert_eq!(calls[1].len(), 4); // system + user + assistant + user
        assert_eq!(calls[1][0].role, "system");
        assert_eq!(calls[1][3].role, "user");
        assert_eq!(calls[1][3].content, "Tell me more about errors");
    }

    #[tokio::test]
    async fn send_message_with_ai_no_provider_returns_error() {
        let mut engine = InsightsEngine::new();
        let session_id = engine.create_session("No AI", "model").id;

        let result = engine
            .send_message_with_ai(&session_id, "hello")
            .await;

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("No LLM provider"));
    }

    #[tokio::test]
    async fn send_message_with_ai_session_not_found() {
        let mock = Arc::new(MockProvider::new("reply"));
        let mut engine = InsightsEngine::with_provider(mock);

        let result = engine
            .send_message_with_ai(&Uuid::new_v4(), "hello")
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn engine_without_provider_backward_compat() {
        let mut engine = InsightsEngine::new();
        let id = engine.create_session("Session", "model").id;

        engine.add_message(&id, ChatRole::User, "hi").unwrap();
        assert_eq!(engine.get_session(&id).unwrap().messages.len(), 1);
        assert!(engine.delete_session(&id));
        assert!(engine.list_sessions().is_empty());
    }
}
