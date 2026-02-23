//! Exhaustive integration tests for InsightsEngine (Insights / AI Chat feature).

use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use futures_util::Stream;
use uuid::Uuid;

use at_intelligence::insights::{ChatMessage, ChatRole, InsightsEngine, InsightsSession};
use at_intelligence::llm::{LlmConfig, LlmError, LlmMessage, LlmProvider, LlmResponse, LlmRole};

// ---------------------------------------------------------------------------
// MockProvider — captures calls & returns canned responses
// ---------------------------------------------------------------------------

struct MockProvider {
    response: String,
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

#[async_trait]
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
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        Err(LlmError::Unsupported(
            "mock does not support streaming".into(),
        ))
    }
}

// ===========================================================================
// Chat Session Management
// ===========================================================================

#[test]
fn test_create_new_chat_session() {
    let mut engine = InsightsEngine::new();
    let session = engine.create_session("My Session", "claude-3");
    assert!(!session.id.is_nil());
    assert_eq!(session.title, "My Session");
    assert_eq!(session.model, "claude-3");
    assert!(session.messages.is_empty());
}

#[test]
fn test_multiple_chat_sessions_unique_ids() {
    let mut engine = InsightsEngine::new();
    let id1 = engine.create_session("S1", "model-a").id;
    let id2 = engine.create_session("S2", "model-b").id;
    let id3 = engine.create_session("S3", "model-c").id;

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
}

#[test]
fn test_list_chat_sessions_returns_all() {
    let mut engine = InsightsEngine::new();
    assert!(engine.list_sessions().is_empty());

    engine.create_session("Alpha", "m1");
    engine.create_session("Beta", "m2");
    engine.create_session("Gamma", "m3");

    let sessions = engine.list_sessions();
    assert_eq!(sessions.len(), 3);

    let titles: Vec<&str> = sessions.iter().map(|s| s.title.as_str()).collect();
    assert!(titles.contains(&"Alpha"));
    assert!(titles.contains(&"Beta"));
    assert!(titles.contains(&"Gamma"));
}

#[test]
fn test_delete_chat_session() {
    let mut engine = InsightsEngine::new();
    let id = engine.create_session("Doomed", "model").id;
    assert_eq!(engine.list_sessions().len(), 1);

    assert!(engine.delete_session(&id));
    assert!(engine.list_sessions().is_empty());

    // Deleting again returns false
    assert!(!engine.delete_session(&id));
}

#[test]
fn test_get_chat_session_by_id() {
    let mut engine = InsightsEngine::new();
    let id = engine.create_session("Lookup", "claude-3").id;

    let session = engine.get_session(&id);
    assert!(session.is_some());
    assert_eq!(session.unwrap().title, "Lookup");

    // Non-existent id returns None
    assert!(engine.get_session(&Uuid::new_v4()).is_none());
}

#[test]
fn test_chat_session_has_created_at_timestamp() {
    let before = Utc::now();
    let mut engine = InsightsEngine::new();
    let id = engine.create_session("Timestamped", "model").id;
    let after = Utc::now();

    let session = engine.get_session(&id).unwrap();
    assert!(session.created_at >= before);
    assert!(session.created_at <= after);
}

// ===========================================================================
// Message Flow
// ===========================================================================

#[test]
fn test_add_user_message_to_session() {
    let mut engine = InsightsEngine::new();
    let id = engine.create_session("Chat", "model").id;

    engine.add_message(&id, ChatRole::User, "Hello!").unwrap();

    let session = engine.get_session(&id).unwrap();
    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].role, ChatRole::User);
    assert_eq!(session.messages[0].content, "Hello!");
}

#[test]
fn test_add_assistant_message_to_session() {
    let mut engine = InsightsEngine::new();
    let id = engine.create_session("Chat", "model").id;

    engine
        .add_message(&id, ChatRole::Assistant, "I can help!")
        .unwrap();

    let session = engine.get_session(&id).unwrap();
    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].role, ChatRole::Assistant);
    assert_eq!(session.messages[0].content, "I can help!");
}

