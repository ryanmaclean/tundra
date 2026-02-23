use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;

/// Tab 8: Config viewer.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = app
        .config_text
        .lines()
        .map(|l| {
            if l.starts_with('[') {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))
            } else if l.contains('=') {
                let mut parts = l.splitn(2, '=');
                let key = parts.next().unwrap_or("");
                let val = parts.next().unwrap_or("");
                Line::from(vec![
                    Span::styled(key.to_string(), Style::default().fg(Color::Cyan)),
                    Span::raw("="),
                    Span::styled(val.to_string(), Style::default().fg(Color::White)),
                ])
            } else if l.starts_with('#') {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(Color::DarkGray),
                ))
            } else {
                Line::from(l.to_string())
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Config (~/.auto-tundra/config.toml) "),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}
