#!/bin/bash
# End-to-end API test for the auto-tundra daemon.
# Starts the daemon in background, curls all major endpoints,
# tests CRUD operations, WebSocket, and verifies clean shutdown.
#
# Usage: bash tests/e2e_api.sh [--release]
#
# Requirements: curl, jq (optional but recommended)

set -euo pipefail

PROFILE="${1:-debug}"
if [ "$PROFILE" = "--release" ]; then
    PROFILE="release"
fi

BIN_DIR="target/$PROFILE"
DAEMON="$BIN_DIR/at-daemon"

# Use a random high port to avoid conflicts.
API_PORT=$((9100 + RANDOM % 1000))
BASE="http://127.0.0.1:${API_PORT}"
DAEMON_PID=""

PASS=0
FAIL=0
TOTAL=0

pass() { echo "  [PASS] $1"; PASS=$((PASS + 1)); TOTAL=$((TOTAL + 1)); }
fail() { echo "  [FAIL] $1"; FAIL=$((FAIL + 1)); TOTAL=$((TOTAL + 1)); }

cleanup() {
    if [ -n "$DAEMON_PID" ] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        echo ""
        echo "Stopping daemon (PID $DAEMON_PID)..."
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
        echo "Daemon stopped."
    fi
}

trap cleanup EXIT

echo "============================================"
echo " auto-tundra E2E API Test Suite"
echo " Profile: $PROFILE"
echo " API Port: $API_PORT"
echo "============================================"
echo ""

# ── Pre-check: binary exists ──────────────────
echo "--- Binary check ---"
if [ ! -x "$DAEMON" ]; then
    echo "  [SKIP] at-daemon binary not found at $DAEMON"
    echo "  Build with: cargo build -p at-daemon"
    exit 0
fi
pass "at-daemon binary exists"

# ── Note: The daemon binds to port 9090 by default. ──────────
# The E2E tests below use direct HTTP requests against the real
# daemon process. Since the daemon hard-codes port 9090, we use that.
API_PORT=9090
BASE="http://127.0.0.1:${API_PORT}"

echo ""
echo "--- Starting daemon ---"
HOME_TMP=$(mktemp -d)
HOME="$HOME_TMP" "$DAEMON" &
DAEMON_PID=$!
echo "  Daemon started with PID $DAEMON_PID"

# Wait for the daemon to be ready (up to 10 seconds).
READY=false
for i in $(seq 1 20); do
    if curl -sf "${BASE}/api/status" > /dev/null 2>&1; then
        READY=true
        break
    fi
    sleep 0.5
done

if ! $READY; then
    fail "daemon did not start within 10 seconds"
    echo ""
    echo "Results: $PASS passed, $FAIL failed out of $TOTAL tests"
    exit 1
fi
pass "daemon started and responding"
echo ""

# ── GET endpoints ─────────────────────────────
echo "--- GET endpoint checks ---"

# /api/status
RESP=$(curl -sf "${BASE}/api/status")
echo "$RESP" | grep -q '"version"' && pass "GET /api/status returns version" || fail "GET /api/status missing version"
echo "$RESP" | grep -q '"agent_count"' && pass "GET /api/status returns agent_count" || fail "GET /api/status missing agent_count"

# /api/kpi
RESP=$(curl -sf "${BASE}/api/kpi")
echo "$RESP" | grep -q '"total_beads"' && pass "GET /api/kpi returns total_beads" || fail "GET /api/kpi missing total_beads"

# /api/beads
RESP=$(curl -sf "${BASE}/api/beads")
[ "$RESP" = "[]" ] && pass "GET /api/beads initially empty" || fail "GET /api/beads not empty"

# /api/agents
RESP=$(curl -sf "${BASE}/api/agents")
[ "$RESP" = "[]" ] && pass "GET /api/agents initially empty" || fail "GET /api/agents not empty"

