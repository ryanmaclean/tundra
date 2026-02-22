# Team D — Native macOS Tahoe/HIG Checklist (Draft)

## Native Shell Baseline
- Native titlebar/toolbar integration
- Native menu semantics (macOS app menu + command routing)
- Sidebar behavior consistent with macOS interaction expectations
- Correct drag regions and traffic-light control behavior

## Interaction Checklist
- Keyboard focus rings and full keyboard navigation
- Command-key shortcut consistency with menu entries
- Trackpad scroll/gesture smoothness in split panes
- Accessibility labels/roles for native and web-hosted controls

## Tauri Touchpoints
| Area | File(s) | Notes |
|---|---|---|
| app bootstrap | `app/tauri/src/main.rs` | macOS titlebar inset already present |
| command surface | `app/tauri/src/commands.rs` | native bridge command expansion point |
| app state | `app/tauri/src/state.rs` | IPC/runtime integration point |

## Phase Plan
1. **Hybrid Native Shell:** native chrome + web content core.
2. **Native Control Layer:** selectively replace high-value controls (sidebar/menu/command palette).
3. **Deep Native Track (optional):** Swift/AppKit plugin bridge for advanced desktop UX.

## Next Steps
1. Build a small native-shell prototype window profile behind feature flag.
2. Add HIG conformance review checklist to PR template for macOS changes.
3. Define what remains web-only to preserve cross-platform velocity.

## Automated Verification
- Static/prototype gate:
  - `tests/interactive/test_native_shell_mode.sh`
- Validates:
  - `AT_NATIVE_SHELL_MACOS` bootstrap flag in `app/tauri/src/main.rs`
  - webview init injection (`__TUNDRA_NATIVE_SHELL__`, `data-native-shell`, `--titlebar-inset`)
  - native-shell CSS hooks in `app/leptos-ui/style.css`
