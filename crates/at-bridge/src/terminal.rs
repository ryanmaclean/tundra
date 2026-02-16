use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub title: String,
    pub status: TerminalStatus,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalStatus {
    Active,
    Idle,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalEvent {
    Output { terminal_id: Uuid, data: String },
    Resize { terminal_id: Uuid, cols: u16, rows: u16 },
    Close { terminal_id: Uuid },
    Title { terminal_id: Uuid, title: String },
}

pub struct TerminalRegistry {
    terminals: HashMap<Uuid, TerminalInfo>,
}

impl TerminalRegistry {
    pub fn new() -> Self {
        Self {
            terminals: HashMap::new(),
        }
    }

    pub fn register(&mut self, info: TerminalInfo) -> Uuid {
        let id = info.id;
        self.terminals.insert(id, info);
        id
    }

    pub fn unregister(&mut self, id: &Uuid) -> Option<TerminalInfo> {
        self.terminals.remove(id)
    }

    pub fn get(&self, id: &Uuid) -> Option<&TerminalInfo> {
        self.terminals.get(id)
    }

    pub fn list(&self) -> Vec<&TerminalInfo> {
        self.terminals.values().collect()
    }

    pub fn list_active(&self) -> Vec<&TerminalInfo> {
        self.terminals
            .values()
            .filter(|t| t.status == TerminalStatus::Active)
            .collect()
    }

    pub fn update_status(&mut self, id: &Uuid, status: TerminalStatus) -> bool {
        if let Some(t) = self.terminals.get_mut(id) {
            t.status = status;
            true
        } else {
            false
        }
    }
}

impl Default for TerminalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_terminal(status: TerminalStatus) -> TerminalInfo {
        TerminalInfo {
            id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            title: "test terminal".to_string(),
            status,
            cols: 80,
            rows: 24,
        }
    }

    #[test]
    fn test_register() {
        let mut reg = TerminalRegistry::new();
        let info = make_terminal(TerminalStatus::Active);
        let id = info.id;
        let returned_id = reg.register(info);
        assert_eq!(returned_id, id);
        assert!(reg.get(&id).is_some());
    }

    #[test]
    fn test_unregister() {
        let mut reg = TerminalRegistry::new();
        let info = make_terminal(TerminalStatus::Active);
        let id = info.id;
        reg.register(info);
        let removed = reg.unregister(&id);
        assert!(removed.is_some());
        assert!(reg.get(&id).is_none());
    }

    #[test]
    fn test_unregister_not_found() {
        let mut reg = TerminalRegistry::new();
        assert!(reg.unregister(&Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_get() {
        let mut reg = TerminalRegistry::new();
        let info = make_terminal(TerminalStatus::Idle);
        let id = info.id;
        reg.register(info);
        let t = reg.get(&id).unwrap();
        assert_eq!(t.title, "test terminal");
        assert_eq!(t.status, TerminalStatus::Idle);
    }

    #[test]
    fn test_list() {
        let mut reg = TerminalRegistry::new();
        reg.register(make_terminal(TerminalStatus::Active));
        reg.register(make_terminal(TerminalStatus::Closed));
        assert_eq!(reg.list().len(), 2);
    }

    #[test]
    fn test_list_active() {
        let mut reg = TerminalRegistry::new();
        reg.register(make_terminal(TerminalStatus::Active));
        reg.register(make_terminal(TerminalStatus::Idle));
        reg.register(make_terminal(TerminalStatus::Closed));
        let active = reg.list_active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].status, TerminalStatus::Active);
    }

    #[test]
    fn test_update_status() {
        let mut reg = TerminalRegistry::new();
        let info = make_terminal(TerminalStatus::Active);
        let id = info.id;
        reg.register(info);
        assert!(reg.update_status(&id, TerminalStatus::Closed));
        assert_eq!(reg.get(&id).unwrap().status, TerminalStatus::Closed);
    }

    #[test]
    fn test_update_status_not_found() {
        let mut reg = TerminalRegistry::new();
        assert!(!reg.update_status(&Uuid::new_v4(), TerminalStatus::Active));
    }
}
