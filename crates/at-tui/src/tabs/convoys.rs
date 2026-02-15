use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use at_core::types::ConvoyStatus;
use crate::app::App;

/// Tab 5: Convoy progress gauges.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    if app.convoys.is_empty() {
        let msg = Paragraph::new("No convoys")
            .block(Block::default().borders(Borders::ALL).title(" Convoys "));
        frame.render_widget(msg, area);
        return;
    }

    let constraints: Vec<Constraint> = app
        .convoys
        .iter()
        .map(|_| Constraint::Length(4))
        .chain(std::iter::once(Constraint::Min(0)))
        .collect();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (i, convoy) in app.convoys.iter().enumerate() {
        let color = match convoy.status {
            ConvoyStatus::Completed => Color::Green,
            ConvoyStatus::Active => Color::Yellow,
            ConvoyStatus::Forming => Color::Cyan,
            ConvoyStatus::Aborted => Color::Red,
        };

        let label = format!(
            "{} [{:?}] ({} beads)",
            convoy.name, convoy.status, convoy.bead_count
        );

        let gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", convoy.name)),
            )
            .gauge_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
            .percent(convoy.progress)
            .label(label);

        frame.render_widget(gauge, rows[i]);
    }
}
