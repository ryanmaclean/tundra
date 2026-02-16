use at_bridge::notifications::{
    notification_from_event, NotificationLevel, NotificationStore,
};
use at_bridge::protocol::{BridgeMessage, EventPayload};
use chrono::Utc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// NotificationStore CRUD
// ---------------------------------------------------------------------------

#[test]
fn test_add_notification() {
    let mut store = NotificationStore::new(100);
    let id = store.add("Build Done", "Build #42 passed", NotificationLevel::Info, "ci");
    assert_eq!(store.total_count(), 1);
    let all = store.list_all(10, 0);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, id);
    assert_eq!(all[0].title, "Build Done");
    assert_eq!(all[0].message, "Build #42 passed");
    assert_eq!(all[0].source, "ci");
    assert!(!all[0].read);
    assert!(all[0].action_url.is_none());
}

#[test]
fn test_add_notification_with_all_levels() {
    let mut store = NotificationStore::new(100);
    let id_info = store.add("I", "info msg", NotificationLevel::Info, "src");
    let id_success = store.add("S", "success msg", NotificationLevel::Success, "src");
    let id_warning = store.add("W", "warning msg", NotificationLevel::Warning, "src");
    let id_error = store.add("E", "error msg", NotificationLevel::Error, "src");

    assert_eq!(store.total_count(), 4);

    let raw = store.all_raw();
    assert_eq!(raw[0].id, id_info);
    assert_eq!(raw[0].level, NotificationLevel::Info);
    assert_eq!(raw[1].id, id_success);
    assert_eq!(raw[1].level, NotificationLevel::Success);
    assert_eq!(raw[2].id, id_warning);
    assert_eq!(raw[2].level, NotificationLevel::Warning);
    assert_eq!(raw[3].id, id_error);
    assert_eq!(raw[3].level, NotificationLevel::Error);
}

#[test]
fn test_add_notification_with_action_url() {
    let mut store = NotificationStore::new(100);
    let id = store.add_with_url(
        "Review Ready",
        "PR #7 needs review",
        NotificationLevel::Success,
        "github",
        Some("/pulls/7".to_string()),
    );
    let all = store.list_all(10, 0);
    assert_eq!(all[0].id, id);
    assert_eq!(all[0].action_url.as_deref(), Some("/pulls/7"));
}

#[test]
fn test_add_with_url_none_leaves_action_url_none() {
    let mut store = NotificationStore::new(100);
    let id = store.add_with_url(
        "Plain",
        "No URL",
        NotificationLevel::Info,
        "system",
        None,
    );
    let all = store.list_all(10, 0);
    assert_eq!(all[0].id, id);
    assert!(all[0].action_url.is_none());
}

#[test]
fn test_list_unread_returns_only_unread() {
    let mut store = NotificationStore::new(100);
    let id1 = store.add("n1", "m1", NotificationLevel::Info, "s");
    store.add("n2", "m2", NotificationLevel::Warning, "s");
    store.add("n3", "m3", NotificationLevel::Error, "s");

    store.mark_read(id1);

    let unread = store.list_unread();
    assert_eq!(unread.len(), 2);
    for n in &unread {
        assert!(!n.read);
        assert_ne!(n.id, id1);
    }
}

#[test]
fn test_list_all_with_pagination() {
    let mut store = NotificationStore::new(100);
    for i in 0..10 {
        store.add(format!("n{i}"), "msg", NotificationLevel::Info, "s");
    }
    // First page: limit 3, offset 0 -> newest 3
    let page1 = store.list_all(3, 0);
    assert_eq!(page1.len(), 3);
    assert_eq!(page1[0].title, "n9");
    assert_eq!(page1[1].title, "n8");
    assert_eq!(page1[2].title, "n7");

    // Second page
    let page2 = store.list_all(3, 3);
    assert_eq!(page2.len(), 3);
    assert_eq!(page2[0].title, "n6");
}

#[test]
fn test_list_all_offset_and_limit() {
    let mut store = NotificationStore::new(100);
    for i in 0..5 {
        store.add(format!("item{i}"), "body", NotificationLevel::Info, "s");
    }
    // offset=2, limit=2 -> skip 2 newest, take next 2
    let page = store.list_all(2, 2);
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].title, "item2");
    assert_eq!(page[1].title, "item1");

    // offset beyond count -> empty
    let empty = store.list_all(10, 100);
    assert!(empty.is_empty());
}

#[test]
fn test_mark_read_by_id() {
    let mut store = NotificationStore::new(100);
    let id = store.add("n1", "m1", NotificationLevel::Info, "s");
    assert_eq!(store.unread_count(), 1);
    assert!(store.mark_read(id));
    assert_eq!(store.unread_count(), 0);

    // Marking nonexistent ID returns false
    assert!(!store.mark_read(Uuid::new_v4()));
}

