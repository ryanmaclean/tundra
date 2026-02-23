//! Agent command system for the Auto-Tundra TUI.
//!
//! Makes the TUI programmable by agents via two interfaces:
//! - **Command mode**: `:` prefixed text commands typed interactively.
//! - **JSON pipe**: Structured JSON commands received over stdin from agents.
//!
//! Query commands return serialized state as JSON strings so that agents can
//! inspect the TUI without needing direct struct access.

use serde_json;

use crate::app::{App, TAB_NAMES};

// ---------------------------------------------------------------------------
// AppCommand enum
// ---------------------------------------------------------------------------

/// Commands that agents (or users in command mode) can issue to the TUI.
#[derive(Debug, Clone, PartialEq)]
pub enum AppCommand {
    // Navigation
    Tab(usize),
    NextTab,
    PrevTab,
    Select(usize),
    Up,
    Down,
    Left,
    Right,

    // Actions
    Refresh,
    Action(String),
    CreateBead(String),

    // Agent queries
    QueryState,
    QueryTab,
    QuerySelected,

    // System
    Quit,
    Help,
}

// ---------------------------------------------------------------------------
// Text command parser  (`:` prefixed)
// ---------------------------------------------------------------------------

/// Parse a `:` prefixed command string.
///
/// Examples: `:tab 3`, `:quit`, `:query state`, `:select 5`, `:up`.
pub fn parse_command(input: &str) -> Option<AppCommand> {
    let input = input.trim();
    let input = input.strip_prefix(':')?;
    let mut parts = input.splitn(2, ' ');
    let verb = parts.next()?.trim();
    let arg = parts.next().map(|s| s.trim());

    match verb {
        "tab" => {
            let idx: usize = arg?.parse().ok()?;
            Some(AppCommand::Tab(idx))
        }
        "next" | "nexttab" | "next_tab" => Some(AppCommand::NextTab),
        "prev" | "prevtab" | "prev_tab" => Some(AppCommand::PrevTab),
        "select" | "sel" => {
            let idx: usize = arg?.parse().ok()?;
            Some(AppCommand::Select(idx))
        }
        "up" | "k" => Some(AppCommand::Up),
        "down" | "j" => Some(AppCommand::Down),
        "left" | "h" => Some(AppCommand::Left),
        "right" | "l" => Some(AppCommand::Right),
        "refresh" | "r" => Some(AppCommand::Refresh),
        "action" => {
            let name = arg?;
            if name.is_empty() {
                return None;
            }
            Some(AppCommand::Action(name.to_string()))
        }
        "create_bead" | "bead" => {
            let title = arg?;
            if title.is_empty() {
                return None;
            }
            Some(AppCommand::CreateBead(title.to_string()))
        }
        "query" => match arg? {
            "state" => Some(AppCommand::QueryState),
            "tab" => Some(AppCommand::QueryTab),
            "selected" => Some(AppCommand::QuerySelected),
            _ => None,
        },
        "quit" | "q" => Some(AppCommand::Quit),
        "help" | "?" => Some(AppCommand::Help),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// JSON command parser
// ---------------------------------------------------------------------------

/// Parse a JSON command from an agent pipe.
///
/// Expected format: `{"cmd":"tab","args":[3]}` or `{"cmd":"query_state"}`.
pub fn parse_json_command(json: &str) -> Option<AppCommand> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let cmd = v.get("cmd")?.as_str()?;
    let args = v.get("args");

    let arg_usize =
        |idx: usize| -> Option<usize> { args?.as_array()?.get(idx)?.as_u64().map(|n| n as usize) };
    let arg_str = |idx: usize| -> Option<&str> { args?.as_array()?.get(idx)?.as_str() };

    match cmd {
        "tab" => Some(AppCommand::Tab(arg_usize(0)?)),
        "next_tab" | "nexttab" => Some(AppCommand::NextTab),
        "prev_tab" | "prevtab" => Some(AppCommand::PrevTab),
        "select" => Some(AppCommand::Select(arg_usize(0)?)),
        "up" => Some(AppCommand::Up),
        "down" => Some(AppCommand::Down),
        "left" => Some(AppCommand::Left),
        "right" => Some(AppCommand::Right),
        "refresh" => Some(AppCommand::Refresh),
        "action" => Some(AppCommand::Action(arg_str(0)?.to_string())),
        "create_bead" => Some(AppCommand::CreateBead(arg_str(0)?.to_string())),
        "query_state" => Some(AppCommand::QueryState),
        "query_tab" => Some(AppCommand::QueryTab),
        "query_selected" => Some(AppCommand::QuerySelected),
        "quit" => Some(AppCommand::Quit),
        "help" => Some(AppCommand::Help),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Command execution
// ---------------------------------------------------------------------------

/// Execute a command against the application state.
///
/// Returns `Some(json_string)` for query commands, `None` for everything else.
pub fn execute_command(app: &mut App, cmd: AppCommand) -> Option<String> {
    match cmd {
        // -- Navigation -----------------------------------------------------
        AppCommand::Tab(idx) => {
            if idx < TAB_NAMES.len() {
                app.current_tab = idx;
                app.selected_index = 0;
            }
            None
        }
        AppCommand::NextTab => {
            app.current_tab = (app.current_tab + 1) % TAB_NAMES.len();
            app.selected_index = 0;
            None
        }
        AppCommand::PrevTab => {
            app.current_tab = if app.current_tab == 0 {
                TAB_NAMES.len() - 1
            } else {
                app.current_tab - 1
            };
            app.selected_index = 0;
            None
        }
        AppCommand::Select(idx) => {
            app.selected_index = idx;
            None
        }
        AppCommand::Up => {
            if app.selected_index > 0 {
                app.selected_index -= 1;
            }
            None
        }
        AppCommand::Down => {
            app.selected_index += 1;
            None
        }
        AppCommand::Left => {
            if app.current_tab == 2 && app.kanban_column > 0 {
                app.kanban_column -= 1;
            }
            None
        }
        AppCommand::Right => {
            if app.current_tab == 2 && app.kanban_column < 4 {
                app.kanban_column += 1;
            }
            None
        }

        // -- Actions --------------------------------------------------------
        AppCommand::Refresh => {
            // Handled by caller (triggers data reload).
            None
        }
        AppCommand::Action(_name) => {
            // Delegate to appropriate action handler in the future.
            None
        }
        AppCommand::CreateBead(_title) => {
            // Delegate to bead creation logic in the future.
            None
        }

        // -- Queries --------------------------------------------------------
        AppCommand::QueryState => {
            let tab_name = TAB_NAMES.get(app.current_tab).unwrap_or(&"unknown");
            let state = serde_json::json!({
                "current_tab": app.current_tab,
                "tab_name": tab_name,
                "selected_index": app.selected_index,
                "api_connected": app.api_connected,
                "offline": app.offline,
                "counts": {
                    "agents": app.agents.len(),
                    "beads": app.beads.len(),
                    "sessions": app.sessions.len(),
                    "convoys": app.convoys.len(),
                    "costs": app.costs.len(),
                    "mcp_servers": app.mcp_servers.len(),
                    "worktrees": app.worktrees.len(),
                    "github_issues": app.github_issues.len(),
                    "github_prs": app.github_prs.len(),
                    "roadmap_items": app.roadmap_items.len(),
                    "ideas": app.ideas.len(),
                    "stacks": app.stacks.len(),
                    "changelog": app.changelog.len(),
                    "memory_entries": app.memory_entries.len(),
                }
            });
            Some(serde_json::to_string(&state).unwrap())
        }
        AppCommand::QueryTab => {
            let data = serialize_tab_data(app);
            Some(serde_json::to_string(&data).unwrap())
        }
        AppCommand::QuerySelected => {
            let data = serialize_selected_item(app);
            Some(serde_json::to_string(&data).unwrap())
        }

        // -- System ---------------------------------------------------------
        AppCommand::Quit => {
            app.should_quit = true;
            None
        }
        AppCommand::Help => {
            app.show_help = true;
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Serialization helpers (using json! macro to avoid Serialize derives)
// ---------------------------------------------------------------------------

fn serialize_tab_data(app: &App) -> serde_json::Value {
    match app.current_tab {
        0 | 1 => {
            // Dashboard / Agents
            serde_json::json!(app
                .agents
                .iter()
                .map(|a| serde_json::json!({
                    "name": a.name,
                    "role": format!("{:?}", a.role),
                    "status": format!("{:?}", a.status),
                    "cli_type": format!("{:?}", a.cli_type),
                    "model": a.model,
                    "last_seen": a.last_seen.to_rfc3339(),
                }))
                .collect::<Vec<_>>())
        }
        2 => {
            // Beads
            serde_json::json!(app
                .beads
                .iter()
                .map(|b| serde_json::json!({
                    "id": b.id,
                    "title": b.title,
                    "status": format!("{:?}", b.status),
                    "lane": format!("{:?}", b.lane),
                }))
                .collect::<Vec<_>>())
        }
        3 => {
            // Sessions
            serde_json::json!(app
                .sessions
                .iter()
                .map(|s| serde_json::json!({
                    "id": s.id,
                    "agent": s.agent,
                    "cli_type": format!("{:?}", s.cli_type),
                    "status": s.status,
                    "duration": s.duration,
                    "cpu": s.cpu,
                }))
                .collect::<Vec<_>>())
        }
        4 => {
            // Convoys
            serde_json::json!(app
                .convoys
                .iter()
                .map(|c| serde_json::json!({
                    "name": c.name,
                    "status": format!("{:?}", c.status),
                    "bead_count": c.bead_count,
                    "progress": c.progress,
                }))
                .collect::<Vec<_>>())
        }
        5 => {
            // Costs
            serde_json::json!(app
                .costs
                .iter()
                .map(|c| serde_json::json!({
                    "provider": c.provider,
                    "model": c.model,
                    "input_tokens": c.input_tokens,
                    "output_tokens": c.output_tokens,
                    "cost_usd": c.cost_usd,
                }))
                .collect::<Vec<_>>())
        }
        8 => {
            // MCP
            serde_json::json!(app
                .mcp_servers
                .iter()
                .map(|m| serde_json::json!({
                    "name": m.name,
                    "transport": m.transport,
                    "status": m.status,
                    "tools": m.tools,
                }))
                .collect::<Vec<_>>())
        }
        9 => {
            // Roadmap
            serde_json::json!(app
                .roadmap_items
                .iter()
                .map(|r| serde_json::json!({
                    "id": r.id,
                    "title": r.title,
                    "description": r.description,
                    "status": r.status,
                    "priority": r.priority,
                }))
                .collect::<Vec<_>>())
        }
        10 => {
            // Ideation
            serde_json::json!(app
                .ideas
                .iter()
                .map(|i| serde_json::json!({
                    "id": i.id,
                    "title": i.title,
                    "description": i.description,
                    "category": i.category,
                    "impact": i.impact,
                    "effort": i.effort,
                }))
                .collect::<Vec<_>>())
        }
        11 => {
            // Worktrees
            serde_json::json!(app
                .worktrees
                .iter()
                .map(|w| serde_json::json!({
                    "id": w.id,
                    "path": w.path,
                    "branch": w.branch,
                    "bead_id": w.bead_id,
                    "status": w.status,
                }))
                .collect::<Vec<_>>())
        }
        12 => {
            // GitHub Issues
            serde_json::json!(app
                .github_issues
                .iter()
                .map(|i| serde_json::json!({
                    "number": i.number,
                    "title": i.title,
                    "labels": i.labels,
                    "assignee": i.assignee,
                    "state": i.state,
                    "created": i.created,
                }))
                .collect::<Vec<_>>())
        }
        13 => {
            // GitHub PRs
            serde_json::json!(app
                .github_prs
                .iter()
                .map(|p| serde_json::json!({
                    "number": p.number,
                    "title": p.title,
                    "author": p.author,
                    "status": p.status,
                    "reviewers": p.reviewers,
                    "created": p.created,
                }))
                .collect::<Vec<_>>())
        }
        14 => {
            // Stacks
            serde_json::json!(app
                .stacks
                .iter()
                .map(|s| serde_json::json!({
                    "id": s.id,
                    "title": s.title,
                    "phase": s.phase,
                    "git_branch": s.git_branch,
                    "pr_number": s.pr_number,
                    "depth": s.depth,
                }))
                .collect::<Vec<_>>())
        }
        15 => {
            // Context (memory entries)
            serde_json::json!(app
                .memory_entries
                .iter()
                .map(|m| serde_json::json!({
                    "id": m.id,
                    "category": m.category,
                    "content": m.content,
                    "created_at": m.created_at,
                }))
                .collect::<Vec<_>>())
        }
        16 => {
            // Changelog
            serde_json::json!(app
                .changelog
                .iter()
                .map(|c| serde_json::json!({
                    "version": c.version,
                    "date": c.date,
                    "sections": c.sections.iter().map(|(cat, items)| {
                        serde_json::json!({ "category": cat, "items": items })
                    }).collect::<Vec<_>>(),
                    "expanded": c.expanded,
                }))
                .collect::<Vec<_>>())
        }
        _ => {
            // Analytics (6), Config (7), or unknown tabs
            serde_json::json!([])
        }
    }
}

fn serialize_selected_item(app: &App) -> serde_json::Value {
    let idx = app.selected_index;
    match app.current_tab {
        0 | 1 => app.agents.get(idx).map(|a| {
            serde_json::json!({
                "name": a.name,
                "role": format!("{:?}", a.role),
                "status": format!("{:?}", a.status),
                "cli_type": format!("{:?}", a.cli_type),
                "model": a.model,
                "last_seen": a.last_seen.to_rfc3339(),
            })
        }),
        2 => app.beads.get(idx).map(|b| {
            serde_json::json!({
                "id": b.id,
                "title": b.title,
                "status": format!("{:?}", b.status),
                "lane": format!("{:?}", b.lane),
            })
        }),
        3 => app.sessions.get(idx).map(|s| {
            serde_json::json!({
                "id": s.id,
                "agent": s.agent,
                "cli_type": format!("{:?}", s.cli_type),
                "status": s.status,
                "duration": s.duration,
                "cpu": s.cpu,
            })
        }),
        4 => app.convoys.get(idx).map(|c| {
            serde_json::json!({
                "name": c.name,
                "status": format!("{:?}", c.status),
                "bead_count": c.bead_count,
                "progress": c.progress,
            })
        }),
        5 => app.costs.get(idx).map(|c| {
            serde_json::json!({
                "provider": c.provider,
                "model": c.model,
                "input_tokens": c.input_tokens,
                "output_tokens": c.output_tokens,
                "cost_usd": c.cost_usd,
            })
        }),
        8 => app.mcp_servers.get(idx).map(|m| {
            serde_json::json!({
                "name": m.name,
                "transport": m.transport,
                "status": m.status,
                "tools": m.tools,
            })
        }),
        9 => app.roadmap_items.get(idx).map(|r| {
            serde_json::json!({
                "id": r.id,
                "title": r.title,
                "description": r.description,
                "status": r.status,
                "priority": r.priority,
            })
        }),
        10 => app.ideas.get(idx).map(|i| {
            serde_json::json!({
                "id": i.id,
                "title": i.title,
                "description": i.description,
                "category": i.category,
                "impact": i.impact,
                "effort": i.effort,
            })
        }),
        11 => app.worktrees.get(idx).map(|w| {
            serde_json::json!({
                "id": w.id,
                "path": w.path,
                "branch": w.branch,
                "bead_id": w.bead_id,
                "status": w.status,
            })
        }),
        12 => app.github_issues.get(idx).map(|i| {
            serde_json::json!({
                "number": i.number,
                "title": i.title,
                "labels": i.labels,
                "assignee": i.assignee,
                "state": i.state,
                "created": i.created,
            })
        }),
        13 => app.github_prs.get(idx).map(|p| {
            serde_json::json!({
                "number": p.number,
                "title": p.title,
                "author": p.author,
                "status": p.status,
                "reviewers": p.reviewers,
                "created": p.created,
            })
        }),
        14 => app.stacks.get(idx).map(|s| {
            serde_json::json!({
                "id": s.id,
                "title": s.title,
                "phase": s.phase,
                "git_branch": s.git_branch,
                "pr_number": s.pr_number,
                "depth": s.depth,
            })
        }),
        15 => app.memory_entries.get(idx).map(|m| {
            serde_json::json!({
                "id": m.id,
                "category": m.category,
                "content": m.content,
                "created_at": m.created_at,
            })
        }),
        16 => app.changelog.get(idx).map(|c| {
            serde_json::json!({
                "version": c.version,
                "date": c.date,
                "sections": c.sections.iter().map(|(cat, items)| {
                    serde_json::json!({ "category": cat, "items": items })
                }).collect::<Vec<_>>(),
                "expanded": c.expanded,
            })
        }),
        _ => None,
    }
    .unwrap_or(serde_json::json!(null))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        App::new(true)
    }

    // -- parse_command ------------------------------------------------------

    #[test]
    fn parse_tab_command() {
        assert_eq!(parse_command(":tab 3"), Some(AppCommand::Tab(3)));
        assert_eq!(parse_command(":tab 0"), Some(AppCommand::Tab(0)));
        assert_eq!(parse_command(":tab 16"), Some(AppCommand::Tab(16)));
    }

    #[test]
    fn parse_tab_command_invalid() {
        assert_eq!(parse_command(":tab"), None);
        assert_eq!(parse_command(":tab abc"), None);
    }

    #[test]
    fn parse_navigation_commands() {
        assert_eq!(parse_command(":next"), Some(AppCommand::NextTab));
        assert_eq!(parse_command(":nexttab"), Some(AppCommand::NextTab));
        assert_eq!(parse_command(":prev"), Some(AppCommand::PrevTab));
        assert_eq!(parse_command(":prevtab"), Some(AppCommand::PrevTab));
        assert_eq!(parse_command(":up"), Some(AppCommand::Up));
        assert_eq!(parse_command(":down"), Some(AppCommand::Down));
        assert_eq!(parse_command(":left"), Some(AppCommand::Left));
        assert_eq!(parse_command(":right"), Some(AppCommand::Right));
        assert_eq!(parse_command(":k"), Some(AppCommand::Up));
        assert_eq!(parse_command(":j"), Some(AppCommand::Down));
    }

    #[test]
    fn parse_select_command() {
        assert_eq!(parse_command(":select 5"), Some(AppCommand::Select(5)));
        assert_eq!(parse_command(":sel 0"), Some(AppCommand::Select(0)));
        assert_eq!(parse_command(":select"), None);
    }

    #[test]
    fn parse_query_commands() {
        assert_eq!(parse_command(":query state"), Some(AppCommand::QueryState));
        assert_eq!(parse_command(":query tab"), Some(AppCommand::QueryTab));
        assert_eq!(
            parse_command(":query selected"),
            Some(AppCommand::QuerySelected)
        );
        assert_eq!(parse_command(":query invalid"), None);
    }

    #[test]
    fn parse_action_commands() {
        assert_eq!(
            parse_command(":action deploy"),
            Some(AppCommand::Action("deploy".into()))
        );
        assert_eq!(parse_command(":action"), None);
        assert_eq!(parse_command(":refresh"), Some(AppCommand::Refresh));
        assert_eq!(parse_command(":r"), Some(AppCommand::Refresh));
    }

    #[test]
    fn parse_create_bead_command() {
        assert_eq!(
            parse_command(":bead Fix the login page"),
            Some(AppCommand::CreateBead("Fix the login page".into()))
        );
        assert_eq!(
            parse_command(":create_bead Add tests"),
            Some(AppCommand::CreateBead("Add tests".into()))
        );
        assert_eq!(parse_command(":bead"), None);
    }

    #[test]
    fn parse_system_commands() {
        assert_eq!(parse_command(":quit"), Some(AppCommand::Quit));
        assert_eq!(parse_command(":q"), Some(AppCommand::Quit));
        assert_eq!(parse_command(":help"), Some(AppCommand::Help));
        assert_eq!(parse_command(":?"), Some(AppCommand::Help));
    }

    #[test]
    fn parse_no_colon_prefix() {
        assert_eq!(parse_command("tab 3"), None);
        assert_eq!(parse_command("quit"), None);
    }

    #[test]
    fn parse_unknown_command() {
        assert_eq!(parse_command(":foobar"), None);
    }

    #[test]
    fn parse_whitespace_handling() {
        assert_eq!(parse_command("  :tab 3  "), Some(AppCommand::Tab(3)));
        assert_eq!(parse_command(":quit  "), Some(AppCommand::Quit));
    }

    // -- parse_json_command -------------------------------------------------

    #[test]
    fn parse_json_tab() {
        assert_eq!(
            parse_json_command(r#"{"cmd":"tab","args":[3]}"#),
            Some(AppCommand::Tab(3))
        );
    }

    #[test]
    fn parse_json_navigation() {
        assert_eq!(
            parse_json_command(r#"{"cmd":"next_tab"}"#),
            Some(AppCommand::NextTab)
        );
        assert_eq!(
            parse_json_command(r#"{"cmd":"prev_tab"}"#),
            Some(AppCommand::PrevTab)
        );
        assert_eq!(parse_json_command(r#"{"cmd":"up"}"#), Some(AppCommand::Up));
        assert_eq!(
            parse_json_command(r#"{"cmd":"down"}"#),
            Some(AppCommand::Down)
        );
    }

    #[test]
    fn parse_json_select() {
        assert_eq!(
            parse_json_command(r#"{"cmd":"select","args":[7]}"#),
            Some(AppCommand::Select(7))
        );
    }

    #[test]
    fn parse_json_query() {
        assert_eq!(
            parse_json_command(r#"{"cmd":"query_state"}"#),
            Some(AppCommand::QueryState)
        );
        assert_eq!(
            parse_json_command(r#"{"cmd":"query_tab"}"#),
            Some(AppCommand::QueryTab)
        );
        assert_eq!(
            parse_json_command(r#"{"cmd":"query_selected"}"#),
            Some(AppCommand::QuerySelected)
        );
    }

    #[test]
    fn parse_json_action() {
        assert_eq!(
            parse_json_command(r#"{"cmd":"action","args":["deploy"]}"#),
            Some(AppCommand::Action("deploy".into()))
        );
    }

    #[test]
    fn parse_json_create_bead() {
        assert_eq!(
            parse_json_command(r#"{"cmd":"create_bead","args":["Fix login"]}"#),
            Some(AppCommand::CreateBead("Fix login".into()))
        );
    }

    #[test]
    fn parse_json_system() {
        assert_eq!(
            parse_json_command(r#"{"cmd":"quit"}"#),
            Some(AppCommand::Quit)
        );
        assert_eq!(
            parse_json_command(r#"{"cmd":"help"}"#),
            Some(AppCommand::Help)
        );
    }

    #[test]
    fn parse_json_invalid() {
        assert_eq!(parse_json_command("not json"), None);
        assert_eq!(parse_json_command(r#"{"cmd":"unknown"}"#), None);
        assert_eq!(parse_json_command(r#"{"no_cmd":true}"#), None);
    }

    // -- execute_command: navigation ----------------------------------------

    #[test]
    fn execute_tab_navigation() {
        let mut app = test_app();
        assert_eq!(app.current_tab, 0);

        let result = execute_command(&mut app, AppCommand::Tab(5));
        assert!(result.is_none());
        assert_eq!(app.current_tab, 5);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn execute_tab_out_of_bounds() {
        let mut app = test_app();
        execute_command(&mut app, AppCommand::Tab(999));
        // Should not change when out of bounds.
        assert_eq!(app.current_tab, 0);
    }

    #[test]
    fn execute_next_prev_tab() {
        let mut app = test_app();
        execute_command(&mut app, AppCommand::NextTab);
        assert_eq!(app.current_tab, 1);
        execute_command(&mut app, AppCommand::PrevTab);
        assert_eq!(app.current_tab, 0);
        // Wrap around backwards
        execute_command(&mut app, AppCommand::PrevTab);
        assert_eq!(app.current_tab, TAB_NAMES.len() - 1);
        // Wrap around forwards
        execute_command(&mut app, AppCommand::NextTab);
        assert_eq!(app.current_tab, 0);
    }

    #[test]
    fn execute_up_down() {
        let mut app = test_app();
        // Tab 1 (Agents) has demo data.
        execute_command(&mut app, AppCommand::Tab(1));
        assert_eq!(app.selected_index, 0);

        execute_command(&mut app, AppCommand::Down);
        assert_eq!(app.selected_index, 1);

        execute_command(&mut app, AppCommand::Down);
        assert_eq!(app.selected_index, 2);

        execute_command(&mut app, AppCommand::Up);
        assert_eq!(app.selected_index, 1);

        // Up at 0 stays at 0.
        execute_command(&mut app, AppCommand::Up);
        execute_command(&mut app, AppCommand::Up);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn execute_select() {
        let mut app = test_app();
        execute_command(&mut app, AppCommand::Select(3));
        assert_eq!(app.selected_index, 3);
    }

    #[test]
    fn execute_left_right_kanban() {
        let mut app = test_app();
        // Switch to Beads tab (kanban)
        execute_command(&mut app, AppCommand::Tab(2));
        assert_eq!(app.kanban_column, 0);

        execute_command(&mut app, AppCommand::Right);
        assert_eq!(app.kanban_column, 1);

        execute_command(&mut app, AppCommand::Left);
        assert_eq!(app.kanban_column, 0);

        // Left at 0 stays at 0
        execute_command(&mut app, AppCommand::Left);
        assert_eq!(app.kanban_column, 0);
    }

    // -- execute_command: system --------------------------------------------

    #[test]
    fn execute_quit() {
        let mut app = test_app();
        assert!(!app.should_quit);
        execute_command(&mut app, AppCommand::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn execute_help() {
        let mut app = test_app();
        assert!(!app.show_help);
        execute_command(&mut app, AppCommand::Help);
        assert!(app.show_help);
    }

    // -- execute_command: queries -------------------------------------------

    #[test]
    fn execute_query_state() {
        let mut app = test_app();
        app.current_tab = 2;
        let result = execute_command(&mut app, AppCommand::QueryState);
        assert!(result.is_some());

        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(json["current_tab"], 2);
        assert_eq!(json["tab_name"], "Beads");
        assert!(json["counts"]["agents"].as_u64().unwrap() > 0);
        assert!(json["counts"]["beads"].as_u64().unwrap() > 0);
    }

    #[test]
    fn execute_query_tab() {
        let mut app = test_app();
        app.current_tab = 1; // Agents
        let result = execute_command(&mut app, AppCommand::QueryTab);
        assert!(result.is_some());

        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let arr = json.as_array().unwrap();
        assert!(!arr.is_empty());
        assert!(arr[0]["name"].as_str().is_some());
        assert!(arr[0]["role"].as_str().is_some());
    }

    #[test]
    fn execute_query_selected() {
        let mut app = test_app();
        app.current_tab = 1; // Agents
        app.selected_index = 0;
        let result = execute_command(&mut app, AppCommand::QuerySelected);
        assert!(result.is_some());

        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(json["name"].as_str().is_some());
    }

    #[test]
    fn execute_query_selected_out_of_bounds() {
        let mut app = test_app();
        app.current_tab = 1;
        app.selected_index = 999;
        let result = execute_command(&mut app, AppCommand::QuerySelected);
        assert!(result.is_some());

        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(json.is_null());
    }

    #[test]
    fn execute_query_tab_empty() {
        let mut app = test_app();
        app.current_tab = 6; // Analytics (empty)
        let result = execute_command(&mut app, AppCommand::QueryTab);
        assert!(result.is_some());

        let json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        let arr = json.as_array().unwrap();
        assert!(arr.is_empty());
    }

    // -- round-trip: parse then execute -------------------------------------

    #[test]
    fn roundtrip_text_query() {
        let mut app = test_app();
        let cmd = parse_command(":query state").unwrap();
        let result = execute_command(&mut app, cmd);
        assert!(result.is_some());
    }

    #[test]
    fn roundtrip_json_query() {
        let mut app = test_app();
        let cmd = parse_json_command(r#"{"cmd":"query_state"}"#).unwrap();
        let result = execute_command(&mut app, cmd);
        assert!(result.is_some());
    }
}
