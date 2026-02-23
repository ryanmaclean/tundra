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

        let entry =
            self.entries
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
        let entry =
            self.entries
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
                                        && ![
                                            "name",
                                            "version",
                                            "edition",
                                            "license",
                                            "rust-version",
                                            "resolver",
                                            "path",
                                            "members",
                                        ]
                                        .contains(&name)
                                    {
                                        let version =
                                            rest.trim().trim_matches('"').trim_matches('{').trim();
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

// ---------------------------------------------------------------------------
// GraphMemory — relational memory with traversal, decay, and persistence
// ---------------------------------------------------------------------------

/// Edge type connecting two memory nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// A depends on B.
    DependsOn,
    /// A is related to B.
    RelatedTo,
    /// A is a child/subtopic of B.
    ChildOf,
    /// A supersedes B.
    Supersedes,
    /// A was derived from B.
    DerivedFrom,
    /// A conflicts with B.
    ConflictsWith,
}

/// A directional edge in the memory graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEdge {
    pub from: Uuid,
    pub to: Uuid,
    pub kind: EdgeKind,
    pub weight: f64,
    pub created_at: DateTime<Utc>,
}

/// A topic cluster grouping related memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicCluster {
    pub id: Uuid,
    pub label: String,
    pub member_ids: Vec<Uuid>,
    pub centroid_keywords: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl TopicCluster {
    pub fn new(label: impl Into<String>, keywords: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            member_ids: Vec::new(),
            centroid_keywords: keywords,
            created_at: Utc::now(),
        }
    }

    pub fn add_member(&mut self, id: Uuid) {
        if !self.member_ids.contains(&id) {
            self.member_ids.push(id);
        }
    }

    pub fn remove_member(&mut self, id: &Uuid) {
        self.member_ids.retain(|m| m != id);
    }
}

/// Graph-based memory store with edges, clusters, decay, and persistence.
#[derive(Debug, Serialize, Deserialize)]
pub struct GraphMemory {
    pub entries: Vec<MemoryEntry>,
    pub edges: Vec<MemoryEdge>,
    pub clusters: Vec<TopicCluster>,
    /// Base decay rate per day (0.0 = no decay, 1.0 = instant forget).
    pub decay_rate: f64,
}

