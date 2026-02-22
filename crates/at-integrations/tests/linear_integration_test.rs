//! Integration tests for Linear: client, import pipeline, sync engine,
//! and type serialization — matching the Linear integration surfaces.
//!
//! NOTE: LinearClient's list_issues/get_issue/list_teams use #[cfg(test)]
//! stub helpers that only compile in unit test mode (same crate). Integration
//! tests (separate crate) cannot call those stubs. Tests here focus on:
//! - Client creation validation
//! - Import pipeline (always-stub)
//! - Sync engine configuration and lifecycle
//! - Type serde roundtrips

use at_integrations::linear::{
    ImportResult, LinearClient, LinearError, LinearIssue, LinearProject, LinearTeam,
};
use at_integrations::linear::sync::{
    LinearSyncEngine, PendingChange, SyncConfig, SyncDirection, SyncResult,
};

use chrono::Utc;

// ===========================================================================
// Client creation
// ===========================================================================

#[test]
fn client_creation_with_valid_key() {
    let client = LinearClient::new("lin_api_abc123").unwrap();
    assert_eq!(client.api_key, "lin_api_abc123");
    assert!(client.active_team_id.is_none());
}

#[test]
fn client_creation_with_test_key() {
    let client = LinearClient::new("test_key").unwrap();
    assert_eq!(client.api_key, "test_key");
}

#[test]
fn client_creation_with_tok_key() {
    let client = LinearClient::new("tok").unwrap();
    assert_eq!(client.api_key, "tok");
}

#[test]
fn client_creation_empty_key_fails() {
    let result = LinearClient::new("");
    assert!(result.is_err());
    match result.unwrap_err() {
        LinearError::MissingApiKey => {}
        other => panic!("Expected MissingApiKey, got: {other}"),
    }
}

#[test]
fn client_creation_whitespace_key_succeeds() {
    let client = LinearClient::new(" ");
    assert!(client.is_ok());
}

#[test]
fn client_creation_various_prefixes() {
    // All non-empty strings succeed
    for key in ["lin_", "tok_", "test_", "abc", "x"] {
        let client = LinearClient::new(key);
        assert!(client.is_ok(), "Key '{}' should succeed", key);
    }
}

// ===========================================================================
// Import operations (always-stub, no network)
// ===========================================================================

#[tokio::test]
async fn import_single_issue() {
    let client = LinearClient::new("tok").unwrap();
    let results = client.import_issues(vec!["issue-001".to_string()]).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].success);
    assert_eq!(results[0].issue_id, "issue-001");
}

#[tokio::test]
async fn import_multiple_issues() {
    let client = LinearClient::new("tok").unwrap();
    let ids: Vec<String> = (1..=10).map(|i| format!("issue-{i:04}")).collect();
    let results = client.import_issues(ids).await.unwrap();
    assert_eq!(results.len(), 10);
    assert!(results.iter().all(|r| r.success));
}

