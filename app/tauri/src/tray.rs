//! System tray integration for the Auto-Tundra desktop app.
//!
//! Provides a persistent system tray icon with a context menu for quick
//! access to core features:
//! - Status indicator (shows app state)
//! - New Task action (opens the existing task wizard)
//! - Quit action (gracefully exits the app)
//!
//! # Architecture
//!
//! ```text
//! System Tray Icon
//!     ├─ Status (label, non-clickable)
//!     ├─ New Task (opens main window + task wizard)
//!     ├─ Separator
//!     └─ Quit (exits app)
//! ```
//!
//! The tray is initialized once during app startup and persists until the
//! app is closed. Menu events are handled via the Tauri event system.

use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tracing::{error, info};
use std::sync::Mutex;

// Re-export image crate for icon loading.
use image;

/// Menu item identifiers for tray actions.
mod menu_ids {
    pub const STATUS: &str = "status";
    pub const NEW_TASK: &str = "new_task";
    pub const QUIT: &str = "quit";
}

#[derive(Default)]
pub struct PendingNewTask(Mutex<PendingNewTaskState>);

#[derive(Default)]
struct PendingNewTaskState {
    next_id: u32,
    pending: Option<u32>,
}

impl PendingNewTask {
    fn request(&self) -> Result<u32, String> {
        let mut state = self.0.lock().map_err(|_| "tray intent lock poisoned")?;
        if let Some(id) = state.pending {
            return Ok(id);
        }
        state.next_id = state.next_id.checked_add(1).ok_or("tray intent IDs exhausted")?;
        state.pending = Some(state.next_id);
        Ok(state.next_id)
    }

    fn pending(&self) -> Result<Option<u32>, String> {
        let state = self.0.lock().map_err(|_| "tray intent lock poisoned")?;
        Ok(state.pending)
    }

