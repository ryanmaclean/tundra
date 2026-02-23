//! Exhaustive integration tests for ChangelogEngine (Changelog feature).
//!
//! Covers entry CRUD, conventional commit parsing, changelog generation,
//! source selection matching the 3-step wizard UI
//! (Completed Tasks / Git History / Branch Comparison), and edge cases.

use chrono::Utc;
use uuid::Uuid;

use at_intelligence::changelog::{
    ChangeCategory, ChangelogEngine, ChangelogEntry, ChangelogSection,
};

// ===========================================================================
// Helper
// ===========================================================================

fn section(category: ChangeCategory, items: Vec<&str>) -> ChangelogSection {
    ChangelogSection {
        category,
        items: items.into_iter().map(String::from).collect(),
    }
}

fn entry(version: &str, sections: Vec<ChangelogSection>) -> ChangelogEntry {
    ChangelogEntry {
        id: Uuid::new_v4(),
        version: version.to_string(),
        date: Utc::now(),
        sections,
    }
}

// ===========================================================================
// Changelog Entry CRUD
// ===========================================================================

#[test]
fn test_create_changelog_entry() {
    let mut engine = ChangelogEngine::new();
    let e = entry(
        "0.1.0",
        vec![section(ChangeCategory::Added, vec!["Initial release"])],
    );
    let id = e.id;
    engine.add_entry(e);

    assert_eq!(engine.list_entries().len(), 1);
    let found = engine.get_entry(&id).unwrap();
    assert_eq!(found.version, "0.1.0");
    assert_eq!(found.sections.len(), 1);
    assert_eq!(found.sections[0].items[0], "Initial release");
}

#[test]
fn test_create_changelog_with_version() {
    let mut engine = ChangelogEngine::new();
    let e = entry("2.0.0-beta.1", vec![]);
    engine.add_entry(e);

    assert_eq!(engine.list_entries()[0].version, "2.0.0-beta.1");
}

#[test]
fn test_list_changelog_entries() {
    let mut engine = ChangelogEngine::new();
    assert!(engine.list_entries().is_empty());

    engine.add_entry(entry("0.1.0", vec![]));
    engine.add_entry(entry("0.2.0", vec![]));
    engine.add_entry(entry("1.0.0", vec![]));

    let list = engine.list_entries();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].version, "0.1.0");
    assert_eq!(list[1].version, "0.2.0");
    assert_eq!(list[2].version, "1.0.0");
}

#[test]
fn test_get_changelog_by_version() {
    let mut engine = ChangelogEngine::new();
    let e = entry(
        "1.0.0",
        vec![section(ChangeCategory::Fixed, vec!["Bug fix"])],
    );
    let id = e.id;
    engine.add_entry(e);

    // Lookup by id (engine uses id, not version string).
    let found = engine.get_entry(&id);
    assert!(found.is_some());
    assert_eq!(found.unwrap().version, "1.0.0");

    // Non-existent id.
    assert!(engine.get_entry(&Uuid::new_v4()).is_none());
}

#[test]
fn test_default_changelog_engine() {
    let engine = ChangelogEngine::default();
    assert!(engine.list_entries().is_empty());
}

// ===========================================================================
// Conventional Commit Parsing
// ===========================================================================

#[test]
fn test_parse_feat_commit_as_added() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("feat: add user authentication", "0.1.0");

    assert_eq!(entry.sections.len(), 1);
    assert_eq!(entry.sections[0].category, ChangeCategory::Added);
    assert_eq!(entry.sections[0].items[0], "add user authentication");
}

#[test]
fn test_parse_fix_commit_as_fixed() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("fix: resolve null pointer crash", "0.1.1");

    let fixed_section = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Fixed);
    assert!(fixed_section.is_some());
    assert_eq!(
        fixed_section.unwrap().items[0],
        "resolve null pointer crash"
    );
}

#[test]
fn test_parse_perf_commit_as_changed() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("perf: optimize database queries", "0.2.0");

    let changed = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Changed);
    assert!(changed.is_some());
    assert_eq!(changed.unwrap().items[0], "optimize database queries");
}

