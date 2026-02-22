# Agent Harness

A multi-agent orchestrator with quota-aware API management for free LLM models.

## Quick Start

### 1. Get API Key
Get a free OpenRouter API key at https://openrouter.ai/keys

### 2. Set up Environment
```bash
export OPENROUTER_API_KEY=your-api-key-here
```

### 3. Run the Harness

**Interactive Mode (Recommended):**
```bash
cargo run --bin interactive
```

**Demo Mode:**
```bash
cargo run --bin demo
```

**Original Demo:**
```bash
cargo run --bin agent-harness
```

## Features

- ✅ **Quota-Aware**: Real-time usage tracking for free models
- ✅ **Multi-Agent**: Specialized research and coding agents
- ✅ **Smart Routing**: Keyword-based agent selection
- ✅ **Free Models**: Optimized for OpenRouter free tier
- ✅ **Interactive Chat**: Real-time conversational interface
- ✅ **Error Handling**: Graceful fallbacks and model suggestions

## 📚 Documentation

**New to Agent Harness?** Follow this learning path:

1. **[Getting Started](docs/GETTING_STARTED.md)** - Complete beginner's guide (10 min setup)
   - Prerequisites and installation
   - API key setup
   - First conversation walkthrough
   - Interactive commands reference

2. **[Examples](docs/EXAMPLES.md)** - Real conversation flows and code samples
   - Simple Q&A and tool usage
   - Code generation examples
   - Multi-turn conversations
   - Quota management examples
   - Programmatic usage patterns

3. **[Architecture](docs/ARCHITECTURE.md)** - System design and internals
   - Component overview and data flow
   - Key concepts (Agents, Tools, Memory, Quota)
   - Module breakdown with file locations
   - Extension points for customization
   - Auto-Tundra vs Agent-Harness comparison

4. **[Troubleshooting](docs/TROUBLESHOOTING.md)** - Solutions to common issues
   - API key problems
   - Model availability errors
   - Quota management
   - Connection and build issues
   - Performance optimization

5. **[Production Guide](PRODUCTION.md)** - Enterprise-grade features
   - Circuit breaker pattern
   - Security guardrails
   - Memory management
   - Rate limiting
   - Health monitoring

## Usage Examples

### Interactive Chat
```bash
$ cargo run --bin interactive
🤖 Agent Harness - Interactive Mode
Type 'help' for commands, 'quit' to exit

💬 You: What is 2+2?
🚀 Processing...
🤔 [assistant] thinking...
🔧 [assistant] using calculate
✅ [assistant] calculate -> [Calculation: 2+2 = Result]
✨ Final: The answer to 2+2 is 4.

💬 You: Write a Rust function for fibonacci
🚀 Processing...
🤔 [assistant] thinking...
✨ Final: Here's an efficient Fibonacci function using memoization...
```

### Commands
- `help` - Show available commands
- `status` - Display quota usage
- `clear` - Clear the screen
- `quit` or `exit` - Exit the program

## Free Models Supported

The system automatically finds working models:
- `meta-llama/llama-3.3-70b-instruct:free` (most reliable)
- `arcee-ai/trinity-large-preview:free`
- `deepseek/deepseek-r1-0528:free`
- And more...

## Quota Management

**Free Tier Limits:**
- 100 requests per day per model
- 10,000 tokens per day per model
- Automatic quota tracking and warnings

**Sample Quota Display:**
```
📊 API QUOTA STATUS
============================================================
✅ 🆓 meta-llama/llama-3.3-70b-instruct:free
  Requests: 12/100 (12.0%)
  Tokens: 2395/10000 (24.0%)
  Reset: SystemTime { tv_sec: 1771113600, tv_nsec: 0 }
============================================================
```

## Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   User Input   │───▶│   Orchestrator   │───▶│    Agents       │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                              │                        │
                              ▼                        ▼
                       ┌──────────────┐         ┌──────────────┐
                       │ Quota Tracker│         │   Tools      │
                       └──────────────┘         └──────────────┘
```

## Troubleshooting

Having issues? See the **[Troubleshooting Guide](docs/TROUBLESHOOTING.md)** for detailed solutions.

**Quick Fixes:**
- **API Key Issues**: Verify key format starts with `sk-or-v1-`
- **Model Unavailable**: System auto-suggests alternatives
- **Quota Exceeded**: Check `status` command, wait for reset, or try different model
- **Build Errors**: Ensure Rust 1.91+, try `cargo clean && cargo build`

For detailed solutions, error codes, and prevention tips, see [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)

## Development

**Build:**
```bash
cargo build
```

**Run Tests:**
```bash
cargo test
```

**Check Code:**
```bash
cargo check
```

## Examples

For comprehensive examples including multi-turn conversations, tool usage, error handling, and more, see **[docs/EXAMPLES.md](docs/EXAMPLES.md)**.

### Programmatic Usage
```rust
use agent_harness::*;

let provider = create_provider(ProviderKind::OpenRouter, config);
let orchestrator = Orchestrator::builder()
    .add_agent(researcher)
    .add_agent(coder)
    .strategy(RoutingStrategy::KeywordBased(keywords))
    .build();

let response = orchestrator.run("conv-1", "Your query here").await?;
```

See [docs/EXAMPLES.md](docs/EXAMPLES.md) for more examples including memory management, tool creation, and streaming responses.
