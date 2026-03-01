use at_core::types::*;
use uuid::Uuid;

/// Stress test: verify log truncation with 50k entries as specified in the plan
#[test]
fn stress_test_50k_entries() {
    let mut task = Task::new(
        "50k stress test",
        Uuid::new_v4(),
        TaskCategory::Performance,
        TaskPriority::High,
        TaskComplexity::Complex,
    );

    // Simulate a long-running task with 50,000 log entries
    for i in 0..50_000 {
        task.log(TaskLogType::Info, format!("Entry {}", i));
        task.add_build_log(BuildStream::Stdout, format!("Build {}", i));
    }
    assert_eq!(task.logs.len(), 50_000);
    assert_eq!(task.build_logs.len(), 50_000);

    // Truncate to 1,000 entries as specified in the plan
    task.truncate_logs(1_000);
    assert_eq!(task.logs.len(), 1_000);
    assert_eq!(task.build_logs.len(), 1_000);

    // Verify we kept the most recent entries (49,000-49,999)
    assert_eq!(task.logs[0].message, "Entry 49000");
    assert_eq!(task.logs[999].message, "Entry 49999");
    assert_eq!(task.build_logs[0].line, "Build 49000");
    assert_eq!(task.build_logs[999].line, "Build 49999");
}

/// Stress test: verify memory doesn't grow unbounded with incremental additions
#[test]
fn stress_test_incremental_growth_with_periodic_truncation() {
    let mut task = Task::new(
        "incremental growth",
        Uuid::new_v4(),
        TaskCategory::Performance,
        TaskPriority::High,
        TaskComplexity::Complex,
    );

    // Simulate a daemon running for extended period with periodic cleanup
    // Add logs in batches, truncating periodically
    for batch in 0..100 {
        // Add 1000 logs per batch
        for i in 0..1_000 {
            let entry_num = batch * 1_000 + i;
            task.log(TaskLogType::Info, format!("Entry {}", entry_num));
            task.add_build_log(BuildStream::Stdout, format!("Build {}", entry_num));
        }

        // After each batch, truncate to max retention (simulate periodic cleanup)
        task.truncate_logs(10_000);
    }

    // After 100 batches (100k total entries generated), we should only have 10k
    assert_eq!(task.logs.len(), 10_000);
    assert_eq!(task.build_logs.len(), 10_000);

    // Verify we kept the most recent 10k entries (90,000-99,999)
    assert_eq!(task.logs[0].message, "Entry 90000");
    assert_eq!(task.logs[9_999].message, "Entry 99999");
    assert_eq!(task.build_logs[0].line, "Build 90000");
    assert_eq!(task.build_logs[9_999].line, "Build 99999");
}

/// Stress test: verify large truncation target doesn't cause issues
#[test]
fn stress_test_large_truncation_target() {
    let mut task = Task::new(
        "large target",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );

    // Add moderate number of entries
    for i in 0..5_000 {
        task.log(TaskLogType::Info, format!("Entry {}", i));
        task.add_build_log(BuildStream::Stdout, format!("Build {}", i));
    }

    // Truncate with very large target (larger than actual size)
    task.truncate_logs(100_000);

    // Should keep all entries since target > actual
    assert_eq!(task.logs.len(), 5_000);
    assert_eq!(task.build_logs.len(), 5_000);
    assert_eq!(task.logs[0].message, "Entry 0");
    assert_eq!(task.logs[4_999].message, "Entry 4999");
}

/// Stress test: verify consecutive truncations work correctly
#[test]
fn stress_test_consecutive_truncations() {
    let mut task = Task::new(
        "consecutive truncations",
        Uuid::new_v4(),
        TaskCategory::Performance,
        TaskPriority::High,
        TaskComplexity::Complex,
    );

    // Add 100k entries
    for i in 0..100_000 {
        task.log(TaskLogType::Info, format!("Entry {}", i));
        task.add_build_log(BuildStream::Stdout, format!("Build {}", i));
    }
    assert_eq!(task.logs.len(), 100_000);

    // First truncation to 50k
    task.truncate_logs(50_000);
    assert_eq!(task.logs.len(), 50_000);
    assert_eq!(task.logs[0].message, "Entry 50000");
    assert_eq!(task.logs[49_999].message, "Entry 99999");

    // Second truncation to 10k
    task.truncate_logs(10_000);
    assert_eq!(task.logs.len(), 10_000);
    assert_eq!(task.logs[0].message, "Entry 90000");
    assert_eq!(task.logs[9_999].message, "Entry 99999");

    // Third truncation to 1k
    task.truncate_logs(1_000);
    assert_eq!(task.logs.len(), 1_000);
    assert_eq!(task.logs[0].message, "Entry 99000");
    assert_eq!(task.logs[999].message, "Entry 99999");

    // Final truncation to 100
    task.truncate_logs(100);
    assert_eq!(task.logs.len(), 100);
    assert_eq!(task.logs[0].message, "Entry 99900");
    assert_eq!(task.logs[99].message, "Entry 99999");
}

