use crate::memory::Memory;
use crate::types::{Message, Role};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info};

/// Token estimation for messages (rough heuristic: ~4 chars per token)
pub fn estimate_tokens(text: &str) -> u32 {
    (text.len() / 4) as u32
}

/// Calculate importance score for a message (0.0 to 1.0)
pub fn calculate_importance(msg: &Message) -> f32 {
    let mut score: f32 = 0.5; // Base score
    
    // System messages are always important
    if matches!(msg.role, Role::System) {
        return 1.0;
    }
    
    // Tool calls are important
    if msg.tool_calls.is_some() {
        score += 0.3;
    }
    
    // Longer messages might be more important
    if msg.content.len() > 500 {
        score += 0.1;
    }
    
    // Questions are important (simple heuristic)
    if msg.content.contains('?') {
        score += 0.1;
    }
    
    score.min(1.0)
}

/// Prune messages based on token budget and importance
pub fn prune_by_importance(messages: &[Message], max_tokens: u32) -> Vec<Message> {
    if messages.is_empty() {
        return vec![];
    }
    
    // Always keep system message if present
    let has_system = !messages.is_empty() && matches!(messages[0].role, Role::System);
    let system_msg = if has_system {
        Some(messages[0].clone())
    } else {
        None
    };
    
    let working_messages = if has_system {
        &messages[1..]
    } else {
        messages
    };
    
    // Calculate importance scores
    let mut scored: Vec<(usize, f32, u32)> = working_messages
        .iter()
        .enumerate()
        .map(|(idx, msg)| {
            let importance = calculate_importance(msg);
            let tokens = estimate_tokens(&msg.content);
            (idx, importance, tokens)
        })
        .collect();
    
    // Sort by importance (descending)
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    // Select messages that fit within budget
    let mut total_tokens = if let Some(ref sys) = system_msg {
        estimate_tokens(&sys.content)
    } else {
        0
    };
    
    let mut selected_indices = Vec::new();
    
    for (idx, _importance, tokens) in scored {
        if total_tokens + tokens <= max_tokens {
            selected_indices.push(idx);
            total_tokens += tokens;
        }
    }
    
    // Sort selected indices to maintain chronological order
    selected_indices.sort_unstable();
    
    // Build result
    let mut result = Vec::new();
    if let Some(sys) = system_msg {
        result.push(sys);
    }
    
    for idx in selected_indices {
        result.push(working_messages[idx].clone());
    }
    
    debug!(
        "Pruned {} messages to {} (target: {} tokens, actual: {} tokens)",
        messages.len(),
        result.len(),
        max_tokens,
        total_tokens
    );
    
    result
}

/// Memory wrapper with automatic pruning
pub struct PruningMemory<M: Memory> {
    inner: Arc<M>,
    max_tokens: u32,
    pruning_strategy: PruningStrategy,
}

#[derive(Debug, Clone, Copy)]
pub enum PruningStrategy {
    /// Keep most recent messages
    Recent,
    /// Keep most important messages
    Importance,
    /// Sliding window (keep last N messages)
    SlidingWindow(usize),
}

impl<M: Memory> PruningMemory<M> {
    pub fn new(inner: Arc<M>, max_tokens: u32, strategy: PruningStrategy) -> Self {
        info!(
            "PruningMemory initialized with max_tokens={}, strategy={:?}",
            max_tokens, strategy
        );
        Self {
            inner,
            max_tokens,
            pruning_strategy: strategy,
        }
    }
    
    fn prune_messages(&self, messages: Vec<Message>) -> Vec<Message> {
        match self.pruning_strategy {
            PruningStrategy::Recent => {
                self.prune_by_recency(messages)
            }
            PruningStrategy::Importance => {
                prune_by_importance(&messages, self.max_tokens)
            }
            PruningStrategy::SlidingWindow(window_size) => {
                self.prune_by_window(messages, window_size)
            }
        }
    }
    
    fn prune_by_recency(&self, messages: Vec<Message>) -> Vec<Message> {
        if messages.is_empty() {
            return vec![];
        }
        
        let has_system = matches!(messages[0].role, Role::System);
        let system_msg = if has_system {
            Some(messages[0].clone())
        } else {
            None
        };
        
        let mut total_tokens = if let Some(ref sys) = system_msg {
            estimate_tokens(&sys.content)
        } else {
            0
        };
        
        let mut result = Vec::new();
        if let Some(sys) = system_msg {
            result.push(sys);
        }
        
        // Add messages from the end (most recent)
        for msg in messages.iter().rev() {
            if matches!(msg.role, Role::System) {
                continue; // Already added
            }
            
            let tokens = estimate_tokens(&msg.content);
            if total_tokens + tokens <= self.max_tokens {
                result.insert(if result.is_empty() { 0 } else { 1 }, msg.clone());
                total_tokens += tokens;
            } else {
                break;
            }
        }
        
        result
    }
    
