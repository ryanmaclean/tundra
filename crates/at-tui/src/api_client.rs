//! Blocking HTTP client for at-bridge REST API.
//!
//! All methods use `reqwest::blocking` so they can be called from a
//! background `std::thread` without an async runtime.

use at_api_types::*;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

/// Reusable blocking client + base URL.
pub struct ApiClient {
    client: reqwest::blocking::Client,
    base: String,
}

// ── Aggregate snapshot sent over the flume channel ──

#[derive(Debug, Clone, Default)]
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
    /// Outcome of this refresh cycle; data fields hold defaults when it failed.
    pub status: FetchStatus,
}

/// Result of one `fetch_all` cycle, so callers never mistake a rejected or
/// unreachable daemon for an empty one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FetchStatus {
    /// Requests that returned 2xx and parsed.
    pub succeeded: usize,
    /// Requests that failed for any reason.
    pub failed: usize,
    /// At least one request got HTTP 401 (missing or wrong API key).
    pub unauthorized: bool,
}

impl FetchStatus {
    /// The daemon answered at least one request successfully.
    pub fn connected(&self) -> bool {
        self.succeeded > 0
    }
}

/// Thread-safe counters shared by the parallel fetches of one cycle.
#[derive(Default)]
struct Tally {
    ok: AtomicUsize,
    err: AtomicUsize,
    unauthorized: AtomicBool,
}

impl Tally {
    fn record<T: Default>(&self, result: Result<T, String>) -> T {
        match result {
            Ok(v) => {
                self.ok.fetch_add(1, Ordering::Relaxed);
                v
            }
            Err(e) => {
                self.err.fetch_add(1, Ordering::Relaxed);
                if is_unauthorized_error(&e) {
                    self.unauthorized.store(true, Ordering::Relaxed);
                }
                T::default()
            }
        }
    }

    fn status(&self) -> FetchStatus {
        FetchStatus {
            succeeded: self.ok.load(Ordering::Relaxed),
            failed: self.err.load(Ordering::Relaxed),
            unauthorized: self.unauthorized.load(Ordering::Relaxed),
        }
    }
}

/// Marker embedded in errors for HTTP 401 responses.
const UNAUTHORIZED_MARKER: &str = "HTTP 401";

/// Whether an error string from [`ApiClient`] denotes an HTTP 401.
pub fn is_unauthorized_error(err: &str) -> bool {
    err.contains(UNAUTHORIZED_MARKER)
}

impl ApiClient {
    /// Client for `base`, authenticating with the key discovered the same way
    /// as the CLI (`AUTO_TUNDRA_API_KEY`, else `~/.auto-tundra/daemon.key`).
    pub fn new(base: &str) -> Self {
        Self::with_api_key(
            base,
            at_core::config::CredentialProvider::read_daemon_api_key(),
        )
    }

    /// Client for a discovered daemon connection.
    pub fn from_connection(conn: &at_core::lockfile::DaemonConnection) -> Self {
        Self::with_api_key(&conn.api_url, conn.api_key.clone())
    }

