use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tokio::sync::RwLockReadGuard;

/// A wrapper around `RwLockReadGuard<Vec<T>>` that implements `IntoResponse` by serializing
/// directly from the guard without cloning the entire collection.
///
/// This is used to optimize list endpoints that return large collections. Instead of cloning
/// the Vec to pass ownership to Axum's Json wrapper, we serialize the data in-place while
/// holding the read lock. This eliminates all allocation overhead while maintaining the same
/// JSON response format.
///
/// # Example
///
/// ```ignore
/// async fn list_items(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
///     let items = state.items.read().await;
///     JsonFromReadGuard(items)
/// }
/// ```
pub struct JsonFromReadGuard<'a, T>(pub RwLockReadGuard<'a, Vec<T>>);

impl<'a, T> IntoResponse for JsonFromReadGuard<'a, T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        // Serialize the Vec<T> directly from the read guard by dereferencing to &Vec<T>
        match serde_json::to_vec(&*self.0) {
            Ok(bytes) => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                bytes,
            )
                .into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to serialize response: {}", err),
            )
                .into_response(),
        }
    }
}
