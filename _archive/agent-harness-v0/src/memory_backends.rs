use async_trait::async_trait;
use rusqlite::params;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_rusqlite::Connection;
use tracing::{debug, error, info};

use crate::memory::Memory;
use crate::types::{Message, Role, ToolCall};

// ---------------------------------------------------------------------------
// SQLite Memory Backend
// ---------------------------------------------------------------------------

pub struct SqliteMemory {
    conn: Arc<RwLock<Connection>>,
}

impl SqliteMemory {
    pub async fn new(db_path: PathBuf) -> Result<Self, tokio_rusqlite::Error> {
        let conn = Connection::open(db_path).await?;
        
        // Create table if it doesn't exist
        conn.call(|conn| {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    conversation_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    tool_calls TEXT,
                    tool_call_id TEXT,
                    timestamp INTEGER NOT NULL,
                    UNIQUE(conversation_id, id)
                )",
                [],
            )?;
            
            // Create index for faster lookups
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_conversation_id ON messages(conversation_id)",
                [],
            )?;
            
            Ok(())
        })
        .await?;
        
        info!("SqliteMemory initialized");
        Ok(Self {
            conn: Arc::new(RwLock::new(conn)),
        })
    }
    
    pub async fn new_in_memory() -> Result<Self, tokio_rusqlite::Error> {
        let conn = Connection::open_in_memory().await?;
        
        conn.call(|conn| {
            conn.execute(
                "CREATE TABLE messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    conversation_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    tool_calls TEXT,
                    tool_call_id TEXT,
                    timestamp INTEGER NOT NULL
                )",
                [],
            )?;
            Ok(())
        })
        .await?;
        
        Ok(Self {
            conn: Arc::new(RwLock::new(conn)),
        })
    }
}

#[async_trait]
impl Memory for SqliteMemory {
    async fn append(&self, conversation_id: &str, messages: &[Message]) {
        let conn = self.conn.read().await;
        let conversation_id_owned = conversation_id.to_string();
        let conversation_id_log = conversation_id.to_string();
        let messages = messages.to_vec();
        let msg_count = messages.len();
        
        let result = conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                
                for msg in &messages {
                    let role = match msg.role {
                        Role::System => "system",
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::Tool => "tool",
                    };
                    
                    let tool_calls_json = if let Some(ref calls) = msg.tool_calls {
                        Some(serde_json::to_string(calls).unwrap_or_default())
                    } else {
                        None
                    };
                    
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64;
                    
                    tx.execute(
                        "INSERT INTO messages (conversation_id, role, content, tool_calls, tool_call_id, timestamp)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            &conversation_id_owned,
                            role,
                            &msg.content,
                            tool_calls_json,
                            &msg.tool_call_id,
                            timestamp,
                        ],
                    )?;
                }
                
                tx.commit()?;
                Ok(())
            })
            .await;
        
        match result {
            Ok(_) => debug!("SqliteMemory: appended {} messages to '{}'", msg_count, conversation_id_log),
            Err(e) => error!("SqliteMemory: failed to append messages: {}", e),
        }
    }
    
    async fn history(&self, conversation_id: &str) -> Vec<Message> {
        let conn = self.conn.read().await;
        let conversation_id_owned = conversation_id.to_string();
        
        let result = conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT role, content, tool_calls, tool_call_id FROM messages 
                     WHERE conversation_id = ?1 ORDER BY id ASC",
                )?;
                
                let messages = stmt
                    .query_map([&conversation_id_owned], |row| {
                        let role_str: String = row.get(0)?;
                        let role = match role_str.as_str() {
                            "system" => Role::System,
                            "user" => Role::User,
                            "assistant" => Role::Assistant,
                            "tool" => Role::Tool,
                            _ => Role::User,
                        };
                        
                        let content: String = row.get(1)?;
                        let tool_calls_json: Option<String> = row.get(2)?;
                        let tool_call_id: Option<String> = row.get(3)?;
                        
                        let tool_calls = tool_calls_json
                            .and_then(|json| serde_json::from_str::<Vec<ToolCall>>(&json).ok());
                        
                        Ok(Message {
                            role,
                            content,
                            tool_calls,
                            tool_call_id,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                
                Ok(messages)
            })
            .await;
        
        match result {
            Ok(messages) => messages,
            Err(e) => {
                error!("SqliteMemory: failed to retrieve history: {}", e);
                vec![]
            }
        }
    }
    
    async fn clear(&self, conversation_id: &str) {
        let conn = self.conn.read().await;
        let conversation_id_owned = conversation_id.to_string();
        let conversation_id_log = conversation_id.to_string();
        
        let result = conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM messages WHERE conversation_id = ?1",
                    [&conversation_id_owned],
                )?;
                Ok(())
            })
            .await;
        
        match result {
            Ok(_) => debug!("SqliteMemory: cleared conversation '{}'", conversation_id_log),
            Err(e) => error!("SqliteMemory: failed to clear conversation: {}", e),
        }
    }
}

