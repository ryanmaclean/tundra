use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tracing::{warn, info, error};

use crate::types::ProviderKind;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("API key format is invalid")]
    InvalidFormat,
    #[error("API key is too short (minimum {min} characters)")]
    TooShort { min: usize },
    #[error("API key is too long (maximum {max} characters)")]
    TooLong { max: usize },
    #[error("API key contains invalid characters")]
    InvalidCharacters,
    #[error("API key is on a blocklist")]
    Blocklisted,
    #[error("API key has expired")]
    Expired,
    #[error("API key provider is not supported")]
    UnsupportedProvider,
    #[error("API key validation failed: {reason}")]
    Custom { reason: String },
}

/// Validates and sanitizes API keys for different providers
pub struct ApiKeyValidator {
    patterns: HashMap<ProviderKind, ValidationPattern>,
    blocklist: HashSet<String>,
    strict_mode: bool,
}

#[derive(Debug, Clone)]
pub struct ValidationPattern {
    pub regex: Regex,
    pub min_length: usize,
    pub max_length: usize,
    pub allowed_chars: String,
    pub provider_name: String,
}

impl ApiKeyValidator {
    pub fn new() -> Result<Self, ValidationError> {
        let mut patterns = HashMap::new();
        
        // OpenRouter pattern (sk-or-v1-xxxxx)
        patterns.insert(
            ProviderKind::OpenRouter,
            ValidationPattern {
                regex: Regex::new(r"^sk-or-v1-[a-zA-Z0-9]{32,64}$").map_err(|e| {
                    ValidationError::Custom { 
                        reason: format!("Failed to compile OpenRouter regex: {}", e) 
                    }
                })?,
                min_length: 40,
                max_length: 80,
                allowed_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-".to_string(),
                provider_name: "OpenRouter".to_string(),
            }
        );

        // HuggingFace pattern (hf_xxxxxx)
        patterns.insert(
            ProviderKind::HuggingFace,
            ValidationPattern {
                regex: Regex::new(r"^hf_[a-zA-Z0-9]{34,}$").map_err(|e| {
                    ValidationError::Custom { 
                        reason: format!("Failed to compile HuggingFace regex: {}", e) 
                    }
                })?,
                min_length: 37,
                max_length: 100,
                allowed_chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_".to_string(),
                provider_name: "HuggingFace".to_string(),
            }
        );

        // Common blocklist patterns (obviously fake keys)
        let blocklist: HashSet<String> = HashSet::from([
            "sk-or-v1-0000000000000000000000000000000000".to_string(),
            "sk-or-v1-1111111111111111111111111111111111".to_string(),
            "sk-or-v1-2222222222222222222222222222222222".to_string(),
            "sk-or-v1-3333333333333333333333333333333333".to_string(),
            "sk-or-v1-4444444444444444444444444444444444".to_string(),
            "sk-or-v1-5555555555555555555555555555555555".to_string(),
            "sk-or-v1-6666666666666666666666666666666666".to_string(),
            "sk-or-v1-7777777777777777777777777777777777".to_string(),
            "sk-or-v1-8888888888888888888888888888888888888".to_string(),
            "sk-or-v1-9999999999999999999999999999999999".to_string(),
            "sk-or-v1-abcdefghijklmnopqrstuvwxzyzab".to_string(),
            "your-api-key-here".to_string(),
            "test-key".to_string(),
            "demo-key".to_string(),
            "api-key".to_string(),
            "1234567890".to_string(),
        ]);

        Ok(Self {
            patterns,
            blocklist,
            strict_mode: true,
        })
    }

    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    pub fn add_blocklist_key(mut self, key: impl Into<String>) -> Self {
        self.blocklist.insert(key.into());
        self
    }

    pub fn add_pattern(mut self, provider: ProviderKind, pattern: ValidationPattern) -> Self {
        self.patterns.insert(provider, pattern);
        self
    }

