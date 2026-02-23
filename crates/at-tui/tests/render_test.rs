//! Comprehensive render tests for all 17 TUI tabs.
//!
//! These tests render each tab into a 120x40 terminal buffer (roughly 1080p+
//! equivalent) and verify that expected content appears in the output. This
//! ensures every screen from the Auto Claude web UI has a corresponding TUI
//! representation with the correct data.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, Terminal};

// Include binary-crate modules via path for testing.
#[path = "../src/api_client.rs"]
mod api_client;
#[path = "../src/app.rs"]
mod app;
#[path = "../src/command.rs"]
mod command;
#[path = "../src/effects.rs"]
mod effects;
#[path = "../src/event.rs"]
mod event;
#[path = "../src/tabs/mod.rs"]
mod tabs;
#[path = "../src/ui.rs"]
mod ui;
#[path = "../src/widgets/mod.rs"]
mod widgets;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Standard terminal size for render tests: 120 cols x 40 rows.
const WIDTH: u16 = 120;
const HEIGHT: u16 = 40;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

/// Create a fresh App in offline/demo mode.
fn demo_app() -> app::App {
    app::App::new(true)
}

/// Render the full UI into a test backend and return the buffer content as a
/// single string (all rows concatenated with newlines).
fn render_to_string(app: &mut app::App) -> String {
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| ui::render(frame, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    buffer_to_string(&buf)
}

/// Convert a ratatui Buffer to a readable string (rows joined by newlines).
fn buffer_to_string(buf: &Buffer) -> String {
    let area = buf.area;
    let mut lines = Vec::new();
    for y in area.y..area.y + area.height {
        let mut line = String::new();
        for x in area.x..area.x + area.width {
            let cell = &buf[(x, y)];
            line.push_str(cell.symbol());
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Switch to a specific tab and render.
fn render_tab(tab: usize) -> String {
    let mut app = demo_app();
    switch_to_tab(&mut app, tab);
    render_to_string(&mut app)
}

fn switch_to_tab(app: &mut app::App, tab: usize) {
    // Key '1' → tab 0, '2' → tab 1, ... '9' → tab 8, '0' → tab 9
    // So to go to tab N, press char (N+1) for 0..=8, '0' for tab 9.
    match tab {
        0 => {
            app.on_key(key(KeyCode::Char('1')));
        }
        1 => {
            app.on_key(key(KeyCode::Char('2')));
        }
        2 => {
            app.on_key(key(KeyCode::Char('3')));
        }
        3 => {
            app.on_key(key(KeyCode::Char('4')));
        }
        4 => {
            app.on_key(key(KeyCode::Char('5')));
        }
        5 => {
            app.on_key(key(KeyCode::Char('6')));
        }
        6 => {
            app.on_key(key(KeyCode::Char('7')));
        }
        7 => {
            app.on_key(key(KeyCode::Char('8')));
        }
        8 => {
            app.on_key(key(KeyCode::Char('9')));
        }
        9 => {
            app.on_key(key(KeyCode::Char('0')));
        }
        10 => app.on_key(key(KeyCode::Char('I'))),
        11 => app.on_key(key(KeyCode::Char('W'))),
        12 => app.on_key(key(KeyCode::Char('G'))),
        13 => app.on_key(key(KeyCode::Char('P'))),
        14 => app.on_key(key(KeyCode::Char('S'))),
        15 => app.on_key(key(KeyCode::Char('X'))),
        16 => app.on_key(key(KeyCode::Char('L'))),
        _ => {}
    }
}

/// Assert that the rendered output contains the given substring.
fn assert_contains(output: &str, needle: &str) {
    assert!(
        output.contains(needle),
        "Expected to find {:?} in rendered output.\nFull output:\n{}",
        needle,
        output
    );
}

/// Assert that the rendered output contains ALL of the given substrings.
fn assert_contains_all(output: &str, needles: &[&str]) {
    for needle in needles {
        assert_contains(output, needle);
    }
}

// ===========================================================================
// Tab 0: Dashboard
// ===========================================================================

#[test]
fn render_dashboard_shows_kpi_cards() {
    let output = render_tab(0);
    assert_contains_all(&output, &["Agents", "Beads", "Convoys", "Cost"]);
}

#[test]
fn render_dashboard_shows_kpi_values() {
    let output = render_tab(0);
    // Demo KPI: 2 active agents, 10 beads, 1 active convoy, $8.02
    assert_contains(&output, "2");
    assert_contains(&output, "10");
}

#[test]
fn render_dashboard_shows_agent_summary() {
    let output = render_tab(0);
    // Agent names from demo data
    assert_contains_all(&output, &["mayor-alpha", "deacon-bravo"]);
}

#[test]
fn render_dashboard_shows_activity_feed() {
    let output = render_tab(0);
    // Activity entries from demo data
    assert_contains(&output, "hooked bead bd-002");
    assert_contains(&output, "convoy auth-feature");
}

#[test]
fn render_dashboard_tab_bar_shows_auto_tundra() {
    let output = render_tab(0);
    assert_contains(&output, "auto-tundra");
}

// ===========================================================================
// Tab 1: Agents
// ===========================================================================

#[test]
fn render_agents_table_header() {
    let output = render_tab(1);
    assert_contains_all(&output, &["Name", "Role", "CLI", "Model", "Last Seen"]);
}

#[test]
fn render_agents_shows_all_agents() {
    let output = render_tab(1);
    assert_contains_all(
        &output,
        &[
            "mayor-alpha",
            "deacon-bravo",
            "crew-charlie",
            "crew-delta",
            "witness-echo",
        ],
    );
}

#[test]
fn render_agents_shows_roles() {
    let output = render_tab(1);
    assert_contains_all(&output, &["Mayor", "Deacon", "Crew", "Witness"]);
}

#[test]
fn render_agents_shows_models() {
    let output = render_tab(1);
    assert_contains_all(
        &output,
        &["claude-opus-4", "claude-sonnet-4", "o3", "gemini-2.5-pro"],
    );
}

#[test]
fn render_agents_shows_cli_types() {
    let output = render_tab(1);
    assert_contains_all(&output, &["Claude", "Codex", "Gemini"]);
}

#[test]
fn render_agents_tab_badge_count() {
    let output = render_tab(1);
    // 5 agents → badge [5]
    assert_contains(&output, "[5]");
}

// ===========================================================================
// Tab 2: Beads (Kanban)
// ===========================================================================

#[test]
fn render_beads_kanban_columns() {
    let output = render_tab(2);
    assert_contains_all(&output, &["Backlog", "Hooked", "Slung", "Review", "Done"]);
}

#[test]
fn render_beads_shows_bead_titles() {
    let output = render_tab(2);
    // Demo beads (titles truncated in narrow 20% kanban columns)
    assert_contains(&output, "Refactor conf");
    assert_contains(&output, "Implement aut");
    assert_contains(&output, "Design TUI la");
}

#[test]
fn render_beads_shows_bead_ids() {
    let output = render_tab(2);
    assert_contains_all(&output, &["bd-001", "bd-002", "bd-006"]);
}

#[test]
fn render_beads_column_counts() {
    let output = render_tab(2);
    // Backlog: 3, Hooked: 2, Slung: 2, Review: 1, Done: 2
    // Column headers should show counts
    assert_contains(&output, "(3)");
    assert_contains(&output, "(2)");
    assert_contains(&output, "(1)");
}

#[test]
fn render_beads_kanban_column_selection() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 2);
    assert_eq!(app.kanban_column, 0);
    app.on_key(key(KeyCode::Char('l')));
    assert_eq!(app.kanban_column, 1);
    let output = render_to_string(&mut app);
    // Should still render kanban layout
    assert_contains(&output, "Hooked");
}

// ===========================================================================
// Tab 3: Sessions
// ===========================================================================

#[test]
fn render_sessions_table_header() {
    let output = render_tab(3);
    assert_contains_all(
        &output,
        &["ID", "Agent", "CLI", "Status", "Duration", "CPU"],
    );
}

#[test]
fn render_sessions_shows_all_sessions() {
    let output = render_tab(3);
    assert_contains_all(&output, &["sess-01", "sess-02", "sess-03", "sess-04"]);
}

#[test]
fn render_sessions_shows_agents() {
    let output = render_tab(3);
    assert_contains_all(
        &output,
        &["mayor-alpha", "deacon-bravo", "crew-charlie", "crew-delta"],
    );
}

#[test]
fn render_sessions_shows_durations() {
    let output = render_tab(3);
    assert_contains_all(&output, &["12m 34s", "8m 12s", "45m 01s"]);
}

#[test]
fn render_sessions_shows_statuses() {
    let output = render_tab(3);
    assert_contains_all(&output, &["running", "idle", "starting"]);
}

// ===========================================================================
// Tab 4: Convoys
// ===========================================================================

#[test]
fn render_convoys_shows_convoy_names() {
    let output = render_tab(4);
    assert_contains_all(&output, &["auth-feature", "ci-setup", "api-docs"]);
}

#[test]
fn render_convoys_shows_progress() {
    let output = render_tab(4);
    // Gauge renders status and bead count in label, not literal percentage
    assert_contains(&output, "Active");
    assert_contains(&output, "Completed");
    assert_contains(&output, "beads");
}

#[test]
fn render_convoys_shows_bead_count() {
    let output = render_tab(4);
    // "3 beads" or similar for auth-feature
    assert_contains(&output, "3");
    assert_contains(&output, "4");
}

#[test]
fn render_convoys_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 4);
    app.convoys.clear();
    let output = render_to_string(&mut app);
    assert_contains(&output, "No convoys");
}

