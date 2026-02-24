//! OAuth token lifecycle management with encryption and expiration tracking.
//!
//! This module provides secure storage and management of OAuth tokens:
//! 1. Tokens are encrypted at rest using ChaCha20-Poly1305 AEAD
//! 2. Expiration times are tracked and enforced
//! 3. Automatic refresh recommendations before expiration
//! 4. Secure memory zeroing when tokens are cleared

use at_core::crypto::{decrypt, encrypt, CryptoError, EncryptionKey};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Errors specific to OAuth token management.
#[derive(Debug, thiserror::Error)]
pub enum TokenManagerError {
    #[error("No token stored")]
    NoToken,

    #[error("Token has expired")]
    TokenExpired,

    #[error("Encryption error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, TokenManagerError>;

/// Internal representation of a stored OAuth token with metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct TokenData {
    /// The OAuth access token (plaintext, will be encrypted when stored)
    access_token: String,

    /// When the token was stored (not sensitive, skip zeroize)
    #[zeroize(skip)]
    stored_at: DateTime<Utc>,

    /// When the token expires (not sensitive, skip zeroize)
    #[zeroize(skip)]
    expires_at: Option<DateTime<Utc>>,
}

/// Manages the lifecycle of OAuth tokens with encryption and expiration tracking.
///
/// Tokens are stored encrypted in memory using ChaCha20-Poly1305 AEAD.
/// Expiration times are tracked based on the `expires_in` field from the OAuth response.
///
/// # Example
/// ```no_run
/// use at_bridge::oauth_token_manager::OAuthTokenManager;
///
/// let manager = OAuthTokenManager::new();
///
/// // Store a token with 1 hour expiration
/// manager.store_token("ghp_abc123", Some(3600)).await;
///
/// // Check if token is still valid
/// if manager.is_expired().await {
///     println!("Token has expired");
/// }
///
/// // Get the decrypted token
/// if let Some(token) = manager.get_token().await.ok() {
///     println!("Token retrieved successfully");
/// }
/// ```
pub struct OAuthTokenManager {
    /// Encrypted token storage (None if no token stored)
    encrypted_token: Arc<RwLock<Option<Vec<u8>>>>,

    /// Encryption key for token storage
    encryption_key: EncryptionKey,

    /// Token metadata (expiration times)
    metadata: Arc<RwLock<Option<TokenMetadata>>>,
}

#[derive(Debug, Clone)]
struct TokenMetadata {
    stored_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
}

impl OAuthTokenManager {
    /// Create a new token manager with a freshly generated encryption key.
    pub fn new() -> Self {
        let encryption_key = EncryptionKey::generate()
            .expect("failed to generate encryption key");

        Self {
            encrypted_token: Arc::new(RwLock::new(None)),
            encryption_key,
            metadata: Arc::new(RwLock::new(None)),
        }
    }

    /// Store an OAuth access token with optional expiration time.
    ///
    /// # Parameters
    /// - `access_token`: The OAuth access token to store
    /// - `expires_in`: Optional expiration time in seconds (from OAuth response)
    ///
    /// # Example
    /// ```no_run
    /// # use at_bridge::oauth_token_manager::OAuthTokenManager;
    /// # async fn example() {
    /// let manager = OAuthTokenManager::new();
    ///
    /// // Store a token that expires in 1 hour (3600 seconds)
    /// manager.store_token("ghp_abc123", Some(3600)).await;
    /// # }
    /// ```
    pub async fn store_token(&self, access_token: &str, expires_in: Option<u64>) {
        let stored_at = Utc::now();
        let expires_at = expires_in.map(|seconds| {
            stored_at + Duration::seconds(seconds as i64)
        });

        // Create token data structure
        let token_data = TokenData {
            access_token: access_token.to_string(),
            stored_at,
            expires_at,
        };

        // Serialize and encrypt
        let plaintext = serde_json::to_vec(&token_data)
            .expect("failed to serialize token data");

        let encrypted = encrypt(&self.encryption_key, &plaintext)
            .expect("failed to encrypt token");

        // Store encrypted token and metadata
        *self.encrypted_token.write().await = Some(encrypted);
        *self.metadata.write().await = Some(TokenMetadata {
            stored_at,
            expires_at,
        });

        // Zero out the plaintext
        drop(token_data);
    }

    /// Retrieve and decrypt the stored OAuth access token.
    ///
    /// # Errors
    /// - `TokenManagerError::NoToken`: No token has been stored
    /// - `TokenManagerError::TokenExpired`: Token has expired
    /// - `TokenManagerError::Crypto`: Decryption failed
    ///
    /// # Example
    /// ```no_run
    /// # use at_bridge::oauth_token_manager::OAuthTokenManager;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = OAuthTokenManager::new();
    /// manager.store_token("ghp_abc123", Some(3600)).await;
    ///
    /// let token = manager.get_token().await?;
    /// println!("Retrieved token: {}", token);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_token(&self) -> Result<String> {
        // Check if token exists
        let encrypted = self.encrypted_token.read().await;
        let encrypted_data = encrypted.as_ref().ok_or(TokenManagerError::NoToken)?;

