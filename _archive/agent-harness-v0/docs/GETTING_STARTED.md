# Getting Started with Agent Harness

Welcome! This guide will help you get the Agent Harness up and running in under 10 minutes.

## 📋 Prerequisites

Before you begin, make sure you have:

### 1. Rust & Cargo Installed

Check your installation:
```bash
rustc --version  # Should be 1.91+
cargo --version  # Should be 1.91+
```

**Don't have Rust?** Install it from [rustup.rs](https://rustup.rs/)

### 2. OpenRouter API Key

The Agent Harness uses OpenRouter's free LLM models. You'll need an API key.

**Get your free API key:**
1. Go to https://openrouter.ai/keys
2. Sign up or log in
3. Click "Create Key"
4. Copy your API key (starts with `sk-or-v1-...`)

**Free Tier Limits:**
- 100 requests per day per model
- 10,000 tokens per day per model
- No credit card required

### 3. Basic Rust Knowledge (Optional)

You don't need to be a Rust expert, but basic familiarity with:
- Running cargo commands
- Understanding async/await concepts
- Reading Rust error messages

will be helpful for extending the system.

## 🚀 Quick Start

### Step 1: Clone or Navigate to the Repository

```bash
cd /Users/studio/rust-harness
```

### Step 2: Set Up Your API Key

**Option A: Environment Variable (Recommended for testing)**
```bash
export OPENROUTER_API_KEY=sk-or-v1-your-actual-key-here
```

**Option B: .env File (Recommended for development)**
```bash
echo "OPENROUTER_API_KEY=sk-or-v1-your-actual-key-here" > .env
```

⚠️ **Important:** Never commit your `.env` file or API key to git!

### Step 3: Build the Project

```bash
cargo build --release
```

This may take a few minutes on first run as it downloads dependencies.

### Step 4: Run Interactive Mode

```bash
cargo run --bin interactive
```

You should see:
```
🤖 Agent Harness - Interactive Mode
Type 'help' for commands, 'quit' to exit

💬 You: _
```

### Step 5: Try Your First Conversation

Type a simple question:
```
💬 You: What is 2 + 2?
```

You should see the agent processing and responding:
```
🚀 Processing...
🤔 [assistant] thinking...
🔧 [assistant] using calculate
✅ [assistant] calculate -> [Calculation: 2+2 = Result]
✨ Final: The answer to 2+2 is 4.
```

🎉 **Congratulations!** You've successfully run your first agent conversation.

## 🎮 Interactive Commands

Once you're in interactive mode, you can use these commands:

| Command | Description |
|---------|-------------|
| `help` | Show all available commands |
| `status` | Display your current quota usage |
| `clear` | Clear the terminal screen |
| `quit` or `exit` | Exit the program |

### Check Your Quota

Type `status` to see your API usage:
```
💬 You: status

📊 API QUOTA STATUS
============================================================
✅ 🆓 meta-llama/llama-3.3-70b-instruct:free
  Requests: 1/100 (1.0%)
  Tokens: 245/10000 (2.5%)
  Reset: 2026-02-16 00:00:00 UTC
============================================================
```

## 🧪 Try Different Examples

### Simple Math
```
💬 You: Calculate the square root of 144
```

### Code Generation
```
💬 You: Write a Rust function that calculates Fibonacci numbers
```

### Multi-Turn Conversation
```
💬 You: What is Rust?
💬 You: What are its main benefits?
💬 You: How does it compare to C++?
```

The agent remembers your conversation context!

## 📦 Other Modes

### Demo Mode
See pre-programmed agent interactions:
```bash
cargo run --bin demo
```

### Original Agent Harness
Run the original implementation:
```bash
cargo run --bin agent-harness
```

## 🔍 Understanding the Output

When the agent responds, you'll see different emoji indicators:

| Emoji | Meaning |
|-------|---------|
| 🚀 | Processing your request |
| 🤔 | Agent is thinking |
| 🔧 | Agent is using a tool |
| ✅ | Tool execution successful |
| ✨ | Final response |
| ⚠️ | Warning (e.g., low quota) |
| ❌ | Error occurred |

## ⚠️ Common First-Time Issues

### "Invalid API key format"
- Check your key starts with `sk-or-v1-`
- Ensure no extra spaces or quotes
- Verify the key is set: `echo $OPENROUTER_API_KEY`

### "Model unavailable"
- The free model might be temporarily down
- The system will suggest alternatives
- Try a different time of day

### "Quota exceeded"
- You've hit your daily limit (100 requests/day)
- Wait for the reset time shown in `status`
- Or try a different free model

### Build Errors
- Ensure Rust 1.91+ is installed
- Try `cargo clean` then `cargo build` again
- Check internet connection for dependencies

## 📚 Next Steps

Now that you're up and running:

1. **See Examples** → Read [EXAMPLES.md](EXAMPLES.md) for more conversation patterns
2. **Understand Architecture** → Check [ARCHITECTURE.md](ARCHITECTURE.md) to learn how it works
3. **Solve Issues** → Visit [TROUBLESHOOTING.md](TROUBLESHOOTING.md) if you encounter problems
4. **Production Deployment** → See [PRODUCTION.md](../PRODUCTION.md) for advanced features

## 🎯 Quick Reference

```bash
# Set API key
export OPENROUTER_API_KEY=your-key-here

# Run interactive mode (recommended)
cargo run --bin interactive

# Check quota inside interactive mode
status

# Exit
quit
```

## 💡 Tips for Success

- **Start Simple**: Try basic questions before complex tasks
- **Monitor Quota**: Check `status` regularly to avoid hitting limits
- **Read Errors**: Error messages are helpful - they suggest solutions
- **Experiment**: The agent can handle many types of requests
- **Ask for Help**: Type `help` to see available commands

## 🤝 Getting Help

- **Issues?** Check [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
- **Questions?** Read [ARCHITECTURE.md](ARCHITECTURE.md) to understand how things work
- **Examples?** See [EXAMPLES.md](EXAMPLES.md) for inspiration

---

**Ready to dive deeper?** Continue to [EXAMPLES.md](EXAMPLES.md) to see more complex use cases!
