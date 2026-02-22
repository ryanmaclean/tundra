#!/usr/bin/env bash
# =============================================================================
# test_settings_roundtrip.sh - Settings E2E roundtrip test for auto-tundra
#
# Tests:
#   1. GET current settings and verify structure
#   2. PUT modified settings and verify persistence
#   3. GET again to confirm roundtrip
#   4. PATCH partial settings and verify merge
#   5. Verify all settings sections that the frontend expects exist
#   6. Restore original settings
#
# Usage: ./tests/interactive/test_settings_roundtrip.sh
# Requires: curl, jq, the API serving at localhost:9090
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
API_BASE="${API_BASE:-http://localhost:9090}"

log_pass()  { PASS=$((PASS+1)); echo -e "  ${GREEN}PASS${NC} $1"; }
log_fail()  { FAIL=$((FAIL+1)); echo -e "  ${RED}FAIL${NC} $1"; }
log_skip()  { SKIP=$((SKIP+1)); echo -e "  ${YELLOW}SKIP${NC} $1"; }
log_info()  { echo -e "  ${CYAN}INFO${NC} $1"; }
log_section() { echo -e "\n${BOLD}== $1 ==${NC}"; }

# Check jq
if ! command -v jq &>/dev/null; then
    echo -e "${RED}ERROR:${NC} jq is required for settings roundtrip tests"
    echo "  Install: brew install jq (macOS) or apt install jq (Linux)"
    exit 1
fi

echo ""
echo "============================================================"
echo "  Settings Roundtrip Tests - auto-tundra"
echo "============================================================"
echo ""

# ── Check if API is reachable ──
if ! curl -sf -o /dev/null --connect-timeout 3 "$API_BASE/api/status"; then
    echo -e "${RED}ERROR:${NC} API not reachable at $API_BASE"
    exit 1
fi
log_info "API is reachable at $API_BASE"

# =============================================================================
# Phase 1: GET current settings and verify structure
# =============================================================================
log_section "Phase 1: GET Settings Structure"

ORIGINAL_SETTINGS=$(curl -sf "$API_BASE/api/settings")
if [ -z "$ORIGINAL_SETTINGS" ]; then
    echo -e "${RED}ERROR:${NC} GET /api/settings returned empty body"
    exit 1
fi

if echo "$ORIGINAL_SETTINGS" | jq . >/dev/null 2>&1; then
    log_pass "GET /api/settings returns valid JSON"
else
    log_fail "GET /api/settings returns invalid JSON"
    exit 1
fi

# Verify settings sections.
# Backend Config has: general, display, agents, terminal, security, integrations, bridge, cache, daemon, dolt, providers, ui
# Frontend ApiSettings adds more with #[serde(default)] for graceful degradation.
CORE_SECTIONS=(general display agents terminal security integrations)
FRONTEND_EXTRA=(appearance language dev_tools agent_profile paths api_profiles updates notifications debug memory)

for section in "${CORE_SECTIONS[@]}"; do
    if echo "$ORIGINAL_SETTINGS" | jq -e "has(\"$section\")" >/dev/null 2>&1; then
        log_pass "Settings has core section: $section"
    else
        log_fail "Settings missing core section: $section"
    fi
done

for section in "${FRONTEND_EXTRA[@]}"; do
    if echo "$ORIGINAL_SETTINGS" | jq -e "has(\"$section\")" >/dev/null 2>&1; then
        log_pass "Settings has extended section: $section"
    else
        log_info "Settings: '$section' not in backend (frontend uses serde defaults)"
    fi
done

# =============================================================================
# Phase 2: Verify specific field shapes within each section
# =============================================================================
log_section "Phase 2: Field Shape Validation"

# general
for field in project_name log_level; do
    if echo "$ORIGINAL_SETTINGS" | jq -e ".general | has(\"$field\")" >/dev/null 2>&1; then
        log_pass "general.$field exists"
    else
        log_fail "general.$field missing"
    fi
done

# display
for field in theme font_size compact_mode; do
    if echo "$ORIGINAL_SETTINGS" | jq -e ".display | has(\"$field\")" >/dev/null 2>&1; then
        log_pass "display.$field exists"
    else
        log_fail "display.$field missing"
    fi
done

