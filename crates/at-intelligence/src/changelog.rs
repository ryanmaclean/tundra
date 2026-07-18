use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ChangeCategory
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeCategory {
    Added,
    Changed,
    Fixed,
    Removed,
    Security,
    Performance,
}

impl ChangeCategory {
    fn heading(&self) -> &'static str {
        match self {
            ChangeCategory::Added => "Added",
            ChangeCategory::Changed => "Changed",
            ChangeCategory::Fixed => "Fixed",
            ChangeCategory::Removed => "Removed",
            ChangeCategory::Security => "Security",
            ChangeCategory::Performance => "Performance",
        }
    }
}

// ---------------------------------------------------------------------------
// ChangelogSection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogSection {
    pub category: ChangeCategory,
    pub items: Vec<String>,
}

// ---------------------------------------------------------------------------
// ChangelogEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub id: Uuid,
    pub version: String,
    pub date: DateTime<Utc>,
    pub sections: Vec<ChangelogSection>,
}

// ---------------------------------------------------------------------------
// ChangelogEngine
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ChangelogEngine {
    entries: Vec<ChangelogEntry>,
}

impl ChangelogEngine {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn add_entry(&mut self, entry: ChangelogEntry) {
        self.entries.push(entry);
    }

    pub fn list_entries(&self) -> &[ChangelogEntry] {
        &self.entries
    }

    pub fn get_entry(&self, id: &Uuid) -> Option<&ChangelogEntry> {
        self.entries.iter().find(|e| e.id == *id)
    }

    /// Parse a raw commit log into a `ChangelogEntry` for the given version.
    ///
    /// Recognised conventional-commit prefixes:
    ///
    /// | Prefix       | Category      |
    /// |------------- |---------------|
    /// | `feat`       | Added         |
    /// | `fix`        | Fixed         |
    /// | `perf`       | Changed       |
    /// | `docs`       | Changed       |
    /// | `refactor`   | Changed       |
    /// | `security`   | Security      |
    ///
    /// Lines that do not match a known prefix are collected under `Added` as a
    /// fallback.  The resulting entry is **also** stored inside the engine.
    ///
    /// This is a synchronous parser — the actual LLM call happens in the API
    /// layer.
    pub fn generate_from_commits(&mut self, commits: &str, version: &str) -> ChangelogEntry {
        use std::collections::BTreeMap;

        // Accumulate items per category.
        let mut buckets: BTreeMap<&str, Vec<String>> = BTreeMap::new();

        for line in commits.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Try to extract "prefix: message" or "prefix(scope): message".
            let (cat_key, message) = if let Some(rest) = trimmed.strip_prefix("feat") {
                let msg = strip_scope_colon(rest);
                ("added", msg)
            } else if let Some(rest) = trimmed.strip_prefix("fix") {
                let msg = strip_scope_colon(rest);
                ("fixed", msg)
            } else if let Some(rest) = trimmed.strip_prefix("perf") {
                let msg = strip_scope_colon(rest);
                ("changed_perf", msg)
            } else if let Some(rest) = trimmed.strip_prefix("refactor") {
                let msg = strip_scope_colon(rest);
                ("changed_refactor", msg)
            } else if let Some(rest) = trimmed.strip_prefix("docs") {
                let msg = strip_scope_colon(rest);
                ("changed_docs", msg)
            } else if let Some(rest) = trimmed.strip_prefix("security") {
                let msg = strip_scope_colon(rest);
                ("security", msg)
            } else {
                ("added", trimmed.to_string())
            };

            buckets.entry(cat_key).or_default().push(message);
        }

        let mut sections = Vec::new();
        // Merge all "added" items.
        if let Some(items) = buckets.remove("added") {
            sections.push(ChangelogSection {
                category: ChangeCategory::Added,
                items,
            });
        }
        // Merge all Changed variants.
        let mut changed_items = Vec::new();
        for key in &["changed_perf", "changed_refactor", "changed_docs"] {
            if let Some(items) = buckets.remove(key) {
                changed_items.extend(items);
            }
        }
        if !changed_items.is_empty() {
            sections.push(ChangelogSection {
                category: ChangeCategory::Changed,
                items: changed_items,
            });
        }
        if let Some(items) = buckets.remove("fixed") {
            sections.push(ChangelogSection {
                category: ChangeCategory::Fixed,
                items,
            });
        }
        if let Some(items) = buckets.remove("security") {
            sections.push(ChangelogSection {
                category: ChangeCategory::Security,
                items,
            });
        }

        let entry = ChangelogEntry {
            id: Uuid::new_v4(),
            version: version.to_string(),
            date: Utc::now(),
            sections,
        };

        self.entries.push(entry.clone());
        entry
    }

    /// Render all changelog entries as a Keep-a-Changelog-style markdown string.
    pub fn generate_markdown(&self) -> String {
        let mut md = String::from("# Changelog\n\n");

        for entry in &self.entries {
            md.push_str(&format!(
                "## [{}] - {}\n\n",
                entry.version,
                entry.date.format("%Y-%m-%d")
            ));

            for section in &entry.sections {
                md.push_str(&format!("### {}\n\n", section.category.heading()));
                for item in &section.items {
                    md.push_str(&format!("- {item}\n"));
                }
                md.push('\n');
            }
        }

        md
    }
}

