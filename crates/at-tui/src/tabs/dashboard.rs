use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Tab 1: KPI cards, agent summary, activity feed.
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // KPI cards
            Constraint::Min(0),    // bottom panels
        ])
        .split(area);

    render_kpi_cards(frame, app, chunks[0]);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    render_agent_summary(frame, app, bottom[0]);
    render_activity_feed(frame, app, bottom[1]);
}

fn render_kpi_cards(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    let cards: Vec<(&str, String, Color)> = vec![
        ("Agents", format!("{}", app.kpi.active_agents), Color::Green),
        ("Beads", format!("{}", app.kpi.total_beads), Color::Yellow),
        (
            "Convoys",
            format!("{}", app.kpi.active_convoys),
            Color::Cyan,
        ),
        (
            "Cost $",
            format!("{:.2}", app.kpi.total_cost),
            Color::Magenta,
        ),
    ];

    for (i, (title, value, color)) in cards.iter().enumerate() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", title))
            .border_style(Style::default().fg(*color));
        let text = Paragraph::new(Line::from(Span::styled(
            value.clone(),
            Style::default().fg(*color).add_modifier(Modifier::BOLD),
        )))
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(text, cols[i]);
    }
}

fn render_agent_summary(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .agents
        .iter()
        .map(|a| {
            let glyph = a.status.glyph();
            let color = glyph_color(glyph);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", glyph),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("{} ({})", a.name, format_role(&a.role))),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Agents "));
    frame.render_widget(list, area);
}

fn render_activity_feed(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .activity
        .iter()
        .map(|entry| {
            let ts = entry.timestamp.format("%H:%M:%S");
            ListItem::new(Line::from(vec![
                Span::styled(format!("[{}] ", ts), Style::default().fg(Color::DarkGray)),
                Span::raw(&entry.message),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Activity "));
    frame.render_widget(list, area);
}

fn glyph_color(glyph: &str) -> Color {
    match glyph {
        "@" => Color::Green,
        "*" => Color::Yellow,
        "!" => Color::Red,
        "?" => Color::Cyan,
        "x" => Color::DarkGray,
        _ => Color::White,
    }
}

fn format_role(role: &at_core::types::AgentRole) -> &'static str {
    use at_core::types::AgentRole::*;
    match role {
        Mayor => "mayor",
        Deacon => "deacon",
        Witness => "witness",
        Refinery => "refinery",
        Polecat => "polecat",
        Crew => "crew",
        SpecGatherer => "spec-gather",
        SpecWriter => "spec-write",
        SpecResearcher => "spec-research",
        SpecCritic => "spec-critic",
        SpecValidator => "spec-validate",
        Planner => "planner",
        FollowupPlanner => "followup",
        Coder => "coder",
        CoderRecovery => "recovery",
        QaReviewer => "qa-review",
        QaFixer => "qa-fix",
        ValidationFixer => "val-fix",
        InsightExtractor => "insights",
        ComplexityAssessor => "complexity",
        CompetitorAnalysis => "competitor",
        AiAnalyzer => "analyzer",
        IdeationCodeQuality => "idea-quality",
        IdeationPerformance => "idea-perf",
        IdeationSecurity => "idea-sec",
        IdeationDocumentation => "idea-docs",
        IdeationUiUx => "idea-ux",
        IdeationCodeImprovements => "idea-improve",
        RoadmapDiscovery => "roadmap-disc",
        RoadmapFeatures => "roadmap-feat",
        CommitMessage => "commit-msg",
        PrTemplateFiller => "pr-template",
        MergeResolver => "merge",
        Plugin => "plugin",
    }
}
