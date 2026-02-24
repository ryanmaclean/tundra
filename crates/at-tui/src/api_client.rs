//! Blocking HTTP client for at-bridge REST API.
//!
//! All methods use `reqwest::blocking` so they can be called from a
//! background `std::thread` without an async runtime.

use serde::Deserialize;
use std::time::Instant;

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

#[derive(Debug, Clone, Deserialize, Default)]
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

#[derive(Debug, Clone, Deserialize, Default)]
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
        let profile = std::env::var_os("AT_TUI_PROFILE").is_some();
        let started = Instant::now();

        let data = std::thread::scope(|scope| {
            let agents = scope.spawn(|| {
                timed_fetch(profile, "agents", || self.fetch_agents().unwrap_or_default())
            });
            let beads = scope
                .spawn(|| timed_fetch(profile, "beads", || self.fetch_beads().unwrap_or_default()));
            let kpi =
                scope.spawn(|| timed_fetch(profile, "kpi", || self.fetch_kpi().unwrap_or_default()));
            let sessions = scope.spawn(|| {
                timed_fetch(profile, "sessions", || self.fetch_sessions().unwrap_or_default())
            });
            let convoys = scope.spawn(|| {
                timed_fetch(profile, "convoys", || self.fetch_convoys().unwrap_or_default())
            });
            let costs = scope.spawn(|| {
                timed_fetch(profile, "costs", || self.fetch_costs().unwrap_or_default())
            });
            let mcp_servers = scope.spawn(|| {
                timed_fetch(profile, "mcp_servers", || {
                    self.fetch_mcp_servers().unwrap_or_default()
                })
            });
            let worktrees = scope.spawn(|| {
                timed_fetch(profile, "worktrees", || self.fetch_worktrees().unwrap_or_default())
            });
            let github_issues = scope.spawn(|| {
                timed_fetch(profile, "github_issues", || {
                    self.fetch_github_issues().unwrap_or_default()
                })
            });
            let github_prs = scope.spawn(|| {
                timed_fetch(profile, "github_prs", || {
                    self.fetch_github_prs().unwrap_or_default()
                })
            });
            let roadmap_items = scope.spawn(|| {
                timed_fetch(profile, "roadmap", || self.fetch_roadmap().unwrap_or_default())
            });
            let ideas =
                scope.spawn(|| timed_fetch(profile, "ideas", || self.fetch_ideas().unwrap_or_default()));
            let stacks = scope.spawn(|| {
                timed_fetch(profile, "stacks", || self.fetch_stacks().unwrap_or_default())
            });
            let changelog = scope.spawn(|| {
                timed_fetch(profile, "changelog", || self.fetch_changelog().unwrap_or_default())
            });
            let memory = scope.spawn(|| {
                timed_fetch(profile, "memory", || self.fetch_memory().unwrap_or_default())
            });

            AppData {
                agents: agents.join().unwrap_or_default(),
                beads: beads.join().unwrap_or_default(),
                kpi: kpi.join().unwrap_or_default(),
                sessions: sessions.join().unwrap_or_default(),
                convoys: convoys.join().unwrap_or_default(),
                costs: costs.join().unwrap_or_default(),
                mcp_servers: mcp_servers.join().unwrap_or_default(),
                worktrees: worktrees.join().unwrap_or_default(),
                github_issues: github_issues.join().unwrap_or_default(),
                github_prs: github_prs.join().unwrap_or_default(),
                roadmap_items: roadmap_items.join().unwrap_or_default(),
                ideas: ideas.join().unwrap_or_default(),
                stacks: stacks.join().unwrap_or_default(),
                changelog: changelog.join().unwrap_or_default(),
                memory: memory.join().unwrap_or_default(),
            }
        });

        if profile {
            eprintln!("[at-tui] fetch_all total={}ms", started.elapsed().as_millis());
        }

        data
    }
}

fn timed_fetch<T, F>(enabled: bool, label: &'static str, fetch: F) -> T
where
    F: FnOnce() -> T,
{
    if !enabled {
        return fetch();
    }
    let started = Instant::now();
    let out = fetch();
    eprintln!("[at-tui] fetch {label}={}ms", started.elapsed().as_millis());
    out
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
