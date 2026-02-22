use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use crate::types::Message;

/// Trait for conversation memory backends.
#[async_trait::async_trait]
pub trait Memory: Send + Sync {
    /// Store messages for a conversation.
    async fn append(&self, conversation_id: &str, messages: &[Message]);

    /// Retrieve the full history for a conversation.
    async fn history(&self, conversation_id: &str) -> Vec<Message>;

    /// Clear a conversation's history.
    async fn clear(&self, conversation_id: &str);

    /// Get a truncated history that fits within a token budget.
    /// Default implementation returns full history (override for smarter truncation).
    async fn history_within_budget(&self, conversation_id: &str, _max_tokens: u32) -> Vec<Message> {
        self.history(conversation_id).await
    }
}

// ---------------------------------------------------------------------------
// In-memory implementation
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct InMemoryMemory {
    conversations: Arc<RwLock<HashMap<String, Vec<Message>>>>,
}

impl InMemoryMemory {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl Memory for InMemoryMemory {
    async fn append(&self, conversation_id: &str, messages: &[Message]) {
        let mut store = self.conversations.write().await;
        let conv = store.entry(conversation_id.to_string()).or_default();
        conv.extend(messages.iter().cloned());
        debug!(
            "Memory: conversation '{}' now has {} messages",
            conversation_id,
            conv.len()
        );
    }

    async fn history(&self, conversation_id: &str) -> Vec<Message> {
        let store = self.conversations.read().await;
        store.get(conversation_id).cloned().unwrap_or_default()
    }

    async fn clear(&self, conversation_id: &str) {
        let mut store = self.conversations.write().await;
        store.remove(conversation_id);
        debug!("Memory: cleared conversation '{}'", conversation_id);
    }

    async fn history_within_budget(&self, conversation_id: &str, max_tokens: u32) -> Vec<Message> {
        let history = self.history(conversation_id).await;

        // Simple heuristic: ~4 chars per token, keep messages from the end.
        let max_chars = (max_tokens * 4) as usize;
        let mut total_chars = 0;
        let mut start_idx = history.len();

        for (i, msg) in history.iter().enumerate().rev() {
            total_chars += msg.content.len();
            if total_chars > max_chars {
                start_idx = i + 1;
                break;
            }
            start_idx = i;
        }

        // Always keep the system message if there is one.
        let mut result = vec![];
        if !history.is_empty() && history[0].role == crate::types::Role::System && start_idx > 0 {
            result.push(history[0].clone());
        }
        result.extend_from_slice(&history[start_idx..]);
        result
    }
}

// ---------------------------------------------------------------------------
// Sliding window memory (keeps last N messages)
// ---------------------------------------------------------------------------

pub struct SlidingWindowMemory {
    inner: InMemoryMemory,
    window_size: usize,
}

impl SlidingWindowMemory {
    pub fn new(window_size: usize) -> Self {
        Self {
            inner: InMemoryMemory::new(),
            window_size,
        }
    }
}

#[async_trait::async_trait]
impl Memory for SlidingWindowMemory {
    async fn append(&self, conversation_id: &str, messages: &[Message]) {
        self.inner.append(conversation_id, messages).await;
    }

    async fn history(&self, conversation_id: &str) -> Vec<Message> {
        let full = self.inner.history(conversation_id).await;
        if full.len() <= self.window_size {
            return full;
        }

        let mut result = vec![];
        // Keep system message.
        if !full.is_empty() && full[0].role == crate::types::Role::System {
            result.push(full[0].clone());
        }
        let start = full.len().saturating_sub(self.window_size);
        result.extend_from_slice(&full[start..]);
        result
    }

    async fn clear(&self, conversation_id: &str) {
        self.inner.clear(conversation_id).await;
    }
}