#[test]
fn test_mark_all_read() {
    let mut store = NotificationStore::new(100);
    store.add("n1", "m1", NotificationLevel::Info, "s");
    store.add("n2", "m2", NotificationLevel::Warning, "s");
    store.add("n3", "m3", NotificationLevel::Error, "s");
    assert_eq!(store.unread_count(), 3);

    store.mark_all_read();
    assert_eq!(store.unread_count(), 0);

    // Verify every notification is marked read
    for n in store.all_raw() {
        assert!(n.read);
    }
}

#[test]
fn test_delete_notification() {
    let mut store = NotificationStore::new(100);
    let id1 = store.add("n1", "m1", NotificationLevel::Info, "s");
    let _id2 = store.add("n2", "m2", NotificationLevel::Error, "s");
    assert_eq!(store.total_count(), 2);

    assert!(store.delete(id1));
    assert_eq!(store.total_count(), 1);
    let all = store.list_all(10, 0);
    assert_eq!(all[0].title, "n2");
}

#[test]
fn test_delete_nonexistent_returns_false() {
    let mut store = NotificationStore::new(100);
    store.add("n1", "m1", NotificationLevel::Info, "s");
    assert!(!store.delete(Uuid::new_v4()));
    assert_eq!(store.total_count(), 1);
}

#[test]
fn test_unread_count() {
    let mut store = NotificationStore::new(100);
    assert_eq!(store.unread_count(), 0);

    store.add("n1", "m1", NotificationLevel::Info, "s");
    assert_eq!(store.unread_count(), 1);

    let id2 = store.add("n2", "m2", NotificationLevel::Error, "s");
    assert_eq!(store.unread_count(), 2);

    store.mark_read(id2);
    assert_eq!(store.unread_count(), 1);
}

#[test]
fn test_total_count() {
    let mut store = NotificationStore::new(100);
    assert_eq!(store.total_count(), 0);

    store.add("n1", "m1", NotificationLevel::Info, "s");
    assert_eq!(store.total_count(), 1);

    let id = store.add("n2", "m2", NotificationLevel::Error, "s");
    assert_eq!(store.total_count(), 2);

    store.delete(id);
    assert_eq!(store.total_count(), 1);
}

// ---------------------------------------------------------------------------
// Ring Buffer Behavior
// ---------------------------------------------------------------------------

#[test]
fn test_ring_buffer_capacity_default_1000() {
    let store = NotificationStore::default();
    // Default capacity is 1000; adding up to 1000 should all be retained.
    // We verify by checking the struct was created and has zero count.
    assert_eq!(store.total_count(), 0);

    // Add exactly 1000 items
    let mut store = NotificationStore::default();
    for i in 0..1000 {
        store.add(format!("n{i}"), "msg", NotificationLevel::Info, "s");
    }
    assert_eq!(store.total_count(), 1000);

    // Adding one more should evict the oldest
    store.add("overflow", "msg", NotificationLevel::Info, "s");
    assert_eq!(store.total_count(), 1000);
}

#[test]
fn test_ring_buffer_overflow_drops_oldest() {
    let mut store = NotificationStore::new(3);
    store.add("first", "m", NotificationLevel::Info, "s");
    store.add("second", "m", NotificationLevel::Info, "s");
    store.add("third", "m", NotificationLevel::Info, "s");
    assert_eq!(store.total_count(), 3);

    // Adding a 4th should drop "first"
    store.add("fourth", "m", NotificationLevel::Info, "s");
    assert_eq!(store.total_count(), 3);

    let raw = store.all_raw();
    assert_eq!(raw[0].title, "second");
    assert_eq!(raw[1].title, "third");
    assert_eq!(raw[2].title, "fourth");
}

#[test]
fn test_ring_buffer_preserves_newest() {
    let mut store = NotificationStore::new(2);
    for i in 0..10 {
        store.add(format!("n{i}"), "m", NotificationLevel::Info, "s");
    }
    assert_eq!(store.total_count(), 2);

    // The two newest should be n8 and n9
    let raw = store.all_raw();
    assert_eq!(raw[0].title, "n8");
    assert_eq!(raw[1].title, "n9");
}

// ---------------------------------------------------------------------------
// Notification Ordering
// ---------------------------------------------------------------------------

#[test]
fn test_notifications_sorted_newest_first() {
    let mut store = NotificationStore::new(100);
    store.add("old", "m", NotificationLevel::Info, "s");
    store.add("middle", "m", NotificationLevel::Info, "s");
    store.add("new", "m", NotificationLevel::Info, "s");

    let all = store.list_all(10, 0);
    assert_eq!(all[0].title, "new");
    assert_eq!(all[1].title, "middle");
    assert_eq!(all[2].title, "old");
}

