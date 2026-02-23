//! Prompt/response caching for LLM requests.
//!
//! Two caching strategies:
//! - **Hash cache**: Exact match on prompt hash (fast, deterministic).
//! - **Prefix cache**: Reuse cached responses when the prompt shares a common
//!   prefix with a previous request (for static system prompts).
//!
//! The cache is thread-safe and uses async RwLock for concurrent access.

use ahash::AHashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::llm::{LlmConfig, LlmMessage, LlmResponse};

// ---------------------------------------------------------------------------
// Cache Entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CacheEntry {
    response: LlmResponse,
    created_at: Instant,
    hit_count: u64,
    /// Hash of the full prompt (messages + config).
    prompt_hash: u64,
}

// ---------------------------------------------------------------------------
// Cache Config
// ---------------------------------------------------------------------------

/// Configuration for the token cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCacheConfig {
    /// Maximum number of entries in the cache.
    pub max_entries: usize,
    /// Time-to-live for cache entries.
    pub ttl_secs: u64,
    /// Enable hash-based exact caching.
    pub enable_hash_cache: bool,
    /// Enable prefix caching (reuse when system prompt matches).
    pub enable_prefix_cache: bool,
    /// Minimum prefix length (in chars) to consider for prefix cache.
    pub min_prefix_len: usize,
}

impl Default for TokenCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            ttl_secs: 3600, // 1 hour
            enable_hash_cache: true,
            enable_prefix_cache: true,
            min_prefix_len: 100,
        }
    }
}

// ---------------------------------------------------------------------------
// Cache Stats
// ---------------------------------------------------------------------------

/// Statistics about cache performance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_lookups: u64,
    pub hash_hits: u64,
    pub prefix_hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub total_entries: usize,
    /// Estimated tokens saved by cache hits.
    pub tokens_saved: u64,
    /// Estimated cost saved in USD.
    pub cost_saved: f64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        if self.total_lookups == 0 {
            return 0.0;
        }
        (self.hash_hits + self.prefix_hits) as f64 / self.total_lookups as f64
    }
}

// ---------------------------------------------------------------------------
// TokenCache
// ---------------------------------------------------------------------------

/// Thread-safe LLM response cache with hash and prefix strategies.
#[derive(Clone)]
pub struct TokenCache {
    config: TokenCacheConfig,
    /// Hash-based exact match cache: prompt_hash → entry.
    hash_cache: Arc<RwLock<AHashMap<u64, CacheEntry>>>,
    /// Prefix cache: system_prompt_hash → (full_entry, user_content_hash).
    prefix_cache: Arc<RwLock<AHashMap<u64, Vec<CacheEntry>>>>,
    stats: Arc<RwLock<CacheStats>>,
}

