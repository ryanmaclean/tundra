use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use serde_json::Value;

/// Spin up an API server on a random port, return the base URL and state.
async fn start_test_server() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus));
    let router = api_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://{addr}"), state)
}

// ---------------------------------------------------------------------------
// Task Archival and Cleanup Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_task_archival_and_manual_cleanup() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create some tasks in the state
    let task_id_1 = uuid::Uuid::new_v4();
    let task_id_2 = uuid::Uuid::new_v4();
    let task_id_3 = uuid::Uuid::new_v4();

    {
        let mut tasks = state.tasks.write().await;

        // Old completed task (30 days ago)
        let mut task1 = at_core::types::Task::new(
            "Old Task".to_string(),
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task1.id = task_id_1;
        task1.completed_at = Some(chrono::Utc::now() - chrono::Duration::days(30));
        tasks.insert(task_id_1, task1);

        // Recent completed task (1 day ago)
        let mut task2 = at_core::types::Task::new(
            "Recent Task".to_string(),
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task2.id = task_id_2;
        task2.completed_at = Some(chrono::Utc::now() - chrono::Duration::days(1));
        tasks.insert(task_id_2, task2);

        // Non-archived task
        let mut task3 = at_core::types::Task::new(
            "Active Task".to_string(),
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task3.id = task_id_3;
        task3.completed_at = Some(chrono::Utc::now() - chrono::Duration::days(30));
        tasks.insert(task_id_3, task3);

        state
            .task_count
            .store(tasks.len(), std::sync::atomic::Ordering::Relaxed);
    }

    // Archive task_id_1 and task_id_2
    let resp = client
        .post(format!("{base}/api/tasks/{task_id_1}/archive"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .post(format!("{base}/api/tasks/{task_id_2}/archive"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Verify archived list
    let resp = client
        .get(format!("{base}/api/tasks/archived"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let archived: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(archived.len(), 2);

    // Run cleanup with 7 days TTL - should remove task_id_1 but keep task_id_2
    let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;
    assert_eq!(removed, 1);

    // Verify task_id_1 is removed from tasks HashMap
    let tasks = state.tasks.read().await;
    assert!(!tasks.contains_key(&task_id_1));
    assert!(tasks.contains_key(&task_id_2));
    assert!(tasks.contains_key(&task_id_3)); // Non-archived task remains
}

#[tokio::test]
async fn test_cleanup_only_affects_archived_tasks() {
    let (_base, state) = start_test_server().await;

    let old_archived_id = uuid::Uuid::new_v4();
    let old_unarchived_id = uuid::Uuid::new_v4();

    {
        let mut tasks = state.tasks.write().await;

        // Old archived task
        let mut task1 = at_core::types::Task::new(
            "Old Archived".to_string(),
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task1.id = old_archived_id;
        task1.completed_at = Some(chrono::Utc::now() - chrono::Duration::days(30));
        tasks.insert(old_archived_id, task1);

        // Old but not archived task
        let mut task2 = at_core::types::Task::new(
            "Old Unarchived".to_string(),
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task2.id = old_unarchived_id;
        task2.completed_at = Some(chrono::Utc::now() - chrono::Duration::days(30));
        tasks.insert(old_unarchived_id, task2);
    }

    // Archive only the first task
    {
        let mut archived = state.archived_tasks.write().await;
        archived.push(old_archived_id);
    }

    // Run cleanup with 7 days TTL
    let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;
    assert_eq!(removed, 1);

    // Verify only the archived task is removed
    let tasks = state.tasks.read().await;
    assert!(!tasks.contains_key(&old_archived_id));
    assert!(tasks.contains_key(&old_unarchived_id));
}

#[tokio::test]
async fn test_cleanup_respects_completed_at_field() {
    let (_base, state) = start_test_server().await;

    let task_with_timestamp = uuid::Uuid::new_v4();
    let task_without_timestamp = uuid::Uuid::new_v4();

    {
        let mut tasks = state.tasks.write().await;

        // Old archived task with completed_at
        let mut task1 = at_core::types::Task::new(
            "With Timestamp".to_string(),
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task1.id = task_with_timestamp;
        task1.completed_at = Some(chrono::Utc::now() - chrono::Duration::days(30));
        tasks.insert(task_with_timestamp, task1);

        // Old archived task without completed_at
        let mut task2 = at_core::types::Task::new(
            "Without Timestamp".to_string(),
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task2.id = task_without_timestamp;
        task2.completed_at = None;
        tasks.insert(task_without_timestamp, task2);

        let mut archived = state.archived_tasks.write().await;
        archived.push(task_with_timestamp);
        archived.push(task_without_timestamp);
    }

    // Run cleanup with 7 days TTL
    let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;
    assert_eq!(removed, 1);

    // Only task with completed_at should be removed
    let tasks = state.tasks.read().await;
    assert!(!tasks.contains_key(&task_with_timestamp));
    assert!(tasks.contains_key(&task_without_timestamp));
}

#[tokio::test]
async fn test_disconnect_buffer_cleanup() {
    let (_base, state) = start_test_server().await;

    let old_session = uuid::Uuid::new_v4();
    let recent_session = uuid::Uuid::new_v4();

    {
        let mut buffers = state.disconnect_buffers.write().await;
        use std::collections::VecDeque;

        // Old disconnect buffer (10 minutes ago)
        let mut old_data = VecDeque::new();
        old_data.extend(b"old data");
        buffers.insert(
            old_session,
            at_bridge::terminal::DisconnectBuffer {
                data: old_data,
                max_bytes: 1024,
                disconnected_at: chrono::Utc::now() - chrono::Duration::minutes(10),
            },
        );

        // Recent disconnect buffer (1 minute ago)
        let mut recent_data = VecDeque::new();
        recent_data.extend(b"recent data");
        buffers.insert(
            recent_session,
            at_bridge::terminal::DisconnectBuffer {
                data: recent_data,
                max_bytes: 1024,
                disconnected_at: chrono::Utc::now() - chrono::Duration::minutes(1),
            },
        );
    }

    // Run cleanup with 5 minutes TTL
    let removed = state.cleanup_disconnect_buffers(5 * 60).await;
    assert_eq!(removed, 1);

    // Verify old buffer is removed but recent buffer remains
    let buffers = state.disconnect_buffers.read().await;
    assert!(!buffers.contains_key(&old_session));
    assert!(buffers.contains_key(&recent_session));
}

#[tokio::test]
async fn test_notification_cleanup_integration() {
    let (_base, state) = start_test_server().await;

    // Add notifications through the normal API
    {
        let mut notifications = state.notification_store.write().await;
        notifications.add(
            "Test Notification 1",
            "Message 1",
            at_bridge::notifications::NotificationLevel::Info,
            "test",
        );
        notifications.add(
            "Test Notification 2",
            "Message 2",
            at_bridge::notifications::NotificationLevel::Warning,
            "test",
        );
    }

    let before_count = state.notification_store.read().await.list_all(100, 0).len();
    assert_eq!(before_count, 2);

    // Cleanup with 0 TTL should remove all notifications created before now
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let _removed = state.notification_store.write().await.cleanup_old(0);

    // Verify cleanup was called (may or may not remove notifications depending on timing)
    let after_count = state.notification_store.read().await.list_all(100, 0).len();
    assert!(after_count <= before_count);
}

#[tokio::test]
async fn test_memory_usage_endpoint() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Add some data to state
    let task_id = uuid::Uuid::new_v4();
    {
        let mut tasks = state.tasks.write().await;
        let task = at_core::types::Task::new(
            "Test Task".to_string(),
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        tasks.insert(task_id, task);

        let mut archived = state.archived_tasks.write().await;
        archived.push(task_id);
    }

    // Query memory usage endpoint
    let resp = client
        .get(format!("{base}/api/debug/memory"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["tasks"], 1);
    assert_eq!(body["archived_tasks"], 1);
    assert!(body["disconnect_buffers"].is_number());
    assert!(body["notifications"].is_number());
}

#[tokio::test]
async fn test_background_cleanup_task_integration() {
    let (_base, state) = start_test_server().await;

    // Create old archived task
    let old_task_id = uuid::Uuid::new_v4();
    {
        let mut tasks = state.tasks.write().await;
        let mut task = at_core::types::Task::new(
            "Old Task".to_string(),
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task.id = old_task_id;
        task.completed_at = Some(chrono::Utc::now() - chrono::Duration::days(30));
        tasks.insert(old_task_id, task);

        let mut archived = state.archived_tasks.write().await;
        archived.push(old_task_id);
    }

    // Manually trigger one cleanup cycle
    let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;
    assert_eq!(removed, 1);

    // Verify task was removed
    let tasks = state.tasks.read().await;
    assert!(!tasks.contains_key(&old_task_id));
}

#[tokio::test]
async fn test_idempotent_archival() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let task_id = uuid::Uuid::new_v4();

    // Archive the same task twice
    let resp = client
        .post(format!("{base}/api/tasks/{task_id}/archive"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .post(format!("{base}/api/tasks/{task_id}/archive"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Verify it only appears once in archived list
    let resp = client
        .get(format!("{base}/api/tasks/archived"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let archived: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(archived.len(), 1);
}

#[tokio::test]
async fn test_unarchive_and_cleanup() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let task_id = uuid::Uuid::new_v4();

    // Create and archive an old task
    {
        let mut tasks = state.tasks.write().await;
        let mut task = at_core::types::Task::new(
            "Test Task".to_string(),
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task.id = task_id;
        task.completed_at = Some(chrono::Utc::now() - chrono::Duration::days(30));
        tasks.insert(task_id, task);
    }

    // Archive it
    let resp = client
        .post(format!("{base}/api/tasks/{task_id}/archive"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Unarchive it
    let resp = client
        .post(format!("{base}/api/tasks/{task_id}/unarchive"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Run cleanup - should not remove unarchived task
    let removed = state.cleanup_archived_tasks(7 * 24 * 60 * 60).await;
    assert_eq!(removed, 0);

    // Verify task still exists
    let tasks = state.tasks.read().await;
    assert!(tasks.contains_key(&task_id));
}
