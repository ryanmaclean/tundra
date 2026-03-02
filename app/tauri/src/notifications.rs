use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Notification levels matching at-bridge::notifications::NotificationLevel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// Send a native OS notification using Tauri's notification API.
///
/// # Arguments
///
/// * `app` - Tauri application handle
/// * `title` - Notification title
/// * `body` - Notification message body
/// * `level` - Notification level (used for logging, not displayed in OS notification)
///
/// # Returns
///
/// Returns `Ok(())` if notification was sent successfully, or an error if it failed.
///
/// # Example
///
/// ```no_run
/// use at_tauri::notifications::{send_notification, NotificationLevel};
///
/// # tauri::async_runtime::block_on(async {
/// # let app = tauri::test::mock_app();
/// send_notification(
///     &app,
///     "Task Completed",
///     "Your bead processing has finished",
///     NotificationLevel::Success
/// ).await.ok();
/// # });
/// ```
pub async fn send_notification(
    app: &AppHandle,
    title: impl Into<String>,
    body: impl Into<String>,
    level: NotificationLevel,
) -> Result<(), NotificationError> {
    let title = title.into();
    let body = body.into();

    tracing::debug!(
        level = ?level,
        title = %title,
        "sending native notification"
    );

    // Build and send the notification using Tauri's notification API
    #[cfg(not(test))]
    {
        app.notification()
            .builder()
            .title(&title)
            .body(&body)
            .show()
            .map_err(|e| NotificationError::TauriError(e.to_string()))?;
    }

    #[cfg(test)]
    {
        // In tests, just log the notification
        let _ = app;
        tracing::info!(
            level = ?level,
            title = %title,
            body = %body,
            "test notification (not sent to OS)"
        );
    }

    Ok(())
}

/// Send a notification with default Info level.
pub async fn send_info(
    app: &AppHandle,
    title: impl Into<String>,
    body: impl Into<String>,
) -> Result<(), NotificationError> {
    send_notification(app, title, body, NotificationLevel::Info).await
}

/// Send a notification with Success level.
pub async fn send_success(
    app: &AppHandle,
    title: impl Into<String>,
    body: impl Into<String>,
) -> Result<(), NotificationError> {
    send_notification(app, title, body, NotificationLevel::Success).await
}

/// Send a notification with Warning level.
pub async fn send_warning(
    app: &AppHandle,
    title: impl Into<String>,
    body: impl Into<String>,
) -> Result<(), NotificationError> {
    send_notification(app, title, body, NotificationLevel::Warning).await
}

/// Send a notification with Error level.
pub async fn send_error(
    app: &AppHandle,
    title: impl Into<String>,
    body: impl Into<String>,
) -> Result<(), NotificationError> {
    send_notification(app, title, body, NotificationLevel::Error).await
}

/// Errors that can occur when sending notifications.
#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("tauri notification error: {0}")]
    TauriError(String),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_send_notification() {
        let app = tauri::test::mock_app();
        let result = send_notification(
            &app,
            "Test Title",
            "Test Body",
            NotificationLevel::Info,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_info() {
        let app = tauri::test::mock_app();
        let result = send_info(&app, "Info Title", "Info Body").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_success() {
        let app = tauri::test::mock_app();
        let result = send_success(&app, "Success Title", "Success Body").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_warning() {
        let app = tauri::test::mock_app();
        let result = send_warning(&app, "Warning Title", "Warning Body").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_error() {
        let app = tauri::test::mock_app();
        let result = send_error(&app, "Error Title", "Error Body").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_notification_level_serialization() {
        let level = NotificationLevel::Success;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, r#""success""#);

        let deserialized: NotificationLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, NotificationLevel::Success);
    }
}
