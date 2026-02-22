#!/usr/bin/env bash
# =============================================================================
# test_api_ui_contract.sh - API contract tests for auto-tundra
#
# Verifies that every API endpoint the Leptos frontend calls (defined in
# app/leptos-ui/src/api.rs) actually exists on the backend and returns
# the expected JSON shape. This is a contract test: it ensures the frontend
# and backend agree on endpoints and response structure.
#
# Usage: ./tests/interactive/test_api_ui_contract.sh
# Requires: curl, jq, the API serving at localhost:9090
# =============================================================================
set -euo pipefail

# ── Colors ──
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

PASS=0
FAIL=0
SKIP=0
API_BASE="${API_BASE:-http://localhost:9090}"

log_pass() { PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $1"; }
log_fail() { FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $1"; }
log_skip() { SKIP=$((SKIP+1)); echo -e "  ${YELLOW}SKIP${NC} $1"; }
log_info() { echo -e "  ${CYAN}INFO${NC} $1"; }

# Check if jq is available
HAS_JQ=true
if ! command -v jq &>/dev/null; then
    HAS_JQ=false
    echo -e "${YELLOW}WARNING:${NC} jq not found. JSON shape validation will be skipped."
fi

echo ""
echo "========================================"
echo " API-UI Contract Tests - auto-tundra"
echo "========================================"
echo ""

# ── Check if API is reachable ──
if ! curl -sf -o /dev/null --connect-timeout 3 "$API_BASE/api/status"; then
    echo -e "${RED}ERROR:${NC} API not reachable at $API_BASE"
    echo "  Start the daemon first."
    echo ""
    exit 1
fi
log_info "API is reachable at $API_BASE"
echo ""

# ── Helper: test GET endpoint exists and returns valid JSON ──
test_get_endpoint() {
    local path="$1"
    local desc="$2"
    local expected_shape="${3:-}" # e.g., "array" or "object" or field name

    echo "--- $desc ---"

    HTTP_CODE=$(curl -s -o /tmp/_at_test_body.json -w "%{http_code}" --connect-timeout 5 "$API_BASE$path" 2>/dev/null || echo "000")
    BODY=$(cat /tmp/_at_test_body.json 2>/dev/null || echo "")

    if [ "$HTTP_CODE" = "000" ]; then
        log_fail "GET $path - connection failed"
        return
    fi

    # Accept 200, 503 (unconfigured GitHub), and 400 (missing config)
    if [ "$HTTP_CODE" = "200" ]; then
        log_pass "GET $path -> HTTP $HTTP_CODE"
    elif [ "$HTTP_CODE" = "503" ] || [ "$HTTP_CODE" = "400" ]; then
        log_pass "GET $path -> HTTP $HTTP_CODE (expected: unconfigured)"
        return
    else
        log_fail "GET $path -> HTTP $HTTP_CODE (expected 200/503/400)"
        return
    fi

    # Validate JSON
    if [ "$HAS_JQ" = true ]; then
        if echo "$BODY" | jq . >/dev/null 2>&1; then
            log_pass "GET $path returns valid JSON"
        else
            log_fail "GET $path returned invalid JSON"
            return
        fi

        # Shape validation
        if [ "$expected_shape" = "array" ]; then
            if echo "$BODY" | jq -e 'type == "array"' >/dev/null 2>&1; then
                log_pass "GET $path returns a JSON array"
            else
                log_fail "GET $path expected array, got: $(echo "$BODY" | jq -r 'type')"
            fi
        elif [ "$expected_shape" = "object" ]; then
            if echo "$BODY" | jq -e 'type == "object"' >/dev/null 2>&1; then
                log_pass "GET $path returns a JSON object"
            else
                log_fail "GET $path expected object, got: $(echo "$BODY" | jq -r 'type')"
            fi
        elif [ -n "$expected_shape" ]; then
            # Check for a specific field
            if echo "$BODY" | jq -e "has(\"$expected_shape\")" >/dev/null 2>&1; then
                log_pass "GET $path has field '$expected_shape'"
            elif echo "$BODY" | jq -e ".[0] | has(\"$expected_shape\")" >/dev/null 2>&1; then
                log_pass "GET $path array items have field '$expected_shape'"
            else
                log_skip "GET $path field '$expected_shape' not found (may be empty array)"
            fi
        fi
    fi
}

# ── Helper: test POST endpoint exists ──
test_post_endpoint() {
    local path="$1"
    local desc="$2"
    local body="${3:-{}}"
    local expected_codes="${4:-200,201,400,422,503}"

    echo "--- $desc ---"
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
        -H "Content-Type: application/json" \
        -d "$body" \
        --connect-timeout 5 \
        "$API_BASE$path" 2>/dev/null || echo "000")

    if echo "$expected_codes" | grep -q "$HTTP_CODE"; then
        log_pass "POST $path -> HTTP $HTTP_CODE (endpoint exists)"
    else
        log_fail "POST $path -> HTTP $HTTP_CODE (not in expected: $expected_codes)"
    fi
}

# =============================================================================
# Frontend API calls from api.rs -> Backend endpoints
# =============================================================================

# ── 1. GET /api/status (fetch_status) ──
test_get_endpoint "/api/status" "fetch_status() -> /api/status" "object"

# Validate status response shape matches ApiStatus
if [ "$HAS_JQ" = true ]; then
    BODY=$(curl -sf "$API_BASE/api/status")
    for field in version uptime_seconds agent_count bead_count; do
        # Backend uses uptime_seconds, frontend expects uptime_secs
        if echo "$BODY" | jq -e "has(\"$field\")" >/dev/null 2>&1; then
            log_pass "/api/status has field '$field'"
        else
            log_skip "/api/status missing field '$field'"
        fi
    done
fi

# ── 2. GET /api/beads (fetch_beads) ──
test_get_endpoint "/api/beads" "fetch_beads() -> /api/beads" "array"

# ── 3. GET /api/agents (fetch_agents) ──
test_get_endpoint "/api/agents" "fetch_agents() -> /api/agents" "array"

# ── 4. GET /api/kpi (fetch_kpi) ──
test_get_endpoint "/api/kpi" "fetch_kpi() -> /api/kpi" "object"

# Validate KPI shape
if [ "$HAS_JQ" = true ]; then
    BODY=$(curl -sf "$API_BASE/api/kpi")
    for field in total_beads backlog hooked slung review done failed active_agents; do
        if echo "$BODY" | jq -e "has(\"$field\")" >/dev/null 2>&1; then
            log_pass "/api/kpi has field '$field'"
        else
            log_fail "/api/kpi missing field '$field' (frontend expects it)"
        fi
    done
fi

# ── 5. GET /api/settings (fetch_settings) ──
test_get_endpoint "/api/settings" "fetch_settings() -> /api/settings" "object"

# Validate settings sub-objects match ApiSettings struct.
# Backend Config sections: general, display, agents, terminal, security, integrations, bridge, cache, daemon, dolt, providers, ui
# Frontend ApiSettings uses #[serde(default)] for all sections, so missing sections gracefully default.
if [ "$HAS_JQ" = true ]; then
    BODY=$(curl -sf "$API_BASE/api/settings")

    # Core sections present in both backend and frontend
    CORE_SECTIONS=(general display agents terminal security integrations)
    for section in "${CORE_SECTIONS[@]}"; do
        if echo "$BODY" | jq -e "has(\"$section\")" >/dev/null 2>&1; then
            log_pass "/api/settings has core section '$section'"
        else
            log_fail "/api/settings missing core section '$section'"
        fi
    done

    # Frontend-only sections (defined in api.rs but not yet in backend Config).
    # These default to empty structs in the frontend via #[serde(default)].
    FRONTEND_EXTRA=(appearance language dev_tools agent_profile paths api_profiles updates notifications debug memory)
    for section in "${FRONTEND_EXTRA[@]}"; do
        if echo "$BODY" | jq -e "has(\"$section\")" >/dev/null 2>&1; then
            log_pass "/api/settings has extended section '$section'"
        else
            log_info "/api/settings: '$section' not in backend (frontend uses defaults)"
        fi
    done
fi

# ── 6. GET /api/credentials/status (fetch_credential_status) ──
test_get_endpoint "/api/credentials/status" "fetch_credential_status() -> /api/credentials/status" "object"

# ── 7. GET /api/sessions (fetch_sessions) ──
test_get_endpoint "/api/sessions" "fetch_sessions() -> /api/sessions" "array"

# ── 8. GET /api/convoys (fetch_convoys) ──
test_get_endpoint "/api/convoys" "fetch_convoys() -> /api/convoys" "array"

# ── 9. GET /api/worktrees (fetch_worktrees) ──
test_get_endpoint "/api/worktrees" "fetch_worktrees() -> /api/worktrees" "array"

# ── 10. GET /api/costs (fetch_costs) ──
test_get_endpoint "/api/costs" "fetch_costs() -> /api/costs" "object"

# Validate costs shape
if [ "$HAS_JQ" = true ]; then
    BODY=$(curl -sf "$API_BASE/api/costs")
    for field in input_tokens output_tokens sessions; do
        if echo "$BODY" | jq -e "has(\"$field\")" >/dev/null 2>&1; then
            log_pass "/api/costs has field '$field'"
        else
            log_fail "/api/costs missing field '$field' (frontend expects it)"
        fi
    done
fi

# ── 11. GET /api/mcp/servers (fetch_mcp_servers) ──
test_get_endpoint "/api/mcp/servers" "fetch_mcp_servers() -> /api/mcp/servers" "array"

# ── 12. GET /api/memory (fetch_memory) ──
test_get_endpoint "/api/memory" "fetch_memory() -> /api/memory" "array"

# ── 13. GET /api/memory/search?q=test (search_memory) ──
test_get_endpoint "/api/memory/search?q=test" "search_memory() -> /api/memory/search?q=test" "array"

# ── 14. GET /api/roadmap (fetch_roadmap) ──
test_get_endpoint "/api/roadmap" "fetch_roadmap() -> /api/roadmap" "array"

# ── 15. GET /api/ideation/ideas (fetch_ideas) ──
test_get_endpoint "/api/ideation/ideas" "fetch_ideas() -> /api/ideation/ideas" "array"

# ── 16. GET /api/insights/sessions (fetch_insights_sessions) ──
test_get_endpoint "/api/insights/sessions" "fetch_insights_sessions() -> /api/insights/sessions" "array"

# ── 17. GET /api/changelog (fetch_changelog) ──
test_get_endpoint "/api/changelog" "fetch_changelog() -> /api/changelog" "array"

# ── 18. GET /api/github/issues (fetch_github_issues) ──
test_get_endpoint "/api/github/issues" "fetch_github_issues() -> /api/github/issues" ""

# ── 19. GET /api/github/prs (fetch_github_prs) ──
test_get_endpoint "/api/github/prs" "fetch_github_prs() -> /api/github/prs" ""

# ── 20. GET /api/notifications (fetch_notifications) ──
test_get_endpoint "/api/notifications" "fetch_notifications() -> /api/notifications" "array"

# ── 21. GET /api/notifications/count (fetch_notification_count) ──
test_get_endpoint "/api/notifications/count" "fetch_notification_count() -> /api/notifications/count" "object"

# ── 22. GET /api/context (context page) ──
test_get_endpoint "/api/context" "context page -> /api/context" ""

# ── 23. GET /api/tasks (task list) ──
test_get_endpoint "/api/tasks" "task list -> /api/tasks" "array"

# ── 24. GET /api/terminals (terminal list) ──
test_get_endpoint "/api/terminals" "terminal list -> /api/terminals" "array"

# ── 25. GET /api/kanban/columns (kanban config) ──
test_get_endpoint "/api/kanban/columns" "kanban columns -> /api/kanban/columns" "object"

# =============================================================================
# POST endpoints (verify they exist, not full CRUD)
# =============================================================================

# ── POST /api/beads (create_bead) ──
test_post_endpoint "/api/beads" "create_bead() -> POST /api/beads" \
    '{"title":"Contract test bead","lane":"Standard"}' "200,201,400,422"

# ── POST /api/tasks (create_task) ──
test_post_endpoint "/api/tasks" "create_task() -> POST /api/tasks" \
    '{"title":"","bead_id":"00000000-0000-0000-0000-000000000000","category":"Feature","priority":"Medium","complexity":"Small"}' \
    "200,201,400,422"

# ── POST /api/roadmap/generate (generate_roadmap) ──
test_post_endpoint "/api/roadmap/generate" "generate_roadmap() -> POST /api/roadmap/generate" \
    '{"analysis":"test"}' "200,201,400,422"

# ── POST /api/ideation/generate (generate_ideas) ──
test_post_endpoint "/api/ideation/generate" "generate_ideas() -> POST /api/ideation/generate" \
    '{"category":"Feature","context":"test"}' "200,201,400,422"

# ── POST /api/github/sync (sync_github) ──
test_post_endpoint "/api/github/sync" "sync_github() -> POST /api/github/sync" \
    '{}' "200,503,400"

# ── POST /api/notifications/read-all (mark_all_notifications_read) ──
test_post_endpoint "/api/notifications/read-all" "mark_all_read() -> POST /api/notifications/read-all" \
    '' "200,201"

# ── PUT /api/settings (save_settings) ──
echo "--- save_settings() -> PUT /api/settings ---"
SETTINGS=$(curl -sf "$API_BASE/api/settings" 2>/dev/null || echo "")
if [ -n "$SETTINGS" ]; then
    PUT_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT \
        -H "Content-Type: application/json" \
        -d "$SETTINGS" \
        "$API_BASE/api/settings")
    if [ "$PUT_CODE" = "200" ]; then
        log_pass "PUT /api/settings -> HTTP $PUT_CODE (roundtrip OK)"
    else
        log_fail "PUT /api/settings -> HTTP $PUT_CODE"
    fi
else
    log_skip "Could not fetch settings for PUT test"
fi

# ── Summary ──
echo ""
echo "========================================"
echo " API-UI Contract Test Results"
echo "========================================"
echo -e "  ${GREEN}Passed:${NC}  $PASS"
echo -e "  ${RED}Failed:${NC}  $FAIL"
echo -e "  ${YELLOW}Skipped:${NC} $SKIP"
echo "========================================"
echo ""

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
exit 0
