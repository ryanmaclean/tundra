use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left: idea list
    let items: Vec<ListItem> = app
        .ideas
        .iter()
        .enumerate()
        .map(|(i, idea)| {
            let cat_color = match idea.category.as_str() {
                "performance" => Color::Cyan,
                "cost" => Color::Yellow,
                "security" => Color::Red,
                "quality" => Color::Green,
                _ => Color::White,
            };
            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<12} ", idea.category),
                    Style::default().fg(cat_color),
                ),
                Span::raw(&idea.title),
            ]))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Ideas ({}) ", app.ideas.len()))
            .border_style(Style::default().fg(Color::Yellow)),
    );
    frame.render_widget(list, chunks[0]);

    // Right: detail panel
    let detail = if let Some(idea) = app.ideas.get(app.selected_index) {
        vec![
            Line::from(vec![
                Span::styled("Title: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&idea.title),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Category: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&idea.category),
            ]),
            Line::from(vec![
                Span::styled("Impact: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&idea.impact),
            ]),
            Line::from(vec![
                Span::styled("Effort: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&idea.effort),
            ]),
            Line::from(""),
            Line::from(Span::raw(&idea.description)),
        ]
    } else {
        vec![Line::from("No idea selected")]
    };

    let para = Paragraph::new(detail)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Detail ")
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(para, chunks[1]);
}