/// Strip an optional `(scope): ` prefix from the remainder after a
/// conventional-commit keyword, returning the cleaned-up message.
fn strip_scope_colon(rest: &str) -> String {
    let rest = rest.trim();
    // Handle optional "(scope)" before the colon.
    let rest = if rest.starts_with('(') {
        if let Some(idx) = rest.find(')') {
            rest[idx + 1..].trim()
        } else {
            rest
        }
    } else {
        rest
    };
    // Strip leading colon and whitespace.
    let rest = rest.strip_prefix(':').unwrap_or(rest).trim();
    rest.to_string()
}

impl Default for ChangelogEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_section(cat: ChangeCategory, items: Vec<&str>) -> ChangelogSection {
        ChangelogSection {
            category: cat,
            items: items.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    fn make_entry(version: &str, sections: Vec<ChangelogSection>) -> ChangelogEntry {
        ChangelogEntry {
            id: Uuid::new_v4(),
            version: version.to_string(),
            date: Utc::now(),
            sections,
        }
    }

    // --- ChangelogEngine::new ------------------------------------------------

    #[test]
    fn test_new_creates_empty_engine() {
        let engine = ChangelogEngine::new();
        assert!(engine.list_entries().is_empty());
    }

    // --- add_entry / list_entries / get_entry --------------------------------

    #[test]
    fn test_add_entry_stores_correct_version() {
        let mut engine = ChangelogEngine::new();
        let entry = make_entry("1.0.0", vec![]);
        let id = entry.id;
        engine.add_entry(entry);

        assert_eq!(engine.list_entries().len(), 1);
        assert_eq!(engine.list_entries()[0].version, "1.0.0");
        assert_eq!(engine.list_entries()[0].id, id);
    }

    #[test]
    fn test_add_entry_multiple_preserves_order() {
        let mut engine = ChangelogEngine::new();
        engine.add_entry(make_entry("1.0.0", vec![]));
        engine.add_entry(make_entry("2.0.0", vec![]));
        engine.add_entry(make_entry("3.0.0", vec![]));

        let entries = engine.list_entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].version, "1.0.0");
        assert_eq!(entries[1].version, "2.0.0");
        assert_eq!(entries[2].version, "3.0.0");
    }

    #[test]
    fn test_get_entry_returns_correct_entry() {
        let mut engine = ChangelogEngine::new();
        let entry = make_entry("1.2.3", vec![make_section(ChangeCategory::Added, vec!["new feature"])]);
        let id = entry.id;
        engine.add_entry(entry);

        let found = engine.get_entry(&id).unwrap();
        assert_eq!(found.version, "1.2.3");
        assert_eq!(found.sections.len(), 1);
    }

    #[test]
    fn test_get_entry_not_found_returns_none() {
        let engine = ChangelogEngine::new();
        assert!(engine.get_entry(&Uuid::new_v4()).is_none());
    }

    // --- generate_from_commits -----------------------------------------------

    #[test]
    fn test_generate_from_commits_feat_prefix_maps_to_added() {
        let mut engine = ChangelogEngine::new();
        let entry = engine.generate_from_commits("feat: add login page", "1.0.0");

        assert_eq!(entry.version, "1.0.0");
        let added = entry.sections.iter().find(|s| s.category == ChangeCategory::Added).unwrap();
        assert!(added.items.iter().any(|i| i.contains("add login page")));
    }

    #[test]
    fn test_generate_from_commits_fix_prefix_maps_to_fixed() {
        let mut engine = ChangelogEngine::new();
        let entry = engine.generate_from_commits("fix: resolve null pointer", "1.0.1");

        let fixed = entry.sections.iter().find(|s| s.category == ChangeCategory::Fixed).unwrap();
        assert!(fixed.items.iter().any(|i| i.contains("resolve null pointer")));
    }

    #[test]
    fn test_generate_from_commits_perf_prefix_maps_to_changed() {
        let mut engine = ChangelogEngine::new();
        let entry = engine.generate_from_commits("perf: optimise query", "1.1.0");

        let changed = entry.sections.iter().find(|s| s.category == ChangeCategory::Changed).unwrap();
        assert!(changed.items.iter().any(|i| i.contains("optimise query")));
    }

    #[test]
    fn test_generate_from_commits_refactor_prefix_maps_to_changed() {
        let mut engine = ChangelogEngine::new();
        let entry = engine.generate_from_commits("refactor: split auth module", "1.1.0");

        let changed = entry.sections.iter().find(|s| s.category == ChangeCategory::Changed).unwrap();
        assert!(changed.items.iter().any(|i| i.contains("split auth module")));
    }

    #[test]
    fn test_generate_from_commits_docs_prefix_maps_to_changed() {
        let mut engine = ChangelogEngine::new();
        let entry = engine.generate_from_commits("docs: update README", "1.1.0");

        let changed = entry.sections.iter().find(|s| s.category == ChangeCategory::Changed).unwrap();
        assert!(changed.items.iter().any(|i| i.contains("update README")));
    }

    #[test]
    fn test_generate_from_commits_security_prefix_maps_to_security() {
        let mut engine = ChangelogEngine::new();
        let entry = engine.generate_from_commits("security: patch XSS vulnerability", "1.0.2");

        let sec = entry.sections.iter().find(|s| s.category == ChangeCategory::Security).unwrap();
        assert!(sec.items.iter().any(|i| i.contains("patch XSS vulnerability")));
    }

    #[test]
    fn test_generate_from_commits_unknown_prefix_falls_to_added() {
        let mut engine = ChangelogEngine::new();
        let entry = engine.generate_from_commits("chore: update dependencies", "1.0.0");

        let added = entry.sections.iter().find(|s| s.category == ChangeCategory::Added).unwrap();
        assert!(added.items.iter().any(|i| i.contains("chore: update dependencies")));
    }

    #[test]
    fn test_generate_from_commits_skips_empty_lines() {
        let mut engine = ChangelogEngine::new();
        let commits = "feat: feature one\n\n\nfix: bug fix\n";
        let entry = engine.generate_from_commits(commits, "1.0.0");

        let total_items: usize = entry.sections.iter().map(|s| s.items.len()).sum();
        assert_eq!(total_items, 2);
    }

    #[test]
    fn test_generate_from_commits_stores_entry_in_engine() {
        let mut engine = ChangelogEngine::new();
        let entry = engine.generate_from_commits("feat: new thing", "2.0.0");
        let id = entry.id;

        assert_eq!(engine.list_entries().len(), 1);
        assert!(engine.get_entry(&id).is_some());
    }

    #[test]
    fn test_generate_from_commits_mixed_categories() {
        let mut engine = ChangelogEngine::new();
        let commits = "feat: new feature\nfix: bug fix\nsecurity: patch cve\nperf: faster load";
        let entry = engine.generate_from_commits(commits, "1.5.0");

        let cats: Vec<&ChangeCategory> = entry.sections.iter().map(|s| &s.category).collect();
        assert!(cats.contains(&&ChangeCategory::Added));
        assert!(cats.contains(&&ChangeCategory::Fixed));
        assert!(cats.contains(&&ChangeCategory::Security));
        assert!(cats.contains(&&ChangeCategory::Changed));
    }

    // --- strip_scope_colon (private, tested via generate_from_commits) -------

    #[test]
    fn test_strip_scope_colon_plain_colon_stripped() {
        // "feat: message" -> "message" (no scope)
        assert_eq!(strip_scope_colon(": hello world"), "hello world");
    }

    #[test]
    fn test_strip_scope_colon_with_scope_stripped() {
        // "(core): message" -> "message"
        assert_eq!(strip_scope_colon("(core): add feature"), "add feature");
    }

    #[test]
    fn test_strip_scope_colon_scope_without_colon_returns_trimmed() {
        // "(scope) message" — no colon after closing paren
        let result = strip_scope_colon("(scope) message");
        assert_eq!(result, "message");
    }

    #[test]
    fn test_generate_from_commits_feat_with_scope() {
        let mut engine = ChangelogEngine::new();
        let entry = engine.generate_from_commits("feat(auth): add OAuth support", "1.0.0");

        let added = entry.sections.iter().find(|s| s.category == ChangeCategory::Added).unwrap();
        assert!(added.items.iter().any(|i| i.contains("add OAuth support")));
        // Scope should not appear in the item
        assert!(!added.items.iter().any(|i| i.contains("(auth)")));
    }

    // --- generate_markdown ---------------------------------------------------

    #[test]
    fn test_generate_markdown_contains_header() {
        let engine = ChangelogEngine::new();
        let md = engine.generate_markdown();
        assert!(md.starts_with("# Changelog"));
    }

    #[test]
    fn test_generate_markdown_includes_version_and_category() {
        let mut engine = ChangelogEngine::new();
        engine.generate_from_commits("feat: initial release", "1.0.0");
        let md = engine.generate_markdown();

        assert!(md.contains("## [1.0.0]"));
        assert!(md.contains("### Added"));
        assert!(md.contains("- initial release"));
    }

    #[test]
    fn test_generate_markdown_multiple_entries_ordered() {
        let mut engine = ChangelogEngine::new();
        engine.generate_from_commits("feat: first", "1.0.0");
        engine.generate_from_commits("fix: second", "1.0.1");
        let md = engine.generate_markdown();

        let pos_1 = md.find("1.0.0").unwrap();
        let pos_2 = md.find("1.0.1").unwrap();
        // Entries appear in insertion order
        assert!(pos_1 < pos_2);
    }

    #[test]
    fn test_generate_markdown_security_section_heading() {
        let mut engine = ChangelogEngine::new();
        engine.generate_from_commits("security: patch csrf", "1.0.0");
        let md = engine.generate_markdown();
        assert!(md.contains("### Security"));
    }
}
