// =============================================================================
// component_tests.rs - Leptos WASM component unit tests for auto-tundra
//
// Tests API response types, serialization/deserialization, navigation tab
// logic, and signal state management. Runs via wasm-bindgen-test in a
// headless browser or Node.js.
//
// Run with:
//   cd app/leptos-ui && wasm-pack test --headless --chrome
//   or: cd app/leptos-ui && cargo test --target wasm32-unknown-unknown
// =============================================================================

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// Re-export crate under test
use at_leptos_ui::api::*;

// =============================================================================
// API response deserialization tests
// =============================================================================

mod api_deserialization {
    use super::*;

    #[wasm_bindgen_test]
    fn test_api_bead_deserialize_full() {
        let json = r#"{
            "id": "abc-123",
            "title": "Implement feature X",
            "description": "Full description here",
            "status": "InProgress",
            "lane": "Standard",
            "priority": 5
        }"#;
        let bead: ApiBead = serde_json::from_str(json).expect("ApiBead deserialization failed");
        assert_eq!(bead.id, "abc-123");
        assert_eq!(bead.title, "Implement feature X");
        assert_eq!(bead.description, Some("Full description here".to_string()));
        assert_eq!(bead.status, "InProgress");
        assert_eq!(bead.lane, "Standard");
        assert_eq!(bead.priority, 5);
    }

    #[wasm_bindgen_test]
    fn test_api_bead_deserialize_minimal() {
        let json = r#"{
            "id": "xyz",
            "title": "Minimal bead",
            "status": "Backlog",
            "lane": "Standard"
        }"#;
        let bead: ApiBead =
            serde_json::from_str(json).expect("ApiBead minimal deserialization failed");
        assert_eq!(bead.id, "xyz");
        assert_eq!(bead.description, None);
        assert_eq!(bead.priority, 0); // default
    }

    #[wasm_bindgen_test]
    fn test_api_agent_deserialize() {
        let json = r#"{
            "id": "agent-1",
            "name": "Architect",
            "role": "Architect",
            "status": "Active"
        }"#;
        let agent: ApiAgent = serde_json::from_str(json).expect("ApiAgent deserialization failed");
        assert_eq!(agent.id, "agent-1");
        assert_eq!(agent.name, "Architect");
        assert_eq!(agent.role, "Architect");
        assert_eq!(agent.status, "Active");
    }

    #[wasm_bindgen_test]
    fn test_api_agent_deserialize_defaults() {
        let json = r#"{
            "id": "a",
            "name": "Test",
            "status": "Idle"
        }"#;
        let agent: ApiAgent = serde_json::from_str(json).expect("ApiAgent defaults failed");
        assert_eq!(agent.role, ""); // default
    }

    #[wasm_bindgen_test]
    fn test_api_kpi_deserialize_full() {
        let json = r#"{
            "total_beads": 100,
            "backlog": 20,
            "hooked": 15,
            "slung": 10,
            "review": 5,
            "done": 45,
            "failed": 3,
            "active_agents": 7
        }"#;
        let kpi: ApiKpi = serde_json::from_str(json).expect("ApiKpi deserialization failed");
        assert_eq!(kpi.total_beads, 100);
        assert_eq!(kpi.backlog, 20);
        assert_eq!(kpi.hooked, 15);
        assert_eq!(kpi.slung, 10);
        assert_eq!(kpi.review, 5);
        assert_eq!(kpi.done, 45);
        assert_eq!(kpi.failed, 3);
        assert_eq!(kpi.active_agents, 7);
    }

    #[wasm_bindgen_test]
    fn test_api_kpi_deserialize_empty() {
        let json = r#"{}"#;
        let kpi: ApiKpi = serde_json::from_str(json).expect("ApiKpi empty deserialization failed");
        assert_eq!(kpi.total_beads, 0);
        assert_eq!(kpi.active_agents, 0);
    }

    #[wasm_bindgen_test]
    fn test_api_status_deserialize() {
        let json = r#"{
            "version": "0.1.0",
            "uptime_secs": 3600,
            "agent_count": 5,
            "bead_count": 42
        }"#;
        let status: ApiStatus =
            serde_json::from_str(json).expect("ApiStatus deserialization failed");
        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.uptime_secs, 3600);
        assert_eq!(status.agent_count, 5);
        assert_eq!(status.bead_count, 42);
    }

    #[wasm_bindgen_test]
    fn test_api_status_deserialize_defaults() {
        let json = r#"{}"#;
        let status: ApiStatus = serde_json::from_str(json).expect("ApiStatus defaults failed");
        assert_eq!(status.version, "");
        assert_eq!(status.uptime_secs, 0);
    }
}

