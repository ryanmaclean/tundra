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

## Configuration Sections Reference

### 2.1 `[general]` - General Settings

Project-level settings and workspace configuration.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `project_name` | String | `"auto-tundra"` | Name of the Auto-Tundra project instance |
| `log_level` | String | `"info"` | Logging verbosity (`trace`, `debug`, `info`, `warn`, `error`) |
| `workspace_root` | String (Optional) | `None` | Root directory for workspace operations |

**Environment Variable References:** None

**Example:**
```toml
[general]
project_name = "auto-tundra"
log_level = "info"
workspace_root = "/Users/studio/projects"
```

---

### 2.2 `[dolt]` - Dolt Database Configuration

Settings for the embedded Dolt database (version-controlled SQL database).

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `dir` | String | `"./dolt"` | Directory path for Dolt database storage |
| `port` | u16 | `3306` | MySQL-compatible port for Dolt server |
| `auto_commit` | bool | `false` | Automatically commit database changes |

**Environment Variable References:** None

**Example:**
```toml
[dolt]
dir = "~/.auto-tundra/dolt"
port = 3306
auto_commit = false
```

---

### 2.3 `[cache]` - Cache Configuration

Token cache settings for LLM provider responses.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | String | `"~/.auto-tundra/cache.db"` | SQLite database path for cache storage |
| `max_size_mb` | u64 | `256` | Maximum cache size in megabytes |

**Environment Variable References:** None

**Example:**
```toml
[cache]
path = "~/.auto-tundra/cache.db"
max_size_mb = 512
```

---

### 2.4 `[providers]` - LLM Provider Configuration

