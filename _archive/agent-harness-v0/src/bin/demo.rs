use std::collections::HashMap;
use std::io::{self, Write};
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
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("agent_harness=info".parse()?))
        .init();

    println!("🚀 Agent Harness Demo");
    println!("====================\n");

    // Check API key
    let api_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| {
        println!("❌ No OPENROUTER_API_KEY found");
        println!("🔑 Get a free key at: https://openrouter.ai/keys");
        println!("💡 Then run: export OPENROUTER_API_KEY=your-key\n");
        std::process::exit(1);
    });

    println!("✅ API Key configured");

    // Try multiple models to find a working one
    let models_to_try = vec![
        "meta-llama/llama-3.3-70b-instruct:free",
        "arcee-ai/trinity-large-preview:free",
        "deepseek/deepseek-r1-0528:free",
        "qwen/qwen3-235b-a22b-thinking-2507",
    ];

    let mut working_model = None;
    let mut working_provider = None;

    for model in models_to_try {
        println!("🔍 Testing model: {}", model);

        let config = ProviderConfig {
            api_key: api_key.clone(),
            base_url: "https://openrouter.ai/api/v1".into(),
            model: model.to_string(),
            extra_headers: HashMap::from([
                ("HTTP-Referer".into(), "https://agent-harness.dev".into()),
                ("X-Title".into(), "Agent Harness Demo".into()),
            ]),
            max_tokens: 1000,
            temperature: 0.7,
        };

        let provider: Arc<dyn LlmProvider> =
            Arc::from(create_provider(ProviderKind::OpenRouter, config));

        // Test with a simple request
        let test_messages = vec![
            Message::system("You are a helpful assistant."),
            Message::user("Say 'Hello' in one word."),
        ];

        match provider.chat_completion(&test_messages, &[]).await {
            Ok(response) => {
                println!(
                    "✅ Model {} works! Response: {}",
                    model,
                    &response.content[..50.min(response.content.len())]
                );
                working_model = Some(model.to_string());
                working_provider = Some(provider);
                break;
            }
            Err(e) => {
                println!("❌ Model {} failed: {}", model, e);
            }
        }
    }

    let provider = working_provider.ok_or_else(|| anyhow::anyhow!("No working model found"))?;
    let model = working_model.unwrap();
    let quota_tracker = provider.quota_tracker();

    println!("\n🎯 Using model: {}", model);
    println!("📊 Quota Status:");
    quota_tracker.display_quota_status();

    // Create tools
    let mut tools = ToolRegistry::new();
    tools.register(FnTool::new(
        ToolDefinition {
            name: "search".into(),
            description: "Search for current information".into(),
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
            Ok(format!("🔍 Found information about: {}", query))
        },
    ));

    // Create specialized agents
    let memory = Arc::new(InMemoryMemory::new());

    let researcher = Agent::new(
        AgentConfig {
            name: "researcher".into(),
            system_prompt: "You are a research assistant. Provide accurate, up-to-date information. Use search tools when needed. Be thorough but concise.".into(),
            max_tool_rounds: 2,
            retry_config: RetryConfig::default(),
            stream: false,
        },
        provider.clone(),
        Arc::new(tools.clone()),
        memory.clone(),
    );

    let coder = Agent::new(
        AgentConfig {
            name: "coder".into(),
            system_prompt: "You are a coding expert. Write clean, efficient code. Explain your solutions clearly. Focus on best practices.".into(),
            max_tool_rounds: 1,
            retry_config: RetryConfig::default(),
            stream: false,
        },
        provider.clone(),
        Arc::new(tools.clone()),
        memory.clone(),
    );

    // Set up orchestrator with keyword routing
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(256);

    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                AgentEvent::Thinking { agent } => println!("  🤔 [{}] thinking...", agent),
                AgentEvent::TextDelta { text, agent: _ } => {
                    print!("{}", text);
                    io::stdout().flush().unwrap();
                }
                AgentEvent::ToolCallStart {
                    agent, tool_name, ..
                } => {
                    println!("  🔧 [{}] calling {}", agent, tool_name);
                }
                AgentEvent::ToolCallResult {
                    agent,
                    tool_name,
                    result,
                } => {
                    println!(
                        "  ✅ [{}] {} -> {}",
                        agent,
                        tool_name,
                        &result[..result.len().min(80)]
                    );
                }
                AgentEvent::Response { agent, content } => {
                    println!(
                        "\n  📝 [{}] Response:\n  {}\n",
                        agent,
                        &content[..content.len().min(400)]
                    );
                }
                AgentEvent::Error { agent, error } => {
                    println!("  ❌ [{}] ERROR: {}", agent, error);
                }
            }
        }
    });

    let keywords = HashMap::from([
        ("research".into(), "researcher".into()),
        ("search".into(), "researcher".into()),
        ("information".into(), "researcher".into()),
        ("code".into(), "coder".into()),
        ("function".into(), "coder".into()),
        ("program".into(), "coder".into()),
        ("debug".into(), "coder".into()),
        ("algorithm".into(), "coder".into()),
    ]);

    let orchestrator = Orchestrator::builder()
        .add_agent(researcher)
        .add_agent(coder)
        .strategy(RoutingStrategy::KeywordBased(keywords))
        .event_channel(event_tx)
        .build();

    // Demo scenarios
    println!("\n🎬 Running Demo Scenarios");
    println!("========================\n");

    let scenarios = [
        (
            "What are the latest developments in quantum computing?",
            "research",
        ),
        (
            "Write a Rust function to calculate fibonacci numbers efficiently",
            "code",
        ),
        (
            "Search for information about renewable energy trends",
            "research",
        ),
        ("Create a Python function to sort a list of numbers", "code"),
    ];

    for (i, (query, expected_agent)) in scenarios.iter().enumerate() {
        println!("--- Scenario {} ---", i + 1);
        println!("📝 Query: {}", query);
        println!("🎯 Expected agent: {}", expected_agent);

        match orchestrator.run(&format!("demo-{}", i + 1), query).await {
            Ok(response) => {
                println!("✅ Success: {}", &response[..response.len().min(200)]);
            }
            Err(e) => {
                println!("❌ Failed: {}", e);
            }
        }

        println!();
    }

    // Final quota report
    println!("📊 Final Quota Report");
    println!("====================");
    quota_tracker.display_quota_status();

    // Suggest next steps
    if let Some(suggested) = quota_tracker.suggest_best_model() {
        println!("💡 Suggested model for next session: {}", suggested);
    }

    println!("\n🎉 Demo completed!");
    println!("💡 Try the interactive mode: cargo run --bin interactive");

    Ok(())
}
