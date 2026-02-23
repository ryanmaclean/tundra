#![allow(dead_code)]
use crate::profiling::record_event;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Custom metrics collector for Datadog
#[derive(Debug, Default)]
pub struct MetricsCollector {
    counters: Arc<RwLock<HashMap<String, u64>>>,
    gauges: Arc<RwLock<HashMap<String, f64>>>,
    histograms: Arc<RwLock<HashMap<String, Vec<f64>>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment a counter metric
    pub async fn increment_counter(&self, name: &str, tags: &[(&str, &str)]) {
        let mut counters = self.counters.write().await;
        let key = self.build_key(name, tags);
        *counters.entry(key).or_insert(0) += 1;

        tracing::info!(
            metric_type = "counter",
            metric_name = name,
            tags = ?tags,
            value = 1,
            "counter incremented"
        );
    }

    /// Set a gauge metric value
    pub async fn set_gauge(&self, name: &str, value: f64, tags: &[(&str, &str)]) {
        let mut gauges = self.gauges.write().await;
        let key = self.build_key(name, tags);
        gauges.insert(key, value);

        tracing::info!(
            metric_type = "gauge",
            metric_name = name,
            tags = ?tags,
            value = value,
            "gauge set"
        );
    }

    /// Record a histogram value
    pub async fn record_histogram(&self, name: &str, value: f64, tags: &[(&str, &str)]) {
        let mut histograms = self.histograms.write().await;
        let key = self.build_key(name, tags);
        histograms.entry(key).or_insert_with(Vec::new).push(value);

        tracing::info!(
            metric_type = "histogram",
            metric_name = name,
            tags = ?tags,
            value = value,
            "histogram recorded"
        );
    }

    /// Get current metrics values for reporting
    pub async fn get_metrics_snapshot(&self) -> MetricsSnapshot {
        let counters = self.counters.read().await.clone();
        let gauges = self.gauges.read().await.clone();
        let histograms = self.histograms.read().await.clone();

        MetricsSnapshot {
            counters,
            gauges,
            histograms,
        }
    }

    /// Build metric key with tags
    fn build_key(&self, name: &str, tags: &[(&str, &str)]) -> String {
        if tags.is_empty() {
            name.to_string()
        } else {
            let tag_str = tags
                .iter()
                .map(|(k, v)| format!("{}:{}", k, v))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{}}}{}", tag_str, name)
        }
    }

    /// Reset all metrics (useful for testing)
    pub async fn reset(&self) {
        self.counters.write().await.clear();
        self.gauges.write().await.clear();
        self.histograms.write().await.clear();
    }
}

/// Snapshot of current metrics
#[derive(Debug, Default, Clone)]
pub struct MetricsSnapshot {
    pub counters: HashMap<String, u64>,
    pub gauges: HashMap<String, f64>,
    pub histograms: HashMap<String, Vec<f64>>,
}

impl MetricsSnapshot {
    /// Calculate histogram statistics
    pub fn histogram_stats(&self, key: &str) -> Option<HistogramStats> {
        let values = self.histograms.get(key)?;
        if values.is_empty() {
            return None;
        }

        let sorted = {
            let mut sorted = values.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            sorted
        };

        let len = sorted.len();
        let sum: f64 = sorted.iter().sum();
        let mean = sum / len as f64;
        let p50 = sorted[len / 2];
        let p95 = sorted[(len as f64 * 0.95) as usize];
        let p99 = sorted[(len as f64 * 0.99) as usize];
        let min = sorted[0];
        let max = sorted[len - 1];

        Some(HistogramStats {
            count: len,
            min,
            max,
            mean,
            p50,
            p95,
            p99,
        })
    }
}

#[derive(Debug, Clone)]
pub struct HistogramStats {
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

/// Global metrics instance
static METRICS: std::sync::LazyLock<MetricsCollector> =
    std::sync::LazyLock::new(MetricsCollector::new);

/// Get global metrics collector
pub fn metrics() -> &'static MetricsCollector {
    &METRICS
}

/// Convenience macros for metrics
#[macro_export]
macro_rules! increment_counter {
    ($name:expr) => {
        $crate::metrics::metrics().increment_counter($name, &[]).await
    };
    ($name:expr, $($key:ident = $value:expr),*) => {
        $crate::metrics::metrics().increment_counter($name, &[$((stringify!($key), $value)),*]).await
    };
}

#[macro_export]
macro_rules! set_gauge {
    ($name:expr, $value:expr) => {
        $crate::metrics::metrics().set_gauge($name, $value, &[]).await
    };
    ($name:expr, $value:expr, $($key:ident = $value_tag:expr),*) => {
        $crate::metrics::metrics().set_gauge($name, $value, &[$((stringify!($key), $value_tag)),*]).await
    };
}

