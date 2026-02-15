use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};

use at_core::types::BeadStatus;
use crate::app::{App, BeadInfo};

/// Tab 3: Kanban board with 5 columns.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(area);

    let statuses = [
        ("Backlog", BeadStatus::Backlog, Color::White),
        ("Hooked", BeadStatus::Hooked, Color::Yellow),
        ("Slung", BeadStatus::Slung, Color::Blue),
        ("Review", BeadStatus::Review, Color::Magenta),
        ("Done", BeadStatus::Done, Color::Green),
    ];

    for (i, (label, status, color)) in statuses.iter().enumerate() {
        let beads: Vec<&BeadInfo> = app
            .beads
            .iter()
            .filter(|b| b.status == *status)
            .collect();

        let items: Vec<ListItem> = beads
            .iter()
            .map(|b| {
                let lane_indicator = match b.lane {
                    at_core::types::Lane::Critical => "!",
                    at_core::types::Lane::Standard => " ",
                    at_core::types::Lane::Experimental => "~",
                };
                let title = if b.title.len() > 20 {
                    format!("{}...", &b.title[..17])
                } else {
                    b.title.clone()
                };
                ListItem::new(format!("{} {} {}", lane_indicator, b.id, title))
            })
            .collect();

        let border_style = if i == app.kanban_column {
            Style::default()
                .fg(*color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(*color)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ({}) ", label, beads.len()))
            .border_style(border_style);

        let list = List::new(items).block(block);
        frame.render_widget(list, columns[i]);
    }
}
