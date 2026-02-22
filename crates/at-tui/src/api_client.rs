//! Blocking HTTP client for at-bridge REST API.
//!
//! All methods use `reqwest::blocking` so they can be called from a
//! background `std::thread` without an async runtime.

use serde::Deserialize;

/// Reusable blocking client + base URL.
pub struct ApiClient {
    client: reqwest::blocking::Client,
    base: String,
}

// ── API response types (matching backend JSON) ──

#[derive(Debug, Clone, Deserialize)]
pub struct ApiAgent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiBead {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub lane: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiKpi {
    #[serde(default)]
    pub total_beads: u64,
    #[serde(default)]
    pub backlog: u64,
    #[serde(default)]
    pub hooked: u64,
    #[serde(default)]
    pub slung: u64,
    #[serde(default)]
    pub review: u64,
    #[serde(default)]
    pub done: u64,
    #[serde(default)]
    pub failed: u64,
    #[serde(default)]
    pub active_agents: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiSession {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub agent_name: String,
    #[serde(default)]
    pub cli_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub duration: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConvoy {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub bead_count: u32,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub bead_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiCosts {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub sessions: Vec<ApiCostSession>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiCostSession {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub agent_name: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiMcpServer {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiWorktree {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub bead_id: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiGithubIssue {
    #[serde(default)]
    pub number: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub created: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiGithubPr {
    #[serde(default)]
    pub number: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub reviewers: Vec<String>,
    #[serde(default)]
    pub created: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRoadmapFeature {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRoadmap {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub features: Vec<ApiRoadmapFeature>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRoadmapItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiIdea {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub impact: String,
    #[serde(default)]
    pub effort: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiStackNode {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub pr_number: Option<u32>,
    #[serde(default)]
    pub stack_position: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiStack {
    pub root: ApiStackNode,
    #[serde(default)]
    pub children: Vec<ApiStackNode>,
    #[serde(default)]
    pub total: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiChangelogSection {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiChangelogEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub sections: Vec<ApiChangelogSection>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiMemoryEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub created_at: String,
}

// ── Aggregate snapshot sent over the flume channel ──

#[derive(Debug, Clone)]
pub struct AppData {
    pub agents: Vec<ApiAgent>,
    pub beads: Vec<ApiBead>,
    pub kpi: ApiKpi,
    pub sessions: Vec<ApiSession>,
    pub convoys: Vec<ApiConvoy>,
    pub costs: ApiCosts,
    pub mcp_servers: Vec<ApiMcpServer>,
    pub worktrees: Vec<ApiWorktree>,
    pub github_issues: Vec<ApiGithubIssue>,
    pub github_prs: Vec<ApiGithubPr>,
    pub roadmap_items: Vec<ApiRoadmapItem>,
    pub ideas: Vec<ApiIdea>,
    pub stacks: Vec<ApiStack>,
    pub changelog: Vec<ApiChangelogEntry>,
    pub memory: Vec<ApiMemoryEntry>,
}

impl ApiClient {
    pub fn new(base: &str) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self {
            client,
            base: base.trim_end_matches('/').to_string(),
        }
    }

    fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, String> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .map_err(|e| format!("GET {path}: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("GET {path}: HTTP {}", resp.status()));
        }
        resp.json::<T>()
            .map_err(|e| format!("GET {path} parse: {e}"))
    }

    pub fn fetch_agents(&self) -> Result<Vec<ApiAgent>, String> {
        self.get("/api/agents")
    }

    pub fn fetch_beads(&self) -> Result<Vec<ApiBead>, String> {
        self.get("/api/beads")
    }

    pub fn fetch_kpi(&self) -> Result<ApiKpi, String> {
        self.get("/api/kpi")
    }

    pub fn fetch_sessions(&self) -> Result<Vec<ApiSession>, String> {
        self.get("/api/sessions")
    }

    pub fn fetch_convoys(&self) -> Result<Vec<ApiConvoy>, String> {
        self.get("/api/convoys")
    }

    pub fn fetch_costs(&self) -> Result<ApiCosts, String> {
        self.get("/api/costs")
    }

    pub fn fetch_mcp_servers(&self) -> Result<Vec<ApiMcpServer>, String> {
        self.get("/api/mcp/servers")
    }

    pub fn fetch_worktrees(&self) -> Result<Vec<ApiWorktree>, String> {
        self.get("/api/worktrees")
    }

    pub fn fetch_github_issues(&self) -> Result<Vec<ApiGithubIssue>, String> {
        self.get("/api/github/issues")
    }

    pub fn fetch_github_prs(&self) -> Result<Vec<ApiGithubPr>, String> {
        self.get("/api/github/prs")
    }

    pub fn fetch_roadmap(&self) -> Result<Vec<ApiRoadmapItem>, String> {
        let roadmaps: Vec<ApiRoadmap> = self.get("/api/roadmap")?;
        Ok(flatten_roadmaps(roadmaps))
    }

    pub fn fetch_ideas(&self) -> Result<Vec<ApiIdea>, String> {
        self.get("/api/ideation/ideas")
    }

    pub fn fetch_stacks(&self) -> Result<Vec<ApiStack>, String> {
        self.get("/api/stacks")
    }

    pub fn fetch_changelog(&self) -> Result<Vec<ApiChangelogEntry>, String> {
        self.get("/api/changelog")
    }

    pub fn fetch_memory(&self) -> Result<Vec<ApiMemoryEntry>, String> {
        self.get("/api/memory")
    }

    /// Fetch all data in one go. Individual failures are logged but don't
    /// block the rest — each endpoint returns its fallback default.
    pub fn fetch_all(&self) -> AppData {
        AppData {
            agents: self.fetch_agents().unwrap_or_default(),
            beads: self.fetch_beads().unwrap_or_default(),
            kpi: self.fetch_kpi().unwrap_or_else(|_| ApiKpi {
                total_beads: 0,
                backlog: 0,
                hooked: 0,
                slung: 0,
                review: 0,
                done: 0,
                failed: 0,
                active_agents: 0,
            }),
            sessions: self.fetch_sessions().unwrap_or_default(),
            convoys: self.fetch_convoys().unwrap_or_default(),
            costs: self.fetch_costs().unwrap_or_else(|_| ApiCosts {
                input_tokens: 0,
                output_tokens: 0,
                sessions: vec![],
            }),
            mcp_servers: self.fetch_mcp_servers().unwrap_or_default(),
            worktrees: self.fetch_worktrees().unwrap_or_default(),
            github_issues: self.fetch_github_issues().unwrap_or_default(),
            github_prs: self.fetch_github_prs().unwrap_or_default(),
            roadmap_items: self.fetch_roadmap().unwrap_or_default(),
            ideas: self.fetch_ideas().unwrap_or_default(),
            stacks: self.fetch_stacks().unwrap_or_default(),
            changelog: self.fetch_changelog().unwrap_or_default(),
            memory: self.fetch_memory().unwrap_or_default(),
        }
    }
}

fn flatten_roadmaps(roadmaps: Vec<ApiRoadmap>) -> Vec<ApiRoadmapItem> {
    roadmaps
        .into_iter()
        .flat_map(|r| {
            r.features.into_iter().map(|f| ApiRoadmapItem {
                id: f.id,
                title: f.title,
                description: f.description,
                status: f.status,
                priority: match f.priority {
                    0..=3 => "high".to_string(),
                    4..=6 => "medium".to_string(),
                    _ => "low".to_string(),
                },
            })
        })
        .collect()
}
