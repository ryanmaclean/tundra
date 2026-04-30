# Setting Up a New Auto-Tundra Project

## 🎯 Understanding Auto-Tundra

Auto-Tundra is **not** a project generator like `cargo new`. It's a **multi-agent orchestration platform** that runs as a service and helps you manage AI-powered workflows.

## 🚀 Two Ways to Use Auto-Tundra

### Method 1: Fork/Clone the Main Repository (Recommended)

```bash
# Clone the main Auto-Tundra codebase
git clone https://github.com/your-org/rust-harness.git my-autotundra-project
cd my-autotundra-project

# Set up your API keys
export ANTHROPIC_API_KEY="your-key-here"
# or
export OPENROUTER_API_KEY="your-key-here"

# Build and start the daemon
cargo build --release --bin at-daemon
./target/release/at-daemon

# In another terminal, use the CLI to create tasks
cargo build --release --bin at-cli
./target/release/at-cli run --task "Build a REST API with Rust" --project-path .
```

### Method 2: Create a Simple Rust Project with Auto-Tundra Client

```bash
# Create a new Rust project
cargo new my-project
cd my-project

# Add Auto-Tundra as a dependency (if published)
# Or use local path for now
# Edit Cargo.toml:
[dependencies]
tokio = { version = "1.0", features = ["full"] }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
reqwest = { version = "0.11", features = ["json"] }

# Create src/main.rs with Auto-Tundra client code
```

## 🏗️ Auto-Tundra Project Structure

When working with Auto-Tundra, your project should have:

```
my-project/
├── .claude/
│   ├── agents/          # Agent definitions (.md files)
│   ├── skills/          # Skill definitions (SKILL.md files)
│   └── prompts/         # Custom prompts
├── AGENTS.md            # Project-level agent instructions
├── SKILL.md            # Project-level skill definitions
├── README.md           # Project documentation
└── src/                # Your Rust code (if any)
```

## 🤖 Creating Your First Auto-Tundra Workflow

### Step 1: Set Up Environment

```bash
# Required: At least one LLM provider API key
export ANTHROPIC_API_KEY="sk-ant-..."
# or
export OPENROUTER_API_KEY="sk-or-..."
# or
export OPENAI_API_KEY="sk-..."
```

### Step 2: Define Skills (Optional)

Create `.claude/skills/my-skill/SKILL.md`:

```markdown
---
name: my-skill
description: Custom skill for my project
allowed_tools: ["read_file", "write_to_file", "bash"]
references:
  - "Project README"
---

# My Custom Skill

This skill helps with [specific task].

## Usage

Use this skill when you need to [describe use case].
```

### Step 3: Start Auto-Tundra Daemon

```bash
# From the Auto-Tundra repository
cargo run --bin at-daemon

# Daemon will start on:
# - API: http://localhost:9090
# - Frontend: http://localhost:3001
```

### Step 4: Create and Execute Tasks

```bash
# Create a task
./target/release/at-cli run \
  --task "Build a web server with authentication" \
  --skill web-development \
  --project-path .

# Or use role-specific agents
./target/release/at-cli agent run \
  --role builder \
  --task "Implement the API endpoints" \
  --project-path .
```

## 🎯 Common Use Cases

### 1. Code Development
```bash
at run --task "Add user authentication to the web app" --skill auth
```

### 2. Documentation
```bash
at run --task "Write API documentation for the endpoints" --skill docs
```

### 3. Testing
```bash
at run --task "Create unit tests for the user service" --skill testing
```

### 4. Code Review
```bash
at agent run --role qa-reviewer --task "Review the latest PR changes"
```

## 🔧 Configuration

Auto-Tundra looks for configuration in:

1. `~/.auto-tundra/config.toml` (global)
2. `.auto-tundra/config.toml` (project-local)
3. Environment variables

### Example Config

```toml
[general]
default_provider = "anthropic"

[providers.anthropic]
model = "claude-3-5-sonnet-20241022"

[agents]
enabled = ["spec", "builder", "qa-reviewer"]

[daemon]
api_port = 9090
frontend_port = 3001
```

## 🚀 Next Steps

1. **Explore Skills**: `at skill list --project-path .`
2. **Check Status**: `at status`
3. **Run Doctor**: `at doctor --project-path .`
4. **View Dashboard**: Open http://localhost:3001

## 💡 Key Concepts

- **Beads**: Tasks that flow through the system (slung → hooked → done)
- **Agents**: Specialized AI workers (spec, builder, qa, etc.)
- **Skills**: Reusable capabilities defined in markdown
- **Context**: Progressive disclosure of relevant information

## 📚 Learn More

- [Auto-Tundra Handbook](docs/PROJECT_HANDBOOK.md)
- [Getting Started Guide](GETTING_STARTED.md)
- [API Documentation](docs/API_REFERENCE.md)
