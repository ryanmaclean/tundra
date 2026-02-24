use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Tab: Planning Poker - displays estimation cards for agile planning sessions.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    // Split into 2 rows for card grid (5 cards per row)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    // Top row: 0, 1, 2, 3, 5
    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(rows[0]);

    // Bottom row: 8, 13, 21, ?, ∞
    let bottom_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(rows[1]);

    // Define cards with values and colors
    let cards = [
        ("0", "🂠", Color::White),
        ("1", "🂡", Color::Green),
        ("2", "🂢", Color::Cyan),
        ("3", "🂣", Color::Blue),
        ("5", "🂥", Color::Yellow),
        ("8", "🂨", Color::Magenta),
        ("13", "🂭", Color::Red),
        ("21", "🂾", Color::LightRed),
        ("?", "🂠", Color::DarkGray),
        ("∞", "♾", Color::LightMagenta),
    ];

    // Detect Unicode support (simple heuristic: check TERM or LANG)
    let use_unicode = supports_unicode();

    // Render top row cards (0-5)
    for (i, area) in top_row.iter().enumerate() {
        render_card(frame, &cards[i], *area, use_unicode);
    }

    // Render bottom row cards (8, 13, 21, ?, ∞)
    for (i, area) in bottom_row.iter().enumerate() {
        render_card(frame, &cards[i + 5], *area, use_unicode);
    }
}

/// Render a single planning poker card.
fn render_card(
    frame: &mut Frame,
    card: &(&str, &str, Color),
    area: Rect,
    use_unicode: bool,
) {
    let (value, unicode_glyph, color) = card;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(*color))
        .title(format!(" {} ", value))
        .title_style(Style::default().fg(*color).add_modifier(Modifier::BOLD));

    // Create card content
    let content = if use_unicode {
        // Unicode mode: show playing card symbol
        vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}  ", unicode_glyph),
                Style::default()
                    .fg(*color)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ]
    } else {
        // ASCII fallback mode: show value in center
        vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}  ", value),
                Style::default()
                    .fg(*color)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ]
    };

    let paragraph = Paragraph::new(content)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(paragraph, area);
}

/// Detect if terminal supports Unicode characters.
/// Uses environment variables as heuristics.
fn supports_unicode() -> bool {
    // Check TERM variable for modern terminal emulators
    if let Ok(term) = std::env::var("TERM") {
        let modern_terms = [
            "xterm-256color",
            "screen-256color",
            "tmux-256color",
            "alacritty",
            "kitty",
            "wezterm",
        ];
        if modern_terms.iter().any(|&t| term.contains(t)) {
            return true;
        }
    }

    // Check LANG for UTF-8 encoding
    if let Ok(lang) = std::env::var("LANG") {
        if lang.to_uppercase().contains("UTF-8") || lang.to_uppercase().contains("UTF8") {
            return true;
        }
    }

    // Check LC_ALL as fallback
    if let Ok(lc_all) = std::env::var("LC_ALL") {
        if lc_all.to_uppercase().contains("UTF-8") || lc_all.to_uppercase().contains("UTF8") {
            return true;
        }
    }

    // Default to Unicode for modern systems
    true
}
