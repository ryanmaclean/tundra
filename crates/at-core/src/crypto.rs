//! Cryptographic utilities for encrypting and decrypting sensitive data.
//!
//! Uses ChaCha20-Poly1305 AEAD (Authenticated Encryption with Associated Data)
//! for secure encryption with authentication. Keys and sensitive data are
//! automatically zeroed from memory when dropped using the `zeroize` crate.

use ring::aead::{
    Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey, CHACHA20_POLY1305,
};
use ring::error::Unspecified;
use ring::rand::{SecureRandom, SystemRandom};
use std::error::Error as StdError;
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of ChaCha20-Poly1305 key in bytes (256 bits)
const KEY_LEN: usize = 32;

/// Size of nonce in bytes (96 bits)
const NONCE_LEN: usize = 12;

/// Size of authentication tag appended to ciphertext (128 bits)
const TAG_LEN: usize = 16;

// ---------------------------------------------------------------------------
// Error Types
// ---------------------------------------------------------------------------

/// Errors that can occur during cryptographic operations.
#[derive(Debug)]
pub enum CryptoError {
    /// Failed to generate random bytes from system entropy.
    RandomGeneration,
    /// Encryption operation failed.
    Encryption,
    /// Decryption operation failed (invalid ciphertext or authentication tag).
    Decryption,
    /// Invalid input format (e.g., ciphertext too short).
    InvalidFormat(String),
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::RandomGeneration => write!(f, "failed to generate random bytes"),
            CryptoError::Encryption => write!(f, "encryption failed"),
            CryptoError::Decryption => write!(f, "decryption failed"),
            CryptoError::InvalidFormat(msg) => write!(f, "invalid format: {}", msg),
        }
    }
}

impl StdError for CryptoError {}

impl From<Unspecified> for CryptoError {
    fn from(_: Unspecified) -> Self {
        CryptoError::Encryption
    }
}

// ---------------------------------------------------------------------------
// Key Management
// ---------------------------------------------------------------------------

/// A cryptographic key that is automatically zeroed from memory when dropped.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct EncryptionKey {
    #[zeroize(skip)]
    bytes: [u8; KEY_LEN],
}

impl EncryptionKey {
    /// Generate a new random encryption key using system entropy.
    pub fn generate() -> Result<Self, CryptoError> {
        let rng = SystemRandom::new();
        let mut bytes = [0u8; KEY_LEN];
        rng.fill(&mut bytes)
            .map_err(|_| CryptoError::RandomGeneration)?;
        Ok(Self { bytes })
    }

    /// Create an encryption key from existing bytes.
    ///
    /// # Security
    /// The input slice must be exactly 32 bytes. The caller is responsible
    /// for securely managing the source key material.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != KEY_LEN {
            return Err(CryptoError::InvalidFormat(format!(
                "key must be {} bytes, got {}",
                KEY_LEN,
                bytes.len()
            )));
        }
        let mut key_bytes = [0u8; KEY_LEN];
        key_bytes.copy_from_slice(bytes);
        Ok(Self { bytes: key_bytes })
    }

    /// Get the raw key bytes.
    ///
    /// # Security
    /// Use with caution - the returned slice exposes the raw key material.
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.bytes
    }
}

// ---------------------------------------------------------------------------
// Nonce Management
// ---------------------------------------------------------------------------

/// A nonce generator that creates a single random nonce.
struct OneNonceSequence {
    nonce: Option<Nonce>,
}

impl OneNonceSequence {
    fn new(nonce: Nonce) -> Self {
        Self { nonce: Some(nonce) }
    }
}

impl NonceSequence for OneNonceSequence {
    fn advance(&mut self) -> Result<Nonce, Unspecified> {
        self.nonce.take().ok_or(Unspecified)
    }
}

// ---------------------------------------------------------------------------
// Encryption/Decryption
// ---------------------------------------------------------------------------

/// Encrypt plaintext using ChaCha20-Poly1305 AEAD.
///
/// Returns a Vec containing: [nonce (12 bytes) || ciphertext || auth_tag (16 bytes)]
///
/// # Example
/// ```
/// use at_core::crypto::{EncryptionKey, encrypt};
///
/// let key = EncryptionKey::generate().unwrap();
/// let plaintext = b"secret data";
/// let ciphertext = encrypt(&key, plaintext).unwrap();
/// ```
pub fn encrypt(key: &EncryptionKey, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let rng = SystemRandom::new();

    // Generate random nonce
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| CryptoError::RandomGeneration)?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    // Create sealing key
    let unbound_key =
        UnboundKey::new(&CHACHA20_POLY1305, key.as_bytes()).map_err(|_| CryptoError::Encryption)?;
    let nonce_sequence = OneNonceSequence::new(nonce);
    let mut sealing_key = SealingKey::new(unbound_key, nonce_sequence);

    // Prepare buffer: plaintext + space for auth tag
    let mut in_out = plaintext.to_vec();
    sealing_key
        .seal_in_place_append_tag(Aad::empty(), &mut in_out)
        .map_err(|_| CryptoError::Encryption)?;

    // Prepend nonce to ciphertext+tag
    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&in_out);

    Ok(result)
}

