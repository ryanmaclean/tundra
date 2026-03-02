/// End-to-end memory leak verification test
///
/// This test verifies that the memory leak fixes work correctly by:
/// 1. Creating tasks with heavy logging (simulating long-running daemon)
/// 2. Archiving completed tasks
/// 3. Running cleanup cycle
/// 4. Verifying tasks are removed from memory
///
/// This test confirms that archived tasks with completed_at timestamps older
/// than the TTL are removed from the tasks HashMap, preventing unbounded growth.
///
/// Note: The archived_tasks Vec is not cleaned up (by design) - it's just a list
/// of task IDs. The main memory consumption comes from the Task objects in the
/// tasks HashMap, which contain all the log entries.
use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::ApiState;
use at_core::types::{
    BuildLogEntry, BuildStream, Task, TaskCategory, TaskComplexity, TaskLogEntry, TaskLogType,
    TaskPhase, TaskPriority,
};

/// Create a task with heavy logging to simulate long-running tasks
fn create_task_with_heavy_logging(
    title: String,
    log_count: usize,
    completed_days_ago: i64,
) -> Task {
    let mut task = Task::new(
        title,
        uuid::Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );

    // Add heavy logging to simulate long-running tasks
    for i in 0..log_count {
        // Add task logs
        task.logs.push(TaskLogEntry {
            timestamp: chrono::Utc::now(),
            phase: TaskPhase::Coding,
            log_type: TaskLogType::Info,
            message: format!(
                "Task log entry {} - simulating long-running operation with detailed output",
                i
            ),
            detail: Some(format!("Detailed information for entry {}", i)),
        });

        // Add build logs
        task.build_logs.push(BuildLogEntry {
            timestamp: chrono::Utc::now(),
            stream: if i % 2 == 0 {
                BuildStream::Stdout
            } else {
                BuildStream::Stderr
            },
            line: format!(
                "Build output line {} - compiling modules and running tests with verbose output\n",
                i
            ),
            phase: TaskPhase::Coding,
        });
    }

    // Mark as completed with timestamp in the past
    task.completed_at = Some(chrono::Utc::now() - chrono::Duration::days(completed_days_ago));

    task
}

