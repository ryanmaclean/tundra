#!/usr/bin/env bash
# =============================================================================
# test_navigation.sh - Navigation tests for auto-tundra Leptos WASM frontend
#
# Verifies that the SPA shell loads correctly, the WASM bundle is referenced,
# and the application HTML structure is present. Since this is a CSR SPA,
# all routes return the same HTML shell; tests verify the shell integrity.
#
# Usage: ./tests/interactive/test_navigation.sh
# Requires: curl, the frontend serving at localhost:3001
# =============================================================================
set -euo pipefail

# ── Colors ──
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

PASS=0
FAIL=0
SKIP=0
FRONTEND_URL="${FRONTEND_URL:-http://localhost:3001}"

log_pass() { PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $1"; }
log_fail() { FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $1"; }
log_skip() { SKIP=$((SKIP+1)); echo -e "  ${YELLOW}SKIP${NC} $1"; }
log_info() { echo -e "  ${CYAN}INFO${NC} $1"; }

echo ""
echo "========================================"
echo " Navigation Tests - auto-tundra WASM UI"
echo "========================================"
echo ""

# ── Check if frontend is reachable ──
if ! curl -sf -o /dev/null --connect-timeout 3 "$FRONTEND_URL"; then
    echo -e "${RED}ERROR:${NC} Frontend not reachable at $FRONTEND_URL"
    echo "  Start the frontend first: trunk serve --port 3001"
    echo ""
    exit 1
fi
log_info "Frontend is reachable at $FRONTEND_URL"
echo ""

# ── Test 1: Root page returns HTTP 200 ──
echo "--- Test: Root page returns HTTP 200 ---"
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "$FRONTEND_URL/")
if [ "$HTTP_CODE" = "200" ]; then
    log_pass "GET / returned HTTP $HTTP_CODE"
else
    log_fail "GET / returned HTTP $HTTP_CODE (expected 200)"
fi

# ── Test 2: HTML contains WASM bootstrap ──
echo "--- Test: WASM bundle referenced in HTML ---"
HTML=$(curl -sf "$FRONTEND_URL/")
if echo "$HTML" | grep -qi 'wasm\|\.js\|at.leptos.ui\|at_leptos_ui\|init\|module'; then
    log_pass "HTML references WASM/JS module"
else
    log_fail "HTML does not reference WASM/JS module"
fi

# ── Test 3: HTML is valid (has <html>, <head>, <body>) ──
echo "--- Test: HTML structure is valid ---"
if echo "$HTML" | grep -qi '<html' && echo "$HTML" | grep -qi '<body'; then
    log_pass "HTML has proper structure (<html>, <body>)"
else
    log_fail "HTML missing proper structure"
fi

# ── Test 4: All SPA routes return 200 (same shell) ──
echo "--- Test: All 15 tab routes return HTTP 200 ---"
ROUTES=(
    "/"
    "/beads"
    "/agents"
    "/insights"
    "/ideation"
    "/roadmap"
    "/changelog"
    "/context"
    "/mcp"
    "/worktrees"
    "/github-issues"
    "/github-prs"
    "/claude-code"
    "/config"
    "/terminals"
)

ALL_ROUTES_OK=true
for route in "${ROUTES[@]}"; do
    CODE=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 5 "$FRONTEND_URL$route")
    if [ "$CODE" = "200" ]; then
        log_pass "GET $route -> HTTP $CODE"
    else
        log_fail "GET $route -> HTTP $CODE (expected 200)"
        ALL_ROUTES_OK=false
    fi
done

# ── Test 5: All routes return the same HTML shell (SPA behavior) ──
echo "--- Test: SPA routes return identical shell ---"
ROOT_HTML=$(curl -sf "$FRONTEND_URL/")
ROOT_HASH=$(echo "$ROOT_HTML" | md5sum 2>/dev/null || echo "$ROOT_HTML" | md5 2>/dev/null || echo "skip")

if [ "$ROOT_HASH" = "skip" ]; then
    log_skip "md5sum/md5 not available for hash comparison"
else
    SPA_CONSISTENT=true
    for route in "/beads" "/config" "/insights"; do
        ROUTE_HTML=$(curl -sf "$FRONTEND_URL$route")
        ROUTE_HASH=$(echo "$ROUTE_HTML" | md5sum 2>/dev/null || echo "$ROUTE_HTML" | md5 2>/dev/null)
        if [ "$ROOT_HASH" != "$ROUTE_HASH" ]; then
            log_fail "GET $route returned different HTML than / (not SPA behavior)"
            SPA_CONSISTENT=false
        fi
    done
    if [ "$SPA_CONSISTENT" = true ]; then
        log_pass "All checked routes return identical HTML shell (SPA confirmed)"
    fi
fi

# ── Test 6: Content-Type is text/html ──
echo "--- Test: Content-Type is text/html ---"
CONTENT_TYPE=$(curl -s -o /dev/null -w "%{content_type}" "$FRONTEND_URL/")
if echo "$CONTENT_TYPE" | grep -qi 'text/html'; then
    log_pass "Content-Type is text/html: $CONTENT_TYPE"
else
    log_fail "Content-Type is $CONTENT_TYPE (expected text/html)"
fi

# ── Test 7: Response is not empty ──
echo "--- Test: Response body is non-empty ---"
BODY_SIZE=$(curl -sf "$FRONTEND_URL/" | wc -c | tr -d ' ')
if [ "$BODY_SIZE" -gt 100 ]; then
    log_pass "Response body size: $BODY_SIZE bytes"
else
    log_fail "Response body too small: $BODY_SIZE bytes"
fi

# ── Test 8: 404 handling for unknown routes ──
echo "--- Test: Unknown routes handled gracefully ---"
CODE_404=$(curl -s -o /dev/null -w "%{http_code}" "$FRONTEND_URL/nonexistent-route-xyz")
if [ "$CODE_404" = "200" ] || [ "$CODE_404" = "404" ]; then
    log_pass "GET /nonexistent-route-xyz -> HTTP $CODE_404 (SPA handles or 404)"
else
    log_fail "GET /nonexistent-route-xyz -> HTTP $CODE_404 (unexpected)"
fi

# ── Summary ──
echo ""
echo "========================================"
echo " Navigation Test Results"
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
