use serde::Serialize;
use std::fmt;

/// Errors returned by Tauri command handlers.
#[derive(Debug, Serialize)]
pub enum TauriError {
    /// Generic startup or runtime error.
    StartupError(String),
}

impl fmt::Display for TauriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TauriError::StartupError(msg) => write!(f, "startup error: {msg}"),
        }
    }
}

impl std::error::Error for TauriError {}

impl From<anyhow::Error> for TauriError {
    fn from(err: anyhow::Error) -> Self {
        TauriError::StartupError(err.to_string())
    }
}
