use leptos::prelude::*;

/// Nav item labels — used for tab_label() lookup and rendering.
const NAV_LABELS: &[&str] = &[
    "Dashboard",        // 0
    "Kanban Board",     // 1
    "Agent Terminals",  // 2
    "Insights",         // 3
    "Ideation",         // 4
    "Roadmap",          // 5
    "Changelog",        // 6
    "Context",          // 7
    "MCP Overview",     // 8
    "Worktrees",        // 9
    "GitHub Issues",    // 10
    "GitHub PRs",       // 11
    "Claude Code",      // 12
    "Settings",         // 13
    "Terminals",        // 14
    "Onboarding",       // 15
    "Stacks",           // 16
];

/// Inline SVG icon for a nav item. Lucide-style 18x18 stroke icons.
fn nav_icon(idx: usize) -> impl IntoView {
    let svg = match idx {
        // Dashboard — grid/layout
        0 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="9" rx="1"/><rect x="14" y="3" width="7" height="5" rx="1"/><rect x="14" y="12" width="7" height="9" rx="1"/><rect x="3" y="16" width="7" height="5" rx="1"/></svg>"#,
        // Kanban — columns
        1 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="5" height="18" rx="1"/><rect x="10" y="3" width="5" height="12" rx="1"/><rect x="17" y="3" width="5" height="15" rx="1"/></svg>"#,
        // Agent Terminals — monitor
        2 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>"#,
        // Insights — bar chart
        3 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/></svg>"#,
        // Ideation — lightbulb
        4 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18h6"/><path d="M10 22h4"/><path d="M15.09 14c.18-.98.65-1.74 1.41-2.5A4.65 4.65 0 0018 8 6 6 0 006 8c0 1 .23 2.23 1.5 3.5A4.61 4.61 0 018.91 14"/></svg>"#,
        // Roadmap — map
        5 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="1 6 1 22 8 18 16 22 23 18 23 2 16 6 8 2 1 6"/><line x1="8" y1="2" x2="8" y2="18"/><line x1="16" y1="6" x2="16" y2="22"/></svg>"#,
        // Changelog — file-text
        6 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>"#,
        // Context — layers
        7 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 2 7 12 12 22 7 12 2"/><polyline points="2 17 12 22 22 17"/><polyline points="2 12 12 17 22 12"/></svg>"#,
        // MCP Overview — plug/zap
        8 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>"#,
        // Worktrees — git-branch
        9 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 01-9 9"/></svg>"#,
        // GitHub Issues — circle-dot
        10 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="3"/></svg>"#,
        // GitHub PRs — git-pull-request
        11 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M13 6h3a2 2 0 012 2v7"/><line x1="6" y1="9" x2="6" y2="21"/></svg>"#,
        // Claude Code — bot/cpu
        12 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6"/><line x1="9" y1="1" x2="9" y2="4"/><line x1="15" y1="1" x2="15" y2="4"/><line x1="9" y1="20" x2="9" y2="23"/><line x1="15" y1="20" x2="15" y2="23"/><line x1="20" y1="9" x2="23" y2="9"/><line x1="20" y1="14" x2="23" y2="14"/><line x1="1" y1="9" x2="4" y2="9"/><line x1="1" y1="14" x2="4" y2="14"/></svg>"#,
        // Settings — gear
        13 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.32 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/></svg>"#,
        // Terminals — terminal/command-line
        14 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg>"#,
        // Stacks — layers/stack
        16 => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2L2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/><path d="M2 12l10 5 10-5"/></svg>"#,
        _ => r#"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/></svg>"#,
    };
    view! { <span class="sidebar-item-icon" inner_html=svg></span> }
}

/// Keyboard shortcut letter for each nav item (matches Auto Claude reference).
fn shortcut_key(idx: usize) -> Option<&'static str> {
    match idx {
        0 => Some("D"),   // Dashboard
        1 => Some("K"),   // Kanban Board
        2 => Some("A"),   // Agent Terminals
        3 => Some("N"),   // Insights
        4 => Some("I"),   // Ideation
        5 => Some("R"),   // Roadmap
        6 => Some("L"),   // Changelog
        7 => Some("C"),   // Context
        8 => Some("M"),   // MCP Overview
        9 => Some("W"),   // Worktrees
        10 => Some("G"),  // GitHub Issues
        11 => Some("P"),  // GitHub PRs
        12 => None,       // Claude Code (bottom link, no shortcut)
        13 => None,       // Settings (bottom, no shortcut)
        14 => Some("T"),  // Terminals
        16 => Some("S"),  // Stacks
        _ => None,
    }
}

/// A single nav button with SVG icon and optional keyboard shortcut badge.
#[component]
fn NavItem(
    idx: usize,
    label: &'static str,
    current_tab: ReadSignal<usize>,
    set_current_tab: WriteSignal<usize>,
    sidebar_collapsed: ReadSignal<bool>,
) -> impl IntoView {
    let key = shortcut_key(idx);
    view! {
        <button
            class="sidebar-item"
            class:active=(move || current_tab.get() == idx)
            class:collapsed=(move || sidebar_collapsed.get())
            on:click=move |_| set_current_tab.set(idx)
            title=move || if sidebar_collapsed.get() { label } else { "" }
        >
            {nav_icon(idx)}
            <span class="sidebar-item-label" class:collapsed=(move || sidebar_collapsed.get())>{label}</span>
            {key.map(|k| view! { <span class="sidebar-shortcut-badge" class:collapsed=(move || sidebar_collapsed.get())>{k}</span> })}
        </button>
    }
}

