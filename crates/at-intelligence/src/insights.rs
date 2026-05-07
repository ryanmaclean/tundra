use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::llm::{LlmConfig, LlmMessage, LlmProvider, LlmRole};
use crate::IntelligenceError;

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

// Manual Debug impl because Arc<dyn LlmProvider> doesn't auto-derive Debug.
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
        // Capture the user-message timestamp at function entry so that the
        // deferred commit (after a possibly-slow provider call) still reflects
        // when the user actually sent the message — not when we got around to
        // recording it.
        let user_timestamp = Utc::now();

        let provider = self
            .provider
            .as_ref()
            .ok_or_else(|| {
                IntelligenceError::InvalidOperation(
                    "No LLM provider configured – use InsightsEngine::with_provider()".into(),
                )
            })?
            .clone();

        // 1. Build the conversation history as LlmMessages WITHOUT committing
        //    the new user message to the session yet.  We defer the commit
        //    until after the provider call succeeds so that a provider error
        //    never leaves an orphaned user message in the session history.
        let (llm_messages, model) = {
            let session = self.sessions.iter().find(|s| s.id == *session_id).ok_or(
                IntelligenceError::NotFound {
                    entity: "session".into(),
                    id: *session_id,
                },
            )?;

            let system_prompt = "You are an expert codebase exploration assistant. \
                Help the user understand code structure, patterns, dependencies, and \
                potential improvements. Be concise and precise.";

            let mut msgs = vec![LlmMessage::system(system_prompt)];

            for msg in &session.messages {
                let role = match msg.role {
                    ChatRole::User => LlmRole::User,
                    ChatRole::Assistant => LlmRole::Assistant,
                    ChatRole::System => LlmRole::System,
                };
                msgs.push(LlmMessage::new(role, msg.content.clone()));
            }

            // Append the pending user message to the LLM payload only — not
            // yet to session.messages.
            msgs.push(LlmMessage::new(LlmRole::User, content));

            (msgs, session.model.clone())
        };

        // 2. Call the LLM.  If this fails we return early and session history
        //    remains unchanged (no half-written user message).
        let config = LlmConfig {
            model,
            max_tokens: 1024,
            temperature: 0.7,
            system_prompt: None,
        };
        let response = provider
            .complete(&llm_messages, &config)
            .await
            .map_err(|e| IntelligenceError::InvalidOperation(format!("LLM call failed: {e}")))?;

        // 3. Provider succeeded — now atomically commit both the user message
        //    and the assistant reply to session history.
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
        session_mut.messages.push(ChatMessage {
            role: ChatRole::User,
            content: content.to_string(),
            timestamp: user_timestamp,
        });
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
    use crate::llm::{LlmConfig, LlmError, LlmMessage, LlmProvider, LlmResponse, LlmRole};
    use futures_util::Stream;
    use std::pin::Pin;
    use std::sync::Mutex;

    // ---- MockProvider --------------------------------------------------------

    struct MockProvider {
        /// The canned response the mock returns.
        response: String,
        /// Captured calls for assertions.
        calls: Mutex<Vec<(Vec<LlmMessage>, LlmConfig)>>,
    }

    impl MockProvider {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn captured_calls(&self) -> Vec<(Vec<LlmMessage>, LlmConfig)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(
            &self,
            messages: &[LlmMessage],
            config: &LlmConfig,
        ) -> Result<LlmResponse, LlmError> {
            self.calls
                .lock()
                .unwrap()
                .push((messages.to_vec(), config.clone()));
            Ok(LlmResponse {
                content: self.response.clone(),
                model: "mock".to_string(),
                input_tokens: 10,
                output_tokens: 5,
                finish_reason: "end_turn".to_string(),
            })
        }

        async fn stream(
            &self,
            _messages: &[LlmMessage],
            _config: &LlmConfig,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError>
        {
            Err(LlmError::Unsupported(
                "mock does not support streaming".into(),
            ))
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

        // Second exchange -- history should accumulate
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
        assert_eq!(calls[0].0.len(), 2); // system + user
        assert_eq!(calls[0].0[0].role, LlmRole::System);
        assert_eq!(calls[0].0[1].role, LlmRole::User);

        // Second call: system + user + assistant + user
        assert_eq!(calls[1].0.len(), 4); // system + user + assistant + user
        assert_eq!(calls[1].0[0].role, LlmRole::System);
        assert_eq!(calls[1].0[3].role, LlmRole::User);
        assert_eq!(calls[1].0[3].content, "Tell me more about errors");
    }

    #[tokio::test]
    async fn send_message_with_ai_no_provider_returns_error() {
        let mut engine = InsightsEngine::new();
        let session_id = engine.create_session("No AI", "model").id;

        let result = engine.send_message_with_ai(&session_id, "hello").await;

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("No LLM provider"));
    }

    #[tokio::test]
    async fn send_message_with_ai_session_not_found() {
        let mock = Arc::new(MockProvider::new("reply"));
        let mut engine = InsightsEngine::with_provider(mock);

        let result = engine.send_message_with_ai(&Uuid::new_v4(), "hello").await;

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

    // ---- New tests: stream-error path and success-path anchor ---------------

    /// Test A (verifies the half-write bug is FIXED): when the LLM provider
    /// returns an error, `send_message_with_ai` returns an error AND the
    /// session history is left completely clean — the user message is NOT
    /// committed because the provider call was deferred until after commit.
    #[tokio::test]
    async fn send_message_with_ai_provider_error_does_not_commit_user_message() {
        use crate::llm::{LlmError, MockProvider as LlmMockProvider};

        // Queue an API-level error so complete() returns Err.
        let failing_provider = Arc::new(LlmMockProvider::new().with_error(LlmError::ApiError {
            status: 500,
            message: "internal server error".into(),
        }));

        let mut engine = InsightsEngine::with_provider(failing_provider);
        let session_id = engine.create_session("Error Test", "test-model").id;

        // Before the call the session is empty.
        assert_eq!(engine.get_session(&session_id).unwrap().messages.len(), 0);

        let result = engine
            .send_message_with_ai(&session_id, "What went wrong?")
            .await;

        // (a) The call must have returned an error.
        assert!(result.is_err(), "expected an error from the provider");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("LLM call failed"),
            "unexpected error message: {err_msg}"
        );

        // (b) BUG-FIXED: previously, the user message was committed before the
        //     provider call, leaving orphaned history on error; now the user
        //     message is only committed after provider.complete() succeeds, so
        //     history must be empty after a provider error.
        let messages = &engine.get_session(&session_id).unwrap().messages;
        assert_eq!(
            messages.len(),
            0,
            "expected 0 messages after provider error (user message must not be committed), got {}",
            messages.len()
        );
    }

    /// Test B: success path — `send_message_with_ai` returns Ok and the session
    /// history grows by exactly 2 messages (user + assistant). This anchors the
    /// mutation surface checked by Test A.
    #[tokio::test]
    async fn send_message_with_ai_success_adds_user_and_assistant_messages() {
        use crate::llm::{LlmResponse, MockProvider as LlmMockProvider};

        let success_provider = Arc::new(LlmMockProvider::new().with_response(LlmResponse {
            content: "Here is my answer.".into(),
            model: "test-model".into(),
            input_tokens: 20,
            output_tokens: 8,
            finish_reason: "end_turn".into(),
        }));

        let mut engine = InsightsEngine::with_provider(success_provider);
        let session_id = engine.create_session("Success Test", "test-model").id;

        assert_eq!(engine.get_session(&session_id).unwrap().messages.len(), 0);

        let reply = engine
            .send_message_with_ai(&session_id, "Tell me something.")
            .await
            .expect("send_message_with_ai should succeed");

        // Return value is the assistant message.
        assert_eq!(reply.role, ChatRole::Assistant);
        assert_eq!(reply.content, "Here is my answer.");

        // Session history must have grown by exactly 2 (user + assistant).
        let messages = &engine.get_session(&session_id).unwrap().messages;
        assert_eq!(
            messages.len(),
            2,
            "expected 2 messages (user + assistant) after a successful call, got {}",
            messages.len()
        );
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[0].content, "Tell me something.");
        assert_eq!(messages[1].role, ChatRole::Assistant);
        assert_eq!(messages[1].content, "Here is my answer.");
    }
}
