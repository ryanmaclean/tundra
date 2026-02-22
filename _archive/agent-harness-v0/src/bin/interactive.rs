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

    println!("🤖 Agent Harness - Interactive Mode");
    println!("Type 'help' for commands, 'quit' to exit\n");

    // Check API key
    let api_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| {
        println!("❌ OPENROUTER_API_KEY not set");
        println!("Get a free key at: https://openrouter.ai/keys");
        std::process::exit(1);
    });

    // Start with a working free model
    let config = ProviderConfig {
        api_key: api_key.clone(),
        base_url: "https://openrouter.ai/api/v1".into(),
        model: "meta-llama/llama-3.3-70b-instruct:free".into(),
        extra_headers: HashMap::from([
            ("HTTP-Referer".into(), "https://agent-harness.dev".into()),
            ("X-Title".into(), "Agent Harness".into()),
        ]),
        max_tokens: 2048,
        temperature: 0.7,
    };

    let provider: Arc<dyn LlmProvider> =
        Arc::from(create_provider(ProviderKind::OpenRouter, config));
    let quota_tracker = provider.quota_tracker();

    // Create tools
    let mut tools = ToolRegistry::new();
    tools.register(FnTool::new(
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
                "[Search results for: '{}'] - Found relevant information about {}.",
                query, query
            ))
        },
    ));

    tools.register(FnTool::new(
        ToolDefinition {
            name: "calculate".into(),
            description: "Perform mathematical calculations".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "expression": { "type": "string", "description": "Mathematical expression" }
                },
                "required": ["expression"]
            }),
        },
        |args| {
            let expr = args["expression"].as_str().unwrap_or("0");
            Ok(format!("[Calculation: {} = Result]", expr))
        },
    ));

    // Create agents
    let memory = Arc::new(InMemoryMemory::new());

    let assistant = Agent::new(
        AgentConfig {
            name: "assistant".into(),
            system_prompt: "You are a helpful AI assistant. Use tools when needed to provide accurate information. Be concise and helpful.".into(),
            max_tool_rounds: 3,
            retry_config: RetryConfig::default(),
            stream: false,
        },
        provider.clone(),
        Arc::new(tools),
        memory.clone(),
    );

    // Set up orchestrator
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(256);

    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                AgentEvent::Thinking { agent } => {
                    println!("🤔 [{}] thinking...", agent);
                    io::stdout().flush().unwrap();
                }
                AgentEvent::TextDelta { agent: _, text } => {
                    print!("{}", text);
                    io::stdout().flush().unwrap();
                }
                AgentEvent::ToolCallStart {
                    agent, tool_name, ..
                } => {
                    println!("🔧 [{}] using {}", agent, tool_name);
                    io::stdout().flush().unwrap();
                }
                AgentEvent::ToolCallResult {
                    agent,
                    tool_name,
                    result,
                } => {
                    println!(
                        "✅ [{}] {} -> {}",
                        agent,
                        tool_name,
                        &result[..result.len().min(100)]
                    );
                    io::stdout().flush().unwrap();
                }
                AgentEvent::Response { agent, content } => {
                    print!(
                        "\n📝 [{}] Response: {}\n\n",
                        agent,
                        &content[..content.len().min(500)]
                    );
                    io::stdout().flush().unwrap();
                }
                AgentEvent::Error { agent, error } => {
                    println!("❌ [{}] ERROR: {}", agent, error);
                    io::stdout().flush().unwrap();
                }
            }
        }
    });

    let orchestrator = Orchestrator::builder()
        .add_agent(assistant)
        .strategy(RoutingStrategy::Fixed("assistant".into()))
        .event_channel(event_tx)
        .build();

    // Interactive loop
    let mut conversation_id = 1;

    loop {
        print!("💬 You: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        match input {
            "quit" | "exit" => {
                println!("👋 Goodbye!");
                break;
            }
            "help" => {
                print_help();
                continue;
            }
            "status" => {
                quota_tracker.display_quota_status();
                continue;
            }
            "clear" => {
                print!("\x1B[2J\x1B[1;1H");
                continue;
            }
            "" => continue,
            _ => {
                println!("🚀 Processing...");
                io::stdout().flush().unwrap();

                match orchestrator
                    .run(&format!("conv-{}", conversation_id), input)
                    .await
                {
                    Ok(response) => {
                        print!("✨ Final: {}\n\n", &response[..response.len().min(300)]);
                        io::stdout().flush().unwrap();
                    }
                    Err(e) => {
                        print!("❌ Error: {}\n\n", e);
                        io::stdout().flush().unwrap();

                        // Try to suggest a different model if this one fails
                        if let Some(suggested) = quota_tracker.suggest_best_model() {
                            println!("💡 Try switching to model: {}", suggested);
                            io::stdout().flush().unwrap();
                        }
                    }
                }

                conversation_id += 1;

                // Show quota status periodically
                if conversation_id % 3 == 0 {
                    print!("📊 ");
                    quota_tracker.display_quota_status();
                }
            }
        }
    }

    Ok(())
}

fn print_help() {
    println!("\n🤖 Agent Harness Commands:");
    println!("  help    - Show this help message");
    println!("  status  - Show quota usage status");
    println!("  clear   - Clear the screen");
    println!("  quit    - Exit the program");
    println!("  exit    - Exit the program");
    println!("\n💡 Tips:");
    println!("  - Ask questions about any topic");
    println!("  - Request calculations with 'calculate: 2+2'");
    println!("  - Search for information with 'search: topic'");
    println!("  - The system uses free models with daily limits");
    println!();
}
