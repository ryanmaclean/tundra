use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::IntelligenceError;

// ---------------------------------------------------------------------------
// MemoryCategory
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    Pattern,
    Convention,
    Architecture,
    Dependency,
    ApiRoute,
    EnvVar,
    ServiceEndpoint,
    Keyword,
}

// ---------------------------------------------------------------------------
// MemoryEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: Uuid,
    pub key: String,
    pub value: String,
    pub category: MemoryCategory,
    pub confidence: f32,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub related: Vec<Uuid>,
}

impl MemoryEntry {
    pub fn new(
        key: impl Into<String>,
        value: impl Into<String>,
        category: MemoryCategory,
        source: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            value: value.into(),
            category,
            confidence: 1.0,
            source: source.into(),
            created_at: now,
            updated_at: now,
            related: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ServiceType
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    HttpApi,
    Database,
    MessageQueue,
    Cache,
    ExternalApi,
    FileSystem,
}

// ---------------------------------------------------------------------------
// Project index types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub language: String,
    pub lines: u64,
    pub last_modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub service_type: ServiceType,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInfo {
    pub name: String,
    pub version: String,
    pub dep_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndex {
    pub files: Vec<FileInfo>,
    pub services: Vec<ServiceInfo>,
    pub dependencies: Vec<DependencyInfo>,
    pub indexed_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// MemoryStore
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct MemoryStore {
    entries: Vec<MemoryEntry>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn add_entry(&mut self, mut entry: MemoryEntry) -> Uuid {
        let id = entry.id;
        entry.created_at = Utc::now();
        entry.updated_at = entry.created_at;
        self.entries.push(entry);
        id
    }

    pub fn get_entry(&self, id: &Uuid) -> Option<&MemoryEntry> {
        self.entries.iter().find(|e| e.id == *id)
    }

    /// Simple substring search across key and value fields.
    pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
        let q = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.key.to_lowercase().contains(&q) || e.value.to_lowercase().contains(&q))
            .collect()
    }

    pub fn list_by_category(&self, category: &MemoryCategory) -> Vec<&MemoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.category == *category)
            .collect()
    }

    pub fn link_entries(&mut self, from: &Uuid, to: &Uuid) -> Result<(), IntelligenceError> {
        // Validate both entries exist
        if self.get_entry(to).is_none() {
            return Err(IntelligenceError::NotFound {
                entity: "memory_entry".into(),
                id: *to,
            });
        }

        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == *from)
            .ok_or(IntelligenceError::NotFound {
                entity: "memory_entry".into(),
                id: *from,
            })?;

        if !entry.related.contains(to) {
            entry.related.push(*to);
            entry.updated_at = Utc::now();
        }
        Ok(())
    }

    pub fn update_entry(&mut self, id: &Uuid, value: &str) -> Result<(), IntelligenceError> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == *id)
            .ok_or(IntelligenceError::NotFound {
                entity: "memory_entry".into(),
                id: *id,
            })?;

        entry.value = value.to_string();
        entry.updated_at = Utc::now();
        Ok(())
    }

    pub fn delete_entry(&mut self, id: &Uuid) -> bool {
        let len_before = self.entries.len();
        self.entries.retain(|e| e.id != *id);
        self.entries.len() < len_before
    }

    /// Build a lightweight project index by scanning the given root path.
    ///
    /// This is a best-effort scan: it walks the directory tree and collects
    /// file metadata. In production this would be more sophisticated, but
    /// for now it provides the structural foundation.
    pub fn build_project_index(&self, root_path: &str) -> ProjectIndex {
        let mut files = Vec::new();
        let mut dependencies = Vec::new();

        if let Ok(entries) = std::fs::read_dir(root_path) {
            for dir_entry in entries.flatten() {
                let path = dir_entry.path();
                if path.is_file() {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_string();
                    let language = match ext.as_str() {
                        "rs" => "rust",
                        "ts" | "tsx" => "typescript",
                        "js" | "jsx" => "javascript",
                        "toml" => "toml",
                        "json" => "json",
                        "yaml" | "yml" => "yaml",
                        _ => "unknown",
                    }
                    .to_string();

                    let lines = std::fs::read_to_string(&path)
                        .map(|c| c.lines().count() as u64)
                        .unwrap_or(0);

                    let last_modified = dir_entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .map(|t| DateTime::<Utc>::from(t))
                        .unwrap_or_else(|_| Utc::now());

                    files.push(FileInfo {
                        path: path.to_string_lossy().to_string(),
                        language,
                        lines,
                        last_modified,
                    });

                    // Parse Cargo.toml for Rust dependencies
                    if path.file_name().is_some_and(|n| n == "Cargo.toml") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            for line in content.lines() {
                                let trimmed = line.trim();
                                if let Some((name, rest)) = trimmed.split_once('=') {
                                    let name = name.trim();
                                    if !name.is_empty()
                                        && !name.starts_with('[')
                                        && !["name", "version", "edition", "license", "rust-version", "resolver", "path", "members"]
                                            .contains(&name)
                                    {
                                        let version = rest
                                            .trim()
                                            .trim_matches('"')
                                            .trim_matches('{')
                                            .trim();
                                        dependencies.push(DependencyInfo {
                                            name: name.to_string(),
                                            version: version.to_string(),
                                            dep_type: "cargo".to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        ProjectIndex {
            files,
            services: Vec::new(),
            dependencies,
            indexed_at: Utc::now(),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}
