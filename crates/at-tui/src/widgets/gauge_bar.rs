use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Renders a horizontal gauge bar with label and percentage.
///
/// Example output: ` auth-feature [████████░░░░░░] 66% `
///
/// The filled portion uses `color`; the empty portion is rendered in dark gray.
/// The label is left-aligned and the percentage is right-aligned.
pub fn render_gauge(frame: &mut Frame, area: Rect, label: &str, progress: u16, color: Color) {
    let progress = progress.min(100);
    let pct_text = format!(" {}%", progress);

    // Reserve space: 1 leading space + label + 2 spaces + brackets + bar + pct_text
    // Minimum: " label [##] 100%"
    let label_display = format!(" {}", label);
    let overhead = label_display.len() + 2 + 2 + pct_text.len(); // " label" + " [" + "]" + " 100%"
    let bar_width = (area.width as usize).saturating_sub(overhead);

    if bar_width == 0 || area.height == 0 {
        return;
    }

    let filled_count = ((bar_width as u32) * (progress as u32) / 100) as usize;
    let empty_count = bar_width.saturating_sub(filled_count);

    let filled: String = "\u{2588}".repeat(filled_count); // █
    let empty: String = "\u{2591}".repeat(empty_count); // ░

    let line = Line::from(vec![
        Span::styled(label_display, Style::default().fg(Color::White)),
        Span::raw(" ["),
        Span::styled(filled, Style::default().fg(color)),
        Span::styled(empty, Style::default().fg(Color::DarkGray)),
        Span::raw("]"),
        Span::styled(pct_text, Style::default().fg(Color::White)),
    ]);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_clamped_to_100() {
        // Just verify it doesn't panic with out-of-range values.
        let backend = ratatui::backend::TestBackend::new(60, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_gauge(frame, area, "test", 150, Color::Green);
            })
            .unwrap();
    }

    #[test]
    fn zero_progress() {
        let backend = ratatui::backend::TestBackend::new(60, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_gauge(frame, area, "empty", 0, Color::Blue);
            })
            .unwrap();
    }

    #[test]
    fn full_progress() {
        let backend = ratatui::backend::TestBackend::new(60, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_gauge(frame, area, "done", 100, Color::Green);
            })
            .unwrap();
    }
}