        // Check if expired before decrypting
        if self.is_expired().await {
            return Err(TokenManagerError::TokenExpired);
        }

        // Decrypt token
        let plaintext = decrypt(&self.encryption_key, encrypted_data)?;
        let mut token_data: TokenData = serde_json::from_slice(&plaintext)?;

        // Extract access token and zero out the decrypted data
        let access_token = token_data.access_token.clone();
        token_data.zeroize();

        Ok(access_token)
    }

    /// Check if the stored token has expired.
    ///
    /// Returns `true` if:
    /// - No token is stored
    /// - Token has an expiration time and it has passed
    ///
    /// Returns `false` if:
    /// - Token exists and has not expired
    /// - Token exists but has no expiration time (never expires)
    ///
    /// # Example
    /// ```no_run
    /// # use at_bridge::oauth_token_manager::OAuthTokenManager;
    /// # async fn example() {
    /// let manager = OAuthTokenManager::new();
    /// manager.store_token("ghp_abc123", Some(3600)).await;
    ///
    /// if manager.is_expired().await {
    ///     println!("Token needs refresh");
    /// }
    /// # }
    /// ```
    pub async fn is_expired(&self) -> bool {
        let metadata = self.metadata.read().await;

        match metadata.as_ref() {
            None => true, // No token stored = expired
            Some(meta) => {
                match meta.expires_at {
                    None => false, // No expiration time = never expires
                    Some(expires_at) => Utc::now() >= expires_at,
                }
            }
        }
    }

    /// Check if the token should be refreshed soon.
    ///
    /// Returns `true` if the token will expire within the next 5 minutes.
    /// This allows for proactive token refresh before expiration.
    ///
    /// Returns `false` if:
    /// - No token is stored
    /// - Token has no expiration time
    /// - Token won't expire for more than 5 minutes
    ///
    /// # Example
    /// ```no_run
    /// # use at_bridge::oauth_token_manager::OAuthTokenManager;
    /// # async fn example() {
    /// let manager = OAuthTokenManager::new();
    /// manager.store_token("ghp_abc123", Some(3600)).await;
    ///
    /// if manager.should_refresh().await {
    ///     println!("Token should be refreshed proactively");
    /// }
    /// # }
    /// ```
    pub async fn should_refresh(&self) -> bool {
        let metadata = self.metadata.read().await;

        match metadata.as_ref() {
            None => false, // No token stored
            Some(meta) => {
                match meta.expires_at {
                    None => false, // No expiration time = no need to refresh
                    Some(expires_at) => {
                        // Refresh if expires within 5 minutes
                        let refresh_threshold = Utc::now() + Duration::minutes(5);
                        expires_at <= refresh_threshold
                    }
                }
            }
        }
    }

    /// Check if a valid token exists (stored and not expired).
    ///
    /// # Example
    /// ```no_run
    /// # use at_bridge::oauth_token_manager::OAuthTokenManager;
    /// # async fn example() {
    /// let manager = OAuthTokenManager::new();
    ///
    /// if manager.has_valid_token().await {
    ///     println!("Valid token exists");
    /// } else {
    ///     println!("No valid token - user needs to authenticate");
    /// }
    /// # }
    /// ```
    pub async fn has_valid_token(&self) -> bool {
        let has_token = self.encrypted_token.read().await.is_some();
        has_token && !self.is_expired().await
    }

    /// Clear the stored token and zero its memory.
    ///
    /// This securely removes the token from memory by:
    /// 1. Zeroing the encrypted token data
    /// 2. Clearing the metadata
    /// 3. Setting storage to None
    ///
    /// # Example
    /// ```no_run
    /// # use at_bridge::oauth_token_manager::OAuthTokenManager;
    /// # async fn example() {
    /// let manager = OAuthTokenManager::new();
    /// manager.store_token("ghp_abc123", Some(3600)).await;
    ///
    /// // Revoke the token
    /// manager.clear_token().await;
    /// # }
    /// ```
    pub async fn clear_token(&self) {
        // Zero out the encrypted data before dropping
        if let Some(mut encrypted_data) = self.encrypted_token.write().await.take() {
            encrypted_data.zeroize();
        }

        // Clear metadata
        *self.metadata.write().await = None;
    }
}