#[test]
fn test_unread_sorted_newest_first() {
    let mut store = NotificationStore::new(100);
    store.add("old", "m", NotificationLevel::Info, "s");
    store.add("middle", "m", NotificationLevel::Warning, "s");
    store.add("new", "m", NotificationLevel::Error, "s");

    let unread = store.list_unread();
    assert_eq!(unread[0].title, "new");
    assert_eq!(unread[1].title, "middle");
    assert_eq!(unread[2].title, "old");
}

// ---------------------------------------------------------------------------
// Event-to-Notification Conversion (notification_from_event)
// ---------------------------------------------------------------------------

fn make_event(event_type: &str, agent_id: Option<Uuid>, bead_id: Option<Uuid>, message: &str) -> BridgeMessage {
    BridgeMessage::Event(EventPayload {
        event_type: event_type.to_string(),
        agent_id,
        bead_id,
        message: message.to_string(),
        timestamp: Utc::now(),
    })
}

#[test]
fn test_bead_state_change_creates_info_notification() {
    let bead = Uuid::new_v4();
    let msg = make_event("bead_state_change", None, Some(bead), "Moved to review");
    let result = notification_from_event(&msg);
    assert!(result.is_some());
    let (title, body, level, source, url) = result.unwrap();
    assert_eq!(title, "Bead State Changed");
    assert_eq!(body, "Moved to review");
    assert_eq!(level, NotificationLevel::Info);
    assert_eq!(source, "system");
    assert!(url.is_some());
    assert!(url.unwrap().contains(&bead.to_string()));
}

#[test]
fn test_agent_spawned_creates_info_notification() {
    let agent = Uuid::new_v4();
    let msg = make_event("agent_spawned", Some(agent), None, "Agent started");
    let result = notification_from_event(&msg);
    assert!(result.is_some());
    let (title, body, level, source, url) = result.unwrap();
    assert_eq!(title, "Agent Spawned");
    assert_eq!(body, "Agent started");
    assert_eq!(level, NotificationLevel::Info);
    assert!(source.contains(&agent.to_string()));
    assert!(url.is_none());
}

#[test]
fn test_agent_spawned_without_agent_id_uses_system_source() {
    let msg = make_event("agent_spawned", None, None, "Agent started");
    let result = notification_from_event(&msg).unwrap();
    assert_eq!(result.3, "system");
}

#[test]
fn test_agent_stopped_creates_warning_notification() {
    let agent = Uuid::new_v4();
    let msg = make_event("agent_stopped", Some(agent), None, "Agent stopped gracefully");
    let result = notification_from_event(&msg);
    assert!(result.is_some());
    let (title, body, level, source, _url) = result.unwrap();
    assert_eq!(title, "Agent Stopped");
    assert_eq!(body, "Agent stopped gracefully");
    assert_eq!(level, NotificationLevel::Warning);
    assert!(source.starts_with("agent:"));
}

#[test]
fn test_agent_crashed_creates_error_notification() {
    let agent = Uuid::new_v4();
    let msg = make_event("agent_crashed", Some(agent), None, "OOM killed");
    let result = notification_from_event(&msg);
    assert!(result.is_some());
    let (title, body, level, source, _url) = result.unwrap();
    assert_eq!(title, "Agent Crashed");
    assert_eq!(body, "OOM killed");
    assert_eq!(level, NotificationLevel::Error);
    assert!(source.starts_with("agent:"));
}

#[test]
fn test_task_completed_creates_success_notification() {
    let bead = Uuid::new_v4();
    let msg = make_event("task_completed", None, Some(bead), "Task done");
    let result = notification_from_event(&msg);
    assert!(result.is_some());
    let (title, body, level, source, url) = result.unwrap();
    assert_eq!(title, "Task Completed");
    assert_eq!(body, "Task done");
    assert_eq!(level, NotificationLevel::Success);
    assert_eq!(source, "system");
    assert!(url.is_some());
    assert!(url.unwrap().contains(&bead.to_string()));
}

#[test]
fn test_unknown_event_creates_no_notification() {
    let msg = make_event("something_else", None, None, "ignored");
    assert!(notification_from_event(&msg).is_none());
}

#[test]
fn test_non_event_message_creates_no_notification() {
    assert!(notification_from_event(&BridgeMessage::GetStatus).is_none());
    assert!(notification_from_event(&BridgeMessage::GetKpi).is_none());
    assert!(notification_from_event(&BridgeMessage::ListAgents).is_none());
}

#[test]
fn test_bead_list_creates_no_notification() {
    let msg = BridgeMessage::BeadList(vec![]);
    assert!(notification_from_event(&msg).is_none());
}

