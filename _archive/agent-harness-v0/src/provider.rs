use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use crate::types::*;
use crate::quota::QuotaTracker;
use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};

/// Trait for LLM providers.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request and get the full response.
    async fn chat_completion(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse, ProviderError>;

    /// Send a streaming chat completion request.
    async fn chat_completion_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), ProviderError>;

    /// Get the quota tracker for this provider.
    fn quota_tracker(&self) -> Arc<QuotaTracker>;

    /// Get the circuit breaker for this provider.
    fn circuit_breaker(&self) -> Arc<CircuitBreaker>;

    /// Get the model name for this provider.
    async fn get_model(&self) -> Option<String>;
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error ({status}): {body}")]
    Api { status: u16, body: String },
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Rate limited, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },
    #[error("Stream error: {0}")]
    Stream(String),
}

impl ProviderError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Http(_))
    }
}

// ---------------------------------------------------------------------------
// OpenRouter Provider (OpenAI-compatible API)
// ---------------------------------------------------------------------------

pub struct OpenRouterProvider {
    client: reqwest::Client,
    config: ProviderConfig,
    quota_tracker: Arc<QuotaTracker>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl OpenRouterProvider {
    pub fn new(config: ProviderConfig) -> Self {
        let circuit_config = CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            call_timeout: Duration::from_secs(30),
        };
        
        Self {
            client: reqwest::Client::new(),
            config,
            quota_tracker: Arc::new(QuotaTracker::new()),
            circuit_breaker: Arc::new(CircuitBreaker::new(circuit_config)),
        }
    }

    pub fn quota_tracker(&self) -> Arc<QuotaTracker> {
        self.quota_tracker.clone()
    }

    pub fn circuit_breaker(&self) -> Arc<CircuitBreaker> {
        self.circuit_breaker.clone()
    }

    pub async fn get_model(&self) -> Option<String> {
        Some(self.config.model.clone())
    }

    fn build_request_body(&self, messages: &[Message], tools: &[ToolDefinition]) -> Value {
        let msgs: Vec<Value> = messages
            .iter()
            .map(|m| {
                let mut obj = json!({
                    "role": match m.role {
                        Role::System => "system",
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::Tool => "tool",
                    },
                    "content": m.content,
                });
                if let Some(ref tc_id) = m.tool_call_id {
                    obj["tool_call_id"] = json!(tc_id);
                }
                if let Some(ref tcs) = m.tool_calls {
                    obj["tool_calls"] = json!(tcs
                        .iter()
                        .map(|tc| json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments.to_string(),
                            }
                        }))
                        .collect::<Vec<_>>());
                }
                obj
            })
            .collect();

        let mut body = json!({
            "model": self.config.model,
            "messages": msgs,
            "max_tokens": self.config.max_tokens,
            "temperature": self.config.temperature,
        });

        if !tools.is_empty() {
            let tool_defs: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = json!(tool_defs);
        }

        body
    }
}

