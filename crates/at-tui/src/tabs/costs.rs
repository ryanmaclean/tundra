use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;

/// Tab 6: Cost breakdown table.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    let header = Row::new(vec![
        Cell::from("Provider"),
        Cell::from("Model"),
        Cell::from("Input Tokens"),
        Cell::from("Output Tokens"),
        Cell::from("Cost (USD)"),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .costs
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let row = Row::new(vec![
                Cell::from(c.provider.as_str()),
                Cell::from(c.model.as_str()),
                Cell::from(format!("{}", c.input_tokens)),
                Cell::from(format!("{}", c.output_tokens)),
                Cell::from(format!("${:.2}", c.cost_usd)),
            ]);
            if i == app.selected_index {
                row.style(Style::default().bg(Color::DarkGray))
            } else {
                row
            }
        })
        .collect();

    let widths = [
        Constraint::Length(12),
        Constraint::Min(20),
        Constraint::Length(14),
        Constraint::Length(15),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Costs "));

    frame.render_widget(table, chunks[0]);

    // Total row
    let total: f64 = app.costs.iter().map(|c| c.cost_usd).sum();
    let total_tokens_in: u64 = app.costs.iter().map(|c| c.input_tokens).sum();
    let total_tokens_out: u64 = app.costs.iter().map(|c| c.output_tokens).sum();

    let total_text = Paragraph::new(Line::from(format!(
        "  Total: {} input / {} output tokens  |  ${:.2} USD",
        total_tokens_in, total_tokens_out, total,
    )))
    .style(
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    )
    .block(Block::default().borders(Borders::TOP));

    frame.render_widget(total_text, chunks[1]);
}
