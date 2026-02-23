#![allow(dead_code)]
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Performance benchmarks for Datadog profiling
pub struct PerformanceBenchmarks;

impl PerformanceBenchmarks {
    /// Benchmark 1: Span creation overhead
    pub async fn benchmark_span_creation() -> Result<()> {
        println!("🏁 Benchmark: Span Creation Overhead");

        let iterations = 100_000;
        let start = Instant::now();

        for i in 0..iterations {
            let _span = crate::traced_span!("benchmark_span", iteration = i);
            // Simulate minimal work
            std::hint::black_box(42);
        }

        let duration = start.elapsed();
        let per_span = duration.as_nanos() / iterations as u128;

        println!("📊 Span Creation Results:");
        println!("   Total iterations: {}", iterations);
        println!("   Total duration: {:?}", duration);
        println!("   Per span overhead: {}ns", per_span);
        println!(
            "   Rating: {}",
            if per_span < 100 {
                "🟢 Excellent"
            } else if per_span < 200 {
                "🟡 Good"
            } else {
                "🔴 Needs Optimization"
            }
        );

        Ok(())
    }

    /// Benchmark 2: Metric recording latency
    pub async fn benchmark_metrics_recording() -> Result<()> {
        println!("🏁 Benchmark: Metrics Recording Latency");

        let iterations = 10_000;
        let start = Instant::now();

        for i in 0..iterations {
            crate::metrics::AppMetrics::daemon_started().await;
            crate::metrics::AppMetrics::llm_profile_bootstrap(
                3,
                "benchmark-model",
                "benchmark-provider",
            )
            .await;

            if i % 1000 == 0 {
                tokio::task::yield_now().await;
            }
        }

        let duration = start.elapsed();
        let per_metric = duration.as_nanos() / (iterations * 2) as u128; // 2 metrics per iteration

        println!("📊 Metrics Recording Results:");
        println!("   Total metrics: {}", iterations * 2);
        println!("   Total duration: {:?}", duration);
        println!("   Per metric latency: {}μs", per_metric / 1000);
        println!(
            "   Rating: {}",
            if per_metric < 50_000 {
                "🟢 Excellent"
            } else if per_metric < 100_000 {
                "🟡 Good"
            } else {
                "🔴 Needs Optimization"
            }
        );

        Ok(())
    }

    /// Benchmark 3: Concurrent operations throughput
    pub async fn benchmark_concurrent_operations() -> Result<()> {
        println!("🏁 Benchmark: Concurrent Operations Throughput");

        let concurrent_tasks = 1_000;
        let operations_per_task = 100;

        let start = Instant::now();
        let mut handles = vec![];

        for task_id in 0..concurrent_tasks {
            let handle = tokio::spawn(async move {
                let task_start = Instant::now();

                for op_id in 0..operations_per_task {
                    let _span = crate::traced_span!(
                        "concurrent_benchmark",
                        task_id = task_id,
                        operation_id = op_id
                    );

                    // Simulate work
                    tokio::time::sleep(Duration::from_nanos(100)).await;

                    // Record metrics
                    crate::metrics::AppMetrics::task_executed("benchmark_task", true, 0.1).await;
                }

                task_start.elapsed()
            });

            handles.push(handle);
        }

        // Wait for all tasks and collect durations
        let mut task_durations = vec![];
        for handle in handles {
            task_durations.push(handle.await?);
        }

        let total_duration = start.elapsed();
        let total_operations = concurrent_tasks * operations_per_task;
        let throughput = total_operations as f64 / total_duration.as_secs_f64();

        let avg_task_duration =
            task_durations.iter().sum::<Duration>() / task_durations.len() as u32;

        println!("📊 Concurrent Operations Results:");
        println!("   Concurrent tasks: {}", concurrent_tasks);
        println!("   Operations per task: {}", operations_per_task);
        println!("   Total operations: {}", total_operations);
        println!("   Total duration: {:?}", total_duration);
        println!("   Throughput: {:.2} ops/sec", throughput);
        println!("   Avg task duration: {:?}", avg_task_duration);
        println!(
            "   Rating: {}",
            if throughput > 10_000.0 {
                "🟢 Excellent"
            } else if throughput > 5_000.0 {
                "🟡 Good"
            } else {
                "🔴 Needs Optimization"
            }
        );

        Ok(())
    }

