# 🔍 Datadog Profiling Comparison: ddprof vs Library Integration

## 📊 Overview

You have **two options** for Datadog profiling with Rust:

| Method | Setup | Performance | Production | Features |
|--------|-------|-------------|------------|----------|
| **ddprof** | Zero code changes | Low overhead | ✅ Production-ready | CPU, Memory, Allocation |
| **Library** | Code integration | Medium overhead | ✅ Production | Custom spans, tracing |

## 🚀 ddprof (Native Profiler)

### ✅ Advantages:
- **Zero instrumentation** - No code changes needed
- **Language agnostic** - Works with any compiled language
- **Low overhead** - < 1% performance impact
- **Easy setup** - Just wrap your command
- **Production ready** - Designed for production use

### ⚙️ Setup:
```bash
# 1. Download ddprof
curl -Lo ddprof https://github.com/DataDog/ddprof/releases/latest/download/ddprof-arm64
chmod +x ddprof

# 2. Set environment variables
export DD_ENV=development
export DD_SERVICE=at-daemon
export DD_VERSION=0.1.0
export DD_API_KEY=cee054f0868d53693f5a956f6ca4dcd1

# 3. Run your app with ddprof
./ddprof cargo run --package at-daemon --bin at-daemon
```

### 📊 What ddprof Provides:
- **CPU profiling** with flame graphs
- **Memory allocation** tracking
- **Native runtime** information
- **System call** profiling
- **Kernel-level** performance data

## 📚 Library Integration (ddtrace)

### ✅ Advantages:
- **Custom spans** - Add your own tracing
- **Distributed tracing** - Track across services
- **Business metrics** - Custom performance data
- **Code-level** insights
- **Integration** with application logic

### ⚙️ Setup:
```rust
// Add to Cargo.toml
[dependencies]
ddtrace = "0.2"
tracing = "0.1"
tracing-subscriber = "0.3"

// Add to main.rs
use ddtrace::tracer;
use tracing_subscriber;

fn main() {
    // Initialize Datadog tracing
    let tracer = tracer::init();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    // Your application code...
}
```

### 📊 What Library Provides:
- **Custom spans** and tracing
- **Distributed tracing** across services
- **Business logic** instrumentation
- **Application-level** metrics
- **Service maps** and dependencies

## 🎯 Recommendation for Your Use Case

### For **at-daemon**:
**Use ddprof** because:
- ✅ Zero code changes required
- ✅ System-level performance insights
- ✅ Easy to enable/disable
- ✅ Perfect for daemon processes
- ✅ Lower overhead

### For **Web services**:
**Use library integration** because:
- ✅ Custom business metrics
- ✅ Distributed tracing
- ✅ Request-level insights
- ✅ Service dependency mapping

## 🚀 Quick Start Commands

### ddprof Method:
```bash
# Setup
source .env.ddprof

# Run with profiling
./ddprof cargo run --package at-daemon --bin at-daemon

# View results
# https://app.datadoghq.com/profiling
```

### Library Method:
```bash
# Setup (already done)
cargo add --package at-daemon ddtrace tracing tracing-subscriber

# Run with profiling
source .env.datadog
cargo run --package at-daemon --bin at-daemon

# View results
# https://app.datadoghq.com/profiling
```

## 📈 When to Use Each

### Use ddprof when:
- 🎯 **Quick profiling** without code changes
- 🎯 **System-level** performance analysis
- 🎯 **Production monitoring** with minimal impact
- 🎯 **Multiple languages** in same environment
- 🎯 **Daemon processes** and background services

### Use Library when:
- 🎯 **Custom business metrics** needed
- 🎯 **Distributed systems** tracing
- 🎯 **Request-level** performance data
- 🎯 **Service dependency** mapping
- 🎯 **Application-specific** insights

## 🔍 Viewing Results

Both methods send data to the same place:
- **URL**: https://app.datadoghq.com/profiling
- **Filter**: Service: `at-daemon`, Environment: `development`
- **Runtime**: Native (ddprof) vs Rust (library)

## 🚨 Troubleshooting

### ddprof Issues:
```bash
# Check binary
file ddprof
chmod +x ddprof

# Test with simple command
./ddprof echo "test"

# Check logs
export DD_LOG_LEVEL=DEBUG
```

### Library Issues:
```bash
# Check dependencies
cargo tree | grep ddtrace

# Verify environment
export DD_LOG_LEVEL=DEBUG
```

## 🎯 Bottom Line

**For your at-daemon**: Start with **ddprof** for easy, zero-instrumentation profiling. If you need custom business metrics later, add the library integration.

**Best of both worlds**: You can use both simultaneously for comprehensive coverage! 🚀