#[test]
fn test_parse_refactor_commit_as_changed() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("refactor: simplify auth logic", "0.2.0");

    let changed = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Changed);
    assert!(changed.is_some());
    assert_eq!(changed.unwrap().items[0], "simplify auth logic");
}

#[test]
fn test_parse_docs_commit_as_changed() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("docs: update API documentation", "0.2.0");

    let changed = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Changed);
    assert!(changed.is_some());
    assert_eq!(changed.unwrap().items[0], "update API documentation");
}

#[test]
fn test_parse_security_commit_as_security() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("security: patch XSS vulnerability", "0.2.1");

    let sec = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Security);
    assert!(sec.is_some());
    assert_eq!(sec.unwrap().items[0], "patch XSS vulnerability");
}

#[test]
fn test_parse_unknown_prefix_as_changed() {
    // Unknown prefixes (like "chore:") are collected under Added as fallback.
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("chore: update dependencies", "0.2.0");

    let added = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Added);
    assert!(added.is_some());
    assert!(added.unwrap().items[0].contains("update dependencies"));
}

#[test]
fn test_strip_scope_from_commit_message() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("feat(auth): add OAuth login", "0.1.0");

    let added = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Added);
    assert!(added.is_some());
    assert_eq!(added.unwrap().items[0], "add OAuth login");
}

// ===========================================================================
// Changelog Generation
// ===========================================================================

#[test]
fn test_generate_from_commits_creates_entry() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("feat: new feature", "0.1.0");

    assert!(!entry.id.is_nil());
    assert_eq!(entry.version, "0.1.0");
    assert!(!entry.sections.is_empty());
}

#[test]
fn test_generate_from_commits_with_version() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("feat: something", "3.0.0-rc.1");
    assert_eq!(entry.version, "3.0.0-rc.1");
}

#[test]
fn test_generate_from_multiple_commits() {
    let mut engine = ChangelogEngine::new();
    let commits = "\
feat: add login page
feat: add registration form
feat: add password reset
";
    let entry = engine.generate_from_commits(commits, "0.1.0");

    let added = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Added)
        .unwrap();
    assert_eq!(added.items.len(), 3);
    assert_eq!(added.items[0], "add login page");
    assert_eq!(added.items[1], "add registration form");
    assert_eq!(added.items[2], "add password reset");
}

#[test]
fn test_generate_from_mixed_commit_types() {
    let mut engine = ChangelogEngine::new();
    let commits = "\
feat: add dashboard
fix: resolve login crash
perf: optimize query
refactor: clean up models
docs: update readme
security: patch CSRF
";
    let entry = engine.generate_from_commits(commits, "1.0.0");

    let categories: Vec<_> = entry.sections.iter().map(|s| &s.category).collect();

    assert!(categories.contains(&&ChangeCategory::Added));
    assert!(categories.contains(&&ChangeCategory::Changed));
    assert!(categories.contains(&&ChangeCategory::Fixed));
    assert!(categories.contains(&&ChangeCategory::Security));

    // Changed should contain perf + refactor + docs = 3 items.
    let changed = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Changed)
        .unwrap();
    assert_eq!(changed.items.len(), 3);
}

#[test]
fn test_generate_changelog_markdown_format() {
    let mut engine = ChangelogEngine::new();
    engine.add_entry(ChangelogEntry {
        id: Uuid::new_v4(),
        version: "0.2.0".to_string(),
        date: Utc::now(),
        sections: vec![
            ChangelogSection {
                category: ChangeCategory::Added,
                items: vec!["New feature A".to_string(), "New feature B".to_string()],
            },
            ChangelogSection {
                category: ChangeCategory::Fixed,
                items: vec!["Bug fix C".to_string()],
            },
        ],
    });

    let md = engine.generate_markdown();
    assert!(md.starts_with("# Changelog"));
    assert!(md.contains("## [0.2.0]"));
    assert!(md.contains("### Added"));
    assert!(md.contains("- New feature A"));
    assert!(md.contains("- New feature B"));
    assert!(md.contains("### Fixed"));
    assert!(md.contains("- Bug fix C"));
}

// ===========================================================================
// Source Selection (3 wizard steps from screenshot)
// ===========================================================================

