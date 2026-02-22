use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::memory::Memory;
use crate::provider::{LlmProvider, ProviderError};
use crate::retry::{with_retry, RetryConfig};
use crate::stream::StreamAccumulator;
use crate::tool::ToolRegistry;
use crate::types::*;

/// Configuration for a single agent.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Unique name for this agent.
    pub name: String,
    /// System prompt defining the agent's role and behavior.
    pub system_prompt: String,
    /// Maximum number of tool-use loops before forcing a stop.
    pub max_tool_rounds: u32,
    /// Retry configuration for LLM calls.
    pub retry_config: RetryConfig,
    /// Whether to use streaming.
    pub stream: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "agent".into(),
            system_prompt: String::new(),
            max_tool_rounds: 10,
            retry_config: RetryConfig::default(),
            stream: false,
        }
    }
}

/// Events emitted by the agent during execution.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// The agent started thinking.
    Thinking { agent: String },
    /// Streaming text delta.
    TextDelta { agent: String, text: String },
    /// The agent is calling a tool.
    ToolCallStart {
        agent: String,
        tool_name: String,
        tool_call_id: String,
    },
    /// A tool returned a result.
    ToolCallResult {
        agent: String,
        tool_name: String,
        result: String,
    },
    /// The agent produced a final response.
    Response { agent: String, content: String },
    /// An error occurred.
    Error { agent: String, error: String },
}

/// A single autonomous agent with tool-calling capabilities.
pub struct Agent {
    pub config: AgentConfig,
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    memory: Arc<dyn Memory>,
}

impl Agent {
    pub fn new(
        config: AgentConfig,
        provider: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn Memory>,
    ) -> Self {
        Self {
            config,
            provider,
            tools,
            memory,
        }
    }

    /// Run the agent with a user message, returning the final response.
    /// Sends events to the optional channel.
    pub async fn run(
        &self,
        conversation_id: &str,
        user_input: &str,
        event_tx: Option<&mpsc::Sender<AgentEvent>>,
    ) -> Result<String, AgentError> {
        let agent_name = &self.config.name;
        info!(
            "[{}] Starting run with input: {}",
            agent_name,
            &user_input[..user_input.len().min(100)]
        );

        // Build initial messages
        let mut messages = self.memory.history(conversation_id).await;
        if messages.is_empty() && !self.config.system_prompt.is_empty() {
            messages.push(Message::system(&self.config.system_prompt));
        }
        messages.push(Message::user(user_input));

        let tool_defs = self.tools.definitions();

        // Agentic loop
        for round in 0..self.config.max_tool_rounds {
            debug!("[{}] Tool round {}", agent_name, round);

            if let Some(tx) = event_tx {
                let _ = tx
                    .send(AgentEvent::Thinking {
                        agent: agent_name.clone(),
                    })
                    .await;
            }

            // Call LLM (with retry)
            let response = self.call_llm(&messages, &tool_defs, event_tx).await?;

            // If no tool calls, we're done
            if response.tool_calls.is_empty() {
                info!("[{}] Final response (round {})", agent_name, round);

                // Save to memory
                let new_messages = vec![
                    Message::user(user_input),
                    Message::assistant(&response.content),
                ];
                self.memory.append(conversation_id, &new_messages).await;

                if let Some(tx) = event_tx {
                    let _ = tx
                        .send(AgentEvent::Response {
                            agent: agent_name.clone(),
                            content: response.content.clone(),
                        })
                        .await;
                }

                return Ok(response.content);
            }

            // Add assistant message with tool calls
            let mut assistant_msg = Message::assistant(&response.content);
            assistant_msg.tool_calls = Some(response.tool_calls.clone());
            messages.push(assistant_msg);

            // Execute tool calls
            for tc in &response.tool_calls {
                if let Some(tx) = event_tx {
                    let _ = tx
                        .send(AgentEvent::ToolCallStart {
                            agent: agent_name.clone(),
                            tool_name: tc.name.clone(),
                            tool_call_id: tc.id.clone(),
                        })
                        .await;
                }

                let result = match self.tools.execute(tc).await {
                    Ok(r) => r,
                    Err(e) => format!("Error: {}", e),
                };

                if let Some(tx) = event_tx {
                    let _ = tx
                        .send(AgentEvent::ToolCallResult {
                            agent: agent_name.clone(),
                            tool_name: tc.name.clone(),
                            result: result.clone(),
                        })
                        .await;
                }

                messages.push(Message::tool_result(&tc.id, &result));
            }
        }

        Err(AgentError::MaxToolRoundsExceeded(
            self.config.max_tool_rounds,
        ))
    }

    async fn call_llm(
        &self,
        messages: &[Message],
        tool_defs: &[ToolDefinition],
        event_tx: Option<&mpsc::Sender<AgentEvent>>,
    ) -> Result<LlmResponse, AgentError> {
        let provider = self.provider.clone();
        let msgs = messages.to_vec();
        let tools = tool_defs.to_vec();
        let agent_name = self.config.name.clone();

        if self.config.stream {
            // Streaming path
            let (tx, mut rx) = mpsc::channel(256);
            let p = provider.clone();
            let m = msgs.clone();
            let t = tools.clone();

            tokio::spawn(async move {
                if let Err(e) = p.chat_completion_stream(&m, &t, tx).await {
                    error!("Stream error: {}", e);
                }
            });

            let mut acc = StreamAccumulator::new();
            while let Some(chunk) = rx.recv().await {
                if let StreamChunk::TextDelta(ref text) = chunk {
                    if let Some(etx) = event_tx {
                        let _ = etx
                            .send(AgentEvent::TextDelta {
                                agent: agent_name.clone(),
                                text: text.clone(),
                            })
                            .await;
                    }
                }
                acc.feed(chunk);
            }

            Ok(LlmResponse {
                content: acc.content.clone(),
                tool_calls: acc.tool_calls(),
                finish_reason: acc.finish_reason.unwrap_or(FinishReason::Stop),
                usage: None,
            })
        } else {
            // Non-streaming with retry
            let retry_cfg = self.config.retry_config.clone();
            with_retry(&retry_cfg, &format!("llm_call[{}]", agent_name), || {
                let p = provider.clone();
                let m = msgs.clone();
                let t = tools.clone();
                async move { p.chat_completion(&m, &t).await }
            })
            .await
            .map_err(AgentError::Provider)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error("Max tool rounds exceeded ({0})")]
    MaxToolRoundsExceeded(u32),
    #[error("Agent error: {0}")]
    Other(String),
}