#[tokio::test]
async fn test_e2e_memory_leak_verification_100_tasks() {
    println!("Starting end-to-end memory leak verification test...");

    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus));

    // Step 1: Create 100 tasks with heavy logging
    println!("\n[Step 1] Creating 100 tasks with heavy logging...");
    let mut task_ids = Vec::new();

    {
        let mut tasks = state.tasks.write().await;

        for i in 0..100 {
            // Create tasks with varying ages and log counts
            let log_count = 100 + (i * 10); // 100 to 1090 logs per task
            let days_ago = 30 + (i / 10); // 30 to 39 days old

            let task = create_task_with_heavy_logging(
                format!("Heavy Task {}", i),
                log_count,
                days_ago as i64,
            );

            let task_id = task.id;
            tasks.insert(task_id, task);
            task_ids.push(task_id);
        }

        state
            .task_count
            .store(tasks.len(), std::sync::atomic::Ordering::Relaxed);
    }

    println!("✓ Created 100 tasks with heavy logging");

    // Step 2: Check initial memory usage
    println!("\n[Step 2] Checking initial memory usage...");
    let tasks_before = {
        let tasks = state.tasks.read().await;
        tasks.len()
    };
    let archived_before = {
        let archived = state.archived_tasks.read().await;
        archived.len()
    };

    println!("  Tasks in memory: {}", tasks_before);
    println!("  Archived tasks: {}", archived_before);
    assert_eq!(tasks_before, 100, "Should have 100 tasks in memory");
    assert_eq!(archived_before, 0, "Should have no archived tasks yet");

    // Step 3: Archive all tasks
    println!("\n[Step 3] Archiving all 100 tasks...");
    {
        let mut archived = state.archived_tasks.write().await;
        for task_id in &task_ids {
            archived.push(*task_id);
        }
    }
    println!("✓ Archived all 100 tasks");

    // Step 4: Verify tasks are archived but still in memory
    println!("\n[Step 4] Verifying tasks are archived...");
    let tasks_after_archive = {
        let tasks = state.tasks.read().await;
        tasks.len()
    };
    let archived_after_archive = {
        let archived = state.archived_tasks.read().await;
        archived.len()
    };

    println!("  Tasks in memory: {}", tasks_after_archive);
    println!("  Archived tasks: {}", archived_after_archive);
    assert_eq!(
        tasks_after_archive, 100,
        "All tasks should still be in memory"
    );
    assert_eq!(
        archived_after_archive, 100,
        "All tasks should be marked as archived"
    );

    // Step 5: Manually trigger cleanup (tasks older than 7 days)
    println!("\n[Step 5] Running cleanup for tasks older than 7 days...");
    let ttl_secs = 7 * 24 * 3600; // 7 days
    state.cleanup_archived_tasks(ttl_secs).await;
    println!("✓ Cleanup completed");

    // Step 6: Verify memory usage is reduced
    println!("\n[Step 6] Verifying memory usage reduction...");
    let tasks_after_cleanup = {
        let tasks = state.tasks.read().await;
        tasks.len()
    };
    let archived_after_cleanup = {
        let archived = state.archived_tasks.read().await;
        archived.len()
    };

    println!("  Tasks in memory: {}", tasks_after_cleanup);
    println!("  Archived tasks: {}", archived_after_cleanup);

    // All tasks should be removed from HashMap because they're all older than 7 days (30-39 days old)
    assert_eq!(
        tasks_after_cleanup, 0,
        "All old archived tasks should be removed from memory"
    );
    // Note: archived_tasks Vec is not cleaned up - it's just task IDs (minimal memory overhead)
    assert_eq!(
        archived_after_cleanup, 100,
        "Archived list retains IDs for historical reference"
    );

    // Calculate memory reduction
    let reduction = tasks_before - tasks_after_cleanup;
    println!("\n✓ Memory leak fix verified!");
    println!("  Tasks removed: {}", reduction);
    println!("  Memory reduction: 100%");

    println!("\n[SUCCESS] End-to-end memory leak verification passed!");
}

#[tokio::test]
async fn test_background_cleanup_cycle() {
    println!("Starting background cleanup cycle test...");

    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus));

    // Create tasks with different ages
    println!("\n[Step 1] Creating tasks with different ages...");
    let mut old_task_ids = Vec::new();
    let mut recent_task_ids = Vec::new();

    {
        let mut tasks = state.tasks.write().await;

        // Create 50 old tasks (30 days ago)
        for i in 0..50 {
            let task = create_task_with_heavy_logging(
                format!("Old Task {}", i),
                200, // 200 logs per task
                30,  // 30 days old
            );
            let task_id = task.id;
            tasks.insert(task_id, task);
            old_task_ids.push(task_id);
        }

        // Create 50 recent tasks (1 day ago)
        for i in 0..50 {
            let task = create_task_with_heavy_logging(
                format!("Recent Task {}", i),
                200, // 200 logs per task
                1,   // 1 day old
            );
            let task_id = task.id;
            tasks.insert(task_id, task);
            recent_task_ids.push(task_id);
        }

        state
            .task_count
            .store(tasks.len(), std::sync::atomic::Ordering::Relaxed);
    }

    println!("✓ Created 50 old tasks and 50 recent tasks");

    // Archive all tasks
    println!("\n[Step 2] Archiving all tasks...");
    {
        let mut archived = state.archived_tasks.write().await;
        for task_id in old_task_ids.iter().chain(recent_task_ids.iter()) {
            archived.push(*task_id);
        }
    }

    // Manually trigger cleanup with 7-day TTL to test selective cleanup
    println!("\n[Step 3] Running cleanup with 7-day TTL...");
    state.cleanup_archived_tasks(7 * 24 * 3600).await;

    // Verify selective cleanup
    println!("\n[Step 4] Verifying selective cleanup...");
    let tasks_after = {
        let tasks = state.tasks.read().await;
        tasks.len()
    };
    let archived_after = {
        let archived = state.archived_tasks.read().await;
        archived.len()
    };

    println!("  Tasks in memory: {}", tasks_after);
    println!("  Archived tasks: {}", archived_after);

    // Only recent tasks (1 day old) should remain in HashMap
    assert_eq!(tasks_after, 50, "Recent tasks should remain in memory");
    // Note: archived_tasks Vec retains all IDs (both old and recent)
    assert_eq!(
        archived_after, 100,
        "Archived list retains all IDs for historical reference"
    );

    println!("\n✓ Selective cleanup works correctly - old tasks removed, recent tasks kept");
    println!("\n[SUCCESS] Background cleanup cycle test passed!");
}