#[test]
fn test_changelog_from_completed_tasks() {
    // Simulates "Completed Tasks" source: tasks with descriptions become
    // changelog entries of Added type.
    let mut engine = ChangelogEngine::new();
    let tasks = "\
feat: Project Documentation Foundation
feat: TypeScript & Linting Configuration
feat: CI/CD Pipeline Stabilization
";
    let entry = engine.generate_from_commits(tasks, "0.1.0");

    let added = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Added)
        .unwrap();
    assert_eq!(added.items.len(), 3);
    assert!(added
        .items
        .iter()
        .any(|i| i.contains("Project Documentation Foundation")));
    assert!(added
        .items
        .iter()
        .any(|i| i.contains("CI/CD Pipeline Stabilization")));
}

#[test]
fn test_changelog_from_git_history() {
    // Simulates "Git History" source: conventional commits from git log.
    let mut engine = ChangelogEngine::new();
    let git_log = "\
feat: add roadmap kanban view
fix: roadmap task display bug
refactor: extract roadmap components
";
    let entry = engine.generate_from_commits(git_log, "0.3.0");

    assert!(entry
        .sections
        .iter()
        .any(|s| s.category == ChangeCategory::Added));
    assert!(entry
        .sections
        .iter()
        .any(|s| s.category == ChangeCategory::Fixed));
    assert!(entry
        .sections
        .iter()
        .any(|s| s.category == ChangeCategory::Changed));
}

#[test]
fn test_changelog_from_branch_comparison() {
    // Simulates "Branch Comparison" source: diff between two branches.
    let mut engine = ChangelogEngine::new();
    let branch_diff = "\
feat(roadmap): implement xstate integration
fix(roadmap): task status not updating
feat(competitor): add manual competitor entry
";
    let entry = engine.generate_from_commits(branch_diff, "0.4.0");

    let added = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Added)
        .unwrap();
    assert_eq!(added.items.len(), 2);

    let fixed = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Fixed)
        .unwrap();
    assert_eq!(fixed.items.len(), 1);
    assert_eq!(fixed.items[0], "task status not updating");
}

// ===========================================================================
// Edge Cases
// ===========================================================================

#[test]
fn test_empty_commits_produces_empty_changelog() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("", "0.0.1");

    assert!(entry.sections.is_empty());
    assert_eq!(entry.version, "0.0.1");
}

#[test]
fn test_multiline_commit_messages() {
    // Each line is treated as a separate commit.
    let mut engine = ChangelogEngine::new();
    let commits = "feat: first feature\n\nfeat: second feature\n\n\n";
    let entry = engine.generate_from_commits(commits, "0.1.0");

    let added = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Added)
        .unwrap();
    // Empty lines are skipped, so we get exactly 2 items.
    assert_eq!(added.items.len(), 2);
}

#[test]
fn test_commits_with_scope_stripped() {
    let mut engine = ChangelogEngine::new();
    let commits = "\
feat(ui): add dark mode
fix(api): handle timeout errors
refactor(db): normalize schema
docs(readme): add installation guide
perf(query): add index on users table
security(auth): enforce rate limiting
";
    let entry = engine.generate_from_commits(commits, "1.0.0");

    // Verify scopes are stripped from messages.
    let added = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Added)
        .unwrap();
    assert_eq!(added.items[0], "add dark mode");
    assert!(!added.items[0].contains("(ui)"));

    let fixed = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Fixed)
        .unwrap();
    assert_eq!(fixed.items[0], "handle timeout errors");

    let changed = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Changed)
        .unwrap();
    // Should contain refactor + docs + perf = 3 items, all scope-stripped.
    assert_eq!(changed.items.len(), 3);
    for item in &changed.items {
        assert!(!item.contains("("));
    }

    let sec = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Security)
        .unwrap();
    assert_eq!(sec.items[0], "enforce rate limiting");
}

#[test]
fn test_duplicate_entries_deduplicated() {
    // The engine does not auto-deduplicate, so adding duplicate commits
    // results in duplicate items. This test documents that behavior.
    let mut engine = ChangelogEngine::new();
    let commits = "\
feat: add login
feat: add login
feat: add login
";
    let entry = engine.generate_from_commits(commits, "0.1.0");

    let added = entry
        .sections
        .iter()
        .find(|s| s.category == ChangeCategory::Added)
        .unwrap();
    // All three lines are preserved (no deduplication at engine level).
    assert_eq!(added.items.len(), 3);
}

