use std::fmt;

/// Errors returned by Tauri command handlers.
#[derive(Debug)]
pub enum TauriError {
    /// An error originating from the cache / database layer.
    CacheError(String),
    /// An error originating from the bridge / event-bus layer.
    BridgeError(String),
    /// A requested resource was not found.
    NotFound(String),
    /// The application state is invalid for the requested operation.
    InvalidState(String),
}

impl fmt::Display for TauriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TauriError::CacheError(msg) => write!(f, "cache error: {msg}"),
            TauriError::BridgeError(msg) => write!(f, "bridge error: {msg}"),
            TauriError::NotFound(msg) => write!(f, "not found: {msg}"),
            TauriError::InvalidState(msg) => write!(f, "invalid state: {msg}"),
        }
    }
}

impl std::error::Error for TauriError {}

impl From<anyhow::Error> for TauriError {
    fn from(err: anyhow::Error) -> Self {
        TauriError::CacheError(err.to_string())
    }
}

impl From<tokio_rusqlite::Error> for TauriError {
    fn from(err: tokio_rusqlite::Error) -> Self {
        TauriError::CacheError(err.to_string())
    }
}
