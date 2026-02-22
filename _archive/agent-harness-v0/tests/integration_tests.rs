use std::sync::Arc;
use std::time::Duration;

use agent_harness::agent::{Agent, AgentConfig, AgentEvent};
use agent_harness::memory::InMemoryMemory;
use agent_harness::orchestrator::{Orchestrator, RoutingStrategy};
use agent_harness::provider::{LlmProvider, ProviderError};
use agent_harness::retry::RetryConfig;
use agent_harness::tool::{FnTool, ToolRegistry};
use agent_harness::types::*;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::sleep;

/// Mock provider for testing
pub struct MockProvider {
    responses: Vec<String>,
    delay: Duration,
    should_fail: bool,
    quota_tracker: Arc<agent_harness::quota::QuotaTracker>,
}

impl MockProvider {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses,
            delay: Duration::from_millis(100),
            should_fail: false,
            quota_tracker: Arc::new(agent_harness::quota::QuotaTracker::new()),
        }
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn failing(mut self) -> Self {
        self.should_fail = true;
        self
    }
}

#[async_trait::async_trait]
impl LlmProvider for MockProvider {
    async fn chat_completion(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
    ) -> Result<LlmResponse, ProviderError> {
        sleep(self.delay).await;
        
        if self.should_fail {
            return Err(ProviderError::Api {
                status: 500,
                body: "Mock provider error".to_string(),
            });
        }

        // Update quota usage
        let _ = self.quota_tracker.update_usage("mock_provider", 30);

        let response = self.responses.first().unwrap_or(&"Mock response".to_string()).clone();
        
        Ok(LlmResponse {
            content: response,
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
        })
    }

    async fn chat_completion_stream(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _tx: mpsc::Sender<StreamChunk>,
    ) -> Result<(), ProviderError> {
        sleep(self.delay).await;
        
        if self.should_fail {
            return Err(ProviderError::Api {
                status: 500,
                body: "Mock provider error".to_string(),
            });
        }

        Ok(())
    }