    /// Benchmark 4: Memory usage efficiency
    pub async fn benchmark_memory_usage() -> Result<()> {
        println!("🏁 Benchmark: Memory Usage Efficiency");

        let initial_memory = Self::get_memory_usage();

        // Create many spans and metrics
        let iterations = 50_000;
        let spans = Arc::new(RwLock::new(Vec::new()));

        for i in 0..iterations {
            let span = crate::traced_span!("memory_benchmark", iteration = i);
            spans.write().await.push(span);

            crate::metrics::AppMetrics::memory_usage_mb((i % 1000) as f64).await;

            if i % 10000 == 0 {
                tokio::task::yield_now().await;
            }
        }

        let peak_memory = Self::get_memory_usage();
        let memory_increase = peak_memory.saturating_sub(initial_memory);
        let memory_per_span = memory_increase as f64 / iterations as f64;

        // Clean up
        spans.write().await.clear();
        tokio::task::yield_now().await;

        let final_memory = Self::get_memory_usage();
        let memory_recovered = peak_memory.saturating_sub(final_memory);

        println!("📊 Memory Usage Results:");
        println!("   Initial memory: {}MB", initial_memory);
        println!("   Peak memory: {}MB", peak_memory);
        println!("   Memory increase: {}MB", memory_increase);
        println!("   Memory per span: {:.2}KB", memory_per_span * 1024.0);
        println!("   Memory recovered: {}MB", memory_recovered);
        println!(
            "   Rating: {}",
            if memory_per_span < 0.5 {
                "🟢 Excellent"
            } else if memory_per_span < 1.0 {
                "🟡 Good"
            } else {
                "🔴 Needs Optimization"
            }
        );

        Ok(())
    }

    /// Benchmark 5: Datadog agent throughput
    pub async fn benchmark_datadog_throughput() -> Result<()> {
        println!("🏁 Benchmark: Datadog Agent Throughput");

        let events_per_second = 1_000;
        let duration_seconds = 5;
        let _total_events = events_per_second * duration_seconds;

        let start = Instant::now();
        let mut sent_events = 0;

        while start.elapsed() < Duration::from_secs(duration_seconds) {
            let batch_start = Instant::now();

            for i in 0..events_per_second {
                crate::profiling::record_event(
                    "benchmark_event",
                    &[
                        ("event_id", &i.to_string()),
                        ("timestamp", &chrono::Utc::now().to_rfc3339()),
                        ("component", "benchmark"),
                    ],
                );

                sent_events += 1;
            }

            let batch_duration = batch_start.elapsed();
            if batch_duration < Duration::from_secs(1) {
                tokio::time::sleep(Duration::from_secs(1) - batch_duration).await;
            }
        }

        let total_duration = start.elapsed();
        let actual_throughput = sent_events as f64 / total_duration.as_secs_f64();

        println!("📊 Datadog Throughput Results:");
        println!("   Target events/sec: {}", events_per_second);
        println!("   Actual events/sec: {:.2}", actual_throughput);
        println!("   Total events: {}", sent_events);
        println!("   Total duration: {:?}", total_duration);
        println!(
            "   Rating: {}",
            if actual_throughput > 800.0 {
                "🟢 Excellent"
            } else if actual_throughput > 500.0 {
                "🟡 Good"
            } else {
                "🔴 Needs Optimization"
            }
        );

        Ok(())
    }

    /// Get current memory usage in MB
    fn get_memory_usage() -> u64 {
        // This is a simplified implementation
        // In production, you'd use a proper memory usage library
        use std::fs;

        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb / 1024; // Convert to MB
                        }
                    }
                }
            }
        }

        // Fallback for non-Linux systems
        100 // Default estimate
    }

    /// Run all benchmarks
    pub async fn run_all_benchmarks() -> Result<()> {
        println!("🚀 Starting Performance Benchmarks\n");

        let benchmark_start = Instant::now();

        // Run all benchmarks
        Self::benchmark_span_creation().await?;
        println!();

        Self::benchmark_metrics_recording().await?;
        println!();

        Self::benchmark_concurrent_operations().await?;
        println!();

        Self::benchmark_memory_usage().await?;
        println!();

        Self::benchmark_datadog_throughput().await?;

        let total_duration = benchmark_start.elapsed();

        println!("\n🎉 All benchmarks completed!");
        println!("📊 Total benchmark duration: {:?}", total_duration);
        println!("✅ Performance analysis complete!");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_performance_benchmarks() {
        PerformanceBenchmarks::run_all_benchmarks()
            .await
            .expect("Benchmarks should complete successfully");
    }
}