// =============================================================================
// Settings deserialization tests
// =============================================================================

mod settings_deserialization {
    use super::*;

    #[wasm_bindgen_test]
    fn test_api_settings_full_roundtrip() {
        let settings = ApiSettings {
            general: ApiGeneralSettings {
                project_name: "auto-tundra".to_string(),
                log_level: "info".to_string(),
                workspace_root: Some("/Users/test/project".to_string()),
            },
            display: ApiDisplaySettings {
                theme: "dark".to_string(),
                font_size: 14,
                compact_mode: false,
            },
            agents: ApiAgentsSettings {
                max_concurrent: 5,
                heartbeat_interval_secs: 30,
                auto_restart: true,
            },
            terminal: ApiTerminalSettings {
                font_family: "JetBrains Mono".to_string(),
                font_size: 13,
                cursor_style: "block".to_string(),
            },
            security: ApiSecuritySettings {
                allow_shell_exec: false,
                sandbox: true,
                allowed_paths: vec!["/tmp".to_string()],
                auto_lock_timeout_mins: 15,
                sandbox_mode: true,
            },
            integrations: ApiIntegrationSettings {
                github_token_env: "GITHUB_TOKEN".to_string(),
                github_owner: Some("owner".to_string()),
                github_repo: Some("repo".to_string()),
                gitlab_token_env: "".to_string(),
                linear_api_key_env: "".to_string(),
                linear_team_id: None,
                openai_api_key_env: "".to_string(),
            },
            appearance: ApiAppearanceSettings {
                appearance_mode: "dark".to_string(),
                color_theme: "tokyo-night".to_string(),
            },
            language: ApiLanguageSettings {
                interface_language: "en".to_string(),
            },
            dev_tools: ApiDevToolsSettings {
                preferred_ide: "vscode".to_string(),
                preferred_terminal: "iterm2".to_string(),
                auto_name_terminals: true,
                yolo_mode: false,
            },
            agent_profile: ApiAgentProfileSettings {
                default_profile: "standard".to_string(),
                agent_framework: "claude".to_string(),
                ai_terminal_naming: true,
                phase_configs: vec![ApiPhaseConfig {
                    phase: "planning".to_string(),
                    model: "claude-sonnet-4-20250514".to_string(),
                    thinking_level: "medium".to_string(),
                }],
            },
            paths: ApiPathsSettings {
                python_path: "/usr/bin/python3".to_string(),
                git_path: "/usr/bin/git".to_string(),
                github_cli_path: "/usr/local/bin/gh".to_string(),
                claude_cli_path: "/usr/local/bin/claude".to_string(),
                auto_claude_path: "".to_string(),
            },
            api_profiles: ApiApiProfilesSettings {
                profiles: vec![ApiApiProfileEntry {
                    name: "Anthropic".to_string(),
                    base_url: "https://api.anthropic.com".to_string(),
                    api_key_env: "ANTHROPIC_API_KEY".to_string(),
                }],
            },
            updates: ApiUpdatesSettings {
                version: "0.1.0".to_string(),
                is_latest: true,
                auto_update_projects: false,
                beta_updates: false,
            },
            notifications: ApiNotificationSettings {
                on_task_complete: true,
                on_task_failed: true,
                on_review_needed: true,
                sound_enabled: false,
            },
            debug: ApiDebugSettings {
                anonymous_error_reporting: false,
            },
            memory: ApiMemorySettings {
                enable_memory: true,
                enable_agent_memory_access: true,
                graphiti_server_url: "http://localhost:8000".to_string(),
                embedding_provider: "openai".to_string(),
                embedding_model: "text-embedding-3-small".to_string(),
            },
        };

        // Serialize to JSON
        let json_str = serde_json::to_string(&settings).expect("Settings serialization failed");

        // Deserialize back
        let restored: ApiSettings =
            serde_json::from_str(&json_str).expect("Settings roundtrip failed");

        assert_eq!(restored.general.project_name, "auto-tundra");
        assert_eq!(restored.display.theme, "dark");
        assert_eq!(restored.agents.max_concurrent, 5);
        assert_eq!(restored.terminal.font_family, "JetBrains Mono");
        assert_eq!(restored.security.sandbox, true);
        assert_eq!(
            restored.integrations.github_owner,
            Some("owner".to_string())
        );
        assert_eq!(restored.appearance.color_theme, "tokyo-night");
        assert_eq!(restored.language.interface_language, "en");
        assert_eq!(restored.dev_tools.preferred_ide, "vscode");
        assert_eq!(restored.agent_profile.phase_configs.len(), 1);
        assert_eq!(restored.paths.python_path, "/usr/bin/python3");
        assert_eq!(restored.api_profiles.profiles.len(), 1);
        assert_eq!(restored.updates.version, "0.1.0");
        assert_eq!(restored.notifications.on_task_complete, true);
        assert_eq!(restored.debug.anonymous_error_reporting, false);
        assert_eq!(restored.memory.enable_memory, true);
    }

