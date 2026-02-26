//! LLM provider abstraction layer.
//!
//! Provides a unified async trait for interacting with various LLM providers
//! (Anthropic, OpenAI, etc.) along with a mock provider for testing.

use std::collections::VecDeque;
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Semaphore;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur when interacting with an LLM provider.
#[derive(Debug, Error)]
pub enum LlmError {
    /// An HTTP-level error (connection failure, DNS, TLS, etc.).
    #[error("HTTP error: {0}")]
    HttpError(String),

    /// The API returned a non-success status with a message.
    #[error("API error (status {status}): {message}")]
    ApiError { status: u16, message: String },

    /// Failed to parse the API response body.
    #[error("parse error: {0}")]
    ParseError(String),

    /// The API indicated rate limiting (HTTP 429).
    #[error("rate limited: retry after {retry_after_secs:?}s")]
    RateLimited { retry_after_secs: Option<u64> },

    /// The request timed out.
    #[error("request timed out")]
    Timeout,

    /// The requested operation is not supported by this provider.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

impl From<reqwest::Error> for LlmError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            LlmError::Timeout
        } else {
            LlmError::HttpError(err.to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// Role of a message participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmRole {
    System,
    User,
    Assistant,
}

impl fmt::Display for LlmRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmRole::System => write!(f, "system"),
            LlmRole::User => write!(f, "user"),
            LlmRole::Assistant => write!(f, "assistant"),
        }
    }
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
}

impl LlmMessage {
    pub fn new(role: LlmRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(LlmRole::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(LlmRole::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(LlmRole::Assistant, content)
    }
}

/// Configuration for an LLM completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub system_prompt: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1024,
            temperature: 0.7,
            system_prompt: None,
        }
    }
}

/// Response from an LLM completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub finish_reason: String,
}

// ---------------------------------------------------------------------------
// LlmProvider trait
// ---------------------------------------------------------------------------

/// Async trait for LLM providers.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a completion request and return the full response.
    async fn complete(
        &self,
        messages: &[LlmMessage],
        config: &LlmConfig,
    ) -> Result<LlmResponse, LlmError>;

    /// Stream a completion response token-by-token.
    ///
    /// Providers that do not support streaming should return
    /// `Err(LlmError::Unsupported(...))`.
    async fn stream(
        &self,
        messages: &[LlmMessage],
        config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError>;
}

// ---------------------------------------------------------------------------
// AnthropicProvider
// ---------------------------------------------------------------------------

/// LLM provider for the Anthropic Messages API.
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    ///
    /// `api_key` is the Anthropic API key (x-api-key header).
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.anthropic.com".to_string(),
        }
    }

    /// Override the base URL (useful for testing with a mock server).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Build the JSON request body for the Anthropic Messages API.
    pub fn build_request_body(messages: &[LlmMessage], config: &LlmConfig) -> serde_json::Value {
        // Anthropic API: system prompt goes in the top-level `system` field,
        // not as a message. Filter system messages out of the messages array.
        let mut system_text: Option<String> = config.system_prompt.clone();

        let api_messages: Vec<serde_json::Value> = messages
            .iter()
            .filter_map(|msg| {
                if msg.role == LlmRole::System {
                    // Accumulate system messages into the system field.
                    if let Some(ref mut s) = system_text {
                        s.push('\n');
                        s.push_str(&msg.content);
                    } else {
                        system_text = Some(msg.content.clone());
                    }
                    None
                } else {
                    Some(serde_json::json!({
                        "role": msg.role.to_string(),
                        "content": msg.content,
                    }))
                }
            })
            .collect();

        let mut body = serde_json::json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
            "messages": api_messages,
        });

        if let Some(system) = system_text {
            body["system"] = serde_json::Value::String(system);
        }

        body
    }
}

