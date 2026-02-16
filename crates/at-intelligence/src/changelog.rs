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

impl Default for ChangelogEngine {
    fn default() -> Self {
        Self::new()
    }
}