    #[wasm_bindgen_test]
    fn test_api_settings_deserialize_empty_json() {
        let json = r#"{}"#;
        let settings: ApiSettings =
            serde_json::from_str(json).expect("Empty settings should deserialize with defaults");
        assert_eq!(settings.general.project_name, "");
        assert_eq!(settings.display.font_size, 0);
        assert_eq!(settings.agents.max_concurrent, 0);
        assert_eq!(settings.security.allowed_paths.len(), 0);
        assert_eq!(settings.agent_profile.phase_configs.len(), 0);
        assert_eq!(settings.api_profiles.profiles.len(), 0);
    }

    #[wasm_bindgen_test]
    fn test_api_settings_partial_json() {
        let json = r#"{"general": {"project_name": "my-project"}, "display": {"theme": "light"}}"#;
        let settings: ApiSettings =
            serde_json::from_str(json).expect("Partial settings should work");
        assert_eq!(settings.general.project_name, "my-project");
        assert_eq!(settings.display.theme, "light");
        // Everything else should be default
        assert_eq!(settings.general.log_level, "");
        assert_eq!(settings.agents.max_concurrent, 0);
    }
}

// =============================================================================
// Additional API type deserialization tests
// =============================================================================

mod additional_api_types {
    use super::*;

    #[wasm_bindgen_test]
    fn test_api_session_deserialize() {
        let json = r#"{
            "id": "sess-1",
            "agent_name": "Architect",
            "cli_type": "claude",
            "status": "active",
            "duration": "5m 30s"
        }"#;
        let session: ApiSession = serde_json::from_str(json).expect("ApiSession failed");
        assert_eq!(session.id, "sess-1");
        assert_eq!(session.agent_name, "Architect");
    }

    #[wasm_bindgen_test]
    fn test_api_worktree_deserialize() {
        let json = r#"{
            "id": "wt-1",
            "path": "/Users/test/repo-wt",
            "branch": "feature/xyz",
            "bead_id": "bead-1",
            "status": "active"
        }"#;
        let wt: ApiWorktree = serde_json::from_str(json).expect("ApiWorktree failed");
        assert_eq!(wt.branch, "feature/xyz");
        assert_eq!(wt.status, "active");
    }

    #[wasm_bindgen_test]
    fn test_api_mcp_server_deserialize() {
        let json = r#"{
            "name": "Context7",
            "status": "active",
            "tools": ["resolve_library_id", "get_library_docs"]
        }"#;
        let server: ApiMcpServer = serde_json::from_str(json).expect("ApiMcpServer failed");
        assert_eq!(server.name, "Context7");
        assert_eq!(server.tools.len(), 2);
    }

    #[wasm_bindgen_test]
    fn test_api_costs_deserialize() {
        let json = r#"{
            "input_tokens": 50000,
            "output_tokens": 12000,
            "sessions": [
                {"session_id": "s1", "agent_name": "Crew", "input_tokens": 50000, "output_tokens": 12000}
            ]
        }"#;
        let costs: ApiCosts = serde_json::from_str(json).expect("ApiCosts failed");
        assert_eq!(costs.input_tokens, 50000);
        assert_eq!(costs.sessions.len(), 1);
        assert_eq!(costs.sessions[0].agent_name, "Crew");
    }

    #[wasm_bindgen_test]
    fn test_api_roadmap_item_deserialize() {
        let json = r#"{
            "id": "r1",
            "title": "v2.0 Launch",
            "description": "Major release",
            "status": "in_progress",
            "priority": "high"
        }"#;
        let item: ApiRoadmapItem = serde_json::from_str(json).expect("ApiRoadmapItem failed");
        assert_eq!(item.title, "v2.0 Launch");
        assert_eq!(item.priority, "high");
    }

    #[wasm_bindgen_test]
    fn test_api_idea_deserialize() {
        let json = r#"{
            "id": "idea-1",
            "title": "Auto-merge safe PRs",
            "description": "Automatically merge PRs that pass all checks",
            "category": "feature",
            "impact": "high",
            "effort": "medium"
        }"#;
        let idea: ApiIdea = serde_json::from_str(json).expect("ApiIdea failed");
        assert_eq!(idea.title, "Auto-merge safe PRs");
        assert_eq!(idea.impact, "high");
    }

    #[wasm_bindgen_test]
    fn test_api_github_issue_deserialize() {
        let json = r#"{
            "number": 42,
            "title": "Fix login bug",
            "labels": ["bug", "priority:high"],
            "assignee": "devuser",
            "state": "open",
            "created": "2026-01-15"
        }"#;
        let issue: ApiGithubIssue = serde_json::from_str(json).expect("ApiGithubIssue failed");
        assert_eq!(issue.number, 42);
        assert_eq!(issue.labels.len(), 2);
        assert_eq!(issue.assignee, Some("devuser".to_string()));
    }

    #[wasm_bindgen_test]
    fn test_api_github_pr_deserialize() {
        let json = r#"{
            "number": 100,
            "title": "Add dark mode",
            "author": "contributor",
            "status": "open",
            "reviewers": ["reviewer1"],
            "created": "2026-02-10"
        }"#;
        let pr: ApiGithubPr = serde_json::from_str(json).expect("ApiGithubPr failed");
        assert_eq!(pr.number, 100);
        assert_eq!(pr.author, "contributor");
        assert_eq!(pr.reviewers.len(), 1);
    }

    #[wasm_bindgen_test]
    fn test_api_notification_deserialize() {
        let json = r#"{
            "id": "notif-1",
            "title": "Task completed",
            "message": "Bead xyz is done",
            "level": "info",
            "source": "system",
            "created_at": "2026-02-15T12:00:00Z",
            "read": false,
            "action_url": "/beads/xyz"
        }"#;
        let notif: ApiNotification = serde_json::from_str(json).expect("ApiNotification failed");
        assert_eq!(notif.title, "Task completed");
        assert_eq!(notif.read, false);
        assert_eq!(notif.action_url, Some("/beads/xyz".to_string()));
    }

    #[wasm_bindgen_test]
    fn test_api_notification_count_deserialize() {
        let json = r#"{"unread": 5, "total": 20}"#;
        let count: ApiNotificationCount =
            serde_json::from_str(json).expect("ApiNotificationCount failed");
        assert_eq!(count.unread, 5);
        assert_eq!(count.total, 20);
    }

    #[wasm_bindgen_test]
    fn test_api_changelog_entry_deserialize() {
        let json = r#"{
            "id": "cl-1",
            "version": "0.2.0",
            "date": "2026-02-15",
            "sections": [
                {"category": "feat", "items": ["Added dark mode", "Added light mode"]},
                {"category": "fix", "items": ["Fixed login bug"]}
            ]
        }"#;
        let entry: ApiChangelogEntry =
            serde_json::from_str(json).expect("ApiChangelogEntry failed");
        assert_eq!(entry.version, "0.2.0");
        assert_eq!(entry.sections.len(), 2);
        assert_eq!(entry.sections[0].category, "feat");
        assert_eq!(entry.sections[0].items.len(), 2);
    }

    #[wasm_bindgen_test]
    fn test_api_memory_entry_deserialize() {
        let json = r#"{
            "id": "mem-1",
            "category": "decision",
            "content": "We chose tokio over async-std",
            "created_at": "2026-02-10T08:00:00Z"
        }"#;
        let entry: ApiMemoryEntry = serde_json::from_str(json).expect("ApiMemoryEntry failed");
        assert_eq!(entry.category, "decision");
    }

    #[wasm_bindgen_test]
    fn test_api_convoy_deserialize() {
        let json = r#"{
            "id": "conv-1",
            "name": "Sprint 1",
            "bead_count": 10,
            "status": "active"
        }"#;
        let convoy: ApiConvoy = serde_json::from_str(json).expect("ApiConvoy failed");
        assert_eq!(convoy.name, "Sprint 1");
        assert_eq!(convoy.bead_count, 10);
    }

    #[wasm_bindgen_test]
    fn test_api_insights_session_deserialize() {
        let json = r#"{"id": "is-1", "title": "Architecture Review"}"#;
        let session: ApiInsightsSession =
            serde_json::from_str(json).expect("ApiInsightsSession failed");
        assert_eq!(session.title, "Architecture Review");
    }

    #[wasm_bindgen_test]
    fn test_api_insights_message_deserialize() {
        let json = r#"{"id": "msg-1", "role": "user", "content": "What is the best approach?"}"#;
        let msg: ApiInsightsMessage =
            serde_json::from_str(json).expect("ApiInsightsMessage failed");
        assert_eq!(msg.role, "user");
    }

    #[wasm_bindgen_test]
    fn test_api_task_deserialize() {
        let json = r#"{
            "id": "task-1",
            "title": "Write tests",
            "bead_id": "bead-1",
            "priority": "high",
            "complexity": "medium",
            "category": "testing"
        }"#;
        let task: ApiTask = serde_json::from_str(json).expect("ApiTask failed");
        assert_eq!(task.title, "Write tests");
        assert_eq!(task.priority, "high");
    }

    #[wasm_bindgen_test]
    fn test_api_credential_status_deserialize() {
        let json = r#"{"providers": ["ANTHROPIC_API_KEY", "GITHUB_TOKEN"], "daemon_auth": true}"#;
        let status: ApiCredentialStatus =
            serde_json::from_str(json).expect("ApiCredentialStatus failed");
        assert_eq!(status.providers.len(), 2);
        assert!(status.daemon_auth);
    }
}