/// Deserialize helpers for Anthropic API response.
#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    _type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        config: &LlmConfig,
    ) -> Result<LlmResponse, LlmError> {
        let body = Self::build_request_body(messages, config);
        let url = format!("{}/v1/messages", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status().as_u16();

        if status == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            return Err(LlmError::RateLimited {
                retry_after_secs: retry_after,
            });
        }

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status,
                message: text,
            });
        }

        let api_resp: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| LlmError::ParseError(e.to_string()))?;

        let content = api_resp
            .content
            .iter()
            .filter_map(|block| block.text.as_deref())
            .collect::<Vec<_>>()
            .join("");

        Ok(LlmResponse {
            content,
            model: api_resp.model,
            input_tokens: api_resp.usage.input_tokens,
            output_tokens: api_resp.usage.output_tokens,
            finish_reason: api_resp.stop_reason.unwrap_or_else(|| "unknown".into()),
        })
    }

    async fn stream(
        &self,
        _messages: &[LlmMessage],
        _config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        Err(LlmError::Unsupported(
            "streaming not yet implemented for AnthropicProvider".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// OpenAiProvider
// ---------------------------------------------------------------------------

/// LLM provider for the OpenAI Chat Completions API.
pub struct OpenAiProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenAiProvider {
    /// Create a new OpenAI provider.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.openai.com".to_string(),
        }
    }

    /// Override the base URL (useful for testing or Azure OpenAI).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Build the JSON request body for the OpenAI Chat Completions API.
    pub fn build_request_body(messages: &[LlmMessage], config: &LlmConfig) -> serde_json::Value {
        // OpenAI format: system messages go inline in the messages array.
        let mut api_messages: Vec<serde_json::Value> = Vec::new();

        // If there is a system_prompt in config, prepend it.
        if let Some(ref system) = config.system_prompt {
            api_messages.push(serde_json::json!({
                "role": "system",
                "content": system,
            }));
        }

        for msg in messages {
            api_messages.push(serde_json::json!({
                "role": msg.role.to_string(),
                "content": msg.content,
            }));
        }

        serde_json::json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
            "messages": api_messages,
        })
    }
}

/// Deserialize helpers for OpenAI API response.
#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    model: String,
    usage: OpenAiUsage,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessageResp,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiMessageResp {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        config: &LlmConfig,
    ) -> Result<LlmResponse, LlmError> {
        let body = Self::build_request_body(messages, config);
        let url = format!("{}/v1/chat/completions", self.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status().as_u16();

        if status == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            return Err(LlmError::RateLimited {
                retry_after_secs: retry_after,
            });
        }

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status,
                message: text,
            });
        }

        let api_resp: OpenAiResponse = resp
            .json()
            .await
            .map_err(|e| LlmError::ParseError(e.to_string()))?;

        let choice = api_resp
            .choices
            .first()
            .ok_or_else(|| LlmError::ParseError("no choices in response".into()))?;

        Ok(LlmResponse {
            content: choice.message.content.clone().unwrap_or_default(),
            model: api_resp.model,
            input_tokens: api_resp.usage.prompt_tokens,
            output_tokens: api_resp.usage.completion_tokens,
            finish_reason: choice
                .finish_reason
                .clone()
                .unwrap_or_else(|| "unknown".into()),
        })
    }

    async fn stream(
        &self,
        _messages: &[LlmMessage],
        _config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        Err(LlmError::Unsupported(
            "streaming not yet implemented for OpenAiProvider".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// LocalProvider — local inference via OpenAI-compatible API
// ---------------------------------------------------------------------------

/// LLM provider for local inference servers that expose an OpenAI-compatible
/// chat completions endpoint (vllm.rs, llama.cpp, Ollama, candle-based servers,
/// text-generation-inference, etc.).
///
/// The provider speaks the standard `/v1/chat/completions` protocol. Most local
/// inference servers support this format out of the box. Authentication is
/// optional — many local servers run without API keys.
///
/// # Configuration
///
/// - `base_url`: URL of the local server (default: `http://localhost:8000`)
/// - `api_key`: Optional; pass empty string or `"none"` if the server doesn't
///   require authentication
///
/// # Supported servers
///
/// - **vllm** / **vllm.rs**: `--api-key` flag optional, default port 8000
/// - **llama.cpp server**: `--api-key` flag optional, default port 8080
/// - **Ollama**: default port 11434, uses `/api/chat` but also supports
///   OpenAI-compatible endpoint at `/v1/chat/completions`
/// - **text-generation-inference (TGI)**: default port 8080
/// - **candle** server: when run with the OpenAI-compatible wrapper
pub struct LocalProvider {
    client: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
}

fn local_llm_max_concurrent() -> usize {
    std::env::var("AT_LOCAL_LLM_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(1)
}

fn local_llm_gate() -> Arc<Semaphore> {
    static LOCAL_LLM_GATE: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LOCAL_LLM_GATE
        .get_or_init(|| Arc::new(Semaphore::new(local_llm_max_concurrent())))
        .clone()
}

impl LocalProvider {
    /// Create a new local inference provider.
    ///
    /// `base_url` is the server address (e.g., `"http://localhost:8000"`).
    /// `api_key` is optional — pass `None` for servers without auth.
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        let key = api_key.filter(|k| !k.is_empty() && k != "none");
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120)) // local inference can be slow
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_key: key,
            base_url: base_url.into(),
        }
    }
}

