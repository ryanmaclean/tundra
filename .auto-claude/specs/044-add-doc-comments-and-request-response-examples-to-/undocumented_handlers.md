# Undocumented Handler Functions in at-bridge/http_api.rs

**Audit Date:** 2026-02-27
**Total Undocumented Handlers:** 19

This document lists all handler functions in `crates/at-bridge/src/http_api.rs` that lack documentation comments (`///`). Each entry includes the line number, endpoint path, HTTP method, and current status.

---

## 1. get_kpi

- **Line Number:** 1253
- **Endpoint:** `GET /api/kpi`
- **Status:** Missing documentation
- **Purpose:** Returns KPI (Key Performance Indicator) snapshot data

---

## 2. get_pipeline_queue_status

- **Line Number:** 1690
- **Endpoint:** `GET /api/pipeline/queue`
- **Status:** Missing documentation
- **Purpose:** Returns current pipeline queue status including waiting/running tasks and available permits

---

## 3. start_planning_poker

- **Line Number:** 3475
- **Endpoint:** `POST /api/kanban/poker/start`
- **Status:** Missing documentation
- **Purpose:** Initiates a planning poker session for a bead

---

## 4. submit_planning_poker_vote

- **Line Number:** 3536
- **Endpoint:** `POST /api/kanban/poker/vote`
- **Status:** Missing documentation
- **Purpose:** Submits a vote in an active planning poker session

---

## 5. reveal_planning_poker

- **Line Number:** 3589
- **Endpoint:** `POST /api/kanban/poker/reveal`
- **Status:** Missing documentation
- **Purpose:** Reveals all votes in a planning poker session

---

## 6. simulate_planning_poker

- **Line Number:** 3640
- **Endpoint:** `POST /api/kanban/poker/simulate`
- **Status:** Missing documentation
- **Purpose:** Simulates a planning poker session (likely for testing)

---

## 7. get_planning_poker_session

- **Line Number:** 3653
- **Endpoint:** `GET /api/kanban/poker/{bead_id}`
- **Status:** Missing documentation
- **Purpose:** Retrieves the current state of a planning poker session for a specific bead

---

## 8. trigger_github_sync

- **Line Number:** 3806
- **Endpoint:** `POST /api/github/sync`
- **Status:** Missing documentation
- **Purpose:** Manually triggers GitHub issue synchronization

---

## 9. get_sync_status

- **Line Number:** 3885
- **Endpoint:** `GET /api/github/sync/status`
- **Status:** Missing documentation
- **Purpose:** Returns the current GitHub synchronization status

---

## 10. handle_ws

- **Line Number:** 4313
- **Endpoint:** Internal handler called by `GET /ws` via `ws_handler`
- **Status:** Missing documentation
- **Purpose:** Handles WebSocket connections for real-time communication (internal implementation)

---

## 11. handle_events_ws

- **Line Number:** 4371
- **Endpoint:** Internal handler called by `GET /api/events/ws` via `events_ws_handler`
- **Status:** Missing documentation
- **Purpose:** Handles WebSocket connections for event streaming (internal implementation)

---

## 12. list_mcp_servers

- **Line Number:** 4431
- **Endpoint:** `GET /api/mcp/servers`
- **Status:** Missing documentation
- **Purpose:** Lists all available MCP (Model Context Protocol) servers

---

## 13. call_mcp_tool

- **Line Number:** 4504
- **Endpoint:** `POST /api/mcp/tools/call`
- **Status:** Missing documentation
- **Purpose:** Invokes a tool on an MCP server

---

## 14. list_github_prs

- **Line Number:** 5298
- **Endpoint:** `GET /api/github/prs`
- **Status:** Missing documentation
- **Purpose:** Lists GitHub pull requests for the current repository

---

## 15. import_github_issue

- **Line Number:** 5365
- **Endpoint:** `POST /api/github/issues/{number}/import`
- **Status:** Missing documentation
- **Purpose:** Imports a specific GitHub issue into the task system

---

## 16. get_costs

- **Line Number:** 5640
- **Endpoint:** `GET /api/costs`
- **Status:** Missing documentation
- **Purpose:** Returns cost tracking information (likely API/model usage costs)

---

## 17. list_convoys

- **Line Number:** 5694
- **Endpoint:** `GET /api/convoys`
- **Status:** Missing documentation
- **Purpose:** Lists all convoys (coordinated groups of agents)

---

## 18. notify_profile_swap

- **Line Number:** 6846
- **Endpoint:** `POST /api/notifications/profile-swap`
- **Status:** Missing documentation
- **Purpose:** Sends a notification when user profile is swapped

---

## 19. check_app_update

- **Line Number:** 6871
- **Endpoint:** `GET /api/notifications/app-update`
- **Status:** Missing documentation
- **Purpose:** Checks for available application updates

---

## Summary by Category

### Planning Poker (5 handlers)
- `start_planning_poker` (Line 3475)
- `submit_planning_poker_vote` (Line 3536)
- `reveal_planning_poker` (Line 3589)
- `simulate_planning_poker` (Line 3640)
- `get_planning_poker_session` (Line 3653)

### GitHub Integration (4 handlers)
- `trigger_github_sync` (Line 3806)
- `get_sync_status` (Line 3885)
- `list_github_prs` (Line 5298)
- `import_github_issue` (Line 5365)

### MCP Servers (2 handlers)
- `list_mcp_servers` (Line 4431)
- `call_mcp_tool` (Line 4504)

### WebSocket (2 handlers - internal)
- `handle_ws` (Line 4313)
- `handle_events_ws` (Line 4371)

### Notifications (2 handlers)
- `notify_profile_swap` (Line 6846)
- `check_app_update` (Line 6871)

### Miscellaneous (4 handlers)
- `get_kpi` (Line 1253)
- `get_pipeline_queue_status` (Line 1690)
- `get_costs` (Line 5640)
- `list_convoys` (Line 5694)

---

## Notes

1. **Internal Handlers**: The `handle_ws` and `handle_events_ws` functions are internal WebSocket handlers that are not directly exposed as routes. They are called via upgrade handlers (`ws_handler` and `events_ws_handler` respectively). While documentation is still recommended, these may have lower priority than public-facing endpoints.

2. **Priority for Documentation**: Consider prioritizing documentation based on:
   - Public API endpoints (higher priority)
   - Frequently used endpoints
   - Complex functionality requiring detailed explanation
   - Endpoints with non-obvious request/response formats

3. **Documentation Pattern**: Well-documented handlers in this file follow this pattern:
   - Summary line with HTTP method and endpoint
   - Detailed description of functionality
   - Request/response format documentation
   - Example JSON request/response bodies

4. **Next Steps**: Use this list to systematically add documentation comments to each handler following the established patterns in the codebase.
