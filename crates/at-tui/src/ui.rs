use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};

use crate::app::{App, TAB_NAMES};
use crate::tabs;
use crate::widgets::{help_modal, status_bar};

/// Master render function: header tabs, content area, status bar.
pub fn render(frame: &mut Frame, app: &mut App) {
    let now = std::time::Instant::now();
    let delta = now.duration_since(app.last_tick);
    let full_area = frame.area();

    // Allocate extra row for command bar when in command mode.
    let status_height = if app.in_command_mode || app.command_result.is_some() { 2 } else { 1 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),              // tab bar
            Constraint::Min(0),                // content
            Constraint::Length(status_height),  // status bar + command
        ])
        .split(full_area);

    render_tab_bar(frame, app, chunks[0]);
    render_content(frame, app, chunks[1]);

    if status_height == 2 {
        let status_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(chunks[2]);
        render_command_bar(frame, app, status_split[0]);
        status_bar::render(frame, app, status_split[1]);
    } else {
        status_bar::render(frame, app, chunks[2]);
    }

    app.effects.tick_and_render(delta, frame.buffer_mut(), full_area);
    app.last_tick = now;

    // Toast notifications (rendered on top of everything)
    app.toasts.tick();
    app.toasts.render(frame, full_area);

    if app.show_help {
        help_modal::render(frame);
    }
}

fn render_command_bar(frame: &mut Frame, app: &App, area: Rect) {
    if app.in_command_mode {
        let spans = vec![
            Span::styled(":", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(&app.command_buffer),
            Span::styled("_", Style::default().fg(Color::Yellow)),
        ];
        let bar = Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Color::Black).fg(Color::White));
        frame.render_widget(bar, area);
    } else if let Some(ref result) = app.command_result {
        let display = if result.len() > area.width as usize {
            format!("{}...", &result[..area.width.saturating_sub(4) as usize])
        } else {
            result.clone()
        };
        let bar = Paragraph::new(Line::from(Span::styled(
            display,
            Style::default().fg(Color::Cyan),
        )))
        .style(Style::default().bg(Color::Black));
        frame.render_widget(bar, area);
    }
}

fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = TAB_NAMES
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let count = tab_count(app, i);
            let badge = if count > 0 {
                format!("[{}]", count)
            } else {
                String::new()
            };
            let label = if i < 9 {
                format!("{}:{}{}", i + 1, t, badge)
            } else if i == 9 {
                format!("0:{}{}", t, badge)
            } else {
                let shortcut = match i {
                    10 => "I",
                    11 => "W",
                    12 => "G",
                    13 => "P",
                    14 => "S",
                    15 => "X",
                    16 => "L",
                    _ => "?",
                };
                format!("{}:{}{}", shortcut, t, badge)
            };
            Line::from(Span::raw(label))
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
        .divider(Span::raw("|"));

    frame.render_widget(tabs, area);
}

/// Return the item count for a tab (used for badges).
fn tab_count(app: &App, tab: usize) -> usize {
    match tab {
        1 => app.agents.len(),
        2 => app.beads.len(),
        3 => app.sessions.len(),
        4 => app.convoys.len(),
        8 => app.mcp_servers.len(),
        9 => app.roadmap_items.len(),
        10 => app.ideas.len(),
        11 => app.worktrees.len(),
        12 => app.github_issues.len(),
        13 => app.github_prs.len(),
        14 => app.stacks.len(),
        16 => app.changelog.len(),
        _ => 0,
    }
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
        9 => tabs::roadmap::render(frame, app, area),
        10 => tabs::ideation::render(frame, app, area),
        11 => tabs::worktrees::render(frame, app, area),
        12 => tabs::github_issues::render(frame, app, area),
        13 => tabs::github_prs::render(frame, app, area),
        14 => tabs::stacks::render(frame, app, area),
        15 => tabs::context::render(frame, app, area),
        16 => tabs::changelog::render(frame, app, area),
        _ => {}
    }
}