#[tokio::test]
async fn import_empty_list() {
    let client = LinearClient::new("tok").unwrap();
    let results = client.import_issues(vec![]).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn import_preserves_issue_ids() {
    let client = LinearClient::new("tok").unwrap();
    let ids = vec!["abc".to_string(), "def".to_string(), "ghi".to_string()];
    let results = client.import_issues(ids).await.unwrap();
    assert_eq!(results[0].issue_id, "abc");
    assert_eq!(results[1].issue_id, "def");
    assert_eq!(results[2].issue_id, "ghi");
}

#[tokio::test]
async fn import_result_message() {
    let client = LinearClient::new("tok").unwrap();
    let results = client.import_issues(vec!["x".to_string()]).await.unwrap();
    assert!(results[0].message.contains("Imported"));
}

#[tokio::test]
async fn import_large_batch() {
    let client = LinearClient::new("tok").unwrap();
    let ids: Vec<String> = (1..=100).map(|i| format!("batch-{i}")).collect();
    let results = client.import_issues(ids).await.unwrap();
    assert_eq!(results.len(), 100);
    assert!(results.iter().all(|r| r.success));
    assert_eq!(results[99].issue_id, "batch-100");
}

// ===========================================================================
// Serde roundtrips
// ===========================================================================

#[test]
fn linear_team_serde_roundtrip() {
    let team = LinearTeam {
        id: "team-42".to_string(),
        name: "Platform".to_string(),
        key: "PLT".to_string(),
    };
    let json = serde_json::to_string(&team).unwrap();
    let de: LinearTeam = serde_json::from_str(&json).unwrap();
    assert_eq!(de.id, "team-42");
    assert_eq!(de.name, "Platform");
    assert_eq!(de.key, "PLT");
}

#[test]
fn linear_project_serde_roundtrip() {
    let project = LinearProject {
        id: "proj-1".to_string(),
        name: "Auto-Tundra".to_string(),
        description: Some("Agent orchestrator".to_string()),
        state: "active".to_string(),
        team_id: "team-001".to_string(),
        created_at: Utc::now(),
    };
    let json = serde_json::to_string(&project).unwrap();
    let de: LinearProject = serde_json::from_str(&json).unwrap();
    assert_eq!(de.id, "proj-1");
    assert_eq!(de.name, "Auto-Tundra");
    assert_eq!(de.state, "active");
}

#[test]
fn linear_project_without_description() {
    let project = LinearProject {
        id: "proj-2".to_string(),
        name: "Minimal".to_string(),
        description: None,
        state: "active".to_string(),
        team_id: "team-001".to_string(),
        created_at: Utc::now(),
    };
    let json = serde_json::to_string(&project).unwrap();
    let de: LinearProject = serde_json::from_str(&json).unwrap();
    assert!(de.description.is_none());
}

#[test]
fn linear_issue_serde_roundtrip() {
    let now = Utc::now();
    let issue = LinearIssue {
        id: "issue-001".to_string(),
        identifier: "ENG-1".to_string(),
        title: "Fix the thing".to_string(),
        description: Some("It's broken".to_string()),
        state_name: "In Progress".to_string(),
        priority: 2,
        team: LinearTeam {
            id: "team-001".to_string(),
            name: "Engineering".to_string(),
            key: "ENG".to_string(),
        },
        assignee_name: Some("Alice".to_string()),
        labels: vec!["bug".to_string(), "urgent".to_string()],
        created_at: now,
        updated_at: now,
        url: "https://linear.app/team/issue/ENG-1".to_string(),
    };
    let json = serde_json::to_string(&issue).unwrap();
    let de: LinearIssue = serde_json::from_str(&json).unwrap();
    assert_eq!(de.id, "issue-001");
    assert_eq!(de.identifier, "ENG-1");
    assert_eq!(de.state_name, "In Progress");
    assert_eq!(de.priority, 2);
    assert_eq!(de.labels.len(), 2);
    assert_eq!(de.assignee_name.as_deref(), Some("Alice"));
}

#[test]
fn linear_issue_without_optional_fields() {
    let now = Utc::now();
    let issue = LinearIssue {
        id: "issue-002".to_string(),
        identifier: "ENG-2".to_string(),
        title: "Minimal issue".to_string(),
        description: None,
        state_name: "Backlog".to_string(),
        priority: 0,
        team: LinearTeam {
            id: "team-001".to_string(),
            name: "Engineering".to_string(),
            key: "ENG".to_string(),
        },
        assignee_name: None,
        labels: vec![],
        created_at: now,
        updated_at: now,
        url: "https://linear.app/team/issue/ENG-2".to_string(),
    };
    let json = serde_json::to_string(&issue).unwrap();
    let de: LinearIssue = serde_json::from_str(&json).unwrap();
    assert!(de.description.is_none());
    assert!(de.assignee_name.is_none());
    assert!(de.labels.is_empty());
}

#[test]
fn import_result_success_roundtrip() {
    let result = ImportResult {
        issue_id: "issue-123".to_string(),
        success: true,
        message: "Imported successfully".to_string(),
    };
    let json = serde_json::to_string(&result).unwrap();
    let de: ImportResult = serde_json::from_str(&json).unwrap();
    assert_eq!(de.issue_id, "issue-123");
    assert!(de.success);
}

#[test]
fn import_result_failure_roundtrip() {
    let result = ImportResult {
        issue_id: "issue-bad".to_string(),
        success: false,
        message: "Duplicate issue".to_string(),
    };
    let json = serde_json::to_string(&result).unwrap();
    let de: ImportResult = serde_json::from_str(&json).unwrap();
    assert!(!de.success);
    assert_eq!(de.message, "Duplicate issue");
}

// ===========================================================================
// Sync engine types
// ===========================================================================

#[test]
fn sync_config_default_values() {
    let cfg = SyncConfig::default();
    assert_eq!(cfg.direction, SyncDirection::Bidirectional);
    assert_eq!(cfg.interval_seconds, 300);
    assert!(cfg.team_id.is_none());
    assert!(!cfg.auto_resolve_conflicts);
}

#[test]
fn sync_config_serde_roundtrip() {
    let cfg = SyncConfig {
        direction: SyncDirection::Push,
        interval_seconds: 60,
        team_id: Some("team-42".to_string()),
        auto_resolve_conflicts: true,
        max_retries: 3,
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let de: SyncConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(de.direction, SyncDirection::Push);
    assert_eq!(de.interval_seconds, 60);
    assert_eq!(de.team_id.as_deref(), Some("team-42"));
    assert!(de.auto_resolve_conflicts);
}

#[test]
fn sync_direction_serde_roundtrip() {
    for dir in [SyncDirection::Push, SyncDirection::Pull, SyncDirection::Bidirectional] {
        let json = serde_json::to_string(&dir).unwrap();
        let de: SyncDirection = serde_json::from_str(&json).unwrap();
        assert_eq!(de, dir);
    }
}

#[test]
fn sync_direction_serde_values() {
    assert_eq!(serde_json::to_string(&SyncDirection::Push).unwrap(), "\"push\"");
    assert_eq!(serde_json::to_string(&SyncDirection::Pull).unwrap(), "\"pull\"");
    assert_eq!(
        serde_json::to_string(&SyncDirection::Bidirectional).unwrap(),
        "\"bidirectional\""
    );
}

#[test]
fn sync_result_serde_roundtrip() {
    let result = SyncResult {
        direction: SyncDirection::Pull,
        pushed: 0,
        pulled: 5,
        conflicts: 1,
        dead_lettered: 0,
        synced_at: Utc::now(),
    };
    let json = serde_json::to_string(&result).unwrap();
    let de: SyncResult = serde_json::from_str(&json).unwrap();
    assert_eq!(de.pulled, 5);
    assert_eq!(de.conflicts, 1);
    assert_eq!(de.direction, SyncDirection::Pull);
    assert_eq!(de.dead_lettered, 0);
}

#[test]
fn sync_result_zero_values() {
    let result = SyncResult {
        direction: SyncDirection::Push,
        pushed: 0,
        pulled: 0,
        conflicts: 0,
        dead_lettered: 0,
        synced_at: Utc::now(),
    };
    let json = serde_json::to_string(&result).unwrap();
    let de: SyncResult = serde_json::from_str(&json).unwrap();
    assert_eq!(de.pushed, 0);
    assert_eq!(de.pulled, 0);
    assert_eq!(de.conflicts, 0);
    assert_eq!(de.dead_lettered, 0);
}

#[test]
fn pending_change_serde_roundtrip() {
    let change = PendingChange {
        id: "ch-1".to_string(),
        direction: SyncDirection::Push,
        entity_type: "task".to_string(),
        entity_id: "task-001".to_string(),
        change_type: "status_update".to_string(),
        created_at: Utc::now(),
    };
    let json = serde_json::to_string(&change).unwrap();
    let de: PendingChange = serde_json::from_str(&json).unwrap();
    assert_eq!(de.id, "ch-1");
    assert_eq!(de.direction, SyncDirection::Push);
    assert_eq!(de.entity_type, "task");
    assert_eq!(de.entity_id, "task-001");
    assert_eq!(de.change_type, "status_update");
}

#[test]
fn pending_change_pull_direction() {
    let change = PendingChange {
        id: "ch-pull".to_string(),
        direction: SyncDirection::Pull,
        entity_type: "issue".to_string(),
        entity_id: "issue-ext-42".to_string(),
        change_type: "title_update".to_string(),
        created_at: Utc::now(),
    };
    let json = serde_json::to_string(&change).unwrap();
    let de: PendingChange = serde_json::from_str(&json).unwrap();
    assert_eq!(de.direction, SyncDirection::Pull);
}

// ===========================================================================
// Sync engine operations (no network — engine config + queue only)
// ===========================================================================

#[test]
fn engine_creation() {
    let client = LinearClient::new("test_key").unwrap();
    let engine = LinearSyncEngine::new(client, SyncConfig::default());
    assert!(engine.last_sync_time().is_none());
    assert!(engine.pending_changes().is_empty());
}

#[test]
fn queue_single_change() {
    let client = LinearClient::new("test_key").unwrap();
    let mut engine = LinearSyncEngine::new(client, SyncConfig::default());

    engine.queue_change(PendingChange {
        id: "ch-1".into(),
        direction: SyncDirection::Push,
        entity_type: "task".into(),
        entity_id: "task-001".into(),
        change_type: "status_update".into(),
        created_at: Utc::now(),
    });
    assert_eq!(engine.pending_changes().len(), 1);
}

#[test]
fn queue_multiple_changes() {
    let client = LinearClient::new("test_key").unwrap();
    let mut engine = LinearSyncEngine::new(client, SyncConfig::default());

    for i in 1..=5 {
        engine.queue_change(PendingChange {
            id: format!("ch-{i}"),
            direction: SyncDirection::Push,
            entity_type: "task".into(),
            entity_id: format!("task-{i:03}"),
            change_type: "status_update".into(),
            created_at: Utc::now(),
        });
    }

    assert_eq!(engine.pending_changes().len(), 5);
}

#[test]
fn queue_mixed_directions() {
    let client = LinearClient::new("test_key").unwrap();
    let mut engine = LinearSyncEngine::new(client, SyncConfig::default());

    engine.queue_change(PendingChange {
        id: "ch-push".into(),
        direction: SyncDirection::Push,
        entity_type: "task".into(),
        entity_id: "task-001".into(),
        change_type: "status_update".into(),
        created_at: Utc::now(),
    });
    engine.queue_change(PendingChange {
        id: "ch-pull".into(),
        direction: SyncDirection::Pull,
        entity_type: "issue".into(),
        entity_id: "issue-ext-42".into(),
        change_type: "title_update".into(),
        created_at: Utc::now(),
    });

    assert_eq!(engine.pending_changes().len(), 2);
    assert_eq!(engine.pending_changes()[0].direction, SyncDirection::Push);
    assert_eq!(engine.pending_changes()[1].direction, SyncDirection::Pull);
}

#[test]
fn set_team_updates_config() {
    let client = LinearClient::new("test_key").unwrap();
    let mut engine = LinearSyncEngine::new(client, SyncConfig::default());

    engine.set_team(Some("team-42".into()));
    // Reset
    engine.set_team(None);
}

#[test]
fn set_config_replaces_config() {
    let client = LinearClient::new("test_key").unwrap();
    let mut engine = LinearSyncEngine::new(client, SyncConfig::default());

    let new_config = SyncConfig {
        direction: SyncDirection::Push,
        interval_seconds: 60,
        team_id: Some("team-99".into()),
        auto_resolve_conflicts: true,
        max_retries: 3,
    };
    engine.set_config(new_config);
}

#[test]
fn engine_no_sync_time_initially() {
    let client = LinearClient::new("test_key").unwrap();
    let engine = LinearSyncEngine::new(client, SyncConfig::default());
    assert!(engine.last_sync_time().is_none());
}

// ===========================================================================
// Import → queue workflow (no network)
// ===========================================================================

#[tokio::test]
async fn e2e_import_then_queue_changes() {
    // 1. Create client
    let client = LinearClient::new("tok").unwrap();

    // 2. Import some issues
    let ids = vec!["issue-1".to_string(), "issue-2".to_string(), "issue-3".to_string()];
    let import_results = client.import_issues(ids.clone()).await.unwrap();
    assert_eq!(import_results.len(), 3);
    assert!(import_results.iter().all(|r| r.success));

    // 3. Set up sync engine
    let config = SyncConfig {
        direction: SyncDirection::Bidirectional,
        interval_seconds: 300,
        team_id: Some("team-001".to_string()),
        auto_resolve_conflicts: false,
        max_retries: 3,
    };
    let mut engine = LinearSyncEngine::new(client, config);

    // 4. Queue local changes for each imported issue
    for (i, result) in import_results.iter().enumerate() {
        engine.queue_change(PendingChange {
            id: format!("ch-{i}"),
            direction: SyncDirection::Push,
            entity_type: "bead".into(),
            entity_id: result.issue_id.clone(),
            change_type: "status_update".into(),
            created_at: Utc::now(),
        });
    }

    assert_eq!(engine.pending_changes().len(), 3);
    assert_eq!(engine.pending_changes()[0].entity_id, "issue-1");
    assert_eq!(engine.pending_changes()[2].entity_id, "issue-3");
}

#[tokio::test]
async fn e2e_import_deduplication() {
    let client = LinearClient::new("tok").unwrap();

    // Import same IDs twice
    let ids = vec!["dup-1".to_string(), "dup-2".to_string()];
    let r1 = client.import_issues(ids.clone()).await.unwrap();
    let r2 = client.import_issues(ids).await.unwrap();

    // Both succeed (import_issues is idempotent)
    assert_eq!(r1.len(), 2);
    assert_eq!(r2.len(), 2);
    assert!(r1.iter().all(|r| r.success));
    assert!(r2.iter().all(|r| r.success));
}
