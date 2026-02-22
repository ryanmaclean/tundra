use std::collections::HashMap;
use std::sync::Arc;

use agent_harness::agent::{Agent, AgentConfig, AgentEvent};
use agent_harness::memory::InMemoryMemory;
use agent_harness::orchestrator::{Orchestrator, RoutingStrategy};
use agent_harness::provider::{create_provider, LlmProvider};
use agent_harness::retry::RetryConfig;
use agent_harness::tool::{FnTool, ToolRegistry};
use agent_harness::types::*;

use serde_json::json;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging (set RUST_LOG=debug for verbose output).
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("agent_harness=info".parse()?))
        .init();

    // -----------------------------------------------------------------------
    // 1. Configure providers
    // -----------------------------------------------------------------------

    let openrouter_config = ProviderConfig {
        api_key: std::env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| "your-api-key-here".into()),
        base_url: "https://openrouter.ai/api/v1".into(),
        model: "deepseek/deepseek-r1-0528:free".into(),
        extra_headers: HashMap::from([
            ("HTTP-Referer".into(), "https://your-app.com".into()),
            ("X-Title".into(), "Agent Harness".into()),
        ]),
        max_tokens: 4096,
        temperature: 0.7,
    };

    let provider: Arc<dyn LlmProvider> =
        Arc::from(create_provider(ProviderKind::OpenRouter, openrouter_config));

    // Get the shared quota tracker from the provider
    let quota_tracker = provider.quota_tracker();

    // Display initial quota status
    println!(
        "🔑 API Key Status: {}",
        if std::env::var("OPENROUTER_API_KEY").is_ok() {
            "✅ Configured"
        } else {
            "❌ Missing"
        }
    );
    quota_tracker.display_quota_status();

    // For HuggingFace Inference Endpoints:
    // let hf_config = ProviderConfig {
    //     api_key: std::env::var("HF_API_KEY").unwrap_or_default(),
    //     base_url: "https://your-endpoint.endpoints.huggingface.cloud/v1".into(),
    //     model: "tgi".into(),
    //     ..Default::default()  // won't work since no Default, fill in fields
    // };
    // let hf_provider = Arc::from(create_provider(ProviderKind::HuggingFace, hf_config));

    // -----------------------------------------------------------------------
    // 2. Create tools
    // -----------------------------------------------------------------------

    let mut research_tools = ToolRegistry::new();
    research_tools.register(FnTool::new(
        ToolDefinition {
            name: "search".into(),
            description: "Search the web for information".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" }
                },
                "required": ["query"]
            }),
        },
        |args| {
            let query = args["query"].as_str().unwrap_or("unknown");
            Ok(format!(
                "[Mock search results for: '{}'] - Result 1: Example info about {}.",
                query, query
            ))
        },
    ));

    let mut coding_tools = ToolRegistry::new();
    coding_tools.register(FnTool::new(
        ToolDefinition {
            name: "run_code".into(),
            description: "Execute a code snippet and return the output".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "language": { "type": "string", "description": "Programming language" },
                    "code": { "type": "string", "description": "Code to execute" }
                },
                "required": ["language", "code"]
            }),
        },
        |args| {
            let lang = args["language"].as_str().unwrap_or("unknown");
            let code = args["code"].as_str().unwrap_or("");
            Ok(format!(
                "[Mock {} execution]\nCode: {}\nOutput: (simulated result)",
                lang,
                &code[..code.len().min(100)]
            ))
        },
    ));

    // -----------------------------------------------------------------------
    // 3. Create agents
    // -----------------------------------------------------------------------

    let memory = Arc::new(InMemoryMemory::new());

    let researcher = Agent::new(
        AgentConfig {
            name: "researcher".into(),
            system_prompt: "You are a research agent. Use the search tool to find information and provide comprehensive answers.".into(),
            max_tool_rounds: 5,
            retry_config: RetryConfig::default(),
            stream: false,
        },
        provider.clone(),
        Arc::new(research_tools),
        memory.clone(),
    );

    let coder = Agent::new(
        AgentConfig {
            name: "coder".into(),
            system_prompt: "You are a coding agent. Help users write and debug code. Use the run_code tool to test solutions.".into(),
            max_tool_rounds: 5,
            retry_config: RetryConfig::default(),
            stream: false,
        },
        provider.clone(),
        Arc::new(coding_tools),
        memory.clone(),
    );

    // -----------------------------------------------------------------------
    // 4. Set up the orchestrator
    // -----------------------------------------------------------------------

    // Event channel for observing agent behavior.
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(256);

    // Spawn event listener.
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                AgentEvent::Thinking { agent } => {
                    println!("  [{agent}] thinking...");
                }
                AgentEvent::TextDelta { agent: _, text } => {
                    print!("{text}");
                }
                AgentEvent::ToolCallStart {
                    agent, tool_name, ..
                } => {
                    println!("  [{agent}] calling tool: {tool_name}");
                }
                AgentEvent::ToolCallResult {
                    agent,
                    tool_name,
                    result,
                } => {
                    println!(
                        "  [{agent}] {tool_name} -> {}",
                        &result[..result.len().min(100)]
                    );
                }
                AgentEvent::Response { agent, content } => {
                    println!(
                        "\n[{agent}] Response:\n{}\n",
                        &content[..content.len().min(500)]
                    );
                }
                AgentEvent::Error { agent, error } => {
                    eprintln!("  [{agent}] ERROR: {error}");
                }
            }
        }
    });

    // Keyword-based routing: "code"/"program" -> coder, else -> researcher.
    let keywords = HashMap::from([
        ("code".into(), "coder".into()),
        ("program".into(), "coder".into()),
        ("function".into(), "coder".into()),
        ("debug".into(), "coder".into()),
    ]);

    let orchestrator = Orchestrator::builder()
        .add_agent(researcher)
        .add_agent(coder)
        .strategy(RoutingStrategy::KeywordBased(keywords))
        .event_channel(event_tx)
        .build();

    // -----------------------------------------------------------------------
    // 5. Run!
    // -----------------------------------------------------------------------

    println!("=== Multi-Agent Orchestrator Demo ===\n");

    // Example: research query.
    println!("--- Query 1: Research ---");
    match orchestrator
        .run(
            "conv-1",
            "What are the latest developments in quantum computing?",
        )
        .await
    {
        Ok(response) => println!("Final: {}", &response[..response.len().min(300)]),
        Err(e) => eprintln!("Error: {e}"),
    }

    println!("\n--- Query 2: Coding ---");
    match orchestrator
        .run(
            "conv-2",
            "Write a function to compute fibonacci numbers in Rust",
        )
        .await
    {
        Ok(response) => println!("Final: {}", &response[..response.len().min(300)]),
        Err(e) => eprintln!("Error: {e}"),
    }

    // Display final quota status
    println!("\n📊 Final Usage Summary:");
    quota_tracker.display_quota_status();

    // Suggest best model for next usage
    if let Some(suggested_model) = quota_tracker.suggest_best_model() {
        println!("\n💡 Suggested model for next request: {}", suggested_model);
    }

    Ok(())
}
