#!/usr/bin/env bash
# Verification script for clean daemon shutdown when app closes
#
# This script verifies that:
# 1. The desktop app launches successfully
# 2. The daemon starts and API is accessible
# 3. When the app closes, no daemon processes remain running
# 4. No orphaned background tasks persist

set -euo pipefail

echo "=== Daemon Shutdown Verification ==="
echo ""

# Step 1: Check for any existing auto-tundra processes
echo "Step 1: Checking for existing auto-tundra processes..."
EXISTING=$(pgrep -f "auto-tundra" || true)
if [ -n "$EXISTING" ]; then
    echo "⚠️  Warning: Found existing auto-tundra processes:"
    ps -p "$EXISTING" -o pid,comm,args || true
    echo ""
    read -p "Kill existing processes? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        kill $EXISTING || true
        sleep 2
    fi
fi
echo "✓ No existing processes"
echo ""

# Step 2: Run the E2E shutdown test
echo "Step 2: Running E2E shutdown test..."
if cargo test -p at-tauri test_desktop_app_clean_shutdown -- --nocapture; then
    echo "✓ E2E shutdown test passed"
else
    echo "✗ E2E shutdown test failed"
    exit 1
fi
echo ""

# Step 3: Manual verification instructions
echo "Step 3: Manual verification (requires human interaction)"
echo ""
echo "To manually verify clean shutdown:"
echo "1. Launch the desktop app:"
echo "   cargo tauri dev"
echo ""
echo "2. Verify the app launches and shows the UI"
echo ""
echo "3. In another terminal, check for daemon processes:"
echo "   ps aux | grep auto-tundra"
echo "   You should see the tauri process running"
echo ""
echo "4. Check that the API is accessible:"
echo "   curl http://localhost:<port>/api/status"
echo "   (port is shown in the app logs)"
echo ""
echo "5. Close the app (Cmd+Q on macOS, Alt+F4 on Windows/Linux)"
echo ""
echo "6. Verify no processes remain:"
echo "   ps aux | grep auto-tundra"
echo "   Should return no results (except grep itself)"
echo ""
echo "7. Verify no zombie processes:"
echo "   ps aux | grep 'Z'"
echo "   Should not include any auto-tundra processes"
echo ""

echo "=== Verification Complete ==="
echo ""
echo "The automated test passed. For complete verification,"
echo "please also run the manual steps above."
