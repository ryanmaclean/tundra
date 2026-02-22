---
name: ollama-refinery-overseer
description: Use for local Ollama-first execution with queued multi-agent CLI workflows. Configures auto-tundra for one-human-overseer and many-agent-subscriber code refinery mode.
allowed_tools: [Bash, Read, Edit]
references: [/Users/studio/rust-harness/scripts/refinery_queue_at.sh, /Users/studio/rust-harness/crates/at-core/src/config.rs, /Users/studio/rust-harness/crates/at-bridge/src/http_api.rs]
---

# Ollama Refinery Overseer

## Trigger
Use this skill when running local-first workflows with many agent workers and one human overseer.

## Goals
1. Route model calls to local Ollama.
2. Enforce queueing to avoid GPU/model contention.
3. Execute CLI submissions through a serialized wrapper.

## One-time Setup

### 1) Start Ollama and pull a coding model
```bash
ollama serve
ollama pull qwen2.5-coder:14b
```

### 2) Configure auto-tundra provider defaults
Edit `~/.auto-tundra/config.toml`:
```toml
[providers]
local_base_url = "http://127.0.0.1:11434"
local_model = "qwen2.5-coder:14b"
local_api_key_env = "LOCAL_API_KEY"
```

### 3) Queue controls (critical)
```bash
# Queue local LLM calls (at-intelligence LocalProvider)
export AT_LOCAL_LLM_MAX_CONCURRENT=1

# Queue task pipeline execution in API layer
export AT_PIPELINE_MAX_CONCURRENT=1
```

## Queue-safe CLI usage
Always submit via the wrapper:
```bash
/Users/studio/rust-harness/scripts/refinery_queue_at.sh run -t "Refinery task" --dry-run -j
/Users/studio/rust-harness/scripts/refinery_queue_at.sh agent run -r qa-reviewer -t "Refinery review" --dry-run -j
```

## Queue observability
```bash
curl -s http://localhost:9090/api/pipeline/queue | jq .
```

Expected fields:
- `limit`
- `waiting`
- `running`
- `available_permits`

## Overseer pattern
- Human creates/refines priorities.
- Agent subscribers submit tasks through `refinery_queue_at.sh`.
- Queue gate ensures local model stability.
- Overseer checks queue pressure and adjusts throughput via env vars.