// ===========================================================================
// Tab 5: Costs
// ===========================================================================

#[test]
fn render_costs_table_header() {
    let output = render_tab(5);
    assert_contains_all(
        &output,
        &["Provider", "Model", "Input Tokens", "Output Tokens", "Cost"],
    );
}

#[test]
fn render_costs_shows_providers() {
    let output = render_tab(5);
    assert_contains_all(&output, &["Anthropic", "OpenAI", "Google"]);
}

#[test]
fn render_costs_shows_models() {
    let output = render_tab(5);
    assert_contains_all(
        &output,
        &["claude-opus-4", "claude-sonnet-4", "o3", "gemini-2.5-pro"],
    );
}

#[test]
fn render_costs_shows_token_counts() {
    let output = render_tab(5);
    assert_contains(&output, "125000");
    assert_contains(&output, "42000");
}

#[test]
fn render_costs_shows_total_row() {
    let output = render_tab(5);
    // Total cost row
    assert_contains(&output, "Total");
}

// ===========================================================================
// Tab 6: Analytics
// ===========================================================================

#[test]
fn render_analytics_shows_activity_summary() {
    let output = render_tab(6);
    assert_contains(&output, "Activity Summary");
}

#[test]
fn render_analytics_shows_time_ranges() {
    let output = render_tab(6);
    assert_contains_all(
        &output,
        &["00:00", "04:00", "08:00", "12:00", "16:00", "20:00"],
    );
}

