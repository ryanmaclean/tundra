use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

// We reference types from the binary crate by including modules directly.
#[path = "../src/api_client.rs"]
mod api_client;
#[path = "../src/effects.rs"]
mod effects;
#[path = "../src/command.rs"]
mod command;
#[path = "../src/app.rs"]
mod app;
#[path = "../src/tabs/mod.rs"]
mod tabs;
#[path = "../src/widgets/mod.rs"]
mod widgets;
#[path = "../src/event.rs"]
mod event;
#[path = "../src/ui.rs"]
mod ui;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

#[test]
fn test_app_new_creates_valid_state() {
    let app = app::App::new(true);
    assert_eq!(app.current_tab, 0);
    assert!(!app.should_quit);
    assert!(!app.show_help);
    assert!(!app.agents.is_empty());
    assert!(!app.beads.is_empty());
    assert!(!app.sessions.is_empty());
    assert!(!app.convoys.is_empty());
    assert!(!app.costs.is_empty());
    assert!(!app.mcp_servers.is_empty());
}

#[test]
fn test_tab_navigation_1_through_9() {
    let mut app = app::App::new(true);

    for i in 1..=9u8 {
        let c = (b'0' + i) as char;
        app.on_key(key(KeyCode::Char(c)));
        assert_eq!(app.current_tab, (i - 1) as usize);
    }
}

#[test]
fn test_tab_next_prev() {
    let mut app = app::App::new(true);
    assert_eq!(app.current_tab, 0);

    app.on_key(key(KeyCode::Tab));
    assert_eq!(app.current_tab, 1);

    app.on_key(key(KeyCode::BackTab));
    assert_eq!(app.current_tab, 0);

    // Wrap backwards (17 tabs: 0..16, wraps to 16)
    app.on_key(key(KeyCode::BackTab));
    assert_eq!(app.current_tab, app::TAB_NAMES.len() - 1);

    // Wrap forwards
    app.on_key(key(KeyCode::Tab));
    assert_eq!(app.current_tab, 0);
}

#[test]
fn test_j_k_navigation() {
    let mut app = app::App::new(true);
    // Tab 2 (agents) has items
    app.on_key(key(KeyCode::Char('2')));
    assert_eq!(app.current_tab, 1);
    assert_eq!(app.selected_index, 0);

    app.on_key(key(KeyCode::Char('j')));
    assert_eq!(app.selected_index, 1);

    app.on_key(key(KeyCode::Char('j')));
    assert_eq!(app.selected_index, 2);

    app.on_key(key(KeyCode::Char('k')));
    assert_eq!(app.selected_index, 1);

    // k at 0 stays at 0
    app.on_key(key(KeyCode::Char('k')));
    assert_eq!(app.selected_index, 0);
    app.on_key(key(KeyCode::Char('k')));
    assert_eq!(app.selected_index, 0);
}

#[test]
fn test_quit() {
    let mut app = app::App::new(true);
    assert!(!app.should_quit);
    app.on_key(key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn test_help_toggle() {
    let mut app = app::App::new(true);
    assert!(!app.show_help);

    app.on_key(key(KeyCode::Char('?')));
    assert!(app.show_help);

    // While help is shown, other keys are ignored
    app.on_key(key(KeyCode::Char('q')));
    assert!(!app.should_quit);

    // ? again closes help
    app.on_key(key(KeyCode::Char('?')));
    assert!(!app.show_help);

    // Esc also closes help
    app.on_key(key(KeyCode::Char('?')));
    assert!(app.show_help);
    app.on_key(key(KeyCode::Esc));
    assert!(!app.show_help);
}

#[test]
fn test_status_glyph() {
    assert_eq!(app::status_glyph("active"), "@");
    assert_eq!(app::status_glyph("idle"), "*");
    assert_eq!(app::status_glyph("pending"), "!");
    assert_eq!(app::status_glyph("unknown"), "?");
    assert_eq!(app::status_glyph("stopped"), "x");
    assert_eq!(app::status_glyph("anything"), "-");
}

#[test]
fn test_kanban_navigation() {
    let mut app = app::App::new(true);
    // Switch to Beads tab (tab 2) for kanban navigation
    app.on_key(key(KeyCode::Char('3')));
    assert_eq!(app.current_tab, 2);
    assert_eq!(app.kanban_column, 0);

    app.on_key(key(KeyCode::Char('l')));
    assert_eq!(app.kanban_column, 1);

    app.on_key(key(KeyCode::Char('l')));
    app.on_key(key(KeyCode::Char('l')));
    app.on_key(key(KeyCode::Char('l')));
    assert_eq!(app.kanban_column, 4);

    // Cannot go past 4
    app.on_key(key(KeyCode::Char('l')));
    assert_eq!(app.kanban_column, 4);

    app.on_key(key(KeyCode::Char('h')));
    assert_eq!(app.kanban_column, 3);

    // Back to 0
    app.on_key(key(KeyCode::Char('h')));
    app.on_key(key(KeyCode::Char('h')));
    app.on_key(key(KeyCode::Char('h')));
    assert_eq!(app.kanban_column, 0);

    // Cannot go below 0
    app.on_key(key(KeyCode::Char('h')));
    assert_eq!(app.kanban_column, 0);
}

#[test]
fn test_tab_switch_resets_selected_index() {
    let mut app = app::App::new(true);
    app.on_key(key(KeyCode::Char('2')));
    app.on_key(key(KeyCode::Char('j')));
    app.on_key(key(KeyCode::Char('j')));
    assert_eq!(app.selected_index, 2);

    // Switch tab resets index
    app.on_key(key(KeyCode::Char('1')));
    assert_eq!(app.selected_index, 0);
}

#[test]
fn test_new_tabs_exist() {
    let app = app::App::new(true);
    assert!(!app.worktrees.is_empty());
    assert!(!app.github_issues.is_empty());
    assert!(!app.github_prs.is_empty());
    assert!(!app.roadmap_items.is_empty());
    assert!(!app.ideas.is_empty());
    assert!(!app.stacks.is_empty());
    assert!(!app.changelog.is_empty());
    assert!(!app.memory_entries.is_empty());
}

#[test]
fn test_letter_shortcuts_for_extended_tabs() {
    let mut app = app::App::new(true);

    // R → Roadmap (tab 9)
    app.on_key(key(KeyCode::Char('R')));
    assert_eq!(app.current_tab, 9);

    // G → GitHub Issues (tab 12)
    app.on_key(key(KeyCode::Char('G')));
    assert_eq!(app.current_tab, 12);

    // P → GitHub PRs (tab 13)
    app.on_key(key(KeyCode::Char('P')));
    assert_eq!(app.current_tab, 13);

    // S → Stacks (tab 14)
    app.on_key(key(KeyCode::Char('S')));
    assert_eq!(app.current_tab, 14);

    // L → Changelog (tab 16)
    app.on_key(key(KeyCode::Char('L')));
    assert_eq!(app.current_tab, 16);
}

#[test]
fn test_0_key_goes_to_tab_10() {
    let mut app = app::App::new(true);
    app.on_key(key(KeyCode::Char('0')));
    assert_eq!(app.current_tab, 9);
}
