//! Exhaustive tests for the LLM provider abstraction layer:
//! LlmMessage types, LlmConfig, LlmResponse, LlmError variants,
//! MockProvider, AnthropicProvider (structure), OpenAiProvider (structure),
//! and feature model configuration.

use at_intelligence::llm::{
    AnthropicProvider, LlmConfig, LlmError, LlmMessage, LlmProvider, LlmResponse, LlmRole,
    LlmUsageTracker, MockProvider, OpenAiProvider,
};

// ===========================================================================
// LlmMessage Types
// ===========================================================================

#[test]
fn test_llm_message_system() {
    let msg = LlmMessage::system("You are a helpful assistant.");
    assert_eq!(msg.role, LlmRole::System);
    assert_eq!(msg.content, "You are a helpful assistant.");
}

#[test]
fn test_llm_message_user() {
    let msg = LlmMessage::user("What is Rust?");
    assert_eq!(msg.role, LlmRole::User);
    assert_eq!(msg.content, "What is Rust?");
}

#[test]
fn test_llm_message_assistant() {
    let msg = LlmMessage::assistant("Rust is a systems programming language.");
    assert_eq!(msg.role, LlmRole::Assistant);
    assert_eq!(msg.content, "Rust is a systems programming language.");
}

#[test]
fn test_llm_role_display() {
    assert_eq!(LlmRole::System.to_string(), "system");
    assert_eq!(LlmRole::User.to_string(), "user");
    assert_eq!(LlmRole::Assistant.to_string(), "assistant");
}

#[test]
fn test_llm_role_serialization() {
    assert_eq!(serde_json::to_string(&LlmRole::System).unwrap(), "\"system\"");
    assert_eq!(serde_json::to_string(&LlmRole::User).unwrap(), "\"user\"");
    assert_eq!(
        serde_json::to_string(&LlmRole::Assistant).unwrap(),
        "\"assistant\""
    );

    // Deserialize back
    let role: LlmRole = serde_json::from_str("\"system\"").unwrap();
    assert_eq!(role, LlmRole::System);
    let role: LlmRole = serde_json::from_str("\"user\"").unwrap();
    assert_eq!(role, LlmRole::User);
    let role: LlmRole = serde_json::from_str("\"assistant\"").unwrap();
    assert_eq!(role, LlmRole::Assistant);
}

#[test]
fn test_llm_message_serialization_roundtrip() {
    let messages = vec![
        LlmMessage::system("Be concise"),
        LlmMessage::user("Hello"),
        LlmMessage::assistant("Hi there!"),
    ];
    for msg in &messages {
        let json = serde_json::to_string(msg).unwrap();
        let deserialized: LlmMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, msg.role);
        assert_eq!(deserialized.content, msg.content);
    }
}

#[test]
fn test_llm_message_new_constructor() {
    let msg = LlmMessage::new(LlmRole::User, "test content");
    assert_eq!(msg.role, LlmRole::User);
    assert_eq!(msg.content, "test content");
}

#[test]
fn test_llm_message_with_empty_content() {
    let msg = LlmMessage::user("");
    assert_eq!(msg.content, "");
    assert_eq!(msg.role, LlmRole::User);
}

// ===========================================================================
// LlmConfig
// ===========================================================================

#[test]
fn test_llm_config_defaults() {
    let config = LlmConfig::default();
    assert!(!config.model.is_empty());
    assert!(config.model.contains("claude"));
    assert_eq!(config.max_tokens, 1024);
    assert!((config.temperature - 0.7).abs() < f32::EPSILON);
    assert!(config.system_prompt.is_none());
}

#[test]
fn test_llm_config_custom_model() {
    let config = LlmConfig {
        model: "gpt-4-turbo".to_string(),
        max_tokens: 4096,
        temperature: 0.3,
        system_prompt: Some("You are a code reviewer.".to_string()),
    };
    assert_eq!(config.model, "gpt-4-turbo");
    assert_eq!(config.max_tokens, 4096);
    assert!((config.temperature - 0.3).abs() < f32::EPSILON);
    assert_eq!(
        config.system_prompt.as_deref(),
        Some("You are a code reviewer.")
    );
}

