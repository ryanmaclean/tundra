# Configuration Reference

**[← Back to README](../README.md)** | **[Getting Started](../GETTING_STARTED.md)** | **[Project Handbook](PROJECT_HANDBOOK.md)**

> Comprehensive reference for all environment variables and configuration options in Auto-Tundra.

---

## 📋 Table of Contents

1. [Environment Variables](#1-environment-variables)
   - [LLM Provider API Keys](#11-llm-provider-api-keys)
   - [Integration Services](#12-integration-services)
   - [Auto-Tundra System](#13-auto-tundra-system)
   - [Datadog Observability](#14-datadog-observability)
   - [Rust & System Configuration](#15-rust--system-configuration)
2. [Configuration File](#2-configuration-file)
3. [Security Best Practices](#3-security-best-practices)

---

# 1. Environment Variables

Auto-Tundra uses environment variables for **all sensitive credentials** (API keys, tokens, secrets). Configuration settings that are not sensitive are stored in `~/.auto-tundra/config.toml`.

**⚠️ NEVER commit API keys or tokens to version control!**

## 1.1 LLM Provider API Keys

Auto-Tundra supports multiple LLM providers for agent execution. You need **at least one** provider configured.

### ANTHROPIC_API_KEY

Anthropic Claude API key for Claude 3/4 models.

| Property | Value |
|----------|-------|
| **Required** | No (but recommended for production) |
| **Default** | None |
| **Provider** | Anthropic |
| **Get Key** | https://console.anthropic.com/settings/keys |
| **Example** | `sk-ant-api03-...` |

**Usage:**
```bash
export ANTHROPIC_API_KEY=sk-ant-api03-your-key-here
```

**Notes:**
- Highest quality for complex reasoning tasks
- Supports streaming, tool use, and artifacts
- No free tier, but cost-effective per token
- Default model: `claude-3-5-sonnet-20241022`

### OPENROUTER_API_KEY

OpenRouter API key for access to 100+ models through a unified API.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | None |
| **Provider** | OpenRouter |
| **Get Key** | https://openrouter.ai/keys |
| **Example** | `sk-or-v1-...` |

**Usage:**
```bash
export OPENROUTER_API_KEY=sk-or-v1-your-key-here
```

**Notes:**
- Free tier: 100 requests/day
- Access to multiple model providers (Anthropic, OpenAI, Meta, Google, etc.)
- Best for experimentation and testing
- Default model: `anthropic/claude-3.5-sonnet`

### OPENAI_API_KEY

OpenAI API key for GPT-3.5/4 models.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | None |
| **Provider** | OpenAI |
| **Get Key** | https://platform.openai.com/api-keys |
| **Example** | `sk-...` |

**Usage:**
```bash
export OPENAI_API_KEY=sk-your-key-here
```

**Notes:**
- Supports GPT-3.5 Turbo, GPT-4, GPT-4 Turbo
- $5 free credit for new accounts
- Default model: `gpt-4-turbo-preview`

### LOCAL_API_KEY

Optional API key for local inference servers (vllm.rs, Ollama, llama.cpp, candle).

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | None |
| **Provider** | Local (localhost) |
| **Example** | `local-test-key` (often not required) |

**Usage:**
```bash
export LOCAL_API_KEY=your-local-key-here
```

**Notes:**
- Most local servers don't require an API key
- Default base URL: `http://127.0.0.1:11434`
- Supports OpenAI-compatible chat completions protocol
- Offline inference for privacy-sensitive workflows

## 1.2 Integration Services

Auto-Tundra integrates with project management and version control platforms.

### GITHUB_TOKEN

GitHub personal access token for repository operations.

| Property | Value |
|----------|-------|
| **Required** | No (only if using GitHub integration) |
| **Default** | None |
| **Scope Required** | `repo`, `read:org` |
| **Get Token** | https://github.com/settings/tokens |
| **Example** | `ghp_...` |

**Usage:**
```bash
export GITHUB_TOKEN=ghp_your-token-here
```

**Notes:**
- Used for PR reviews, issue management, and repository operations
- Fine-grained tokens recommended over classic tokens
- Config setting: `integrations.github_token_env = "GITHUB_TOKEN"`

### GITLAB_TOKEN

GitLab personal access token for merge request operations.

| Property | Value |
|----------|-------|
| **Required** | No (only if using GitLab integration) |
| **Default** | None |
| **Scope Required** | `api`, `read_repository` |
| **Get Token** | https://gitlab.com/-/profile/personal_access_tokens |
| **Example** | `glpat-...` |

**Usage:**
```bash
export GITLAB_TOKEN=glpat-your-token-here
```

**Notes:**
- Used for MR reviews and repository operations
- Config setting: `integrations.gitlab_token_env = "GITLAB_TOKEN"`

### LINEAR_API_KEY

Linear API key for issue tracking integration.

| Property | Value |
|----------|-------|
| **Required** | No (only if using Linear integration) |
| **Default** | None |
| **Get Key** | https://linear.app/settings/api |
| **Example** | `lin_api_...` |

**Usage:**
```bash
export LINEAR_API_KEY=lin_api_your-key-here
```

**Notes:**
- Used for issue creation, updates, and status tracking
- Config setting: `integrations.linear_api_key_env = "LINEAR_API_KEY"`

## 1.3 Auto-Tundra System

### AUTO_TUNDRA_API_KEY

API key for authenticating with the Auto-Tundra HTTP/WebSocket bridge API.

| Property | Value |
|----------|-------|
| **Required** | No (only for remote API access) |
| **Default** | None |
| **Example** | `at-api-...` |

**Usage:**
```bash
export AUTO_TUNDRA_API_KEY=at-api-your-key-here
```

**Notes:**
- Used when accessing Auto-Tundra daemon via HTTP/WebSocket API
- Not required for local CLI usage
- Secure API access for external clients

### AT_LOCAL_LLM_MAX_CONCURRENT

Maximum concurrent requests to local LLM inference servers.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `2` |
| **Valid Range** | 1-10 |
| **Example** | `4` |

**Usage:**
```bash
export AT_LOCAL_LLM_MAX_CONCURRENT=4
```

**Notes:**
- Prevents overloading local inference servers
- Adjust based on available GPU/CPU resources

## 1.4 Datadog Observability

Auto-Tundra provides comprehensive Datadog integration for metrics, traces, logs, and profiling.

### DD_SERVICE

Datadog service name for identifying your Auto-Tundra instance.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `at-daemon` |
| **Example** | `auto-tundra-prod` |

**Usage:**
```bash
export DD_SERVICE=auto-tundra-prod
```

### DD_ENV

Datadog environment tag (development, staging, production).

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `development` |
| **Valid Values** | `development`, `staging`, `production` |
| **Example** | `production` |

**Usage:**
```bash
export DD_ENV=production
```

### DD_VERSION

Application version for Datadog deployment tracking.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `0.1.0` |
| **Example** | `1.2.3` |

**Usage:**
```bash
export DD_VERSION=1.2.3
```

### DD_API_KEY

Datadog API key for cloud Datadog integration (metrics and logs upload).

| Property | Value |
|----------|-------|
| **Required** | No (only for cloud Datadog) |
| **Default** | None |
| **Get Key** | https://app.datadoghq.com/organization-settings/api-keys |
| **Example** | `ddapi...` |

**Usage:**
```bash
export DD_API_KEY=ddapi-your-key-here
```

**Notes:**
- Not required if using local Datadog Agent
- Required for direct cloud submission

### DD_SITE

Datadog site region (US, EU, etc.).

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `datadoghq.com` |
| **Valid Values** | `datadoghq.com`, `datadoghq.eu`, `ddog-gov.com`, `us3.datadoghq.com`, `us5.datadoghq.com` |
| **Example** | `datadoghq.eu` |

**Usage:**
```bash
export DD_SITE=datadoghq.eu
```

### DD_TRACE_AGENT_URL

Datadog trace agent URL for APM trace submission.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `http://localhost:8126` |
| **Example** | `http://datadog-agent:8126` |

**Usage:**
```bash
export DD_TRACE_AGENT_URL=http://datadog-agent:8126
```

### DD_TRACE_AGENT_PORT

Datadog trace agent port.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `8126` |
| **Example** | `8126` |

**Usage:**
```bash
export DD_TRACE_AGENT_PORT=8126
```

### DD_TRACE_ENABLED

Enable or disable Datadog APM tracing.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `true` |
| **Valid Values** | `true`, `false` |
| **Example** | `true` |

**Usage:**
```bash
export DD_TRACE_ENABLED=true
```

### DD_TRACE_DEBUG

Enable debug logging for Datadog tracer.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `false` |
| **Valid Values** | `true`, `false` |
| **Example** | `false` |

**Usage:**
```bash
export DD_TRACE_DEBUG=false
```

### DD_TRACE_SAMPLE_RATE

Trace sampling rate (0.0 to 1.0).

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `1.0` (100% sampling) |
| **Valid Range** | `0.0` - `1.0` |
| **Example** | `0.5` (50% sampling) |

**Usage:**
```bash
export DD_TRACE_SAMPLE_RATE=0.5
```

**Notes:**
- `1.0` = 100% sampling (all traces)
- `0.1` = 10% sampling
- Lower rates reduce costs and overhead

### DD_TRACE_RATE_LIMIT

Maximum number of traces per second to send.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `100` |
| **Example** | `200` |

**Usage:**
```bash
export DD_TRACE_RATE_LIMIT=200
```

### DD_PROFILING_ENABLED

Enable Datadog continuous profiler (CPU, memory, allocations).

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `true` |
| **Valid Values** | `true`, `false` |
| **Example** | `true` |

**Usage:**
```bash
export DD_PROFILING_ENABLED=true
```

### DD_PROFILING_CPU_ENABLED

Enable CPU profiling.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `true` |
| **Valid Values** | `true`, `false` |
| **Example** | `true` |

**Usage:**
```bash
export DD_PROFILING_CPU_ENABLED=true
```

### DD_PROFILING_MEMORY_ENABLED

Enable heap memory profiling.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `true` |
| **Valid Values** | `true`, `false` |
| **Example** | `true` |

**Usage:**
```bash
export DD_PROFILING_MEMORY_ENABLED=true
```

### DD_PROFILING_ALLOCATION_ENABLED

Enable allocation profiling (tracks every memory allocation).

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `true` |
| **Valid Values** | `true`, `false` |
| **Example** | `false` (can have performance impact) |

**Usage:**
```bash
export DD_PROFILING_ALLOCATION_ENABLED=false
```

**Notes:**
- Can add overhead in high-allocation workloads
- Disable if experiencing performance issues

### DD_METRICS_ENABLED

Enable Datadog metrics collection.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `true` |
| **Valid Values** | `true`, `false` |
| **Example** | `true` |

**Usage:**
```bash
export DD_METRICS_ENABLED=true
```

### DD_METRICS_HOSTNAME

Include hostname in Datadog metrics tags.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `true` |
| **Valid Values** | `true`, `false` |
| **Example** | `true` |

**Usage:**
```bash
export DD_METRICS_HOSTNAME=true
```

### DD_LOGS_ENABLED

Enable Datadog log collection.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `true` |
| **Valid Values** | `true`, `false` |
| **Example** | `true` |

**Usage:**
```bash
export DD_LOGS_ENABLED=true
```

### DD_LOGS_INJECTION

Enable automatic trace ID injection into logs for correlation.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `true` |
| **Valid Values** | `true`, `false` |
| **Example** | `true` |

**Usage:**
```bash
export DD_LOGS_INJECTION=true
```

**Notes:**
- Automatically adds `dd.trace_id` and `dd.span_id` to logs
- Enables seamless log-trace correlation in Datadog UI

## 1.5 Rust & System Configuration

### RUST_LOG

Rust logging configuration (controls tracing-subscriber log levels).

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `info` |
| **Valid Values** | `error`, `warn`, `info`, `debug`, `trace` |
| **Example** | `info,at_daemon=debug,at_core=debug` |

**Usage:**
```bash
export RUST_LOG=info,at_daemon=debug,at_core=debug
```

**Notes:**
- Supports per-crate log levels
- Format: `global_level,crate1=level1,crate2=level2`
- Higher verbosity impacts performance

**Examples:**
```bash
# Minimal logging
export RUST_LOG=warn

# Verbose debugging
export RUST_LOG=debug

# Targeted debugging
export RUST_LOG=info,at_agents=debug,at_intelligence=trace
```

### RUST_BACKTRACE

Enable Rust panic backtraces for debugging.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | `0` (disabled) |
| **Valid Values** | `0`, `1`, `full` |
| **Example** | `1` |

**Usage:**
```bash
export RUST_BACKTRACE=1
```

**Values:**
- `0` - Disabled (production default)
- `1` - Enabled (shows backtrace on panic)
- `full` - Full backtrace with all symbols

### HOME

User home directory (used for config file resolution).

| Property | Value |
|----------|-------|
| **Required** | Yes (usually set by OS) |
| **Default** | OS-dependent |
| **Example** | `/Users/studio` (macOS), `/home/user` (Linux) |

**Usage:**
```bash
export HOME=/Users/studio
```

**Notes:**
- Automatically set by operating system
- Auto-Tundra stores config in `$HOME/.auto-tundra/config.toml`
- Database in `$HOME/.auto-tundra/dolt/`

### TOKIO_WORKER_THREADS

Number of worker threads for Tokio async runtime.

| Property | Value |
|----------|-------|
| **Required** | No |
| **Default** | Number of CPU cores |
| **Valid Range** | 1-128 |
| **Example** | `4` |

**Usage:**
```bash
export TOKIO_WORKER_THREADS=4
```

**Notes:**
- Auto-detects CPU core count if not set
- Higher values improve concurrency but increase overhead
- Recommended: 1-2x CPU core count

---

# 2. Configuration File

Non-sensitive settings are stored in `~/.auto-tundra/config.toml`.

## File Location

```
~/.auto-tundra/config.toml
```

## Configuration Structure

The configuration file is divided into 20+ sections:

| Section | Purpose |
|---------|---------|
| `[general]` | Project name, log level, workspace root |
| `[dolt]` | Dolt database directory, port, auto-commit |
| `[cache]` | Token cache path and size limits |
| `[providers]` | LLM provider configuration and failover |
| `[agents]` | Agent concurrency, heartbeat, auto-restart |
| `[security]` | Shell execution, sandbox, execution profiles |
| `[daemon]` | Daemon port, host, TLS settings |
| `[ui]` | UI theme, refresh rate, token cost display |
| `[bridge]` | API transport, socket path, buffer size |
| `[display]` | Display theme, font size, compact mode |
| `[kanban]` | Kanban board column mode, planning poker |
| `[terminal]` | Terminal font, size, cursor style |
| `[integrations]` | GitHub, GitLab, Linear env var names |
| `[appearance]` | Appearance mode, color theme |
| `[language]` | Interface language |
| `[dev_tools]` | IDE preferences, terminal settings, yolo mode |
| `[agent_profile]` | Default agent profile and framework settings |
| `[paths]` | Python, Git, GitHub CLI paths |
| `[api_profiles]` | Custom API profile definitions |
| `[updates]` | Version, auto-update, beta channel settings |
| `[notifications]` | Task completion, failure, review notifications |
| `[debug]` | Anonymous error reporting settings |
| `[memory]` | Memory system (Graphiti integration) settings |

## Example Configuration

```toml
[general]
project_name = "auto-tundra"
log_level = "info"
workspace_root = "/Users/studio/projects"

[dolt]
dir = "~/.auto-tundra/dolt"
port = 3306
auto_commit = false

[agents]
max_concurrent = 10
heartbeat_interval_secs = 30
auto_restart = true
direct_mode = false

[security]
allow_shell_exec = true

[daemon]
port = 9090
host = "127.0.0.1"

[integrations]
github_token_env = "GITHUB_TOKEN"
gitlab_token_env = "GITLAB_TOKEN"
linear_api_key_env = "LINEAR_API_KEY"

[memory]
enable_memory = false
graphiti_server_url = "http://localhost:8000"
embedding_provider = "openai"
```

## Generating Default Config

To generate a default configuration file:

```bash
# Config is auto-generated on first run
cargo run --bin at -- status

# Config is saved to:
# ~/.auto-tundra/config.toml
```

---

# 3. Security Best Practices

## Never Commit Secrets

**⚠️ CRITICAL: Never commit API keys, tokens, or secrets to version control!**

```bash
# Add to .gitignore
echo ".env" >> .gitignore
echo "*.env" >> .gitignore
echo ".auto-tundra/config.toml" >> .gitignore  # If it contains secrets (it shouldn't!)
```

## Use Environment Variables for Secrets

**✅ GOOD: Environment variables**
```bash
export ANTHROPIC_API_KEY=sk-ant-...
export GITHUB_TOKEN=ghp_...
```

**❌ BAD: Hardcoded in config files**
```toml
# DON'T DO THIS!
[credentials]
anthropic_key = "sk-ant-..."  # WRONG!
```

## Secure Storage

Auto-Tundra uses a **credential provider pattern**:
- Secrets are **only** read from environment variables at runtime
- Config files (`config.toml`) **never** store API keys or tokens
- Config files only store the **name** of the env var to read (e.g., `"GITHUB_TOKEN"`)

## Shell Profile Setup (Recommended)

Add API keys to your shell profile for persistent configuration:

**For Zsh (macOS default):**
```bash
# Edit ~/.zshrc
nano ~/.zshrc

# Add API keys
export ANTHROPIC_API_KEY=sk-ant-your-key-here
export GITHUB_TOKEN=ghp_your-token-here

# Reload
source ~/.zshrc
```

**For Bash:**
```bash
# Edit ~/.bashrc
nano ~/.bashrc

# Add API keys
export ANTHROPIC_API_KEY=sk-ant-your-key-here
export GITHUB_TOKEN=ghp_your-token-here

# Reload
source ~/.bashrc
```

## Verify Configuration

Check that your environment variables are set correctly:

```bash
# List all Auto-Tundra related env vars
env | grep -E 'ANTHROPIC|OPENAI|OPENROUTER|GITHUB|GITLAB|LINEAR|DD_'

# Check specific key (without revealing it)
if [ -n "$ANTHROPIC_API_KEY" ]; then echo "✅ ANTHROPIC_API_KEY is set"; else echo "❌ ANTHROPIC_API_KEY is not set"; fi
```

## Development vs Production

Use different API keys for development and production:

```bash
# Development
export DD_ENV=development
export ANTHROPIC_API_KEY=sk-ant-dev-key-here

# Production
export DD_ENV=production
export ANTHROPIC_API_KEY=sk-ant-prod-key-here
```

---

## 📚 Related Documentation

- **[Getting Started](../GETTING_STARTED.md)** - Initial setup and first run
- **[Project Handbook](PROJECT_HANDBOOK.md)** - Architecture and system overview
- **[README](../README.md)** - Project overview and quick start
- **[Contributing](../CONTRIBUTING.md)** - Development workflow and guidelines

---

**Last Updated:** 2026-03-01
