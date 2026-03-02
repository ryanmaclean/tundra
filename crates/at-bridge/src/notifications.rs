use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

use crate::protocol::{BridgeMessage, EventPayload};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// Backward-compatible alias so old code referencing `NotificationType` still compiles.
pub type NotificationType = NotificationLevel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub title: String,
    pub message: String,
    pub level: NotificationLevel,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub read: bool,
    pub action_url: Option<String>,
}

/// Ring-buffer backed notification store using `VecDeque` for O(1) eviction.
#[derive(Debug, Clone)]
pub struct NotificationStore {
    notifications: VecDeque<Notification>,
    max_stored: usize,
}

impl NotificationStore {
    pub fn new(max_stored: usize) -> Self {
        Self {
            notifications: VecDeque::new(),
            max_stored,
        }
    }

    /// Create and store a new notification. Returns its id.
    pub fn add(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        level: NotificationLevel,
        source: impl Into<String>,
    ) -> Uuid {
        let notification = Notification {
            id: Uuid::new_v4(),
            title: title.into(),
            message: message.into(),
            level,
            source: source.into(),
            created_at: Utc::now(),
            read: false,
            action_url: None,
        };
        let id = notification.id;
        self.notifications.push_back(notification);
        // Ring buffer: evict oldest when over capacity (O(1) with VecDeque).
        while self.notifications.len() > self.max_stored {
            self.notifications.pop_front();
        }
        id
    }

    /// Create and store a notification with an optional action URL.
    pub fn add_with_url(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        level: NotificationLevel,
        source: impl Into<String>,
        action_url: Option<String>,
    ) -> Uuid {
        let id = self.add(title, message, level, source);
        if let Some(url) = action_url {
            if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
                n.action_url = Some(url);
            }
        }
        id
    }

    /// Return all unread notifications (newest first).
    pub fn list_unread(&self) -> Vec<&Notification> {
        self.notifications
            .iter()
            .rev()
            .filter(|n| !n.read)
            .collect()
    }

    /// Paginated listing of all notifications (newest first).
    pub fn list_all(&self, limit: usize, offset: usize) -> Vec<&Notification> {
        self.notifications
            .iter()
            .rev()
            .skip(offset)
            .take(limit)
            .collect()
    }

    /// Return a reference to all stored notifications (oldest first, raw order).
    pub fn all_raw(&self) -> &VecDeque<Notification> {
        &self.notifications
    }

    /// Mark a single notification as read. Returns false if not found.
    pub fn mark_read(&mut self, id: Uuid) -> bool {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.read = true;
            true
        } else {
            false
        }
    }

    /// Mark every notification as read.
    pub fn mark_all_read(&mut self) {
        for n in &mut self.notifications {
            n.read = true;
        }
    }

    /// Delete a notification by id. Returns true if found and removed.
    pub fn delete(&mut self, id: Uuid) -> bool {
        let before = self.notifications.len();
        self.notifications.retain(|n| n.id != id);
        self.notifications.len() < before
    }

    /// Count of unread notifications.
    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| !n.read).count()
    }

    /// Total notification count.
    pub fn total_count(&self) -> usize {
        self.notifications.len()
    }

    /// Remove notifications older than the specified TTL in seconds.
    /// Returns the number of notifications removed.
    pub fn cleanup_old(&mut self, ttl_secs: u64) -> usize {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(ttl_secs as i64);

        let before_count = self.notifications.len();
        self.notifications.retain(|n| n.created_at >= cutoff);
        let after_count = self.notifications.len();

        before_count - after_count
    }
}

impl Default for NotificationStore {
    fn default() -> Self {
        Self::new(1000)
    }
}

// ---------------------------------------------------------------------------
// Event-to-notification conversion
// ---------------------------------------------------------------------------