#[test]
fn test_llm_config_temperature_range() {
    // Temperature 0.0 (deterministic)
    let config = LlmConfig {
        temperature: 0.0,
        ..LlmConfig::default()
    };
    assert!((config.temperature - 0.0).abs() < f32::EPSILON);

    // Temperature 1.0 (maximum creativity)
    let config = LlmConfig {
        temperature: 1.0,
        ..LlmConfig::default()
    };
    assert!((config.temperature - 1.0).abs() < f32::EPSILON);

    // Temperature 2.0 (some providers allow > 1)
    let config = LlmConfig {
        temperature: 2.0,
        ..LlmConfig::default()
    };
    assert!((config.temperature - 2.0).abs() < f32::EPSILON);
}

#[test]
fn test_llm_config_max_tokens() {
    let config = LlmConfig {
        max_tokens: 100_000,
        ..LlmConfig::default()
    };
    assert_eq!(config.max_tokens, 100_000);

    let config = LlmConfig {
        max_tokens: 1,
        ..LlmConfig::default()
    };
    assert_eq!(config.max_tokens, 1);
}

#[test]
fn test_llm_config_serialization_roundtrip() {
    let config = LlmConfig {
        model: "claude-opus-4-20250514".to_string(),
        max_tokens: 8192,
        temperature: 0.5,
        system_prompt: Some("Analyze code quality.".to_string()),
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: LlmConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.model, "claude-opus-4-20250514");
    assert_eq!(deserialized.max_tokens, 8192);
    assert!((deserialized.temperature - 0.5).abs() < f32::EPSILON);
    assert_eq!(
        deserialized.system_prompt.as_deref(),
        Some("Analyze code quality.")
    );
}

#[test]
fn test_llm_config_without_system_prompt() {
    let config = LlmConfig {
        model: "test".to_string(),
        max_tokens: 512,
        temperature: 0.7,
        system_prompt: None,
    };
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: LlmConfig = serde_json::from_str(&json).unwrap();
    assert!(deserialized.system_prompt.is_none());
}

// ===========================================================================
// LlmResponse
// ===========================================================================

#[test]
fn test_llm_response_fields() {
    let resp = LlmResponse {
        content: "Hello, world!".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
        input_tokens: 150,
        output_tokens: 42,
        finish_reason: "end_turn".to_string(),
    };
    assert_eq!(resp.content, "Hello, world!");
    assert_eq!(resp.model, "claude-sonnet-4-20250514");
    assert_eq!(resp.input_tokens, 150);
    assert_eq!(resp.output_tokens, 42);
    assert_eq!(resp.finish_reason, "end_turn");
}

