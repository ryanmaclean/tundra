# Getting Started with Auto-Tundra

**[← Back to README](README.md)**

This guide will get you from zero to running your first AI agent in under 15 minutes.

---

## 📋 Prerequisites

### 1. Rust Toolchain

Auto-Tundra requires **Rust 1.91 or later**.

**Check your version:**
```bash
rustc --version
```

**Don't have Rust? Install it:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
rustup update
```

### 2. API Keys

You need **at least one** LLM provider API key:

| Provider | Get Key | Free Tier | Best For |
|----------|---------|-----------|----------|
| **Anthropic** | https://console.anthropic.com/settings/keys | No (but cheapest per token) | Claude 3/4 models, production use |
| **OpenRouter** | https://openrouter.ai/keys | Yes (100 req/day) | Testing, multiple models, experimentation |
| **OpenAI** | https://platform.openai.com/api-keys | No (but $5 free credit) | GPT-3.5/4 models |

**Recommendation for beginners:** Start with **OpenRouter** for free testing, then add **Anthropic** for production.

### 3. Development Tools (Recommended)

```bash
# Fast test runner (highly recommended)
cargo install cargo-nextest --locked

# Security checking
cargo install cargo-deny --locked

# Code analysis
pip3 install semgrep

# Datadog CI (optional, for metrics upload)
npm install -g @datadog/datadog-ci
```

---

## 🚀 Installation

### Step 1: Navigate to Repository

```bash
cd /Users/studio/rust-harness
```

### Step 2: Configure API Keys

**Option A: Environment Variables (Quick Start)**
```bash
# Choose ONE or MORE providers
export ANTHROPIC_API_KEY=sk-ant-your-key-here
export OPENROUTER_API_KEY=sk-or-v1-your-key-here
export OPENAI_API_KEY=sk-your-key-here
```

**Option B: Shell Profile (Persistent)**

Add to `~/.zshrc` or `~/.bashrc`:
```bash
echo 'export ANTHROPIC_API_KEY=sk-ant-your-key-here' >> ~/.zshrc
source ~/.zshrc
```

**Option C: .env File (Not Recommended - Security Risk)**
```bash
# Create .env in project root
cat > .env << EOF
ANTHROPIC_API_KEY=sk-ant-your-key-here
OPENROUTER_API_KEY=sk-or-v1-your-key-here
EOF

# Add to .gitignore (CRITICAL!)
echo ".env" >> .gitignore
```

⚠️ **NEVER commit API keys to git!**

**Verify Your Keys:**
```bash
echo $ANTHROPIC_API_KEY    # Should show your key
echo $OPENROUTER_API_KEY   # Should show your key
```

### Step 3: Build the Project

```bash
# Build all crates
make build

# Or use cargo directly
cargo build --release
```

**First build takes 5-10 minutes** as it downloads and compiles dependencies.

**Build Output:**
```
Compiling tokio v1.40...
Compiling serde v1.0...
Compiling at-core v0.1.0
Compiling at-agents v0.1.0
...
Finished `release` profile [optimized] target(s) in 8m 23s
```

### Step 4: Run Tests (Verify Setup)

```bash
make test
```

**Expected Output:**
```
Running 1483 tests...
✓ at-core::tests::config_loads_from_env (0.002s)
✓ at-agents::tests::role_registry_loads (0.005s)
...
Test run successful. 1483 tests passed.
```

If tests pass, **you're ready to go!** 🎉

---

## 🎯 First Agent Run

### Understanding Auto-Tundra's "Bead Board"

Auto-Tundra uses a **bead board metaphor** for task management:
- **Bead** = A task or work item
- **Lane** = Priority level (experimental, standard, critical)
- **States**: Slung (queued) → Hooked (active) → Done (completed)

### CLI Commands Overview

```bash
# Show system status
cargo run --bin at -- status

# Create a new task (bead)
cargo run --bin at -- sling "Task description" --lane standard

# Start working on a task
cargo run --bin at -- hook <bead-id>

# Mark task complete
cargo run --bin at -- done <bead-id>

# Restart a stuck agent
cargo run --bin at -- nudge <agent-id>
```

### Example: Create and Execute Your First Task

**1. Check Status:**
```bash
cargo run --bin at -- status
```

**Expected Output:**
```
🟢 Auto-Tundra Status
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
API URL: http://localhost:9090
Daemon: Running
Active Beads: 0
Agents: 5 registered (Spec, QA, Build, Utility, Ideation)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**2. Create a Task:**
```bash
cargo run --bin at -- sling "Write a Rust function to parse JSON" --lane standard
```

**Output:**
```
✨ Created bead: bead_a1b2c3d4
   Title: Write a Rust function to parse JSON
   Lane: standard
   State: slung (queued)
```

**3. Hook (Start) the Task:**
```bash
cargo run --bin at -- hook bead_a1b2c3d4
```

**Output:**
```
🪝 Hooked bead: bead_a1b2c3d4
   Assigned to: agent_spec_001
   Status: Processing...
```

**4. Monitor Progress:**
```bash
cargo run --bin at -- status
```

**5. Mark Complete:**
```bash
cargo run --bin at -- done bead_a1b2c3d4
```

---

## 📝 Understanding Context Files

Auto-Tundra uses markdown files to steer agent behavior. These files are loaded by the **context engine** (`at-core::context_engine`).

### File Locations

```
/Users/studio/rust-harness/
├── AGENTS.md              # Project-level agent instructions (already exists!)
├── todo.md                # Task backlog (create if needed)
├── CLAUDE.md              # Claude-specific instructions (optional)
└── .claude/
    ├── agents/            # Custom agent definitions
    │   └── my-agent.md
    └── skills/            # Skill definitions
        └── my-skill/
            └── SKILL.md
```