    /// Validate and sanitize an API key
    pub fn validate_and_sanitize(&self, provider: ProviderKind, api_key: &str) -> Result<String, ValidationError> {
        // Step 1: Basic sanitization
        let sanitized = self.sanitize(api_key);

        // Step 2: Check blocklist
        if self.blocklist.contains(&sanitized) {
            warn!("API key found in blocklist");
            return Err(ValidationError::Blocklisted);
        }

        // Step 3: Get validation pattern
        let pattern = self.patterns.get(&provider).ok_or_else(|| {
            ValidationError::UnsupportedProvider
        })?;

        // Step 4: Length validation
        if sanitized.len() < pattern.min_length {
            return Err(ValidationError::TooShort { 
                min: pattern.min_length 
            });
        }

        if sanitized.len() > pattern.max_length {
            return Err(ValidationError::TooLong { 
                max: pattern.max_length 
            });
        }

        // Step 5: Character validation
        if !self.validate_characters(&sanitized, &pattern.allowed_chars) {
            return Err(ValidationError::InvalidCharacters);
        }

        // Step 6: Regex validation (if strict mode)
        if self.strict_mode && !pattern.regex.is_match(&sanitized) {
            return Err(ValidationError::InvalidFormat);
        }

        info!("API key validated successfully for {}", pattern.provider_name);
        Ok(sanitized)
    }

    /// Quick validation without sanitization (for performance)
    pub fn quick_validate(&self, provider: ProviderKind, api_key: &str) -> Result<(), ValidationError> {
        let pattern = self.patterns.get(&provider).ok_or_else(|| {
            ValidationError::UnsupportedProvider
        })?;

        if api_key.len() < pattern.min_length {
            return Err(ValidationError::TooShort { 
                min: pattern.min_length 
            });
        }

        if api_key.len() > pattern.max_length {
            return Err(ValidationError::TooLong { 
                max: pattern.max_length 
            });
        }

        if self.strict_mode && !pattern.regex.is_match(api_key) {
            return Err(ValidationError::InvalidFormat);
        }

        Ok(())
    }

    /// Sanitize API key by removing whitespace and normalizing
    fn sanitize(&self, api_key: &str) -> String {
        api_key
            .trim()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    }

    /// Validate characters against allowed set
    fn validate_characters(&self, api_key: &str, allowed: &str) -> bool {
        let allowed_set: HashSet<char> = allowed.chars().collect();
        api_key.chars().all(|c| allowed_set.contains(&c))
    }

    /// Get validation info for a provider
    pub fn get_validation_info(&self, provider: ProviderKind) -> Option<&ValidationPattern> {
        self.patterns.get(&provider)
    }

    /// Check if a provider is supported
    pub fn is_supported(&self, provider: ProviderKind) -> bool {
        self.patterns.contains_key(&provider)
    }

    /// Get all supported providers
    pub fn supported_providers(&self) -> Vec<ProviderKind> {
        self.patterns.keys().cloned().collect()
    }
}

/// API key manager with caching and rotation support
pub struct ApiKeyManager {
    validator: Arc<ApiKeyValidator>,
    cache: Arc<tokio::sync::RwLock<HashMap<String, CachedKey>>>,
    rotation_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct CachedKey {
    pub key: String,
    pub provider: ProviderKind,
    pub validated_at: std::time::SystemTime,
    pub last_used: std::time::SystemTime,
    pub usage_count: u64,
}

impl ApiKeyManager {
    pub fn new(validator: Arc<ApiKeyValidator>) -> Self {
        Self {
            validator,
            cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            rotation_enabled: false,
        }
    }

    pub fn with_rotation(mut self, enabled: bool) -> Self {
        self.rotation_enabled = enabled;
        self
    }

    /// Add and validate an API key
    pub async fn add_key(&self, key_id: &str, provider: ProviderKind, api_key: &str) -> Result<(), ValidationError> {
        let validated_key = self.validator.validate_and_sanitize(provider, api_key)?;
        
        let mut cache = self.cache.write().await;
        cache.insert(key_id.to_string(), CachedKey {
            key: validated_key,
            provider,
            validated_at: std::time::SystemTime::now(),
            last_used: std::time::SystemTime::now(),
            usage_count: 0,
        });

        info!("API key '{}' added and validated for {:?}", key_id, provider);
        Ok(())
    }

