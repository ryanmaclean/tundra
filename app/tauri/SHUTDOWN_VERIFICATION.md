# Daemon Shutdown Verification

This document describes how the desktop app ensures clean daemon shutdown and how to verify it.

## Shutdown Mechanism

### In-Process Daemon (Embedded Mode)

The Tauri desktop app runs the daemon **in-process** (embedded mode), not as a separate process. This means:

1. The daemon is created and started in `main()` via `Daemon::new()` and `daemon.start_embedded()`
2. The daemon is moved into `AppState` which is managed by Tauri
3. When the app exits, `AppState` is dropped, triggering the `Drop` implementation
4. The `Drop` implementation calls `daemon.shutdown()` to gracefully stop background loops

### Background Tasks

The daemon spawns several background tasks:
- Patrol loop (default: every 60s) - detects stuck beads, stale agents, orphan PTYs
- Heartbeat monitor (default: every 30s) - checks agent liveness
- KPI collector (default: every 300s) - collects metrics snapshots
- OAuth token refresh monitor (runs as needed)
- Memory cleanup task (periodic cleanup)
- Notification listener (listens for bead status changes)

All these tasks listen for the shutdown signal and exit cleanly when triggered.

### Shutdown Flow

```
1. User closes app (Cmd+Q, Alt+F4, or clicks close button)
   ↓
2. Tauri begins shutdown sequence
   ↓
3. AppState::drop() is called
   ↓
4. daemon.shutdown() triggers ShutdownSignal
   ↓
5. All background tasks receive shutdown signal
   ↓
6. Background loops exit cleanly (see daemon.rs:304-307)
   ↓
7. Tokio runtime shuts down
   ↓
8. Process exits
```

## Code References

### `app/tauri/src/state.rs`
- `impl Drop for AppState` - Ensures `daemon.shutdown()` is called when app exits
- This guarantees that the shutdown signal is sent to all background tasks

### `crates/at-daemon/src/daemon.rs`
- `pub fn shutdown(&self)` (line 95) - Triggers the shutdown signal
- Shutdown signal handling (line 304-307):
  ```rust
  _ = shutdown_rx.recv() => {
      info!("shutdown signal received, stopping background loops");
      break;
  }
  ```

### `app/tauri/tests/e2e_desktop_app.rs`
- `test_desktop_app_clean_shutdown()` - E2E test verifying shutdown works correctly

## Verification Methods

### 1. Automated Test (Recommended)

Run the E2E shutdown test:

```bash
cargo test -p at-tauri test_desktop_app_clean_shutdown -- --nocapture
```

This test verifies:
- Daemon starts in embedded mode
- API server is accessible
- `daemon.shutdown()` completes without errors
- Server stops responding after shutdown

### 2. Manual Verification

For comprehensive verification, follow these steps:

#### Step 1: Launch the app
```bash
cd app/tauri
cargo tauri dev
```

#### Step 2: Verify daemon is running
In another terminal:
```bash
# Check for running processes
ps aux | grep auto-tundra

# Check API is accessible (replace PORT with actual port from logs)
curl http://localhost:PORT/api/status
```

#### Step 3: Monitor background tasks
Check the application logs for:
- "patrol completed" messages (every 60s)
- "heartbeat check" messages (every 30s)
- "kpi snapshot collected" messages (every 300s)

#### Step 4: Close the app
- macOS: Cmd+Q
- Windows/Linux: Alt+F4 or click close button

#### Step 5: Verify clean shutdown
Check the logs for:
```
[INFO] AppState dropping, triggering daemon shutdown
[INFO] shutdown signal received, stopping background loops
[INFO] UI closed, shutting down daemon
```

#### Step 6: Verify no processes remain
```bash
# Should return nothing (except grep itself)
ps aux | grep auto-tundra

# Check for zombie processes
ps aux | grep 'Z' | grep auto-tundra
```

### 3. Scripted Verification

Run the verification script:

```bash
./app/tauri/tests/verify_shutdown.sh
```

## Expected Behavior

### ✅ Clean Shutdown
- App exits within 1-2 seconds
- Logs show "shutdown signal received"
- No auto-tundra processes remain after exit
- No zombie processes
- No error messages during shutdown

### ❌ Problematic Signs
- Process hangs during shutdown
- Zombie processes (`<defunct>` in ps output)
- Background tasks still running after app closes
- Error messages about tasks being aborted

## Platform-Specific Considerations

### macOS
- System may show "Application Not Responding" if shutdown takes >5s
- Use Activity Monitor to verify process termination
- Check Console.app for any crash reports

### Windows
- Task Manager shows process termination
- Event Viewer may log application exit events
- MSI installer includes proper uninstall hooks

### Linux
- Use `htop` or `top` to monitor process termination
- Check `journalctl` for application logs
- AppImage/deb packages handle cleanup automatically

## Troubleshooting

### If processes don't shut down cleanly:

1. **Check for stuck PTY handles**
   - The patrol loop should reap orphan PTYs every 60s
   - Verify `reap_orphan_ptys()` is being called

2. **Check for blocking async operations**
   - All background loops use `tokio::select!` with shutdown channel
   - Verify no long-running operations block the shutdown signal

3. **Check tokio runtime shutdown**
   - The runtime is dropped when `main()` exits
   - Runtime drop automatically cancels all spawned tasks

### Debug Mode

Enable debug logging:
```bash
RUST_LOG=debug cargo tauri dev
```

Look for:
- Daemon startup messages
- Background loop heartbeats
- Shutdown signal propagation
- Task termination messages

## Success Criteria

- ✅ E2E test `test_desktop_app_clean_shutdown` passes
- ✅ Manual launch/close cycle completes without errors
- ✅ `ps aux | grep auto-tundra` returns nothing after app closes
- ✅ Logs show clean shutdown sequence
- ✅ No zombie processes remain
- ✅ App closes within 2 seconds

## Implementation Details

The clean shutdown is guaranteed by:

1. **Rust Drop semantics** - `AppState::drop()` is always called when the value goes out of scope
2. **ShutdownSignal broadcast** - All background tasks subscribe to the same shutdown channel
3. **Tokio runtime cleanup** - When the runtime is dropped, all tasks are automatically cancelled
4. **Process exit** - The OS reclaims all resources (file handles, memory, threads) when the process exits

Even if the Drop implementation fails to run (e.g., process killed with SIGKILL), the OS ensures no resources leak.
