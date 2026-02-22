#!/bin/bash

# Simple Playwright Sidebar Test
echo "🎭 Starting Simple Sidebar Test"
echo "=============================="

# Configuration
APP_URL="http://localhost:3001"

echo "🚀 Opening browser..."
playwright-cli open "$APP_URL"
sleep 3

echo "📸 Taking initial screenshot..."
playwright-cli screenshot > /tmp/initial.png 2>/dev/null || echo "Screenshot saved"

echo "🔍 Checking sidebar width..."
playwright-cli eval "getComputedStyle(document.querySelector('.sidebar')).width"

echo "🎯 Checking if toggle button exists..."
playwright-cli eval "!!document.querySelector('.sidebar-toggle-btn')"

echo "🖱️ Clicking toggle button..."
playwright-cli click ".sidebar-toggle-btn"
sleep 2

echo "📸 Taking screenshot after click..."
playwright-cli screenshot > /tmp/after_click.png 2>/dev/null || echo "Screenshot saved"

echo "🔍 Checking sidebar width after click..."
playwright-cli eval "getComputedStyle(document.querySelector('.sidebar')).width"

echo "🏷️ Checking if sidebar has collapsed class..."
playwright-cli eval "document.querySelector('.sidebar').classList.contains('collapsed')"

echo "🖱️ Clicking toggle button again..."
playwright-cli click ".sidebar-toggle-btn"
sleep 2

echo "📸 Taking final screenshot..."
playwright-cli screenshot > /tmp/final.png 2>/dev/null || echo "Screenshot saved"

echo "🔍 Checking final sidebar width..."
playwright-cli eval "getComputedStyle(document.querySelector('.sidebar')).width"

echo "📋 Checking console errors..."
playwright-cli console error

echo "🔚 Closing browser..."
playwright-cli close

echo "✅ Test complete!"
echo "Screenshots saved to /tmp/initial.png, /tmp/after_click.png, /tmp/final.png"