#[tokio::test]
async fn test_stress_cleanup_with_buffers() {
    println!("Starting stress test with disconnect buffers...");

    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus));

    // Create 100 tasks with heavy logging
    println!("\n[Step 1] Creating 100 tasks...");
    {
        let mut tasks = state.tasks.write().await;
        for i in 0..100 {
            let task = create_task_with_heavy_logging(
                format!("Stress Task {}", i),
                500, // 500 logs per task (heavy)
                30,  // 30 days old
            );
            tasks.insert(task.id, task);
        }
        state
            .task_count
            .store(tasks.len(), std::sync::atomic::Ordering::Relaxed);
    }

    // Add disconnect buffers
    println!("\n[Step 2] Creating 50 old disconnect buffers...");
    {
        let mut buffers = state.disconnect_buffers.write().await;
        for _i in 0..50 {
            let session_id = uuid::Uuid::new_v4();
            let mut buffer = at_bridge::terminal::DisconnectBuffer::new(10_000); // 10KB max
            buffer.disconnected_at = chrono::Utc::now() - chrono::Duration::minutes(10);

            // Add some data to the buffer
            for j in 0..100 {
                let data = format!("Buffer data {}\n", j);
                buffer.push(data.as_bytes());
            }

            buffers.insert(session_id, buffer);
        }
    }

    // Check initial memory
    println!("\n[Step 3] Checking initial memory usage...");
    let tasks_before = {
        let tasks = state.tasks.read().await;
        tasks.len()
    };
    let buffers_before = {
        let buffers = state.disconnect_buffers.read().await;
        buffers.len()
    };

    println!("  Tasks: {}", tasks_before);
    println!("  Disconnect buffers: {}", buffers_before);
    assert_eq!(tasks_before, 100);
    assert_eq!(buffers_before, 50);

    // Archive all tasks
    println!("\n[Step 4] Archiving all tasks...");
    {
        let task_ids: Vec<_> = {
            let tasks = state.tasks.read().await;
            tasks.keys().copied().collect()
        };

        let mut archived = state.archived_tasks.write().await;
        for task_id in task_ids {
            archived.push(task_id);
        }
    }

    // Run comprehensive cleanup
    println!("\n[Step 5] Running comprehensive cleanup...");
    state.cleanup_archived_tasks(7 * 24 * 3600).await;
    state.cleanup_disconnect_buffers(5 * 60).await; // 5 minutes

    // Check memory after cleanup
    println!("\n[Step 6] Verifying memory after cleanup...");
    let tasks_after = {
        let tasks = state.tasks.read().await;
        tasks.len()
    };
    let buffers_after = {
        let buffers = state.disconnect_buffers.read().await;
        buffers.len()
    };

    println!("  Tasks: {}", tasks_after);
    println!("  Disconnect buffers: {}", buffers_after);

    // Verify cleanup
    assert_eq!(
        tasks_after, 0,
        "All archived tasks should be removed from HashMap"
    );
    assert_eq!(
        buffers_after, 0,
        "All old disconnect buffers should be removed"
    );

    println!("\n✓ Comprehensive cleanup successful - old tasks and buffers removed");
    println!("\n[SUCCESS] Stress test with disconnect buffers passed!");
}
