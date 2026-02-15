use chrono::Local;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Render the bottom status bar.
pub fn render(frame: &mut Frame, area: Rect) {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");

    let left = vec![
        Span::styled("[/]", Style::default().fg(Color::Yellow)),
        Span::raw(" Search  "),
        Span::styled("[:]", Style::default().fg(Color::Yellow)),
        Span::raw(" Command  "),
        Span::styled("[?]", Style::default().fg(Color::Yellow)),
        Span::raw(" Help  "),
        Span::styled("[q]", Style::default().fg(Color::Yellow)),
        Span::raw(" Quit"),
    ];

    // We render left-aligned hints and right-aligned timestamp on the same line.
    // Ratatui doesn't natively support split alignment in a single Paragraph,
    // so we pad the middle.
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