/// Convert a `BridgeMessage` event into a notification, if relevant.
/// Returns `None` for events that don't warrant a notification.
pub fn notification_from_event(
    msg: &BridgeMessage,
) -> Option<(String, String, NotificationLevel, String, Option<String>)> {
    match msg {
        BridgeMessage::Event(EventPayload {
            event_type,
            agent_id,
            bead_id,
            message,
            ..
        }) => {
            let etype = event_type.as_str();
            match etype {
                "bead_created" => {
                    let url = bead_id.map(|id| format!("/beads/{}", id));
                    Some((
                        "Bead Created".to_string(),
                        message.clone(),
                        NotificationLevel::Success,
                        "system".to_string(),
                        url,
                    ))
                }
                "bead_updated" => {
                    let url = bead_id.map(|id| format!("/beads/{}", id));
                    Some((
                        "Bead Updated".to_string(),
                        message.clone(),
                        NotificationLevel::Info,
                        "system".to_string(),
                        url,
                    ))
                }
                "bead_state_change" => {
                    let url = bead_id.map(|id| format!("/beads/{}", id));
                    Some((
                        "Bead State Changed".to_string(),
                        message.clone(),
                        NotificationLevel::Info,
                        "system".to_string(),
                        url,
                    ))
                }
                "agent_spawned" => {
                    let src = agent_id
                        .map(|id| format!("agent:{}", id))
                        .unwrap_or_else(|| "system".to_string());
                    Some((
                        "Agent Spawned".to_string(),
                        message.clone(),
                        NotificationLevel::Info,
                        src,
                        None,
                    ))
                }
                "agent_stopped" => {
                    let src = agent_id
                        .map(|id| format!("agent:{}", id))
                        .unwrap_or_else(|| "system".to_string());
                    Some((
                        "Agent Stopped".to_string(),
                        message.clone(),
                        NotificationLevel::Warning,
                        src,
                        None,
                    ))
                }
                "agent_crashed" => {
                    let src = agent_id
                        .map(|id| format!("agent:{}", id))
                        .unwrap_or_else(|| "system".to_string());
                    Some((
                        "Agent Crashed".to_string(),
                        message.clone(),
                        NotificationLevel::Error,
                        src,
                        None,
                    ))
                }
                "task_completed" => {
                    let url = bead_id.map(|id| format!("/beads/{}", id));
                    Some((
                        "Task Completed".to_string(),
                        message.clone(),
                        NotificationLevel::Success,
                        "system".to_string(),
                        url,
                    ))
                }
                _ => None,
            }
        }
        // BeadList updates also generate a lightweight notification.
        BridgeMessage::BeadList(_beads) => {
            // We intentionally don't auto-notify on every list refresh.
            None
        }
        // Handle new enum variants for bead creation and updates.
        BridgeMessage::BeadCreated(bead) => Some((
            "Bead Created".to_string(),
            format!("Created bead: {}", bead.title),
            NotificationLevel::Success,
            "system".to_string(),
            Some(format!("/beads/{}", bead.id)),
        )),
        BridgeMessage::BeadUpdated(bead) => Some((
            "Bead Updated".to_string(),
            format!("Updated bead: {}", bead.title),
            NotificationLevel::Info,
            "system".to_string(),
            Some(format!("/beads/{}", bead.id)),
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_list() {
        let mut store = NotificationStore::new(10);
        store.add("Hello", "World", NotificationLevel::Info, "system");
        assert_eq!(store.total_count(), 1);
        assert_eq!(store.unread_count(), 1);
        let all = store.list_all(100, 0);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "Hello");
    }

    #[test]
    fn test_ring_buffer_overflow() {
        let mut store = NotificationStore::new(3);
        store.add("n1", "m1", NotificationLevel::Info, "system");
        store.add("n2", "m2", NotificationLevel::Success, "system");
        store.add("n3", "m3", NotificationLevel::Warning, "system");
        store.add("n4", "m4", NotificationLevel::Error, "system");
        assert_eq!(store.total_count(), 3);
        // Oldest (n1) should have been evicted
        let all = store.list_all(10, 0);
        // Returned newest-first: n4, n3, n2
        assert_eq!(all[0].title, "n4");
        assert_eq!(all[1].title, "n3");
        assert_eq!(all[2].title, "n2");
    }

    #[test]
    fn test_list_unread() {
        let mut store = NotificationStore::new(10);
        let id1 = store.add("n1", "m1", NotificationLevel::Info, "system");
        store.add("n2", "m2", NotificationLevel::Error, "system");
        store.mark_read(id1);
        let unread = store.list_unread();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].title, "n2");
    }

    #[test]
    fn test_pagination() {
        let mut store = NotificationStore::new(100);
        for i in 0..20 {
            store.add(format!("n{i}"), "msg", NotificationLevel::Info, "system");
        }
        // limit=5, offset=0 -> newest 5
        let page1 = store.list_all(5, 0);
        assert_eq!(page1.len(), 5);
        assert_eq!(page1[0].title, "n19");
        assert_eq!(page1[4].title, "n15");

        // limit=5, offset=5 -> next 5
        let page2 = store.list_all(5, 5);
        assert_eq!(page2.len(), 5);
        assert_eq!(page2[0].title, "n14");
    }

    #[test]
    fn test_mark_read() {
        let mut store = NotificationStore::new(10);
        let id = store.add("n1", "m1", NotificationLevel::Info, "system");
        assert_eq!(store.unread_count(), 1);
        assert!(store.mark_read(id));
        assert_eq!(store.unread_count(), 0);
    }

    #[test]
    fn test_mark_read_not_found() {
        let mut store = NotificationStore::new(10);
        assert!(!store.mark_read(Uuid::new_v4()));
    }

    #[test]
    fn test_mark_all_read() {
        let mut store = NotificationStore::new(10);
        store.add("n1", "m1", NotificationLevel::Info, "system");
        store.add("n2", "m2", NotificationLevel::Warning, "system");
        assert_eq!(store.unread_count(), 2);
        store.mark_all_read();
        assert_eq!(store.unread_count(), 0);
    }

    #[test]
    fn test_delete() {
        let mut store = NotificationStore::new(10);
        let id = store.add("n1", "m1", NotificationLevel::Info, "system");
        store.add("n2", "m2", NotificationLevel::Error, "system");
        assert!(store.delete(id));
        assert_eq!(store.total_count(), 1);
        assert_eq!(store.list_all(10, 0)[0].title, "n2");
    }

    #[test]
    fn test_delete_not_found() {
        let mut store = NotificationStore::new(10);
        assert!(!store.delete(Uuid::new_v4()));
    }

    #[test]
    fn test_unread_count() {
        let mut store = NotificationStore::new(10);
        store.add("n1", "m1", NotificationLevel::Info, "system");
        store.add("n2", "m2", NotificationLevel::Success, "system");
        store.add("n3", "m3", NotificationLevel::Warning, "system");
        assert_eq!(store.unread_count(), 3);
        store.mark_all_read();
        assert_eq!(store.unread_count(), 0);
    }

    #[test]
    fn test_add_with_url() {
        let mut store = NotificationStore::new(10);
        let id = store.add_with_url(
            "Bead Ready",
            "Check it out",
            NotificationLevel::Success,
            "system",
            Some("/beads/abc123".to_string()),
        );
        let all = store.list_all(10, 0);
        assert_eq!(all[0].id, id);
        assert_eq!(all[0].action_url.as_deref(), Some("/beads/abc123"));
    }

    #[test]
    fn test_notification_from_event_bead_state() {
        let msg = BridgeMessage::Event(EventPayload {
            event_type: "bead_state_change".to_string(),
            agent_id: None,
            bead_id: Some(Uuid::new_v4()),
            message: "Bead moved to review".to_string(),
            timestamp: Utc::now(),
        });
        let result = notification_from_event(&msg);
        assert!(result.is_some());
        let (title, _msg, level, _src, url) = result.unwrap();
        assert_eq!(title, "Bead State Changed");
        assert_eq!(level, NotificationLevel::Info);
        assert!(url.is_some());
    }

    #[test]
    fn test_notification_from_event_agent_crashed() {
        let msg = BridgeMessage::Event(EventPayload {
            event_type: "agent_crashed".to_string(),
            agent_id: Some(Uuid::new_v4()),
            bead_id: None,
            message: "Agent OOM".to_string(),
            timestamp: Utc::now(),
        });
        let result = notification_from_event(&msg);
        assert!(result.is_some());
        let (title, _msg, level, _src, _url) = result.unwrap();
        assert_eq!(title, "Agent Crashed");
        assert_eq!(level, NotificationLevel::Error);
    }

    #[test]
    fn test_notification_from_event_task_completed() {
        let msg = BridgeMessage::Event(EventPayload {
            event_type: "task_completed".to_string(),
            agent_id: None,
            bead_id: Some(Uuid::new_v4()),
            message: "Task finished".to_string(),
            timestamp: Utc::now(),
        });
        let result = notification_from_event(&msg);
        assert!(result.is_some());
        let (title, _msg, level, _src, _url) = result.unwrap();
        assert_eq!(title, "Task Completed");
        assert_eq!(level, NotificationLevel::Success);
    }

    #[test]
    fn test_notification_from_event_unknown_type() {
        let msg = BridgeMessage::Event(EventPayload {
            event_type: "some_random_thing".to_string(),
            agent_id: None,
            bead_id: None,
            message: "whatever".to_string(),
            timestamp: Utc::now(),
        });
        assert!(notification_from_event(&msg).is_none());
    }

    #[test]
    fn test_notification_from_non_event() {
        let msg = BridgeMessage::GetStatus;
        assert!(notification_from_event(&msg).is_none());
    }

    // ---------------------------------------------------------------------------
    // Notification cleanup tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_notification_cleanup_removes_old_notifications() {
        let mut store = NotificationStore::new(100);

        // Create a notification that's 10 days old
        let old_id = store.add(
            "Old notification",
            "old msg",
            NotificationLevel::Info,
            "system",
        );
        // Manually set created_at to 10 days ago
        if let Some(n) = store.notifications.iter_mut().find(|n| n.id == old_id) {
            n.created_at = Utc::now() - chrono::Duration::days(10);
        }

        // Create a recent notification (1 day old)
        let recent_id = store.add(
            "Recent notification",
            "recent msg",
            NotificationLevel::Info,
            "system",
        );
        if let Some(n) = store.notifications.iter_mut().find(|n| n.id == recent_id) {
            n.created_at = Utc::now() - chrono::Duration::days(1);
        }

        assert_eq!(store.total_count(), 2);

        // Cleanup with TTL of 7 days (604800 seconds)
        let removed = store.cleanup_old(7 * 24 * 60 * 60);

        assert_eq!(removed, 1);
        assert_eq!(store.total_count(), 1);
        assert_eq!(store.list_all(10, 0)[0].id, recent_id);
    }

    #[test]
    fn test_notification_cleanup_keeps_recent_notifications() {
        let mut store = NotificationStore::new(100);

        // Create a notification that's 5 days old
        let id = store.add(
            "Recent notification",
            "msg",
            NotificationLevel::Info,
            "system",
        );
        if let Some(n) = store.notifications.iter_mut().find(|n| n.id == id) {
            n.created_at = Utc::now() - chrono::Duration::days(5);
        }

        assert_eq!(store.total_count(), 1);

        // Cleanup with TTL of 7 days (notification is only 5 days old)
        let removed = store.cleanup_old(7 * 24 * 60 * 60);

        assert_eq!(removed, 0);
        assert_eq!(store.total_count(), 1);
        assert_eq!(store.list_all(10, 0)[0].id, id);
    }

    #[test]
    fn test_notification_cleanup_handles_multiple_notifications() {
        let mut store = NotificationStore::new(100);

        // Create multiple notifications with different ages
        let old_id1 = store.add("Old 1", "msg1", NotificationLevel::Info, "system");
        if let Some(n) = store.notifications.iter_mut().find(|n| n.id == old_id1) {
            n.created_at = Utc::now() - chrono::Duration::days(10);
        }

        let old_id2 = store.add("Old 2", "msg2", NotificationLevel::Info, "system");
        if let Some(n) = store.notifications.iter_mut().find(|n| n.id == old_id2) {
            n.created_at = Utc::now() - chrono::Duration::days(15);
        }

        let recent_id1 = store.add("Recent 1", "msg3", NotificationLevel::Info, "system");
        if let Some(n) = store.notifications.iter_mut().find(|n| n.id == recent_id1) {
            n.created_at = Utc::now() - chrono::Duration::days(5);
        }

        let recent_id2 = store.add("Recent 2", "msg4", NotificationLevel::Info, "system");
        if let Some(n) = store.notifications.iter_mut().find(|n| n.id == recent_id2) {
            n.created_at = Utc::now() - chrono::Duration::days(3);
        }

        assert_eq!(store.total_count(), 4);

        // Cleanup with TTL of 7 days (should remove 2 old notifications)
        let removed = store.cleanup_old(7 * 24 * 60 * 60);

        assert_eq!(removed, 2);
        assert_eq!(store.total_count(), 2);

        // Verify the correct notifications remain
        let remaining = store.list_all(10, 0);
        let remaining_ids: Vec<Uuid> = remaining.iter().map(|n| n.id).collect();
        assert!(remaining_ids.contains(&recent_id1));
        assert!(remaining_ids.contains(&recent_id2));
        assert!(!remaining_ids.contains(&old_id1));
        assert!(!remaining_ids.contains(&old_id2));
    }

    #[test]
    fn test_notification_cleanup_empty_state() {
        let mut store = NotificationStore::new(100);

        // Cleanup with no notifications
        let removed = store.cleanup_old(7 * 24 * 60 * 60);

        assert_eq!(removed, 0);
        assert_eq!(store.total_count(), 0);
    }

    #[test]
    fn test_notification_cleanup_with_zero_ttl() {
        let mut store = NotificationStore::new(100);

        // Create a notification just now
        store.add("New notification", "msg", NotificationLevel::Info, "system");
        assert_eq!(store.total_count(), 1);

        // Cleanup with TTL of 0 seconds (should remove all notifications)
        let removed = store.cleanup_old(0);

        assert_eq!(removed, 1);
        assert_eq!(store.total_count(), 0);
    }

    #[test]
    fn test_notification_cleanup_exact_ttl_boundary() {
        let mut store = NotificationStore::new(100);

        // Create a notification exactly 7 days old (plus 1 second to ensure it's before cutoff)
        let id = store.add(
            "Boundary notification",
            "msg",
            NotificationLevel::Info,
            "system",
        );
        if let Some(n) = store.notifications.iter_mut().find(|n| n.id == id) {
            n.created_at = Utc::now() - chrono::Duration::seconds(7 * 24 * 60 * 60 + 1);
        }

        assert_eq!(store.total_count(), 1);

        // Cleanup with TTL of 7 days (notification is just past the boundary)
        // The notification should be removed because created_at < cutoff
        let removed = store.cleanup_old(7 * 24 * 60 * 60);

        assert_eq!(removed, 1);
        assert_eq!(store.total_count(), 0);
    }

    #[test]
    fn test_notification_cleanup_preserves_data() {
        let mut store = NotificationStore::new(100);

        // Create a notification with specific data
        let id = store.add_with_url(
            "Important notification",
            "This has an action URL",
            NotificationLevel::Warning,
            "test-source",
            Some("/action/url".to_string()),
        );
        if let Some(n) = store.notifications.iter_mut().find(|n| n.id == id) {
            n.created_at = Utc::now() - chrono::Duration::days(3);
            n.read = true; // Mark as read
        }

        assert_eq!(store.total_count(), 1);

        // Cleanup with TTL of 7 days (should not remove)
        let removed = store.cleanup_old(7 * 24 * 60 * 60);

        assert_eq!(removed, 0);
        assert_eq!(store.total_count(), 1);

        // Verify all data is preserved
        let notifications = store.list_all(10, 0);
        assert_eq!(notifications.len(), 1);
        let n = notifications[0];
        assert_eq!(n.id, id);
        assert_eq!(n.title, "Important notification");
        assert_eq!(n.message, "This has an action URL");
        assert_eq!(n.level, NotificationLevel::Warning);
        assert_eq!(n.source, "test-source");
        assert_eq!(n.action_url.as_deref(), Some("/action/url"));
        assert!(n.read);
    }
}
