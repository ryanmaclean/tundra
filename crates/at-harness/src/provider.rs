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

/// A tool/function call requested by the LLM in a response.
///
/// When the LLM determines it needs to call a tool to answer a user's question,
/// it returns one or more `ToolCall` instances in the [`Response::tool_calls`] field.
/// Each `ToolCall` contains the tool name, a unique ID for tracking, and JSON-encoded
/// arguments for the function.
///
/// # Fields
///
/// - `id`: Unique identifier for this specific tool call (provider-generated)
/// - `name`: Name of the tool to call, matching a [`Tool::name`] from the request
/// - `arguments`: JSON-encoded string of arguments conforming to the tool's parameter schema
///
/// # Tool Calling Workflow
///
/// 1. Send a chat request with tools defined via [`LlmProvider::chat`]
/// 2. LLM responds with `ToolCall` instances in [`Response::tool_calls`]
/// 3. Parse `arguments` and execute the corresponding function
/// 4. Send results back as [`Role::Tool`] messages with matching `tool_call_id`
/// 5. LLM uses the results to formulate a final answer
///
/// # Examples
///
/// ```rust
/// use at_harness::provider::{LlmProvider, Message, Tool, Role};
/// use serde_json::json;
///
/// # async fn example(provider: impl LlmProvider) -> Result<(), Box<dyn std::error::Error>> {
/// // Define available tools
/// let tools = vec![Tool {
///     name: "get_weather".to_string(),
///     description: "Get weather for a city".to_string(),
///     parameters: json!({
///         "type": "object",
///         "properties": {
///             "city": {"type": "string"}
///         },
///         "required": ["city"]
///     }),
/// }];
///
/// // Ask a question requiring a tool call
/// let messages = vec![Message::user("What's the weather in Tokyo?")];
/// let response = provider.chat(messages.clone(), Some(tools)).await?;
///
/// // Process tool calls
/// for tool_call in &response.tool_calls {
///     println!("Tool ID: {}", tool_call.id);
///     println!("Tool name: {}", tool_call.name);
///     println!("Arguments: {}", tool_call.arguments);
///
///     // Parse arguments and execute tool
///     let args: serde_json::Value = serde_json::from_str(&tool_call.arguments)?;
///     let city = args["city"].as_str().unwrap();
///
///     // Execute tool (example)
///     let result = format!(r#"{{"temperature": 22, "condition": "sunny"}}"#);
///
///     // Send result back to LLM
///     let mut next_messages = messages.clone();
///     next_messages.push(Message {
///         role: Role::Tool,
///         content: result,
///         name: Some(tool_call.name.clone()),
///         tool_call_id: Some(tool_call.id.clone()),
///     });
///
///     // Continue conversation with tool result
///     let final_response = provider.chat(next_messages, None).await?;
///     println!("Final answer: {}", final_response.content.unwrap_or_default());
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Argument Parsing
///
/// The `arguments` field contains JSON that conforms to the tool's parameter schema.
/// Always validate and handle parsing errors gracefully:
///
/// ```rust
/// use at_harness::provider::ToolCall;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Deserialize)]
/// struct WeatherArgs {
///     city: String,
///     units: Option<String>,
/// }
///
/// fn parse_tool_args(tool_call: &ToolCall) -> Result<WeatherArgs, serde_json::Error> {
///     serde_json::from_str(&tool_call.arguments)
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this tool call.
    ///
    /// This ID is generated by the provider and must be included in the
    /// [`Message::tool_call_id`] field when sending tool results back to the LLM.
    /// It allows the provider to match tool results with their originating calls.
    pub id: String,

    /// Name of the tool to call.
    ///
    /// This matches the [`Tool::name`] of one of the tools provided in the request.
    /// Use this to determine which function to execute.
    pub name: String,

    /// JSON-encoded arguments for the tool call.
    ///
    /// This string contains a JSON object conforming to the tool's parameter schema
    /// (as defined in [`Tool::parameters`]). Parse this with `serde_json::from_str`
    /// to extract the arguments for function execution.
    ///
    /// # Example
    ///
    /// For a tool with schema:
    /// ```json
    /// {
    ///   "type": "object",
    ///   "properties": {
    ///     "city": {"type": "string"}
    ///   }
    /// }
    /// ```
    ///
    /// The `arguments` might be:
    /// ```json
    /// {"city": "Tokyo"}
    /// ```
    pub arguments: String,
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

