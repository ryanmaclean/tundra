use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};

use crate::app::{App, TAB_NAMES};
use crate::tabs;
use crate::widgets::{help_modal, status_bar};

/// Master render function: header tabs, content area, status bar.
pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(0),   // content
            Constraint::Length(1), // status bar
        ])
        .split(frame.area());

    render_tab_bar(frame, app, chunks[0]);
    render_content(frame, app, chunks[1]);
    status_bar::render(frame, chunks[2]);

    if app.show_help {
        help_modal::render(frame);
    }
}

fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = TAB_NAMES
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let num = format!("{}", i + 1);
            Line::from(vec![
                Span::styled(
                    num,
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(":"),
                Span::raw(*t),
            ])
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(" auto-tundra ")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .select(app.current_tab)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::raw(" | "));

    frame.render_widget(tabs, area);
}

fn render_content(frame: &mut Frame, app: &App, area: Rect) {
    match app.current_tab {
        0 => tabs::dashboard::render(frame, app, area),
        1 => tabs::agents::render(frame, app, area),
        2 => tabs::beads::render(frame, app, area),
        3 => tabs::sessions::render(frame, app, area),
        4 => tabs::convoys::render(frame, app, area),
        5 => tabs::costs::render(frame, app, area),
        6 => tabs::analytics::render(frame, app, area),
        7 => tabs::config::render(frame, app, area),
        8 => tabs::mcp::render(frame, app, area),
        _ => {}
    }
}
