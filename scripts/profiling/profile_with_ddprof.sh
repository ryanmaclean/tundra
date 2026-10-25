#!/bin/bash
# profile_with_ddprof.sh - Run Rust app with ddprof profiling

# Set Datadog environment variables
export DD_ENV=development
export DD_SERVICE=at-daemon
export DD_VERSION=0.1.0
export DD_API_KEY=cee054f0868d53693f5a956f6ca4dcd1
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
