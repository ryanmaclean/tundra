//! LLM provider abstraction for at-harness.
//!
//! Provides a unified async trait for interacting with LLM providers,
//! supporting chat completions with optional tool calling capabilities.
//!
//! # Overview
//!
//! This module defines the core [`LlmProvider`] trait and supporting types
//! for building LLM-powered test harnesses. The trait provides:
//!
//! - **Chat completions** via the [`LlmProvider::chat`] method
//! - **Tool calling** support for function/API interactions
//! - **Standardized error handling** through [`ProviderError`]
//! - **Message formatting** with [`Message`] and [`Role`] types
//!
//! Concrete provider implementations (Anthropic, OpenAI, etc.) are provided
//! by dependent crates. This crate includes a [`StubProvider`] for testing
//! and placeholder scenarios.
//!
//! # Implementation Guide
//!
//! To implement a new provider:
//!
//! 1. Create a struct to hold client state (API key, HTTP client, etc.)
//! 2. Implement [`LlmProvider`] with your provider's API calls
//! 3. Map provider-specific errors to [`ProviderError`] variants
//! 4. Handle tool calls if your provider supports them
//!
//! # Example
//!
//! ```rust,no_run
//! use at_harness::provider::{LlmProvider, Message, Tool, ProviderError};
//!
//! async fn example(provider: impl LlmProvider) -> Result<(), ProviderError> {
//!     // Simple chat completion
//!     let messages = vec![Message::user("Hello, world!")];
//!     let response = provider.chat(messages, None).await?;
//!     println!("Response: {}", response.content.unwrap_or_default());
//!
//!     // Chat with tool calling
//!     let messages = vec![Message::user("What's the weather in Tokyo?")];
//!     let tools = vec![Tool {
//!         name: "get_weather".to_string(),
//!         description: "Get current weather for a city".to_string(),
//!         parameters: serde_json::json!({
//!             "type": "object",
//!             "properties": {
//!                 "city": {"type": "string"}
//!             }
//!         }),
//!     }];
//!     let response = provider.chat(messages, Some(tools)).await?;
//!
//!     // Handle tool calls in response
//!     for tool_call in &response.tool_calls {
//!         println!("Tool: {} - Args: {}", tool_call.name, tool_call.arguments);
//!     }
//!
//!     Ok(())
//! }
//! ```

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider not configured: {0}")]
    NotConfigured(String),
    #[error("api error: {0}")]
    Api(String),
    #[error("rate limited – retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },
    #[error("request timed out")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tool definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool parameters.
    pub parameters: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Tool call (in a response)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    pub model: String,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

// ---------------------------------------------------------------------------
// LlmProvider trait
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request.
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
    ) -> Result<Response, ProviderError>;

    /// Human-readable provider name (e.g. "anthropic", "openai").
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// StubProvider – returns an error for every call.
// ---------------------------------------------------------------------------

/// A placeholder provider that always returns `NotConfigured`.
/// Real implementations (Anthropic, OpenAI, etc.) will be added in future
/// crates that depend on genai / rig.
#[derive(Debug, Clone)]
pub struct StubProvider {
    provider_name: String,
}

impl StubProvider {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            provider_name: name.into(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for StubProvider {
    async fn chat(
        &self,
        _messages: Vec<Message>,
        _tools: Option<Vec<Tool>>,
    ) -> Result<Response, ProviderError> {
        Err(ProviderError::NotConfigured(format!(
            "{} provider is not configured – install a concrete implementation",
            self.provider_name
        )))
    }

    fn name(&self) -> &str {
        &self.provider_name
    }
}
