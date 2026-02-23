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

    // Left: issue list
    let items: Vec<ListItem> = app
        .github_issues
        .iter()
        .enumerate()
        .map(|(i, issue)| {
            let state_color = match issue.state.as_str() {
                "open" => Color::Green,
                "closed" => Color::Red,
                _ => Color::DarkGray,
            };
            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" #{:<5} ", issue.number),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{:<6} ", issue.state),
                    Style::default().fg(state_color),
                ),
                Span::raw(&issue.title),
            ]))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Issues ({}) ", app.github_issues.len()))
            .border_style(Style::default().fg(Color::Green)),
    );
    frame.render_widget(list, chunks[0]);

    // Right: detail
    let detail = if let Some(issue) = app.github_issues.get(app.selected_index) {
        let labels = if issue.labels.is_empty() {
            "none".to_string()
        } else {
            issue.labels.join(", ")
        };
        vec![
            Line::from(vec![
                Span::styled(
                    format!("#{} ", issue.number),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(&issue.title),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("State: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&issue.state),
            ]),
            Line::from(vec![
                Span::styled("Labels: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(labels),
            ]),
            Line::from(vec![
                Span::styled("Assignee: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(issue.assignee.as_deref().unwrap_or("unassigned")),
            ]),
            Line::from(vec![
                Span::styled("Created: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&issue.created),
            ]),
        ]
    } else {
        vec![Line::from("No issue selected")]
    };

    let para = Paragraph::new(detail)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Detail ")
                .border_style(Style::default().fg(Color::Green)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(para, chunks[1]);
}