#[test]
fn test_llm_response_serialization() {
    let resp = LlmResponse {
        content: "Answer".to_string(),
        model: "gpt-4".to_string(),
        input_tokens: 100,
        output_tokens: 50,
        finish_reason: "stop".to_string(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: LlmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.content, "Answer");
    assert_eq!(deserialized.model, "gpt-4");
    assert_eq!(deserialized.input_tokens, 100);
    assert_eq!(deserialized.output_tokens, 50);
    assert_eq!(deserialized.finish_reason, "stop");
}

#[test]
fn test_llm_response_with_zero_tokens() {
    let resp = LlmResponse {
        content: String::new(),
        model: "test".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        finish_reason: "length".to_string(),
    };
    assert_eq!(resp.input_tokens, 0);
    assert_eq!(resp.output_tokens, 0);
    assert!(resp.content.is_empty());
}

// ===========================================================================
// LlmError Variants
// ===========================================================================

#[test]
fn test_llm_error_http() {
    let err = LlmError::HttpError("connection refused".to_string());
    let display = err.to_string();
    assert!(display.contains("HTTP error"));
    assert!(display.contains("connection refused"));
}

#[test]
fn test_llm_error_api_with_status() {
    let err = LlmError::ApiError {
        status: 400,
        message: "bad request body".to_string(),
    };
    let display = err.to_string();
    assert!(display.contains("400"));
    assert!(display.contains("bad request body"));

    // Test other status codes
    let err = LlmError::ApiError {
        status: 500,
        message: "internal server error".to_string(),
    };
    assert!(err.to_string().contains("500"));

    let err = LlmError::ApiError {
        status: 401,
        message: "unauthorized".to_string(),
    };
    assert!(err.to_string().contains("401"));
}

#[test]
fn test_llm_error_parse() {
    let err = LlmError::ParseError("invalid JSON at line 5".to_string());
    let display = err.to_string();
    assert!(display.contains("parse error"));
    assert!(display.contains("invalid JSON at line 5"));
}

#[test]
fn test_llm_error_rate_limited() {
    let err = LlmError::RateLimited {
        retry_after_secs: Some(30),
    };
    let display = err.to_string();
    assert!(display.contains("rate limited"));
    assert!(display.contains("30"));

    // Without retry_after
    let err = LlmError::RateLimited {
        retry_after_secs: None,
    };
    let display = err.to_string();
    assert!(display.contains("rate limited"));
}

#[test]
fn test_llm_error_timeout() {
    let err = LlmError::Timeout;
    let display = err.to_string();
    assert!(display.contains("timed out"));
}

#[test]
fn test_llm_error_unsupported() {
    let err = LlmError::Unsupported("streaming not available".to_string());
    let display = err.to_string();
    assert!(display.contains("unsupported"));
    assert!(display.contains("streaming not available"));
}

#[test]
fn test_llm_error_from_reqwest() {
    // We can test the From<reqwest::Error> conversion indirectly.
    // A timeout reqwest error should map to LlmError::Timeout,
    // and other errors should map to LlmError::HttpError.
    // Since we cannot easily construct a reqwest::Error in tests,
    // we verify the error variants are distinct and pattern-matchable.
    let http_err = LlmError::HttpError("some reqwest error".to_string());
    assert!(matches!(http_err, LlmError::HttpError(_)));

    let timeout_err = LlmError::Timeout;
    assert!(matches!(timeout_err, LlmError::Timeout));
}

#[test]
fn test_llm_error_debug_format() {
    // Verify all error variants implement Debug
    let errors: Vec<LlmError> = vec![
        LlmError::HttpError("test".into()),
        LlmError::ApiError {
            status: 403,
            message: "forbidden".into(),
        },
        LlmError::ParseError("bad json".into()),
        LlmError::RateLimited {
            retry_after_secs: Some(60),
        },
        LlmError::Timeout,
        LlmError::Unsupported("feature X".into()),
    ];
    for err in &errors {
        let debug = format!("{:?}", err);
        assert!(!debug.is_empty());
    }
}

// ===========================================================================
// MockProvider
// ===========================================================================

#[tokio::test]
async fn test_mock_provider_returns_queued_response() {
    let custom_response = LlmResponse {
        content: "Custom answer from mock".to_string(),
        model: "custom-model".to_string(),
        input_tokens: 42,
        output_tokens: 99,
        finish_reason: "stop".to_string(),
    };
    let provider = MockProvider::new().with_response(custom_response);
    let config = LlmConfig::default();

    let resp = provider
        .complete(&[LlmMessage::user("Hi")], &config)
        .await
        .unwrap();
    assert_eq!(resp.content, "Custom answer from mock");
    assert_eq!(resp.model, "custom-model");
    assert_eq!(resp.input_tokens, 42);
    assert_eq!(resp.output_tokens, 99);
}

#[tokio::test]
async fn test_mock_provider_captures_requests() {
    let provider = MockProvider::new();
    let config = LlmConfig {
        model: "test-model".to_string(),
        max_tokens: 512,
        temperature: 0.5,
        system_prompt: None,
    };
    let messages = vec![
        LlmMessage::system("Be helpful"),
        LlmMessage::user("What is 2+2?"),
    ];

    provider.complete(&messages, &config).await.unwrap();

    let captured = provider.captured_requests();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].0.len(), 2);
    assert_eq!(captured[0].0[0].role, LlmRole::System);
    assert_eq!(captured[0].0[0].content, "Be helpful");
    assert_eq!(captured[0].0[1].role, LlmRole::User);
    assert_eq!(captured[0].0[1].content, "What is 2+2?");
    assert_eq!(captured[0].1.model, "test-model");
}