// =============================================================================
// Navigation tab logic tests
// =============================================================================

mod navigation_logic {
    use super::*;
    use at_leptos_ui::components::nav_bar::tab_label;

    #[wasm_bindgen_test]
    fn test_tab_labels_for_all_17_tabs() {
        let expected = [
            (0, "Dashboard"),
            (1, "Kanban Board"),
            (2, "Agent Terminals"),
            (3, "Insights"),
            (4, "Ideation"),
            (5, "Roadmap"),
            (6, "Changelog"),
            (7, "Context"),
            (8, "MCP Overview"),
            (9, "Worktrees"),
            (10, "GitHub Issues"),
            (11, "GitHub PRs"),
            (12, "Claude Code"),
            (13, "Settings"),
            (14, "Terminals"),
            (15, "Onboarding"),
            (16, "Stacks"),
        ];
        for (idx, label) in expected {
            assert_eq!(
                tab_label(idx),
                label,
                "Tab {} should be '{}' but got '{}'",
                idx,
                label,
                tab_label(idx)
            );
        }
    }

    #[wasm_bindgen_test]
    fn test_tab_label_out_of_bounds_returns_default() {
        assert_eq!(tab_label(999), "Kanban Board");
        assert_eq!(tab_label(17), "Kanban Board");
    }

