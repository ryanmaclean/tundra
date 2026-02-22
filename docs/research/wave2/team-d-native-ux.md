# Team D — Native UX (macOS Tahoe)

## Scope
- Native shell strategy in Tauri (AppKit/SwiftUI bridge path)
- Apple HIG alignment checklist (Tahoe-era desktop UX)
- Web-hybrid vs native controls decision matrix

## Deliverables
- Native shell architecture note
- Tahoe/HIG compliance checklist for key flows
- Prototype plan for titlebar/sidebar/command-palette shell

## Sub-Agent Breakdown
- **D1 (HIG Compliance):** produce Tahoe-era desktop interaction checklist.
- **D2 (Tauri Native Shell):** prototype native chrome (titlebar/sidebar/menu/toolbar).
- **D3 (Inspiration Watch):** track Lapce/Redox interaction and architecture patterns.

## Kickoff Findings
- Tauri app already has macOS-aware titlebar inset handling and can host a native-shell migration incrementally.
- Apple HIG direction supports hybrid architecture: native chrome + web content core.
- Lapce and Redox remain useful watchpoints for Rust-first UX and process-isolation patterns.

## Immediate Tasks
1. Define minimum native surface:
   - titlebar, toolbar, sidebar split, native menu semantics
2. Map Tauri + native bridge options:
   - plugin route for AppKit hooks
   - webview-hosted content with native chrome
3. Build an interaction checklist:
   - focus rings, keyboard nav, trackpad gestures, drag regions
4. Define migration path:
   - phase 1 hybrid shell, phase 2 deeper native widgets

## Acceptance Criteria
- Explicit “native now vs later” boundary
- Risk list for maintainability and cross-platform parity
- Pilot implementation milestones with exit criteria

## Status
- In Progress