#[tokio::test]
async fn test_mock_provider_error_queueing() {
    let provider = MockProvider::new().with_error(LlmError::Timeout);
    let config = LlmConfig::default();

    let result = provider
        .complete(&[LlmMessage::user("Hi")], &config)
        .await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), LlmError::Timeout));
}

#[tokio::test]
async fn test_mock_provider_multiple_calls() {
    let resp1 = LlmResponse {
        content: "First response".to_string(),
        model: "model-1".to_string(),
        input_tokens: 10,
        output_tokens: 5,
        finish_reason: "end_turn".to_string(),
    };
    let resp2 = LlmResponse {
        content: "Second response".to_string(),
        model: "model-2".to_string(),
        input_tokens: 20,
        output_tokens: 15,
        finish_reason: "stop".to_string(),
    };
    let provider = MockProvider::new()
        .with_response(resp1)
        .with_response(resp2);
    let config = LlmConfig::default();

    // First call gets first queued response
    let r1 = provider
        .complete(&[LlmMessage::user("1")], &config)
        .await
        .unwrap();
    assert_eq!(r1.content, "First response");

    // Second call gets second queued response
    let r2 = provider
        .complete(&[LlmMessage::user("2")], &config)
        .await
        .unwrap();
    assert_eq!(r2.content, "Second response");

    // Third call falls back to default
    let r3 = provider
        .complete(&[LlmMessage::user("3")], &config)
        .await
        .unwrap();
    assert_eq!(r3.content, "Mock response");

    // All three captured
    assert_eq!(provider.captured_requests().len(), 3);
}

#[tokio::test]
async fn test_mock_provider_default_response() {
    let provider = MockProvider::new();
    let config = LlmConfig {
        model: "my-model".to_string(),
        max_tokens: 256,
        temperature: 0.5,
        system_prompt: None,
    };

    let resp = provider
        .complete(&[LlmMessage::user("Hello")], &config)
        .await
        .unwrap();
    assert_eq!(resp.content, "Mock response");
    assert_eq!(resp.model, "my-model");
    assert_eq!(resp.input_tokens, 10);
    assert_eq!(resp.output_tokens, 5);
    assert_eq!(resp.finish_reason, "end_turn");
}

#[tokio::test]
async fn test_mock_provider_stream_unsupported() {
    let provider = MockProvider::new();
    let config = LlmConfig::default();

    let result = provider
        .stream(&[LlmMessage::user("Hi")], &config)
        .await;
    assert!(result.is_err());
    match result {
        Err(LlmError::Unsupported(msg)) => {
            assert!(msg.contains("MockProvider"));
        }
        _ => panic!("Expected LlmError::Unsupported"),
    }
}

#[tokio::test]
async fn test_mock_provider_as_trait_object() {
    let provider: Box<dyn LlmProvider> = Box::new(MockProvider::new());
    let config = LlmConfig::default();
    let resp = provider
        .complete(&[LlmMessage::user("test")], &config)
        .await
        .unwrap();
    assert_eq!(resp.content, "Mock response");
}

#[tokio::test]
async fn test_mock_provider_mixed_responses_and_errors() {
    let resp = LlmResponse {
        content: "OK".to_string(),
        model: "m".to_string(),
        input_tokens: 1,
        output_tokens: 1,
        finish_reason: "stop".to_string(),
    };
    let provider = MockProvider::new()
        .with_response(resp)
        .with_error(LlmError::RateLimited {
            retry_after_secs: Some(5),
        });
    let config = LlmConfig::default();

    // First: success
    let r1 = provider
        .complete(&[LlmMessage::user("a")], &config)
        .await;
    assert!(r1.is_ok());

    // Second: error
    let r2 = provider
        .complete(&[LlmMessage::user("b")], &config)
        .await;
    assert!(r2.is_err());
    assert!(matches!(r2.unwrap_err(), LlmError::RateLimited { .. }));

    // Third: default
    let r3 = provider
        .complete(&[LlmMessage::user("c")], &config)
        .await;
    assert!(r3.is_ok());
}

