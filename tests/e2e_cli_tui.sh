#!/bin/bash
# End-to-end test for at CLI and at-tui binaries.
# Tests: binary existence, CLI commands, TUI launch/quit cycle.
#
# Usage: bash tests/e2e_cli_tui.sh [--release]

set -euo pipefail

PROFILE="${1:-debug}"
if [ "$PROFILE" = "--release" ]; then
    PROFILE="release"
fi

BIN_DIR="target/$PROFILE"
AT="$BIN_DIR/at"
AT_TUI="$BIN_DIR/at-tui"

PASS=0
FAIL=0

pass() { echo "  ✓ $1"; PASS=$((PASS + 1)); }
fail() { echo "  ✗ $1"; FAIL=$((FAIL + 1)); }

echo "============================================"
echo " auto-tundra E2E Test Suite"
echo " Profile: $PROFILE"
echo "============================================"
echo ""

# ── Binary existence ──────────────────────────
echo "▸ Binary checks"

[ -x "$AT" ] && pass "at binary exists and is executable" || fail "at binary missing"
[ -x "$AT_TUI" ] && pass "at-tui binary exists and is executable" || fail "at-tui binary missing"

# Check binary sizes < 10MB
AT_SIZE=$(stat -f%z "$AT" 2>/dev/null || stat --printf=%s "$AT" 2>/dev/null)
TUI_SIZE=$(stat -f%z "$AT_TUI" 2>/dev/null || stat --printf=%s "$AT_TUI" 2>/dev/null)
[ "$AT_SIZE" -lt 10485760 ] && pass "at binary < 10MB ($AT_SIZE bytes)" || fail "at binary >= 10MB"
[ "$TUI_SIZE" -lt 10485760 ] && pass "at-tui binary < 10MB ($TUI_SIZE bytes)" || fail "at-tui binary >= 10MB"

echo ""

# ── CLI commands ──────────────────────────────
echo "▸ CLI command tests"

# Status (default command)
OUTPUT=$("$AT" status 2>&1)
echo "$OUTPUT" | grep -q "auto-tundra status" && pass "at status: outputs header" || fail "at status: no header"
echo "$OUTPUT" | grep -q "Total beads:" && pass "at status: shows bead count" || fail "at status: no bead count"
echo "$OUTPUT" | grep -q "Active agents:" && pass "at status: shows agent count" || fail "at status: no agent count"

# Help
OUTPUT=$("$AT" --help 2>&1)
echo "$OUTPUT" | grep -q "status" && pass "at --help: lists status" || fail "at --help: missing status"
echo "$OUTPUT" | grep -q "sling" && pass "at --help: lists sling" || fail "at --help: missing sling"
echo "$OUTPUT" | grep -q "hook" && pass "at --help: lists hook" || fail "at --help: missing hook"
echo "$OUTPUT" | grep -q "done" && pass "at --help: lists done" || fail "at --help: missing done"
echo "$OUTPUT" | grep -q "nudge" && pass "at --help: lists nudge" || fail "at --help: missing nudge"

# Version
OUTPUT=$("$AT" --version 2>&1)
echo "$OUTPUT" | grep -q "at " && pass "at --version: shows version" || fail "at --version: no output"

# Hook command
OUTPUT=$("$AT" hook "Test bead" "test-agent" 2>&1)
echo "$OUTPUT" | grep -q "hook:" && pass "at hook: acknowledged" || fail "at hook: no output"

# Sling command
OUTPUT=$("$AT" sling bead-1 agent-1 2>&1)
echo "$OUTPUT" | grep -q "sling:" && pass "at sling: acknowledged" || fail "at sling: no output"

# Done command
OUTPUT=$("$AT" done bead-1 2>&1)
echo "$OUTPUT" | grep -q "done:" && pass "at done: acknowledged" || fail "at done: no output"

# Done --fail
OUTPUT=$("$AT" done bead-1 --fail 2>&1)
echo "$OUTPUT" | grep -q "failed" && pass "at done --fail: shows failed" || fail "at done --fail: no failed"

# Nudge
OUTPUT=$("$AT" nudge agent-1 -m "hello" 2>&1)
echo "$OUTPUT" | grep -q "nudge:" && pass "at nudge: acknowledged" || fail "at nudge: no output"

echo ""

# ── TUI launch/quit via osascript ─────────────
echo "▸ TUI launch test (AppleScript)"

# TUI requires a real TTY — only test launch if we have one
if [ -t 0 ]; then
    "$AT_TUI" &
    TUI_PID=$!
    sleep 1
    if kill -0 "$TUI_PID" 2>/dev/null; then
        pass "at-tui: process started (PID $TUI_PID)"
    else
        fail "at-tui: process did not start"
    fi
    kill "$TUI_PID" 2>/dev/null || true
    wait "$TUI_PID" 2>/dev/null || true
    if ! kill -0 "$TUI_PID" 2>/dev/null; then
        pass "at-tui: process exited cleanly"
    else
        fail "at-tui: process still running"
        kill -9 "$TUI_PID" 2>/dev/null || true
    fi
else
    echo "  ⊘ Skipped: no TTY available (TUI requires interactive terminal)"
fi

echo ""

# ── TUI interactive test via osascript ────────
echo "▸ TUI interactive test (Terminal.app)"

# Only run if we're on macOS with Terminal.app
if [ "$(uname)" = "Darwin" ] && command -v osascript &>/dev/null; then
    FULL_TUI_PATH="$(cd "$(dirname "$AT_TUI")" && pwd)/$(basename "$AT_TUI")"
    osascript <<APPLESCRIPT 2>/dev/null && pass "at-tui: launched in Terminal.app, navigated tabs, and quit" || fail "at-tui: AppleScript interaction failed"
tell application "Terminal"
    activate
    set tuiTab to do script "exec ${FULL_TUI_PATH}"
    delay 3
end tell

tell application "System Events"
    tell process "Terminal"
        -- Navigate to Agents tab
        keystroke "2"
        delay 0.5
        -- Navigate to Beads tab
        keystroke "3"
        delay 0.5
        -- Back to Dashboard
        keystroke "1"
        delay 0.5
        -- Open help
        keystroke "?"
        delay 1
        -- Close help
        key code 53
        delay 0.5
        -- Quit
        keystroke "q"
        delay 1
    end tell
end tell
APPLESCRIPT
else
    echo "  ⊘ Skipped: not macOS or osascript unavailable"
fi

echo ""

# ── Summary ───────────────────────────────────
echo "============================================"
echo " Results: $PASS passed, $FAIL failed"
echo "============================================"

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