impl GraphMemory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            edges: Vec::new(),
            clusters: Vec::new(),
            decay_rate: 0.02, // 2% per day
        }
    }

    // -- Entry operations --

    pub fn add_entry(&mut self, entry: MemoryEntry) -> Uuid {
        let id = entry.id;
        self.entries.push(entry);
        id
    }

    pub fn get_entry(&self, id: &Uuid) -> Option<&MemoryEntry> {
        self.entries.iter().find(|e| e.id == *id)
    }

    pub fn get_entry_mut(&mut self, id: &Uuid) -> Option<&mut MemoryEntry> {
        self.entries.iter_mut().find(|e| e.id == *id)
    }

    pub fn remove_entry(&mut self, id: &Uuid) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != *id);
        // Also remove related edges
        self.edges.retain(|e| e.from != *id && e.to != *id);
        // Remove from clusters
        for cluster in &mut self.clusters {
            cluster.remove_member(id);
        }
        self.entries.len() < before
    }

    // -- Edge operations --

    pub fn add_edge(&mut self, from: Uuid, to: Uuid, kind: EdgeKind, weight: f64) {
        self.edges.push(MemoryEdge {
            from,
            to,
            kind,
            weight,
            created_at: Utc::now(),
        });
    }

    pub fn edges_from(&self, id: &Uuid) -> Vec<&MemoryEdge> {
        self.edges.iter().filter(|e| e.from == *id).collect()
    }

    pub fn edges_to(&self, id: &Uuid) -> Vec<&MemoryEdge> {
        self.edges.iter().filter(|e| e.to == *id).collect()
    }

    /// Get all neighbors (both directions) of a node.
    pub fn neighbors(&self, id: &Uuid) -> Vec<Uuid> {
        let mut result = Vec::new();
        for edge in &self.edges {
            if edge.from == *id && !result.contains(&edge.to) {
                result.push(edge.to);
            }
            if edge.to == *id && !result.contains(&edge.from) {
                result.push(edge.from);
            }
        }
        result
    }

    /// BFS traversal from a starting node up to `max_depth` hops.
    /// Returns (entry_id, depth) pairs.
    pub fn traverse(&self, start: &Uuid, max_depth: usize) -> Vec<(Uuid, usize)> {
        let mut visited = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((*start, 0usize));
        visited.push((*start, 0));

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            for neighbor_id in self.neighbors(&current) {
                if !visited.iter().any(|(id, _)| *id == neighbor_id) {
                    visited.push((neighbor_id, depth + 1));
                    queue.push_back((neighbor_id, depth + 1));
                }
            }
        }
        visited
    }

    /// Find the shortest path between two nodes using BFS.
    /// Returns the path as a list of node IDs, or None if no path exists.
    pub fn shortest_path(&self, from: &Uuid, to: &Uuid) -> Option<Vec<Uuid>> {
        if from == to {
            return Some(vec![*from]);
        }

        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut parent: std::collections::HashMap<Uuid, Uuid> = std::collections::HashMap::new();

        visited.insert(*from);
        queue.push_back(*from);

        while let Some(current) = queue.pop_front() {
            for neighbor_id in self.neighbors(&current) {
                if !visited.contains(&neighbor_id) {
                    visited.insert(neighbor_id);
                    parent.insert(neighbor_id, current);

                    if neighbor_id == *to {
                        // Reconstruct path
                        let mut path = vec![neighbor_id];
                        let mut node = neighbor_id;
                        while let Some(&p) = parent.get(&node) {
                            path.push(p);
                            node = p;
                        }
                        path.reverse();
                        return Some(path);
                    }

                    queue.push_back(neighbor_id);
                }
            }
        }
        None
    }

    // -- Cluster operations --

    pub fn create_cluster(&mut self, label: impl Into<String>, keywords: Vec<String>) -> Uuid {
        let cluster = TopicCluster::new(label, keywords);
        let id = cluster.id;
        self.clusters.push(cluster);
        id
    }

    pub fn add_to_cluster(&mut self, cluster_id: &Uuid, entry_id: Uuid) -> bool {
        if let Some(cluster) = self.clusters.iter_mut().find(|c| c.id == *cluster_id) {
            cluster.add_member(entry_id);
            true
        } else {
            false
        }
    }

    pub fn get_cluster(&self, id: &Uuid) -> Option<&TopicCluster> {
        self.clusters.iter().find(|c| c.id == *id)
    }

    /// Get all entries in a cluster.
    pub fn cluster_entries(&self, cluster_id: &Uuid) -> Vec<&MemoryEntry> {
        if let Some(cluster) = self.get_cluster(cluster_id) {
            cluster
                .member_ids
                .iter()
                .filter_map(|id| self.get_entry(id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Auto-assign an entry to clusters based on keyword overlap.
    pub fn auto_cluster(&mut self, entry_id: &Uuid) {
        let entry_text = match self.get_entry(entry_id) {
            Some(e) => format!("{} {}", e.key, e.value).to_lowercase(),
            None => return,
        };
        let entry_id = *entry_id;

        let matching_clusters: Vec<Uuid> = self
            .clusters
            .iter()
            .filter(|c| {
                c.centroid_keywords
                    .iter()
                    .any(|kw| entry_text.contains(&kw.to_lowercase()))
            })
            .map(|c| c.id)
            .collect();

        for cid in matching_clusters {
            self.add_to_cluster(&cid, entry_id);
        }
    }

    // -- Decay --

    /// Apply confidence decay to all entries based on age.
    /// Entries with confidence below `min_confidence` are removed.
    pub fn apply_decay(&mut self, min_confidence: f32) {
        let now = Utc::now();
        for entry in &mut self.entries {
            let age_days = (now - entry.updated_at).num_days().max(0) as f64;
            let decay_factor = (1.0 - self.decay_rate).powf(age_days);
            entry.confidence = (entry.confidence as f64 * decay_factor) as f32;
        }
        // Collect IDs to remove before mutating
        let to_remove: Vec<Uuid> = self
            .entries
            .iter()
            .filter(|e| e.confidence < min_confidence)
            .map(|e| e.id)
            .collect();
        for id in to_remove {
            self.remove_entry(&id);
        }
    }

    /// Boost an entry's confidence (e.g., when it's accessed or confirmed).
    pub fn boost_confidence(&mut self, id: &Uuid, amount: f32) {
        if let Some(entry) = self.get_entry_mut(id) {
            entry.confidence = (entry.confidence + amount).min(1.0);
            entry.updated_at = Utc::now();
        }
    }

    // -- Search --

    /// Search entries by text, weighted by confidence.
    /// Returns entries sorted by (relevance * confidence) descending.
    pub fn search_ranked(&self, query: &str) -> Vec<(&MemoryEntry, f64)> {
        let q = query.to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();

        let mut results: Vec<(&MemoryEntry, f64)> = self
            .entries
            .iter()
            .filter_map(|e| {
                let text = format!("{} {}", e.key, e.value).to_lowercase();
                let match_count = words.iter().filter(|w| text.contains(**w)).count();
                if match_count == 0 {
                    return None;
                }
                let relevance = match_count as f64 / words.len().max(1) as f64;
                let score = relevance * e.confidence as f64;
                Some((e, score))
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    // -- Persistence --

    /// Serialize the entire graph to JSON.
    pub fn to_json(&self) -> std::result::Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> std::result::Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Save to a file path.
    pub fn save_to_file(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)
    }

    /// Load from a file path. Returns a new empty graph if file doesn't exist.
    pub fn load_from_file(path: &str) -> std::io::Result<Self> {
        if !std::path::Path::new(path).exists() {
            return Ok(Self::new());
        }
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    // -- Stats --

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn cluster_count(&self) -> usize {
        self.clusters.len()
    }
}

impl Default for GraphMemory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// GraphMemory tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod graph_tests {
    use super::*;

    fn make_entry(key: &str, value: &str, cat: MemoryCategory) -> MemoryEntry {
        MemoryEntry::new(key, value, cat, "test")
    }

    #[test]
    fn graph_add_and_get() {
        let mut g = GraphMemory::new();
        let e = make_entry("api_url", "http://localhost", MemoryCategory::ApiRoute);
        let id = e.id;
        g.add_entry(e);
        assert!(g.get_entry(&id).is_some());
        assert_eq!(g.entry_count(), 1);
    }

    #[test]
    fn graph_remove_entry_cleans_edges() {
        let mut g = GraphMemory::new();
        let e1 = make_entry("a", "1", MemoryCategory::Pattern);
        let e2 = make_entry("b", "2", MemoryCategory::Pattern);
        let id1 = e1.id;
        let id2 = e2.id;
        g.add_entry(e1);
        g.add_entry(e2);
        g.add_edge(id1, id2, EdgeKind::RelatedTo, 1.0);
        assert_eq!(g.edge_count(), 1);

        g.remove_entry(&id1);
        assert_eq!(g.entry_count(), 1);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn graph_edges_from_and_to() {
        let mut g = GraphMemory::new();
        let e1 = make_entry("a", "1", MemoryCategory::Pattern);
        let e2 = make_entry("b", "2", MemoryCategory::Pattern);
        let id1 = e1.id;
        let id2 = e2.id;
        g.add_entry(e1);
        g.add_entry(e2);
        g.add_edge(id1, id2, EdgeKind::DependsOn, 0.8);

        assert_eq!(g.edges_from(&id1).len(), 1);
        assert_eq!(g.edges_to(&id2).len(), 1);
        assert_eq!(g.edges_from(&id2).len(), 0);
    }

    #[test]
    fn graph_neighbors() {
        let mut g = GraphMemory::new();
        let e1 = make_entry("a", "1", MemoryCategory::Pattern);
        let e2 = make_entry("b", "2", MemoryCategory::Pattern);
        let e3 = make_entry("c", "3", MemoryCategory::Pattern);
        let id1 = e1.id;
        let id2 = e2.id;
        let id3 = e3.id;
        g.add_entry(e1);
        g.add_entry(e2);
        g.add_entry(e3);
        g.add_edge(id1, id2, EdgeKind::RelatedTo, 1.0);
        g.add_edge(id3, id1, EdgeKind::DerivedFrom, 1.0);

        let neighbors = g.neighbors(&id1);
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&id2));
        assert!(neighbors.contains(&id3));
    }

    #[test]
    fn graph_traverse_bfs() {
        let mut g = GraphMemory::new();
        let e1 = make_entry("a", "1", MemoryCategory::Pattern);
        let e2 = make_entry("b", "2", MemoryCategory::Pattern);
        let e3 = make_entry("c", "3", MemoryCategory::Pattern);
        let e4 = make_entry("d", "4", MemoryCategory::Pattern);
        let id1 = e1.id;
        let id2 = e2.id;
        let id3 = e3.id;
        let id4 = e4.id;
        g.add_entry(e1);
        g.add_entry(e2);
        g.add_entry(e3);
        g.add_entry(e4);
        // Chain: 1->2->3->4
        g.add_edge(id1, id2, EdgeKind::RelatedTo, 1.0);
        g.add_edge(id2, id3, EdgeKind::RelatedTo, 1.0);
        g.add_edge(id3, id4, EdgeKind::RelatedTo, 1.0);

        // Depth 1: should get id1, id2
        let result = g.traverse(&id1, 1);
        assert_eq!(result.len(), 2);

        // Depth 3: should get all 4
        let result = g.traverse(&id1, 3);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn graph_shortest_path() {
        let mut g = GraphMemory::new();
        let ids: Vec<Uuid> = (0..4)
            .map(|i| {
                let e = make_entry(&format!("n{i}"), &format!("v{i}"), MemoryCategory::Pattern);
                let id = e.id;
                g.add_entry(e);
                id
            })
            .collect();
        // 0->1->2->3
        g.add_edge(ids[0], ids[1], EdgeKind::RelatedTo, 1.0);
        g.add_edge(ids[1], ids[2], EdgeKind::RelatedTo, 1.0);
        g.add_edge(ids[2], ids[3], EdgeKind::RelatedTo, 1.0);

        let path = g.shortest_path(&ids[0], &ids[3]).unwrap();
        assert_eq!(path.len(), 4);
        assert_eq!(path[0], ids[0]);
        assert_eq!(path[3], ids[3]);

        // Self path
        assert_eq!(g.shortest_path(&ids[0], &ids[0]).unwrap(), vec![ids[0]]);

        // No path
        let isolated = make_entry("x", "y", MemoryCategory::Pattern);
        let iso_id = isolated.id;
        g.add_entry(isolated);
        assert!(g.shortest_path(&ids[0], &iso_id).is_none());
    }

    #[test]
    fn graph_clusters() {
        let mut g = GraphMemory::new();
        let e1 = make_entry("auth_service", "handles login", MemoryCategory::Pattern);
        let e2 = make_entry("auth_token", "jwt token", MemoryCategory::Pattern);
        let id1 = e1.id;
        let id2 = e2.id;
        g.add_entry(e1);
        g.add_entry(e2);

        let cid = g.create_cluster("Authentication", vec!["auth".into(), "login".into()]);
        g.add_to_cluster(&cid, id1);
        g.add_to_cluster(&cid, id2);

        let members = g.cluster_entries(&cid);
        assert_eq!(members.len(), 2);
        assert_eq!(g.cluster_count(), 1);
    }

    #[test]
    fn graph_auto_cluster() {
        let mut g = GraphMemory::new();
        g.create_cluster(
            "Database",
            vec!["postgres".into(), "database".into(), "sql".into()],
        );
        g.create_cluster("Auth", vec!["auth".into(), "token".into()]);

        let e = make_entry(
            "db_url",
            "postgres://localhost/mydb",
            MemoryCategory::ServiceEndpoint,
        );
        let id = e.id;
        g.add_entry(e);
        g.auto_cluster(&id);

        // Should be in Database cluster but not Auth
        assert!(g.clusters[0].member_ids.contains(&id));
        assert!(!g.clusters[1].member_ids.contains(&id));
    }

    #[test]
    fn graph_confidence_decay() {
        let mut g = GraphMemory::new();
        let mut e = make_entry("old_api", "deprecated endpoint", MemoryCategory::ApiRoute);
        // Simulate an old entry
        e.updated_at = Utc::now() - chrono::Duration::days(100);
        e.confidence = 0.5;
        g.add_entry(e);

        g.apply_decay(0.1);
        // After 100 days at 2% decay, 0.5 * 0.98^100 ≈ 0.066 — should be removed
        assert_eq!(g.entry_count(), 0);
    }

    #[test]
    fn graph_boost_confidence() {
        let mut g = GraphMemory::new();
        let mut e = make_entry("k", "v", MemoryCategory::Pattern);
        e.confidence = 0.5;
        let id = e.id;
        g.add_entry(e);

        g.boost_confidence(&id, 0.3);
        assert!((g.get_entry(&id).unwrap().confidence - 0.8).abs() < 0.01);

        // Clamp to 1.0
        g.boost_confidence(&id, 0.5);
        assert!((g.get_entry(&id).unwrap().confidence - 1.0).abs() < 0.01);
    }

    #[test]
    fn graph_search_ranked() {
        let mut g = GraphMemory::new();
        let mut e1 = make_entry(
            "api_auth",
            "authentication endpoint for users",
            MemoryCategory::ApiRoute,
        );
        e1.confidence = 1.0;
        let mut e2 = make_entry(
            "cache_config",
            "redis cache settings",
            MemoryCategory::Convention,
        );
        e2.confidence = 0.5;
        let mut e3 = make_entry(
            "auth_token",
            "jwt auth token handler",
            MemoryCategory::Pattern,
        );
        e3.confidence = 0.8;
        g.add_entry(e1);
        g.add_entry(e2);
        g.add_entry(e3);

        let results = g.search_ranked("auth");
        assert_eq!(results.len(), 2); // e1 and e3
                                      // e1 should be first (confidence 1.0 > 0.8)
        assert_eq!(results[0].0.key, "api_auth");
    }

    #[test]
    fn graph_persistence_roundtrip() {
        let mut g = GraphMemory::new();
        let e1 = make_entry("k1", "v1", MemoryCategory::Pattern);
        let e2 = make_entry("k2", "v2", MemoryCategory::Convention);
        let id1 = e1.id;
        let id2 = e2.id;
        g.add_entry(e1);
        g.add_entry(e2);
        g.add_edge(id1, id2, EdgeKind::RelatedTo, 0.9);
        g.create_cluster("test", vec!["k1".into()]);

        let json = g.to_json().unwrap();
        let loaded = GraphMemory::from_json(&json).unwrap();
        assert_eq!(loaded.entry_count(), 2);
        assert_eq!(loaded.edge_count(), 1);
        assert_eq!(loaded.cluster_count(), 1);
    }

    #[test]
    fn graph_file_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.json");
        let path_str = path.to_str().unwrap();

        let mut g = GraphMemory::new();
        g.add_entry(make_entry("k", "v", MemoryCategory::Pattern));
        g.save_to_file(path_str).unwrap();

        let loaded = GraphMemory::load_from_file(path_str).unwrap();
        assert_eq!(loaded.entry_count(), 1);
    }

    #[test]
    fn graph_load_nonexistent_returns_empty() {
        let g = GraphMemory::load_from_file("/nonexistent/path/memory.json").unwrap();
        assert_eq!(g.entry_count(), 0);
    }
}
