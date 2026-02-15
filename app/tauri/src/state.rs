use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_core::cache::CacheDb;

use crate::error::TauriError;

/// Shared application state passed to every Tauri command handler.
pub struct AppState {
    pub cache: Arc<CacheDb>,
    pub event_bus: Arc<EventBus>,
}

impl AppState {
    /// Create a new `AppState` backed by a SQLite file at `cache_path`.
    pub async fn new(cache_path: &str) -> Result<Self, TauriError> {
        let cache = CacheDb::new(cache_path)
            .await
            .map_err(|e| TauriError::CacheError(e.to_string()))?;
        Ok(Self {
            cache: Arc::new(cache),
            event_bus: Arc::new(EventBus::new()),
        })
    }

    /// Create an `AppState` with an in-memory cache (useful for testing).
    pub async fn with_in_memory_cache() -> Self {
        let cache = CacheDb::new_in_memory()
            .await
            .expect("failed to create in-memory cache");
        Self {
            cache: Arc::new(cache),
            event_bus: Arc::new(EventBus::new()),
        }
    }
}