    /// Get a cached API key
    pub async fn get_key(&self, key_id: &str) -> Option<String> {
        let mut cache = self.cache.write().await;
        if let Some(cached) = cache.get_mut(key_id) {
            cached.last_used = std::time::SystemTime::now();
            cached.usage_count += 1;
            Some(cached.key.clone())
        } else {
            None
        }
    }

    /// Remove an API key
    pub async fn remove_key(&self, key_id: &str) -> bool {
        let mut cache = self.cache.write().await;
        let removed = cache.remove(key_id).is_some();
        if removed {
            info!("API key '{}' removed", key_id);
        }
        removed
    }

    /// List all cached keys (without exposing the actual keys)
    pub async fn list_keys(&self) -> Vec<String> {
        let cache = self.cache.read().await;
        cache.keys().cloned().collect()
    }

    /// Get key metadata
    pub async fn get_key_metadata(&self, key_id: &str) -> Option<(ProviderKind, std::time::SystemTime, u64)> {
        let cache = self.cache.read().await;
        cache.get(key_id).map(|cached| {
            (
                cached.provider,
                cached.validated_at,
                cached.usage_count,
            )
        })
    }

    /// Clean up old keys (older than specified duration)
    pub async fn cleanup_old_keys(&self, max_age: std::time::Duration) -> usize {
        let mut cache = self.cache.write().await;
        let now = std::time::SystemTime::now();
        let mut removed = 0;

        cache.retain(|_, cached| {
            let age = now.duration_since(cached.validated_at).unwrap_or_default();
            if age > max_age {
                removed += 1;
                false
            } else {
                true
            }
        });

        if removed > 0 {
            info!("Cleaned up {} old API keys", removed);
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProviderKind;

    #[test]
    fn test_api_key_validation() {
        let validator = ApiKeyValidator::new().unwrap();

        // Valid OpenRouter key
        let valid_key = "sk-or-v1-abcdefghijklmnopqrstuvwxyz123456";
        let result = validator.validate_and_sanitize(ProviderKind::OpenRouter, valid_key);
        assert!(result.is_ok());

        // Invalid format
        let invalid_key = "invalid-key";
        let result = validator.validate_and_sanitize(ProviderKind::OpenRouter, invalid_key);
        assert!(result.is_err());

        // Blocklisted key
        let blocklisted_key = "sk-or-v1-0000000000000000000000000000000000";
        let result = validator.validate_and_sanitize(ProviderKind::OpenRouter, blocklisted_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_key_sanitization() {
        let validator = ApiKeyValidator::new().unwrap();
        
        // Key with whitespace
        let dirty_key = "  sk-or-v1-abcdefghijklmnopqrstuvwxyz123456  ";
        let result = validator.validate_and_sanitize(ProviderKind::OpenRouter, dirty_key);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "sk-or-v1-abcdefghijklmnopqrstuvwxyz123456");
    }

    #[tokio::test]
    async fn test_api_key_manager() {
        let validator = Arc::new(ApiKeyValidator::new().unwrap());
        let manager = ApiKeyManager::new(validator);

        // Add a key
        let key = "sk-or-v1-abcdefghijklmnopqrstuvwxyz123456";
        let result = manager.add_key("test-key", ProviderKind::OpenRouter, key).await;
        assert!(result.is_ok());

        // Retrieve the key
        let retrieved = manager.get_key("test-key").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), key);

        // List keys
        let keys = manager.list_keys().await;
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&"test-key".to_string()));

        // Remove key
        let removed = manager.remove_key("test-key").await;
        assert!(removed);

        // Verify it's gone
        let keys = manager.list_keys().await;
        assert_eq!(keys.len(), 0);
    }
}
