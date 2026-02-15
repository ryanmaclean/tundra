use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// Render a centered help modal overlay.
pub fn render(frame: &mut Frame) {
    let area = centered_rect(60, 70, frame.area());

    // Clear the area behind the popup.
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::from(Span::styled(
            "  Keybindings",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        help_line("1-9", "Jump to tab"),
        help_line("Tab / Shift-Tab", "Next / previous tab"),
        help_line("j / Down", "Move down in list"),
        help_line("k / Up", "Move up in list"),
        help_line("h / Left", "Kanban column left"),
        help_line("l / Right", "Kanban column right"),
        help_line("r", "Refresh data"),
        help_line("?", "Toggle this help"),
        help_line("Esc", "Close help / cancel"),
        help_line("q", "Quit"),
        help_line("Ctrl-c", "Force quit"),
        Line::from(""),
        Line::from(Span::styled(
            "  Press ? or Esc to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Help ")
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(paragraph, area);
}

fn help_line(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{:<20}", key),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(desc.to_string()),
    ])
}

/// Create a centered rectangle of the given percentage of the parent.
fn centered_rect(percent_x: u16, percent_y: u16, parent: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(parent);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);

    horizontal[1]
}