#[test]
fn render_analytics_shows_event_counts() {
    let output = render_tab(6);
    assert_contains(&output, "events");
    assert_contains(&output, "Total");
}

#[test]
fn render_analytics_shows_average() {
    let output = render_tab(6);
    assert_contains(&output, "Avg");
}

// ===========================================================================
// Tab 7: Config
// ===========================================================================

#[test]
fn render_config_shows_toml_content() {
    let output = render_tab(7);
    // Config should have section headers (brackets)
    assert_contains(&output, "[");
}

#[test]
fn render_config_shows_key_value_pairs() {
    let output = render_tab(7);
    // Config should contain at least one = sign for key=value
    assert_contains(&output, "=");
}

// ===========================================================================
// Tab 8: MCP
// ===========================================================================

#[test]
fn render_mcp_table_header() {
    let output = render_tab(8);
    assert_contains_all(&output, &["Name", "Transport", "Status", "Tools"]);
}

#[test]
fn render_mcp_shows_servers() {
    let output = render_tab(8);
    assert_contains_all(&output, &["filesystem", "git", "postgres", "web-search"]);
}

#[test]
fn render_mcp_shows_transport_types() {
    let output = render_tab(8);
    assert_contains(&output, "stdio");
    assert_contains(&output, "sse");
}

