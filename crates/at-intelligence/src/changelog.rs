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
