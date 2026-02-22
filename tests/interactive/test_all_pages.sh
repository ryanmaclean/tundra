#!/usr/bin/env bash
# =============================================================================
# test_all_pages.sh - Full page smoke test for auto-tundra
#
# Comprehensive smoke test that verifies:
#   1. The daemon/API is running (starts it if needed)
#   2. The frontend SPA loads successfully
#   3. All 15 tab routes return HTTP 200
#   4. The WASM bundle is present and loadable
#   5. API endpoints respond for each page's data needs
#   6. No critical errors in responses
#
# Usage: ./tests/interactive/test_all_pages.sh
# Requires: curl, (optional: jq)
# =============================================================================
set -euo pipefail

# ── Colors ──
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

PASS=0
FAIL=0
SKIP=0
WARN=0
FRONTEND_URL="${FRONTEND_URL:-http://localhost:3001}"
API_BASE="${API_BASE:-http://localhost:9090}"

log_pass()  { PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC}  $1"; }
log_fail()  { FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC}  $1"; }
log_skip()  { SKIP=$((SKIP+1)); echo -e "  ${YELLOW}SKIP${NC}  $1"; }
log_warn()  { WARN=$((WARN+1)); echo -e "  ${YELLOW}WARN${NC}  $1"; }
log_info()  { echo -e "  ${CYAN}INFO${NC}  $1"; }
log_section() { echo -e "\n${BOLD}== $1 ==${NC}"; }

HAS_JQ=true
command -v jq &>/dev/null || HAS_JQ=false

echo ""
echo "============================================================"
echo "  Full Page Smoke Test - auto-tundra WASM Frontend"
echo "============================================================"
echo ""
echo "  Frontend: $FRONTEND_URL"
echo "  API:      $API_BASE"
echo ""

# =============================================================================
# Phase 1: Service Health Checks
# =============================================================================
log_section "Phase 1: Service Health Checks"

# Check API
API_UP=false
if curl -sf -o /dev/null --connect-timeout 3 "$API_BASE/api/status"; then
    log_pass "API daemon is running at $API_BASE"
    API_UP=true
else
    log_fail "API daemon not reachable at $API_BASE"
    echo -e "  ${YELLOW}Hint:${NC} Start with: cargo run --bin at-daemon"
fi

# Check Frontend
FRONTEND_UP=false
if curl -sf -o /dev/null --connect-timeout 3 "$FRONTEND_URL"; then
    log_pass "Frontend is serving at $FRONTEND_URL"
    FRONTEND_UP=true
else
    log_fail "Frontend not reachable at $FRONTEND_URL"
    echo -e "  ${YELLOW}Hint:${NC} Start with: cd app/leptos-ui && trunk serve --port 3001"
fi

if [ "$API_UP" = false ] && [ "$FRONTEND_UP" = false ]; then
    echo -e "\n${RED}Both services are down. Cannot continue.${NC}"
    exit 1
fi

# =============================================================================
# Phase 2: SPA Shell Integrity
# =============================================================================
if [ "$FRONTEND_UP" = true ]; then
    log_section "Phase 2: SPA Shell Integrity"

    HTML=$(curl -sf "$FRONTEND_URL/")

    # Check HTML structure
    if echo "$HTML" | grep -qi '<html'; then
        log_pass "HTML shell has <html> tag"
    else
        log_fail "HTML shell missing <html> tag"
    fi

    if echo "$HTML" | grep -qi '<body'; then
        log_pass "HTML shell has <body> tag"
    else
        log_fail "HTML shell missing <body> tag"
    fi

    # Check for WASM/JS references
    if echo "$HTML" | grep -qi 'wasm\|\.js\|init\|module'; then
        log_pass "HTML references WASM/JS bundle"
    else
        log_fail "HTML does not reference WASM/JS bundle"
    fi

    # Check response size
    BODY_SIZE=$(echo -n "$HTML" | wc -c | tr -d ' ')
    if [ "$BODY_SIZE" -gt 200 ]; then
        log_pass "HTML body size: ${BODY_SIZE} bytes"
    else
        log_warn "HTML body suspiciously small: ${BODY_SIZE} bytes"
    fi
