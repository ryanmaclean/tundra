use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::{App, BeadInfo};
use crate::ui::truncate_to_width;
use at_core::types::BeadStatus;

/// Number of kanban columns (used to bound left/right navigation).
pub const KANBAN_COLUMNS: usize = 6;

/// Maximum display width (terminal columns) of a bead title in a card.
const TITLE_MAX_WIDTH: usize = 20;

/// Kanban column index for a bead status. Every `BeadStatus` variant maps to
/// exactly one column; Failed and Escalated share the "Attention" column so
/// work needing a human is never shown as queued Backlog.
pub fn column_for(status: &BeadStatus) -> usize {
    match status {
        BeadStatus::Backlog => 0,
        BeadStatus::Hooked => 1,
        BeadStatus::Slung => 2,
        BeadStatus::Review => 3,
        BeadStatus::Done => 4,
        BeadStatus::Failed | BeadStatus::Escalated => 5,
    }
}

/// Tab 3: Kanban board with 6 columns.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, KANBAN_COLUMNS as u32); KANBAN_COLUMNS])
        .split(area);

    let labels: [(&str, Color); KANBAN_COLUMNS] = [
        ("Backlog", Color::White),
        ("Hooked", Color::Yellow),
        ("Slung", Color::Blue),
        ("Review", Color::Magenta),
        ("Done", Color::Green),
        ("Attention", Color::Red),
    ];

    for (i, (label, color)) in labels.iter().enumerate() {
        let beads: Vec<&BeadInfo> = app
            .beads
            .iter()
            .filter(|b| column_for(&b.status) == i)
            .collect();

        let items: Vec<ListItem> = beads
            .iter()
            .map(|b| {
                let lane_indicator = match b.lane {
                    at_core::types::Lane::Critical => "!",
                    at_core::types::Lane::Standard => " ",
                    at_core::types::Lane::Experimental => "~",
                };
                let marker = match b.status {
                    BeadStatus::Failed => "x ",
                    BeadStatus::Escalated => "? ",
                    _ => "",
                };
                let title = truncate_to_width(&b.title, TITLE_MAX_WIDTH);
                ListItem::new(format!("{} {} {}{}", lane_indicator, b.id, marker, title))
            })
            .collect();

        let border_style = if i == app.kanban_column {
            Style::default().fg(*color).add_modifier(Modifier::BOLD)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_has_a_column_and_failures_are_not_backlog() {
        let all = [
            BeadStatus::Backlog,
            BeadStatus::Hooked,
            BeadStatus::Slung,
            BeadStatus::Review,
            BeadStatus::Done,
            BeadStatus::Failed,
            BeadStatus::Escalated,
        ];
        for s in all {
            assert!(column_for(&s) < KANBAN_COLUMNS, "{s:?}");
        }
        assert_ne!(column_for(&BeadStatus::Failed), column_for(&BeadStatus::Backlog));
        assert_ne!(column_for(&BeadStatus::Escalated), column_for(&BeadStatus::Backlog));
    }

    #[test]
    fn render_multibyte_titles_does_not_panic() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut app = App::new(false);
        for (i, title) in [
            "Implement 日本語 translation layer",
            "café résumé naïve façade déjà vu encore",
            "🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀🚀 launch",
            "Ünïcödé everywhere in this long bead title",
        ]
        .iter()
        .enumerate()
        {
            if let Some(b) = app.beads.get_mut(i) {
                b.title = (*title).to_string();
            }
        }
        if let Some(b) = app.beads.get_mut(4) {
            b.status = BeadStatus::Failed;
        }
        let mut terminal = Terminal::new(TestBackend::new(160, 30)).unwrap();
        terminal
            .draw(|f| render(f, &app, f.area()))
            .expect("render must not panic on multibyte titles");
    }
}
