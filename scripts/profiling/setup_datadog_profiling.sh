#!/bin/bash
# setup_datadog_profiling.sh - Enable Datadog profiling for Rust apps

set -e

echo "🔍 Setting up Datadog Profiling for Rust..."

# 1. Check if Datadog agent is running
if ! pgrep -f "datadog-agent" > /dev/null; then
    echo "❌ Datadog agent not running. Please start it first."
    exit 1
fi

echo "✅ Datadog agent is running"

# 2. Backup current configuration
echo "📋 Backing up current configuration..."
sudo cp /opt/datadog-agent/etc/datadog.yaml /opt/datadog-agent/etc/datadog.yaml.backup.$(date +%Y%m%d_%H%M%S)

# 3. Add profiling configuration
echo "⚙️ Adding profiling configuration..."
sudo tee -a /opt/datadog-agent/etc/datadog.yaml << 'EOF'

# Rust Profiling Configuration
apm_config:
  enabled: true
  profiling:
    enabled: true
    cpu_profiling_enabled: true
    heap_profiling_enabled: true
    allocation_profiling_enabled: true
    service: at-daemon
    env: development
    version: "0.1.0"
EOF

# 4. Restart Datadog agent
echo "🔄 Restarting Datadog agent..."
sudo launchctl unload /Library/LaunchDaemons/com.datadoghq.agent.plist 2>/dev/null || true
sleep 2
sudo launchctl load /Library/LaunchDaemons/com.datadoghq.agent.plist

# 5. Wait for agent to start
echo "⏳ Waiting for agent to start..."
sleep 5

# 6. Check agent status
echo "📊 Checking agent status..."
sudo datadog-agent status | grep -A 10 "profiling" || echo "⚠️ Profiling section not found in status"

# 7. Verify profiling endpoint
echo "🔗 Verifying profiling endpoint..."
if curl -s http://localhost:8126/v0.7/config > /dev/null; then
    echo "✅ Profiling endpoint accessible"
else
    echo "❌ Profiling endpoint not accessible"
fi

# 8. Add Rust dependencies
echo "🦀 Adding Rust dependencies..."
cd /Users/studio/rust-harness

if ! grep -q "ddtrace" Cargo.toml; then
    echo "Adding ddtrace to dependencies..."
    cargo add ddtrace || echo "⚠️ Failed to add ddtrace"
fi

if ! grep -q "tracing" Cargo.toml; then
    echo "Adding tracing to dependencies..."
    cargo add tracing tracing-subscriber || echo "⚠️ Failed to add tracing"
fi

# 9. Create environment file
echo "🌍 Creating environment file..."
cat > .env.datadog << 'EOF'
# Datadog Profiling Environment Variables
export DD_SERVICE=at-daemon
export DD_ENV=development
export DD_VERSION=0.1.0
export DD_TRACE_AGENT_URL=http://localhost:8126
export DD_PROFILING_ENABLED=true
export DD_CPU_PROFILING_ENABLED=true
export DD_HEAP_PROFILING_ENABLED=true
export DD_ALLOCATION_PROFILING_ENABLED=true
EOF

echo "✅ Setup complete!"
echo ""
echo "🚀 To run with profiling:"
echo "source .env.datadog"
echo "cargo run --bin at-daemon"
echo ""
echo "📊 View results at: https://app.datadoghq.com/profiling"
echo ""
echo "🔍 Check agent status with: sudo datadog-agent status"