#[async_trait]
impl LlmProvider for OpenRouterProvider {
    async fn chat_completion(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse, ProviderError> {
        // Check quota before making request
        self.quota_tracker
            .update_usage(&self.config.model, 0)
            .map_err(|e| ProviderError::Api {
                status: 429,
                body: e.to_string(),
            })?;

        let body = self.build_request_body(messages, tools);

        // Use circuit breaker for resilience
        let result = self.circuit_breaker.call(async {
            let mut req = self
                .client
                .post(format!("{}/chat/completions", self.config.base_url))
                .bearer_auth(&self.config.api_key)
                .json(&body);

            for (k, v) in &self.config.extra_headers {
                req = req.header(k, v);
            }

            let resp = req.send().await.map_err(|e| ProviderError::Http(e))?;
            let status = resp.status().as_u16();

            if status == 429 {
                let retry_after = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1000);
                return Err(ProviderError::RateLimited {
                    retry_after_ms: retry_after * 1000,
                });
            }

            if status >= 400 {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                return Err(ProviderError::Api {
                    status,
                    body,
                });
            }

            let data: Value = resp.json().await.map_err(|e| ProviderError::Http(e))?;
            parse_openai_response(&data)
        })
        .await;

        match result {
            Ok(response) => {
                // Update quota with actual token usage
                if let Some(usage) = &response.usage {
                    let _ = self
                        .quota_tracker
                        .update_usage(&self.config.model, usage.total_tokens);
                }
                Ok(response)
            }
            Err(circuit_error) => {
                // Convert circuit breaker error to provider error
                match circuit_error {
                    crate::circuit_breaker::CircuitBreakerError::Open => {
                        Err(ProviderError::Api {
                            status: 503,
                            body: "Service temporarily unavailable - circuit breaker open"
                                .to_string(),
                        })
                    }
                    crate::circuit_breaker::CircuitBreakerError::Timeout => {
                        Err(ProviderError::Api {
                            status: 408,
                            body: "Request timeout".to_string(),
                        })
                    }
                }
            }
        }
    }

    async fn chat_completion_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), ProviderError> {
        let mut body = self.build_request_body(messages, tools);
        body["stream"] = json!(true);

        // Use circuit breaker for resilience
        let result = self.circuit_breaker.call(async {
            let mut req = self
                .client
                .post(format!("{}/chat/completions", self.config.base_url))
                .bearer_auth(&self.config.api_key)
                .json(&body);

            for (k, v) in &self.config.extra_headers {
                req = req.header(k, v);
            }

            let resp = req.send().await.map_err(|e| ProviderError::Http(e))?;
            if resp.status().as_u16() >= 400 {
                return Err(ProviderError::Api {
                    status: resp.status().as_u16(),
                    body: resp.text().await.unwrap_or_default(),
                });
            }

            let mut stream = resp.bytes_stream();
            use futures::StreamExt;
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| ProviderError::Stream(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim().to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if line.is_empty() || line == "data: [DONE]" {
                        if line == "data: [DONE]" {
                            let _ = tx.send(StreamChunk::Done(FinishReason::Stop)).await;
                        }
                        continue;
                    }

                    if line.starts_with("data: ") {
                        let data = line.strip_prefix("data: ").unwrap();
                        if let Ok(value) = serde_json::from_str::<Value>(data) {
                            if let Some(chunks) = parse_sse_chunk(&value) {
                                for chunk in chunks {
                                    let _ = tx.send(chunk).await;
                                }
                            }
                        }
                    }
                }
            }

            Ok(())
        }).await;

        match result {
            Ok(_) => Ok(()),
            Err(circuit_error) => {
                // Convert circuit breaker error to provider error
                match circuit_error {
                    crate::circuit_breaker::CircuitBreakerError::Open => {
                        Err(ProviderError::Api {
                            status: 503,
                            body: "Service temporarily unavailable - circuit breaker open"
                                .to_string(),
                        })
                    }
                    crate::circuit_breaker::CircuitBreakerError::Timeout => {
                        Err(ProviderError::Api {
                            status: 408,
                            body: "Request timeout".to_string(),
                        })
                    }
                }
            }
        }
    }

    fn quota_tracker(&self) -> Arc<QuotaTracker> {
        self.quota_tracker.clone()
    }

    fn circuit_breaker(&self) -> Arc<CircuitBreaker> {
        self.circuit_breaker.clone()
    }

    async fn get_model(&self) -> Option<String> {
        Some(self.config.model.clone())
    }
}

// ---------------------------------------------------------------------------
// HuggingFace Provider (TGI / Inference Endpoints — OpenAI-compatible)
// ---------------------------------------------------------------------------

pub struct HuggingFaceProvider {
    inner: OpenRouterProvider,
}