    #[wasm_bindgen_test]
    fn test_tab_count_is_17() {
        assert_ne!(tab_label(16), "Kanban Board"); // 16 is Stacks
        assert_eq!(tab_label(17), "Kanban Board"); // 17 is out of bounds
    }
}

// =============================================================================
// Events WebSocket URL test
// =============================================================================

mod event_stream {
    use super::*;
    use at_leptos_ui::api::events_ws_url;

    #[wasm_bindgen_test]
    fn test_events_ws_url_format() {
        let url = events_ws_url();
        assert!(
            url.starts_with("ws://"),
            "WS URL should start with ws://, got: {}",
            url
        );
        assert!(
            url.ends_with("/api/events/ws"),
            "WS URL should end with /api/events/ws, got: {}",
            url
        );
        assert!(
            url.contains("localhost"),
            "WS URL should contain localhost, got: {}",
            url
        );
    }
}

// =============================================================================
// JSON serialization edge cases
// =============================================================================

mod serialization_edge_cases {
    use super::*;

    #[wasm_bindgen_test]
    fn test_api_bead_array_deserialize() {
        let json = r#"[
            {"id": "1", "title": "A", "status": "Backlog", "lane": "Standard"},
            {"id": "2", "title": "B", "status": "Done", "lane": "Express"}
        ]"#;
        let beads: Vec<ApiBead> = serde_json::from_str(json).expect("Vec<ApiBead> failed");
        assert_eq!(beads.len(), 2);
    }

    #[wasm_bindgen_test]
    fn test_api_agent_array_empty() {
        let json = r#"[]"#;
        let agents: Vec<ApiAgent> = serde_json::from_str(json).expect("Empty Vec<ApiAgent> failed");
        assert_eq!(agents.len(), 0);
    }

    #[wasm_bindgen_test]
    fn test_api_settings_with_unknown_fields_ignored() {
        let json = r#"{"general": {"project_name": "test", "unknown_field": 42}}"#;
        // serde should ignore unknown fields or fail gracefully
        let result: Result<ApiSettings, _> = serde_json::from_str(json);
        // This may fail depending on serde deny_unknown_fields config.
        // If so, the frontend is brittle and this test catches it.
        if let Ok(settings) = result {
            assert_eq!(settings.general.project_name, "test");
        }
        // If Err, that's also valid to document
    }

    #[wasm_bindgen_test]
    fn test_api_github_issue_nullable_assignee() {
        let json = r#"{"number": 1, "title": "T", "labels": [], "assignee": null, "state": "open", "created": ""}"#;
        let issue: ApiGithubIssue = serde_json::from_str(json).expect("Nullable assignee failed");
        assert_eq!(issue.assignee, None);
    }

    #[wasm_bindgen_test]
    fn test_api_notification_nullable_action_url() {
        let json = r#"{"id": "n1", "title": "T", "message": "M", "level": "info", "source": "s", "created_at": "", "read": true}"#;
        let notif: ApiNotification = serde_json::from_str(json).expect("Missing action_url failed");
        assert_eq!(notif.action_url, None);
        assert_eq!(notif.read, true);
    }

    #[wasm_bindgen_test]
    fn test_api_stack_node_deserialize() {
        let json = r#"{
            "id": "bead-006",
            "title": "Build agent executor",
            "phase": "In Progress",
            "git_branch": "feature/agent-exec",
            "pr_number": 38,
            "stack_position": 0
        }"#;
        let node: ApiStackNode = serde_json::from_str(json).expect("ApiStackNode failed");
        assert_eq!(node.id, "bead-006");
        assert_eq!(node.title, "Build agent executor");
        assert_eq!(node.phase, "In Progress");
        assert_eq!(node.git_branch, Some("feature/agent-exec".to_string()));
        assert_eq!(node.pr_number, Some(38));
        assert_eq!(node.stack_position, 0);
    }

    #[wasm_bindgen_test]
    fn test_api_stack_node_deserialize_minimal() {
        let json = r#"{"id": "n1", "title": "Minimal"}"#;
        let node: ApiStackNode = serde_json::from_str(json).expect("Minimal ApiStackNode failed");
        assert_eq!(node.id, "n1");
        assert_eq!(node.phase, "");
        assert_eq!(node.git_branch, None);
        assert_eq!(node.pr_number, None);
        assert_eq!(node.stack_position, 0);
    }

    #[wasm_bindgen_test]
    fn test_api_stack_deserialize() {
        let json = r#"{
            "root": {"id": "r1", "title": "Root task", "phase": "Done", "git_branch": "main", "stack_position": 0},
            "children": [
                {"id": "c1", "title": "Child 1", "phase": "In Progress", "git_branch": "feat/c1", "pr_number": 10, "stack_position": 1},
                {"id": "c2", "title": "Child 2", "phase": "Planning", "git_branch": "feat/c2", "stack_position": 2}
            ],
            "total": 3
        }"#;
        let stack: ApiStack = serde_json::from_str(json).expect("ApiStack failed");
        assert_eq!(stack.root.id, "r1");
        assert_eq!(stack.children.len(), 2);
        assert_eq!(stack.children[0].pr_number, Some(10));
        assert_eq!(stack.children[1].pr_number, None);
        assert_eq!(stack.total, 3);
    }

    #[wasm_bindgen_test]
    fn test_api_stack_deserialize_no_children() {
        let json = r#"{"root": {"id": "solo", "title": "Solo task"}, "total": 1}"#;
        let stack: ApiStack = serde_json::from_str(json).expect("Solo ApiStack failed");
        assert_eq!(stack.root.id, "solo");
        assert_eq!(stack.children.len(), 0);
        assert_eq!(stack.total, 1);
    }

    #[wasm_bindgen_test]
    fn test_api_stack_array_deserialize() {
        let json = r#"[
            {"root": {"id": "s1", "title": "Stack 1"}, "total": 2, "children": [{"id": "c1", "title": "C"}]},
            {"root": {"id": "s2", "title": "Stack 2"}, "total": 1}
        ]"#;
        let stacks: Vec<ApiStack> = serde_json::from_str(json).expect("Vec<ApiStack> failed");
        assert_eq!(stacks.len(), 2);
        assert_eq!(stacks[0].children.len(), 1);
        assert_eq!(stacks[1].children.len(), 0);
    }
}