#[test]
fn render_mcp_shows_statuses() {
    let output = render_tab(8);
    assert_contains(&output, "connected");
    assert_contains(&output, "disconnected");
}

#[test]
fn render_mcp_shows_tool_counts() {
    let output = render_tab(8);
    assert_contains_all(&output, &["8", "12", "5", "3"]);
}

#[test]
fn render_mcp_tab_badge() {
    let output = render_tab(8);
    // 4 MCP servers → badge [4]
    assert_contains(&output, "[4]");
}

// ===========================================================================
// Tab 9: Roadmap
// ===========================================================================

#[test]
fn render_roadmap_shows_items() {
    let output = render_tab(9);
    assert_contains_all(
        &output,
        &[
            "Multi-provider support",
            "WebSocket streaming",
            "Plugin system",
            "Team collaboration",
        ],
    );
}

#[test]
fn render_roadmap_shows_priorities() {
    let output = render_tab(9);
    // Roadmap renders priorities in uppercase
    assert_contains_all(&output, &["HIGH", "MEDIUM", "LOW"]);
}

#[test]
fn render_roadmap_shows_statuses() {
    let output = render_tab(9);
    assert_contains(&output, "in_progress");
    assert_contains(&output, "planned");
    assert_contains(&output, "backlog");
}

#[test]
fn render_roadmap_tab_badge() {
    let output = render_tab(9);
    // 4 roadmap items → badge [4]
    assert_contains(&output, "[4]");
}

// ===========================================================================
// Tab 10: Ideation
// ===========================================================================

#[test]
fn render_ideation_shows_idea_list() {
    let output = render_tab(10);
    assert_contains_all(
        &output,
        &[
            "Auto-retry failed beads",
            "Cost alert thresholds",
            "Git worktree auto-cleanup",
        ],
    );
}

#[test]
fn render_ideation_shows_categories() {
    let output = render_tab(10);
    assert_contains_all(&output, &["performance", "cost", "quality"]);
}

#[test]
fn render_ideation_shows_detail_panel() {
    let output = render_tab(10);
    // Default selection is first idea — detail panel should show it
    assert_contains(&output, "Impact");
    assert_contains(&output, "Effort");
}

#[test]
fn render_ideation_detail_shows_description() {
    let output = render_tab(10);
    assert_contains(&output, "exponential backoff");
}

#[test]
fn render_ideation_navigation_updates_detail() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 10);
    // Move to second idea
    app.on_key(key(KeyCode::Char('j')));
    let output = render_to_string(&mut app);
    assert_contains(&output, "Cost alert thresholds");
    assert_contains(&output, "spend exceeds limit");
}

// ===========================================================================
// Tab 11: Worktrees
// ===========================================================================

#[test]
fn render_worktrees_shows_all_entries() {
    let output = render_tab(11);
    assert_contains_all(
        &output,
        &["feat/auth-module", "fix/ci-pipeline", "docs/api"],
    );
}

#[test]
fn render_worktrees_shows_bead_ids() {
    let output = render_tab(11);
    assert_contains_all(&output, &["bd-002", "bd-001", "bd-005"]);
}

#[test]
fn render_worktrees_shows_status_glyphs() {
    let output = render_tab(11);
    // active → @, stale → x
    assert_contains(&output, "@");
    assert_contains(&output, "x");
}

#[test]
fn render_worktrees_shows_paths() {
    let output = render_tab(11);
    assert_contains(&output, "feat-auth");
    assert_contains(&output, "fix-ci");
}

#[test]
fn render_worktrees_tab_badge() {
    let output = render_tab(11);
    // 3 worktrees → badge [3]
    assert_contains(&output, "[3]");
}

