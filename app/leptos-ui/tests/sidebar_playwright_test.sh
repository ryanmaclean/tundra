#!/bin/bash

# Playwright Sidebar Test Script
# Tests the sidebar collapse/expand functionality

echo "🎭 Starting Playwright Sidebar Test"
echo "=================================="

# Configuration
APP_URL="http://localhost:3001"
SCREENSHOT_DIR="/tmp/sidebar_test_screenshots"
TEST_LOG="/tmp/sidebar_test.log"

# Create screenshot directory
mkdir -p "$SCREENSHOT_DIR"

# Function to log with timestamp
log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$TEST_LOG"
}

# Function to take screenshot
take_screenshot() {
    local name=$1
    log "📸 Taking screenshot: $name"
    playwright-cli screenshot "$SCREENSHOT_DIR/${name}.png"
}

# Function to check if sidebar is collapsed
is_sidebar_collapsed() {
    local width=$(playwright-cli eval "document.querySelector('.sidebar') ? getComputedStyle(document.querySelector('.sidebar')).width : '0'" | tr -d '"')
    # Remove 'px' and convert to number
    width_num=$(echo "$width" | sed 's/px//')
    log "📏 Sidebar width: ${width}px"
    
    if [ "$width_num" -le 60 ]; then
        return 0  # Collapsed (56px + small margin)
    else
        return 1  # Expanded
    fi
}

# Function to check if toggle button exists and is visible
toggle_button_exists() {
    local result=$(playwright-cli eval "
        const btn = document.querySelector('.sidebar-toggle-btn');
        if (!btn) return 'not_found';
        const style = getComputedStyle(btn);
        const isVisible = style.display !== 'none' && style.opacity !== '0' && btn.offsetParent !== null;
        return isVisible ? 'visible' : 'hidden';
    " | tr -d '"')
    
    log "🎯 Toggle button status: $result"
    [ "$result" = "visible" ]
}

# Function to click toggle button
click_toggle() {
    log "🖱️ Clicking toggle button..."
    playwright-cli click ".sidebar-toggle-btn"
    sleep 1  # Wait for animation
}

# Function to check sidebar class
has_collapsed_class() {
    local result=$(playwright-cli eval "document.querySelector('.sidebar') ? document.querySelector('.sidebar').classList.contains('collapsed') : false" | tr -d '"')
    log "🏷️ Sidebar has 'collapsed' class: $result"
    [ "$result" = "true" ]
}

# Start test
log "🚀 Opening browser and navigating to app..."

# Open browser and navigate to app
playwright-cli open "$APP_URL"
sleep 3  # Wait for page to load

# Take initial screenshot
take_screenshot "01_initial_state"

# Check initial state
log "🔍 Checking initial sidebar state..."

if is_sidebar_collapsed; then
    log "✅ Sidebar starts collapsed"
    INITIAL_STATE="collapsed"
else
    log "ℹ️ Sidebar starts expanded"
    INITIAL_STATE="expanded"
fi

# Check toggle button
if toggle_button_exists; then
    log "✅ Toggle button is visible"
else
    log "❌ Toggle button is not visible or missing"
    take_screenshot "02_toggle_missing"
    exit 1
fi

# Test collapse functionality
if [ "$INITIAL_STATE" = "expanded" ]; then
    log "🔄 Testing collapse functionality..."
    
    # Click toggle to collapse
    click_toggle
    take_screenshot "03_after_collapse_click"
    
    # Check if collapsed
    if is_sidebar_collapsed && has_collapsed_class; then
        log "✅ SUCCESS: Sidebar collapsed correctly"
    else
        log "❌ FAILED: Sidebar did not collapse"
        take_screenshot "04_collapse_failed"
    fi
    
    # Test expand functionality
    log "🔄 Testing expand functionality..."
    
    # Click toggle to expand
    click_toggle
    take_screenshot "05_after_expand_click"
    
    # Check if expanded
    if !is_sidebar_collapsed && !has_collapsed_class; then
        log "✅ SUCCESS: Sidebar expanded correctly"
    else
        log "❌ FAILED: Sidebar did not expand"
        take_screenshot "06_expand_failed"
    fi
    
else
    log "ℹ️ Sidebar starts collapsed, testing expand first..."
    
    # Click toggle to expand
    click_toggle
    take_screenshot "03_after_expand_click"
    
    # Check if expanded
    if !is_sidebar_collapsed && !has_collapsed_class; then
        log "✅ SUCCESS: Sidebar expanded correctly"
    else
        log "❌ FAILED: Sidebar did not expand"
        take_screenshot "04_expand_failed"
    fi
    
    # Test collapse functionality
    log "🔄 Testing collapse functionality..."
    
    # Click toggle to collapse
    click_toggle
    take_screenshot "05_after_collapse_click"
    
    # Check if collapsed
    if is_sidebar_collapsed && has_collapsed_class; then
        log "✅ SUCCESS: Sidebar collapsed correctly"
    else
        log "❌ FAILED: Sidebar did not collapse"
        take_screenshot "06_collapse_failed"
    fi
fi

# Test final state
take_screenshot "07_final_state"

# Check theme/appearance
log "🎨 Checking theme appearance..."

local bg_color=$(playwright-cli eval "getComputedStyle(document.body).backgroundColor" | tr -d '"')
log "🎨 Body background: $bg_color"

local text_color=$(playwright-cli eval "getComputedStyle(document.body).color" | tr -d '"')
log "🎨 Body text color: $text_color"

# Check console for errors
log "📋 Checking console for errors..."
playwright-cli console error > "$SCREENSHOT_DIR/console_errors.log" 2>&1

if [ -s "$SCREENSHOT_DIR/console_errors.log" ]; then
    log "⚠️ Console errors found:"
    cat "$SCREENSHOT_DIR/console_errors.log" | tee -a "$TEST_LOG"
else
    log "✅ No console errors"
fi

# Close browser
playwright-cli close

# Summary
log "📊 Test Summary"
log "============="
log "Screenshots saved to: $SCREENSHOT_DIR"
log "Test log saved to: $TEST_LOG"

if [ -f "$SCREENSHOT_DIR/console_errors.log" ] && [ -s "$SCREENSHOT_DIR/console_errors.log" ]; then
    log "⚠️ Some issues detected - check screenshots and console errors"
else
    log "✅ Test completed successfully"
fi

echo ""
echo "🎭 Playwright Test Complete!"
echo "Screenshots: $SCREENSHOT_DIR"
echo "Log file: $TEST_LOG"
