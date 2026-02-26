TEST REPO! 
SHAMELESS COPY OF GAS TOWN 
DISGUISED AS A KANBAN BOARD IN RUST

USED FOR CICD MONITORING TESTING AND PR BOT DEMOS

# Auto-Tundra

> **A production-grade, multi-agent orchestration platform for Rust** with context-aware LLM integration, extensible agent/skill definitions, and multi-provider AI support.

---

## 🎯 What is Auto-Tundra?

Auto-Tundra is a Rust-based system that orchestrates AI agents to execute complex workflows. It uses a **"bead board"** task management metaphor backed by Dolt (versioned database) and integrates with multiple LLM providers (Anthropic, OpenRouter, OpenAI).

**Key Capabilities:**
- 🤖 **Multi-Agent Orchestration** - Specialized agents (Spec, QA, Build, Utility, Ideation) work together
- 🧠 **Context-Aware Intelligence** - Progressive context disclosure with token budget management
- 🔌 **Multi-Provider Support** - Anthropic Claude, OpenRouter, OpenAI with automatic failover
- 📝 **Markdown-Defined Extensibility** - Define agents and skills in simple markdown files
- 🏗️ **Production-Ready** - 1,483+ tests, CI/CD with Datadog, security scanning, comprehensive telemetry
- 🌐 **API-First** - HTTP/WebSocket bridge for external integrations

---

## ⚡ Quick Start (5 Minutes)

### Prerequisites
- **Rust 1.91+** (`rustup update`)
- **API Key** for at least one provider:
  - Anthropic: https://console.anthropic.com/settings/keys
  - OpenRouter: https://openrouter.ai/keys
  - OpenAI: https://platform.openai.com/api-keys

### Setup

```bash
# 1. Clone and navigate
cd /Users/studio/rust-harness

# 2. Set API key (choose one or more)
export ANTHROPIC_API_KEY=sk-ant-...
export OPENROUTER_API_KEY=sk-or-v1-...
export OPENAI_API_KEY=sk-...

# 3. Build the project
make build

# 4. Run tests to verify setup
make test

# 5. Check system status
cargo run --bin at -- status
```

**🎉 Success!** You're ready to orchestrate agents.

For detailed setup instructions, see **[GETTING_STARTED.md](GETTING_STARTED.md)**

---

## 📦 Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                          User / External Client                  │
└────────────────────────────┬────────────────────────────────────┘
                             │
                 ┌───────────┴──────────┐
                 │                      │
         ┌───────▼───────┐      ┌──────▼──────┐
         │    at-cli     │      │  at-bridge  │
         │   (Commands)  │      │  (HTTP/WS)  │
         └───────┬───────┘      └──────┬──────┘
                 │                     │
                 └──────────┬──────────┘
                            │
                    ┌───────▼───────┐
                    │   at-daemon   │
                    │ (Orchestrator)│
                    └───────┬───────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
   ┌────▼────┐      ┌───────▼───────┐   ┌──────▼─────┐
   │at-agents│      │at-intelligence│   │at-harness  │
   │(Roles & │      │  (LLM Calls)  │   │ (Providers)│
   │Executor)│      │               │   │            │
   └────┬────┘      └───────┬───────┘   └──────┬─────┘
        │                   │                   │
        └──────────┬────────┴──────────┬────────┘
                   │                   │
          ┌────────▼────────┐   ┌──────▼──────┐
          │    at-core      │   │ at-session  │
          │ (Types, Config) │   │   (PTY)     │
          └─────────────────┘   └─────────────┘
