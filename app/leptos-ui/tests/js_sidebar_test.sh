#!/bin/bash

echo "🧪 JavaScript Sidebar Test"
echo "========================"

APP_URL="http://localhost:3001"

echo "🚀 Opening browser..."
playwright-cli open "$APP_URL"
sleep 3

echo "🔍 Initial state..."
echo "Sidebar width: $(playwright-cli eval "getComputedStyle(document.querySelector('.sidebar')).width")"
echo "Toggle button exists: $(playwright-cli eval "!!document.querySelector('.sidebar-toggle-btn')")"

echo "🖱️ Clicking toggle button via JavaScript..."
playwright-cli eval "document.querySelector('.sidebar-toggle-btn').click()"

sleep 2

echo "🔍 State after first click..."
echo "Sidebar width: $(playwright-cli eval "getComputedStyle(document.querySelector('.sidebar')).width")"
echo "Has collapsed class: $(playwright-cli eval "document.querySelector('.sidebar').classList.contains('collapsed')")"

echo "🖱️ Clicking toggle button again via JavaScript..."
playwright-cli eval "document.querySelector('.sidebar-toggle-btn').click()"

sleep 2

echo "🔍 Final state..."
echo "Sidebar width: $(playwright-cli eval "getComputedStyle(document.querySelector('.sidebar')).width")"
echo "Has collapsed class: $(playwright-cli eval "document.querySelector('.sidebar').classList.contains('collapsed')")"

echo "🔚 Closing browser..."
playwright-cli close

echo "✅ JavaScript test complete!"