/// Response from an LLM chat completion request.
///
/// This struct represents the complete response from an LLM provider after a
/// chat completion request via [`LlmProvider::chat`]. It contains the assistant's
/// message content, any tool calls the LLM wants to make, model information, and
/// token usage statistics.
///
/// # Fields
///
/// - `content`: The text response from the assistant (may be `None` if only tool calls are returned)
/// - `tool_calls`: Any tool/function calls the LLM wants to execute
/// - `model`: The specific model that generated this response
/// - `usage`: Token usage statistics for the request (may be `None` if provider doesn't report it)
///
/// # Response Types
///
/// Depending on the request and LLM decision, a response can contain:
///
/// - **Text only**: `content` is `Some`, `tool_calls` is empty
/// - **Tool calls only**: `content` is `None`, `tool_calls` has entries
/// - **Both**: `content` is `Some` with explanation, `tool_calls` has entries
///
/// # Examples
///
/// ## Simple Text Response
///
/// ```rust
/// use at_harness::provider::{Response, Usage};
///
/// let response = Response {
///     content: Some("The capital of France is Paris.".to_string()),
///     tool_calls: vec![],
///     model: "claude-sonnet-4-20250514".to_string(),
///     usage: Some(Usage {
///         input_tokens: 15,
///         output_tokens: 8,
///     }),
/// };
///
/// if let Some(text) = &response.content {
///     println!("Assistant: {}", text);
/// }
/// ```
///
/// ## Response with Tool Calls
///
/// ```rust
/// use at_harness::provider::{Response, ToolCall};
///
/// # let response = Response {
/// #     content: None,
/// #     tool_calls: vec![ToolCall {
/// #         id: "call_abc123".to_string(),
/// #         name: "get_weather".to_string(),
/// #         arguments: r#"{"city":"Tokyo"}"#.to_string(),
/// #     }],
/// #     model: "gpt-4".to_string(),
/// #     usage: None,
/// # };
/// // Response with tool calls but no text
/// for tool_call in &response.tool_calls {
///     println!("LLM wants to call: {}", tool_call.name);
///     println!("With arguments: {}", tool_call.arguments);
/// }
/// ```
///
/// ## Checking Usage
///
/// ```rust
/// use at_harness::provider::Response;
///
/// # let response = Response {
/// #     content: Some("Hello".to_string()),
/// #     tool_calls: vec![],
/// #     model: "claude-sonnet-4-20250514".to_string(),
/// #     usage: Some(at_harness::provider::Usage {
/// #         input_tokens: 10,
/// #         output_tokens: 5,
/// #     }),
/// # };
/// if let Some(usage) = &response.usage {
///     let total = usage.input_tokens + usage.output_tokens;
///     println!("Total tokens used: {}", total);
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// The text content of the assistant's response.
    ///
    /// This field contains the LLM's natural language reply to the user's message.
    /// It may be `None` in cases where the LLM only returns tool calls without
    /// accompanying text.
    ///
    /// # When `None`
    ///
    /// - The LLM determined it needs to call tools without providing text
    /// - Some providers return empty content when making tool calls
    ///
    /// # When `Some`
    ///
    /// - Normal conversational responses
    /// - Explanations accompanying tool calls
    /// - Answers derived from tool call results (in follow-up turns)
    pub content: Option<String>,

    /// Tool/function calls requested by the LLM.
    ///
    /// When the LLM determines it needs to call one or more tools to answer the
    /// user's question, it returns [`ToolCall`] instances in this vector. Each
    /// tool call contains:
    /// - A unique ID for tracking
    /// - The tool name to execute
    /// - JSON-encoded arguments for the function
    ///
    /// Execute these tool calls and send results back as [`Role::Tool`] messages
    /// to continue the conversation. See [`ToolCall`] for a complete workflow example.
    ///
    /// This field defaults to an empty vector if the provider doesn't return tool calls.
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,

    /// The specific model that generated this response.
    ///
    /// This is the exact model identifier used by the provider (e.g.,
    /// "claude-sonnet-4-20250514", "gpt-4-turbo"). It may differ from the
    /// requested model if the provider performs substitution or aliasing.
    ///
    /// Use this for logging, debugging, or tracking which model version
    /// produced specific outputs.
    pub model: String,

    /// Token usage statistics for this request.
    ///
    /// Contains the number of input and output tokens consumed by this chat
    /// completion. Useful for:
    /// - Tracking API costs (tokens typically map to pricing)
    /// - Monitoring usage quotas
    /// - Optimizing prompt efficiency
    ///
    /// May be `None` if the provider doesn't report usage statistics or if
    /// usage tracking is disabled.
    pub usage: Option<Usage>,
}