    fn claim(&self, id: u32) -> Result<bool, String> {
        let mut state = self.0.lock().map_err(|_| "tray intent lock poisoned")?;
        if state.pending == Some(id) {
            state.pending = None;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod pending_intent_tests {
    use super::PendingNewTask;

    #[test]
    fn coalesces_unclaimed_clicks_and_rejects_stale_claim() {
        let intents = PendingNewTask::default();
        let first = intents.request().unwrap();
        assert_eq!(intents.request().unwrap(), first);
        assert_eq!(intents.pending().unwrap(), Some(first));
        assert!(!intents.claim(first + 1).unwrap());
        assert_eq!(intents.pending().unwrap(), Some(first));
        assert!(intents.claim(first).unwrap());
        assert_eq!(intents.pending().unwrap(), None);
        assert!(!intents.claim(first).unwrap());
        assert!(intents.request().unwrap() > first);
    }
}

/// Called only after the webview has successfully installed its event listener.
#[tauri::command]
pub fn cmd_pending_tray_new_task(
    state: tauri::State<'_, PendingNewTask>,
) -> Result<Option<u32>, String> {
    state.pending()
}

#[tauri::command]
pub fn cmd_claim_tray_new_task(
    state: tauri::State<'_, PendingNewTask>,
    id: u32,
) -> Result<bool, String> {
    state.claim(id)
}

/// Initialize the system tray with icon and menu.
///
/// # Returns
/// - `Ok(())` if tray was successfully created
/// - `Err(e)` if tray creation failed (e.g., icon not found, platform doesn't support tray)
///
/// # Errors
/// This function will return an error if:
/// - The tray icon file cannot be loaded
/// - The platform doesn't support system tray icons
/// - Menu creation fails
pub fn init_tray(app: &AppHandle<tauri::Wry>) -> Result<(), Box<dyn std::error::Error>> {
    info!("initializing system tray");

    // Load the tray icon from the bundled assets.
    let icon = load_tray_icon()?;

    // Build the context menu.
    let menu = build_tray_menu(app)?;

    // Create the tray icon with the menu attached.
    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false) // Right-click shows menu (standard behavior)
        .on_tray_icon_event(|tray, event| {
            // Handle tray icon events (click, double-click, etc.)
            handle_tray_event(tray, event);
        })
        .on_menu_event(|app, event| {
            // Handle menu item clicks.
            handle_menu_event(app, event.id().as_ref());
        })
        .build(app)?;

    info!("system tray initialized");
    Ok(())
}

/// Load the tray icon from bundled assets.
///
/// Uses the same icon as the app window icon for consistency.
fn load_tray_icon() -> Result<Image<'static>, Box<dyn std::error::Error>> {
    // Load the icon bytes from the bundled assets.
    // The icon.png is bundled via tauri.conf.json.
    let icon_bytes = include_bytes!("../icons/icon.png");

    // Decode the PNG image to get RGBA data.
    let img = image::load_from_memory(icon_bytes)?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let rgba_vec = rgba.into_raw();

    Ok(Image::new_owned(rgba_vec, width, height))
}

/// Build the tray context menu.
///
/// Menu structure:
/// - Status (disabled label showing current state)
/// - New Task (opens main window + creates bead)
/// - ---
/// - Quit
fn build_tray_menu(
    app: &AppHandle<tauri::Wry>,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    // Status item (disabled, acts as a label).
    let status_item = MenuItemBuilder::with_id(menu_ids::STATUS, "Auto-Tundra Running")
        .enabled(false)
        .build(app)?;

    // New Task action.
    let new_task_item = MenuItemBuilder::with_id(menu_ids::NEW_TASK, "New Task")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;

    // Quit action.
    let quit_item = MenuItemBuilder::with_id(menu_ids::QUIT, "Quit")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    // Build the menu.
    let menu = MenuBuilder::new(app)
        .item(&status_item)
        .separator()
        .item(&new_task_item)
        .separator()
        .item(&quit_item)
        .build()?;

    Ok(menu)
}

/// Handle tray icon events (click, double-click, etc.).
fn handle_tray_event(tray: &tauri::tray::TrayIcon<tauri::Wry>, event: TrayIconEvent) {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            ..
        } => {
            // Left-click: show/focus the main window.
            if let Some(window) = tray.app_handle().get_webview_window("main") {
                if let Err(e) = window.show() {
                    error!(error = %e, "failed to show main window");
                }
                if let Err(e) = window.set_focus() {
                    error!(error = %e, "failed to focus main window");
                }
            }
        }
        TrayIconEvent::DoubleClick { .. } => {
            // Double-click: same as left-click (show/focus window).
            if let Some(window) = tray.app_handle().get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        _ => {
            // Ignore other events (right-click shows menu automatically).
        }
    }
}

/// Handle tray menu item clicks.
fn handle_menu_event(app: &AppHandle<tauri::Wry>, menu_id: &str) {
    match menu_id {
        menu_ids::NEW_TASK => {
            info!("tray: new task requested");
            // Retain one intent until the webview claims it. Pre-listener
            // clicks survive startup/reload; completed wizards cannot replay.
            let id = match app.state::<PendingNewTask>().request() {
                Ok(id) => id,
                Err(e) => {
                    error!(error = %e, "could not record tray intent");
                    return;
                }
            };
            // Open the existing task wizard; creation happens only after user input.
            if let Some(window) = app.get_webview_window("main") {
                if let Err(e) = window.show() {
                    error!(error = %e, "failed to show main window");
                    return;
                }
                if let Err(e) = window.set_focus() {
                    error!(error = %e, "failed to focus main window");
                    return;
                }
                if let Err(e) = window.emit("tray:new-task", id) {
                    error!(error = %e, "failed to emit tray:new-task event");
                }
            }
        }
        menu_ids::QUIT => {
            info!("tray: quit requested");
            // Gracefully exit the app (daemon cleanup happens in Drop).
            app.exit(0);
        }
        menu_ids::STATUS => {
            // Status item is disabled, should never be clicked.
        }
        _ => {
            error!(menu_id, "unknown tray menu item");
        }
    }
}
