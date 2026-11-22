#!/bin/bash
# setup_ddprof.sh - Setup Datadog Native Profiler (ddprof) for Rust

set -e

echo "🔍 Setting up Datadog ddprof for Rust profiling..."

# 1. Check system architecture
ARCH=$(uname -m)
echo "Architecture: $ARCH"

# 2. Download ddprof for macOS ARM64
echo "📥 Downloading ddprof..."
if [ "$ARCH" = "arm64" ]; then
    curl -Lo ddprof https://github.com/DataDog/ddprof/releases/latest/download/ddprof-arm64
else
    curl -Lo ddprof https://github.com/DataDog/ddprof/releases/latest/download/ddprof-amd64
fi

# 3. Make executable
chmod +x ddprof

# 4. Test ddprof
echo "🧪 Testing ddprof..."
./ddprof --version || echo "⚠️ ddprof test failed"

# 5. Create ddprof profiling script
echo "📝 Creating ddprof profiling script..."
cat > profile_with_ddprof.sh << 'EOF'
#!/bin/bash
# profile_with_ddprof.sh - Run Rust app with ddprof profiling

# Set Datadog environment variables
export DD_ENV=development
export DD_SERVICE=at-daemon
export DD_VERSION=0.1.0
export DD_API_KEY="${DD_API_KEY:?set DD_API_KEY in your environment}"
export DD_SITE=datadoghq.com
export DD_LOG_LEVEL=INFO

# Optional: Enable specific profiling types
export DD_PROFILING_ENABLED=true
export DD_CPU_PROFILING_ENABLED=true
export DD_HEAP_PROFILING_ENABLED=true
export DD_ALLOCATION_PROFILING_ENABLED=true

echo "🚀 Starting at-daemon with ddprof profiling..."
echo "📊 Profiles will appear in: https://app.datadoghq.com/profiling"
echo "🔍 Service: at-daemon, Environment: development"

# Run the application with ddprof
./ddprof cargo run --package at-daemon --bin at-daemon
EOF

chmod +x profile_with_ddprof.sh

# 6. Create environment file
echo "🌍 Creating ddprof environment file..."
cat > .env.ddprof << 'EOF'
# Datadog ddprof Environment Variables
export DD_ENV=development
export DD_SERVICE=at-daemon
export DD_VERSION=0.1.0
export DD_API_KEY="${DD_API_KEY:?set DD_API_KEY in your environment}"
export DD_SITE=datadoghq.com
export DD_LOG_LEVEL=INFO
export DD_PROFILING_ENABLED=true
export DD_CPU_PROFILING_ENABLED=true
export DD_HEAP_PROFILING_ENABLED=true
export DD_ALLOCATION_PROFILING_ENABLED=true
EOF

echo "✅ ddprof setup complete!"
echo ""
echo "🚀 To profile your Rust app:"
echo "source .env.ddprof"
echo "./ddprof cargo run --package at-daemon --bin at-daemon"
echo ""
echo "📊 Or use the convenience script:"
echo "./profile_with_ddprof.sh"
echo ""
echo "🔍 View results at: https://app.datadoghq.com/profiling"
echo ""
echo "📋 ddprof features:"
echo "- ✅ Zero instrumentation required"
echo "- ✅ CPU profiling with flame graphs"
echo "- ✅ Memory allocation tracking"
echo "- ✅ Native runtime profiling"
echo "- ✅ Production-ready"
