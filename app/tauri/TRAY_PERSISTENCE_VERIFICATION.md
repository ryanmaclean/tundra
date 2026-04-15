# System Tray Persistence Verification

## Overview

This document verifies that the Auto-Tundra system tray icon persists when the main window is hidden, and that the window can be restored by clicking the tray icon.

## Implementation Details

### Window Close Behavior

The app implements "hide to tray" behavior:

1. **Window Close Prevention**: When the user clicks the window's close button, the `CloseRequested` event is intercepted (main.rs:177-182)
2. **Hide Instead of Exit**: The window is hidden using `window.hide()` rather than allowing the app to exit
3. **Tray Persistence**: The system tray icon remains active even when the window is hidden
4. **Window Restoration**: Clicking the tray icon (left-click or double-click) restores the hidden window

### Code Reference

**main.rs** (lines 177-182):
```rust
.on_window_event(|window, event| {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        info!("window close requested, hiding instead (tray icon will restore)");
        window.hide().unwrap();
        api.prevent_close();
    }
})
```

**tray.rs** (lines 140-160):
```rust
fn handle_tray_event(tray: &tauri::tray::TrayIcon, event: TrayIconEvent) {
    match event {
        TrayIconEvent::Click { button: MouseButton::Left, .. } => {
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
        // ...
    }
}
```

## Manual Verification Steps

### Prerequisites

- Build the desktop app: `cargo tauri dev` or `cargo tauri build`
- Launch the Auto-Tundra desktop application

### Test Case 1: Window Hides on Close

1. Launch the Auto-Tundra desktop app
2. Verify the system tray icon appears in your system tray/menu bar
3. Click the window's close button (X)
4. **Expected Result**:
   - Window disappears (hides)
   - App does NOT exit
   - System tray icon REMAINS visible in the system tray

### Test Case 2: Tray Icon Restores Window (Left-Click)

1. With the window hidden (from Test Case 1)
2. Left-click on the system tray icon
3. **Expected Result**:
   - Window appears again (restored)
   - Window receives focus
   - Window content is still loaded and functional

### Test Case 3: Tray Icon Restores Window (Double-Click)

1. Hide the window again by clicking the close button
2. Double-click on the system tray icon
3. **Expected Result**:
   - Window appears again (restored)
   - Window receives focus

### Test Case 4: Tray Menu Actions

1. With window visible or hidden, right-click the system tray icon
2. Verify the context menu shows:
   - "Auto-Tundra Running" (disabled status label)
   - "New Task" (enabled)
   - "Quit" (enabled)
3. Click "New Task"
4. **Expected Result**:
   - Window shows (if hidden) and receives focus
   - A new bead is created
   - Frontend receives a `tray:new-task` event
5. Right-click tray icon again, select "Quit"
6. **Expected Result**:
   - App exits completely
   - System tray icon disappears

### Test Case 5: Multiple Hide/Show Cycles

1. Launch the app
2. Close the window (hide to tray) → Click tray to restore → Repeat 5 times
3. **Expected Result**:
   - Window hides and restores reliably each time
   - No memory leaks or performance degradation
   - Tray icon remains responsive

## Platform-Specific Notes

### macOS

- System tray icon appears in the menu bar (top-right)
- Right-click shows the context menu
- Left-click/double-click restores the window

### Windows

- System tray icon appears in the notification area (bottom-right)
- Right-click shows the context menu
- Left-click/double-click restores the window

### Linux

- System tray icon appears in the panel's notification area (varies by desktop environment)
- Behavior depends on the desktop environment (GNOME, KDE, XFCE, etc.)
- Some environments may not support system tray icons fully

## Troubleshooting

### Tray Icon Doesn't Appear

- Check the logs for "failed to initialize system tray" warnings
- Verify the `icons/icon.png` file exists
- On Linux, ensure your desktop environment supports system tray icons

### Window Doesn't Restore

- Check the logs for "failed to show main window" errors
- Try right-click → "New Task" as an alternative way to show the window
- Restart the app if the window becomes unresponsive

### App Exits Instead of Hiding

- Verify the `on_window_event` handler is registered in main.rs
- Check that `api.prevent_close()` is being called
- Review logs for any errors during the close event

## Success Criteria

The system tray persistence feature is verified if:

- ✅ Window hides (not exits) when close button is clicked
- ✅ System tray icon remains visible when window is hidden
- ✅ Left-click on tray icon restores the window
- ✅ Double-click on tray icon restores the window
- ✅ Tray context menu is accessible and functional
- ✅ "Quit" menu item fully exits the app and removes tray icon
- ✅ Multiple hide/restore cycles work reliably

## Related Files

- `app/tauri/src/main.rs` - Window event handler
- `app/tauri/src/tray.rs` - System tray implementation
- `app/tauri/tauri.conf.json` - Tauri configuration
- `app/tauri/src/notifications.rs` - Notification integration