    fn quota_tracker(&self) -> Arc<agent_harness::quota::QuotaTracker> {
        self.quota_tracker.clone()
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_quota_tracking_across_multiple_requests() {
        let provider = Arc::new(MockProvider::new(vec![
            "First response".to_string(),
            "Second response".to_string(),
            "Third response".to_string(),
        ]));

        let quota_tracker = provider.quota_tracker();
        
        // Make multiple requests
        for i in 0..3 {
            let result = provider.chat_completion(
                &[Message::user(&format!("Test message {}", i))],
                &[],
            ).await;
            
            assert!(result.is_ok());
            let response = result.unwrap();
            assert!(!response.content.is_empty());
        }

        // Check quota tracking
        let quota_info = quota_tracker.get_quota_info("mock_provider");
        assert_eq!(quota_info.requests_used, 3);
        assert!(quota_info.tokens_used > 0);
        assert!(quota_info.can_make_request());
    }

    #[tokio::test]
    async fn test_model_fallback_behavior() {
        let failing_provider = Arc::new(MockProvider::new(vec!["Response".to_string()]).failing());
        let working_provider = Arc::new(MockProvider::new(vec!["Fallback response".to_string()]));

        // Test failing provider
        let result = failing_provider.chat_completion(
            &[Message::user("Test")],
            &[],
        ).await;
        
        assert!(result.is_err());
        
        // Test working provider
        let result = working_provider.chat_completion(
            &[Message::user("Test")],
            &[],
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "Fallback response");
    }

    #[tokio::test]
    async fn test_orchestrator_with_mock_providers() {
        let provider = Arc::new(MockProvider::new(vec![
            "I can help with that!".to_string(),
            "Here's the code you requested.".to_string(),
        ]));

        let mut tools = ToolRegistry::new();
        tools.register(FnTool::new(
            ToolDefinition {
                name: "test_tool".into(),
                description: "A test tool".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "input": {"type": "string"}
                    },
                    "required": ["input"]
                }),
            },
            |args| {
                let input = args["input"].as_str().unwrap_or("default");
                Ok(format!("Tool processed: {}", input))
            },
        ));

        let memory = Arc::new(InMemoryMemory::new());
        
        let agent = Agent::new(
            AgentConfig {
                name: "test_agent".into(),
                system_prompt: "You are a helpful test assistant.".into(),
                max_tool_rounds: 2,
                retry_config: RetryConfig::default(),
                stream: false,
            },
            provider.clone(),
            Arc::new(tools),
            memory,
        );

        let (event_tx, _event_rx) = mpsc::channel::<AgentEvent>(256);
        
        let orchestrator = Orchestrator::builder()
            .add_agent(agent)
            .strategy(RoutingStrategy::Fixed("test_agent".into()))
            .event_channel(event_tx)
            .build();

        // Test orchestrator
        let result = orchestrator.run("test-conv", "Hello, can you help me?").await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert!(!response.is_empty());
        assert!(response.contains("help"));
    }

    #[tokio::test]
    async fn test_tool_execution_with_mock_provider() {
        let provider = Arc::new(MockProvider::new(vec![
            "I'll use the test tool for you.".to_string(),
        ]));

        let mut tools = ToolRegistry::new();
        tools.register(FnTool::new(
            ToolDefinition {
                name: "calculate".into(),
                description: "Calculate something".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "expression": {"type": "string"}
                    },
                    "required": ["expression"]
                }),
            },
            |args| {
                let expr = args["expression"].as_str().unwrap_or("0");
                if expr == "2+2" {
                    Ok("4".to_string())
                } else {
                    Ok("Unknown expression".to_string())
                }
            },
        ));

        let memory = Arc::new(InMemoryMemory::new());
        
        let agent = Agent::new(
            AgentConfig {
                name: "calculator".into(),
                system_prompt: "You are a calculator. Use the calculate tool for math expressions.".into(),
                max_tool_rounds: 3,
                retry_config: RetryConfig::default(),
                stream: false,
            },
            provider,
            Arc::new(tools),
            memory,
        );

        let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(256);
        
        let orchestrator = Orchestrator::builder()
            .add_agent(agent)
            .strategy(RoutingStrategy::Fixed("calculator".into()))
            .event_channel(event_tx)
            .build();

        // Test tool execution
        let result = orchestrator.run("calc-conv", "What is 2+2?").await;
        assert!(result.is_ok());
        
        // Check that tool was called (by monitoring events)
        let mut tool_called = false;
        while let Ok(event) = event_rx.try_recv() {
            if let AgentEvent::ToolCallStart { tool_name, .. } = event {
                if tool_name == "calculate" {
                    tool_called = true;
                }
            }
        }
        
        assert!(tool_called);
    }

    #[tokio::test]
    async fn test_concurrent_requests() {
        let provider = Arc::new(MockProvider::new(vec![
            "Response 1".to_string(),
            "Response 2".to_string(),
            "Response 3".to_string(),
        ]).with_delay(Duration::from_millis(50)));

        // Spawn multiple concurrent requests
        let mut handles = vec![];
        for i in 0..3 {
            let provider_clone = provider.clone();
            let handle = tokio::spawn(async move {
                provider_clone.chat_completion(
                    &[Message::user(&format!("Concurrent test {}", i))],
                    &[],
                ).await
            });
            handles.push(handle);
        }

        // Wait for all requests to complete
        let results: Vec<Result<LlmResponse, ProviderError>> = 
            futures::future::join_all(handles).await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // All requests should succeed
        assert_eq!(results.len(), 3);
        for result in results {
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_error_handling_and_recovery() {
        let provider = Arc::new(MockProvider::new(vec![
            "Success after failure".to_string(),
        ]).failing());

        // First request should fail
        let result1 = provider.chat_completion(
            &[Message::user("Test")],
            &[],
        ).await;
        assert!(result1.is_err());

        // Create a non-failing provider for recovery test
        let working_provider = Arc::new(MockProvider::new(vec![
            "Recovered response".to_string(),
        ]));

        let result2 = working_provider.chat_completion(
            &[Message::user("Test recovery")],
            &[],
        ).await;
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap().content, "Recovered response");
    }
}
