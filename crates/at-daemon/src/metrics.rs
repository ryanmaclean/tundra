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

    // ------------------------------------------------------------------
    // Counter math
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_counter_zero_state_is_absent() {
        let metrics = MetricsCollector::new();
        let snapshot = metrics.get_metrics_snapshot().await;
        // No counter writes ⇒ HashMap entry is absent (not Some(0)).
        assert!(snapshot.counters.is_empty());
        assert_eq!(snapshot.counters.get("never_incremented"), None);
    }

    #[tokio::test]
    async fn test_counter_many_increments_sum_correctly() {
        let metrics = MetricsCollector::new();
        for _ in 0..100 {
            metrics.increment_counter("hits", &[]).await;
        }
        let snapshot = metrics.get_metrics_snapshot().await;
        assert_eq!(snapshot.counters.get("hits"), Some(&100));
    }

    #[tokio::test]
    async fn test_counter_distinct_tag_keys_are_independent() {
        let metrics = MetricsCollector::new();
        metrics
            .increment_counter("requests", &[("status", "200")])
            .await;
        metrics
            .increment_counter("requests", &[("status", "200")])
            .await;
        metrics
            .increment_counter("requests", &[("status", "500")])
            .await;
        let snapshot = metrics.get_metrics_snapshot().await;
        // Exactly two distinct keys, with independent counts.
        assert_eq!(snapshot.counters.len(), 2);
        assert_eq!(
            snapshot.counters.get("{status:200}requests"),
            Some(&2)
        );
        assert_eq!(
            snapshot.counters.get("{status:500}requests"),
            Some(&1)
        );
    }

    /// Pin behavior: u64 counter increments do NOT saturate; they will overflow
    /// and panic in debug or wrap in release. We pin the current behavior at the
    /// API level: a counter just before u64::MAX accepts one more increment
    /// without changing the API surface (we test the boundary minus 1, not MAX
    /// itself, to avoid debug-mode panic regressions).
    #[tokio::test]
    async fn test_counter_near_max_does_not_clamp() {
        let metrics = MetricsCollector::new();
        // Seed the counter close to (but not at) u64::MAX, then increment once.
        {
            let mut guard = metrics.counters.write().await;
            guard.insert("near_max".to_string(), u64::MAX - 1);
        }
        metrics.increment_counter("near_max", &[]).await;
        let snapshot = metrics.get_metrics_snapshot().await;
        assert_eq!(snapshot.counters.get("near_max"), Some(&u64::MAX));
    }

    // ------------------------------------------------------------------
    // Gauge behavior
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_gauge_overwrites_previous_value() {
        let metrics = MetricsCollector::new();
        metrics.set_gauge("temp", 1.0, &[]).await;
        metrics.set_gauge("temp", 2.0, &[]).await;
        metrics.set_gauge("temp", 3.5, &[]).await;
        let snapshot = metrics.get_metrics_snapshot().await;
        assert_eq!(snapshot.gauges.get("temp"), Some(&3.5));
    }

    #[tokio::test]
    async fn test_gauge_negative_and_zero_values_preserved() {
        let metrics = MetricsCollector::new();
        metrics.set_gauge("delta", -1.5, &[]).await;
        metrics.set_gauge("zero", 0.0, &[]).await;
        let snapshot = metrics.get_metrics_snapshot().await;
        assert_eq!(snapshot.gauges.get("delta"), Some(&-1.5));
        assert_eq!(snapshot.gauges.get("zero"), Some(&0.0));
    }

    // ------------------------------------------------------------------
    // Histogram behavior + aggregation purity
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_histogram_appends_in_recorded_order() {
        let metrics = MetricsCollector::new();
        for v in [1.0_f64, 2.0, 3.0] {
            metrics.record_histogram("seq", v, &[]).await;
        }
        let snapshot = metrics.get_metrics_snapshot().await;
        assert_eq!(
            snapshot.histograms.get("seq"),
            Some(&vec![1.0, 2.0, 3.0])
        );
    }

    #[test]
    fn test_histogram_stats_missing_key_returns_none() {
        let snapshot = MetricsSnapshot::default();
        assert!(snapshot.histogram_stats("absent").is_none());
    }

    #[test]
    fn test_histogram_stats_empty_vec_returns_none() {
        let mut snapshot = MetricsSnapshot::default();
        snapshot.histograms.insert("empty".to_string(), vec![]);
        assert!(snapshot.histogram_stats("empty").is_none());
    }

    #[test]
    fn test_histogram_stats_single_value() {
        let mut snapshot = MetricsSnapshot::default();
        snapshot.histograms.insert("one".to_string(), vec![7.0]);
        let stats = snapshot.histogram_stats("one").unwrap();
        assert_eq!(stats.count, 1);
        assert_eq!(stats.min, 7.0);
        assert_eq!(stats.max, 7.0);
        assert_eq!(stats.mean, 7.0);
        assert_eq!(stats.p50, 7.0);
        assert_eq!(stats.p95, 7.0);
        assert_eq!(stats.p99, 7.0);
    }

    /// Pin the exact percentile-index formula used today:
    /// p50 = sorted[len/2], p95 = sorted[(len*0.95) as usize],
    /// p99 = sorted[(len*0.99) as usize]. For len=100 with values 1..=100
    /// this gives p50=51, p95=96, p99=100 (since (100*0.99)as usize = 99
    /// indexes the 100th element, value 100).
    #[test]
    fn test_histogram_stats_percentiles_pinned_for_len_100() {
        let mut snapshot = MetricsSnapshot::default();
        let values: Vec<f64> = (1..=100).map(|n| n as f64).collect();
        snapshot.histograms.insert("h".to_string(), values);
        let stats = snapshot.histogram_stats("h").unwrap();
        assert_eq!(stats.count, 100);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 100.0);
        assert_eq!(stats.mean, 50.5);
        // sorted[100/2] = sorted[50] = 51
        assert_eq!(stats.p50, 51.0);
        // sorted[(100*0.95) as usize] = sorted[95] = 96
        assert_eq!(stats.p95, 96.0);
        // sorted[(100*0.99) as usize] = sorted[99] = 100
        assert_eq!(stats.p99, 100.0);
    }

    #[test]
    fn test_histogram_stats_sorts_unsorted_input() {
        let mut snapshot = MetricsSnapshot::default();
        // Deliberately scrambled input must still produce min/max correctly.
        snapshot
            .histograms
            .insert("u".to_string(), vec![3.0, 1.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0]);
        let stats = snapshot.histogram_stats("u").unwrap();
        assert_eq!(stats.count, 8);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 9.0);
        // mean = 31 / 8 = 3.875
        assert!((stats.mean - 3.875).abs() < 1e-12);
    }

    // ------------------------------------------------------------------
    // build_key formatter (pinned byte-for-byte)
    // ------------------------------------------------------------------

    #[test]
    fn test_build_key_no_tags_returns_bare_name() {
        let m = MetricsCollector::new();
        assert_eq!(m.build_key("api.requests", &[]), "api.requests");
    }

    #[test]
    fn test_build_key_single_tag_format() {
        let m = MetricsCollector::new();
        assert_eq!(
            m.build_key("api.requests", &[("method", "GET")]),
            "{method:GET}api.requests"
        );
    }

    #[test]
    fn test_build_key_multiple_tags_joined_with_comma_in_order() {
        let m = MetricsCollector::new();
        // The build_key implementation joins in iteration order of the slice.
        let key = m.build_key(
            "http",
            &[("method", "POST"), ("status", "201"), ("path", "/v1/x")],
        );
        assert_eq!(key, "{method:POST,status:201,path:/v1/x}http");
    }

    #[test]
    fn test_build_key_empty_string_tag_value_preserved() {
        let m = MetricsCollector::new();
        // Empty values must round-trip without being dropped (escaping pin).
        assert_eq!(
            m.build_key("evt", &[("tag", "")]),
            "{tag:}evt"
        );
    }

    // ------------------------------------------------------------------
    // Reset
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_reset_clears_all_three_maps() {
        let metrics = MetricsCollector::new();
        metrics.increment_counter("c", &[]).await;
        metrics.set_gauge("g", 1.0, &[]).await;
        metrics.record_histogram("h", 1.0, &[]).await;
        metrics.reset().await;
        let snapshot = metrics.get_metrics_snapshot().await;
        assert!(snapshot.counters.is_empty());
        assert!(snapshot.gauges.is_empty());
        assert!(snapshot.histograms.is_empty());
    }

    // ------------------------------------------------------------------
    // Snapshot semantics
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_snapshot_is_a_clone_not_a_view() {
        let metrics = MetricsCollector::new();
        metrics.increment_counter("c", &[]).await;
        let snapshot = metrics.get_metrics_snapshot().await;
        // Mutate underlying collector after snapshot.
        metrics.increment_counter("c", &[]).await;
        // Snapshot value must remain pinned at 1.
        assert_eq!(snapshot.counters.get("c"), Some(&1));
        let snapshot_after = metrics.get_metrics_snapshot().await;
        assert_eq!(snapshot_after.counters.get("c"), Some(&2));
    }

    // ------------------------------------------------------------------
    // Global metrics() singleton
    // ------------------------------------------------------------------

    #[test]
    fn test_metrics_singleton_returns_same_address() {
        let a = metrics() as *const MetricsCollector;
        let b = metrics() as *const MetricsCollector;
        assert_eq!(a, b, "metrics() must return the same static instance");
    }

    // ------------------------------------------------------------------
    // AppMetrics — these are async wrappers that ultimately call into the
    // global static collector. We exercise them only to assert they're
    // callable from #[tokio::test] without panicking. We do not assert
    // counts here because the global static is shared with other tests
    // and the rest of the binary.
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_app_metrics_smoke_calls_do_not_panic() {
        AppMetrics::daemon_started().await;
        AppMetrics::daemon_stopped().await;
        AppMetrics::api_request("GET", "/v1/health", 200, 1.5).await;
        AppMetrics::frontend_request("/index.html", 200, 0.5).await;
        AppMetrics::task_executed("compile", true, 12.0).await;
        AppMetrics::llm_request("gpt-4", "openai", 100, 250.0, true).await;
        AppMetrics::llm_agent_execution("planner", "spec", 12.0, true).await;
        AppMetrics::memory_usage_mb(128.0).await;
        AppMetrics::cpu_usage_percent(7.5).await;
        AppMetrics::active_connections(3).await;
        AppMetrics::error_occurred("timeout", "http").await;
    }
}
