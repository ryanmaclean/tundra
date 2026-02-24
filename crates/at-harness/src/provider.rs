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

/// Errors that can occur when interacting with an LLM provider.
///
/// This enum standardizes error handling across different provider implementations
/// (Anthropic, OpenAI, etc.), allowing callers to handle common failure modes
/// uniformly regardless of the underlying provider.
///
/// # Examples
///
/// ```rust
/// use at_harness::provider::{ProviderError, LlmProvider};
///
/// async fn handle_provider_error(provider: impl LlmProvider) {
///     match provider.chat(vec![], None).await {
///         Err(ProviderError::RateLimited { retry_after_ms }) => {
///             println!("Rate limited, retry after {}ms", retry_after_ms);
///         }
///         Err(ProviderError::Timeout) => {
///             println!("Request timed out, may need to retry");
///         }
///         Err(e) => {
///             println!("Other error: {}", e);
///         }
///         Ok(_) => {}
///     }
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// Provider is not properly configured or initialized.
    ///
    /// This typically occurs when:
    /// - Required API keys or credentials are missing
    /// - The provider client hasn't been set up
    /// - Using [`StubProvider`] without a real implementation
    ///
    /// The contained string provides details about what's missing.
    #[error("provider not configured: {0}")]
    NotConfigured(String),

    /// The provider's API returned an error.
    ///
    /// This represents errors from the LLM provider's service, such as:
    /// - Invalid request parameters
    /// - Model not found or unavailable
    /// - Content policy violations
    /// - Server-side errors (5xx responses)
    ///
    /// The contained string includes the provider's error message.
    #[error("api error: {0}")]
    Api(String),

    /// Request was rate limited by the provider.
    ///
    /// The provider has temporarily blocked requests due to rate limits.
    /// The `retry_after_ms` field indicates how long to wait before retrying.
    ///
    /// Callers should implement exponential backoff or respect the retry delay.
    #[error("rate limited – retry after {retry_after_ms}ms")]
    RateLimited {
        /// Milliseconds to wait before retrying the request.
        retry_after_ms: u64,
    },

    /// The request timed out.
    ///
    /// The provider didn't respond within the configured timeout period.
    /// This may indicate network issues or the provider's service being slow.
    #[error("request timed out")]
    Timeout,

    /// A catch-all for other errors not covered by specific variants.
    ///
    /// This includes:
    /// - Network/connection errors
    /// - Serialization/deserialization failures
    /// - Unexpected provider behaviors
    ///
    /// The contained string provides error details.
    #[error("{0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

/// The role of a participant in an LLM conversation.
///
/// Roles determine how messages are interpreted by the LLM and control
/// conversation flow. Most providers support these standard roles, though
/// specific behavior may vary by provider.
///
/// # Examples
///
/// ```rust
/// use at_harness::provider::{Message, Role};
///
/// // Create messages with different roles
/// let system = Message::system("You are a helpful assistant.");
/// assert_eq!(system.role, Role::System);
///
/// let user = Message::user("What is 2+2?");
/// assert_eq!(user.role, Role::User);
///
/// let assistant = Message::assistant("2+2 equals 4.");
/// assert_eq!(assistant.role, Role::Assistant);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// System instructions that set context and behavior.
    ///
    /// System messages typically appear at the beginning of conversations to:
    /// - Define the assistant's personality or expertise
    /// - Set behavioral guidelines or constraints
    /// - Provide background context or knowledge
    ///
    /// Not all providers support system messages; some may treat them as user messages.
    System,

    /// Messages from the human user.
    ///
    /// User messages represent input from the person interacting with the LLM.
    /// They drive the conversation forward and typically contain questions,
    /// commands, or information for the assistant to process.
    User,

    /// Messages from the LLM assistant.
    ///
    /// Assistant messages represent the LLM's responses. In multi-turn conversations,
    /// previous assistant messages provide context for subsequent interactions and
    /// help maintain conversation coherence.
    Assistant,

    /// Messages representing tool/function call results.
    ///
    /// Tool messages are used in tool-calling workflows to provide the results
    /// of function executions back to the LLM. They typically include:
    /// - A `tool_call_id` linking to the original tool call
    /// - The serialized output from the tool/function
    ///
    /// See [`ToolCall`] and [`Message::tool_call_id`] for more details.
    Tool,
}