#[macro_export]
macro_rules! record_histogram {
    ($name:expr, $value:expr) => {
        $crate::metrics::metrics().record_histogram($name, $value, &[]).await
    };
    ($name:expr, $value:expr, $($key:ident = $value_tag:expr),*) => {
        $crate::metrics::metrics().record_histogram($name, $value, &[$((stringify!($key), $value_tag)),*]).await
    };
}

/// Application-specific metrics
pub struct AppMetrics;

impl AppMetrics {
    /// Record daemon startup
    pub async fn daemon_started() {
        increment_counter!("daemon.startup");
        set_gauge!("daemon.uptime", 0.0);
    }

    /// Record daemon shutdown
    pub async fn daemon_stopped() {
        increment_counter!("daemon.shutdown");
    }

    /// Record API request
    pub async fn api_request(method: &str, endpoint: &str, status: u16, duration_ms: f64) {
        increment_counter!(
            "api.requests",
            method = method,
            endpoint = endpoint,
            status = &status.to_string()
        );
        record_histogram!(
            "api.request_duration_ms",
            duration_ms,
            method = method,
            endpoint = endpoint
        );
    }

    /// Record frontend request
    pub async fn frontend_request(path: &str, status: u16, duration_ms: f64) {
        increment_counter!(
            "frontend.requests",
            path = path,
            status = &status.to_string()
        );
        record_histogram!("frontend.request_duration_ms", duration_ms, path = path);
    }

    /// Record task execution
    pub async fn task_executed(task_type: &str, success: bool, duration_ms: f64) {
        increment_counter!(
            "tasks.executed",
            task_type = task_type,
            success = &success.to_string()
        );
        record_histogram!("task.duration_ms", duration_ms, task_type = task_type);
    }

    /// Record LLM-specific metrics
    pub async fn llm_request(
        model: &str,
        provider: &str,
        tokens_used: u32,
        duration_ms: f64,
        success: bool,
    ) {
        increment_counter!(
            "llm.requests",
            model = model,
            provider = provider,
            success = &success.to_string()
        );
        record_histogram!(
            "llm.request_duration_ms",
            duration_ms,
            model = model,
            provider = provider
        );
        record_histogram!(
            "llm.tokens_used",
            tokens_used as f64,
            model = model,
            provider = provider
        );
    }

    /// Record LLM profile bootstrap
    pub async fn llm_profile_bootstrap(
        total_profiles: u32,
        best_profile: &str,
        best_provider: &str,
    ) {
        increment_counter!("llm.profile.bootstrap");
        set_gauge!("llm.profile.total_available", total_profiles as f64);
        record_event(
            "llm_profile_bootstrap",
            &[
                ("total_profiles", &total_profiles.to_string()),
                ("best_profile", best_profile),
                ("best_provider", best_provider),
            ],
        );
    }

    /// Record LLM agent execution
    pub async fn llm_agent_execution(
        agent_name: &str,
        task_type: &str,
        duration_ms: f64,
        success: bool,
    ) {
        increment_counter!(
            "llm.agent.executions",
            agent = agent_name,
            task_type = task_type,
            success = &success.to_string()
        );
        record_histogram!(
            "llm.agent.duration_ms",
            duration_ms,
            agent = agent_name,
            task_type = task_type
        );
    }

    /// Record memory usage
    pub async fn memory_usage_mb(usage_mb: f64) {
        set_gauge!("memory.usage_mb", usage_mb);
    }

    /// Record CPU usage
    pub async fn cpu_usage_percent(percent: f64) {
        set_gauge!("cpu.usage_percent", percent);
    }

    /// Record active connections
    pub async fn active_connections(count: u64) {
        set_gauge!("connections.active", count as f64);
    }

    /// Record error
    pub async fn error_occurred(error_type: &str, component: &str) {
        increment_counter!("errors", error_type = error_type, component = component);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collection() {
        let metrics = MetricsCollector::new();

        metrics.increment_counter("test_counter", &[]).await;
        metrics.set_gauge("test_gauge", 42.0, &[]).await;
        metrics.record_histogram("test_histogram", 1.5, &[]).await;

        let snapshot = metrics.get_metrics_snapshot().await;
        assert_eq!(snapshot.counters.get("test_counter"), Some(&1));
        assert_eq!(snapshot.gauges.get("test_gauge"), Some(&42.0));
        assert_eq!(snapshot.histograms.get("test_histogram"), Some(&vec![1.5]));
    }

    #[tokio::test]
    async fn test_histogram_stats() {
        let mut snapshot = MetricsSnapshot::default();
        snapshot
            .histograms
            .insert("test".to_string(), vec![1.0, 2.0, 3.0, 4.0, 5.0]);

        let stats = snapshot.histogram_stats("test").unwrap();
        assert_eq!(stats.count, 5);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 5.0);
        assert_eq!(stats.mean, 3.0);
        assert_eq!(stats.p50, 3.0);
    }
}