#[test]
fn test_message_order_preserved() {
    let mut engine = InsightsEngine::new();
    let id = engine.create_session("Ordered", "model").id;

    engine.add_message(&id, ChatRole::User, "First").unwrap();
    engine
        .add_message(&id, ChatRole::Assistant, "Second")
        .unwrap();
    engine.add_message(&id, ChatRole::User, "Third").unwrap();

    let session = engine.get_session(&id).unwrap();
    assert_eq!(session.messages.len(), 3);
    assert_eq!(session.messages[0].content, "First");
    assert_eq!(session.messages[1].content, "Second");
    assert_eq!(session.messages[2].content, "Third");
}

#[test]
fn test_message_has_role_and_content() {
    let mut engine = InsightsEngine::new();
    let id = engine.create_session("Msg Check", "model").id;

    engine
        .add_message(&id, ChatRole::User, "What is Rust?")
        .unwrap();
    engine
        .add_message(&id, ChatRole::Assistant, "A systems programming language.")
        .unwrap();
    engine
        .add_message(&id, ChatRole::System, "You are helpful.")
        .unwrap();

    let session = engine.get_session(&id).unwrap();

    assert_eq!(session.messages[0].role, ChatRole::User);
    assert_eq!(session.messages[0].content, "What is Rust?");

    assert_eq!(session.messages[1].role, ChatRole::Assistant);
    assert_eq!(
        session.messages[1].content,
        "A systems programming language."
    );

    assert_eq!(session.messages[2].role, ChatRole::System);
    assert_eq!(session.messages[2].content, "You are helpful.");
}

#[test]
fn test_conversation_history_builds_context() {
    let mut engine = InsightsEngine::new();
    let id = engine.create_session("Context Builder", "model").id;

    let messages_to_add = vec![
        (ChatRole::User, "Explain the project structure"),
        (ChatRole::Assistant, "The project has 3 crates..."),
        (ChatRole::User, "What about the at-intelligence crate?"),
        (ChatRole::Assistant, "It handles AI-powered features..."),
        (ChatRole::User, "Tell me about the insights module"),
    ];

    for (role, content) in &messages_to_add {
        engine.add_message(&id, role.clone(), content).unwrap();
    }

    let session = engine.get_session(&id).unwrap();
    assert_eq!(session.messages.len(), messages_to_add.len());

    // Verify full history is intact and ordered
    for (i, (role, content)) in messages_to_add.iter().enumerate() {
        assert_eq!(&session.messages[i].role, role);
        assert_eq!(session.messages[i].content, *content);
    }
}

// ===========================================================================
// AI Integration
// ===========================================================================

#[tokio::test]
async fn test_send_message_with_ai_returns_response() {
    let mock = Arc::new(MockProvider::new("Here is my analysis of the codebase."));
    let mut engine = InsightsEngine::with_provider(mock);

    let session_id = engine.create_session("AI Chat", "claude-3").id;

    let reply = engine
        .send_message_with_ai(&session_id, "Analyze the project")
        .await
        .unwrap();

    assert_eq!(reply.role, ChatRole::Assistant);
    assert_eq!(reply.content, "Here is my analysis of the codebase.");
}

#[tokio::test]
async fn test_send_message_with_ai_includes_system_prompt() {
    let mock = Arc::new(MockProvider::new("reply"));
    let mut engine = InsightsEngine::with_provider(mock.clone());

    let session_id = engine.create_session("SysPrompt", "claude-3").id;

    engine
        .send_message_with_ai(&session_id, "hello")
        .await
        .unwrap();

    let calls = mock.captured_calls();
    assert_eq!(calls.len(), 1);

    // First message should be the system prompt
    assert_eq!(calls[0].0[0].role, LlmRole::System);
    assert!(calls[0].0[0].content.contains("codebase"));
}

