#!/bin/bash

echo "🔍 Debug Sidebar Test"
echo "===================="

APP_URL="http://localhost:3001"

echo "🚀 Opening browser..."
playwright-cli open "$APP_URL"
sleep 3

echo "📸 Capturing snapshot..."
playwright-cli snapshot

echo "🎯 Looking for toggle button in snapshot..."
echo "Available elements with 'toggle' or 'sidebar':"
playwright-cli eval "Array.from(document.querySelectorAll('*')).filter(el => el.className && (el.className.includes('toggle') || el.className.includes('sidebar'))).map(el => ({tag: el.tagName, class: el.className, id: el.id}))"

echo "🔍 Checking toggle button styles..."
playwright-cli eval "
const btn = document.querySelector('.sidebar-toggle-btn');
if (!btn) {
  'BUTTON_NOT_FOUND';
} else {
  const style = getComputedStyle(btn);
  {
    display: style.display,
    visibility: style.visibility,
    opacity: style.opacity,
    zIndex: style.zIndex,
    position: style.position,
    pointerEvents: style.pointerEvents,
    offsetParent: btn.offsetParent ? btn.offsetParent.tagName : 'none',
    offsetWidth: btn.offsetWidth,
    offsetHeight: btn.offsetHeight
  };
}
"

echo "🖱️ Trying to click with different selector..."
playwright-cli click "button.sidebar-toggle-btn" 2>/dev/null || echo "Click with button.sidebar-toggle-btn failed"

sleep 2

echo "🔍 Checking sidebar width after attempt..."
playwright-cli eval "getComputedStyle(document.querySelector('.sidebar')).width"

echo "🔚 Closing browser..."
playwright-cli close

echo "✅ Debug test complete!"
