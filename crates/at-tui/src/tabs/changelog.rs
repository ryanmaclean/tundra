use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let mut items: Vec<ListItem> = Vec::new();

    for (idx, entry) in app.changelog.iter().enumerate() {
        let arrow = if entry.expanded { "v" } else { ">" };
        let style = if idx == app.selected_index {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };

        items.push(
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", arrow),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("v{} ", entry.version),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({})", entry.date),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
            .style(style),
        );

        if entry.expanded {
            for (category, items_list) in &entry.sections {
                items.push(ListItem::new(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(
                        format!("{}: ", category),
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    ),
                ])));
                for item in items_list {
                    items.push(ListItem::new(Line::from(vec![
                        Span::raw("     - "),
                        Span::raw(item.as_str()),
                    ])));
                }
            }
        }
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Changelog ({} versions) ", app.changelog.len()))
            .border_style(Style::default().fg(Color::Yellow)),
    );
    frame.render_widget(list, area);
}