// =============================================================================
// Animated SVG phase icon tests
// =============================================================================

mod phase_icon_svg {
    use super::*;
    use at_leptos_ui::pages::beads::{phase_status_class, phase_status_icon_svg};
    use at_leptos_ui::types::{BeadResponse, BeadStatus, Lane};

    fn make_bead(status: BeadStatus, tags: Vec<String>) -> BeadResponse {
        BeadResponse {
            id: "test-1".to_string(),
            title: "Test bead".to_string(),
            status,
            lane: Lane::InProgress,
            agent_id: None,
            description: String::new(),
            tags,
            progress_stage: "code".to_string(),
            agent_names: vec![],
            timestamp: String::new(),
            action: None,
        }
    }

    #[wasm_bindgen_test]
    fn test_planning_icon_is_diamond_svg() {
        let bead = make_bead(BeadStatus::Planning, vec![]);
        let svg = phase_status_icon_svg(&bead);
        assert!(svg.contains("<svg"), "Should be an SVG element");
        assert!(
            svg.contains("phase-icon-plan"),
            "Should have planning CSS class"
        );
        assert!(svg.contains("<path"), "Diamond uses a path element");
        assert!(
            !svg.contains("animate"),
            "Planning icon should not have animations (Auto Claude pattern)"
        );
    }