// ===========================================================================
// AnthropicProvider (structure tests, no real API)
// ===========================================================================

#[test]
fn test_anthropic_provider_creation() {
    let provider = AnthropicProvider::new("sk-test-key-123");
    // Verify it compiles and can be used as a trait object
    let _: &dyn LlmProvider = &provider;
}

#[test]
fn test_anthropic_provider_custom_base_url() {
    let provider = AnthropicProvider::new("sk-test")
        .with_base_url("http://localhost:8080");
    // Verify it accepts a custom base URL (for mock server testing)
    let _: &dyn LlmProvider = &provider;
}

#[test]
fn test_anthropic_provider_debug_format() {
    // Verify the request body builder works correctly
    let messages = vec![LlmMessage::user("Hello")];
    let config = LlmConfig {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 1024,
        temperature: 0.7,
        system_prompt: None,
    };

    let body = AnthropicProvider::build_request_body(&messages, &config);
    assert_eq!(body["model"], "claude-sonnet-4-20250514");
    assert_eq!(body["max_tokens"], 1024);

    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[0]["content"], "Hello");

    // No system field when no system prompt
    assert!(body.get("system").is_none());
}

#[test]
fn test_anthropic_provider_system_extraction() {
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

    // System messages extracted to top-level system field
    assert_eq!(body["system"], "Be concise");
    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3); // user, assistant, user (no system inline)
    for msg in msgs {
        assert_ne!(msg["role"], "system");
    }
}

#[test]
fn test_anthropic_provider_config_system_plus_message_system() {
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
    let system = body["system"].as_str().unwrap();
    assert!(system.contains("Base system prompt"));
    assert!(system.contains("Additional instruction"));
}

#[tokio::test]
async fn test_anthropic_provider_stream_unsupported() {
    let provider = AnthropicProvider::new("sk-test")
        .with_base_url("http://localhost:1"); // won't connect
    let config = LlmConfig::default();

    let result = provider
        .stream(&[LlmMessage::user("Hi")], &config)
        .await;
    assert!(result.is_err());
    match result {
        Err(LlmError::Unsupported(msg)) => {
            assert!(msg.contains("AnthropicProvider"));
        }
        _ => panic!("Expected LlmError::Unsupported for streaming"),
    }
}

// ===========================================================================
// OpenAiProvider (structure tests)
// ===========================================================================

#[test]
fn test_openai_provider_creation() {
    let provider = OpenAiProvider::new("sk-openai-test-key");
    let _: &dyn LlmProvider = &provider;
}

#[test]
fn test_openai_provider_custom_base_url() {
    let provider = OpenAiProvider::new("sk-test")
        .with_base_url("https://my-azure-openai.openai.azure.com");
    let _: &dyn LlmProvider = &provider;
}

#[test]
fn test_openai_provider_debug_format() {
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

    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[0]["content"], "What is Rust?");
}

#[test]
fn test_openai_provider_system_inline() {
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
    // OpenAI keeps system messages inline
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[0]["content"], "Be concise");
    assert_eq!(msgs[1]["role"], "user");
    assert_eq!(msgs[2]["role"], "assistant");
}

#[test]
fn test_openai_provider_config_system_prepended() {
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
    // Config system comes first, then inline system, then user
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[0]["content"], "Base system");
    assert_eq!(msgs[1]["role"], "system");
    assert_eq!(msgs[1]["content"], "Additional system");
    assert_eq!(msgs[2]["role"], "user");
}

#[tokio::test]
async fn test_openai_provider_stream_unsupported() {
    let provider = OpenAiProvider::new("sk-test")
        .with_base_url("http://localhost:1");
    let config = LlmConfig::default();

    let result = provider
        .stream(&[LlmMessage::user("Hi")], &config)
        .await;
    assert!(result.is_err());
    match result {
        Err(LlmError::Unsupported(msg)) => {
            assert!(msg.contains("OpenAiProvider"));
        }
        _ => panic!("Expected LlmError::Unsupported for streaming"),
    }
}