impl HuggingFaceProvider {
    pub fn new(config: ProviderConfig) -> Self {
        // HuggingFace Inference Endpoints expose an OpenAI-compatible API,
        // so we can reuse the same implementation.
        Self {
            inner: OpenRouterProvider::new(config),
        }
    }
}

#[async_trait]
impl LlmProvider for HuggingFaceProvider {
    async fn chat_completion(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse, ProviderError> {
        self.inner.chat_completion(messages, tools).await
    }

    async fn chat_completion_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), ProviderError> {
        self.inner.chat_completion_stream(messages, tools, tx).await
    }

    fn quota_tracker(&self) -> Arc<QuotaTracker> {
        self.inner.quota_tracker()
    }

    fn circuit_breaker(&self) -> Arc<CircuitBreaker> {
        self.inner.circuit_breaker()
    }

    async fn get_model(&self) -> Option<String> {
        self.inner.get_model().await
    }
}

// ---------------------------------------------------------------------------
// Helper: parse OpenAI-compatible responses
// ---------------------------------------------------------------------------

fn parse_openai_response(data: &Value) -> Result<LlmResponse, ProviderError> {
    let choice = data["choices"]
        .get(0)
        .ok_or_else(|| ProviderError::Parse("No choices in response".into()))?;

    let msg = &choice["message"];
    let content = msg["content"].as_str().unwrap_or("").to_string();

    let finish_reason = match choice["finish_reason"].as_str() {
        Some("stop") => FinishReason::Stop,
        Some("tool_calls") => FinishReason::ToolUse,
        Some("length") => FinishReason::Length,
        Some(other) => FinishReason::Other(other.to_string()),
        None => FinishReason::Stop,
    };

    let tool_calls = if let Some(tcs) = msg["tool_calls"].as_array() {
        tcs.iter()
            .filter_map(|tc| {
                Some(ToolCall {
                    id: tc["id"].as_str()?.to_string(),
                    name: tc["function"]["name"].as_str()?.to_string(),
                    arguments: serde_json::from_str(tc["function"]["arguments"].as_str()?)
                        .unwrap_or(Value::Null),
                })
            })
            .collect()
    } else {
        vec![]
    };

    let usage = data.get("usage").map(|u| Usage {
        prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
        completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
        total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
    });

    Ok(LlmResponse {
        content,
        tool_calls,
        finish_reason,
        usage,
    })
}

fn parse_sse_chunk(data: &Value) -> Option<Vec<StreamChunk>> {
    let choice = data["choices"].get(0)?;
    let delta = &choice["delta"];
    let mut chunks = vec![];

    if let Some(text) = delta["content"].as_str() {
        if !text.is_empty() {
            chunks.push(StreamChunk::TextDelta(text.to_string()));
        }
    }

    if let Some(tcs) = delta["tool_calls"].as_array() {
        for tc in tcs {
            let index = tc["index"].as_u64().unwrap_or(0) as usize;
            chunks.push(StreamChunk::ToolCallDelta {
                index,
                id: tc["id"].as_str().map(String::from),
                name: tc["function"]["name"].as_str().map(String::from),
                arguments_delta: tc["function"]["arguments"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }

    if let Some(reason) = choice["finish_reason"].as_str() {
        chunks.push(StreamChunk::Done(match reason {
            "stop" => FinishReason::Stop,
            "tool_calls" => FinishReason::ToolUse,
            "length" => FinishReason::Length,
            other => FinishReason::Other(other.to_string()),
        }));
    }

    if chunks.is_empty() {
        None
    } else {
        Some(chunks)
    }
}

/// Factory function to create providers based on kind
pub fn create_provider(kind: ProviderKind, config: ProviderConfig) -> Box<dyn LlmProvider> {
    match kind {
        ProviderKind::OpenRouter => Box::new(OpenRouterProvider::new(config)),
        ProviderKind::HuggingFace => Box::new(HuggingFaceProvider::new(config)),
    }
}