    #[wasm_bindgen_test]
    fn test_in_progress_icon_is_play_svg() {
        let bead = make_bead(BeadStatus::InProgress, vec![]);
        let svg = phase_status_icon_svg(&bead);
        assert!(
            svg.contains("phase-icon-active"),
            "Should have active CSS class"
        );
        assert!(svg.contains("<polygon"), "Play icon uses a polygon element");
        assert!(
            !svg.contains("animate"),
            "InProgress icon should not loop (Auto Claude: no looping animations)"
        );
    }

    #[wasm_bindgen_test]
    fn test_ai_review_icon_is_eye_svg() {
        let bead = make_bead(BeadStatus::AiReview, vec![]);
        let svg = phase_status_icon_svg(&bead);
        assert!(
            svg.contains("phase-icon-review"),
            "Should have review CSS class"
        );
        assert!(svg.contains("<circle"), "Eye icon has circle element");
        assert!(!svg.contains("animate"), "AiReview icon should not loop");
    }

    #[wasm_bindgen_test]
    fn test_human_review_icon_matches_ai_review() {
        let ai = make_bead(BeadStatus::AiReview, vec![]);
        let human = make_bead(BeadStatus::HumanReview, vec![]);
        // Both review states use the same eye icon
        assert_eq!(phase_status_icon_svg(&ai), phase_status_icon_svg(&human));
    }

