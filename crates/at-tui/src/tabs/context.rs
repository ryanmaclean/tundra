use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Tabs};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // sub-tab bar
            Constraint::Min(0),   // content
        ])
        .split(area);

    // Sub-tab bar
    let sub_tabs = Tabs::new(vec![
        Line::from("Index"),
        Line::from("Memory"),
    ])
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .title(" Context ")
            .border_style(Style::default().fg(Color::Cyan)),
    )
    .select(app.context_sub_tab)
    .highlight_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .divider(Span::raw(" | "));

    frame.render_widget(sub_tabs, chunks[0]);

    match app.context_sub_tab {
        0 => render_index(frame, app, chunks[1]),
        1 => render_memory(frame, app, chunks[1]),
        _ => {}
    }
}

fn render_index(frame: &mut Frame, _app: &App, area: Rect) {
    let items = vec![
        ListItem::new(Line::from(vec![
            Span::styled(" L0 ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("Identity — system prompt, role definition"),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(" L1 ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Active — current task, recent context"),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(" L2 ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Reference — project docs, patterns"),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(" L3 ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
            Span::raw("Deep — full codebase, archived context"),
        ])),
    ];

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Context Steering Levels ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(list, area);
}

fn render_memory(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .memory_entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<12} ", entry.category),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{:<12} ", entry.created_at),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(&entry.content),
            ]))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Memory ({}) ", app.memory_entries.len()))
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(list, area);
}