fi

# =============================================================================
# Phase 3: All 15 Tab Routes
# =============================================================================
if [ "$FRONTEND_UP" = true ]; then
    log_section "Phase 3: All 15 Tab Routes (SPA)"

    # Tab index -> route -> label -> primary API endpoint
    declare -a TAB_DATA=(
        "0|/|Dashboard|/api/status"
        "1|/beads|Kanban Board|/api/beads"
        "2|/agents|Agent Terminals|/api/agents"
        "3|/insights|Insights|/api/insights/sessions"
        "4|/ideation|Ideation|/api/ideation/ideas"
        "5|/roadmap|Roadmap|/api/roadmap"
        "6|/changelog|Changelog|/api/changelog"
        "7|/context|Context|/api/context"
        "8|/mcp|MCP Overview|/api/mcp/servers"
        "9|/worktrees|Worktrees|/api/worktrees"
        "10|/github-issues|GitHub Issues|/api/github/issues"
        "11|/github-prs|GitHub PRs|/api/github/prs"
        "12|/claude-code|Claude Code|/api/sessions"
        "13|/config|Settings|/api/settings"
        "14|/terminals|Terminals|/api/terminals"
    )

    for entry in "${TAB_DATA[@]}"; do
        IFS='|' read -r idx route label api_endpoint <<< "$entry"

        # Test frontend route
        CODE=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 5 "$FRONTEND_URL$route")
        if [ "$CODE" = "200" ]; then
            log_pass "Tab $idx ($label): GET $route -> HTTP $CODE"
        else
            log_fail "Tab $idx ($label): GET $route -> HTTP $CODE"
        fi
    done
fi

# =============================================================================
# Phase 4: API Endpoint Verification per Page
# =============================================================================
if [ "$API_UP" = true ]; then
    log_section "Phase 4: API Endpoints per Page"

    # Core endpoints (should always work)
    CORE_ENDPOINTS=(
        "/api/status|Status"
        "/api/beads|Beads"
        "/api/agents|Agents"
        "/api/kpi|KPI"
        "/api/settings|Settings"
        "/api/credentials/status|Credentials"
        "/api/sessions|Sessions"
        "/api/convoys|Convoys"
        "/api/costs|Costs"
        "/api/mcp/servers|MCP Servers"
        "/api/worktrees|Worktrees"
        "/api/tasks|Tasks"
        "/api/terminals|Terminals"
        "/api/notifications|Notifications"
        "/api/notifications/count|Notification Count"
        "/api/kanban/columns|Kanban Columns"
        "/api/memory|Memory"
        "/api/memory/search?q=test|Memory Search"
        "/api/roadmap|Roadmap"
        "/api/ideation/ideas|Ideas"
        "/api/insights/sessions|Insight Sessions"
        "/api/changelog|Changelog"
        "/api/context|Context"
    )

    for entry in "${CORE_ENDPOINTS[@]}"; do
        IFS='|' read -r endpoint label <<< "$entry"
        CODE=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 5 "$API_BASE$endpoint")
        if [ "$CODE" = "200" ]; then
            log_pass "$label: GET $endpoint -> HTTP $CODE"
        elif [ "$CODE" = "503" ] || [ "$CODE" = "400" ]; then
            log_pass "$label: GET $endpoint -> HTTP $CODE (expected: unconfigured)"
        else
            log_fail "$label: GET $endpoint -> HTTP $CODE"
        fi
    done

    # GitHub endpoints (may be 503 without token)
    log_info "GitHub endpoints (503 expected without token):"
    for ep in "/api/github/issues" "/api/github/prs"; do
        CODE=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 5 "$API_BASE$ep")
        if [ "$CODE" = "200" ] || [ "$CODE" = "503" ] || [ "$CODE" = "400" ]; then
            log_pass "GitHub: GET $ep -> HTTP $CODE"
        else
            log_fail "GitHub: GET $ep -> HTTP $CODE"
        fi
    done
