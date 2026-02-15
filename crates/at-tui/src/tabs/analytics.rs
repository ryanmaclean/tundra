use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

/// Tab 7: Activity summary / analytics placeholder.
pub fn render(frame: &mut Frame, _app: &App, area: Rect) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Activity Summary (last 24h)",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from("  00:00 - 04:00  |||                     3 events"),
        Line::from("  04:00 - 08:00  ||||||                  6 events"),
        Line::from("  08:00 - 12:00  ||||||||||||||||        16 events"),
        Line::from("  12:00 - 16:00  ||||||||||||||||||||    20 events"),
        Line::from("  16:00 - 20:00  ||||||||||||||          14 events"),
        Line::from("  20:00 - 24:00  ||||||||                8 events"),
        Line::from(""),
        Line::from(Span::styled(
            "  Total: 67 events  |  Avg: 2.8/hour",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  (Placeholder -- future: sparklines & heatmap)",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Analytics "),
    );

    frame.render_widget(paragraph, area);
}