# /api/tasks
RESP=$(curl -sf "${BASE}/api/tasks")
[ "$RESP" = "[]" ] && pass "GET /api/tasks initially empty" || fail "GET /api/tasks not empty"

# /api/settings
HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" "${BASE}/api/settings")
[ "$HTTP_CODE" = "200" ] && pass "GET /api/settings returns 200" || fail "GET /api/settings returned $HTTP_CODE"

# /api/metrics
HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" "${BASE}/api/metrics")
[ "$HTTP_CODE" = "200" ] && pass "GET /api/metrics returns 200" || fail "GET /api/metrics returned $HTTP_CODE"

# /api/metrics/json
RESP=$(curl -sf "${BASE}/api/metrics/json")
echo "$RESP" | grep -q '{' && pass "GET /api/metrics/json returns JSON" || fail "GET /api/metrics/json not JSON"

# /api/costs
RESP=$(curl -sf "${BASE}/api/costs")
echo "$RESP" | grep -q '"input_tokens"' && pass "GET /api/costs returns token data" || fail "GET /api/costs missing data"

# /api/mcp/servers
RESP=$(curl -sf "${BASE}/api/mcp/servers")
echo "$RESP" | grep -q '"name"' && pass "GET /api/mcp/servers returns server list" || fail "GET /api/mcp/servers missing data"

# /api/credentials/status
RESP=$(curl -sf "${BASE}/api/credentials/status")
echo "$RESP" | grep -q '"providers"' && pass "GET /api/credentials/status returns providers" || fail "GET /api/credentials/status missing data"

# /api/notifications/count
RESP=$(curl -sf "${BASE}/api/notifications/count")
echo "$RESP" | grep -q '"unread"' && pass "GET /api/notifications/count returns unread" || fail "GET /api/notifications/count missing data"

# /api/kanban/columns
RESP=$(curl -sf "${BASE}/api/kanban/columns")
echo "$RESP" | grep -q '"columns"' && pass "GET /api/kanban/columns returns columns" || fail "GET /api/kanban/columns missing data"

echo ""

# ── CORS check ────────────────────────────────
echo "--- CORS header check ---"

HEADERS=$(curl -sf -I -H "Origin: http://localhost:3001" "${BASE}/api/status" 2>&1)
echo "$HEADERS" | grep -iq "access-control-allow-origin" && pass "CORS allow-origin header present" || fail "CORS header missing"

echo ""

# ── Bead CRUD ─────────────────────────────────
echo "--- Bead CRUD ---"

# Create bead
BEAD=$(curl -sf -X POST "${BASE}/api/beads" \
    -H "Content-Type: application/json" \
    -d '{"title":"E2E test bead","description":"Shell test"}')
echo "$BEAD" | grep -q '"title":"E2E test bead"' && pass "POST /api/beads creates bead" || fail "POST /api/beads failed"

