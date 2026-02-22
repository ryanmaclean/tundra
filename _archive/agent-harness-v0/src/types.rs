use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Role of a message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// If this is a tool result, the tool_call_id it responds to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// If this is an assistant message with tool calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Tool definition sent to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

/// Response from an LLM provider.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FinishReason {
    Stop,
    ToolUse,
    Length,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A streaming chunk from the LLM.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// A piece of text content.
    TextDelta(String),
    /// A tool call being built up.
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    /// Stream finished.
    Done(FinishReason),
    /// Usage info (may come at end).
    UsageInfo(Usage),
}

/// Configuration for an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_max_tokens() -> u32 {
    4096
}
fn default_temperature() -> f32 {
    0.7
}

/// Identifies which provider backend to use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenRouter,
    HuggingFace,
}

/// Quota information for API usage tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaInfo {
    pub requests_used: u32,
    pub requests_limit: u32,
    pub tokens_used: u32,
    pub tokens_limit: Option<u32>,
    pub model_type: ModelType,
    pub is_free_tier: bool,
    pub reset_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelType {
    Free,
    Paid,
    Freemium,
}

impl QuotaInfo {
    pub fn usage_percentage(&self) -> f32 {
        if self.requests_limit == 0 {
            0.0
        } else {
            (self.requests_used as f32 / self.requests_limit as f32) * 100.0
        }
    }

    pub fn tokens_usage_percentage(&self) -> Option<f32> {
        self.tokens_limit.map(|limit| {
            if limit == 0 {
                0.0
            } else {
                (self.tokens_used as f32 / limit as f32) * 100.0
            }
        })
    }

    pub fn is_near_limit(&self, threshold: f32) -> bool {
        self.usage_percentage() >= threshold
    }

    pub fn can_make_request(&self) -> bool {
        self.requests_used < self.requests_limit
    }
}
