# Agent Harness Examples

This document shows real examples of Agent Harness conversations, from simple to complex.

## 📚 Table of Contents

1. [Simple Q&A](#simple-qa)
2. [Tool Usage](#tool-usage)
3. [Code Generation](#code-generation)
4. [Multi-Turn Conversations](#multi-turn-conversations)
5. [Quota Management](#quota-management)
6. [Error Handling](#error-handling)
7. [Programmatic Usage](#programmatic-usage)

---

## Simple Q&A

The most basic use case - ask a question, get an answer.

### Example 1: Basic Math

**Input:**
```
💬 You: What is 2 + 2?
```

**Output:**
```
🚀 Processing...
🤔 [assistant] thinking...
🔧 [assistant] using calculate
✅ [assistant] calculate -> [Calculation: 2+2 = Result]
✨ Final: The answer to 2+2 is 4.
```

### Example 2: General Knowledge

**Input:**
```
💬 You: What is the capital of France?
```

**Output:**
```
🚀 Processing...
🤔 [assistant] thinking...
✨ Final: The capital of France is Paris.
```

### Example 3: Definitions

**Input:**
```
💬 You: What is Rust?
```

**Output:**
```
🚀 Processing...
🤔 [assistant] thinking...
✨ Final: Rust is a systems programming language focused on safety,
speed, and concurrency. It provides memory safety without garbage
collection through its ownership system and borrow checker.
```

---

## Tool Usage

Watch the agent use tools to solve problems.

### Example 4: Calculator Tool

**Input:**
```
💬 You: Calculate the square root of 144
```

**Output:**
```
🚀 Processing...
🤔 [assistant] thinking...
🔧 [assistant] using calculate
✅ [assistant] calculate -> [Result: 12]
✨ Final: The square root of 144 is 12.
```

**What Happened:**
1. Agent recognized this needs calculation
2. Used the `calculate` tool
3. Tool returned result
4. Agent formatted response

### Example 5: Multiple Tool Calls

**Input:**
```
💬 You: What is 5 factorial plus 10 squared?
```

**Output:**
```
🚀 Processing...
🤔 [assistant] thinking...
🔧 [assistant] using calculate
✅ [assistant] calculate -> [5! = 120]
🔧 [assistant] using calculate
✅ [assistant] calculate -> [10² = 100]
✨ Final: 5 factorial (120) plus 10 squared (100) equals 220.
```

---

## Code Generation

The agent can write code for you.

### Example 6: Simple Function

**Input:**
```
💬 You: Write a Rust function to calculate Fibonacci numbers
```

**Output:**
```
🚀 Processing...
🤔 [assistant] thinking...
✨ Final: Here's an efficient Fibonacci function using memoization:

fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut a = 0;
            let mut b = 1;
            for _ in 2..=n {
                let temp = a + b;
                a = b;
                b = temp;
            }
            b
        }
    }
}

This implementation runs in O(n) time and O(1) space, making it
efficient for calculating Fibonacci numbers.
```

### Example 7: Code with Tests

**Input:**
```
💬 You: Write a function to check if a string is a palindrome with tests
```

**Output:**
```
🚀 Processing...
🤔 [assistant] thinking...
✨ Final: Here's a palindrome checker with tests:

fn is_palindrome(s: &str) -> bool {
    let cleaned: String = s.chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_lowercase().next().unwrap())
        .collect();

    cleaned == cleaned.chars().rev().collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_palindrome() {
        assert!(is_palindrome("racecar"));
        assert!(is_palindrome("A man a plan a canal Panama"));
    }

    #[test]
    fn test_not_palindrome() {
        assert!(!is_palindrome("hello"));
        assert!(!is_palindrome("world"));
    }

    #[test]
    fn test_empty_and_single() {
        assert!(is_palindrome(""));
        assert!(is_palindrome("a"));
    }
}
```

---

## Multi-Turn Conversations

The agent remembers context from previous messages.

### Example 8: Building on Context

**Turn 1:**
```
💬 You: What is async/await in Rust?

🚀 Processing...
✨ Final: Async/await in Rust is a way to write asynchronous code that
looks synchronous. The `async` keyword creates a Future, and `await`
pauses execution until that Future completes, allowing other tasks to
run in the meantime.
```

**Turn 2:**
```
💬 You: Can you show me an example?

🚀 Processing...
✨ Final: Here's a simple example building on what we just discussed:

use tokio;

#[tokio::main]
async fn main() {
    let result = fetch_data().await;
    println!("Got: {}", result);
}

async fn fetch_data() -> String {
    // Simulate async operation
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    "Data loaded!".to_string()
}

The `.await` pauses fetch_data() execution, allowing other tasks to run.
```

**Turn 3:**
```
💬 You: What about error handling?

🚀 Processing...
✨ Final: For error handling with async/await, use Result with the ? operator:

async fn fetch_data() -> Result<String, Box<dyn std::error::Error>> {
    let response = reqwest::get("https://api.example.com").await?;
    let body = response.text().await?;
    Ok(body)
}

The ? operator propagates errors up, just like in synchronous code.
```

**What Happened:**
- Agent remembered we were discussing async/await
- Each response built on the previous context
- No need to repeat the question

---

## Quota Management

Understanding and managing your API limits.

### Example 9: Checking Status

**Input:**
```
💬 You: status
```

**Output:**
```
📊 API QUOTA STATUS
============================================================
✅ 🆓 meta-llama/llama-3.3-70b-instruct:free
  Requests: 12/100 (12.0%)
  Tokens: 2,395/10,000 (24.0%)
  Reset: 2026-02-16 00:00:00 UTC
============================================================
```

**Interpretation:**
- ✅ Model is available
- 🆓 Using free tier
- Used 12 of 100 daily requests
- Used ~24% of token quota
- Quota resets at midnight UTC

### Example 10: Low Quota Warning

**Input:**
```
💬 You: Explain quantum computing in detail
```

**Output:**
```
🚀 Processing...
⚠️  Warning: Token quota at 85% (8,500/10,000)
🤔 [assistant] thinking...
✨ Final: [Response about quantum computing...]

⚠️  Tip: You're running low on tokens. Consider using 'status' to monitor usage.
```

### Example 11: Quota Exceeded

**Input:**
```
💬 You: Tell me another long story
```

**Output:**
```
🚀 Processing...
❌ Error: Quota exceeded for meta-llama/llama-3.3-70b-instruct:free
   Requests: 100/100 (100%)
   Reset: 2026-02-16 00:00:00 UTC (in 5h 23m)

💡 Suggestions:
   - Try alternative model: arcee-ai/trinity-large-preview:free
   - Wait for quota reset
   - Upgrade your OpenRouter account for higher limits
```

---

## Error Handling

See how the system handles common errors.

### Example 12: Model Unavailable

**Input:**
```
💬 You: Hello!
```

**Output:**
```
🚀 Processing...
❌ Model unavailable: meta-llama/llama-3.3-70b-instruct:free

🔄 Attempting fallback to: arcee-ai/trinity-large-preview:free
🚀 Processing...
✨ Final: Hello! How can I help you today?
```

**What Happened:**
- Primary model was down
- System automatically tried alternative
- Request succeeded without user intervention

### Example 13: Invalid API Key

**Input:**
```bash
export OPENROUTER_API_KEY=invalid-key
cargo run --bin interactive
```

**Output:**
```
❌ Error: Invalid API key format

API key must:
  - Start with 'sk-or-v1-' (OpenRouter prefix)
  - Be at least 20 characters long
  - Contain only alphanumeric characters, hyphens, and underscores

Please check your OPENROUTER_API_KEY environment variable.
```

### Example 14: Network Error

**Input:**
```
💬 You: What is the weather like?
```

**Output (if network is down):**
```
🚀 Processing...
❌ Connection error: Failed to connect to OpenRouter API

Possible causes:
  - No internet connection
  - OpenRouter service is down
  - Firewall blocking requests

Please check:
  1. Your internet connection
  2. OpenRouter status: https://status.openrouter.ai
  3. Firewall/proxy settings
```

---

## Programmatic Usage

Using Agent Harness in your own Rust code.

### Example 15: Basic Integration

```rust
use agent_harness::*;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let api_key = env::var("OPENROUTER_API_KEY")?;

    // Create provider
    let config = ProviderConfig {
        api_key,
        model: "meta-llama/llama-3.3-70b-instruct:free".to_string(),
        ..Default::default()
    };

    let provider = create_provider(ProviderKind::OpenRouter, config);

    // Create a simple agent
    let agent = Agent::builder()
        .name("assistant")
        .instructions("You are a helpful assistant.")
        .build();

    // Send a message
    let messages = vec![
        Message::user("What is 2+2?"),
    ];

    let response = provider.chat_completion(messages, vec![]).await?;
    println!("Response: {}", response.content);

    Ok(())
}
```

### Example 16: With Orchestrator

```rust
use agent_harness::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create specialized agents
    let researcher = Agent::builder()
        .name("researcher")
        .instructions("Research and provide factual information.")
        .build();

    let coder = Agent::builder()
        .name("coder")
        .instructions("Write clean, efficient code.")
        .build();

    // Define routing keywords
    let keywords = vec![
        ("researcher".to_string(), vec!["what".to_string(), "explain".to_string()]),
        ("coder".to_string(), vec!["write".to_string(), "function".to_string()]),
    ];

    // Create orchestrator
    let orchestrator = Orchestrator::builder()
        .add_agent(researcher)
        .add_agent(coder)
        .strategy(RoutingStrategy::KeywordBased(keywords))
        .build();

    // Process requests
    let response = orchestrator
        .run("conv-1", "Write a Rust function for sorting")
        .await?;

    println!("Agent used: {}", response.agent_name);
    println!("Response: {}", response.content);

    Ok(())
}
```

### Example 17: With Memory

```rust
use agent_harness::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create memory backend
    let memory = Arc::new(InMemoryMemory::new());

    let agent = Agent::builder()
        .name("assistant")
        .instructions("You are helpful and remember context.")
        .memory(memory.clone())
        .build();

    // First message
    memory.append("conv-1", &[Message::user("My name is Alice")]).await;
    let response1 = agent.process("conv-1", "Hello").await?;
    memory.append("conv-1", &[Message::assistant(response1.content)]).await;

    // Second message - agent remembers
    let response2 = agent.process("conv-1", "What's my name?").await?;
    println!("{}", response2.content); // "Your name is Alice"

    Ok(())
}
```

---

## 💡 Tips for Best Results

1. **Be Specific**: "Write a function to sort integers" is better than "write code"
2. **Use Context**: Reference previous messages - the agent remembers
3. **Monitor Quota**: Check `status` regularly to avoid interruptions
4. **Experiment**: Try different phrasings if you don't get what you want
5. **Read Errors**: Error messages are helpful and suggest fixes

## 📚 Next Steps

- **Understand the Architecture**: [ARCHITECTURE.md](ARCHITECTURE.md)
- **Troubleshoot Issues**: [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
- **Production Deployment**: [PRODUCTION.md](../PRODUCTION.md)

---

**Want to try these examples?** Run `cargo run --bin interactive` and copy-paste the inputs!
