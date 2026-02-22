use chrono::Local;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;

/// Render the bottom status bar.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");

    let conn_indicator = if app.offline {
        Span::styled(" OFFLINE ", Style::default().fg(Color::Black).bg(Color::Yellow))
    } else if app.api_connected {
        Span::styled(" LIVE ", Style::default().fg(Color::Black).bg(Color::Green))
    } else {
        Span::styled(" ... ", Style::default().fg(Color::Black).bg(Color::DarkGray))
    };

    let left = vec![
        conn_indicator,
        Span::raw(" "),
        Span::styled("[Tab]", Style::default().fg(Color::Yellow)),
        Span::raw(" Switch  "),
        Span::styled("[?]", Style::default().fg(Color::Yellow)),
        Span::raw(" Help  "),
        Span::styled("[q]", Style::default().fg(Color::Yellow)),
        Span::raw(" Quit"),
    ];

    let left_len: usize = left.iter().map(|s| s.content.len()).sum();
    let right_text = format!("{}", now);
    let total_width = area.width as usize;
    let padding = if total_width > left_len + right_text.len() {
        total_width - left_len - right_text.len()
    } else {
        1
    };

    let mut spans = left;
    spans.push(Span::raw(" ".repeat(padding)));
    spans.push(Span::styled(
        right_text,
        Style::default().fg(Color::DarkGray),
    ));

    let bar = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    frame.render_widget(bar, area);
}