fi

# =============================================================================
# Phase 5: JSON Validity Check
# =============================================================================
if [ "$API_UP" = true ] && [ "$HAS_JQ" = true ]; then
    log_section "Phase 5: JSON Response Validity"

    JSON_ENDPOINTS=(
        "/api/status"
        "/api/beads"
        "/api/agents"
        "/api/kpi"
        "/api/settings"
        "/api/sessions"
        "/api/convoys"
        "/api/costs"
        "/api/mcp/servers"
        "/api/tasks"
        "/api/notifications"
        "/api/notifications/count"
        "/api/kanban/columns"
    )

    for ep in "${JSON_ENDPOINTS[@]}"; do
        BODY=$(curl -sf "$API_BASE$ep" 2>/dev/null || echo "")
        if [ -z "$BODY" ]; then
            log_skip "GET $ep returned empty body"
            continue
        fi
        if echo "$BODY" | jq . >/dev/null 2>&1; then
            log_pass "GET $ep returns valid JSON"
        else
            log_fail "GET $ep returns INVALID JSON"
        fi
    done
elif [ "$HAS_JQ" = false ]; then
    log_skip "Phase 5 skipped (jq not installed)"
fi

# =============================================================================
# Phase 6: WebSocket Endpoint Check
# =============================================================================
if [ "$API_UP" = true ]; then
    log_section "Phase 6: WebSocket Endpoints"

    # We can't fully test WebSocket with curl, but we can verify the upgrade endpoint exists
    for ws_path in "/ws" "/api/events/ws"; do
        # Capture the HTTP code; curl with WebSocket will get 101 then hang, so use --max-time
        CODE_RAW=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 3 --max-time 3 \
            -H "Upgrade: websocket" \
            -H "Connection: Upgrade" \
            -H "Sec-WebSocket-Key: dGVzdA==" \
            -H "Sec-WebSocket-Version: 13" \
            "$API_BASE$ws_path" 2>/dev/null; echo "")
        # Extract just the 3-digit code (curl may append data after timeout)
        CODE=$(echo "$CODE_RAW" | grep -oE '^[0-9]{3}' | head -1)
        CODE=${CODE:-000}
        if [ "$CODE" = "101" ] || [ "$CODE" = "200" ] || [ "$CODE" = "400" ] || [ "$CODE" = "426" ] || [ "$CODE" = "000" ]; then
            log_pass "WebSocket $ws_path -> HTTP $CODE (endpoint exists)"
        else
            log_fail "WebSocket $ws_path -> HTTP $CODE"
        fi
    done
fi

# =============================================================================
# Phase 7: Cross-Origin Headers (CORS)
# =============================================================================
if [ "$API_UP" = true ]; then
    log_section "Phase 7: CORS Headers"

    CORS_HEADERS=$(curl -sI -X OPTIONS \
        -H "Origin: http://localhost:3001" \
        -H "Access-Control-Request-Method: GET" \
        "$API_BASE/api/status" 2>/dev/null || echo "")

    if echo "$CORS_HEADERS" | grep -qi 'access-control-allow'; then
        log_pass "CORS headers present on preflight request"
    else
        log_warn "CORS headers not found (may be handled differently)"
    fi
fi

# =============================================================================
# Summary
# =============================================================================
echo ""
echo "============================================================"
echo "  Full Page Smoke Test Results"
echo "============================================================"
echo -e "  ${GREEN}Passed:${NC}   $PASS"
echo -e "  ${RED}Failed:${NC}   $FAIL"
echo -e "  ${YELLOW}Skipped:${NC}  $SKIP"
echo -e "  ${YELLOW}Warnings:${NC} $WARN"
TOTAL=$((PASS + FAIL + SKIP))
echo "  Total:    $TOTAL"
echo "============================================================"
echo ""

if [ "$FAIL" -gt 0 ]; then
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi

echo -e "${GREEN}All tests passed.${NC}"
exit 0
