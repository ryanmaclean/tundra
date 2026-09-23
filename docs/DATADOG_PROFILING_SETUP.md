# 🔍 Datadog Profiling Setup for Rust

## 📊 Current Datadog Status

✅ **Datadog Agent Running**:
- Main agent: `/opt/datadog-agent/bin/agent/agent`
- Trace agent: `trace-agent` (APM enabled)
- Process agent: `process-agent` (process monitoring)
- API Key: Configured

## 🚀 Enabling Datadog Profiling for Rust

### 1. Add Datadog Dependencies to Cargo.toml

```toml
[dependencies]
# Add to your at-daemon Cargo.toml
ddtrace = "0.9"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }
```

### 2. Enable Profiling in Datadog Config

```yaml
# Add to /opt/datadog-agent/etc/datadog.yaml
apm_config:
  enabled: true
  profiling:
    enabled: true
    cpu_profiling_enabled: true
    heap_profiling_enabled: true
    allocation_profiling_enabled: true
    api_key: cee054f0868d53693f5a956f6ca4dcd1  # Your API key
    site: datadoghq.com
    env: development
    service: at-daemon
    version: "0.1.0"
```

### 3. Rust Code Integration

```rust
// Add to at-daemon/src/main.rs
use ddtrace::tracer;
use tracing_subscriber;

fn main() {
    // Initialize Datadog tracing
    let tracer = tracer::init();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    // Your existing code...
    at_daemon::run();
}
```

### 4. Environment Variables

```bash
export DD_SERVICE=at-daemon
export DD_ENV=development
export DD_VERSION=0.1.0
export DD_TRACE_AGENT_URL=http://localhost:8126
export DD_PROFILING_ENABLED=true
export DD_CPU_PROFILING_ENABLED=true
export DD_HEAP_PROFILING_ENABLED=true
```

## 🔧 Setup Commands

### Enable Profiling in Datadog Agent
```bash
# Backup current config
sudo cp /opt/datadog-agent/etc/datadog.yaml /opt/datadog-agent/etc/datadog.yaml.backup

# Add profiling configuration
sudo tee -a /opt/datadog-agent/etc/datadog.yaml << 'EOF'

# Profiling Configuration
apm_config:
  enabled: true
  profiling:
    enabled: true
    cpu_profiling_enabled: true
    heap_profiling_enabled: true
    allocation_profiling_enabled: true
    service: at-daemon
    env: development
EOF

# Restart Datadog agent
sudo launchctl unload /Library/LaunchDaemons/com.datadoghq.agent.plist
sudo launchctl load /Library/LaunchDaemons/com.datadoghq.agent.plist
```

### Install Rust Dependencies
```bash
cd /Users/studio/rust-harness
cargo add ddtrace tracing tracing-subscriber
```

## 📈 What Datadog Profiling Provides

### ✅ CPU Profiling
- Flame graphs
- Hot spot detection
- Function-level performance

### ✅ Memory Profiling
- Heap usage analysis
- Allocation tracking
- Memory leak detection

### ✅ Real-time Monitoring
- Live performance metrics
- Alerting on anomalies
- Historical trend analysis

## 🎯 Benefits Over Traditional Tools

| Feature | Datadog | cargo-flamegraph | dd |
|---------|---------|------------------|-----|
| Real-time | ✅ | ❌ | ❌ |
| Historical | ✅ | ❌ | ❌ |
| Alerting | ✅ | ❌ | ❌ |
| Distributed | ✅ | ❌ | ❌ |
| Production | ✅ | ❌ | ❌ |

## 🚀 Quick Start

```bash
# 1. Enable profiling
./setup_datadog_profiling.sh

# 2. Add dependencies
cd /Users/studio/rust-harness
cargo add ddtrace tracing tracing-subscriber

# 3. Run with profiling
DD_PROFILING_ENABLED=true DD_SERVICE=at-daemon cargo run --bin at-daemon

# 4. View results
# Open: https://app.datadoghq.com/profiling
```

## 📊 Viewing Results

1. **Navigate**: https://app.datadoghq.com/profiling
2. **Filter**: Service: `at-daemon`, Env: `development`
3. **Analyze**: CPU, Memory, and Allocation profiles
4. **Compare**: Before/after optimization snapshots

## 🔍 Troubleshooting

### Check Agent Status
```bash
sudo datadog-agent status
```

### Verify Profiling
```bash
curl http://localhost:8126/v0.7/config
```

### Check Logs
```bash
sudo tail -f /opt/datadog-agent/logs/agent.log
```
