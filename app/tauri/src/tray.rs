//! System tray integration for the Auto-Tundra desktop app.
//!
//! Provides a persistent system tray icon with a context menu for quick
//! access to core features:
//! - Status indicator (shows app state)
//! - New Task action (opens bead creation dialog)
//! - Quit action (gracefully exits the app)
//!
//! # Architecture
//!
//! ```text
//! System Tray Icon
//!     ├─ Status (label, non-clickable)
//!     ├─ New Task (opens main window + triggers bead creation)
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
    AppHandle, Emitter, Manager, Runtime,
};
use tracing::{error, info};

// Re-export image crate for icon loading.
use image;

/// Menu item identifiers for tray actions.
mod menu_ids {
    pub const STATUS: &str = "status";
    pub const NEW_TASK: &str = "new_task";
    pub const QUIT: &str = "quit";
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
pub fn init_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    info!("initializing system tray");

    // Load the tray icon from the bundled assets.
    let icon = load_tray_icon()?;

    // Build the context menu.
    let menu = build_tray_menu(app)?;

    // Create the tray icon with the menu attached.
    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .menu_on_left_click(false) // Right-click shows menu (standard behavior)
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
fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<tauri::menu::Menu<R>, Box<dyn std::error::Error>> {
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
fn handle_tray_event(tray: &tauri::tray::TrayIcon, event: TrayIconEvent) {
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
fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, menu_id: &str) {
    match menu_id {
        menu_ids::NEW_TASK => {
            info!("tray: new task requested");
            // Show the main window.
            if let Some(window) = app.get_webview_window("main") {
                if let Err(e) = window.show() {
                    error!(error = %e, "failed to show main window");
                    return;
                }
                if let Err(e) = window.set_focus() {
                    error!(error = %e, "failed to focus main window");
                    return;
                }

                // Trigger the "new task" action in the frontend.
                // Emit an event that the Leptos frontend listens for.
                if let Err(e) = window.emit("tray:new-task", ()) {
                    error!(error = %e, "failed to emit tray:new-task event");
                }
            }
        }
        menu_ids::QUIT => {
            info!("tray: quit requested");
            // Gracefully exit the app.
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