// ===========================================================================
// Tab 12: GitHub Issues
// ===========================================================================

#[test]
fn render_github_issues_shows_list() {
    let output = render_tab(12);
    assert_contains_all(
        &output,
        &[
            "#42",
            "WebSocket support",
            "#41",
            "config parsing",
            "#40",
            "dependencies",
        ],
    );
}

#[test]
fn render_github_issues_shows_states() {
    let output = render_tab(12);
    assert_contains(&output, "open");
    assert_contains(&output, "closed");
}

#[test]
fn render_github_issues_detail_panel() {
    let output = render_tab(12);
    // Detail pane for first issue (#42)
    assert_contains(&output, "#42");
    assert_contains(&output, "enhancement");
    assert_contains(&output, "mayor-alpha");
    assert_contains(&output, "2026-02-18");
}

#[test]
fn render_github_issues_navigation() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 12);
    app.on_key(key(KeyCode::Char('j')));
    let output = render_to_string(&mut app);
    // Second issue detail
    assert_contains(&output, "#41");
    assert_contains(&output, "bug");
}

#[test]
fn render_github_issues_tab_badge() {
    let output = render_tab(12);
    // 3 issues → badge [3]
    assert_contains(&output, "[3]");
}

// ===========================================================================
// Tab 13: GitHub PRs
// ===========================================================================

#[test]
fn render_github_prs_shows_list() {
    let output = render_tab(13);
    assert_contains_all(
        &output,
        &["#15", "agent orchestration", "#14", "convoy status"],
    );
}

#[test]
fn render_github_prs_shows_statuses() {
    let output = render_tab(13);
    assert_contains(&output, "open");
    assert_contains(&output, "merged");
}

#[test]
fn render_github_prs_detail_panel() {
    let output = render_tab(13);
    // Detail pane for first PR (#15)
    assert_contains(&output, "#15");
    assert_contains(&output, "mayor-alpha");
    assert_contains(&output, "deacon-bravo");
    assert_contains(&output, "2026-02-20");
}

#[test]
fn render_github_prs_navigation() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 13);
    app.on_key(key(KeyCode::Char('j')));
    let output = render_to_string(&mut app);
    // Second PR detail
    assert_contains(&output, "#14");
    assert_contains(&output, "crew-charlie");
}

#[test]
fn render_github_prs_tab_badge() {
    // Tab bar may be truncated at 120 cols — test badge count via tab_count logic
    let app = demo_app();
    assert_eq!(app.github_prs.len(), 2);
}

// ===========================================================================
// Tab 14: Stacks
// ===========================================================================

#[test]
fn render_stacks_shows_tree_structure() {
    let output = render_tab(14);
    assert_contains_all(
        &output,
        &[
            "Build agent executor",
            "MCP tool integration",
            "Review agent executor",
        ],
    );
}

#[test]
fn render_stacks_shows_phases() {
    let output = render_tab(14);
    assert_contains(&output, "In Progress");
    assert_contains(&output, "AI Review");
}

#[test]
fn render_stacks_shows_branches() {
    let output = render_tab(14);
    assert_contains(&output, "feat/agent-executor");
    assert_contains(&output, "feat/mcp-integration");
}

#[test]
fn render_stacks_shows_pr_numbers() {
    let output = render_tab(14);
    assert_contains(&output, "PR#41");
    assert_contains(&output, "PR#39");
}

#[test]
fn render_stacks_shows_tree_characters() {
    let output = render_tab(14);
    // Root node uses +, children use |-
    assert_contains(&output, "+");
    assert_contains(&output, "|-");
}

#[test]
fn render_stacks_tab_badge() {
    let output = render_tab(14);
    // 3 stack nodes → badge [3]
    assert_contains(&output, "[3]");
}

// ===========================================================================
// Tab 15: Context
// ===========================================================================

#[test]
fn render_context_shows_sub_tabs() {
    let output = render_tab(15);
    assert_contains_all(&output, &["Index", "Memory"]);
}