    /// Client that sends `api_key` as `X-API-Key` on every request.
    pub fn with_api_key(base: &str, api_key: Option<String>) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(key) = api_key.filter(|k| !k.is_empty()) {
            if let Ok(mut value) = reqwest::header::HeaderValue::from_str(&key) {
                value.set_sensitive(true);
                headers.insert(auth::API_KEY_HEADER, value);
            }
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .default_headers(headers)
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
            // Formats as "HTTP 401 Unauthorized", matched by is_unauthorized_error.
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

    pub fn fetch_bootstrap(&self) -> Result<ApiBootstrap, String> {
        self.get("/api/bootstrap")
    }

    /// Fetch all data in one go. Individual failures are logged but don't
    /// block the rest — each endpoint returns its fallback default.
    ///
    /// Fast path: tries `GET /api/bootstrap` first. On success, beads/agents/kpi
    /// are taken from the single response and the remaining endpoints are still
    /// fetched in parallel (they cover different data: sessions, GitHub, etc.).
    /// On failure (old server, network error) falls back to the full parallel fan-out.
    pub fn fetch_all(&self) -> AppData {
        let profile = std::env::var_os("AT_TUI_PROFILE").is_some();
        let started = Instant::now();

        let tally = Tally::default();

        // Try the one-shot bootstrap first. If it works, skip those 3 thread spawns.
        // (A failed bootstrap is not tallied: old servers lack the route.)
        let bootstrap = timed_fetch(profile, "bootstrap", || match self.fetch_bootstrap() {
            Ok(b) => {
                tally.ok.fetch_add(1, Ordering::Relaxed);
                Some(b)
            }
            Err(e) => {
                if is_unauthorized_error(&e) {
                    tally.unauthorized.store(true, Ordering::Relaxed);
                }
                None
            }
        });

        let tally = &tally;
        let mut data = std::thread::scope(|scope| {
            let agents = match &bootstrap {
                Some(b) => b.agents.clone(),
                None => scope
                    .spawn(|| timed_fetch(profile, "agents", || tally.record(self.fetch_agents())))
                    .join()
                    .unwrap_or_default(),
            };
            let beads = match &bootstrap {
                Some(b) => b.beads.clone(),
                None => scope
                    .spawn(|| timed_fetch(profile, "beads", || tally.record(self.fetch_beads())))
                    .join()
                    .unwrap_or_default(),
            };
            let kpi = match &bootstrap {
                Some(b) => b.kpi.clone(),
                None => scope
                    .spawn(|| timed_fetch(profile, "kpi", || tally.record(self.fetch_kpi())))
                    .join()
                    .unwrap_or_default(),
            };

            let sessions = scope.spawn(|| {
                timed_fetch(profile, "sessions", || {
                    tally.record(self.fetch_sessions())
                })
            });
            let convoys = scope.spawn(|| {
                timed_fetch(profile, "convoys", || {
                    tally.record(self.fetch_convoys())
                })
            });
            let costs = scope
                .spawn(|| timed_fetch(profile, "costs", || tally.record(self.fetch_costs())));
            let mcp_servers = scope.spawn(|| {
                timed_fetch(profile, "mcp_servers", || {
                    tally.record(self.fetch_mcp_servers())
                })
            });
            let worktrees = scope.spawn(|| {
                timed_fetch(profile, "worktrees", || {
                    tally.record(self.fetch_worktrees())
                })
            });
            let github_issues = scope.spawn(|| {
                timed_fetch(profile, "github_issues", || {
                    tally.record(self.fetch_github_issues())
                })
            });
            let github_prs = scope.spawn(|| {
                timed_fetch(profile, "github_prs", || {
                    tally.record(self.fetch_github_prs())
                })
            });
            let roadmap_items = scope.spawn(|| {
                timed_fetch(profile, "roadmap", || {
                    tally.record(self.fetch_roadmap())
                })
            });
            let ideas = scope
                .spawn(|| timed_fetch(profile, "ideas", || tally.record(self.fetch_ideas())));
            let stacks = scope.spawn(|| {
                timed_fetch(profile, "stacks", || {
                    tally.record(self.fetch_stacks())
                })
            });
            let changelog = scope.spawn(|| {
                timed_fetch(profile, "changelog", || {
                    tally.record(self.fetch_changelog())
                })
            });
            let memory = scope.spawn(|| {
                timed_fetch(profile, "memory", || {
                    tally.record(self.fetch_memory())
                })
            });

            AppData {
                agents,
                beads,
                kpi,
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
                status: FetchStatus::default(),
            }
        });
        data.status = tally.status();

        if profile {
            eprintln!(
                "[at-tui] fetch_all total={}ms",
                started.elapsed().as_millis()
            );
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

#[cfg(test)]
mod auth_tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// Minimal blocking HTTP server: answers 200 `[]` when the request carries
    /// `x-api-key: <key>`, else 401. Returns the base URL.
    fn start_auth_server(key: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut authorized = false;
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    let l = line.trim_end().to_ascii_lowercase();
                    if l.is_empty() {
                        break;
                    }
                    if l == format!("x-api-key: {key}") {
                        authorized = true;
                    }
                    line.clear();
                }
                let (status, body) = if authorized {
                    ("200 OK", "[]")
                } else {
                    ("401 Unauthorized", "{\"error\":\"unauthorized\"}")
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn sends_api_key_header() {
        let base = start_auth_server("tui-key");
        let ok = ApiClient::with_api_key(&base, Some("tui-key".into()));
        assert!(ok.fetch_agents().is_ok());

        let err = ApiClient::with_api_key(&base, None).fetch_agents().unwrap_err();
        assert!(is_unauthorized_error(&err), "{err}");
    }

    #[test]
    fn fetch_all_reports_unauthorized_instead_of_connected() {
        let base = start_auth_server("tui-key");
        let data = ApiClient::with_api_key(&base, Some("wrong".into())).fetch_all();
        assert!(!data.status.connected());
        assert!(data.status.unauthorized);
        assert!(data.status.failed > 0);
    }

    #[test]
    fn fetch_all_reports_connected_with_key() {
        let base = start_auth_server("tui-key");
        let data = ApiClient::with_api_key(&base, Some("tui-key".into())).fetch_all();
        assert!(data.status.connected());
        assert!(!data.status.unauthorized);
    }
}