#[tokio::test]
async fn test_send_message_with_ai_includes_conversation_history() {
    let mock = Arc::new(MockProvider::new("AI response"));
    let mut engine = InsightsEngine::with_provider(mock.clone());

    let session_id = engine.create_session("History", "claude-3").id;

    // First exchange
    engine
        .send_message_with_ai(&session_id, "What frameworks are used?")
        .await
        .unwrap();

    // Second exchange — should include full history
    engine
        .send_message_with_ai(&session_id, "Tell me more about Axum")
        .await
        .unwrap();

    let calls = mock.captured_calls();
    assert_eq!(calls.len(), 2);

    // First call: system + 1 user message
    assert_eq!(calls[0].0.len(), 2);
    assert_eq!(calls[0].0[0].role, LlmRole::System);
    assert_eq!(calls[0].0[1].role, LlmRole::User);

    // Second call: system + user + assistant + user = 4 messages
    assert_eq!(calls[1].0.len(), 4);
    assert_eq!(calls[1].0[0].role, LlmRole::System);
    assert_eq!(calls[1].0[1].role, LlmRole::User);
    assert_eq!(calls[1].0[2].role, LlmRole::Assistant);
    assert_eq!(calls[1].0[3].role, LlmRole::User);
    assert_eq!(calls[1].0[3].content, "Tell me more about Axum");
}

#[tokio::test]
async fn test_send_message_without_provider_returns_error() {
    let mut engine = InsightsEngine::new(); // No provider
    let session_id = engine.create_session("No AI", "model").id;

    let result = engine.send_message_with_ai(&session_id, "hello").await;

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("No LLM provider"));
}

#[tokio::test]
async fn test_ai_response_stored_in_session() {
    let mock = Arc::new(MockProvider::new("Stored response"));
    let mut engine = InsightsEngine::with_provider(mock);

    let session_id = engine.create_session("Store Test", "claude-3").id;

    engine
        .send_message_with_ai(&session_id, "Ask something")
        .await
        .unwrap();

    let session = engine.get_session(&session_id).unwrap();
    assert_eq!(session.messages.len(), 2); // user + assistant

    assert_eq!(session.messages[0].role, ChatRole::User);
    assert_eq!(session.messages[0].content, "Ask something");

    assert_eq!(session.messages[1].role, ChatRole::Assistant);
    assert_eq!(session.messages[1].content, "Stored response");
}

// ===========================================================================
// Codebase Analysis
// ===========================================================================

#[tokio::test]
async fn test_insights_analyze_project_structure() {
    let mock = Arc::new(MockProvider::new(
        "The project is a Rust workspace with 10 crates. \
         The main crate is `at-core` which defines shared types. \
         `at-bridge` handles HTTP/WS communication. \
         `at-intelligence` provides AI features.",
    ));
    let mut engine = InsightsEngine::with_provider(mock.clone());

    let session_id = engine.create_session("Analysis", "claude-3").id;

    let reply = engine
        .send_message_with_ai(&session_id, "Analyze the project structure")
        .await
        .unwrap();

    assert!(reply.content.contains("Rust workspace"));
    assert!(reply.content.contains("at-core"));
    assert!(reply.content.contains("at-bridge"));
    assert!(reply.content.contains("at-intelligence"));

    // Verify the prompt was sent correctly
    let calls = mock.captured_calls();
    assert!(calls[0].0[1].content.contains("project structure"));
}

#[tokio::test]
async fn test_insights_suggest_improvements() {
    let mock = Arc::new(MockProvider::new(
        "1. Add connection pooling for database queries\n\
         2. Implement caching for frequently accessed data\n\
         3. Add comprehensive error handling in the API layer",
    ));
    let mut engine = InsightsEngine::with_provider(mock);

    let session_id = engine.create_session("Improvements", "claude-3").id;

    let reply = engine
        .send_message_with_ai(&session_id, "What improvements would you suggest?")
        .await
        .unwrap();

    assert!(reply.content.contains("connection pooling"));
    assert!(reply.content.contains("caching"));
    assert!(reply.content.contains("error handling"));
}

#[tokio::test]
async fn test_insights_create_task_from_suggestion() {
    // This tests the flow: user asks for suggestions -> AI responds ->
    // user asks to create a task -> the suggestion gets stored in the session.
    let mock = Arc::new(MockProvider::new(
        "Task created: Add connection pooling to the database layer.",
    ));
    let mut engine = InsightsEngine::with_provider(mock);

    let session_id = engine.create_session("Task Creation", "claude-3").id;

    // Simulate the "create task" prompt
    let reply = engine
        .send_message_with_ai(&session_id, "Create a task for adding connection pooling")
        .await
        .unwrap();

    assert!(reply.content.contains("Task created"));
    assert!(reply.content.contains("connection pooling"));

    // Verify the full conversation is stored
    let session = engine.get_session(&session_id).unwrap();
    assert_eq!(session.messages.len(), 2);
    assert_eq!(session.messages[0].role, ChatRole::User);
    assert_eq!(session.messages[1].role, ChatRole::Assistant);
}

