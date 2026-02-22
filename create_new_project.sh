#!/bin/bash

# Auto-Tundra Project Creator
# Creates a new Auto-Tundra project with proper structure and configuration

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if project name is provided
if [ -z "$1" ]; then
    echo "Usage: $0 <project-name> [directory]"
    echo "Example: $0 my-awesome-project"
    echo "Example: $0 my-awesome-project /path/to/projects"
    exit 1
fi

PROJECT_NAME="$1"
PROJECT_DIR="${2:-$(pwd)/$PROJECT_NAME}"

print_status "Creating new Auto-Tundra project: $PROJECT_NAME"
print_status "Project directory: $PROJECT_DIR"

# Check if directory already exists
if [ -d "$PROJECT_DIR" ]; then
    print_error "Directory $PROJECT_DIR already exists!"
    exit 1
fi

# Create project directory
mkdir -p "$PROJECT_DIR"
cd "$PROJECT_DIR"

print_status "Setting up project structure..."

# Create basic Auto-Tundra project structure
mkdir -p .claude/{agents,skills,prompts}
mkdir -p .claude/skills/core
mkdir -p .claude/skills/custom
mkdir -p docs/{plans,research}
mkdir -p tests/{unit,integration,e2e}
mkdir -p scripts
mkdir -p config

# Create project configuration files
print_status "Creating configuration files..."

# Create Cargo.toml for the project
cat > Cargo.toml << 'EOF'
[package]
name = "{{PROJECT_NAME}}"
version = "0.1.0"
edition = "2021"
description = "Auto-Tundra AI agent orchestration project"
authors = ["Your Name <your.email@example.com>"]
license = "MIT"

[dependencies]
# Auto-Tundra core dependencies
tokio = { version = "1.0", features = ["full"] }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# LLM providers
anthropic = "0.1"
openai = "1.0"

# Auto-Tundra workspace dependencies (if using local workspace)
# at-core = { path = "../rust-harness/crates/at-core" }
# at-agents = { path = "../rust-harness/crates/at-agents" }

[dev-dependencies]
tokio-test = "0.4"

[[bin]]
name = "main"
path = "src/main.rs"
EOF

# Replace placeholder with actual project name
sed -i.bak "s/{{PROJECT_NAME}}/$PROJECT_NAME/g" Cargo.toml
rm Cargo.toml.bak

# Create README.md
cat > README.md << EOF
# $PROJECT_NAME

> Auto-Tundra AI agent orchestration project

## 🎯 Project Overview

This project uses Auto-Tundra to orchestrate AI agents for intelligent task execution.

## 🚀 Quick Start

### Prerequisites