LLM provider settings and failover configuration.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `anthropic_key_env` | String (Optional) | `None` | Env var name for Anthropic API key (see [ANTHROPIC_API_KEY](#anthropic_api_key)) |
| `openai_key_env` | String (Optional) | `None` | Env var name for OpenAI API key (see [OPENAI_API_KEY](#openai_api_key)) |
| `google_key_env` | String (Optional) | `None` | Env var name for Google API key |
| `local_base_url` | String | `"http://127.0.0.1:11434"` | Base URL for local inference server (OpenAI-compatible) |
| `local_model` | String | `"qwen2.5-coder:14b"` | Default model alias for local inference |
| `local_api_key_env` | String | `"LOCAL_API_KEY"` | Env var name for local server API key (see [LOCAL_API_KEY](#local_api_key)) |
| `default_max_tokens` | u32 | `16384` | Default max tokens for LLM requests |

**Environment Variable References:**
- `anthropic_key_env` → [ANTHROPIC_API_KEY](#anthropic_api_key)
- `openai_key_env` → [OPENAI_API_KEY](#openai_api_key)
- `local_api_key_env` → [LOCAL_API_KEY](#local_api_key)

**Example:**
```toml
[providers]
anthropic_key_env = "ANTHROPIC_API_KEY"
openai_key_env = "OPENAI_API_KEY"
local_base_url = "http://127.0.0.1:11434"
local_model = "qwen2.5-coder:14b"
default_max_tokens = 16384
```

---

### 2.5 `[agents]` - Agent Configuration

Agent execution, concurrency, and lifecycle settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_concurrent` | u32 | `8` | Maximum concurrent agent executions |
| `heartbeat_interval_secs` | u64 | `30` | Agent heartbeat interval in seconds |
| `auto_restart` | bool | `false` | Automatically restart failed agents |
| `direct_mode` | bool | `false` | When `true`, agents work in repo root instead of worktrees |

**Environment Variable References:** None

**Example:**
```toml
[agents]
max_concurrent = 10
heartbeat_interval_secs = 30
auto_restart = true
direct_mode = false
```

**Notes:**
- `direct_mode = true` disables git worktree isolation (use with caution)
- Higher `max_concurrent` values increase parallelism but consume more resources

---

### 2.6 `[security]` - Security Configuration

Security policies, sandboxing, and execution profiles.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `allow_shell_exec` | bool | `false` | Allow shell command execution (legacy field) |
| `sandbox` | bool | `true` | Enable sandbox mode (legacy field) |
| `allowed_paths` | Vec<String> | `[]` | Filesystem paths allowed for agent access |
| `allowed_origins` | Vec<String> | `[]` | HTTP origins allowed for CORS |
| `auto_lock_timeout_mins` | u32 | `15` | Auto-lock timeout in minutes |
| `sandbox_mode` | bool | `true` | Enable sandbox mode globally |
| `active_execution_profile` | String | `"balanced"` | Active profile name (must exist in `execution_profiles`) |
| `execution_profiles` | Vec<ExecutionProfile> | See below | Execution profile definitions |

**Default Execution Profiles:**
```toml
[[security.execution_profiles]]
name = "safe"
sandbox = true
allow_network = false
allow_shell_exec = false
approval_mode = "always"

[[security.execution_profiles]]
name = "balanced"
sandbox = true
allow_network = true
allow_shell_exec = true
approval_mode = "on_failure"

[[security.execution_profiles]]
name = "trusted"
sandbox = false
allow_network = true
allow_shell_exec = true
approval_mode = "never"
```

**Validation Rules:**
- `execution_profiles` must not be empty
- Each profile must have a unique, non-empty `name`
- `active_execution_profile` must reference a profile in `execution_profiles`

**Environment Variable References:** None

**Example:**
```toml
[security]
allow_shell_exec = true
sandbox = true
allowed_paths = ["/tmp", "/Users/studio/projects"]
active_execution_profile = "balanced"

[[security.execution_profiles]]
name = "custom"
sandbox = true
allow_network = true
allow_shell_exec = false
approval_mode = "on_failure"
```

**Notes:**
- `approval_mode` values: `never`, `on_failure`, `always`
- Profiles control sandboxing, network access, and user approval requirements

---

### 2.7 `[daemon]` - Daemon Configuration

HTTP/WebSocket daemon server settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `port` | u16 | `9876` | TCP port for daemon HTTP/WebSocket server |
| `host` | String | `"127.0.0.1"` | Bind address for daemon server |
| `tls` | bool | `false` | Enable TLS/HTTPS (requires certificates) |

**Environment Variable References:**
- See [AUTO_TUNDRA_API_KEY](#auto_tundra_api_key) for daemon authentication

**Example:**
```toml
[daemon]
port = 9090
host = "127.0.0.1"
tls = false
```

---

### 2.8 `[ui]` - UI Configuration

Legacy UI theme and refresh settings (deprecated in favor of `[display]`).

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `theme` | String | `"dark"` | UI theme (`dark`, `light`) |
| `refresh_ms` | u64 | `500` | UI refresh interval in milliseconds |
| `show_token_costs` | bool | `false` | Display token usage costs in UI |

**Environment Variable References:** None

**Example:**
```toml
[ui]
theme = "dark"
refresh_ms = 500
show_token_costs = true
```

---

### 2.9 `[bridge]` - Bridge Configuration

API bridge transport and buffering settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `transport` | String | `"unix"` | Transport protocol (`unix`, `tcp`) |
| `socket_path` | String | `"/tmp/auto-tundra.sock"` | Unix socket path (when `transport = "unix"`) |
| `buffer_size` | usize | `8192` | Message buffer size in bytes |

**Environment Variable References:** None

**Example:**
```toml
[bridge]
transport = "unix"
socket_path = "/tmp/auto-tundra.sock"
buffer_size = 8192
```

---

### 2.10 `[display]` - Display Configuration

UI display settings (font, theme, compact mode).

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `theme` | String | `"dark"` | Display theme (`dark`, `light`) |
| `font_size` | u8 | `14` | Font size in points |
| `compact_mode` | bool | `false` | Enable compact UI mode |

**Environment Variable References:** None

**Example:**
```toml
[display]
theme = "dark"
font_size = 16
compact_mode = false
```

---

### 2.11 `[kanban]` - Kanban Board Configuration

Kanban board layout and planning poker settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `column_mode` | String | `"classic_8"` | Column layout mode (must not be empty) |
| `planning_poker` | PlanningPokerConfig | See below | Planning poker settings |

**PlanningPokerConfig Fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable planning poker feature |
| `default_deck` | String | `"fibonacci"` | Default card deck (`fibonacci`, `modified_fibonacci`, `powers_of_two`, `tshirt`) |
| `allow_custom_deck` | bool | `true` | Allow custom estimation decks |
| `reveal_requires_all_votes` | bool | `false` | Require all team members to vote before reveal |
| `round_duration_seconds` | u64 | `300` | Planning poker round timeout (1-86400 seconds) |

**Validation Rules:**
- `column_mode` must not be empty
- `planning_poker.default_deck` must be one of: `fibonacci`, `modified_fibonacci`, `powers_of_two`, `tshirt`
- `planning_poker.round_duration_seconds` must be between 1 and 86400

**Environment Variable References:** None

**Example:**
```toml
[kanban]
column_mode = "classic_8"

[kanban.planning_poker]
enabled = true
default_deck = "fibonacci"
allow_custom_deck = true
reveal_requires_all_votes = false
round_duration_seconds = 600
```

---

### 2.12 `[terminal]` - Terminal Configuration

Terminal emulator font and cursor settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `font_family` | String | `"JetBrains Mono"` | Terminal font family |
| `font_size` | u8 | `14` | Terminal font size in points |
| `cursor_style` | String | `"block"` | Cursor style (`block`, `underline`, `bar`) |

**Environment Variable References:** None

**Example:**
```toml
[terminal]
font_family = "Fira Code"
font_size = 16
cursor_style = "block"
```

---

### 2.13 `[integrations]` - Integration Configuration

Third-party service integration settings (GitHub, GitLab, Linear).

**⚠️ Security:** This section stores **environment variable names**, NOT actual credentials.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `github_token_env` | String | `"GITHUB_TOKEN"` | Env var name for GitHub token (see [GITHUB_TOKEN](#github_token)) |
| `github_owner` | String (Optional) | `None` | GitHub repository owner (org or user) |
| `github_repo` | String (Optional) | `None` | GitHub repository name |
| `gitlab_token_env` | String | `"GITLAB_TOKEN"` | Env var name for GitLab token (see [GITLAB_TOKEN](#gitlab_token)) |
| `gitlab_project_id` | String (Optional) | `None` | GitLab project ID (numeric or `group/project`) |
| `gitlab_url` | String (Optional) | `None` | GitLab instance URL (defaults to `https://gitlab.com`) |
| `linear_api_key_env` | String | `"LINEAR_API_KEY"` | Env var name for Linear API key (see [LINEAR_API_KEY](#linear_api_key)) |
| `linear_team_id` | String (Optional) | `None` | Linear team ID for issue scoping |

**Environment Variable References:**
- `github_token_env` → [GITHUB_TOKEN](#github_token)
- `gitlab_token_env` → [GITLAB_TOKEN](#gitlab_token)
- `linear_api_key_env` → [LINEAR_API_KEY](#linear_api_key)

**Example:**
```toml
[integrations]
github_token_env = "GITHUB_TOKEN"
github_owner = "your-org"
github_repo = "your-repo"
gitlab_token_env = "GITLAB_TOKEN"
gitlab_project_id = "12345"
linear_api_key_env = "LINEAR_API_KEY"
linear_team_id = "TEAM-123"
```

---

### 2.14 `[appearance]` - Appearance Configuration

Global appearance and color theme settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `appearance_mode` | String | `"system"` | Appearance mode (`system`, `light`, `dark`) |
| `color_theme` | String | `"arctic"` | Color theme name |

**Environment Variable References:** None

**Example:**
```toml
[appearance]
appearance_mode = "dark"
color_theme = "arctic"
```

---

### 2.15 `[language]` - Language Configuration

UI language and localization settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `interface_language` | String | `"en"` | Interface language code (ISO 639-1) |

**Environment Variable References:** None

**Example:**
```toml
[language]
interface_language = "en"
```

---

### 2.16 `[dev_tools]` - Developer Tools Configuration

IDE, terminal, and developer workflow preferences.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `preferred_ide` | String | `"vscode"` | Preferred IDE (`vscode`, `vim`, `emacs`, etc.) |
| `preferred_terminal` | String | `"default"` | Preferred terminal emulator |
| `auto_name_terminals` | bool | `false` | Automatically name terminal sessions |
| `yolo_mode` | bool | `false` | Enable YOLO mode (skip confirmations - use with caution!) |
| `terminal_font_family` | String | `"JetBrains Mono, monospace"` | Terminal font family with fallbacks |
| `terminal_font_size` | u16 | `14` | Terminal font size in points |
| `terminal_cursor_style` | String | `"block"` | Terminal cursor style |
| `terminal_cursor_blink` | bool | `true` | Enable cursor blinking |
| `terminal_scrollback_lines` | u32 | `5000` | Terminal scrollback buffer size (lines) |

**Environment Variable References:** None

**Example:**
```toml
[dev_tools]
preferred_ide = "vscode"
preferred_terminal = "iTerm2"
auto_name_terminals = true
yolo_mode = false
terminal_font_family = "Fira Code, monospace"
terminal_font_size = 16
terminal_cursor_style = "block"
terminal_cursor_blink = true
terminal_scrollback_lines = 10000
```

**Notes:**
- ⚠️ `yolo_mode = true` disables safety confirmations - use only in trusted environments

---

### 2.17 `[agent_profile]` - Agent Profile Configuration

Agent execution profiles and AI model selection per phase.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `default_profile` | String | `"default"` | Default agent profile name |
| `agent_framework` | String | `"auto-tundra"` | Agent framework identifier |
| `ai_terminal_naming` | bool | `false` | Use AI to generate terminal session names |
| `phase_configs` | Vec<AgentPhaseConfig> | `[]` | Phase-specific model configurations |

**AgentPhaseConfig Fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `phase` | String | `""` | Phase name (e.g., `"planning"`, `"coding"`, `"testing"`) |
| `model` | String | `""` | LLM model for this phase |
| `thinking_level` | String | `""` | Thinking/reasoning level |

**Environment Variable References:** None

**Example:**
```toml
[agent_profile]
default_profile = "default"
agent_framework = "auto-tundra"
ai_terminal_naming = true

[[agent_profile.phase_configs]]
phase = "planning"
model = "claude-3-5-sonnet-20241022"
thinking_level = "deep"

[[agent_profile.phase_configs]]
phase = "coding"
model = "qwen2.5-coder:14b"
thinking_level = "normal"
```

---

### 2.18 `[paths]` - Paths Configuration

Custom paths for external tools and executables.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `python_path` | String | `""` | Python interpreter path (empty = auto-detect) |
| `git_path` | String | `""` | Git executable path (empty = auto-detect) |
| `github_cli_path` | String | `""` | GitHub CLI (`gh`) path (empty = auto-detect) |
| `claude_cli_path` | String | `""` | Claude CLI path (empty = auto-detect) |
| `auto_claude_path` | String | `""` | Auto-Claude CLI path (empty = auto-detect) |

**Environment Variable References:** None

**Example:**
```toml
[paths]
python_path = "/usr/local/bin/python3"
git_path = "/usr/bin/git"
github_cli_path = "/opt/homebrew/bin/gh"
```

**Notes:**
- Empty strings trigger auto-detection via `$PATH` lookup

---

### 2.19 `[api_profiles]` - API Profiles Configuration

Custom API endpoint profiles for alternative LLM providers.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `profiles` | Vec<ApiProfileEntry> | `[]` | Custom API profile definitions |

**ApiProfileEntry Fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | String | `""` | Profile name |
| `base_url` | String | `""` | API base URL |
| `api_key_env` | String | `""` | Env var name for API key |

**Environment Variable References:**
- `api_key_env` → Custom environment variable (user-defined)

**Example:**
```toml
[[api_profiles.profiles]]
name = "custom-openai"
base_url = "https://api.openai.com/v1"
api_key_env = "CUSTOM_OPENAI_KEY"

[[api_profiles.profiles]]
name = "ollama-local"
base_url = "http://localhost:11434"
api_key_env = "OLLAMA_API_KEY"
```

---

### 2.20 `[updates]` - Updates Configuration

Version tracking and auto-update settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `version` | String | `""` | Current version string |
| `is_latest` | bool | `false` | Whether running latest version |
| `auto_update_projects` | bool | `false` | Automatically update project dependencies |
| `beta_updates` | bool | `false` | Enable beta/canary update channel |

**Environment Variable References:** None

**Example:**
```toml
[updates]
version = "0.1.0"
is_latest = true
auto_update_projects = false
beta_updates = false
```

---

### 2.21 `[notifications]` - Notification Configuration

Task and event notification settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `on_task_complete` | bool | `true` | Notify on task completion |
| `on_task_failed` | bool | `true` | Notify on task failure |
| `on_review_needed` | bool | `true` | Notify when code review is needed |
| `sound_enabled` | bool | `true` | Enable notification sounds |

**Environment Variable References:** None

**Example:**
```toml
[notifications]
on_task_complete = true
on_task_failed = true
on_review_needed = true
sound_enabled = false
```

---

### 2.22 `[debug]` - Debug Configuration

Debugging and error reporting settings.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `anonymous_error_reporting` | bool | `false` | Enable anonymous crash/error reporting |

**Environment Variable References:** None

**Example:**
```toml
[debug]
anonymous_error_reporting = true
```

---

### 2.23 `[memory]` - Memory Configuration

AI memory system settings (Graphiti integration for long-term memory).

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enable_memory` | bool | `false` | Enable AI memory system |
| `enable_agent_memory_access` | bool | `false` | Allow agents to access memory |
| `graphiti_server_url` | String | `""` | Graphiti server URL (e.g., `http://localhost:8000`) |
| `embedding_provider` | String | `""` | Embedding provider (`openai`, `anthropic`, etc.) |
| `embedding_model` | String | `""` | Embedding model name |

**Environment Variable References:** None

**Example:**
```toml
[memory]
enable_memory = true
enable_agent_memory_access = true
graphiti_server_url = "http://localhost:8000"
embedding_provider = "openai"
embedding_model = "text-embedding-3-small"
```

**Notes:**
- Requires external Graphiti server running
- Embedding provider must have API key configured (see [LLM Provider API Keys](#11-llm-provider-api-keys))

---

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