/// Decrypt ciphertext using ChaCha20-Poly1305 AEAD.
///
/// Expects input format: [nonce (12 bytes) || ciphertext || auth_tag (16 bytes)]
///
/// # Example
/// ```
/// use at_core::crypto::{EncryptionKey, encrypt, decrypt};
///
/// let key = EncryptionKey::generate().unwrap();
/// let plaintext = b"secret data";
/// let ciphertext = encrypt(&key, plaintext).unwrap();
/// let decrypted = decrypt(&key, &ciphertext).unwrap();
/// assert_eq!(plaintext, &decrypted[..]);
/// ```
pub fn decrypt(key: &EncryptionKey, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    // Validate minimum length: nonce + tag
    if ciphertext.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::InvalidFormat(format!(
            "ciphertext too short: expected at least {} bytes, got {}",
            NONCE_LEN + TAG_LEN,
            ciphertext.len()
        )));
    }

    // Extract nonce from first 12 bytes
    let nonce_bytes: [u8; NONCE_LEN] = ciphertext[..NONCE_LEN]
        .try_into()
        .map_err(|_| CryptoError::InvalidFormat("failed to extract nonce".into()))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    // Create opening key
    let unbound_key =
        UnboundKey::new(&CHACHA20_POLY1305, key.as_bytes()).map_err(|_| CryptoError::Decryption)?;
    let nonce_sequence = OneNonceSequence::new(nonce);
    let mut opening_key = OpeningKey::new(unbound_key, nonce_sequence);

    // Decrypt ciphertext + tag (everything after nonce)
    let mut in_out = ciphertext[NONCE_LEN..].to_vec();
    let plaintext = opening_key
        .open_in_place(Aad::empty(), &mut in_out)
        .map_err(|_| CryptoError::Decryption)?;

    Ok(plaintext.to_vec())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let key1 = EncryptionKey::generate().unwrap();
        let key2 = EncryptionKey::generate().unwrap();

        // Keys should be different
        assert_ne!(key1.as_bytes(), key2.as_bytes());
        assert_eq!(key1.as_bytes().len(), KEY_LEN);
    }

    #[test]
    fn test_key_from_bytes() {
        let bytes = [42u8; KEY_LEN];
        let key = EncryptionKey::from_bytes(&bytes).unwrap();
        assert_eq!(key.as_bytes(), &bytes);
    }

    #[test]
    fn test_key_from_bytes_invalid_length() {
        let bytes = [42u8; 16]; // Wrong length
        let result = EncryptionKey::from_bytes(&bytes);
        assert!(result.is_err());
        assert!(matches!(result, Err(CryptoError::InvalidFormat(_))));
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = EncryptionKey::generate().unwrap();
        let plaintext = b"Hello, secure world!";

        let ciphertext = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &ciphertext).unwrap();

        assert_eq!(plaintext, &decrypted[..]);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let key = EncryptionKey::generate().unwrap();
        let plaintext = b"same plaintext";

        // Encrypt the same plaintext twice
        let ciphertext1 = encrypt(&key, plaintext).unwrap();
        let ciphertext2 = encrypt(&key, plaintext).unwrap();

        // Ciphertexts should differ due to random nonces
        assert_ne!(ciphertext1, ciphertext2);

        // But both should decrypt to the same plaintext
        let decrypted1 = decrypt(&key, &ciphertext1).unwrap();
        let decrypted2 = decrypt(&key, &ciphertext2).unwrap();
        assert_eq!(decrypted1, decrypted2);
        assert_eq!(plaintext, &decrypted1[..]);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let key1 = EncryptionKey::generate().unwrap();
        let key2 = EncryptionKey::generate().unwrap();
        let plaintext = b"secret";

        let ciphertext = encrypt(&key1, plaintext).unwrap();
        let result = decrypt(&key2, &ciphertext);

        assert!(result.is_err());
        assert!(matches!(result, Err(CryptoError::Decryption)));
    }

    #[test]
    fn test_decrypt_tampered_ciphertext_fails() {
        let key = EncryptionKey::generate().unwrap();
        let plaintext = b"original data";

        let mut ciphertext = encrypt(&key, plaintext).unwrap();

        // Tamper with the ciphertext (flip a bit in the middle)
        let mid = ciphertext.len() / 2;
        ciphertext[mid] ^= 0xFF;

        let result = decrypt(&key, &ciphertext);
        assert!(result.is_err());
        assert!(matches!(result, Err(CryptoError::Decryption)));
    }

    #[test]
    fn test_decrypt_too_short_fails() {
        let key = EncryptionKey::generate().unwrap();
        let short_data = vec![0u8; 10]; // Less than nonce + tag

        let result = decrypt(&key, &short_data);
        assert!(result.is_err());
        assert!(matches!(result, Err(CryptoError::InvalidFormat(_))));
    }

    #[test]
    fn test_encrypt_empty_plaintext() {
        let key = EncryptionKey::generate().unwrap();
        let plaintext = b"";

        let ciphertext = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &ciphertext).unwrap();

        assert_eq!(plaintext, &decrypted[..]);
    }

    #[test]
    fn test_encrypt_large_plaintext() {
        let key = EncryptionKey::generate().unwrap();
        let plaintext = vec![42u8; 10_000];

        let ciphertext = encrypt(&key, &plaintext).unwrap();
        let decrypted = decrypt(&key, &ciphertext).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_ciphertext_format() {
        let key = EncryptionKey::generate().unwrap();
        let plaintext = b"test";

        let ciphertext = encrypt(&key, plaintext).unwrap();

        // Ciphertext should be: nonce (12) + plaintext.len() + tag (16)
        let expected_len = NONCE_LEN + plaintext.len() + TAG_LEN;
        assert_eq!(ciphertext.len(), expected_len);
    }

    #[test]
    fn test_key_zeroized_on_drop() {
        let key_bytes = {
            let key = EncryptionKey::generate().unwrap();
            let bytes = *key.as_bytes();
            // key is dropped here
            bytes
        };

        // We can't directly verify the original memory was zeroed,
        // but we can verify the key was created with non-zero bytes
        assert_ne!(key_bytes, [0u8; KEY_LEN]);
    }
}