impl TokenCache {
    pub fn new(config: TokenCacheConfig) -> Self {
        Self {
            config,
            hash_cache: Arc::new(RwLock::new(AHashMap::new())),
            prefix_cache: Arc::new(RwLock::new(AHashMap::new())),
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Look up a cached response for the given messages and config.
    pub async fn get(&self, messages: &[LlmMessage], config: &LlmConfig) -> Option<LlmResponse> {
        let mut stats = self.stats.write().await;
        stats.total_lookups += 1;

        let prompt_hash = compute_prompt_hash(messages, config);

        // Try hash cache first (exact match)
        if self.config.enable_hash_cache {
            let mut cache = self.hash_cache.write().await;
            if let Some(entry) = cache.get_mut(&prompt_hash) {
                if entry.created_at.elapsed() < Duration::from_secs(self.config.ttl_secs) {
                    entry.hit_count += 1;
                    stats.hash_hits += 1;
                    stats.tokens_saved +=
                        entry.response.input_tokens + entry.response.output_tokens;
                    return Some(entry.response.clone());
                } else {
                    // Expired — remove it
                    cache.remove(&prompt_hash);
                }
            }
        }

        // Try prefix cache (system prompt match)
        if self.config.enable_prefix_cache {
            if let Some(system_hash) = compute_system_prefix_hash(messages, config) {
                let cache = self.prefix_cache.read().await;
                if let Some(entries) = cache.get(&system_hash) {
                    let user_hash = compute_user_content_hash(messages);
                    for entry in entries {
                        if entry.prompt_hash == user_hash
                            && entry.created_at.elapsed()
                                < Duration::from_secs(self.config.ttl_secs)
                        {
                            stats.prefix_hits += 1;
                            stats.tokens_saved +=
                                entry.response.input_tokens + entry.response.output_tokens;
                            return Some(entry.response.clone());
                        }
                    }
                }
            }
        }

        stats.misses += 1;
        None
    }

    /// Store a response in the cache.
    pub async fn put(&self, messages: &[LlmMessage], config: &LlmConfig, response: &LlmResponse) {
        let prompt_hash = compute_prompt_hash(messages, config);

        let entry = CacheEntry {
            response: response.clone(),
            created_at: Instant::now(),
            hit_count: 0,
            prompt_hash,
        };

        // Store in hash cache
        if self.config.enable_hash_cache {
            let mut cache = self.hash_cache.write().await;
            if cache.len() >= self.config.max_entries {
                self.evict_hash_cache(&mut cache).await;
            }
            cache.insert(prompt_hash, entry.clone());
        }

        // Store in prefix cache
        if self.config.enable_prefix_cache {
            if let Some(system_hash) = compute_system_prefix_hash(messages, config) {
                let user_hash = compute_user_content_hash(messages);
                let prefix_entry = CacheEntry {
                    response: response.clone(),
                    created_at: Instant::now(),
                    hit_count: 0,
                    prompt_hash: user_hash,
                };

                let mut cache = self.prefix_cache.write().await;
                cache
                    .entry(system_hash)
                    .or_insert_with(Vec::new)
                    .push(prefix_entry);
            }
        }

        let mut stats = self.stats.write().await;
        stats.total_entries = self.hash_cache.read().await.len();
    }

    /// Record estimated cost savings from a cache hit.
    pub async fn record_cost_saved(&self, cost: f64) {
        let mut stats = self.stats.write().await;
        stats.cost_saved += cost;
    }

    /// Get current cache statistics.
    pub async fn stats(&self) -> CacheStats {
        let mut stats = self.stats.read().await.clone();
        stats.total_entries = self.hash_cache.read().await.len();
        stats
    }

    /// Clear all cached entries.
    pub async fn clear(&self) {
        self.hash_cache.write().await.clear();
        self.prefix_cache.write().await.clear();
    }

    /// Evict the least-recently-used entries from hash cache.
    async fn evict_hash_cache(&self, cache: &mut AHashMap<u64, CacheEntry>) {
        // Remove expired entries first
        let ttl = Duration::from_secs(self.config.ttl_secs);
        let before = cache.len();
        cache.retain(|_, entry| entry.created_at.elapsed() < ttl);
        let expired = before - cache.len();

        // If still over capacity, remove least-hit entries
        if cache.len() >= self.config.max_entries {
            let mut entries: Vec<(u64, u64, Instant)> = cache
                .iter()
                .map(|(k, v)| (*k, v.hit_count, v.created_at))
                .collect();
            entries.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));

            let to_remove = cache.len() - self.config.max_entries / 2;
            for (key, _, _) in entries.into_iter().take(to_remove) {
                cache.remove(&key);
            }
        }

        let mut stats = self.stats.write().await;
        stats.evictions += expired as u64 + (before - cache.len()) as u64;
    }
}

impl Default for TokenCache {
    fn default() -> Self {
        Self::new(TokenCacheConfig::default())
    }
}

// ---------------------------------------------------------------------------
// Hashing helpers
// ---------------------------------------------------------------------------

