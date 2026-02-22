---
name: api-operations
description: Use the Auto-Tundra HTTP API directly (via curl) for operations not exposed in the CLI, such as task updates, terminal management, GitLab/Linear queries, and Kanban board manipulation.
allowed_tools: [Bash, Read]
references: [/Users/studio/rust-harness/crates/at-bridge/src/http_api.rs, /Users/studio/rust-harness/docs/PROJECT_HANDBOOK.md]
---

# API Operations

## Trigger
Use this skill when you need to interact with the Auto-Tundra daemon API directly — either because the CLI doesn't expose the operation, or you need fine-grained control.

## Base URL
Default: `http://localhost:9090`

## When to Use API vs CLI

| Use CLI (`at`) when | Use API (`curl`) when |
|---------------------|----------------------|
| Creating/completing beads | Updating task fields (PATCH/PUT) |
| Running skill-aware tasks | Managing terminals |
| Doctor/status checks | Querying GitLab/Linear issues |
| Dry-run previews | Kanban column operations |
| | WebSocket subscriptions |

## Core Endpoints

### Status & Health
```bash
curl -s http://localhost:9090/api/status | jq .
curl -s http://localhost:9090/api/kpi | jq .
curl -s http://localhost:9090/api/metrics/json | jq .
```

### Beads (Tasks)
```bash
# List all beads
curl -s http://localhost:9090/api/beads | jq .

# Create a bead
curl -s -X POST http://localhost:9090/api/beads \
  -H 'Content-Type: application/json' \
  -d '{"title":"Fix bug","lane":"Standard"}' | jq .

# Update bead status
curl -s -X POST http://localhost:9090/api/beads/<UUID>/status \
  -H 'Content-Type: application/json' \
  -d '{"status":"Hooked"}' | jq .
```

### Tasks (Detailed Work Items)
```bash
# List tasks
curl -s http://localhost:9090/api/tasks | jq .

# Create task
curl -s -X POST http://localhost:9090/api/tasks \
  -H 'Content-Type: application/json' \
  -d '{
    "title": "Review auth flow",
    "bead_id": "<BEAD_UUID>",
    "category": "QA",
    "priority": "High",
    "complexity": "Medium"
  }' | jq .

# Update task
curl -s -X PUT http://localhost:9090/api/tasks/<UUID> \
  -H 'Content-Type: application/json' \
  -d '{"priority":"Critical","description":"Updated scope"}' | jq .

# Execute task pipeline
curl -s -X POST http://localhost:9090/api/tasks/<UUID>/execute \
  -H 'Content-Type: application/json' \
  -d '{}' | jq .
```

### Integrations
```bash
# GitLab issues (requires GITLAB_TOKEN env)
curl -s "http://localhost:9090/api/gitlab/issues?project_id=12345" | jq .

# GitLab MR review
curl -s -X POST http://localhost:9090/api/gitlab/merge-requests/42/review \
  -H 'Content-Type: application/json' | jq .

# Linear issues (requires LINEAR_API_KEY env)
curl -s "http://localhost:9090/api/linear/issues?team_id=TEAM123" | jq .
```

### Terminals
```bash
# List terminals
curl -s http://localhost:9090/api/terminals | jq .

# Create terminal
curl -s -X POST http://localhost:9090/api/terminals \
  -H 'Content-Type: application/json' \
  -d '{"name":"agent-workspace"}' | jq .
```

### Kanban
```bash
# Get columns
curl -s http://localhost:9090/api/kanban/columns | jq .

# Save task ordering
curl -s -X POST http://localhost:9090/api/kanban/ordering \
  -H 'Content-Type: application/json' \
  -d '{"column_id":"in_progress","task_ids":["<UUID1>","<UUID2>"]}' | jq .
```

## Rules
1. Always pipe through `jq .` for readable output; use `jq -r .field` for extracting values.
2. Always check HTTP status codes: 2xx = success, 4xx = client error, 5xx = server error.
3. Missing credentials return `503` with `{"error":"...","env_var":"GITLAB_TOKEN"}` — check env vars.
4. When creating resources, capture the returned UUID for subsequent operations.
5. Prefer the CLI for bead lifecycle (sling/hook/done/nudge) — it handles error formatting better.
