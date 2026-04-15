use at_bridge::event_bus::EventBus;
use at_bridge::protocol::BridgeMessage;
use at_core::types::BeadStatus;
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
// Event Bus Listener
// ---------------------------------------------------------------------------

/// Subscribe to the event bus and send notifications when beads reach
/// terminal states (Done, Failed, Review).
///
/// This function spawns a background task that listens for `BeadUpdated`
/// messages and triggers OS notifications for status changes.
///
/// # Arguments
///
/// * `app` - Tauri application handle
/// * `event_bus` - Event bus to subscribe to
///
/// # Example
///
/// ```no_run
/// use at_tauri::notifications::start_notification_listener;
///
/// # tauri::async_runtime::block_on(async {
/// # let app = tauri::test::mock_app();
/// # let event_bus = at_bridge::event_bus::EventBus::new();
/// start_notification_listener(&app, &event_bus);
/// # });
/// ```
pub fn start_notification_listener(app: &AppHandle, event_bus: &EventBus) {
    let rx = event_bus.subscribe_filtered(|msg| matches!(msg, BridgeMessage::BeadUpdated(_)));
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv_async().await {
                Ok(msg) => {
                    if let BridgeMessage::BeadUpdated(bead) = msg.as_ref() {
                        // Send notifications for terminal/review states.
                        let (title, body, level) = match &bead.status {
                            BeadStatus::Done => (
                                "Bead Completed",
                                format!("✓ {}", bead.title),
                                NotificationLevel::Success,
                            ),
                            BeadStatus::Failed => (
                                "Bead Failed",
                                format!("✗ {}", bead.title),
                                NotificationLevel::Error,
                            ),
                            BeadStatus::Review => (
                                "Bead Ready for Review",
                                format!("⊙ {}", bead.title),
                                NotificationLevel::Info,
                            ),
                            _ => continue,
                        };

                        if let Err(e) = send_notification(&app_handle, title, body, level).await {
                            tracing::warn!(error = %e, "failed to send bead notification");
                        }
                    }
                }
                Err(_) => {
                    tracing::debug!("notification listener event bus disconnected");
                    break;
                }
            }
        }
    });

    tracing::info!("notification listener started");
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