/// Deserialize helpers for OpenAI-compatible local server responses.
///
/// Reuses the same JSON schema as OpenAI Chat Completions API since
/// all supported local servers implement this format.
#[derive(Deserialize)]
struct LocalResponse {
    choices: Vec<LocalChoice>,
    model: Option<String>,
    usage: Option<LocalUsage>,
}

#[derive(Deserialize)]
struct LocalChoice {
    message: LocalMessageResp,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct LocalMessageResp {
    content: Option<String>,
}

#[derive(Deserialize)]
struct LocalUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

#[async_trait]
impl LlmProvider for LocalProvider {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        config: &LlmConfig,
    ) -> Result<LlmResponse, LlmError> {
        // Queue local inference calls to prevent model-server overload when many
        // agent subscribers are active concurrently.
        let _permit = local_llm_gate()
            .acquire_owned()
            .await
            .map_err(|_| LlmError::HttpError("local LLM queue unavailable".into()))?;

        // Build messages in OpenAI format (system messages inline).
        let mut api_messages: Vec<serde_json::Value> = Vec::new();

        if let Some(ref system) = config.system_prompt {
            api_messages.push(serde_json::json!({
                "role": "system",
                "content": system,
            }));
        }

        for msg in messages {
            api_messages.push(serde_json::json!({
                "role": msg.role.to_string(),
                "content": msg.content,
            }));
        }

        let body = serde_json::json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
            "messages": api_messages,
        });

        let url = format!("{}/v1/chat/completions", self.base_url);

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                LlmError::Timeout
            } else if e.is_connect() {
                LlmError::HttpError(format!(
                    "cannot connect to local inference server at {}: {}",
                    self.base_url, e
                ))
            } else {
                LlmError::HttpError(e.to_string())
            }
        })?;

        let status = resp.status().as_u16();

        if status == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            return Err(LlmError::RateLimited {
                retry_after_secs: retry_after,
            });
        }

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status,
                message: text,
            });
        }

        let api_resp: LocalResponse = resp
            .json()
            .await
            .map_err(|e| LlmError::ParseError(e.to_string()))?;

        let choice = api_resp
            .choices
            .first()
            .ok_or_else(|| LlmError::ParseError("no choices in local response".into()))?;

        let usage = api_resp.usage.as_ref();

        Ok(LlmResponse {
            content: choice.message.content.clone().unwrap_or_default(),
            model: api_resp.model.unwrap_or_else(|| config.model.clone()),
            input_tokens: usage.and_then(|u| u.prompt_tokens).unwrap_or(0),
            output_tokens: usage.and_then(|u| u.completion_tokens).unwrap_or(0),
            finish_reason: choice
                .finish_reason
                .clone()
                .unwrap_or_else(|| "stop".into()),
        })
    }

    async fn stream(
        &self,
        _messages: &[LlmMessage],
        _config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        Err(LlmError::Unsupported(
            "streaming not yet implemented for LocalProvider".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// MockProvider
// ---------------------------------------------------------------------------

/// A mock LLM provider for testing.
///
/// Returns pre-configured responses. Each call to `complete` pops the next
/// response from the queue. If the queue is empty, returns a default response.
pub struct MockProvider {
    responses: Arc<Mutex<VecDeque<Result<LlmResponse, LlmError>>>>,
    /// Captured request bodies for test assertions.
    #[allow(clippy::type_complexity)]
    captured_requests: Arc<Mutex<Vec<(Vec<LlmMessage>, LlmConfig)>>>,
}

impl MockProvider {
    /// Create a mock provider with no pre-configured responses (returns defaults).
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::new())),
            captured_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Queue a successful response.
    pub fn with_response(self, response: LlmResponse) -> Self {
        self.responses.lock().unwrap().push_back(Ok(response));
        self
    }

    /// Queue an error response.
    pub fn with_error(self, error: LlmError) -> Self {
        self.responses.lock().unwrap().push_back(Err(error));
        self
    }

    /// Get captured requests for assertions.
    pub fn captured_requests(&self) -> Vec<(Vec<LlmMessage>, LlmConfig)> {
        self.captured_requests.lock().unwrap().clone()
    }

    fn default_response(model: &str) -> LlmResponse {
        LlmResponse {
            content: "Mock response".to_string(),
            model: model.to_string(),
            input_tokens: 10,
            output_tokens: 5,
            finish_reason: "end_turn".to_string(),
        }
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        config: &LlmConfig,
    ) -> Result<LlmResponse, LlmError> {
        self.captured_requests
            .lock()
            .unwrap()
            .push((messages.to_vec(), config.clone()));

        let mut queue = self.responses.lock().unwrap();
        if queue.is_empty() {
            Ok(Self::default_response(&config.model))
        } else {
            queue.pop_front().unwrap()
        }
    }

    async fn stream(
        &self,
        _messages: &[LlmMessage],
        _config: &LlmConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        Err(LlmError::Unsupported(
            "streaming not implemented for MockProvider".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// LlmUsageTracker
// ---------------------------------------------------------------------------

/// Simple tracker for cumulative LLM usage across multiple requests.
#[derive(Debug, Clone, Default)]
pub struct LlmUsageTracker {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_requests: u64,
}

impl LlmUsageTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record usage from an [`LlmResponse`].
    pub fn record(&mut self, response: &LlmResponse) {
        self.total_input_tokens += response.input_tokens;
        self.total_output_tokens += response.output_tokens;
        self.total_requests += 1;
    }

    /// Total tokens (input + output) across all tracked requests.
    pub fn total_tokens(&self) -> u64 {
        self.total_input_tokens + self.total_output_tokens
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> LlmConfig {
        LlmConfig {
            model: "test-model".to_string(),
            max_tokens: 512,
            temperature: 0.5,
            system_prompt: None,
        }
    }

    // -- MockProvider tests --------------------------------------------------

    #[tokio::test]
    async fn mock_provider_returns_default_response() {
        let provider = MockProvider::new();
        let config = default_config();
        let messages = vec![LlmMessage::user("Hello")];

        let resp = provider.complete(&messages, &config).await.unwrap();
        assert_eq!(resp.content, "Mock response");
        assert_eq!(resp.model, "test-model");
        assert_eq!(resp.input_tokens, 10);
        assert_eq!(resp.output_tokens, 5);
    }

    #[tokio::test]
    async fn mock_provider_returns_queued_response() {
        let custom = LlmResponse {
            content: "Custom answer".to_string(),
            model: "custom-model".to_string(),
            input_tokens: 42,
            output_tokens: 99,
            finish_reason: "stop".to_string(),
        };
        let provider = MockProvider::new().with_response(custom);
        let config = default_config();

        let resp = provider
            .complete(&[LlmMessage::user("Hi")], &config)
            .await
            .unwrap();
        assert_eq!(resp.content, "Custom answer");
        assert_eq!(resp.model, "custom-model");
        assert_eq!(resp.input_tokens, 42);

        // Second call falls back to default since queue is empty.
        let resp2 = provider
            .complete(&[LlmMessage::user("Hi again")], &config)
            .await
            .unwrap();
        assert_eq!(resp2.content, "Mock response");
    }

    #[tokio::test]
    async fn mock_provider_returns_queued_error() {
        let provider = MockProvider::new().with_error(LlmError::Timeout);
        let config = default_config();

        let result = provider.complete(&[LlmMessage::user("Hi")], &config).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::Timeout));
    }

    #[tokio::test]
    async fn mock_provider_captures_requests() {
        let provider = MockProvider::new();
        let config = default_config();
        let messages = vec![
            LlmMessage::system("You are helpful"),
            LlmMessage::user("Hello"),
        ];

        provider.complete(&messages, &config).await.unwrap();

        let captured = provider.captured_requests();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].0.len(), 2);
        assert_eq!(captured[0].0[0].role, LlmRole::System);
        assert_eq!(captured[0].0[1].content, "Hello");
    }

    #[tokio::test]
    async fn mock_provider_stream_returns_unsupported() {
        let provider = MockProvider::new();
        let config = default_config();

        let result = provider.stream(&[LlmMessage::user("Hi")], &config).await;
        assert!(result.is_err());
        match result {
            Err(LlmError::Unsupported(_)) => {} // expected
            _ => panic!("expected LlmError::Unsupported"),
        }
    }

    // -- LlmMessage / LlmConfig serialization tests -------------------------

    #[test]
    fn llm_message_serialization_roundtrip() {
        let msg = LlmMessage::user("Hello world");
        let json = serde_json::to_string(&msg).unwrap();
        let deser: LlmMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.role, LlmRole::User);
        assert_eq!(deser.content, "Hello world");
    }

    #[test]
    fn llm_config_serialization_roundtrip() {
        let config = LlmConfig {
            model: "gpt-4".to_string(),
            max_tokens: 2048,
            temperature: 0.9,
            system_prompt: Some("Be concise".to_string()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deser: LlmConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.model, "gpt-4");
        assert_eq!(deser.max_tokens, 2048);
        assert!((deser.temperature - 0.9).abs() < f32::EPSILON);
        assert_eq!(deser.system_prompt.as_deref(), Some("Be concise"));
    }

    #[test]
    fn llm_response_serialization_roundtrip() {
        let resp = LlmResponse {
            content: "Answer".to_string(),
            model: "claude-3".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            finish_reason: "end_turn".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deser: LlmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.content, "Answer");
        assert_eq!(deser.input_tokens, 100);
    }

    #[test]
    fn llm_role_serialization() {
        let json = serde_json::to_string(&LlmRole::System).unwrap();
        assert_eq!(json, "\"system\"");
        let json = serde_json::to_string(&LlmRole::User).unwrap();
        assert_eq!(json, "\"user\"");
        let json = serde_json::to_string(&LlmRole::Assistant).unwrap();
        assert_eq!(json, "\"assistant\"");
    }

    // -- AnthropicProvider request body tests --------------------------------

    #[test]
    fn anthropic_request_body_basic() {
        let messages = vec![LlmMessage::user("What is Rust?")];
        let config = LlmConfig {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1024,
            temperature: 0.7,
            system_prompt: None,
        };

        let body = AnthropicProvider::build_request_body(&messages, &config);

        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["max_tokens"], 1024);
        let temp = body["temperature"].as_f64().unwrap();
        assert!((temp - 0.7).abs() < 0.01);
        assert!(body.get("system").is_none());

        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "What is Rust?");
    }

    #[test]
    fn anthropic_request_body_with_system_prompt() {
        let messages = vec![LlmMessage::user("Hello")];
        let config = LlmConfig {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 512,
            temperature: 0.5,
            system_prompt: Some("You are a helpful assistant".to_string()),
        };

        let body = AnthropicProvider::build_request_body(&messages, &config);

        assert_eq!(body["system"], "You are a helpful assistant");
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        // System message should NOT appear in messages array for Anthropic.
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn anthropic_request_body_system_messages_extracted() {
        let messages = vec![
            LlmMessage::system("Be concise"),
            LlmMessage::user("Hi"),
            LlmMessage::assistant("Hello!"),
            LlmMessage::user("What is 2+2?"),
        ];
        let config = LlmConfig {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 256,
            temperature: 0.0,
            system_prompt: None,
        };

        let body = AnthropicProvider::build_request_body(&messages, &config);

        // System messages should be extracted to the top-level system field.
        assert_eq!(body["system"], "Be concise");
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3); // user, assistant, user (no system)
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[1]["role"], "assistant");
        assert_eq!(msgs[2]["role"], "user");
    }

    #[test]
    fn anthropic_request_body_config_system_plus_message_system() {
        let messages = vec![
            LlmMessage::system("Additional instruction"),
            LlmMessage::user("Hi"),
        ];
        let config = LlmConfig {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 256,
            temperature: 0.0,
            system_prompt: Some("Base system prompt".to_string()),
        };

        let body = AnthropicProvider::build_request_body(&messages, &config);

        // Both should be concatenated.
        let system = body["system"].as_str().unwrap();
        assert!(system.contains("Base system prompt"));
        assert!(system.contains("Additional instruction"));
    }

    // -- OpenAiProvider request body tests -----------------------------------

    #[test]
    fn openai_request_body_basic() {
        let messages = vec![LlmMessage::user("What is Rust?")];
        let config = LlmConfig {
            model: "gpt-4".to_string(),
            max_tokens: 1024,
            temperature: 0.7,
            system_prompt: None,
        };

        let body = OpenAiProvider::build_request_body(&messages, &config);

        assert_eq!(body["model"], "gpt-4");
        assert_eq!(body["max_tokens"], 1024);
        let temp = body["temperature"].as_f64().unwrap();
        assert!((temp - 0.7).abs() < 0.01);

        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "What is Rust?");
    }

    #[test]
    fn openai_request_body_with_system_prompt() {
        let messages = vec![LlmMessage::user("Hello")];
        let config = LlmConfig {
            model: "gpt-4".to_string(),
            max_tokens: 512,
            temperature: 0.5,
            system_prompt: Some("You are a helpful assistant".to_string()),
        };

        let body = OpenAiProvider::build_request_body(&messages, &config);

        let msgs = body["messages"].as_array().unwrap();
        // System prompt from config should be prepended as first message.
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are a helpful assistant");
        assert_eq!(msgs[1]["role"], "user");
    }

    #[test]
    fn openai_request_body_system_messages_inline() {
        let messages = vec![
            LlmMessage::system("Be concise"),
            LlmMessage::user("Hi"),
            LlmMessage::assistant("Hello!"),
        ];
        let config = LlmConfig {
            model: "gpt-4".to_string(),
            max_tokens: 256,
            temperature: 0.0,
            system_prompt: None,
        };

        let body = OpenAiProvider::build_request_body(&messages, &config);

        let msgs = body["messages"].as_array().unwrap();
        // OpenAI keeps system messages inline.
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "Be concise");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[2]["role"], "assistant");
    }

    #[test]
    fn openai_request_body_config_system_prepended() {
        let messages = vec![
            LlmMessage::system("Additional system"),
            LlmMessage::user("Hi"),
        ];
        let config = LlmConfig {
            model: "gpt-4".to_string(),
            max_tokens: 256,
            temperature: 0.0,
            system_prompt: Some("Base system".to_string()),
        };

        let body = OpenAiProvider::build_request_body(&messages, &config);

        let msgs = body["messages"].as_array().unwrap();
        // Config system prompt comes first, then inline system message.
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "Base system");
        assert_eq!(msgs[1]["role"], "system");
        assert_eq!(msgs[1]["content"], "Additional system");
        assert_eq!(msgs[2]["role"], "user");
    }

    // -- Error type tests ----------------------------------------------------

    #[test]
    fn error_display_messages() {
        let e = LlmError::HttpError("connection refused".into());
        assert!(e.to_string().contains("connection refused"));

        let e = LlmError::ApiError {
            status: 400,
            message: "bad request".into(),
        };
        assert!(e.to_string().contains("400"));
        assert!(e.to_string().contains("bad request"));

        let e = LlmError::ParseError("invalid json".into());
        assert!(e.to_string().contains("invalid json"));

        let e = LlmError::RateLimited {
            retry_after_secs: Some(30),
        };
        assert!(e.to_string().contains("30"));

        let e = LlmError::Timeout;
        assert!(e.to_string().contains("timed out"));

        let e = LlmError::Unsupported("streaming".into());
        assert!(e.to_string().contains("streaming"));
    }

    #[test]
    fn error_rate_limited_no_retry_after() {
        let e = LlmError::RateLimited {
            retry_after_secs: None,
        };
        let display = e.to_string();
        assert!(display.contains("rate limited"));
    }

    // -- Convenience constructors -------------------------------------------

    #[test]
    fn llm_message_convenience_constructors() {
        let s = LlmMessage::system("sys");
        assert_eq!(s.role, LlmRole::System);
        assert_eq!(s.content, "sys");

        let u = LlmMessage::user("usr");
        assert_eq!(u.role, LlmRole::User);

        let a = LlmMessage::assistant("ast");
        assert_eq!(a.role, LlmRole::Assistant);
    }

    #[test]
    fn llm_config_default() {
        let config = LlmConfig::default();
        assert!(!config.model.is_empty());
        assert!(config.max_tokens > 0);
        assert!(config.temperature > 0.0);
        assert!(config.system_prompt.is_none());
    }

    // -- Provider trait object safety ----------------------------------------

    #[tokio::test]
    async fn provider_as_trait_object() {
        let provider: Box<dyn LlmProvider> = Box::new(MockProvider::new());
        let config = default_config();
        let resp = provider
            .complete(&[LlmMessage::user("test")], &config)
            .await
            .unwrap();
        assert_eq!(resp.content, "Mock response");
    }

    // -- LlmUsageTracker tests -----------------------------------------------

    #[test]
    fn usage_tracker_starts_empty() {
        let tracker = LlmUsageTracker::new();
        assert_eq!(tracker.total_input_tokens, 0);
        assert_eq!(tracker.total_output_tokens, 0);
        assert_eq!(tracker.total_requests, 0);
        assert_eq!(tracker.total_tokens(), 0);
    }

    #[test]
    fn usage_tracker_records_response() {
        let mut tracker = LlmUsageTracker::new();
        let resp = LlmResponse {
            content: "answer".to_string(),
            model: "test".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            finish_reason: "end_turn".to_string(),
        };

        tracker.record(&resp);
        assert_eq!(tracker.total_input_tokens, 100);
        assert_eq!(tracker.total_output_tokens, 50);
        assert_eq!(tracker.total_requests, 1);
        assert_eq!(tracker.total_tokens(), 150);

        // Record a second response.
        let resp2 = LlmResponse {
            content: "another".to_string(),
            model: "test".to_string(),
            input_tokens: 200,
            output_tokens: 75,
            finish_reason: "end_turn".to_string(),
        };
        tracker.record(&resp2);
        assert_eq!(tracker.total_input_tokens, 300);
        assert_eq!(tracker.total_output_tokens, 125);
        assert_eq!(tracker.total_requests, 2);
        assert_eq!(tracker.total_tokens(), 425);
    }

    // -- LocalProvider tests -------------------------------------------------

    #[test]
    fn local_provider_creation_with_api_key() {
        let provider = LocalProvider::new("http://localhost:8000", Some("test-key".into()));
        assert_eq!(provider.base_url, "http://localhost:8000");
        assert_eq!(provider.api_key, Some("test-key".into()));
    }

    #[test]
    fn local_provider_creation_without_api_key() {
        let provider = LocalProvider::new("http://localhost:8000", None);
        assert_eq!(provider.base_url, "http://localhost:8000");
        assert!(provider.api_key.is_none());
    }

    #[test]
    fn local_provider_empty_key_treated_as_none() {
        let provider = LocalProvider::new("http://localhost:8000", Some("".into()));
        assert!(provider.api_key.is_none());
    }

    #[test]
    fn local_provider_none_key_treated_as_none() {
        let provider = LocalProvider::new("http://localhost:8000", Some("none".into()));
        assert!(provider.api_key.is_none());
    }

    #[tokio::test]
    async fn local_provider_stream_returns_unsupported() {
        let provider = LocalProvider::new("http://localhost:9999", None);
        let config = default_config();
        let result = provider.stream(&[LlmMessage::user("Hi")], &config).await;
        assert!(result.is_err());
        match result {
            Err(LlmError::Unsupported(msg)) => {
                assert!(msg.contains("LocalProvider"));
            }
            _ => panic!("expected LlmError::Unsupported"),
        }
    }

    #[tokio::test]
    async fn local_provider_connection_refused_returns_http_error() {
        // Connect to a port where nothing is listening.
        let provider = LocalProvider::new("http://127.0.0.1:19999", None);
        let config = default_config();
        let result = provider.complete(&[LlmMessage::user("Hi")], &config).await;
        assert!(result.is_err());
        match result {
            Err(LlmError::HttpError(msg)) => {
                assert!(msg.contains("cannot connect") || msg.contains("error"));
            }
            Err(LlmError::Timeout) => {} // also acceptable on slow CI
            other => panic!("expected HttpError or Timeout, got: {:?}", other),
        }
    }

    #[test]
    fn local_provider_as_trait_object_compiles() {
        // Verify LocalProvider is object-safe for dyn LlmProvider.
        let _: Box<dyn LlmProvider> = Box::new(LocalProvider::new("http://localhost:8000", None));
    }

    #[test]
    fn local_response_deserializes_minimal() {
        // Minimal valid response from a local server.
        let json = r#"{
            "choices": [{
                "message": {"content": "Hello!"},
                "finish_reason": "stop"
            }]
        }"#;
        let resp: LocalResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.content.as_deref(), Some("Hello!"));
        assert!(resp.model.is_none()); // model field is optional
        assert!(resp.usage.is_none()); // usage field is optional
    }

    #[test]
    fn local_response_deserializes_full() {
        // Full response with all optional fields.
        let json = r#"{
            "choices": [{
                "message": {"content": "Hi there"},
                "finish_reason": "length"
            }],
            "model": "llama-2-70b",
            "usage": {
                "prompt_tokens": 42,
                "completion_tokens": 10
            }
        }"#;
        let resp: LocalResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.model.as_deref(), Some("llama-2-70b"));
        let usage = resp.usage.unwrap();
        assert_eq!(usage.prompt_tokens, Some(42));
        assert_eq!(usage.completion_tokens, Some(10));
    }
}
