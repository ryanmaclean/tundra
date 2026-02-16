use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
    TaskUpdate,
    AgentEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub read: bool,
    pub action_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NotificationManager {
    notifications: Vec<Notification>,
    max_notifications: usize,
}

impl NotificationManager {
    pub fn new(max: usize) -> Self {
        Self {
            notifications: Vec::new(),
            max_notifications: max,
        }
    }

    pub fn push(&mut self, n: Notification) {
        self.notifications.push(n);
        // If we exceed the max, remove the oldest notifications.
        while self.notifications.len() > self.max_notifications {
            self.notifications.remove(0);
        }
    }

    pub fn list_unread(&self) -> Vec<&Notification> {
        self.notifications.iter().filter(|n| !n.read).collect()
    }

    pub fn list_all(&self) -> &[Notification] {
        &self.notifications
    }

    pub fn mark_read(&mut self, id: &Uuid) -> bool {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == *id) {
            n.read = true;
            true
        } else {
            false
        }
    }

    pub fn mark_all_read(&mut self) {
        for n in &mut self.notifications {
            n.read = true;
        }
    }

    pub fn clear_read(&mut self) {
        self.notifications.retain(|n| !n.read);
    }

    pub fn count_unread(&self) -> usize {
        self.notifications.iter().filter(|n| !n.read).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_notification(title: &str, ntype: NotificationType) -> Notification {
        Notification {
            id: Uuid::new_v4(),
            notification_type: ntype,
            title: title.to_string(),
            message: format!("{title} message"),
            timestamp: Utc::now(),
            read: false,
            action_url: None,
        }
    }

    #[test]
    fn test_push() {
        let mut mgr = NotificationManager::new(5);
        mgr.push(make_notification("n1", NotificationType::Info));
        assert_eq!(mgr.list_all().len(), 1);
    }

    #[test]
    fn test_push_overflow() {
        let mut mgr = NotificationManager::new(2);
        mgr.push(make_notification("n1", NotificationType::Info));
        mgr.push(make_notification("n2", NotificationType::Success));
        mgr.push(make_notification("n3", NotificationType::Warning));
        assert_eq!(mgr.list_all().len(), 2);
        assert_eq!(mgr.list_all()[0].title, "n2");
        assert_eq!(mgr.list_all()[1].title, "n3");
    }

    #[test]
    fn test_list_unread() {
        let mut mgr = NotificationManager::new(10);
        let mut n1 = make_notification("n1", NotificationType::Info);
        n1.read = true;
        mgr.push(n1);
        mgr.push(make_notification("n2", NotificationType::Error));
        let unread = mgr.list_unread();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].title, "n2");
    }

    #[test]
    fn test_mark_read() {
        let mut mgr = NotificationManager::new(10);
        let n = make_notification("n1", NotificationType::Info);
        let id = n.id;
        mgr.push(n);
        assert_eq!(mgr.count_unread(), 1);
        assert!(mgr.mark_read(&id));
        assert_eq!(mgr.count_unread(), 0);
    }

    #[test]
    fn test_mark_read_not_found() {
        let mut mgr = NotificationManager::new(10);
        assert!(!mgr.mark_read(&Uuid::new_v4()));
    }

    #[test]
    fn test_mark_all_read() {
        let mut mgr = NotificationManager::new(10);
        mgr.push(make_notification("n1", NotificationType::Info));
        mgr.push(make_notification("n2", NotificationType::Warning));
        assert_eq!(mgr.count_unread(), 2);
        mgr.mark_all_read();
        assert_eq!(mgr.count_unread(), 0);
    }

    #[test]
    fn test_clear_read() {
        let mut mgr = NotificationManager::new(10);
        let mut n1 = make_notification("n1", NotificationType::Info);
        n1.read = true;
        mgr.push(n1);
        mgr.push(make_notification("n2", NotificationType::Error));
        mgr.clear_read();
        assert_eq!(mgr.list_all().len(), 1);
        assert_eq!(mgr.list_all()[0].title, "n2");
    }

    #[test]
    fn test_count_unread() {
        let mut mgr = NotificationManager::new(10);
        mgr.push(make_notification("n1", NotificationType::Info));
        mgr.push(make_notification("n2", NotificationType::TaskUpdate));
        mgr.push(make_notification("n3", NotificationType::AgentEvent));
        assert_eq!(mgr.count_unread(), 3);
        mgr.mark_all_read();
        assert_eq!(mgr.count_unread(), 0);
    }
}
