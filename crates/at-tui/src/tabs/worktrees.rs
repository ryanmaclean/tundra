use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .worktrees
        .iter()
        .enumerate()
        .map(|(i, wt)| {
            let status_color = match wt.status.as_str() {
                "active" => Color::Green,
                "stale" => Color::DarkGray,
                _ => Color::Yellow,
            };
            let dot = match wt.status.as_str() {
                "active" => "@",
                "stale" => "x",
                _ => "*",
            };
            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", dot),
                    Style::default().fg(status_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<24} ", wt.branch),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{:<8} ", wt.bead_id),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(&wt.path),
            ]))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Worktrees ({}) ", app.worktrees.len()))
            .border_style(Style::default().fg(Color::Blue)),
    );
    frame.render_widget(list, area);
}