/// Stress test: verify mixed log types are preserved correctly during truncation
#[test]
fn stress_test_mixed_log_types() {
    let mut task = Task::new(
        "mixed log types",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );

    // Add various log types in a pattern
    for i in 0..10_000 {
        let log_type = match i % 4 {
            0 => TaskLogType::Info,
            1 => TaskLogType::Success,
            2 => TaskLogType::Error,
            _ => TaskLogType::Text,
        };
        task.log(log_type, format!("Entry {}", i));

        let stream = if i % 2 == 0 {
            BuildStream::Stdout
        } else {
            BuildStream::Stderr
        };
        task.add_build_log(stream, format!("Build {}", i));
    }

    // Truncate to 1k
    task.truncate_logs(1_000);
    assert_eq!(task.logs.len(), 1_000);
    assert_eq!(task.build_logs.len(), 1_000);

    // Verify the most recent 1k entries are preserved
    assert_eq!(task.logs[0].message, "Entry 9000");
    assert_eq!(task.logs[999].message, "Entry 9999");

    // Verify log types are preserved correctly
    assert_eq!(task.logs[0].log_type, TaskLogType::Info); // 9000 % 4 == 0
    assert_eq!(task.logs[1].log_type, TaskLogType::Success); // 9001 % 4 == 1
    assert_eq!(task.logs[2].log_type, TaskLogType::Error); // 9002 % 4 == 2
    assert_eq!(task.logs[3].log_type, TaskLogType::Text); // 9003 % 4 == 3

    // Verify stream types are preserved
    assert_eq!(task.build_logs[0].stream, BuildStream::Stdout); // 9000 % 2 == 0
    assert_eq!(task.build_logs[1].stream, BuildStream::Stderr); // 9001 % 2 == 1
}

/// Stress test: verify empty logs handle truncation correctly
#[test]
fn stress_test_empty_logs() {
    let mut task = Task::new(
        "empty logs",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Low,
        TaskComplexity::Trivial,
    );

    // Task starts with no logs
    assert_eq!(task.logs.len(), 0);
    assert_eq!(task.build_logs.len(), 0);

    // Truncate empty logs - should be no-op
    task.truncate_logs(1_000);
    assert_eq!(task.logs.len(), 0);
    assert_eq!(task.build_logs.len(), 0);

    // Add one entry and truncate to zero
    task.log(TaskLogType::Info, "Single entry");
    task.add_build_log(BuildStream::Stdout, "Single build");
    task.truncate_logs(0);
    assert_eq!(task.logs.len(), 0);
    assert_eq!(task.build_logs.len(), 0);
}

/// Stress test: verify asymmetric log counts (different sizes for logs vs build_logs)
#[test]
fn stress_test_asymmetric_log_counts() {
    let mut task = Task::new(
        "asymmetric logs",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );

    // Add many more task logs than build logs
    for i in 0..20_000 {
        task.log(TaskLogType::Info, format!("Entry {}", i));
    }
    for i in 0..5_000 {
        task.add_build_log(BuildStream::Stdout, format!("Build {}", i));
    }
    assert_eq!(task.logs.len(), 20_000);
    assert_eq!(task.build_logs.len(), 5_000);

    // Truncate to 3k - should affect logs but not build_logs
    task.truncate_logs(3_000);
    assert_eq!(task.logs.len(), 3_000);
    assert_eq!(task.build_logs.len(), 3_000);

    // Verify correct entries were kept
    assert_eq!(task.logs[0].message, "Entry 17000");
    assert_eq!(task.logs[2_999].message, "Entry 19999");
    assert_eq!(task.build_logs[0].line, "Build 2000");
    assert_eq!(task.build_logs[2_999].line, "Build 4999");
}

/// Stress test: verify truncation with single entry
#[test]
fn stress_test_single_entry_truncation() {
    let mut task = Task::new(
        "single entry",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Low,
        TaskComplexity::Trivial,
    );

    // Add single entry
    task.log(TaskLogType::Info, "Only entry");
    task.add_build_log(BuildStream::Stdout, "Only build");

    // Truncate to 1 - should keep the entry
    task.truncate_logs(1);
    assert_eq!(task.logs.len(), 1);
    assert_eq!(task.build_logs.len(), 1);
    assert_eq!(task.logs[0].message, "Only entry");
    assert_eq!(task.build_logs[0].line, "Only build");

    // Truncate to 0 - should remove all
    task.truncate_logs(0);
    assert_eq!(task.logs.len(), 0);
    assert_eq!(task.build_logs.len(), 0);
}

/// Stress test: verify very large entries don't cause memory issues
#[test]
fn stress_test_very_large_log_entries() {
    let mut task = Task::new(
        "large entries",
        Uuid::new_v4(),
        TaskCategory::Performance,
        TaskPriority::High,
        TaskComplexity::Complex,
    );

    // Create large log messages (simulate verbose output)
    let large_message = "A".repeat(1_000); // 1KB per message
    let large_build_output = "B".repeat(1_000); // 1KB per message

    // Add 10k large entries = ~10MB of log data
    for i in 0..10_000 {
        task.log(TaskLogType::Info, format!("{}-{}", large_message, i));
        task.add_build_log(BuildStream::Stdout, format!("{}-{}", large_build_output, i));
    }
    assert_eq!(task.logs.len(), 10_000);

    // Truncate to reasonable size
    task.truncate_logs(1_000);
    assert_eq!(task.logs.len(), 1_000);
    assert_eq!(task.build_logs.len(), 1_000);

    // Verify the most recent entries with large payloads are preserved
    assert!(task.logs[0].message.starts_with(&large_message));
    assert!(task.logs[0].message.ends_with("-9000"));
    assert!(task.logs[999].message.ends_with("-9999"));
}