    fn prune_by_window(&self, messages: Vec<Message>, window_size: usize) -> Vec<Message> {
        if messages.len() <= window_size {
            return messages;
        }
        
        let mut result = Vec::new();
        
        // Keep system message if present
        if !messages.is_empty() && matches!(messages[0].role, Role::System) {
            result.push(messages[0].clone());
        }
        
        // Keep last N messages
        let start = messages.len().saturating_sub(window_size);
        for msg in &messages[start..] {
            if !matches!(msg.role, Role::System) {
                result.push(msg.clone());
            }
        }
        
        result
    }
}

#[async_trait]
impl<M: Memory> Memory for PruningMemory<M> {
    async fn append(&self, conversation_id: &str, messages: &[Message]) {
        self.inner.append(conversation_id, messages).await;
    }
    
    async fn history(&self, conversation_id: &str) -> Vec<Message> {
        let full_history = self.inner.history(conversation_id).await;
        self.prune_messages(full_history)
    }
    
    async fn clear(&self, conversation_id: &str) {
        self.inner.clear(conversation_id).await;
    }
    
    async fn history_within_budget(&self, conversation_id: &str, max_tokens: u32) -> Vec<Message> {
        let full_history = self.inner.history(conversation_id).await;
        
        // Use the smaller of the two budgets
        let effective_budget = max_tokens.min(self.max_tokens);
        
        match self.pruning_strategy {
            PruningStrategy::Importance => {
                prune_by_importance(&full_history, effective_budget)
            }
            _ => {
                // For other strategies, use the standard pruning
                let pruned = self.prune_messages(full_history);
                
                // Further prune if needed
                let mut total_tokens = 0;
                let mut result = Vec::new();
                
                for msg in pruned {
                    let tokens = estimate_tokens(&msg.content);
                    if total_tokens + tokens <= effective_budget {
                        result.push(msg);
                        total_tokens += tokens;
                    } else {
                        break;
                    }
                }
                
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::InMemoryMemory;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("test"), 1);
        assert_eq!(estimate_tokens("this is a test"), 3);
        assert_eq!(estimate_tokens("a".repeat(400).as_str()), 100);
    }

    #[test]
    fn test_calculate_importance() {
        let system_msg = Message::system("You are helpful");
        assert_eq!(calculate_importance(&system_msg), 1.0);
        
        let user_msg = Message::user("Hello");
        assert!(calculate_importance(&user_msg) >= 0.5);
        
        let question_msg = Message::user("What is this?");
        assert!(calculate_importance(&question_msg) > calculate_importance(&user_msg));
    }

    #[test]
    fn test_prune_by_importance() {
        let messages = vec![
            Message::system("You are a helpful assistant that provides detailed answers."),
            Message::user("Hello, how are you today?"),
            Message::assistant("I am doing well, thank you for asking!"),
            Message::user("What is the capital of France and why is it important?"),
            Message::assistant("The capital of France is Paris, which is important for many historical and cultural reasons."),
        ];
        
        let pruned = prune_by_importance(&messages, 30); // Budget that forces pruning
        
        // Should keep system message
        assert!(matches!(pruned[0].role, Role::System));
        
        // Should have fewer messages due to token budget
        assert!(pruned.len() < messages.len());
        assert!(pruned.len() >= 1); // At least system message
    }

    #[tokio::test]
    async fn test_pruning_memory_recent() {
        let inner = Arc::new(InMemoryMemory::new());
        let pruning = PruningMemory::new(inner.clone(), 100, PruningStrategy::Recent);
        
        let messages = vec![
            Message::system("System"),
            Message::user("Message 1"),
            Message::user("Message 2"),
            Message::user("Message 3"),
        ];
        
        pruning.append("test", &messages).await;
        
        let history = pruning.history("test").await;
        
        // Should keep system message
        assert!(matches!(history[0].role, Role::System));
        
        // Should have pruned some messages
        assert!(history.len() <= messages.len());
    }

    #[tokio::test]
    async fn test_pruning_memory_sliding_window() {
        let inner = Arc::new(InMemoryMemory::new());
        let pruning = PruningMemory::new(inner.clone(), 1000, PruningStrategy::SlidingWindow(2));
        
        let messages = vec![
            Message::system("System"),
            Message::user("Message 1"),
            Message::user("Message 2"),
            Message::user("Message 3"),
            Message::user("Message 4"),
        ];
        
        pruning.append("test", &messages).await;
        
        let history = pruning.history("test").await;
        
        // Should keep system message + last 2 messages
        assert_eq!(history.len(), 3);
        assert!(matches!(history[0].role, Role::System));
        assert_eq!(history[1].content, "Message 3");
        assert_eq!(history[2].content, "Message 4");
    }

    #[tokio::test]
    async fn test_pruning_memory_importance() {
        let inner = Arc::new(InMemoryMemory::new());
        let pruning = PruningMemory::new(inner.clone(), 100, PruningStrategy::Importance);
        
        let messages = vec![
            Message::system("System"),
            Message::user("Short"),
            Message::user("What is this very important question?"),
            Message::user("Another short one"),
        ];
        
        pruning.append("test", &messages).await;
        
        let history = pruning.history("test").await;
        
        // Should keep system message
        assert!(matches!(history[0].role, Role::System));
        
        // Should prioritize the question
        assert!(history.iter().any(|m| m.content.contains("important question")));
    }
}
