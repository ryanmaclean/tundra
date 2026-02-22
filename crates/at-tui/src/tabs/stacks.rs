use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .stacks
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let indent = "  ".repeat(node.depth);
            let tree_char = if node.depth == 0 { "+" } else { "|-" };
            let phase_color = match node.phase.as_str() {
                "In Progress" => Color::Cyan,
                "AI Review" => Color::Magenta,
                "Done" | "Merged" => Color::Green,
                _ => Color::Yellow,
            };
            let pr_label = match node.pr_number {
                Some(n) => format!(" PR#{}", n),
                None => String::new(),
            };
            let branch = node
                .git_branch
                .as_deref()
                .unwrap_or("-");
            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(format!(" {}{} ", indent, tree_char)),
                Span::styled(
                    format!("{:<14} ", node.phase),
                    Style::default().fg(phase_color),
                ),
                Span::styled(
                    format!("{:<28} ", branch),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(&node.title),
                Span::styled(
                    pr_label,
                    Style::default().fg(Color::Yellow),
                ),
            ]))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Stacks ({}) ", app.stacks.len()))
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(list, area);
}
