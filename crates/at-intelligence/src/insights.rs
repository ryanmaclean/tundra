use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::IntelligenceError;

// ---------------------------------------------------------------------------
// ChatRole
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

// ---------------------------------------------------------------------------
// ChatMessage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// InsightsSession
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightsSession {
    pub id: Uuid,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub model: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// InsightsEngine
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct InsightsEngine {
    sessions: Vec<InsightsSession>,
}

impl InsightsEngine {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    pub fn create_session(&mut self, title: &str, model: &str) -> &InsightsSession {
        let session = InsightsSession {
            id: Uuid::new_v4(),
            title: title.to_string(),
            messages: Vec::new(),
            model: model.to_string(),
            created_at: Utc::now(),
        };
        self.sessions.push(session);
        self.sessions.last().unwrap()
    }

    pub fn list_sessions(&self) -> &[InsightsSession] {
        &self.sessions
    }

    pub fn get_session(&self, id: &Uuid) -> Option<&InsightsSession> {
        self.sessions.iter().find(|s| s.id == *id)
    }

    pub fn add_message(
        &mut self,
        session_id: &Uuid,
        role: ChatRole,
        content: &str,
    ) -> Result<(), IntelligenceError> {
        let session = self
            .sessions
            .iter_mut()
            .find(|s| s.id == *session_id)
            .ok_or(IntelligenceError::NotFound {
                entity: "session".into(),
                id: *session_id,
            })?;

        session.messages.push(ChatMessage {
            role,
            content: content.to_string(),
            timestamp: Utc::now(),
        });
        Ok(())
    }

    pub fn delete_session(&mut self, id: &Uuid) -> bool {
        let len_before = self.sessions.len();
        self.sessions.retain(|s| s.id != *id);
        self.sessions.len() < len_before
    }
}

impl Default for InsightsEngine {
    fn default() -> Self {
        Self::new()
    }
}