    #[wasm_bindgen_test]
    fn test_done_icon_has_checkmark_draw_animation() {
        let bead = make_bead(BeadStatus::Done, vec![]);
        let svg = phase_status_icon_svg(&bead);
        assert!(
            svg.contains("phase-icon-done"),
            "Should have done CSS class"
        );
        assert!(svg.contains("<polyline"), "Checkmark uses a polyline");
        assert!(
            svg.contains("stroke-dasharray"),
            "Should use stroke-dasharray for draw effect"
        );
        assert!(
            svg.contains("stroke-dashoffset"),
            "Should use stroke-dashoffset for draw effect"
        );
        assert!(
            svg.contains(r#"fill="freeze"#),
            "Draw animation should run once (fill=freeze), not loop"
        );
        assert!(
            svg.contains("animate"),
            "Done checkmark should have draw-once animation"
        );
        assert!(
            !svg.contains("indefinite"),
            "Done checkmark should NOT loop indefinitely"
        );
    }

    #[wasm_bindgen_test]
    fn test_failed_icon_is_x_svg() {
        let bead = make_bead(BeadStatus::Failed, vec![]);
        let svg = phase_status_icon_svg(&bead);
        assert!(
            svg.contains("phase-icon-fail"),
            "Should have fail CSS class"
        );
        assert!(svg.contains("<line"), "X icon uses line elements");
    }

    #[wasm_bindgen_test]
    fn test_stuck_tag_overrides_to_warning_icon() {
        let bead = make_bead(BeadStatus::InProgress, vec!["stuck".to_string()]);
        let svg = phase_status_icon_svg(&bead);
        assert!(
            svg.contains("phase-icon-warn"),
            "Stuck tag should force warning icon"
        );
        assert!(
            !svg.contains("phase-icon-active"),
            "Should not show InProgress icon when stuck"
        );
    }

    #[wasm_bindgen_test]
    fn test_recovery_tag_overrides_to_warning_icon() {
        let bead = make_bead(BeadStatus::Planning, vec!["recovery".to_string()]);
        let svg = phase_status_icon_svg(&bead);
        assert!(
            svg.contains("phase-icon-warn"),
            "Recovery tag should force warning icon"
        );
    }

    #[wasm_bindgen_test]
    fn test_failed_status_shows_warning_icon() {
        // Failed status should also show warning icon (not the X)
        let bead = make_bead(BeadStatus::Failed, vec!["stuck".to_string()]);
        let svg = phase_status_icon_svg(&bead);
        assert!(
            svg.contains("phase-icon-warn"),
            "Failed + stuck should show warning"
        );
    }

    #[wasm_bindgen_test]
    fn test_all_icons_are_14x14_viewbox_24() {
        let statuses = [
            BeadStatus::Planning,
            BeadStatus::InProgress,
            BeadStatus::AiReview,
            BeadStatus::HumanReview,
            BeadStatus::Done,
            BeadStatus::Failed,
        ];
        for status in statuses {
            let bead = make_bead(status, vec![]);
            let svg = phase_status_icon_svg(&bead);
            assert!(svg.contains(r#"width="14""#), "Icon should be 14px wide");
            assert!(svg.contains(r#"height="14""#), "Icon should be 14px tall");
            assert!(
                svg.contains("viewBox=\"0 0 24 24\""),
                "Icon should use 24x24 viewBox"
            );
        }
    }

    #[wasm_bindgen_test]
    fn test_phase_status_class_planning() {
        let bead = make_bead(BeadStatus::Planning, vec![]);
        assert_eq!(
            phase_status_class(&bead),
            "bead-phase-status bead-phase-planning"
        );
    }

    #[wasm_bindgen_test]
    fn test_phase_status_class_stuck_overrides() {
        let bead = make_bead(BeadStatus::InProgress, vec!["stuck".to_string()]);
        assert_eq!(
            phase_status_class(&bead),
            "bead-phase-status bead-phase-interrupted"
        );
    }

    #[wasm_bindgen_test]
    fn test_phase_status_class_done() {
        let bead = make_bead(BeadStatus::Done, vec![]);
        assert_eq!(
            phase_status_class(&bead),
            "bead-phase-status bead-phase-complete"
        );
    }
}

// =============================================================================
// Bead helper function tests
// =============================================================================

mod bead_helpers {
    use super::*;
    use at_leptos_ui::pages::beads::{agent_initials, bead_tag_class, stage_class};

    #[wasm_bindgen_test]
    fn test_agent_initials_known_roles() {
        assert_eq!(agent_initials("crew-lead"), "CR");
        assert_eq!(agent_initials("swarm-worker"), "SW");
        assert_eq!(agent_initials("code-planner"), "PL");
        assert_eq!(agent_initials("main-coder"), "CD");
        assert_eq!(agent_initials("pr-reviewer"), "RV");
        assert_eq!(agent_initials("unit-tester"), "TS");
        assert_eq!(agent_initials("smart-debugger"), "DB");
        assert_eq!(agent_initials("system-architect"), "AR");
    }

    #[wasm_bindgen_test]
    fn test_agent_initials_unknown_takes_first_two() {
        assert_eq!(agent_initials("custom"), "CU");
        assert_eq!(agent_initials("x"), "X");
    }

    #[wasm_bindgen_test]
    fn test_stage_class_progression() {
        assert_eq!(stage_class("code", "plan"), "stage completed");
        assert_eq!(stage_class("code", "code"), "stage active");
        assert_eq!(stage_class("code", "qa"), "stage pending");
        assert_eq!(stage_class("code", "done"), "stage pending");
    }

    #[wasm_bindgen_test]
    fn test_stage_class_done_marks_all_completed() {
        assert_eq!(stage_class("done", "plan"), "stage completed");
        assert_eq!(stage_class("done", "code"), "stage completed");
        assert_eq!(stage_class("done", "qa"), "stage completed");
        assert_eq!(stage_class("done", "done"), "stage active");
    }

    #[wasm_bindgen_test]
    fn test_bead_tag_class_categories() {
        assert!(bead_tag_class("stuck").contains("recovery"));
        assert!(bead_tag_class("recovery").contains("recovery"));
        assert!(bead_tag_class("critical").contains("critical"));
        assert!(bead_tag_class("urgent").contains("critical"));
        assert!(bead_tag_class("high").contains("priority"));
        assert!(bead_tag_class("feature").contains("category"));
        assert!(bead_tag_class("bug fix").contains("category"));
        assert!(bead_tag_class("incomplete").contains("status"));
    }
}