fn compute_prompt_hash(messages: &[LlmMessage], config: &LlmConfig) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    config.model.hash(&mut hasher);
    config.max_tokens.hash(&mut hasher);
    // Hash temperature as bits to avoid float hashing issues
    config.temperature.to_bits().hash(&mut hasher);
    if let Some(ref sp) = config.system_prompt {
        sp.hash(&mut hasher);
    }
    for msg in messages {
        msg.content.hash(&mut hasher);
        format!("{:?}", msg.role).hash(&mut hasher);
    }
    hasher.finish()
}

fn compute_system_prefix_hash(messages: &[LlmMessage], config: &LlmConfig) -> Option<u64> {
    let mut system_text = String::new();
    if let Some(ref sp) = config.system_prompt {
        system_text.push_str(sp);
    }
    for msg in messages {
        if matches!(msg.role, crate::llm::LlmRole::System) {
            system_text.push_str(&msg.content);
        }
    }
    if system_text.is_empty() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    system_text.hash(&mut hasher);
    config.model.hash(&mut hasher);
    Some(hasher.finish())
}

fn compute_user_content_hash(messages: &[LlmMessage]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for msg in messages {
        if !matches!(msg.role, crate::llm::LlmRole::System) {
            msg.content.hash(&mut hasher);
            format!("{:?}", msg.role).hash(&mut hasher);
        }
    }
    hasher.finish()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmConfig, LlmMessage, LlmResponse, LlmRole};

    fn test_config() -> LlmConfig {
        LlmConfig {
            model: "test-model".into(),
            max_tokens: 512,
            temperature: 0.5,
            system_prompt: Some("You are a test assistant".into()),
        }
    }

    fn test_response() -> LlmResponse {
        LlmResponse {
            content: "cached answer".into(),
            model: "test-model".into(),
            input_tokens: 100,
            output_tokens: 50,
            finish_reason: "end_turn".into(),
        }
    }

    fn test_messages() -> Vec<LlmMessage> {
        vec![LlmMessage::user("What is 2+2?")]
    }

    // -- Hash Cache --

    #[tokio::test]
    async fn hash_cache_miss_then_hit() {
        let cache = TokenCache::new(TokenCacheConfig::default());
        let messages = test_messages();
        let config = test_config();

        // Miss
        assert!(cache.get(&messages, &config).await.is_none());

        // Store
        cache.put(&messages, &config, &test_response()).await;

        // Hit
        let cached = cache.get(&messages, &config).await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().content, "cached answer");
    }

    #[tokio::test]
    async fn hash_cache_different_prompts_no_collision() {
        let cache = TokenCache::new(TokenCacheConfig::default());
        let config = test_config();

        let msg1 = vec![LlmMessage::user("Hello")];
        let msg2 = vec![LlmMessage::user("Goodbye")];

        cache.put(&msg1, &config, &test_response()).await;

        assert!(cache.get(&msg1, &config).await.is_some());
        assert!(cache.get(&msg2, &config).await.is_none());
    }

    #[tokio::test]
    async fn hash_cache_different_models_no_collision() {
        let cache = TokenCache::new(TokenCacheConfig::default());
        let messages = test_messages();

        let config1 = LlmConfig {
            model: "model-a".into(),
            ..test_config()
        };
        let config2 = LlmConfig {
            model: "model-b".into(),
            ..test_config()
        };

        cache.put(&messages, &config1, &test_response()).await;

        assert!(cache.get(&messages, &config1).await.is_some());
        assert!(cache.get(&messages, &config2).await.is_none());
    }

    #[tokio::test]
    async fn hash_cache_ttl_expiry() {
        let cache = TokenCache::new(TokenCacheConfig {
            ttl_secs: 0, // expire immediately
            ..Default::default()
        });
        let messages = test_messages();
        let config = test_config();

        cache.put(&messages, &config, &test_response()).await;

        // Should be expired
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(cache.get(&messages, &config).await.is_none());
    }

    // -- Prefix Cache --

    #[tokio::test]
    async fn prefix_cache_same_system_prompt() {
        let cache = TokenCache::new(TokenCacheConfig {
            enable_hash_cache: false,
            enable_prefix_cache: true,
            ..Default::default()
        });

        let config = test_config();
        let messages = test_messages();

        cache.put(&messages, &config, &test_response()).await;

        // Same system prompt + same user content = hit
        let cached = cache.get(&messages, &config).await;
        assert!(cached.is_some());
    }

    #[tokio::test]
    async fn prefix_cache_no_system_prompt_skips() {
        let cache = TokenCache::new(TokenCacheConfig {
            enable_hash_cache: false,
            enable_prefix_cache: true,
            ..Default::default()
        });

        let config = LlmConfig {
            system_prompt: None,
            ..test_config()
        };
        let messages = vec![LlmMessage::user("Hello")];

        cache.put(&messages, &config, &test_response()).await;

        // No system prompt means no prefix cache entry
        let cached = cache.get(&messages, &config).await;
        assert!(cached.is_none());
    }

    // -- Stats --

    #[tokio::test]
    async fn stats_track_hits_and_misses() {
        let cache = TokenCache::new(TokenCacheConfig::default());
        let messages = test_messages();
        let config = test_config();

        // Miss
        cache.get(&messages, &config).await;
        // Store + hit
        cache.put(&messages, &config, &test_response()).await;
        cache.get(&messages, &config).await;

        let stats = cache.stats().await;
        assert_eq!(stats.total_lookups, 2);
        assert_eq!(stats.misses, 1);
        assert!(stats.hash_hits >= 1);
        assert!(stats.tokens_saved > 0);
    }

    #[tokio::test]
    async fn stats_hit_rate() {
        let stats = CacheStats {
            total_lookups: 10,
            hash_hits: 3,
            prefix_hits: 2,
            misses: 5,
            ..Default::default()
        };
        assert!((stats.hit_rate() - 0.5).abs() < 0.001);
    }

    #[tokio::test]
    async fn stats_hit_rate_zero_lookups() {
        let stats = CacheStats::default();
        assert_eq!(stats.hit_rate(), 0.0);
    }

    // -- Clear --

    #[tokio::test]
    async fn clear_removes_all_entries() {
        let cache = TokenCache::new(TokenCacheConfig::default());
        let messages = test_messages();
        let config = test_config();

        cache.put(&messages, &config, &test_response()).await;
        assert!(cache.get(&messages, &config).await.is_some());

        cache.clear().await;
        assert!(cache.get(&messages, &config).await.is_none());
    }

    // -- Eviction --

    #[tokio::test]
    async fn eviction_when_over_capacity() {
        let cache = TokenCache::new(TokenCacheConfig {
            max_entries: 2,
            ..Default::default()
        });
        let config = test_config();

        for i in 0..5 {
            let messages = vec![LlmMessage::user(format!("Question {i}"))];
            cache.put(&messages, &config, &test_response()).await;
        }

        let stats = cache.stats().await;
        assert!(stats.total_entries <= 2);
    }

    // -- Cost saved --

    #[tokio::test]
    async fn record_cost_saved() {
        let cache = TokenCache::new(TokenCacheConfig::default());
        cache.record_cost_saved(0.05).await;
        cache.record_cost_saved(0.03).await;

        let stats = cache.stats().await;
        assert!((stats.cost_saved - 0.08).abs() < 0.001);
    }

    // -- Config serialization --

    #[test]
    fn cache_config_serialization() {
        let config = TokenCacheConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deser: TokenCacheConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.max_entries, 1000);
        assert_eq!(deser.ttl_secs, 3600);
    }

    // -- Hash determinism --

    #[test]
    fn prompt_hash_is_deterministic() {
        let messages = test_messages();
        let config = test_config();
        let h1 = compute_prompt_hash(&messages, &config);
        let h2 = compute_prompt_hash(&messages, &config);
        assert_eq!(h1, h2);
    }

    #[test]
    fn prompt_hash_differs_for_different_content() {
        let config = test_config();
        let m1 = vec![LlmMessage::user("Hello")];
        let m2 = vec![LlmMessage::user("World")];
        assert_ne!(
            compute_prompt_hash(&m1, &config),
            compute_prompt_hash(&m2, &config)
        );
    }
}