# agents
for field in max_concurrent heartbeat_interval_secs auto_restart; do
    if echo "$ORIGINAL_SETTINGS" | jq -e ".agents | has(\"$field\")" >/dev/null 2>&1; then
        log_pass "agents.$field exists"
    else
        log_fail "agents.$field missing"
    fi
done

# security
for field in allow_shell_exec sandbox allowed_paths; do
    if echo "$ORIGINAL_SETTINGS" | jq -e ".security | has(\"$field\")" >/dev/null 2>&1; then
        log_pass "security.$field exists"
    else
        log_fail "security.$field missing"
    fi
done

# integrations
for field in github_token_env; do
    if echo "$ORIGINAL_SETTINGS" | jq -e ".integrations | has(\"$field\")" >/dev/null 2>&1; then
        log_pass "integrations.$field exists"
    else
        log_fail "integrations.$field missing"
    fi
done

# notifications (frontend-only section, may not exist in backend)
if echo "$ORIGINAL_SETTINGS" | jq -e 'has("notifications")' >/dev/null 2>&1; then
    for field in on_task_complete on_task_failed on_review_needed sound_enabled; do
        if echo "$ORIGINAL_SETTINGS" | jq -e ".notifications | has(\"$field\")" >/dev/null 2>&1; then
            log_pass "notifications.$field exists"
        else
            log_fail "notifications.$field missing"
        fi
    done
else
    log_info "notifications section not in backend (frontend uses serde defaults)"
fi

# memory (frontend-only section, may not exist in backend)
if echo "$ORIGINAL_SETTINGS" | jq -e 'has("memory")' >/dev/null 2>&1; then
    for field in enable_memory graphiti_server_url; do
        if echo "$ORIGINAL_SETTINGS" | jq -e ".memory | has(\"$field\")" >/dev/null 2>&1; then
            log_pass "memory.$field exists"
        else
            log_fail "memory.$field missing"
        fi
    done
else
    log_info "memory section not in backend (frontend uses serde defaults)"
fi

# =============================================================================
# Phase 3: PUT roundtrip (modify and verify)
# =============================================================================
log_section "Phase 3: PUT Settings Roundtrip"

# Create modified settings: change fields that exist in the backend Config
MODIFIED_SETTINGS=$(echo "$ORIGINAL_SETTINGS" | jq '
    .general.project_name = "roundtrip-test-project" |
    .display.compact_mode = true |
    .display.font_size = 16
')

# PUT the modified settings
PUT_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT \
    -H "Content-Type: application/json" \
    -d "$MODIFIED_SETTINGS" \
    "$API_BASE/api/settings")

if [ "$PUT_CODE" = "200" ]; then
    log_pass "PUT /api/settings -> HTTP $PUT_CODE"
else
    log_fail "PUT /api/settings -> HTTP $PUT_CODE (expected 200)"
fi

# GET and verify the changes persisted
AFTER_PUT=$(curl -sf "$API_BASE/api/settings")

PROJECT_NAME=$(echo "$AFTER_PUT" | jq -r '.general.project_name')
if [ "$PROJECT_NAME" = "roundtrip-test-project" ]; then
    log_pass "Roundtrip: general.project_name = '$PROJECT_NAME'"
else
    log_fail "Roundtrip: general.project_name = '$PROJECT_NAME' (expected 'roundtrip-test-project')"
fi

COMPACT=$(echo "$AFTER_PUT" | jq -r '.display.compact_mode')
if [ "$COMPACT" = "true" ]; then
    log_pass "Roundtrip: display.compact_mode = $COMPACT"
else
    log_fail "Roundtrip: display.compact_mode = $COMPACT (expected true)"
fi

FONT_SIZE=$(echo "$AFTER_PUT" | jq -r '.display.font_size')
if [ "$FONT_SIZE" = "16" ]; then
    log_pass "Roundtrip: display.font_size = $FONT_SIZE"
else
    log_fail "Roundtrip: display.font_size = $FONT_SIZE (expected 16)"
fi

# =============================================================================
# Phase 4: PATCH partial settings
# =============================================================================
log_section "Phase 4: PATCH Partial Settings"

PATCH_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PATCH \
    -H "Content-Type: application/json" \
    -d '{"display": {"theme": "solarized"}, "general": {"log_level": "debug"}}' \
    "$API_BASE/api/settings")

