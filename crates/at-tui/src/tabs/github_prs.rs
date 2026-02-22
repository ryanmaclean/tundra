use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    // Left: PR list
    let items: Vec<ListItem> = app
        .github_prs
        .iter()
        .enumerate()
        .map(|(i, pr)| {
            let status_color = match pr.status.as_str() {
                "open" => Color::Green,
                "merged" => Color::Magenta,
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
                    format!(" #{:<5} ", pr.number),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{:<8} ", pr.status),
                    Style::default().fg(status_color),
                ),
                Span::raw(&pr.title),
            ]))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Pull Requests ({}) ", app.github_prs.len()))
            .border_style(Style::default().fg(Color::Magenta)),
    );
    frame.render_widget(list, chunks[0]);

    // Right: detail
    let detail = if let Some(pr) = app.github_prs.get(app.selected_index) {
        let reviewers = if pr.reviewers.is_empty() {
            "none".to_string()
        } else {
            pr.reviewers.join(", ")
        };
        vec![
            Line::from(vec![
                Span::styled(format!("#{} ", pr.number), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(&pr.title),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&pr.status),
            ]),
            Line::from(vec![
                Span::styled("Author: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&pr.author),
            ]),
            Line::from(vec![
                Span::styled("Reviewers: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(reviewers),
            ]),
            Line::from(vec![
                Span::styled("Created: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&pr.created),
            ]),
        ]
    } else {
        vec![Line::from("No PR selected")]
    };

    let para = Paragraph::new(detail)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Detail ")
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(para, chunks[1]);
}