#[component]
pub fn NavBar(
    current_tab: ReadSignal<usize>,
    set_current_tab: WriteSignal<usize>,
    on_new_task: WriteSignal<bool>,
) -> impl IntoView {
    // Sidebar collapse state
    let (sidebar_collapsed, set_sidebar_collapsed) = signal(false);
    
    // Toggle sidebar collapse
    let toggle_sidebar = move |_| {
        set_sidebar_collapsed.update(|collapsed| *collapsed = !*collapsed);
    };
    
    view! {
        <aside 
            class="sidebar" 
            class:collapsed=(move || sidebar_collapsed.get())
            aria-label="Main navigation"
        >
            <div class="sidebar-header">
                <button 
                    class="sidebar-toggle-btn"
                    title=move || if sidebar_collapsed.get() { "Expand sidebar" } else { "Collapse sidebar" }
                    on:click=toggle_sidebar
                >
                    <span class="sidebar-toggle-icon">
                        {move || if sidebar_collapsed.get() { "→" } else { "←" }}
                    </span>
                </button>
                
                <div class="sidebar-brand" class:collapsed=(move || sidebar_collapsed.get())>
                    <div class="sidebar-brand-icon" aria-hidden="true">"AT"</div>
                    <div class:collapsed=(move || sidebar_collapsed.get())>
                        <div class="sidebar-brand-name">"auto-tundra"</div>
                        <div class="sidebar-brand-badge">"v0.1.0"</div>
                    </div>
                </div>
            </div>

            <nav class="sidebar-nav" aria-label="Page navigation">
                <div class="sidebar-section-label" class:collapsed=(move || sidebar_collapsed.get())>"Project"</div>
                <NavItem idx=0 label="Dashboard" current_tab set_current_tab sidebar_collapsed />
                <NavItem idx=1 label="Kanban Board" current_tab set_current_tab sidebar_collapsed />
                <NavItem idx=2 label="Agent Terminals" current_tab set_current_tab sidebar_collapsed />
                <NavItem idx=3 label="Insights" current_tab set_current_tab sidebar_collapsed />

                <div class="sidebar-section-label" class:collapsed=(move || sidebar_collapsed.get())>"Planning"</div>
                <NavItem idx=4 label="Ideation" current_tab set_current_tab sidebar_collapsed />
                <NavItem idx=5 label="Roadmap" current_tab set_current_tab sidebar_collapsed />
                <NavItem idx=6 label="Changelog" current_tab set_current_tab sidebar_collapsed />
                <NavItem idx=7 label="Context" current_tab set_current_tab sidebar_collapsed />

                <div class="sidebar-section-label" class:collapsed=(move || sidebar_collapsed.get())>"Infrastructure"</div>
                <NavItem idx=8 label="MCP Overview" current_tab set_current_tab sidebar_collapsed />
                <NavItem idx=9 label="Worktrees" current_tab set_current_tab sidebar_collapsed />
                <NavItem idx=16 label="Stacks" current_tab set_current_tab sidebar_collapsed />

                <div class="sidebar-section-label" class:collapsed=(move || sidebar_collapsed.get())>"Integrations"</div>
                <NavItem idx=10 label="GitHub Issues" current_tab set_current_tab sidebar_collapsed />
                <NavItem idx=11 label="GitHub PRs" current_tab set_current_tab sidebar_collapsed />

                <div class="sidebar-section-label" class:collapsed=(move || sidebar_collapsed.get())>"System"</div>
                <NavItem idx=14 label="Terminals" current_tab set_current_tab sidebar_collapsed />
            </nav>

            <div class="sidebar-footer">
                <button
                    class="new-task-btn"
                    class:collapsed=(move || sidebar_collapsed.get())
                    aria-label=move || if sidebar_collapsed.get() { "Create new task" } else { "" }
                    title="Create new task"
                    on:click=move |_| on_new_task.set(true)
                >
                    <span class="new-task-btn-icon" aria-hidden="true">"+"</span>
                    <span class="new-task-btn-text" class:collapsed=(move || sidebar_collapsed.get())>" New Task"</span>
                </button>

                <button
                    class="sidebar-footer-link"
                    class:collapsed=(move || sidebar_collapsed.get())
                    on:click=move |_| set_current_tab.set(12)
                    class:active=(move || current_tab.get() == 12)
                    title="Claude Code"
                >
                    <span class="sidebar-footer-link-icon" inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6"/><line x1="9" y1="1" x2="9" y2="4"/><line x1="15" y1="1" x2="15" y2="4"/><line x1="9" y1="20" x2="9" y2="23"/><line x1="15" y1="20" x2="15" y2="23"/><line x1="20" y1="9" x2="23" y2="9"/><line x1="20" y1="14" x2="23" y2="14"/><line x1="1" y1="9" x2="4" y2="9"/><line x1="1" y1="14" x2="4" y2="14"/></svg>"#></span>
                    <span class="sidebar-footer-link-text" class:collapsed=(move || sidebar_collapsed.get())>"Claude Code"</span>
                </button>

                <button
                    class="sidebar-footer-link"
                    class:collapsed=(move || sidebar_collapsed.get())
                    on:click=move |_| set_current_tab.set(13)
                    class:active=(move || current_tab.get() == 13)
                    title="Settings"
                >
                    <span class="sidebar-footer-link-icon" inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.32 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/></svg>"#></span>
                    <span class="sidebar-footer-link-text" class:collapsed=(move || sidebar_collapsed.get())>"Settings"</span>
                    <span class="sidebar-help-icon" title="Help">
                        <span inner_html=r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 015.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>"#></span>
                    </span>
                </button>
            </div>
        </aside>
    }
}

/// Returns the label for a given tab index.
pub fn tab_label(idx: usize) -> &'static str {
    NAV_LABELS.get(idx).copied().unwrap_or("Kanban Board")
}