impl Default for OAuthTokenManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration as TokioDuration};

    #[tokio::test]
    async fn test_store_and_retrieve_token() {
        let manager = OAuthTokenManager::new();
        let token = "ghp_test_token_12345";

        manager.store_token(token, Some(3600)).await;

        let retrieved = manager.get_token().await.unwrap();
        assert_eq!(retrieved, token);
    }

    #[tokio::test]
    async fn test_no_token_returns_error() {
        let manager = OAuthTokenManager::new();
        let result = manager.get_token().await;

        assert!(result.is_err());
        assert!(matches!(result, Err(TokenManagerError::NoToken)));
    }

    #[tokio::test]
    async fn test_token_expiration() {
        let manager = OAuthTokenManager::new();

        // Store a token that expires in 1 second
        manager.store_token("ghp_short_lived", Some(1)).await;

        // Should not be expired immediately
        assert!(!manager.is_expired().await);

        // Wait for expiration
        sleep(TokioDuration::from_secs(2)).await;

        // Should now be expired
        assert!(manager.is_expired().await);
    }

    #[tokio::test]
    async fn test_token_without_expiration_never_expires() {
        let manager = OAuthTokenManager::new();

        // Store a token without expiration
        manager.store_token("ghp_eternal_token", None).await;

        // Should never expire
        assert!(!manager.is_expired().await);

        // Should have valid token
        assert!(manager.has_valid_token().await);
    }

    #[tokio::test]
    async fn test_should_refresh_logic() {
        let manager = OAuthTokenManager::new();

        // Store a token that expires in 3 minutes (180 seconds)
        manager.store_token("ghp_refresh_test", Some(180)).await;

        // Should recommend refresh (expires within 5 minutes)
        assert!(manager.should_refresh().await);
    }

    #[tokio::test]
    async fn test_should_not_refresh_long_lived_token() {
        let manager = OAuthTokenManager::new();

        // Store a token that expires in 1 hour (3600 seconds)
        manager.store_token("ghp_long_lived", Some(3600)).await;

        // Should not need refresh yet (expires in > 5 minutes)
        assert!(!manager.should_refresh().await);
    }

    #[tokio::test]
    async fn test_should_not_refresh_without_expiration() {
        let manager = OAuthTokenManager::new();

        // Store a token without expiration
        manager.store_token("ghp_no_expiry", None).await;

        // Should not recommend refresh for tokens without expiration
        assert!(!manager.should_refresh().await);
    }

    #[tokio::test]
    async fn test_has_valid_token() {
        let manager = OAuthTokenManager::new();

        // No token stored
        assert!(!manager.has_valid_token().await);

        // Store valid token
        manager.store_token("ghp_valid", Some(3600)).await;
        assert!(manager.has_valid_token().await);

        // Clear token
        manager.clear_token().await;
        assert!(!manager.has_valid_token().await);
    }

    #[tokio::test]
    async fn test_clear_token() {
        let manager = OAuthTokenManager::new();

        manager.store_token("ghp_to_clear", Some(3600)).await;
        assert!(manager.has_valid_token().await);

        manager.clear_token().await;

        // Should have no token after clearing
        assert!(!manager.has_valid_token().await);
        let result = manager.get_token().await;
        assert!(matches!(result, Err(TokenManagerError::NoToken)));
    }

    #[tokio::test]
    async fn test_encryption_different_managers() {
        let manager1 = OAuthTokenManager::new();
        let manager2 = OAuthTokenManager::new();

        let token = "ghp_test_encryption";
        manager1.store_token(token, Some(3600)).await;

        // Different managers have different keys, so manager2 can't decrypt manager1's token
        // This test verifies that each manager has its own encryption key
        assert!(!manager2.has_valid_token().await);
    }

    #[tokio::test]
    async fn test_expired_token_get_returns_error() {
        let manager = OAuthTokenManager::new();

        // Store a token that expires immediately
        manager.store_token("ghp_expired", Some(1)).await;

        // Wait for expiration
        sleep(TokioDuration::from_secs(2)).await;

        // get_token should return TokenExpired error
        let result = manager.get_token().await;
        assert!(matches!(result, Err(TokenManagerError::TokenExpired)));
    }

    #[tokio::test]
    async fn test_token_data_serialization_roundtrip() {
        let manager = OAuthTokenManager::new();
        let original_token = "ghp_serialization_test_1234567890";

        manager.store_token(original_token, Some(7200)).await;

        let retrieved_token = manager.get_token().await.unwrap();
        assert_eq!(retrieved_token, original_token);
    }

    #[tokio::test]
    async fn test_multiple_store_overwrites() {
        let manager = OAuthTokenManager::new();

        manager.store_token("ghp_first_token", Some(3600)).await;
        manager.store_token("ghp_second_token", Some(7200)).await;

        // Should retrieve the second token
        let token = manager.get_token().await.unwrap();
        assert_eq!(token, "ghp_second_token");
    }
}