// ===========================================================================
// Edge cases & additional coverage
// ===========================================================================

#[test]
fn test_add_message_to_nonexistent_session_returns_error() {
    let mut engine = InsightsEngine::new();
    let result = engine.add_message(&Uuid::new_v4(), ChatRole::User, "oops");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_send_message_with_ai_nonexistent_session_returns_error() {
    let mock = Arc::new(MockProvider::new("reply"));
    let mut engine = InsightsEngine::with_provider(mock);

    let result = engine.send_message_with_ai(&Uuid::new_v4(), "hello").await;

    assert!(result.is_err());
}

#[test]
fn test_delete_session_does_not_affect_others() {
    let mut engine = InsightsEngine::new();
    let id1 = engine.create_session("Keep", "m1").id;
    let id2 = engine.create_session("Remove", "m2").id;
    let id3 = engine.create_session("Keep Too", "m3").id;

    assert!(engine.delete_session(&id2));
    assert_eq!(engine.list_sessions().len(), 2);
    assert!(engine.get_session(&id1).is_some());
    assert!(engine.get_session(&id2).is_none());
    assert!(engine.get_session(&id3).is_some());
}

#[test]
fn test_default_creates_empty_engine() {
    let engine = InsightsEngine::default();
    assert!(engine.list_sessions().is_empty());
}

#[test]
fn test_serde_roundtrip_chat_message() {
    let msg = ChatMessage {
        role: ChatRole::Assistant,
        content: "Hello world".to_string(),
        timestamp: Utc::now(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.role, msg.role);
    assert_eq!(deserialized.content, msg.content);
}

#[test]
fn test_serde_roundtrip_insights_session() {
    let session = InsightsSession {
        id: Uuid::new_v4(),
        title: "Test".to_string(),
        messages: vec![
            ChatMessage {
                role: ChatRole::User,
                content: "hi".to_string(),
                timestamp: Utc::now(),
            },
            ChatMessage {
                role: ChatRole::Assistant,
                content: "hello".to_string(),
                timestamp: Utc::now(),
            },
        ],
        model: "claude-3".to_string(),
        created_at: Utc::now(),
    };
    let json = serde_json::to_string(&session).unwrap();
    let deserialized: InsightsSession = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, session.id);
    assert_eq!(deserialized.title, session.title);
    assert_eq!(deserialized.messages.len(), 2);
    assert_eq!(deserialized.model, session.model);
}

#[test]
fn test_chat_role_variants() {
    // Ensure all ChatRole variants are distinct
    assert_ne!(ChatRole::User, ChatRole::Assistant);
    assert_ne!(ChatRole::User, ChatRole::System);
    assert_ne!(ChatRole::Assistant, ChatRole::System);
}

#[test]
fn test_message_timestamp_is_set() {
    let before = Utc::now();
    let mut engine = InsightsEngine::new();
    let id = engine.create_session("Timestamps", "model").id;
    engine.add_message(&id, ChatRole::User, "hi").unwrap();
    let after = Utc::now();

    let session = engine.get_session(&id).unwrap();
    let ts = session.messages[0].timestamp;
    assert!(ts >= before);
    assert!(ts <= after);
}

#[tokio::test]
async fn test_multiple_sessions_independent_messages() {
    let mock = Arc::new(MockProvider::new("response"));
    let mut engine = InsightsEngine::with_provider(mock);

    let id1 = engine.create_session("Session A", "model").id;
    let id2 = engine.create_session("Session B", "model").id;

    engine
        .send_message_with_ai(&id1, "Question for A")
        .await
        .unwrap();
    engine
        .send_message_with_ai(&id2, "Question for B")
        .await
        .unwrap();

    let s1 = engine.get_session(&id1).unwrap();
    let s2 = engine.get_session(&id2).unwrap();

    assert_eq!(s1.messages.len(), 2);
    assert_eq!(s2.messages.len(), 2);
    assert_eq!(s1.messages[0].content, "Question for A");
    assert_eq!(s2.messages[0].content, "Question for B");
}