// ---------------------------------------------------------------------------
// File System Memory Backend
// ---------------------------------------------------------------------------

use tokio::fs;

pub struct FileSystemMemory {
    base_path: PathBuf,
}

impl FileSystemMemory {
    pub async fn new(base_path: PathBuf) -> Result<Self, std::io::Error> {
        // Create base directory if it doesn't exist
        fs::create_dir_all(&base_path).await?;
        
        info!("FileSystemMemory initialized at {:?}", base_path);
        Ok(Self { base_path })
    }
    
    fn conversation_path(&self, conversation_id: &str) -> PathBuf {
        // Sanitize conversation_id to be filesystem-safe
        let safe_id = conversation_id
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>();
        
        self.base_path.join(format!("{}.json", safe_id))
    }
}

#[async_trait]
impl Memory for FileSystemMemory {
    async fn append(&self, conversation_id: &str, messages: &[Message]) {
        let path = self.conversation_path(conversation_id);
        
        // Read existing messages
        let mut existing = self.history(conversation_id).await;
        
        // Append new messages
        existing.extend(messages.iter().cloned());
        
        // Serialize to JSON
        let json = match serde_json::to_string_pretty(&existing) {
            Ok(j) => j,
            Err(e) => {
                error!("FileSystemMemory: failed to serialize messages: {}", e);
                return;
            }
        };
        
        // Write to file
        match fs::write(&path, json).await {
            Ok(_) => debug!(
                "FileSystemMemory: appended {} messages to '{}'",
                messages.len(),
                conversation_id
            ),
            Err(e) => error!("FileSystemMemory: failed to write file: {}", e),
        }
    }
    
    async fn history(&self, conversation_id: &str) -> Vec<Message> {
        let path = self.conversation_path(conversation_id);
        
        // Check if file exists
        if !path.exists() {
            return vec![];
        }
        
        // Read file
        let contents = match fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                error!("FileSystemMemory: failed to read file: {}", e);
                return vec![];
            }
        };
        
        // Deserialize from JSON
        match serde_json::from_str::<Vec<Message>>(&contents) {
            Ok(messages) => messages,
            Err(e) => {
                error!("FileSystemMemory: failed to deserialize messages: {}", e);
                vec![]
            }
        }
    }
    
    async fn clear(&self, conversation_id: &str) {
        let path = self.conversation_path(conversation_id);
        
        if path.exists() {
            match fs::remove_file(&path).await {
                Ok(_) => debug!("FileSystemMemory: cleared conversation '{}'", conversation_id),
                Err(e) => error!("FileSystemMemory: failed to remove file: {}", e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_sqlite_memory_basic() {
        let memory = SqliteMemory::new_in_memory().await.unwrap();
        
        let messages = vec![
            Message::system("You are a helpful assistant."),
            Message::user("Hello!"),
        ];
        
        memory.append("test-conv", &messages).await;
        
        let history = memory.history("test-conv").await;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "You are a helpful assistant.");
        assert_eq!(history[1].content, "Hello!");
    }

    #[tokio::test]
    async fn test_sqlite_memory_clear() {
        let memory = SqliteMemory::new_in_memory().await.unwrap();
        
        let messages = vec![Message::user("Test message")];
        memory.append("test-conv", &messages).await;
        
        assert_eq!(memory.history("test-conv").await.len(), 1);
        
        memory.clear("test-conv").await;
        assert_eq!(memory.history("test-conv").await.len(), 0);
    }

    #[tokio::test]
    async fn test_filesystem_memory_basic() {
        let temp_dir = TempDir::new().unwrap();
        let memory = FileSystemMemory::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        
        let messages = vec![
            Message::system("You are a helpful assistant."),
            Message::user("Hello!"),
        ];
        
        memory.append("test-conv", &messages).await;
        
        let history = memory.history("test-conv").await;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "You are a helpful assistant.");
        assert_eq!(history[1].content, "Hello!");
    }

    #[tokio::test]
    async fn test_filesystem_memory_clear() {
        let temp_dir = TempDir::new().unwrap();
        let memory = FileSystemMemory::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        
        let messages = vec![Message::user("Test message")];
        memory.append("test-conv", &messages).await;
        
        assert_eq!(memory.history("test-conv").await.len(), 1);
        
        memory.clear("test-conv").await;
        assert_eq!(memory.history("test-conv").await.len(), 0);
    }

    #[tokio::test]
    async fn test_filesystem_memory_sanitizes_id() {
        let temp_dir = TempDir::new().unwrap();
        let memory = FileSystemMemory::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        
        // Test with special characters that should be sanitized
        let messages = vec![Message::user("Test")];
        memory.append("test/conv:with*special?chars", &messages).await;
        
        let history = memory.history("test/conv:with*special?chars").await;
        assert_eq!(history.len(), 1);
    }
}