// ===========================================================================
// Feature Model Config (matching screenshot per-feature model settings)
//
// The feature model configuration maps to the various intelligence modules:
// Insights Chat, Ideation, Roadmap, GitHub Issues, PR Review
// ===========================================================================

#[test]
fn test_feature_model_insights_chat() {
    // InsightsEngine uses an LLM model for chat. Verify config can be set up
    // for the Insights Chat feature with appropriate model.
    let config = LlmConfig {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 4096,
        temperature: 0.7,
        system_prompt: Some("You are an insights chat assistant.".to_string()),
    };
    assert!(config.model.contains("claude"));
    assert!(config.system_prompt.is_some());
    assert!(config.max_tokens > 0);
}

#[test]
fn test_feature_model_ideation() {
    // Ideation feature uses a model for generating ideas
    let config = LlmConfig {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 8192,
        temperature: 0.9, // Higher temperature for creative ideation
        system_prompt: Some("Generate creative software improvement ideas.".to_string()),
    };
    assert!(config.temperature > 0.5); // Ideation benefits from higher temp
    assert!(config.max_tokens >= 4096);
}

#[test]
fn test_feature_model_roadmap() {
    // Roadmap feature uses a model for planning
    let config = LlmConfig {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 16384,
        temperature: 0.3, // Lower temperature for structured planning
        system_prompt: Some("Create a structured roadmap plan.".to_string()),
    };
    assert!(config.temperature < 0.5); // Roadmap needs structured output
    assert!(config.max_tokens >= 8192);
}

#[test]
fn test_feature_model_github_issues() {
    // GitHub Issues feature uses a model for issue analysis
    let config = LlmConfig {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 4096,
        temperature: 0.5,
        system_prompt: Some("Analyze and triage GitHub issues.".to_string()),
    };
    assert_eq!(config.max_tokens, 4096);
    assert!(config.system_prompt.as_deref().unwrap().contains("GitHub"));
}

#[test]
fn test_feature_model_pr_review() {
    // PR Review feature uses a model for code review
    let config = LlmConfig {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 8192,
        temperature: 0.2, // Very low temp for precise code review
        system_prompt: Some("Review pull requests for correctness and style.".to_string()),
    };
    assert!(config.temperature < 0.5); // PR review needs precision
    assert!(config.system_prompt.as_deref().unwrap().contains("Review"));
}

// ===========================================================================
// LlmUsageTracker
// ===========================================================================

#[test]
fn test_usage_tracker_new() {
    let tracker = LlmUsageTracker::new();
    assert_eq!(tracker.total_input_tokens, 0);
    assert_eq!(tracker.total_output_tokens, 0);
    assert_eq!(tracker.total_requests, 0);
    assert_eq!(tracker.total_tokens(), 0);
}

#[test]
fn test_usage_tracker_record() {
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
}

#[test]
fn test_usage_tracker_multiple_records() {
    let mut tracker = LlmUsageTracker::new();

    for i in 1..=5 {
        let resp = LlmResponse {
            content: format!("resp {}", i),
            model: "m".to_string(),
            input_tokens: i * 10,
            output_tokens: i * 5,
            finish_reason: "stop".to_string(),
        };
        tracker.record(&resp);
    }

    // Sum: input = 10+20+30+40+50 = 150, output = 5+10+15+20+25 = 75
    assert_eq!(tracker.total_input_tokens, 150);
    assert_eq!(tracker.total_output_tokens, 75);
    assert_eq!(tracker.total_requests, 5);
    assert_eq!(tracker.total_tokens(), 225);
}

#[test]
fn test_usage_tracker_default() {
    let tracker = LlmUsageTracker::default();
    assert_eq!(tracker.total_tokens(), 0);
    assert_eq!(tracker.total_requests, 0);
}

// ===========================================================================
// Provider Default implementations
// ===========================================================================

#[test]
fn test_mock_provider_default() {
    let provider = MockProvider::default();
    assert!(provider.captured_requests().is_empty());
}
