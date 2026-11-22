# Datadog Profiling Setup for Rust

## 🎯 Overview
This document describes a proposed Datadog profiling setup. The current
`at-daemon` initialization logs configuration and creates tracing spans; it
does not install a Datadog exporter or a continuous profiler. The setup and
outputs described below have not been verified for the current daemon.

## 📋 Prerequisites
- ✅ Datadog agent installed and running
- ✅ Rust project with workspace structure
- ✅ Datadog API credentials (if not using local agent)

## 🔧 Step 1: Enable Profiling in Datadog Agent

Edit the Datadog configuration file:
```bash
sudo nano /opt/datadog-agent/etc/datadog.yaml
```

Add these configuration blocks:
```yaml
# Enable APM and Continuous Profiling
apm_config:
  enabled: true
  profiling:
    enabled: true
    profiling_receiver_timeout: 5

# Optional: Configure profiling settings
process_config:
  enabled: true
  process_collection:
    enabled: true
```

Restart the Datadog agent:
```bash
sudo launchctl stop com.datadog.agent
sudo launchctl start com.datadog.agent
```

## 🦀 Step 2: Add Datadog Dependencies to Rust

The workspace currently declares:
```toml
[workspace.dependencies]
tracing-datadog = "0.6"
```

And the daemon crate has these dependencies:
```toml
[dependencies]
tracing-datadog = { workspace = true }
```

Declaring this dependency alone does not initialize an exporter or profiler.

## 📊 Step 3: Initialize Datadog in Your Application

The `profiling.rs` module provides:
- `init_datadog_telemetry()` - Log the intended service and agent settings
- `traced_span!` macro - Create traced spans
- `profile_async!` macro - Profile async functions

## 🚀 Step 4: Run Your Application

Build and run with profiling enabled:
```bash
# Set environment variables for Datadog
export DD_SERVICE="at-daemon"
export DD_ENV="development"
export DD_VERSION="0.1.0"
export DD_TRACE_AGENT_URL="http://localhost:8126"

# Run the application
cargo run --release --bin at-daemon
```

## 📈 What You'll See in Datadog

### 1. **Continuous Profiler**
- Flame graphs showing CPU hotspots
- Memory allocation patterns
- Function call latency distributions

### 2. **APM Tracing**
- Request traces through your application
- Service dependency maps
- Error tracking and alerting

### 3. **Infrastructure Monitoring**
- Process resource usage
- System metrics correlation
- Custom metrics and tags

## 🔍 Example Profiling Output

When you run the application, you'll see traces like:
```
INFO  auto-tundra daemon starting
INFO  Datadog APM and profiling initialized
INFO  dashboard: http://localhost:3001
INFO  API server: http://localhost:9090
```

## 🛠️ Advanced Configuration

### Custom Service Names
```rust
let tracer_config = config::DatadogTracerConfig::builder()
    .service_name("my-custom-service")
    .env("production")
    .version("1.0.0")
    .build();
```

### Adding Custom Tags
```rust
let _span = tracing::info_span!(
    "database_query",
    service = "at-daemon",
    query_type = "select",
    table = "users"
);
```

### Async Function Profiling
```rust
let result = profile_async!("heavy_computation", async {
    // Your async code here
    compute_something().await
}).await;
```

## 🐛 Troubleshooting

### Common Issues

1. **Agent Connection Failed**
   - Check if Datadog agent is running: `ps aux | grep datadog`
   - Verify agent is listening on port 8126: `lsof -i :8126`

2. **No Profiling Data**
   - Ensure profiling is enabled in agent config
   - Check agent logs: `tail -f /opt/datadog-agent/logs/agent.log`

3. **High Overhead**
   - Reduce sampling rate in configuration
   - Use profiling only in development/staging

### Debug Commands
```bash
# Check agent status
datadog-agent status

# View agent configuration
cat /opt/datadog-agent/etc/datadog.yaml

# Test agent connectivity
curl http://localhost:8126/info
```

## 📚 Next Steps

1. **Explore Datadog UI**: Navigate to APM > Services to see your service
2. **Set Up Alerts**: Configure performance-based alerts
3. **Dashboard Creation**: Build custom dashboards for key metrics
4. **Integration Testing**: Test profiling under load

## 🎯 Benefits

- **Real-time Insights**: See performance issues as they happen
- **Root Cause Analysis**: Quickly identify bottlenecks
- **Production Safety**: Low-overhead profiling suitable for production
- **Historical Analysis**: Track performance trends over time

## 📞 Support

- Datadog Documentation: https://docs.datadoghq.com/
- Rust APM docs: https://docs.datadoghq.com/tracing/setup_overview/setup/rust/
- Continuous Profiler: https://docs.datadoghq.com/profiling/