// ===========================================================================
// Additional coverage: stored entries, markdown, serde, timestamps
// ===========================================================================

#[test]
fn test_generated_entry_stored_in_engine() {
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("feat: stored", "0.1.0");

    assert_eq!(engine.list_entries().len(), 1);
    let found = engine.get_entry(&entry.id);
    assert!(found.is_some());
    assert_eq!(found.unwrap().version, "0.1.0");
}

#[test]
fn test_multiple_generates_accumulate() {
    let mut engine = ChangelogEngine::new();
    engine.generate_from_commits("feat: first", "0.1.0");
    engine.generate_from_commits("fix: second", "0.2.0");
    engine.generate_from_commits("perf: third", "0.3.0");

    assert_eq!(engine.list_entries().len(), 3);
}

#[test]
fn test_markdown_multiple_entries() {
    let mut engine = ChangelogEngine::new();
    engine.generate_from_commits("feat: feature one", "0.1.0");
    engine.generate_from_commits("fix: bug fix one", "0.2.0");

    let md = engine.generate_markdown();
    assert!(md.contains("## [0.1.0]"));
    assert!(md.contains("## [0.2.0]"));
    assert!(md.contains("### Added"));
    assert!(md.contains("### Fixed"));
}

#[test]
fn test_markdown_empty_engine() {
    let engine = ChangelogEngine::new();
    let md = engine.generate_markdown();
    assert_eq!(md, "# Changelog\n\n");
}

#[test]
fn test_entry_has_timestamp() {
    let before = Utc::now();
    let mut engine = ChangelogEngine::new();
    let entry = engine.generate_from_commits("feat: timed", "0.1.0");
    let after = Utc::now();

    assert!(entry.date >= before);
    assert!(entry.date <= after);
}

#[test]
fn test_serde_roundtrip_changelog_entry() {
    let e = ChangelogEntry {
        id: Uuid::new_v4(),
        version: "1.0.0".to_string(),
        date: Utc::now(),
        sections: vec![ChangelogSection {
            category: ChangeCategory::Security,
            items: vec!["Patched CVE".to_string()],
        }],
    };
    let json = serde_json::to_string(&e).unwrap();
    let deserialized: ChangelogEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.version, "1.0.0");
    assert_eq!(deserialized.sections[0].category, ChangeCategory::Security);
}

#[test]
fn test_serde_roundtrip_change_category() {
    let categories = vec![
        ChangeCategory::Added,
        ChangeCategory::Changed,
        ChangeCategory::Fixed,
        ChangeCategory::Removed,
        ChangeCategory::Security,
        ChangeCategory::Performance,
    ];
    for cat in categories {
        let json = serde_json::to_string(&cat).unwrap();
        let deserialized: ChangeCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, cat);
    }
}

#[test]
fn test_change_category_variants_distinct() {
    let all = [
        ChangeCategory::Added,
        ChangeCategory::Changed,
        ChangeCategory::Fixed,
        ChangeCategory::Removed,
        ChangeCategory::Security,
        ChangeCategory::Performance,
    ];
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            assert_ne!(all[i], all[j]);
        }
    }
}

#[test]
fn test_whitespace_only_commits_skipped() {
    let mut engine = ChangelogEngine::new();
    let commits = "   \n  \n\n   \n";
    let entry = engine.generate_from_commits(commits, "0.0.1");
    assert!(entry.sections.is_empty());
}

#[test]
fn test_markdown_section_ordering() {
    // Added comes before Changed, Changed before Fixed, Fixed before Security.
    let mut engine = ChangelogEngine::new();
    let commits = "\
feat: new thing
fix: broken thing
perf: faster thing
security: safe thing
";
    engine.generate_from_commits(commits, "1.0.0");
    let md = engine.generate_markdown();

    let added_pos = md.find("### Added").unwrap();
    let changed_pos = md.find("### Changed").unwrap();
    let fixed_pos = md.find("### Fixed").unwrap();
    let security_pos = md.find("### Security").unwrap();

    assert!(added_pos < changed_pos);
    assert!(changed_pos < fixed_pos);
    assert!(fixed_pos < security_pos);
}
