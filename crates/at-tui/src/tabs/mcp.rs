use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;

use crate::app::App;

/// Tab 9: MCP server status.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec![
        Cell::from("Name"),
        Cell::from("Transport"),
        Cell::from("Status"),
        Cell::from("Tools"),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .mcp_servers
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let status_color = match m.status.as_str() {
                "connected" => Color::Green,
                "disconnected" => Color::Red,
                _ => Color::White,
            };
            let row = Row::new(vec![
                Cell::from(m.name.as_str()),
                Cell::from(m.transport.as_str()),
                Cell::from(m.status.as_str()).style(Style::default().fg(status_color)),
                Cell::from(format!("{}", m.tools)),
            ]);
            if i == app.selected_index {
                row.style(Style::default().bg(Color::DarkGray))
            } else {
                row
            }
        })
        .collect();

    let widths = [
        Constraint::Min(16),
        Constraint::Length(12),
        Constraint::Length(14),
        Constraint::Length(6),
    ];

    let table = Table::new(rows, widths).header(header).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" MCP Servers "),
    );

    frame.render_widget(table, area);
}
