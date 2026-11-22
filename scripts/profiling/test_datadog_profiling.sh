#!/bin/bash

# Test script to verify Datadog profiling setup

echo "🔍 Testing Datadog Profiling Setup"
echo "=================================="

# 1. Check if Datadog agent is running
echo "1. Checking Datadog agent status..."
if pgrep -f "datadog-agent" > /dev/null; then
    echo "✅ Datadog agent is running"
else
    echo "❌ Datadog agent is not running"
    exit 1
fi

# 2. Check if agent is listening on port 8126
echo "2. Checking agent port 8126..."
if lsof -i :8126 > /dev/null 2>&1; then
    echo "✅ Agent is listening on port 8126"
else
    echo "❌ Agent is not listening on port 8126"
fi

# 3. Test agent connectivity
echo "3. Testing agent connectivity..."
if curl -s http://localhost:8126/info > /dev/null 2>&1; then
    echo "✅ Agent is responding on /info endpoint"
else
    echo "❌ Agent is not responding on /info endpoint"
fi

# 4. Check agent configuration
echo "4. Checking agent configuration..."
if grep -q "enabled: true" /opt/datadog-agent/etc/datadog.yaml 2>/dev/null; then
    echo "✅ APM appears to be enabled"
else
    echo "⚠️  APM might not be enabled in agent config"
fi

# 5. Build and test the application
echo "5. Building application with Datadog integration..."
cd /Users/studio/rust-harness

if cargo build --release --bin at-daemon > /dev/null 2>&1; then
    echo "✅ Application builds successfully"
else
    echo "❌ Application build failed"
    exit 1
fi

# 6. Test application startup
echo "6. Testing application startup with Datadog..."
export DD_SERVICE="at-daemon"
export DD_ENV="development"
export DD_VERSION="0.1.0"
export DD_TRACE_AGENT_URL="http://localhost:8126"

# Run for 5 seconds and check if it starts
cargo run --release --bin at-daemon > /tmp/at-daemon-test.log 2>&1 &
APP_PID=$!

sleep 3

if kill -0 $APP_PID 2>/dev/null; then
    echo "✅ Application starts successfully with Datadog"
    kill $APP_PID 2>/dev/null
else
    echo "❌ Application failed to start"
    echo "Last few lines of log:"
    tail -10 /tmp/at-daemon-test.log
    exit 1
fi

# 7. Check for Datadog traces
echo "7. Checking for Datadog traces..."
if grep -q "Datadog APM and profiling initialized" /tmp/at-daemon-test.log 2>/dev/null; then
    echo "✅ Datadog profiling is initialized"
else
    echo "⚠️  Datadog profiling might not be properly initialized"
fi

echo ""
echo "🎯 Test Summary"
echo "=============="
echo "✅ Datadog agent: Running"
echo "✅ Application: Builds and starts"
echo "✅ Profiling integration: Added to code"
echo ""
echo "📊 Next Steps:"
echo "1. Enable profiling in Datadog agent config"
echo "2. Set DD_API_KEY if using cloud Datadog"
echo "3. Check Datadog UI for traces and profiles"
echo "4. Add more spans to critical functions"
echo ""
echo "🔧 To enable profiling in agent:"
echo "sudo nano /opt/datadog-agent/etc/datadog.yaml"
echo "# Add: apm_config.profiling.enabled: true"
echo "sudo launchctl restart com.datadog.agent"