/// Token usage statistics for an LLM request.
///
/// This struct tracks the number of tokens consumed by a chat completion request,
/// split between input (prompt) and output (completion) tokens. Token counts are
/// essential for:
///
/// - **Cost tracking**: Most LLM providers charge per token, often with different
///   rates for input vs. output tokens
/// - **Quota management**: Tracking usage against rate limits or subscription quotas
/// - **Optimization**: Identifying opportunities to reduce prompt size or output length
///
/// # Token Counting
///
/// Token counts are determined by the provider using their specific tokenization
/// algorithm (e.g., BPE, SentencePiece). The same text may have different token
/// counts across providers.
///
/// - `input_tokens`: Includes all message content, system prompts, and tool definitions
/// - `output_tokens`: Includes only the assistant's generated response
///
/// # Examples
///
/// ```rust
/// use at_harness::provider::Usage;
///
/// let usage = Usage {
///     input_tokens: 150,
///     output_tokens: 75,
/// };
///
/// let total = usage.input_tokens + usage.output_tokens;
/// println!("Total tokens: {}", total);
///
/// // Calculate cost (example rates)
/// let input_cost = usage.input_tokens as f64 * 0.001;  // $0.001 per token
/// let output_cost = usage.output_tokens as f64 * 0.002; // $0.002 per token
/// let total_cost = input_cost + output_cost;
/// println!("Estimated cost: ${:.4}", total_cost);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// Number of tokens in the input (prompt).
    ///
    /// This includes:
    /// - All user, assistant, and system messages in the conversation
    /// - Tool definitions (if tools were provided)
    /// - Any additional context or instructions
    ///
    /// Input tokens are typically cheaper than output tokens in provider pricing.
    pub input_tokens: u64,

    /// Number of tokens in the output (completion).
    ///
    /// This includes:
    /// - The assistant's text response
    /// - Any tool call names and arguments (if applicable)
    ///
    /// Output tokens are typically more expensive than input tokens in provider pricing.
    pub output_tokens: u64,
}

// ---------------------------------------------------------------------------
// LlmProvider trait
// ---------------------------------------------------------------------------