### AGENTS.md (Project-Level Instructions)

**Already exists in your repo!** Located at `/Users/studio/rust-harness/AGENTS.md`

This file defines:
- Agent teams and responsibilities
- Crate ownership
- How to extend the system

**Key Sections:**
- Agent teams table (Core, Agents, Intelligence, etc.)
- Context steering (SKILL.md, todo.md usage)
- API profiles and multi-provider setup

### Creating Custom Agents

**Create `.claude/agents/code-reviewer.md`:**
```markdown
# Code Reviewer Agent

Role: code-reviewer
Capabilities: static analysis, security review, best practices

## Instructions

Review Rust code for:
1. Memory safety and ownership patterns
2. Error handling completeness
3. Security vulnerabilities
4. Performance optimizations
5. Idiomatic Rust usage

## Style

- Be constructive and specific
- Provide code examples for fixes
- Explain *why* changes improve code
```

**Load and use:**
```rust
use at_agents::registry::AgentRegistry;

let registry = AgentRegistry::load_from_project()?;
let reviewer = registry.get_agent("code-reviewer")?;
```

### Creating Skills

**Create `.claude/skills/rust-refactor/SKILL.md`:**
```markdown
---
name: rust-refactor
description: Refactor Rust code for clarity and performance
allowed_tools: [edit, read, analyze]
references: ["rust-book", "effective-rust"]
---

# Rust Refactoring Skill

Refactor Rust code following these principles:

## Zero-Cost Abstractions
- Use generics instead of dynamic dispatch where possible
- Leverage compile-time evaluation

## Ownership Patterns
- Minimize clones
- Use borrowing effectively
- Consider Cow<T> for read-heavy workloads

## Type Safety
- Leverage the type system for correctness
- Use NewType pattern for domain types
- Prefer compile-time checks over runtime
```

**Load and use:**
```rust
use at_agents::registry::AgentRegistry;

let registry = AgentRegistry::load_from_project()?;
let skill = registry.get_skill("rust-refactor")?;
```

---

## 🔧 Configuration

### Context Engine Settings

The context engine manages how much context is loaded based on token budgets.

**Default Settings:**
- **Token Budget**: Automatically calculated based on model limits
- **Progressive Disclosure**: Context is loaded incrementally as needed
- **BFS Traversal**: Related context is discovered via graph traversal

**Customize in code:**
```rust
use at_core::context_engine::ContextEngine;

let engine = ContextEngine::builder()
    .max_token_budget(8000)
    .enable_progressive_disclosure(true)
    .build()?;
```

### Multi-Provider Failover

Auto-Tundra automatically fails over between providers if one fails.

**Failover Order:**
1. Primary provider (first with API key set)
2. Secondary providers (in order of configuration)

**Example Failover:**
```
Anthropic (primary) → [Rate limited]
  ↓
OpenRouter (fallback) → [Success!]
```

**Check Which Provider is Active:**
```bash
# In your code
use at_intelligence::profile::ProfileRegistry;

let registry = ProfileRegistry::load()?;
let active = registry.current_profile();
println!("Using: {:?}", active.provider);
```

---

## 🐛 Troubleshooting

### "Error: API key not found"

**Problem:** No API keys configured.

**Solution:**
```bash
# Verify keys are set
env | grep API_KEY

# If not set:
export ANTHROPIC_API_KEY=your-key-here
```

### "Error: Connection refused (localhost:9090)"

**Problem:** Daemon not running.

**Solution:**
```bash
# Start the daemon (in separate terminal)
cargo run --bin at-daemon

# Then try your command again
cargo run --bin at -- status
```

### "Build failed: linking with `cc` failed"

**Problem:** Missing system dependencies.

**Solution (macOS):**
```bash
xcode-select --install
```

**Solution (Linux):**
```bash
sudo apt-get install build-essential libssl-dev pkg-config
```

### "Cargo nextest not found"

**Problem:** Test runner not installed.

**Solution:**
```bash
make nextest-install
# Or
cargo install cargo-nextest --locked
```

### Tests Failing

**Check:**
1. API keys are valid: `echo $ANTHROPIC_API_KEY`
2. Internet connection is working
3. All dependencies are up to date: `cargo update`

**Run with verbose output:**
```bash
RUST_LOG=debug make test
```

---

## ✅ Verification Checklist

Before moving forward, verify:

- [ ] Rust 1.91+ installed (`rustc --version`)
- [ ] At least one API key configured (`env | grep API_KEY`)
- [ ] Project builds successfully (`make build`)
- [ ] Tests pass (`make test`)
- [ ] CLI responds (`cargo run --bin at -- status`)
- [ ] Can create a bead (`cargo run --bin at -- sling "Test task"`)

**All checked?** You're ready to use Auto-Tundra! 🚀

---

## 📚 Next Steps

Now that you're set up:

1. **Learn the CLI** → [Project Handbook — CLI](docs/PROJECT_HANDBOOK.md#2-cli-guide) - Comprehensive command reference
2. **Understand Architecture** → [Project Handbook — Architecture](docs/PROJECT_HANDBOOK.md#1-architecture) - How the system works
3. **Contribute** → [CONTRIBUTING.md](CONTRIBUTING.md) - Add agents, skills, or features
4. **Explore Crates** → `make doc` then open `target/doc/at_core/index.html`

---

## 🆘 Still Stuck?

- Check existing **GitHub Issues**
- Review the **[Project Handbook](docs/PROJECT_HANDBOOK.md)** for system understanding
- Enable debug logging: `RUST_LOG=debug cargo run ...`
- Reach out to maintainers

---

**Ready to dive deeper?** → [Project Handbook — CLI](docs/PROJECT_HANDBOOK.md#2-cli-guide)