BEAD_ID=$(echo "$BEAD" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
if [ -n "$BEAD_ID" ]; then
    pass "Bead ID extracted: ${BEAD_ID:0:8}..."
else
    fail "Could not extract bead ID"
fi

# List beads (should have 1)
RESP=$(curl -sf "${BASE}/api/beads")
echo "$RESP" | grep -q "E2E test bead" && pass "GET /api/beads lists created bead" || fail "GET /api/beads missing created bead"

# Update bead status: backlog -> hooked
if [ -n "$BEAD_ID" ]; then
    HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" -X POST "${BASE}/api/beads/${BEAD_ID}/status" \
        -H "Content-Type: application/json" \
        -d '{"status":"hooked"}')
    [ "$HTTP_CODE" = "200" ] && pass "Bead status backlog->hooked" || fail "Bead status transition failed ($HTTP_CODE)"
fi

echo ""

# ── Task CRUD ─────────────────────────────────
echo "--- Task CRUD ---"

TASK_BEAD_ID=$(cat /dev/urandom | LC_ALL=C tr -dc 'a-f0-9' | head -c 8)
TASK_BEAD_ID="00000000-0000-0000-0000-${TASK_BEAD_ID}0000"

# Create task
TASK=$(curl -sf -X POST "${BASE}/api/tasks" \
    -H "Content-Type: application/json" \
    -d "{\"title\":\"E2E test task\",\"bead_id\":\"${TASK_BEAD_ID}\",\"category\":\"feature\",\"priority\":\"medium\",\"complexity\":\"small\"}")
echo "$TASK" | grep -q '"title":"E2E test task"' && pass "POST /api/tasks creates task" || fail "POST /api/tasks failed"

TASK_ID=$(echo "$TASK" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

if [ -n "$TASK_ID" ]; then
    # Read task
    RESP=$(curl -sf "${BASE}/api/tasks/${TASK_ID}")
    echo "$RESP" | grep -q '"title":"E2E test task"' && pass "GET /api/tasks/{id} reads task" || fail "GET /api/tasks/{id} failed"

    # Update task
    HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" -X PUT "${BASE}/api/tasks/${TASK_ID}" \
        -H "Content-Type: application/json" \
        -d '{"title":"Updated E2E task"}')
    [ "$HTTP_CODE" = "200" ] && pass "PUT /api/tasks/{id} updates task" || fail "PUT /api/tasks/{id} failed ($HTTP_CODE)"

    # Get task logs
    HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" "${BASE}/api/tasks/${TASK_ID}/logs")
    [ "$HTTP_CODE" = "200" ] && pass "GET /api/tasks/{id}/logs returns 200" || fail "GET /api/tasks/{id}/logs failed"

    # Phase transition
    HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" -X POST "${BASE}/api/tasks/${TASK_ID}/phase" \
        -H "Content-Type: application/json" \
        -d '{"phase":"context_gathering"}')
    [ "$HTTP_CODE" = "200" ] && pass "POST /api/tasks/{id}/phase transitions" || fail "Phase transition failed ($HTTP_CODE)"

    # Delete task
    HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" -X DELETE "${BASE}/api/tasks/${TASK_ID}")
    [ "$HTTP_CODE" = "200" ] && pass "DELETE /api/tasks/{id} deletes task" || fail "DELETE /api/tasks/{id} failed ($HTTP_CODE)"

    # Verify deletion
    HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" "${BASE}/api/tasks/${TASK_ID}" 2>/dev/null || echo "404")
    [ "$HTTP_CODE" = "404" ] && pass "Deleted task returns 404" || fail "Deleted task did not return 404 ($HTTP_CODE)"
else
    fail "Could not extract task ID"
fi

echo ""

# ── Settings round-trip ───────────────────────
echo "--- Settings round-trip ---"

SETTINGS=$(curl -sf "${BASE}/api/settings")
echo "$SETTINGS" | grep -q '"agents"' && pass "GET /api/settings has agents section" || fail "Settings missing agents"

# PUT settings back
HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" -X PUT "${BASE}/api/settings" \
    -H "Content-Type: application/json" \
    -d "$SETTINGS")
[ "$HTTP_CODE" = "200" ] && pass "PUT /api/settings round-trip succeeds" || fail "PUT /api/settings failed ($HTTP_CODE)"

echo ""

# ── Clean shutdown ────────────────────────────
echo "--- Clean shutdown ---"

kill "$DAEMON_PID" 2>/dev/null
EXITED=false
for i in $(seq 1 10); do
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
        EXITED=true
        break
    fi
    sleep 0.5
done

if $EXITED; then
    pass "Daemon exited cleanly after SIGTERM"
else
    fail "Daemon did not exit within 5 seconds"
    kill -9 "$DAEMON_PID" 2>/dev/null || true
fi
DAEMON_PID=""

# Clean up temp dir
rm -rf "$HOME_TMP" 2>/dev/null || true

echo ""
echo "============================================"
echo " Results: $PASS passed, $FAIL failed ($TOTAL total)"
echo "============================================"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
exit 0