```

---

## 🏗️ Crate Organization

| Team | Crate(s) | Responsibility |
|------|----------|----------------|
| **Core** | `at-core` | Types, config, context engine, workflow DSL, health checks, project context loading (AGENTS.md, SKILL.md, todo.md, CLAUDE.md) |
| **Agents** | `at-agents` | Agent roles (Spec, QA, Build, Utility, Ideation), executor, prompts, registry, lifecycle, task runner, approval system |
| **Intelligence** | `at-intelligence` | LLM providers, model router, token cache, cost tracking, spec pipeline, insights, roadmap, ideation, memory management |
| **Bridge** | `at-bridge` | HTTP API, WebSocket server, task CRUD operations, settings management, credential handling |
| **Harness** | `at-harness` | Provider trait, rate limiting, circuit breaker, MCP protocol, shutdown handling, trace context, security |
| **Daemon** | `at-daemon` | Main orchestrator, task pipeline, event bus, daemon entry point |
| **Session** | `at-session` | PTY terminal management, terminal pool for command execution |
| **Integrations** | `at-integrations` | GitHub, GitLab, Linear API clients for external system integration |
| **Telemetry** | `at-telemetry` | Metrics, logging, tracing, Datadog integration |
| **CLI** | `at-cli` | Command-line interface (status, sling, hook, done, nudge commands) |
| **TUI** | `at-tui` | Terminal user interface (interactive mode) |
| **UI** | `app/tauri`, `app/leptos-ui` | Desktop app (Tauri) and web UI (Leptos) |

For detailed architecture, see **[Project Handbook](docs/PROJECT_HANDBOOK.md#1-architecture)**

---

## 🧠 Context-Driven Agents

Auto-Tundra uses markdown files to define agents, skills, and project context:

### AGENTS.md (Project-Level Agent Instructions)
```markdown
# My Custom Agent

Role: code-reviewer
Capabilities: static analysis, best practices, security review

## Instructions
Review code for quality, security, and maintainability.
Focus on Rust idioms and memory safety.
```

### SKILL.md (Skill Definitions)
```markdown
---
name: rust-refactor
description: Refactor Rust code for clarity and performance
allowed_tools: [edit, read, analyze]
---

Refactor Rust code following these principles:
- Zero-cost abstractions
- Prefer composition over inheritance
- Use type system for correctness
```

### Context Engine
The context engine (`at-core::context_engine`) loads these files and:
- Builds a context graph with progressive disclosure
- Manages token budgets for LLM calls
- Injects relevant context based on task requirements

---

## 🚀 Common Commands

```bash
# Development
make build              # Build all crates
make test               # Run tests with cargo-nextest
make clippy             # Lint with clippy
make fmt                # Format code
make doc                # Generate documentation

# CLI Usage
cargo run --bin at -- status                    # Show system status
cargo run --bin at -- sling "Fix bug #123"      # Create new task (bead)
cargo run --bin at -- hook <bead-id>            # Start working on task
cargo run --bin at -- done <bead-id>            # Mark task complete
cargo run --bin at -- nudge <agent-id>          # Restart stuck agent
cargo run --bin at -- smoke -p . -S             # Browser smoke (WebGPU + audio cues)

# Security & Quality
make deny               # Check dependencies for security issues
make ast-grep           # Run AST-based code analysis
make security           # Run all security checks

# CI/CD
make test-ci            # Run tests with JUnit output for Datadog
make ci                 # Full CI pipeline (test + upload)
```

See the **[CLI Guide](docs/PROJECT_HANDBOOK.md#2-cli-guide)** for comprehensive command reference.

---

## 📚 Documentation

**For Beginners:**
1. **[GETTING_STARTED.md](GETTING_STARTED.md)** - Detailed setup, first agent run, understanding context files
2. **[Project Handbook — CLI](docs/PROJECT_HANDBOOK.md#2-cli-guide)** - Command reference and usage patterns

**For Developers:**
3. **[Project Handbook — Architecture](docs/PROJECT_HANDBOOK.md#1-architecture)** - System design, crate interactions, data flows
4. **[CONTRIBUTING.md](CONTRIBUTING.md)** - Development setup, testing, PR workflow, adding agents/skills
5. **[AGENTS.md](AGENTS.md)** - Agent teams, crate ownership, context steering

**Technical Docs:**
- `docs/plans/` - Design documents and implementation plans
- `make doc` - Generate Rust API documentation

---

## 🧪 Testing

```bash
# Local development (fast)
make test