// ---------------------------------------------------------------------------
// Notification Preferences (matching 4 toggles from screenshot)
// ---------------------------------------------------------------------------

/// The notification preferences are modeled as simple booleans that a UI
/// layer would check before displaying a notification. We test the logic
/// of filtering notifications based on these preference flags.
#[derive(Debug, Clone)]
struct NotificationPreferences {
    on_task_complete: bool,
    on_task_failed: bool,
    on_review_needed: bool,
    sound_enabled: bool,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            on_task_complete: true,
            on_task_failed: true,
            on_review_needed: true,
            sound_enabled: true,
        }
    }
}

impl NotificationPreferences {
    fn should_notify(&self, event_type: &str) -> bool {
        match event_type {
            "task_completed" => self.on_task_complete,
            "agent_crashed" => self.on_task_failed,
            "bead_state_change" => self.on_review_needed,
            _ => true,
        }
    }
}

#[test]
fn test_notification_on_task_complete_toggle() {
    let mut prefs = NotificationPreferences::default();
    assert!(prefs.should_notify("task_completed"));

    prefs.on_task_complete = false;
    assert!(!prefs.should_notify("task_completed"));

    // Other types remain unaffected
    assert!(prefs.should_notify("agent_crashed"));
    assert!(prefs.should_notify("bead_state_change"));
}

#[test]
fn test_notification_on_task_failed_toggle() {
    let mut prefs = NotificationPreferences::default();
    assert!(prefs.should_notify("agent_crashed"));

    prefs.on_task_failed = false;
    assert!(!prefs.should_notify("agent_crashed"));

    // Other types remain unaffected
    assert!(prefs.should_notify("task_completed"));
}

#[test]
fn test_notification_on_review_needed_toggle() {
    let mut prefs = NotificationPreferences::default();
    assert!(prefs.should_notify("bead_state_change"));

    prefs.on_review_needed = false;
    assert!(!prefs.should_notify("bead_state_change"));

    // Other types remain unaffected
    assert!(prefs.should_notify("task_completed"));
}

#[test]
fn test_notification_sound_toggle() {
    let mut prefs = NotificationPreferences::default();
    assert!(prefs.sound_enabled);

    prefs.sound_enabled = false;
    assert!(!prefs.sound_enabled);

    // Sound toggle doesn't affect whether notifications are shown
    assert!(prefs.should_notify("task_completed"));
    assert!(prefs.should_notify("agent_crashed"));
}

// ---------------------------------------------------------------------------
// Integration: event -> store with preferences
// ---------------------------------------------------------------------------

#[test]
fn test_event_to_store_integration() {
    let mut store = NotificationStore::new(100);
    let prefs = NotificationPreferences::default();

    let events = vec![
        make_event("task_completed", None, Some(Uuid::new_v4()), "Done"),
        make_event("agent_crashed", Some(Uuid::new_v4()), None, "OOM"),
        make_event("bead_state_change", None, Some(Uuid::new_v4()), "Review"),
        make_event("unknown_type", None, None, "ignored"),
    ];

    for evt in &events {
        if let Some((title, msg, level, source, url)) = notification_from_event(evt) {
            if let BridgeMessage::Event(ep) = evt {
                if prefs.should_notify(&ep.event_type) {
                    store.add_with_url(title, msg, level, source, url);
                }
            }
        }
    }

    // 3 known event types generate notifications; unknown does not
    assert_eq!(store.total_count(), 3);
}

#[test]
fn test_event_to_store_with_disabled_prefs() {
    let mut store = NotificationStore::new(100);
    let prefs = NotificationPreferences {
        on_task_complete: false,
        on_task_failed: false,
        on_review_needed: false,
        sound_enabled: false,
    };

    let events = vec![
        make_event("task_completed", None, Some(Uuid::new_v4()), "Done"),
        make_event("agent_crashed", Some(Uuid::new_v4()), None, "OOM"),
        make_event("bead_state_change", None, Some(Uuid::new_v4()), "Review"),
        make_event("agent_spawned", Some(Uuid::new_v4()), None, "Spawned"),
    ];

    for evt in &events {
        if let Some((title, msg, level, source, url)) = notification_from_event(evt) {
            if let BridgeMessage::Event(ep) = evt {
                if prefs.should_notify(&ep.event_type) {
                    store.add_with_url(title, msg, level, source, url);
                }
            }
        }
    }

    // task_completed, agent_crashed, bead_state_change are filtered out.
    // agent_spawned falls through to default (true).
    assert_eq!(store.total_count(), 1);
    assert_eq!(store.list_all(10, 0)[0].title, "Agent Spawned");
}