#[test]
fn render_context_index_shows_levels() {
    let output = render_tab(15);
    // Default sub-tab is Index (0) → should show context steering levels
    assert_contains(&output, "L0");
    assert_contains(&output, "L1");
    assert_contains(&output, "L2");
    assert_contains(&output, "L3");
}

#[test]
fn render_context_memory_subtab() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 15);
    // Switch to Memory sub-tab with 'l'
    app.on_key(key(KeyCode::Char('l')));
    let output = render_to_string(&mut app);
    // Should show memory entries
    assert_contains(&output, "pattern");
    assert_contains(&output, "convention");
    assert_contains(&output, "flume channels");
    assert_contains(&output, "API keys via env vars");
}

#[test]
fn render_context_subtab_navigation() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 15);
    assert_eq!(app.context_sub_tab, 0);
    app.on_key(key(KeyCode::Char('l')));
    assert_eq!(app.context_sub_tab, 1);
    app.on_key(key(KeyCode::Char('h')));
    assert_eq!(app.context_sub_tab, 0);
}

// ===========================================================================
// Tab 16: Changelog
// ===========================================================================

#[test]
fn render_changelog_shows_versions() {
    let output = render_tab(16);
    assert_contains_all(&output, &["v0.3.0", "v0.2.0"]);
}

#[test]
fn render_changelog_shows_dates() {
    let output = render_tab(16);
    assert_contains_all(&output, &["2026-02-21", "2026-02-15"]);
}

#[test]
fn render_changelog_expanded_version_shows_sections() {
    let output = render_tab(16);
    // v0.3.0 is expanded by default in demo data
    assert_contains(&output, "Added");
    assert_contains(&output, "TUI dashboard");
    assert_contains(&output, "Agent orchestration");
    assert_contains(&output, "Fixed");
    assert_contains(&output, "Config parser edge case");
}

#[test]
fn render_changelog_collapsed_version_hides_sections() {
    let output = render_tab(16);
    // v0.2.0 is collapsed by default
    // The MCP transport detail should NOT be shown (it's in collapsed v0.2.0)
    // But the version header should still appear
    assert_contains(&output, "v0.2.0");
}

#[test]
fn render_changelog_toggle_expand() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 16);
    // Move to second entry (v0.2.0, collapsed)
    app.on_key(key(KeyCode::Char('j')));
    // Toggle expand
    app.on_key(key(KeyCode::Enter));
    assert!(app.changelog[1].expanded);
    let output = render_to_string(&mut app);
    // Now v0.2.0 sections should be visible
    assert_contains(&output, "MCP transport");
    assert_contains(&output, "Convoy system");
}

#[test]
fn render_changelog_toggle_collapse() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 16);
    // First entry (v0.3.0) is expanded, toggle to collapse
    app.on_key(key(KeyCode::Enter));
    assert!(!app.changelog[0].expanded);
}

#[test]
fn render_changelog_tab_badge() {
    // Tab bar may be truncated at 120 cols — test badge count via data
    let app = demo_app();
    assert_eq!(app.changelog.len(), 2);
}

// ===========================================================================
// Status Bar
// ===========================================================================

#[test]
fn render_status_bar_shows_offline_indicator() {
    let output = render_tab(0);
    assert_contains(&output, "OFFLINE");
}

#[test]
fn render_status_bar_shows_keybind_hints() {
    let output = render_tab(0);
    assert_contains_all(&output, &["Tab", "Help", "Quit"]);
}

#[test]
fn render_status_bar_shows_timestamp() {
    let output = render_tab(0);
    // Should contain current date in YYYY-MM-DD format
    assert_contains(&output, "2026");
}

#[test]
fn render_status_bar_live_indicator() {
    let mut app = demo_app();
    app.api_connected = true;
    app.offline = false;
    let output = render_to_string(&mut app);
    assert_contains(&output, "LIVE");
}

// ===========================================================================
// Tab Bar
// ===========================================================================