/// A single message in an LLM conversation.
///
/// Messages form the backbone of LLM interactions, representing exchanges between
/// the user, assistant, and system. Each message has a [`Role`] that determines
/// how it's interpreted by the LLM.
///
/// # Fields
///
/// - `role`: The participant role ([`Role::User`], [`Role::Assistant`], etc.)
/// - `content`: The message text content
/// - `name`: Optional identifier for the message sender (for multi-user scenarios)
/// - `tool_call_id`: Links tool result messages to their originating tool calls
///
/// # Examples
///
/// ```rust
/// use at_harness::provider::Message;
///
/// // Simple user message
/// let msg = Message::user("Hello, assistant!");
///
/// // System message with context
/// let system = Message::system("You are a helpful coding assistant.");
///
/// // Assistant response
/// let response = Message::assistant("Hello! How can I help you today?");
/// ```
///
/// # Tool Calling Workflow
///
/// When working with tool calls, messages follow this pattern:
///
/// ```rust
/// use at_harness::provider::{Message, Role};
///
/// // 1. User asks a question
/// let user_msg = Message::user("What's the weather in Tokyo?");
///
/// // 2. Assistant responds with a tool call (handled by provider)
/// // 3. Tool result is sent back with tool_call_id
/// let tool_result = Message {
///     role: Role::Tool,
///     content: r#"{"temperature": 22, "condition": "sunny"}"#.to_string(),
///     name: Some("get_weather".to_string()),
///     tool_call_id: Some("call_abc123".to_string()),
/// };
/// ```
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
    /// Create a system message with the given content.
    ///
    /// System messages set the context, behavior, and personality of the assistant.
    /// They typically appear at the start of a conversation to provide instructions
    /// or background information.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use at_harness::provider::Message;
    ///
    /// let msg = Message::system("You are a helpful assistant specializing in Rust.");
    /// assert_eq!(msg.content, "You are a helpful assistant specializing in Rust.");
    /// ```
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }

    /// Create a user message with the given content.
    ///
    /// User messages represent input from the human user and drive the conversation
    /// forward. They typically contain questions, requests, or information for the
    /// assistant to process.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use at_harness::provider::Message;
    ///
    /// let msg = Message::user("What is the capital of France?");
    /// assert_eq!(msg.content, "What is the capital of France?");
    /// ```
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            name: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message with the given content.
    ///
    /// Assistant messages represent responses from the LLM. In multi-turn conversations,
    /// including previous assistant messages helps maintain context and conversation
    /// coherence.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use at_harness::provider::Message;
    ///
    /// let msg = Message::assistant("The capital of France is Paris.");
    /// assert_eq!(msg.content, "The capital of France is Paris.");
    /// ```
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

/// Definition of a tool/function that the LLM can call.
///
/// Tools enable LLMs to interact with external functions, APIs, or systems by
/// providing structured function definitions. The LLM can decide when to call
/// a tool based on the conversation context and return structured arguments
/// that your code can execute.
///
/// # Fields
///
/// - `name`: Unique identifier for the tool (e.g., "get_weather", "calculate")
/// - `description`: Human-readable explanation of what the tool does and when to use it
/// - `parameters`: JSON Schema defining the tool's input parameters
///
/// # Tool Calling Workflow
///
/// 1. Define tools with their schemas
/// 2. Pass tools to [`LlmProvider::chat`]
/// 3. LLM returns [`ToolCall`] instances in the [`Response`]
/// 4. Execute the tool calls with the provided arguments
/// 5. Send results back as [`Role::Tool`] messages
///
/// # Examples
///
/// ```rust
/// use at_harness::provider::Tool;
/// use serde_json::json;
///
/// // Simple tool with no parameters
/// let ping = Tool {
///     name: "ping".to_string(),
///     description: "Check if the service is alive".to_string(),
///     parameters: json!({
///         "type": "object",
///         "properties": {}
///     }),
/// };
///
/// // Tool with required and optional parameters
/// let get_weather = Tool {
///     name: "get_weather".to_string(),
///     description: "Get current weather for a city".to_string(),
///     parameters: json!({
///         "type": "object",
///         "properties": {
///             "city": {
///                 "type": "string",
///                 "description": "City name"
///             },
///             "units": {
///                 "type": "string",
///                 "enum": ["celsius", "fahrenheit"],
///                 "description": "Temperature units"
///             }
///         },
///         "required": ["city"]
///     }),
/// };
/// ```
///
/// # JSON Schema Format
///
/// The `parameters` field must be a valid JSON Schema (typically an object schema).
/// Most providers support JSON Schema Draft 7 or similar. Common patterns:
///
/// - Use `"type": "object"` with `"properties"` to define structured inputs
/// - Mark required fields in the `"required"` array
/// - Use `"enum"` to restrict values to specific options
/// - Add `"description"` fields to help the LLM understand each parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Unique name identifying this tool.
    ///
    /// Should be descriptive and follow snake_case convention (e.g., "get_weather",
    /// "send_email"). This name is used by the LLM to reference the tool in
    /// [`ToolCall`] responses.
    pub name: String,

    /// Human-readable description of the tool's purpose and usage.
    ///
    /// This description helps the LLM decide when to use the tool. Be specific
    /// about what the tool does, what inputs it expects, and what it returns.
    ///
    /// Good: "Get the current weather forecast for a specified city, returning
    /// temperature, conditions, and humidity."
    ///
    /// Bad: "Weather tool"
    pub description: String,

    /// JSON Schema defining the tool's input parameters.
    ///
    /// This schema describes the structure and types of arguments the tool accepts.
    /// The LLM uses this schema to generate valid arguments when calling the tool.
    ///
    /// Must be a JSON Schema object (typically `{"type": "object", "properties": {...}}`).
    /// Most providers support JSON Schema Draft 7 or compatible formats.
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
