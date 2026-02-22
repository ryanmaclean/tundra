#!/usr/bin/env bash
# =============================================================================
# test_native_shell_mode.sh - Native shell prototype verification (macOS/Tauri)
#
# Verifies that native-shell prototype hooks remain wired:
#   1) Tauri bootstrap respects AT_NATIVE_SHELL_MACOS
#   2) Runtime init script injects native-shell flags into webview
#   3) Leptos CSS includes data-native-shell selectors
#
# Usage:
#   ./tests/interactive/test_native_shell_mode.sh
# =============================================================================
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

PASS=0
FAIL=0

log_pass() { PASS=$((PASS + 1)); echo -e "  ${GREEN}PASS${NC}  $1"; }
log_fail() { FAIL=$((FAIL + 1)); echo -e "  ${RED}FAIL${NC}  $1"; }
log_info() { echo -e "  ${YELLOW}INFO${NC}  $1"; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TAURI_MAIN="$ROOT/app/tauri/src/main.rs"
STYLE_CSS="$ROOT/app/leptos-ui/style.css"

echo ""
echo "============================================================"
echo "  Native Shell Prototype Verification"
echo "============================================================"
echo ""
echo "  repo: $ROOT"
echo ""

echo -e "${BOLD}== Tauri bootstrap hooks ==${NC}"
if rg -q 'AT_NATIVE_SHELL_MACOS' "$TAURI_MAIN"; then
  log_pass "AT_NATIVE_SHELL_MACOS flag is present"
else
  log_fail "AT_NATIVE_SHELL_MACOS flag is missing"
fi

if rg -q '__TUNDRA_NATIVE_SHELL__' "$TAURI_MAIN"; then
  log_pass "window.__TUNDRA_NATIVE_SHELL__ init script is present"
else
  log_fail "window.__TUNDRA_NATIVE_SHELL__ init script is missing"
fi

if rg -q 'dataset\.nativeShell' "$TAURI_MAIN"; then
  log_pass "documentElement.dataset.nativeShell wiring is present"
else
  log_fail "documentElement.dataset.nativeShell wiring is missing"
fi

if rg -q -- '--titlebar-inset' "$TAURI_MAIN"; then
  log_pass "titlebar inset runtime variable injection is present"
else
  log_fail "titlebar inset runtime variable injection is missing"
fi

echo -e "\n${BOLD}== Leptos native-shell CSS hooks ==${NC}"
for selector in \
  'html\[data-native-shell="1"\] \.top-bar' \
  'html\[data-native-shell="1"\] \.sidebar' \
  'html\[data-native-shell="1"\] \.sidebar-header'
do
  if rg -q "$selector" "$STYLE_CSS"; then
    log_pass "selector exists: $selector"
  else
    log_fail "selector missing: $selector"
  fi
done

if rg -q -- '--titlebar-inset' "$STYLE_CSS"; then
  log_pass "CSS uses --titlebar-inset variable"
else
  log_fail "CSS missing --titlebar-inset variable usage"
fi

echo -e "\n${BOLD}== Optional runtime hint ==${NC}"
log_info "To manually validate in app:"
log_info "  AT_NATIVE_SHELL_MACOS=1 cargo run --manifest-path app/tauri/Cargo.toml"
log_info "Then verify native chrome spacing + draggable title region."

echo ""
echo "============================================================"
echo "  Results: PASS=$PASS FAIL=$FAIL"
echo "============================================================"
echo ""

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
exit 0