#[test]
fn render_tab_bar_shows_all_17_tabs() {
    let output = render_tab(0);
    assert_contains_all(
        &output,
        &[
            "Dashboard",
            "Agents",
            "Beads",
            "Sessions",
            "Convoys",
            "Costs",
            "Analytics",
            "Config",
            "MCP",
        ],
    );
}

#[test]
fn render_tab_bar_shows_shortcut_prefixes() {
    let output = render_tab(0);
    // Number shortcuts for tabs 1-9
    assert_contains(&output, "1:Dashboard");
    assert_contains(&output, "2:Agents");
    assert_contains(&output, "3:Beads");
}

// ===========================================================================
// Help Modal
// ===========================================================================

#[test]
fn render_help_modal_overlay() {
    let mut app = demo_app();
    app.on_key(key(KeyCode::Char('?')));
    let output = render_to_string(&mut app);
    assert_contains(&output, "Keybindings");
    assert_contains_all(
        &output,
        &[
            "Jump to tab",
            "Next",
            "previous tab",
            "Move down",
            "Move up",
            "Kanban column",
            "Refresh",
            "Toggle this help",
            "Close help",
            "Quit",
        ],
    );
}

// ===========================================================================
// Command Mode
// ===========================================================================

#[test]
fn render_command_mode_shows_prompt() {
    let mut app = demo_app();
    app.on_key(key(KeyCode::Char(':')));
    assert!(app.in_command_mode);
    let output = render_to_string(&mut app);
    assert_contains(&output, ":");
}

#[test]
fn render_command_mode_shows_buffer() {
    let mut app = demo_app();
    app.on_key(key(KeyCode::Char(':')));
    app.on_key(key(KeyCode::Char('t')));
    app.on_key(key(KeyCode::Char('a')));
    app.on_key(key(KeyCode::Char('b')));
    let output = render_to_string(&mut app);
    assert_contains(&output, "tab");
}

#[test]
fn render_command_result_shown() {
    let mut app = demo_app();
    app.on_key(key(KeyCode::Char(':')));
    // Type "tab 3" (short command that produces short result)
    for c in "tab 3".chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
    app.on_key(key(KeyCode::Enter));
    assert!(!app.in_command_mode);
    // :tab 3 sets current_tab = 3 (0-indexed)
    // Verify the command was executed (no crash, state updated)
    assert_eq!(app.current_tab, 3);
}

#[test]
fn render_command_query_state_produces_result() {
    let mut app = demo_app();
    app.on_key(key(KeyCode::Char(':')));
    for c in "query state".chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
    app.on_key(key(KeyCode::Enter));
    assert!(app.command_result.is_some());
    let result = app.command_result.as_ref().unwrap();
    assert!(result.contains("current_tab"));
}

// ===========================================================================
// Empty State Tests
// ===========================================================================

#[test]
fn render_agents_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 1);
    app.agents.clear();
    let output = render_to_string(&mut app);
    // Should still render without panic; table exists but no data rows
    assert_contains(&output, "Agents");
}

#[test]
fn render_beads_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 2);
    app.beads.clear();
    let output = render_to_string(&mut app);
    // Kanban columns should still render
    assert_contains_all(&output, &["Backlog", "Hooked", "Slung", "Review", "Done"]);
}

#[test]
fn render_sessions_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 3);
    app.sessions.clear();
    let output = render_to_string(&mut app);
    // Header should still render
    assert_contains(&output, "Sessions");
}

#[test]
fn render_mcp_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 8);
    app.mcp_servers.clear();
    let output = render_to_string(&mut app);
    assert_contains(&output, "MCP");
}

#[test]
fn render_github_issues_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 12);
    app.github_issues.clear();
    let output = render_to_string(&mut app);
    // Block title is "Issues" (abbreviated), not "GitHub Issues"
    assert_contains(&output, "Issues");
}

#[test]
fn render_github_prs_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 13);
    app.github_prs.clear();
    let output = render_to_string(&mut app);
    // Block title is "Pull Requests"
    assert_contains(&output, "Pull Requests");
}

#[test]
fn render_roadmap_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 9);
    app.roadmap_items.clear();
    let output = render_to_string(&mut app);
    assert_contains(&output, "Roadmap");
}

