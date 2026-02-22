use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .roadmap_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let priority_color = match item.priority.as_str() {
                "high" => Color::Red,
                "medium" => Color::Yellow,
                _ => Color::Green,
            };
            let status_color = match item.status.as_str() {
                "in_progress" => Color::Cyan,
                "planned" => Color::Yellow,
                "completed" => Color::Green,
                _ => Color::DarkGray,
            };
            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:>4} ", item.priority.to_uppercase()),
                    Style::default().fg(priority_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<12} ", item.status),
                    Style::default().fg(status_color),
                ),
                Span::raw(&item.title),
            ]))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Roadmap ({}) ", app.roadmap_items.len()))
            .border_style(Style::default().fg(Color::Magenta)),
    );
    frame.render_widget(list, area);
}