/// Async trait for LLM provider implementations.
///
/// This trait defines the interface for interacting with LLM providers such as
/// Anthropic, OpenAI, or custom implementations. Implementations handle the
/// provider-specific API calls, authentication, and response mapping.
///
/// # Required Methods
///
/// - [`chat`](LlmProvider::chat): Send a chat completion request with optional tool calling
/// - [`name`](LlmProvider::name): Return a human-readable provider identifier
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to support concurrent usage across async tasks.
/// Most providers use an internal HTTP client (like `reqwest::Client`) which is already
/// `Send + Sync`, making this requirement straightforward to satisfy.
///
/// # Implementation Guide
///
/// To create a new provider implementation:
///
/// 1. **Define a provider struct** with necessary client state (API key, HTTP client, etc.)
/// 2. **Implement the `LlmProvider` trait** with provider-specific API logic
/// 3. **Map provider errors** to the standardized [`ProviderError`] variants
/// 4. **Handle tool calling** if your provider supports function calling
///
/// ## Example Implementation
///
/// ```rust
/// use at_harness::provider::{LlmProvider, Message, Tool, Response, ProviderError, Usage};
/// use async_trait::async_trait;
///
/// /// Custom LLM provider implementation.
/// pub struct MyCustomProvider {
///     client: reqwest::Client,
///     api_key: String,
///     base_url: String,
/// }
///
/// impl MyCustomProvider {
///     /// Create a new provider instance.
///     pub fn new(api_key: impl Into<String>) -> Self {
///         Self {
///             client: reqwest::Client::new(),
///             api_key: api_key.into(),
///             base_url: "https://api.example.com".to_string(),
///         }
///     }
///
///     /// Build the provider-specific request payload.
///     fn build_request_body(
///         &self,
///         messages: &[Message],
///         tools: Option<&[Tool]>,
///     ) -> serde_json::Value {
///         // Transform our standard Message format to provider's API format
///         let api_messages: Vec<_> = messages
///             .iter()
///             .map(|msg| {
///                 serde_json::json!({
///                     "role": msg.role,
///                     "content": msg.content,
///                 })
///             })
///             .collect();
///
///         let mut body = serde_json::json!({
///             "messages": api_messages,
///             "model": "my-model-v1",
///         });
///
///         // Add tools if provided
///         if let Some(tools) = tools {
///             body["tools"] = serde_json::json!(tools);
///         }
///
///         body
///     }
/// }
///
/// #[async_trait]
/// impl LlmProvider for MyCustomProvider {
///     async fn chat(
///         &self,
///         messages: Vec<Message>,
///         tools: Option<Vec<Tool>>,
///     ) -> Result<Response, ProviderError> {
///         // Build the request payload
///         let body = self.build_request_body(&messages, tools.as_deref());
///
///         // Make the API request
///         let response = self
///             .client
///             .post(format!("{}/v1/chat/completions", self.base_url))
///             .header("Authorization", format!("Bearer {}", self.api_key))
///             .json(&body)
///             .send()
///             .await
///             .map_err(|e| {
///                 if e.is_timeout() {
///                     ProviderError::Timeout
///                 } else {
///                     ProviderError::Other(e.to_string())
///                 }
///             })?;
///
///         // Handle rate limiting
///         if response.status() == 429 {
///             let retry_after_ms = response
///                 .headers()
///                 .get("retry-after")
///                 .and_then(|h| h.to_str().ok())
///                 .and_then(|s| s.parse::<u64>().ok())
///                 .unwrap_or(1000);
///
///             return Err(ProviderError::RateLimited { retry_after_ms });
///         }
///
///         // Handle API errors
///         if !response.status().is_success() {
///             let error_text = response.text().await.unwrap_or_default();
///             return Err(ProviderError::Api(format!(
///                 "HTTP {}: {}",
///                 response.status(),
///                 error_text
///             )));
///         }
///
///         // Parse the response
///         let api_response: serde_json::Value = response
///             .json()
///             .await
///             .map_err(|e| ProviderError::Other(format!("parse error: {}", e)))?;
///
///         // Map to our standard Response format
///         Ok(Response {
///             content: api_response["choices"][0]["message"]["content"]
///                 .as_str()
///                 .map(String::from),
///             tool_calls: vec![], // Parse tool calls if provider supports them
///             model: api_response["model"]
///                 .as_str()
///                 .unwrap_or("unknown")
///                 .to_string(),
///             usage: api_response.get("usage").map(|u| Usage {
///                 input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0),
///                 output_tokens: u["completion_tokens"].as_u64().unwrap_or(0),
///             }),
///         })
///     }
///
///     fn name(&self) -> &str {
///         "my-custom-provider"
///     }
/// }
/// ```
///
/// ## Error Mapping
///
/// Map provider-specific errors to [`ProviderError`] variants:
///
/// - **Authentication/configuration issues** → [`ProviderError::NotConfigured`]
/// - **API errors (4xx/5xx)** → [`ProviderError::Api`]
/// - **Rate limits (HTTP 429)** → [`ProviderError::RateLimited`]
/// - **Timeouts** → [`ProviderError::Timeout`]
/// - **Other errors** → [`ProviderError::Other`]
///
/// ## Tool Calling Support
///
/// If your provider supports tool calling:
///
/// 1. Include tools in the request payload when `tools` parameter is `Some`
/// 2. Parse tool calls from the API response
/// 3. Map them to [`ToolCall`] instances in [`Response::tool_calls`]
///
/// See the [`Tool`] and [`ToolCall`] documentation for the complete workflow.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request with optional tool calling.
    ///
    /// This method sends a conversation to the LLM and returns the assistant's response.
    /// It supports multi-turn conversations by accepting a history of messages, and
    /// enables tool calling by accepting an optional list of tool definitions.
    ///
    /// # Parameters
    ///
    /// - `messages`: The conversation history, including user, assistant, system, and tool messages
    /// - `tools`: Optional list of tools the LLM can call (enables function calling)
    ///
    /// # Returns
    ///
    /// - `Ok(Response)`: The LLM's response, potentially including tool calls
    /// - `Err(ProviderError)`: An error from the provider or network layer
    ///
    /// # Errors
    ///
    /// This method can return various [`ProviderError`] variants:
    ///
    /// - [`NotConfigured`](ProviderError::NotConfigured): Missing API credentials
    /// - [`Api`](ProviderError::Api): Provider API returned an error
    /// - [`RateLimited`](ProviderError::RateLimited): Request was rate limited
    /// - [`Timeout`](ProviderError::Timeout): Request timed out
    /// - [`Other`](ProviderError::Other): Network or parsing errors
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use at_harness::provider::{LlmProvider, Message, ProviderError};
    ///
    /// async fn simple_chat(provider: impl LlmProvider) -> Result<(), ProviderError> {
    ///     let messages = vec![
    ///         Message::system("You are a helpful assistant."),
    ///         Message::user("What is 2+2?"),
    ///     ];
    ///
    ///     let response = provider.chat(messages, None).await?;
    ///     println!("Assistant: {}", response.content.unwrap_or_default());
    ///     Ok(())
    /// }
    /// ```
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
    ) -> Result<Response, ProviderError>;

    /// Return a human-readable provider name.
    ///
    /// This method returns a string identifier for the provider, used for logging,
    /// debugging, and display purposes. Common examples include "anthropic", "openai",
    /// "gemini", etc.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use at_harness::provider::{LlmProvider, StubProvider};
    ///
    /// let provider = StubProvider::new("test-provider");
    /// assert_eq!(provider.name(), "test-provider");
    /// ```
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