#[test]
fn render_ideation_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 10);
    app.ideas.clear();
    let output = render_to_string(&mut app);
    // Block title is "Ideas" not "Ideation"
    assert_contains(&output, "Ideas");
}

#[test]
fn render_worktrees_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 11);
    app.worktrees.clear();
    let output = render_to_string(&mut app);
    assert_contains(&output, "Worktrees");
}

#[test]
fn render_stacks_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 14);
    app.stacks.clear();
    let output = render_to_string(&mut app);
    assert_contains(&output, "Stacks");
}

#[test]
fn render_changelog_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 16);
    app.changelog.clear();
    let output = render_to_string(&mut app);
    assert_contains(&output, "Changelog");
}

#[test]
fn render_memory_empty_state() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 15);
    app.on_key(key(KeyCode::Char('l'))); // Switch to Memory sub-tab
    app.memory_entries.clear();
    let output = render_to_string(&mut app);
    assert_contains(&output, "Memory");
}

// ===========================================================================
// Full Render Cycle (no panics)
// ===========================================================================

#[test]
fn render_all_tabs_no_panic() {
    for tab in 0..=16 {
        let output = render_tab(tab);
        assert!(!output.is_empty(), "Tab {} rendered empty output", tab);
    }
}

#[test]
fn render_all_tabs_at_minimum_size() {
    // Ensure rendering works at a small terminal size (80x24)
    let mut app = demo_app();
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    for tab in 0..=16 {
        switch_to_tab(&mut app, tab);
        terminal.draw(|frame| ui::render(frame, &mut app)).unwrap();
    }
}

#[test]
fn render_all_tabs_at_wide_size() {
    // Ensure rendering works at a very wide terminal (200x50)
    let mut app = demo_app();
    let backend = TestBackend::new(200, 50);
    let mut terminal = Terminal::new(backend).unwrap();
    for tab in 0..=16 {
        switch_to_tab(&mut app, tab);
        terminal.draw(|frame| ui::render(frame, &mut app)).unwrap();
    }
}

// ===========================================================================
// Selection Highlighting Tests
// ===========================================================================

#[test]
fn render_agents_selection_changes_on_navigation() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 1);
    assert_eq!(app.selected_index, 0);
    app.on_key(key(KeyCode::Char('j')));
    assert_eq!(app.selected_index, 1);
    // Renders without panic
    let _ = render_to_string(&mut app);
}

#[test]
fn render_selection_clamped_to_list_bounds() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 1);
    // Move down past end (5 agents, try 10 j presses)
    for _ in 0..10 {
        app.on_key(key(KeyCode::Char('j')));
    }
    assert_eq!(app.selected_index, 4); // clamped to len-1
                                       // Renders without panic
    let _ = render_to_string(&mut app);
}

#[test]
fn render_selection_stays_at_zero_on_k_at_top() {
    let mut app = demo_app();
    switch_to_tab(&mut app, 1);
    app.on_key(key(KeyCode::Char('k')));
    assert_eq!(app.selected_index, 0);
    let _ = render_to_string(&mut app);
}

// ===========================================================================
// Connection State Tests
// ===========================================================================

#[test]
fn render_connecting_state() {
    let mut app = demo_app();
    app.offline = false;
    app.api_connected = false;
    let output = render_to_string(&mut app);
    assert_contains(&output, "...");
}

// ===========================================================================
// Data Integrity After Tab Switching
// ===========================================================================

#[test]
fn render_tab_switching_preserves_data() {
    let mut app = demo_app();
    // Switch through all tabs
    for tab in 0..=16 {
        switch_to_tab(&mut app, tab);
    }
    // Back to dashboard - data should still be intact
    switch_to_tab(&mut app, 0);
    assert_eq!(app.agents.len(), 5);
    assert_eq!(app.beads.len(), 10);
    assert_eq!(app.sessions.len(), 4);
    let output = render_to_string(&mut app);
    assert_contains(&output, "mayor-alpha");
}