# CI-style with JUnit output
make test-ci

# Release validation (strict, retry flaky tests 3x)
make test-release

# Run specific crate tests
cargo nextest run -p at-core
cargo nextest run -p at-agents
```

**Test Coverage:**
- 1,483+ tests across the workspace
- Unit tests in each crate
- Integration tests for cross-crate functionality
- Doc tests for API examples

---

## 🔧 Development Tools Required

Install these for full development experience:

```bash
# Required
rustup update                              # Rust 1.91+
make nextest-install                       # Fast test runner

# Recommended
make deny-install                          # Dependency security checking
make ast-grep-install                      # AST-based code analysis
make dd-ci-install                         # Datadog CI uploads (optional)
```

---

## 🔐 Security

Auto-Tundra follows security best practices:

✅ **Dependency Scanning** - `cargo-deny` checks for vulnerabilities
✅ **Static Analysis** - ast-grep rules for common security issues
✅ **API Key Management** - Environment variables only, never committed
✅ **Rate Limiting** - Built-in request throttling
✅ **Circuit Breakers** - Prevent cascade failures
✅ **Input Validation** - Tool call firewall, sanitization

See `deny.toml` and `.ast-grep.yml` for security configurations.

---

## 🤝 Contributing

We welcome contributions! Please see **[CONTRIBUTING.md](CONTRIBUTING.md)** for:
- Development environment setup
- Code style and linting requirements
- Testing guidelines
- PR submission process
- How to add new agents and skills

**Quick Contribution Checklist:**
- [ ] Run `make fmt` before committing
- [ ] Run `make clippy` and fix warnings
- [ ] Run `make test` and ensure all tests pass
- [ ] Run `make security` for security checks
- [ ] Update documentation for new features
- [ ] Add tests for new functionality

---

## 📊 Monitoring & Observability

**Datadog Integration:**
- JUnit test results upload via `datadog-ci`
- Metrics and traces via `at-telemetry`
- Custom dashboards for agent performance

**Local Monitoring:**
```bash
cargo run --bin at -- status    # System health
RUST_LOG=debug cargo run ...    # Debug logging
```

---

## 🔗 Multi-Provider AI Support

Auto-Tundra supports multiple LLM providers with automatic failover:

| Provider | Models | Setup |
|----------|--------|-------|
| **Anthropic** | Claude 3/4 family | `export ANTHROPIC_API_KEY=sk-ant-...` |
| **OpenRouter** | 100+ models | `export OPENROUTER_API_KEY=sk-or-v1-...` |
| **OpenAI** | GPT-3.5/4 | `export OPENAI_API_KEY=sk-...` |

Failover is automatic - if one provider fails, the system tries the next configured provider.

---

## 🎯 Use Cases

- **Code Review Automation** - QA agents review PRs for quality and security
- **Spec Generation** - Spec agents create detailed technical specifications
- **Build Orchestration** - Build agents manage complex build pipelines
- **Ideation & Planning** - Ideation agents generate and refine project ideas
- **Multi-Agent Workflows** - Coordinate multiple specialized agents for complex tasks

---

## 📜 License

MIT OR Apache-2.0 (dual license)

---

## 🚦 Project Status

**Current Version:** 0.1.0
**Rust Version:** 1.91+
**Test Count:** 1,483+
**Production Ready:** ✅ (with comprehensive CI/CD and monitoring)

---

## 🆘 Getting Help

- 📖 **Documentation** - Start with [GETTING_STARTED.md](GETTING_STARTED.md)
- 🐛 **Issues** - Check existing issues or create a new one
- 💬 **Discussions** - Ask questions and share ideas
- 📧 **Contact** - Reach out to maintainers

---

**Ready to get started?** → [GETTING_STARTED.md](GETTING_STARTED.md)
