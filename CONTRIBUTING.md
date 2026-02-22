# Contributing to Auto-Tundra

**[← Back to README](README.md)**

Thank you for contributing! This guide covers development setup, coding standards, testing, and the PR workflow.

---

## 📋 Table of Contents

1. [Development Setup](#development-setup)
2. [Code Style](#code-style)
3. [Testing Requirements](#testing-requirements)
4. [Adding Features](#adding-features)
5. [Pull Request Process](#pull-request-process)
6. [Security](#security)

---

## 🛠️ Development Setup

### Prerequisites

- **Rust 1.91+** - `rustup update`
- **Git** - Version control
- **API Keys** - At least one (Anthropic, OpenRouter, or OpenAI)

### Install Development Tools

```bash
# Fast test runner (essential)
make nextest-install

# Dependency security scanner
make deny-install

# Static code analyzer
make ast-grep-install

# Datadog CI (optional)
make dd-ci-install
```

### Clone and Build

```bash
# Clone the repository
git clone <your-fork-url>
cd rust-harness

# Set up API keys
export ANTHROPIC_API_KEY=your-key-here

# Build the workspace
make build

# Run tests to verify setup
make test
```

### Development Workflow

```bash
# Create a feature branch
git checkout -b feature/my-feature

# Make changes, then format
make fmt

# Check for issues
make clippy

# Run tests
make test

# Run security checks
make security

# Commit and push
git add .
git commit -m "feat: add my feature"
git push origin feature/my-feature
```

---

## 🎨 Code Style

### Formatting

**Always run before committing:**
```bash
make fmt
```

Auto-Tundra uses standard `rustfmt` configuration.

### Linting

**Zero clippy warnings required:**
```bash
make clippy
```

Fix all warnings before submitting PR.

### Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Crates | `at-<name>` | `at-core`, `at-agents` |
| Modules | `snake_case` | `context_engine`, `rate_limiter` |
| Types | `PascalCase` | `AgentRole`, `BeadState` |
| Functions | `snake_case` | `load_context()`, `execute_task()` |
| Constants | `SCREAMING_SNAKE` | `MAX_RETRIES`, `DEFAULT_TIMEOUT` |

### Module Organization

```rust
// Imports
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// Type definitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyType { ... }

// Implementation
impl MyType {
    pub fn new() -> Self { ... }

    // Public methods first
    pub fn public_method(&self) { ... }

    // Private methods last
    fn private_method(&self) { ... }
}

// Tests at bottom
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() { ... }
}
```

### Documentation

**Public APIs must have doc comments:**
```rust
/// Creates a new agent with the specified role.
///
/// # Arguments
///
/// * `role` - The agent's role (Spec, QA, Build, etc.)
/// * `config` - Configuration for the agent
///
/// # Example
///
/// ```
/// use at_agents::{Agent, AgentRole};
///
/// let agent = Agent::new(AgentRole::Spec, config)?;
/// ```
///
/// # Errors
///
/// Returns `Err` if the role is invalid or config is incomplete.
pub fn new(role: AgentRole, config: Config) -> Result<Self> {
    // Implementation
}
```

---

## 🧪 Testing Requirements

### Test Coverage

Auto-Tundra has **1,483+ tests** across the workspace. New code must include tests.

### Running Tests

```bash
# Run all tests (fast with nextest)
make test

# Run specific crate tests
cargo nextest run -p at-core
cargo nextest run -p at-agents

# Run tests with coverage
cargo nextest run --coverage

# Run doc tests
cargo test --doc
```

### Writing Tests

**Unit Tests:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_creation() {
        let agent = Agent::new(AgentRole::Spec).unwrap();
        assert_eq!(agent.role(), AgentRole::Spec);
    }

    #[tokio::test]
    async fn test_async_operation() {
        let result = async_function().await;
        assert!(result.is_ok());
    }
}
```

**Integration Tests:**
```rust
// tests/integration_test.rs
use at_core::context_engine::ContextEngine;

#[tokio::test]
async fn test_full_workflow() {
    let engine = ContextEngine::load_from_project(".").unwrap();
    let context = engine.collect_context("task-1", 8000).unwrap();
    assert!(!context.is_empty());
}
```

### Test Organization

| Location | Purpose |
|----------|---------|
| `src/module.rs` → `#[cfg(test)] mod tests` | Unit tests |
| `tests/` | Integration tests |
| Doc comments → `/// # Example` | Doc tests |

### CI Test Profile

```bash
# CI runs tests with strict settings
make test-ci

# Generates JUnit XML for Datadog
# Output: target/nextest/ci/ci-junit.xml
```

---

## ✨ Adding Features

### Adding a New Agent Role

**1. Define in `at-core/src/types.rs`:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentRole {
    Spec,
    QA,
    Build,
    Utility,
    Ideation,
    YourNewRole, // Add here
}
```

**2. Create role module in `at-agents/src/roles/`:**
```rust
// at-agents/src/roles/your_new_role.rs
use at_core::types::{Message, Tool};

pub struct YourNewRole;

impl RoleConfig for YourNewRole {
    fn system_prompt(&self) -> String {
        "You are a specialized agent for...".to_string()
    }

    fn tools(&self) -> Vec<Tool> {
        vec![
            // Define tools this agent can use
        ]
    }

    fn context_requirements(&self) -> Vec<String> {
        vec!["specific-context-needed".to_string()]
    }
}
```

**3. Register in `at-agents/src/registry.rs`:**
```rust
impl AgentRegistry {
    pub fn default() -> Self {
        let mut registry = Self::new();
        // ... existing roles ...
        registry.register_role(
            AgentRole::YourNewRole,
            Box::new(YourNewRole),
        );
        registry
    }
}
```

**4. Add tests:**
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_your_new_role() {
        let role = YourNewRole;
        assert!(!role.system_prompt().is_empty());
        assert!(!role.tools().is_empty());
    }
}
```

**5. Update documentation:**
- Add to README.md agent list
- Document in [Project Handbook](docs/PROJECT_HANDBOOK.md)
- Add usage example

### Adding a Markdown-Defined Agent

**Create `.claude/agents/custom-agent.md`:**
```markdown
# Custom Agent Name

Role: custom-role
Capabilities: capability1, capability2, capability3

## Instructions

Detailed instructions for this agent's behavior.

## Examples

Show examples of tasks this agent handles.
```

**No code changes needed!** The registry auto-loads from `.claude/agents/`.

### Adding a New Skill

**Create `.claude/skills/my-skill/SKILL.md`:**
```markdown
---
name: my-skill
description: Brief description of what this skill does
allowed_tools: [tool1, tool2, tool3]
references: ["external-doc", "api-reference"]
---

# My Skill

## Purpose

Explain what this skill accomplishes.

## Usage

Provide usage instructions and examples.

## Best Practices

List best practices for using this skill effectively.
```

### Adding a New Crate

```bash
# Create crate
cargo new --lib crates/at-mynewcrate

# Add to workspace Cargo.toml
[workspace]
members = [
    # ... existing ...
    "crates/at-mynewcrate",
]

# Add dependencies
cd crates/at-mynewcrate
cargo add tokio --features full
cargo add at-core --path ../at-core

# Implement, test, document
```

---

## 🔒 Security

### Security Scanning

**Always run before submitting PR:**
```bash
make security
```

This runs:
1. `cargo deny` - Dependency vulnerability scan
2. `ast-grep` - AST-based static code analysis

### Security Checklist

- [ ] No hardcoded API keys or secrets
- [ ] API keys from environment variables only
- [ ] No `.env` files committed
- [ ] Input validation for user data
- [ ] No SQL injection vulnerabilities
- [ ] No command injection risks
- [ ] Proper error handling (don't leak sensitive info)

### Security Vulnerabilities

**Found a security issue?**

Do NOT create a public GitHub issue. Instead:
1. Email maintainers privately
2. Include:
   - Description of vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

---

## 📝 Pull Request Process

### Before Submitting

**Checklist:**
- [ ] Run `make fmt` (code formatted)
- [ ] Run `make clippy` (no warnings)
- [ ] Run `make test` (all tests pass)
- [ ] Run `make security` (security checks pass)
- [ ] Add tests for new features
- [ ] Update documentation (README, ARCHITECTURE, etc.)
- [ ] Commit messages follow convention

### Commit Message Convention

We use conventional commits:

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Formatting, missing semicolons, etc.
- `refactor`: Code restructuring
- `test`: Adding tests
- `chore`: Maintenance tasks

**Examples:**
```
feat(at-agents): add Security role for vulnerability scanning

Implements a new Security agent role that scans code for
common vulnerabilities and security best practices.

Closes #123
```

```
fix(at-intelligence): resolve rate limiting bug in failover

The failover logic wasn't properly handling rate limit errors,
causing unnecessary retries instead of switching providers.
```

### PR Description Template

```markdown
## Description

Brief description of changes.

## Motivation

Why is this change necessary?

## Changes Made

- Change 1
- Change 2
- Change 3

## Testing

How was this tested?
- [ ] Unit tests added
- [ ] Integration tests added
- [ ] Manual testing performed

## Checklist

- [ ] Code formatted (`make fmt`)
- [ ] Clippy clean (`make clippy`)
- [ ] Tests pass (`make test`)
- [ ] Security checks pass (`make security`)
- [ ] Documentation updated
```

### Review Process

1. **Submit PR** - Create PR from your fork
2. **CI Checks** - Automated tests must pass
3. **Code Review** - Maintainer reviews code
4. **Address Feedback** - Make requested changes
5. **Approval** - PR approved by maintainer
6. **Merge** - Maintainer merges PR

---

## 🧩 Common Tasks

### Adding a New CLI Command

**1. Define in `at-cli/src/main.rs`:**
```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing ...
    MyCommand {
        #[arg(long)]
        my_arg: String,
    },
}
```

**2. Create handler in `at-cli/src/commands/my_command.rs`:**
```rust
pub async fn run(api_url: &str, my_arg: &str) -> anyhow::Result<()> {
    // Implementation
    Ok(())
}
```

**3. Wire up in `main()`:**
```rust
Some(Commands::MyCommand { my_arg }) => {
    commands::my_command::run(&api_url, &my_arg).await?;
}
```

**4. Test:**
```bash
cargo run --bin at -- my-command --my-arg "test"
```

### Adding a New API Endpoint

**1. Define route in `at-bridge/src/lib.rs`:**
```rust
use axum::{Router, routing::post};

let app = Router::new()
    .route("/api/my-endpoint", post(handlers::my_handler));
```

**2. Implement handler:**
```rust
async fn my_handler(
    Json(payload): Json<MyRequest>,
) -> Result<Json<MyResponse>, StatusCode> {
    // Implementation
    Ok(Json(response))
}
```

**3. Add tests:**
```rust
#[tokio::test]
async fn test_my_endpoint() {
    let response = client
        .post("/api/my-endpoint")
        .json(&request)
        .send()
        .await?;
    assert_eq!(response.status(), 200);
}
```

---

## 📊 CI/CD

### GitHub Actions (if configured)

```yaml
# .github/workflows/ci.yml
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
      - run: make test-ci
      - run: make security
```

### Datadog Integration

```bash
# Upload test results to Datadog
export DATADOG_API_KEY=your-key
export DD_ENV=ci
make dd-upload
```

---

## 🏆 Recognition

Contributors are recognized in:
- README.md contributors section
- Release notes
- Project website (if applicable)

---

## 📚 Resources

- **[Project Handbook](docs/PROJECT_HANDBOOK.md)** - Architecture, CLI, research, observability
- **[Project Handbook — CLI](docs/PROJECT_HANDBOOK.md#2-cli-guide)** - CLI command reference
- **[GETTING_STARTED.md](GETTING_STARTED.md)** - Setup guide
- **Rust Book** - https://doc.rust-lang.org/book/
- **Tokio Docs** - https://tokio.rs/

---

## 🤝 Code of Conduct

- Be respectful and inclusive
- Provide constructive feedback
- Focus on what is best for the project
- Show empathy towards other contributors

---

**Ready to contribute?** Start with small PRs to get familiar with the process!

**Questions?** Open a discussion on GitHub or contact maintainers.

---

**Happy coding!** 🚀
