# Bundled Terminal Stack Plan (MIT-first)

This project now treats terminal UX as a **bundled runtime target** (not a cross-terminal compatibility target).

## Current implementation

- Backend PTY/session lifecycle: `portable-pty` + WebSocket bridge.
- GUI terminal renderer: xterm.js runtime bridge in
  `/Users/studio/rust-harness/app/leptos-ui/src/components/terminal_view.rs`
  and `/Users/studio/rust-harness/app/leptos-ui/terminal_runtime.js`.
- Bundled deterministic profile: `bundled-card` (fixed font metrics + palette).

## MIT-licensed Rust libraries to leverage next

### 1) `crossterm` + `ratatui` (already in stack)
- Use as canonical TUI path (`at-tui`) for operator dashboards and headless automation.
- Add card-render widgets in `at-tui` for planning poker and lane estimate overlays.

### 2) `console`
- Use for richer CLI output formatting in `at-cli` (doctor/perf/probe commands).
- Keep stable color fallback for non-interactive CI.

### 3) `indicatif`
- Use for long-running CLI actions (`sync`, `doctor --deep`, repo parity jobs).
- Add bounded spinner/progress bars with explicit timeouts.

### 4) `termimad`
- Use for markdown-rich terminal help and in-CLI docs (`at --help`, skill docs).
- Reuse project docs content directly instead of plain-text duplication.

### 5) `InTerm`
- Candidate for advanced interactive prompts and terminal workflows.
- Evaluate against current command-mode and headless JSON protocol before adoption.

## Execution rules

1. GUI mode can assume bundled renderer + bundled font profile.
2. TUI mode remains robust for CI/headless and remote operators.
3. Shared data model for terminal sessions/settings across GUI + TUI.
4. New terminal features must include:
   - API contract tests (`at-bridge`)
   - render/behavior tests (`at-tui` + `leptos-ui` where possible)
   - perf baseline before/after for PTY, WS, and frame/render timing.
