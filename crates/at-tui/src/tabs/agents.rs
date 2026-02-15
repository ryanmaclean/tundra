use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};

use crate::app::App;

/// Tab 2: Agent table with status glyphs.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec![
        Cell::from("St"),
        Cell::from("Name"),
        Cell::from("Role"),
        Cell::from("CLI"),
        Cell::from("Model"),
        Cell::from("Last Seen"),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let glyph = a.status.glyph();
            let glyph_color = match glyph {
                "@" => Color::Green,
                "*" => Color::Yellow,
                "!" => Color::Red,
                "?" => Color::Cyan,
                "x" => Color::DarkGray,
                _ => Color::White,
            };
            let row = Row::new(vec![
                Cell::from(glyph).style(Style::default().fg(glyph_color).add_modifier(Modifier::BOLD)),
                Cell::from(a.name.as_str()),
                Cell::from(format!("{:?}", a.role)),
                Cell::from(format!("{:?}", a.cli_type)),
                Cell::from(a.model.as_str()),
                Cell::from(a.last_seen.format("%H:%M:%S").to_string()),
            ]);
            if i == app.selected_index {
                row.style(Style::default().bg(Color::DarkGray))
            } else {
                row
            }
        })
        .collect();

    let widths = [
        ratatui::layout::Constraint::Length(3),
        ratatui::layout::Constraint::Min(16),
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Length(8),
        ratatui::layout::Constraint::Min(18),
        ratatui::layout::Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Agents "),
        );

    frame.render_widget(table, area);
}
