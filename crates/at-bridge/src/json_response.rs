use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tokio::sync::RwLockReadGuard;

/// A wrapper that serializes directly from `RwLockReadGuard<Vec<T>>` without cloning the collection.
///
/// This is used to optimize list endpoints that return large collections. Instead of cloning
/// the Vec to pass ownership to Axum's Json wrapper, we serialize the data in-place while
/// holding the read lock. This eliminates all allocation overhead while maintaining the same
/// JSON response format.
///
/// The serialization happens immediately when constructing this type, so the read lock is
/// released before the response is returned.
///
/// # Example
///
/// ```ignore
/// async fn list_items(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
///     let guard = state.items.read().await;
///     JsonFromReadGuard::from_guard(guard)
/// }
/// ```
pub struct JsonFromReadGuard {
    bytes: Result<Vec<u8>, String>,
}

impl JsonFromReadGuard {
    /// Creates a new JsonFromReadGuard by serializing immediately from the guard.
    /// The guard is dropped after serialization, releasing the read lock.
    pub fn from_guard<T>(guard: RwLockReadGuard<'_, Vec<T>>) -> Self
    where
        T: Serialize,
    {
        let bytes = serde_json::to_vec(&*guard)
            .map_err(|err| format!("Failed to serialize response: {}", err));
        // Guard is dropped here, releasing the read lock
        Self { bytes }
    }
}

impl IntoResponse for JsonFromReadGuard {
    fn into_response(self) -> Response {
        match self.bytes {
            Ok(bytes) => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                bytes,
            )
                .into_response(),
            Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
        }
    }
}