if [ "$PATCH_CODE" = "200" ]; then
    log_pass "PATCH /api/settings -> HTTP $PATCH_CODE"
else
    log_fail "PATCH /api/settings -> HTTP $PATCH_CODE (expected 200)"
fi

# Verify patched fields changed
AFTER_PATCH=$(curl -sf "$API_BASE/api/settings")

THEME=$(echo "$AFTER_PATCH" | jq -r '.display.theme')
if [ "$THEME" = "solarized" ]; then
    log_pass "Patch: display.theme = '$THEME'"
else
    log_fail "Patch: display.theme = '$THEME' (expected 'solarized')"
fi

LOG_LEVEL=$(echo "$AFTER_PATCH" | jq -r '.general.log_level')
if [ "$LOG_LEVEL" = "debug" ]; then
    log_pass "Patch: general.log_level = '$LOG_LEVEL'"
else
    log_fail "Patch: general.log_level = '$LOG_LEVEL' (expected 'debug')"
fi

# Verify non-patched fields are preserved
PATCHED_PROJECT=$(echo "$AFTER_PATCH" | jq -r '.general.project_name')
if [ "$PATCHED_PROJECT" = "roundtrip-test-project" ]; then
    log_pass "Patch preserved: general.project_name = '$PATCHED_PROJECT'"
else
    log_fail "Patch corrupted: general.project_name = '$PATCHED_PROJECT' (expected 'roundtrip-test-project')"
fi

PATCHED_COMPACT=$(echo "$AFTER_PATCH" | jq -r '.display.compact_mode')
if [ "$PATCHED_COMPACT" = "true" ]; then
    log_pass "Patch preserved: display.compact_mode = $PATCHED_COMPACT"
else
    log_fail "Patch corrupted: display.compact_mode = $PATCHED_COMPACT (expected true from PUT)"
fi

# =============================================================================
# Phase 5: Credential status endpoint
# =============================================================================
log_section "Phase 5: Credential Status"

CRED_RESP=$(curl -sf "$API_BASE/api/credentials/status")
if echo "$CRED_RESP" | jq -e 'has("providers")' >/dev/null 2>&1; then
    log_pass "credentials/status has 'providers' field"
else
    log_fail "credentials/status missing 'providers' field"
fi

if echo "$CRED_RESP" | jq -e 'has("daemon_auth")' >/dev/null 2>&1; then
    log_pass "credentials/status has 'daemon_auth' field"
else
    log_fail "credentials/status missing 'daemon_auth' field"
fi

PROVIDERS_TYPE=$(echo "$CRED_RESP" | jq -r '.providers | type')
if [ "$PROVIDERS_TYPE" = "array" ]; then
    log_pass "credentials/status.providers is an array"
else
    log_fail "credentials/status.providers type = $PROVIDERS_TYPE (expected array)"
fi

# =============================================================================
# Phase 6: Restore original settings
# =============================================================================
log_section "Phase 6: Restore Original Settings"

RESTORE_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT \
    -H "Content-Type: application/json" \
    -d "$ORIGINAL_SETTINGS" \
    "$API_BASE/api/settings")

if [ "$RESTORE_CODE" = "200" ]; then
    log_pass "Original settings restored"
else
    log_fail "Failed to restore original settings (HTTP $RESTORE_CODE)"
fi

# Verify restoration
RESTORED=$(curl -sf "$API_BASE/api/settings")
ORIG_NAME=$(echo "$ORIGINAL_SETTINGS" | jq -r '.general.project_name')
REST_NAME=$(echo "$RESTORED" | jq -r '.general.project_name')

if [ "$ORIG_NAME" = "$REST_NAME" ]; then
    log_pass "Verified restoration: project_name matches original"
else
    log_fail "Restoration mismatch: got '$REST_NAME', expected '$ORIG_NAME'"
fi

# =============================================================================
# Summary
# =============================================================================
echo ""
echo "============================================================"
echo "  Settings Roundtrip Test Results"
echo "============================================================"
echo -e "  ${GREEN}Passed:${NC}  $PASS"
echo -e "  ${RED}Failed:${NC}  $FAIL"
echo -e "  ${YELLOW}Skipped:${NC} $SKIP"
echo "============================================================"
echo ""

if [ "$FAIL" -gt 0 ]; then
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi

echo -e "${GREEN}All tests passed.${NC}"
exit 0
