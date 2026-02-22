use ahash::AHashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::RwLock;

// ---------------------------------------------------------------------------
// Histogram
// ---------------------------------------------------------------------------

/// A histogram that tracks the distribution of observed values across buckets.
#[derive(Debug)]
pub struct Histogram {
    pub buckets: Vec<f64>,
    pub counts: Vec<AtomicU64>,
    pub sum: AtomicU64,
    pub count: AtomicU64,
}

impl Histogram {
    /// Create a new histogram with the given bucket boundaries.
    pub fn new(buckets: Vec<f64>) -> Self {
        let counts = buckets.iter().map(|_| AtomicU64::new(0)).collect();
        Self {
            buckets,
            counts,
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    /// Record a value into the histogram.
    pub fn observe(&self, value: f64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        // Store sum as bits so we can do atomic add on f64
        loop {
            let current = self.sum.load(Ordering::Relaxed);
            let current_f = f64::from_bits(current);
            let new_f = current_f + value;
            match self.sum.compare_exchange_weak(
                current,
                new_f.to_bits(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(_) => continue,
            }
        }
        for (i, boundary) in self.buckets.iter().enumerate() {
            if value <= *boundary {
                self.counts[i].fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Get the current sum of all observed values.
    pub fn get_sum(&self) -> f64 {
        f64::from_bits(self.sum.load(Ordering::Relaxed))
    }

    /// Get the total number of observations.
    pub fn get_count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

/// Default HTTP duration buckets (in seconds).
fn default_duration_buckets() -> Vec<f64> {
    vec![
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ]
}

// ---------------------------------------------------------------------------
// Label key for counters
// ---------------------------------------------------------------------------

/// A label set is a sorted list of key=value pairs, used to distinguish
/// counter/gauge families.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Labels(Vec<(String, String)>);

impl Labels {
    pub fn new(pairs: &[(&str, &str)]) -> Self {
        let mut v: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        Self(v)
    }

    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Format labels as `{key="value",key2="value2"}` for Prometheus output.
    pub fn prometheus_str(&self) -> String {
        if self.0.is_empty() {
            return String::new();
        }
        let inner: Vec<String> = self
            .0
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, v))
            .collect();
        format!("{{{}}}", inner.join(","))
    }
}

// ---------------------------------------------------------------------------
// MetricsCollector
// ---------------------------------------------------------------------------

/// Central metrics collector supporting counters, gauges, and histograms.
///
/// Thread-safe via interior mutability (`Arc<RwLock<...>>` for dynamic
/// registration, `Atomic*` for values).
#[derive(Debug)]
pub struct MetricsCollector {
    counters: RwLock<AHashMap<(String, Labels), AtomicU64>>,
    gauges: RwLock<AHashMap<String, AtomicI64>>,
    histograms: RwLock<AHashMap<String, Histogram>>,
}

impl MetricsCollector {
    /// Create a new empty collector.
    pub fn new() -> Self {
        Self {
            counters: RwLock::new(AHashMap::new()),
            gauges: RwLock::new(AHashMap::new()),
            histograms: RwLock::new(AHashMap::new()),
        }
    }

    /// Create a collector pre-loaded with the standard auto-tundra metrics.
    pub fn with_defaults() -> Self {
        let collector = Self::new();
        // Pre-register histograms with default buckets
        {
            let mut h = collector.histograms.write().unwrap();
            h.insert(
                "llm_request_duration_seconds".to_string(),
                Histogram::new(default_duration_buckets()),
            );
            h.insert(
                "api_request_duration_seconds".to_string(),
                Histogram::new(default_duration_buckets()),
            );
        }
        collector
    }

    // -- Counters -----------------------------------------------------------

    /// Increment a counter by 1.
    pub fn increment_counter(&self, name: &str, labels: &[(&str, &str)]) {
        self.increment_counter_by(name, labels, 1);
    }

    /// Increment a counter by an arbitrary amount.
    pub fn increment_counter_by(&self, name: &str, labels: &[(&str, &str)], amount: u64) {
        let key = (name.to_string(), Labels::new(labels));
        // Fast-path: read lock
        {
            let map = self.counters.read().unwrap();
            if let Some(c) = map.get(&key) {
                c.fetch_add(amount, Ordering::Relaxed);
                return;
            }
        }
        // Slow-path: write lock to insert
        let mut map = self.counters.write().unwrap();
        let c = map
            .entry(key)
            .or_insert_with(|| AtomicU64::new(0));
        c.fetch_add(amount, Ordering::Relaxed);
    }

    /// Get the current value of a counter.
    pub fn get_counter(&self, name: &str, labels: &[(&str, &str)]) -> u64 {
        let key = (name.to_string(), Labels::new(labels));
        let map = self.counters.read().unwrap();
        map.get(&key)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    // -- Gauges -------------------------------------------------------------

    /// Set a gauge to an absolute value.
    pub fn set_gauge(&self, name: &str, value: i64) {
        {
            let map = self.gauges.read().unwrap();
            if let Some(g) = map.get(name) {
                g.store(value, Ordering::Relaxed);
                return;
            }
        }
        let mut map = self.gauges.write().unwrap();
        let g = map
            .entry(name.to_string())
            .or_insert_with(|| AtomicI64::new(0));
        g.store(value, Ordering::Relaxed);
    }

    /// Get the current value of a gauge.
    pub fn get_gauge(&self, name: &str) -> i64 {
        let map = self.gauges.read().unwrap();
        map.get(name)
            .map(|g| g.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    // -- Histograms ---------------------------------------------------------

    /// Record a value into a histogram. If the histogram does not exist it is
    /// created with default duration buckets.
    pub fn record_histogram(&self, name: &str, value: f64) {
        {
            let map = self.histograms.read().unwrap();
            if let Some(h) = map.get(name) {
                h.observe(value);
                return;
            }
        }
        let mut map = self.histograms.write().unwrap();
        let h = map
            .entry(name.to_string())
            .or_insert_with(|| Histogram::new(default_duration_buckets()));
        h.observe(value);
    }

    // -- Export --------------------------------------------------------------

    /// Export all metrics in Prometheus text exposition format.
    pub fn export_prometheus(&self) -> String {
        let mut out = String::new();

        // Counters
        {
            let map = self.counters.read().unwrap();
            // Group by metric name for TYPE header
            let mut grouped: AHashMap<&str, Vec<(&Labels, u64)>> = AHashMap::new();
            for ((name, labels), val) in map.iter() {
                let v = val.load(Ordering::Relaxed);
                grouped
                    .entry(name.as_str())
                    .or_default()
                    .push((labels, v));
            }
            let mut names: Vec<&&str> = grouped.keys().collect();
            names.sort();
            for name in names {
                out.push_str(&format!("# TYPE {} counter\n", name));
                let entries = &grouped[name];
                for (labels, value) in entries {
                    out.push_str(&format!(
                        "{}{} {}\n",
                        name,
                        labels.prometheus_str(),
                        value
                    ));
                }
            }
        }

        // Gauges
        {
            let map = self.gauges.read().unwrap();
            let mut names: Vec<&String> = map.keys().collect();
            names.sort();
            for name in names {
                let val = map[name].load(Ordering::Relaxed);
                out.push_str(&format!("# TYPE {} gauge\n", name));
                out.push_str(&format!("{} {}\n", name, val));
            }
        }

        // Histograms
        {
            let map = self.histograms.read().unwrap();
            let mut names: Vec<&String> = map.keys().collect();
            names.sort();
            for name in names {
                let h = &map[name];
                out.push_str(&format!("# TYPE {} histogram\n", name));
                let mut cumulative = 0u64;
                for (i, boundary) in h.buckets.iter().enumerate() {
                    cumulative += h.counts[i].load(Ordering::Relaxed);
                    out.push_str(&format!(
                        "{}_bucket{{le=\"{}\"}} {}\n",
                        name, boundary, cumulative
                    ));
                }
                out.push_str(&format!("{}_bucket{{le=\"+Inf\"}} {}\n", name, h.get_count()));
                out.push_str(&format!("{}_sum {}\n", name, h.get_sum()));
                out.push_str(&format!("{}_count {}\n", name, h.get_count()));
            }
        }

        out
    }

    /// Export all metrics as a JSON value.
    pub fn export_json(&self) -> serde_json::Value {
        let mut counters_json = serde_json::Map::new();
        {
            let map = self.counters.read().unwrap();
            for ((name, labels), val) in map.iter() {
                let v = val.load(Ordering::Relaxed);
                let key = if labels.0.is_empty() {
                    name.clone()
                } else {
                    format!("{}{}", name, labels.prometheus_str())
                };
                counters_json.insert(key, serde_json::json!(v));
            }
        }

        let mut gauges_json = serde_json::Map::new();
        {
            let map = self.gauges.read().unwrap();
            for (name, val) in map.iter() {
                gauges_json.insert(
                    name.clone(),
                    serde_json::json!(val.load(Ordering::Relaxed)),
                );
            }
        }

        let mut histograms_json = serde_json::Map::new();
        {
            let map = self.histograms.read().unwrap();
            for (name, h) in map.iter() {
                let buckets: Vec<serde_json::Value> = h
                    .buckets
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        serde_json::json!({
                            "le": b,
                            "count": h.counts[i].load(Ordering::Relaxed),
                        })
                    })
                    .collect();
                histograms_json.insert(
                    name.clone(),
                    serde_json::json!({
                        "buckets": buckets,
                        "sum": h.get_sum(),
                        "count": h.get_count(),
                    }),
                );
            }
        }

        serde_json::json!({
            "counters": counters_json,
            "gauges": gauges_json,
            "histograms": histograms_json,
        })
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ---------------------------------------------------------------------------
// Global singleton
// ---------------------------------------------------------------------------

/// Returns a reference to the global `MetricsCollector` singleton.
///
/// The collector is created once with default metrics and shared across the
/// entire process.
pub fn global_metrics() -> &'static MetricsCollector {
    use std::sync::OnceLock;
    static INSTANCE: OnceLock<MetricsCollector> = OnceLock::new();
    INSTANCE.get_or_init(MetricsCollector::with_defaults)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_increment() {
        let m = MetricsCollector::new();
        m.increment_counter("beads_total", &[("status", "done")]);
        m.increment_counter("beads_total", &[("status", "done")]);
        m.increment_counter("beads_total", &[("status", "failed")]);

        assert_eq!(m.get_counter("beads_total", &[("status", "done")]), 2);
        assert_eq!(m.get_counter("beads_total", &[("status", "failed")]), 1);
        assert_eq!(m.get_counter("beads_total", &[("status", "review")]), 0);
    }

    #[test]
    fn test_counter_increment_by() {
        let m = MetricsCollector::new();
        m.increment_counter_by("llm_tokens_used", &[("direction", "input")], 150);
        m.increment_counter_by("llm_tokens_used", &[("direction", "input")], 50);
        assert_eq!(
            m.get_counter("llm_tokens_used", &[("direction", "input")]),
            200
        );
    }

    #[test]
    fn test_gauge_set() {
        let m = MetricsCollector::new();
        m.set_gauge("beads_active", 5);
        assert_eq!(m.get_gauge("beads_active"), 5);
        m.set_gauge("beads_active", 3);
        assert_eq!(m.get_gauge("beads_active"), 3);
    }

    #[test]
    fn test_histogram_record() {
        let m = MetricsCollector::with_defaults();
        m.record_histogram("api_request_duration_seconds", 0.05);
        m.record_histogram("api_request_duration_seconds", 0.5);
        m.record_histogram("api_request_duration_seconds", 2.0);

        let map = m.histograms.read().unwrap();
        let h = map.get("api_request_duration_seconds").unwrap();
        assert_eq!(h.get_count(), 3);
        let sum = h.get_sum();
        assert!((sum - 2.55).abs() < 0.001);
    }

    #[test]
    fn test_prometheus_export() {
        let m = MetricsCollector::new();
        m.increment_counter("beads_total", &[("status", "done")]);
        m.set_gauge("agents_running", 2);
        m.record_histogram("api_request_duration_seconds", 0.1);

        let output = m.export_prometheus();
        assert!(output.contains("# TYPE beads_total counter"));
        assert!(output.contains("beads_total{status=\"done\"} 1"));
        assert!(output.contains("# TYPE agents_running gauge"));
        assert!(output.contains("agents_running 2"));
        assert!(output.contains("# TYPE api_request_duration_seconds histogram"));
        assert!(output.contains("api_request_duration_seconds_count 1"));
    }

    #[test]
    fn test_json_export() {
        let m = MetricsCollector::new();
        m.increment_counter("beads_total", &[("status", "done")]);
        m.set_gauge("agents_running", 4);

        let json = m.export_json();
        assert_eq!(json["gauges"]["agents_running"], 4);
        assert!(json["counters"].is_object());
    }

    #[test]
    fn test_labels_prometheus_format() {
        let l = Labels::new(&[("method", "GET"), ("status", "200")]);
        assert_eq!(l.prometheus_str(), "{method=\"GET\",status=\"200\"}");

        let empty = Labels::empty();
        assert_eq!(empty.prometheus_str(), "");
    }

    #[test]
    fn test_global_metrics_singleton() {
        let m1 = global_metrics();
        let m2 = global_metrics();
        // Should be the same pointer
        assert!(std::ptr::eq(m1, m2));
    }
}
