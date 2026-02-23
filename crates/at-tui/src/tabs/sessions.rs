use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;

use crate::app::App;

/// Tab 4: Session list.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec![
        Cell::from("ID"),
        Cell::from("Agent"),
        Cell::from("CLI"),
        Cell::from("Status"),
        Cell::from("Duration"),
        Cell::from("CPU"),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let status_color = match s.status.as_str() {
                "running" => Color::Green,
                "idle" => Color::Yellow,
                "starting" => Color::Cyan,
                _ => Color::White,
            };
            let row = Row::new(vec![
                Cell::from(s.id.as_str()),
                Cell::from(s.agent.as_str()),
                Cell::from(format!("{:?}", s.cli_type)),
                Cell::from(s.status.as_str()).style(Style::default().fg(status_color)),
                Cell::from(s.duration.as_str()),
                Cell::from(s.cpu.as_str()),
            ]);
            if i == app.selected_index {
                row.style(Style::default().bg(Color::DarkGray))
            } else {
                row
            }
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Min(16),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(6),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Sessions "));

    frame.render_widget(table, area);
}