1. **Rust 1.91+**
   \`\`\`bash
   rustup update
   \`\`\`

2. **API Keys** - Set at least one:
   \`\`\`bash
   export ANTHROPIC_API_KEY="your-key-here"
   # or
   export OPENROUTER_API_KEY="your-key-here"
   # or  
   export OPENAI_API_KEY="your-key-here"
   \`\`\`

### Running the Project

\`\`\`bash
# Build and run
cargo run

# Or with Auto-Tundra CLI (if installed)
at run --task "your task description" --project-path .
\`\`\`

## 📁 Project Structure

\`\`\`
$PROJECT_NAME/
├── .claude/
│   ├── agents/          # Agent definitions
│   ├── skills/          # Skill definitions  
│   └── prompts/         # Custom prompts
├── docs/                # Documentation
├── tests/               # Test suites
├── scripts/             # Utility scripts
├── config/              # Configuration files
├── src/                 # Source code
└── Cargo.toml          # Rust project file
\`\`\`

## 🤖 Available Skills

Run \`at skill list\` to see available skills.

## 📚 Documentation

- [Auto-Tundra Handbook](docs/PROJECT_HANDBOOK.md)
- [Getting Started](docs/GETTING_STARTED.md)
- [API Reference](docs/API_REFERENCE.md)

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests
5. Submit a pull request

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.
EOF

# Create main source directory and files
mkdir -p src

# Create main.rs
cat > src/main.rs << 'EOF'
use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🚀 Starting Auto-Tundra project");

    // Your Auto-Tundra logic will go here
    info!("✅ Auto-Tundra project running successfully!");

    Ok(())
}
EOF

# Create AGENTS.md for project-specific agent instructions
cat > AGENTS.md << EOF
# Project-Specific Agent Instructions

This file contains project-level instructions for Auto-Tundra agents.

## Project Context

- **Project Name**: $PROJECT_NAME
- **Purpose**: [Describe your project's purpose]
- **Tech Stack**: Rust, Auto-Tundra
- **Key Components**: [List main components]

## Agent Guidelines

### General Behavior
- Be helpful and proactive
- Ask clarifying questions when needed
- Provide code examples when relevant
- Follow Rust best practices

### Code Style
- Use \`cargo fmt\` for formatting
- Follow Rust naming conventions
- Add appropriate comments and documentation
- Include error handling

### Testing
- Write unit tests for new functionality
- Use \`cargo test\` to verify
- Consider integration tests for complex features

## Project-Specific Rules

[Add any project-specific rules or guidelines here]

## Available Skills

[Document available skills and their usage]
EOF

# Create a sample skill
mkdir -p ".claude/skills/core/project-setup"
cat > ".claude/skills/core/project-setup/SKILL.md" << 'EOF'
---
name: project-setup
description: Set up and configure Auto-Tundra project components
allowed_tools: ["read_file", "write_to_file", "bash", "list_dir"]
references:
  - "Auto-Tundra documentation"
  - "Project README"
---

# Project Setup Skill

Helps with setting up and configuring Auto-Tundra project components including:

- Initial project structure
- Configuration files
- Agent definitions
- Skill creation
- Testing setup

## Usage

Use this skill when you need to:
- Create new project components
- Set up configuration
- Initialize testing framework
- Add new agents or skills

## Commands

- \`setup-project\` - Initialize project structure
- \`add-skill <name>\` - Create a new skill
- \`add-agent <name>\` - Create a new agent
- \`setup-testing\` - Initialize test framework
EOF

# Create .gitignore
cat > .gitignore << 'EOF'
# Rust
/target/
**/*.rs.bk
Cargo.lock

# IDE
.vscode/
.idea/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db

# Auto-Tundra
.claude/local/
.claude/cache/
beads/

# Environment
.env
.env.local
.env.*.local

# Logs
*.log
logs/

# API Keys (should be in environment, not files)
*.key
.api_keys

# Temporary files
tmp/
temp/
*.tmp
EOF

# Create basic Makefile
cat > Makefile << 'EOF'
.PHONY: help build run test clean fmt lint

help: ## Show this help message
	@echo "Available commands:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build the project
	cargo build

run: ## Run the project
	cargo run

test: ## Run tests
	cargo test

clean: ## Clean build artifacts
	cargo clean

fmt: ## Format code
	cargo fmt

lint: ## Run linter
	cargo clippy

check: ## Run all checks
	cargo check
	cargo test
	cargo clippy
EOF

# Create initial documentation
mkdir -p docs
cat > docs/GETTING_STARTED.md << EOF
# Getting Started with $PROJECT_NAME

This guide will help you get started with your new Auto-Tundra project.

## Prerequisites

1. **Rust 1.91+**
2. **API Key** (Anthropic, OpenRouter, or OpenAI)

## Setup

1. Clone or navigate to your project directory
2. Set your API key as an environment variable
3. Run the project

\`\`\`bash
export ANTHROPIC_API_KEY="your-key"
cargo run
\`\`\`

## Next Steps

1. Read the [Project Handbook](PROJECT_HANDBOOK.md)
2. Explore available skills with \`at skill list\`
3. Create your first task with \`at run\`
4. Customize agents and skills for your needs

## Support

Refer to the main Auto-Tundra documentation for detailed guidance.
EOF

cat > docs/PROJECT_HANDBOOK.md << EOF
# $PROJECT_NAME Project Handbook

## Overview

This handbook contains project-specific information and guidelines.

## Architecture

[Describe your project architecture]

## Components

[List and describe main components]

## Development Workflow

1. Feature development
2. Testing
3. Code review
4. Deployment

## Guidelines

- Code style
- Testing requirements
- Documentation standards
- Release process

## Resources

- [Auto-Tundra Documentation](https://docs.auto-tundra.com)
- [Rust Documentation](https://doc.rust-lang.org)
- [Project Repository](https://github.com/your-repo/$PROJECT_NAME)
EOF

# Initialize git repository
print_status "Initializing git repository..."
git init

# Create initial commit
git add .
git commit -m "feat: initial Auto-Tundra project setup

- Project structure and configuration
- Basic Rust application scaffold
- Sample skill and agent documentation
- Project handbook and getting started guide
- Development tools configuration (Makefile, .gitignore)"

print_success "✅ Auto-Tundra project '$PROJECT_NAME' created successfully!"
print_status "Project location: $PROJECT_DIR"

echo ""
print_status "Next steps:"
echo "1. cd $PROJECT_DIR"
echo "2. Set your API key: export ANTHROPIC_API_KEY=\"your-key\""
echo "3. Run the project: cargo run"
echo "4. Explore skills: at skill list --project-path ."
echo "5. Create your first task: at run --task \"your task\" --project-path ."
echo ""
print_success "Happy coding with Auto-Tundra! 🚀"
