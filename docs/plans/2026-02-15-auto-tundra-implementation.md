# auto-tundra Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a high-performance Rust + Tauri + Leptos multi-agent terminal orchestrator that manages Claude Code, Codex CLI, Gemini CLI, and OpenCode agents with a native desktop dashboard.

**Architecture:** Cargo workspace with 8 crates (at-core, at-harness, at-session, at-agents, at-daemon, at-telemetry, at-cli, at-bridge) plus a Tauri desktop app with Leptos WASM frontend. Dolt DB for git-versioned data, SQLite for caching, OpenTelemetry for observability, Vector for log routing.

**Tech Stack:** Rust 1.91+, Tauri 2.10, Leptos (WASM), Dolt DB, SQLite, genai, rig-core, claude-agent-sdk-rs, alacritty_terminal, portable-pty, expectrl, ratatui, xterm.js, OpenTelemetry, Vector, Zellij (sidecar)

**Design Doc:** `docs/plans/2026-02-15-auto-tundra-design.md`

---

## Phase 0: Environment Setup

### Task 0.1: Install Required Tooling

**Step 1: Install WASM target and Leptos toolchain**

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
cargo install cargo-leptos
```

**Step 2: Install Tauri CLI**

```bash
cargo install tauri-cli
```

**Step 3: Install Dolt**

```bash
brew install dolt
# Or: curl -L https://github.com/dolthub/dolt/releases/latest/download/install.sh | bash
dolt version
```

**Step 4: Install Zellij**

```bash
brew install zellij
zellij --version
```

**Step 5: Install Vector**

```bash
brew install vector
vector --version
```

**Step 6: Verify**

Run: `rustup target list --installed | grep wasm && dolt version && zellij --version && trunk --version`
Expected: All installed and reporting versions.

**Step 7: Commit environment notes**

```bash
# No commit yet - workspace doesn't exist
```

---

### Task 0.2: Create Workspace and Initialize Git

**Step 1: Create project directory**

```bash
mkdir -p /Users/studio/auto-tundra
cd /Users/studio/auto-tundra
git init
```

**Step 2: Create workspace Cargo.toml**

Create: `/Users/studio/auto-tundra/Cargo.toml`

```toml
[workspace]
resolver = "2"
members = [
    "crates/at-core",
    "crates/at-harness",
    "crates/at-session",
    "crates/at-agents",
    "crates/at-daemon",
    "crates/at-telemetry",
    "crates/at-cli",
    "crates/at-bridge",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
rust-version = "1.91"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
async-trait = "0.1"
flume = "0.11"
dashmap = "6"
```

**Step 3: Create .gitignore**

Create: `/Users/studio/auto-tundra/.gitignore`

```
/target
*.db
*.db-shm
*.db-wal
.env
.env.*
/dolt/.dolt/
/node_modules/
```

**Step 4: Create directory structure**

```bash
mkdir -p crates/{at-core,at-harness,at-session,at-agents,at-daemon,at-telemetry,at-cli,at-bridge}/src
mkdir -p app/{tauri,leptos-ui}/src
mkdir -p dolt
mkdir -p docs/plans
```

**Step 5: Copy design doc**

```bash
cp /Users/studio/rust-harness/docs/plans/2026-02-15-auto-tundra-design.md docs/plans/
```

**Step 6: Initial commit**

```bash
git add .
git commit -m "chore: initialize auto-tundra workspace"
```

---

## Phase 1: Foundation (at-core + at-telemetry + at-cli)

### Task 1.1: at-telemetry Crate (Logging + Tracing)

**Files:**
- Create: `crates/at-telemetry/Cargo.toml`
- Create: `crates/at-telemetry/src/lib.rs`
- Create: `crates/at-telemetry/src/logging.rs`
- Test: `crates/at-telemetry/tests/logging_test.rs`

**Step 1: Create Cargo.toml**

Create: `crates/at-telemetry/Cargo.toml`

```toml
[package]
name = "at-telemetry"
version.workspace = true
edition.workspace = true

[dependencies]
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
opentelemetry = { version = "0.29", features = ["trace", "metrics"] }
opentelemetry-otlp = { version = "0.29", features = ["tonic"] }
opentelemetry_sdk = { version = "0.29", features = ["trace", "metrics", "rt-tokio"] }
tracing-opentelemetry = "0.29"
opentelemetry-semantic-conventions = "0.29"
serde = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
tokio = { workspace = true }
```

**Step 2: Write failing test**

Create: `crates/at-telemetry/tests/logging_test.rs`

```rust
use at_telemetry::logging;

#[test]
fn test_init_logging_does_not_panic() {
    logging::init_logging("test", "debug");
}

#[test]
fn test_init_logging_with_json() {
    logging::init_logging_json("test", "info");
}
```

**Step 3: Run test to verify it fails**

Run: `cargo test -p at-telemetry`
Expected: FAIL - module not found

**Step 4: Write minimal implementation**

Create: `crates/at-telemetry/src/lib.rs`

```rust
pub mod logging;
```

Create: `crates/at-telemetry/src/logging.rs`

```rust
use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize structured logging with human-readable format.
pub fn init_logging(service_name: &str, default_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true),
        )
        .try_init()
        .ok(); // Allow multiple init calls in tests
}

/// Initialize structured logging with JSON format (for Vector ingestion).
pub fn init_logging_json(service_name: &str, default_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .json()
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .flatten_event(true),
        )
        .try_init()
        .ok();
}
```

**Step 5: Run test to verify it passes**

Run: `cargo test -p at-telemetry`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/at-telemetry/
git commit -m "feat(telemetry): add at-telemetry crate with structured logging"
```

---

### Task 1.2: at-core Types and Domain Model

**Files:**
- Create: `crates/at-core/Cargo.toml`
- Create: `crates/at-core/src/lib.rs`
- Create: `crates/at-core/src/types.rs`
- Test: `crates/at-core/tests/types_test.rs`

**Step 1: Create Cargo.toml**

Create: `crates/at-core/Cargo.toml`

```toml
[package]
name = "at-core"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
thiserror = { workspace = true }
at-telemetry = { path = "../at-telemetry" }

[dev-dependencies]
tokio = { workspace = true }
```

**Step 2: Write failing test for core types**

Create: `crates/at-core/tests/types_test.rs`

```rust
use at_core::types::*;

#[test]
fn test_bead_status_transitions() {
    let status = BeadStatus::Backlog;
    assert!(status.can_transition_to(&BeadStatus::Hooked));
    assert!(!status.can_transition_to(&BeadStatus::Done));
}

#[test]
fn test_bead_creation() {
    let bead = Bead::new("Fix auth bug", Lane::Critical);
    assert_eq!(bead.status, BeadStatus::Backlog);
    assert_eq!(bead.lane, Lane::Critical);
    assert!(!bead.id.is_nil());
}

#[test]
fn test_agent_creation() {
    let agent = Agent::new("mayor", AgentRole::Mayor, CliType::Claude);
    assert_eq!(agent.status, AgentStatus::Idle);
    assert_eq!(agent.role, AgentRole::Mayor);
}

#[test]
fn test_lane_ordering() {
    assert!(Lane::Critical > Lane::Standard);
    assert!(Lane::Standard > Lane::Experimental);
}

#[test]
fn test_bead_serialization_roundtrip() {
    let bead = Bead::new("Test bead", Lane::Standard);
    let json = serde_json::to_string(&bead).unwrap();
    let deserialized: Bead = serde_json::from_str(&json).unwrap();
    assert_eq!(bead.id, deserialized.id);
    assert_eq!(bead.title, deserialized.title);
}

#[test]
fn test_agent_status_glyph() {
    assert_eq!(AgentStatus::Active.glyph(), "@");
    assert_eq!(AgentStatus::Idle.glyph(), "*");
    assert_eq!(AgentStatus::Pending.glyph(), "!");
    assert_eq!(AgentStatus::Unknown.glyph(), "?");
}
```

**Step 3: Run test to verify it fails**

Run: `cargo test -p at-core`
Expected: FAIL - types module not found

**Step 4: Write implementation**

Create: `crates/at-core/src/lib.rs`

```rust
pub mod types;
```

Create: `crates/at-core/src/types.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Bead (Work Unit) ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadStatus {
    Backlog,
    Hooked,
    Slung,
    Review,
    Done,
    Failed,
    Escalated,
}

impl BeadStatus {
    pub fn can_transition_to(&self, target: &BeadStatus) -> bool {
        use BeadStatus::*;
        matches!(
            (self, target),
            (Backlog, Hooked)
                | (Hooked, Slung)
                | (Hooked, Backlog) // unhook
                | (Slung, Review)
                | (Slung, Failed)
                | (Slung, Escalated)
                | (Review, Done)
                | (Review, Slung) // rejected
                | (Review, Failed)
                | (Failed, Backlog) // retry
                | (Escalated, Backlog) // retry
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    Experimental = 0,
    Standard = 1,
    Critical = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bead {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: BeadStatus,
    pub lane: Lane,
    pub priority: i32,
    pub agent_id: Option<Uuid>,
    pub convoy_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub hooked_at: Option<DateTime<Utc>>,
    pub slung_at: Option<DateTime<Utc>>,
    pub done_at: Option<DateTime<Utc>>,
    pub git_branch: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

impl Bead {
    pub fn new(title: &str, lane: Lane) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.to_string(),
            description: None,
            status: BeadStatus::Backlog,
            lane,
            priority: 0,
            agent_id: None,
            convoy_id: None,
            created_at: now,
            updated_at: now,
            hooked_at: None,
            slung_at: None,
            done_at: None,
            git_branch: None,
            metadata: None,
        }
    }
}

// --- Agent ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Mayor,
    Deacon,
    Witness,
    Refinery,
    Polecat,
    Crew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliType {
    Claude,
    Codex,
    Gemini,
    OpenCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Active,
    Idle,
    Pending,
    Unknown,
    Stopped,
}

impl AgentStatus {
    pub fn glyph(&self) -> &'static str {
        match self {
            AgentStatus::Active => "@",
            AgentStatus::Idle => "*",
            AgentStatus::Pending => "!",
            AgentStatus::Unknown => "?",
            AgentStatus::Stopped => "x",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub role: AgentRole,
    pub cli_type: CliType,
    pub model: Option<String>,
    pub status: AgentStatus,
    pub rig: Option<String>,
    pub pid: Option<u32>,
    pub session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
}

impl Agent {
    pub fn new(name: &str, role: AgentRole, cli_type: CliType) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            role,
            cli_type,
            model: None,
            status: AgentStatus::Idle,
            rig: None,
            pid: None,
            session_id: None,
            created_at: Utc::now(),
            last_seen: None,
            metadata: None,
        }
    }
}

// --- Convoy (Work Batch) ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvoyStatus {
    Pending,
    Active,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Convoy {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub lane: Lane,
    pub status: ConvoyStatus,
    pub progress: f64,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
}

impl Convoy {
    pub fn new(name: &str, lane: Lane) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: None,
            lane,
            status: ConvoyStatus::Pending,
            progress: 0.0,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            metadata: None,
        }
    }
}

// --- Mail (Inter-Agent Messaging) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mail {
    pub id: Uuid,
    pub from_agent: Uuid,
    pub to_agent: Uuid,
    pub subject: Option<String>,
    pub body: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

// --- Event (Audit Log) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub event_type: String,
    pub actor: Option<Uuid>,
    pub bead_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub rig: Option<String>,
    pub lane: Option<Lane>,
    pub payload: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

// --- Metrics ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetric {
    pub id: Uuid,
    pub agent_id: Option<Uuid>,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub duration_ms: u64,
    pub created_at: DateTime<Utc>,
}

// --- KPI Snapshot ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiSnapshot {
    pub id: Uuid,
    pub beads_active: u32,
    pub beads_completed: u32,
    pub beads_failed: u32,
    pub agents_active: u32,
    pub convoys_active: u32,
    pub total_cost_today: f64,
    pub total_tokens_today: u64,
    pub created_at: DateTime<Utc>,
}
```

**Step 5: Run test to verify it passes**

Run: `cargo test -p at-core`
Expected: PASS (all 6 tests)

**Step 6: Commit**

```bash
git add crates/at-core/
git commit -m "feat(core): add domain types - beads, agents, convoys, mail, events, metrics"
```

---

### Task 1.3: at-core SQLite Cache Layer

**Files:**
- Modify: `crates/at-core/Cargo.toml` (add rusqlite)
- Create: `crates/at-core/src/cache.rs`
- Test: `crates/at-core/tests/cache_test.rs`

**Step 1: Add rusqlite dependency**

Add to `crates/at-core/Cargo.toml` under `[dependencies]`:

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
tokio-rusqlite = "0.6"
tokio = { workspace = true }
```

**Step 2: Write failing test**

Create: `crates/at-core/tests/cache_test.rs`

```rust
use at_core::cache::CacheDb;
use at_core::types::*;

#[tokio::test]
async fn test_cache_create_and_get_bead() {
    let cache = CacheDb::new_in_memory().await.unwrap();
    let bead = Bead::new("Test bead", Lane::Standard);
    cache.upsert_bead(&bead).await.unwrap();
    let retrieved = cache.get_bead(bead.id).await.unwrap().unwrap();
    assert_eq!(retrieved.id, bead.id);
    assert_eq!(retrieved.title, "Test bead");
}

#[tokio::test]
async fn test_cache_list_beads_by_status() {
    let cache = CacheDb::new_in_memory().await.unwrap();

    let b1 = Bead::new("Bead 1", Lane::Critical);
    let mut b2 = Bead::new("Bead 2", Lane::Standard);
    b2.status = BeadStatus::Slung;

    cache.upsert_bead(&b1).await.unwrap();
    cache.upsert_bead(&b2).await.unwrap();

    let backlog = cache.list_beads_by_status(BeadStatus::Backlog).await.unwrap();
    assert_eq!(backlog.len(), 1);
    assert_eq!(backlog[0].title, "Bead 1");

    let slung = cache.list_beads_by_status(BeadStatus::Slung).await.unwrap();
    assert_eq!(slung.len(), 1);
}

#[tokio::test]
async fn test_cache_upsert_agent() {
    let cache = CacheDb::new_in_memory().await.unwrap();
    let agent = Agent::new("mayor", AgentRole::Mayor, CliType::Claude);
    cache.upsert_agent(&agent).await.unwrap();
    let retrieved = cache.get_agent_by_name("mayor").await.unwrap().unwrap();
    assert_eq!(retrieved.name, "mayor");
    assert_eq!(retrieved.role, AgentRole::Mayor);
}

#[tokio::test]
async fn test_cache_kpi_snapshot() {
    let cache = CacheDb::new_in_memory().await.unwrap();
    let kpi = cache.compute_kpi_snapshot().await.unwrap();
    assert_eq!(kpi.beads_active, 0);
    assert_eq!(kpi.agents_active, 0);
}
```

**Step 3: Run test to verify it fails**

Run: `cargo test -p at-core`
Expected: FAIL - cache module not found

**Step 4: Write implementation**

Create: `crates/at-core/src/cache.rs`

```rust
use crate::types::*;
use rusqlite::params;
use thiserror::Error;
use tokio_rusqlite::Connection;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] tokio_rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),
}

pub struct CacheDb {
    conn: Connection,
}

impl CacheDb {
    pub async fn new(path: &str) -> Result<Self, CacheError> {
        let conn = Connection::open(path).await?;
        let db = Self { conn };
        db.init_schema().await?;
        Ok(db)
    }

    pub async fn new_in_memory() -> Result<Self, CacheError> {
        let conn = Connection::open_in_memory().await?;
        let db = Self { conn };
        db.init_schema().await?;
        Ok(db)
    }

    async fn init_schema(&self) -> Result<(), CacheError> {
        self.conn
            .call(|conn| {
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS beads (
                        id TEXT PRIMARY KEY,
                        title TEXT NOT NULL,
                        description TEXT,
                        status TEXT NOT NULL,
                        lane TEXT NOT NULL,
                        priority INTEGER DEFAULT 0,
                        agent_id TEXT,
                        convoy_id TEXT,
                        created_at TEXT NOT NULL,
                        updated_at TEXT NOT NULL,
                        hooked_at TEXT,
                        slung_at TEXT,
                        done_at TEXT,
                        git_branch TEXT,
                        metadata TEXT
                    );
                    CREATE INDEX IF NOT EXISTS idx_beads_status ON beads(status);
                    CREATE INDEX IF NOT EXISTS idx_beads_lane ON beads(lane);
                    CREATE INDEX IF NOT EXISTS idx_beads_agent ON beads(agent_id);

                    CREATE TABLE IF NOT EXISTS agents (
                        id TEXT PRIMARY KEY,
                        name TEXT NOT NULL UNIQUE,
                        role TEXT NOT NULL,
                        cli_type TEXT NOT NULL,
                        model TEXT,
                        status TEXT NOT NULL,
                        rig TEXT,
                        pid INTEGER,
                        session_id TEXT,
                        created_at TEXT NOT NULL,
                        last_seen TEXT,
                        metadata TEXT
                    );
                    CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);
                    CREATE INDEX IF NOT EXISTS idx_agents_role ON agents(role);

                    CREATE TABLE IF NOT EXISTS events (
                        id TEXT PRIMARY KEY,
                        event_type TEXT NOT NULL,
                        actor TEXT,
                        bead_id TEXT,
                        agent_id TEXT,
                        rig TEXT,
                        lane TEXT,
                        payload TEXT,
                        created_at TEXT NOT NULL
                    );
                    CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
                    CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);",
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn upsert_bead(&self, bead: &Bead) -> Result<(), CacheError> {
        let bead = bead.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO beads (id, title, description, status, lane, priority,
                        agent_id, convoy_id, created_at, updated_at, hooked_at, slung_at, done_at,
                        git_branch, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    params![
                        bead.id.to_string(),
                        bead.title,
                        bead.description,
                        serde_json::to_string(&bead.status).unwrap().trim_matches('"'),
                        serde_json::to_string(&bead.lane).unwrap().trim_matches('"'),
                        bead.priority,
                        bead.agent_id.map(|u| u.to_string()),
                        bead.convoy_id.map(|u| u.to_string()),
                        bead.created_at.to_rfc3339(),
                        bead.updated_at.to_rfc3339(),
                        bead.hooked_at.map(|d| d.to_rfc3339()),
                        bead.slung_at.map(|d| d.to_rfc3339()),
                        bead.done_at.map(|d| d.to_rfc3339()),
                        bead.git_branch,
                        bead.metadata.map(|m| serde_json::to_string(&m).unwrap()),
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_bead(&self, id: Uuid) -> Result<Option<Bead>, CacheError> {
        let id_str = id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, title, description, status, lane, priority, agent_id, convoy_id,
                            created_at, updated_at, hooked_at, slung_at, done_at, git_branch, metadata
                     FROM beads WHERE id = ?1",
                )?;
                let result = stmt.query_row(params![id_str], |row| {
                    Ok(row_to_bead(row))
                });
                match result {
                    Ok(bead) => Ok(Some(bead)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e.into()),
                }
            })
            .await
            .map_err(CacheError::from)
    }

    pub async fn list_beads_by_status(&self, status: BeadStatus) -> Result<Vec<Bead>, CacheError> {
        let status_str = serde_json::to_string(&status)
            .unwrap()
            .trim_matches('"')
            .to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, title, description, status, lane, priority, agent_id, convoy_id,
                            created_at, updated_at, hooked_at, slung_at, done_at, git_branch, metadata
                     FROM beads WHERE status = ?1 ORDER BY priority DESC, created_at ASC",
                )?;
                let beads = stmt
                    .query_map(params![status_str], |row| Ok(row_to_bead(row)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(beads)
            })
            .await
            .map_err(CacheError::from)
    }

    pub async fn upsert_agent(&self, agent: &Agent) -> Result<(), CacheError> {
        let agent = agent.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO agents (id, name, role, cli_type, model, status,
                        rig, pid, session_id, created_at, last_seen, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        agent.id.to_string(),
                        agent.name,
                        serde_json::to_string(&agent.role).unwrap().trim_matches('"'),
                        serde_json::to_string(&agent.cli_type).unwrap().trim_matches('"'),
                        agent.model,
                        serde_json::to_string(&agent.status).unwrap().trim_matches('"'),
                        agent.rig,
                        agent.pid,
                        agent.session_id,
                        agent.created_at.to_rfc3339(),
                        agent.last_seen.map(|d| d.to_rfc3339()),
                        agent.metadata.map(|m| serde_json::to_string(&m).unwrap()),
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_agent_by_name(&self, name: &str) -> Result<Option<Agent>, CacheError> {
        let name = name.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, role, cli_type, model, status, rig, pid, session_id,
                            created_at, last_seen, metadata
                     FROM agents WHERE name = ?1",
                )?;
                let result = stmt.query_row(params![name], |row| Ok(row_to_agent(row)));
                match result {
                    Ok(agent) => Ok(Some(agent)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(e.into()),
                }
            })
            .await
            .map_err(CacheError::from)
    }

    pub async fn compute_kpi_snapshot(&self) -> Result<KpiSnapshot, CacheError> {
        self.conn
            .call(|conn| {
                let beads_active: u32 = conn.query_row(
                    "SELECT COUNT(*) FROM beads WHERE status IN ('hooked','slung','review')",
                    [],
                    |row| row.get(0),
                )?;
                let beads_completed: u32 = conn.query_row(
                    "SELECT COUNT(*) FROM beads WHERE status = 'done'",
                    [],
                    |row| row.get(0),
                )?;
                let beads_failed: u32 = conn.query_row(
                    "SELECT COUNT(*) FROM beads WHERE status IN ('failed','escalated')",
                    [],
                    |row| row.get(0),
                )?;
                let agents_active: u32 = conn.query_row(
                    "SELECT COUNT(*) FROM agents WHERE status = 'active'",
                    [],
                    |row| row.get(0),
                )?;

                Ok(KpiSnapshot {
                    id: uuid::Uuid::new_v4(),
                    beads_active,
                    beads_completed,
                    beads_failed,
                    agents_active,
                    convoys_active: 0,
                    total_cost_today: 0.0,
                    total_tokens_today: 0,
                    created_at: chrono::Utc::now(),
                })
            })
            .await
            .map_err(CacheError::from)
    }
}

fn row_to_bead(row: &rusqlite::Row) -> Bead {
    let id_str: String = row.get(0).unwrap();
    let status_str: String = row.get(3).unwrap();
    let lane_str: String = row.get(4).unwrap();

    Bead {
        id: Uuid::parse_str(&id_str).unwrap(),
        title: row.get(1).unwrap(),
        description: row.get(2).unwrap(),
        status: serde_json::from_str(&format!("\"{}\"", status_str)).unwrap(),
        lane: serde_json::from_str(&format!("\"{}\"", lane_str)).unwrap(),
        priority: row.get(5).unwrap(),
        agent_id: row
            .get::<_, Option<String>>(6)
            .unwrap()
            .map(|s| Uuid::parse_str(&s).unwrap()),
        convoy_id: row
            .get::<_, Option<String>>(7)
            .unwrap()
            .map(|s| Uuid::parse_str(&s).unwrap()),
        created_at: row
            .get::<_, String>(8)
            .unwrap()
            .parse()
            .unwrap(),
        updated_at: row
            .get::<_, String>(9)
            .unwrap()
            .parse()
            .unwrap(),
        hooked_at: row
            .get::<_, Option<String>>(10)
            .unwrap()
            .map(|s| s.parse().unwrap()),
        slung_at: row
            .get::<_, Option<String>>(11)
            .unwrap()
            .map(|s| s.parse().unwrap()),
        done_at: row
            .get::<_, Option<String>>(12)
            .unwrap()
            .map(|s| s.parse().unwrap()),
        git_branch: row.get(13).unwrap(),
        metadata: row
            .get::<_, Option<String>>(14)
            .unwrap()
            .map(|s| serde_json::from_str(&s).unwrap()),
    }
}

fn row_to_agent(row: &rusqlite::Row) -> Agent {
    let id_str: String = row.get(0).unwrap();
    let role_str: String = row.get(2).unwrap();
    let cli_str: String = row.get(3).unwrap();
    let status_str: String = row.get(5).unwrap();

    Agent {
        id: Uuid::parse_str(&id_str).unwrap(),
        name: row.get(1).unwrap(),
        role: serde_json::from_str(&format!("\"{}\"", role_str)).unwrap(),
        cli_type: serde_json::from_str(&format!("\"{}\"", cli_str)).unwrap(),
        model: row.get(4).unwrap(),
        status: serde_json::from_str(&format!("\"{}\"", status_str)).unwrap(),
        rig: row.get(6).unwrap(),
        pid: row.get(7).unwrap(),
        session_id: row.get(8).unwrap(),
        created_at: row.get::<_, String>(9).unwrap().parse().unwrap(),
        last_seen: row
            .get::<_, Option<String>>(10)
            .unwrap()
            .map(|s| s.parse().unwrap()),
        metadata: row
            .get::<_, Option<String>>(11)
            .unwrap()
            .map(|s| serde_json::from_str(&s).unwrap()),
    }
}
```

Update `crates/at-core/src/lib.rs`:

```rust
pub mod cache;
pub mod types;
```

**Step 5: Run test to verify it passes**

Run: `cargo test -p at-core`
Expected: PASS (all 10 tests)

**Step 6: Commit**

```bash
git add crates/at-core/
git commit -m "feat(core): add SQLite cache layer with bead/agent CRUD and KPI snapshots"
```

---

### Task 1.4: at-core Configuration

**Files:**
- Create: `crates/at-core/src/config.rs`
- Test: `crates/at-core/tests/config_test.rs`

**Step 1: Add dependencies**

Add to `crates/at-core/Cargo.toml`:

```toml
toml = "0.8"
dirs = "6"
```

**Step 2: Write failing test**

Create: `crates/at-core/tests/config_test.rs`

```rust
use at_core::config::Config;

#[test]
fn test_default_config() {
    let config = Config::default();
    assert_eq!(config.general.max_agents, 20);
    assert_eq!(config.general.max_ptys, 20);
    assert_eq!(config.general.default_lane, "standard");
}

#[test]
fn test_config_from_toml() {
    let toml_str = r#"
    [general]
    rig = "test_rig"
    max_agents = 10

    [agents.claude]
    binary = "claude"
    args = ["--dangerously-skip-permissions"]
    timeout_seconds = 300
    "#;

    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.general.rig, Some("test_rig".to_string()));
    assert_eq!(config.general.max_agents, 10);
    assert_eq!(config.agents.claude.binary, "claude");
}

#[test]
fn test_config_serialization_roundtrip() {
    let config = Config::default();
    let toml_str = toml::to_string_pretty(&config).unwrap();
    let deserialized: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(config.general.max_agents, deserialized.general.max_agents);
}
```

**Step 3: Run test to verify it fails**

Run: `cargo test -p at-core`
Expected: FAIL - config module not found

**Step 4: Write implementation**

Create: `crates/at-core/src/config.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub dolt: DoltConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub agents: AgentCliConfigs,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub bridge: BridgeConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            dolt: DoltConfig::default(),
            cache: CacheConfig::default(),
            providers: ProvidersConfig::default(),
            agents: AgentCliConfigs::default(),
            security: SecurityConfig::default(),
            daemon: DaemonConfig::default(),
            ui: UiConfig::default(),
            bridge: BridgeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub rig: Option<String>,
    #[serde(default = "default_lane")]
    pub default_lane: String,
    #[serde(default = "default_max_agents")]
    pub max_agents: u32,
    #[serde(default = "default_max_ptys")]
    pub max_ptys: u32,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            rig: None,
            default_lane: default_lane(),
            max_agents: default_max_agents(),
            max_ptys: default_max_ptys(),
        }
    }
}

fn default_lane() -> String { "standard".to_string() }
fn default_max_agents() -> u32 { 20 }
fn default_max_ptys() -> u32 { 20 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoltConfig {
    #[serde(default = "default_dolt_host")]
    pub host: String,
    #[serde(default = "default_dolt_port")]
    pub port: u16,
    #[serde(default = "default_dolt_database")]
    pub database: String,
    #[serde(default = "default_dolt_data_dir")]
    pub data_dir: String,
}

impl Default for DoltConfig {
    fn default() -> Self {
        Self {
            host: default_dolt_host(),
            port: default_dolt_port(),
            database: default_dolt_database(),
            data_dir: default_dolt_data_dir(),
        }
    }
}

fn default_dolt_host() -> String { "127.0.0.1".to_string() }
fn default_dolt_port() -> u16 { 3306 }
fn default_dolt_database() -> String { "auto_tundra".to_string() }
fn default_dolt_data_dir() -> String { "~/.auto-tundra/dolt".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_path")]
    pub sqlite_path: String,
    #[serde(default = "default_cache_ttl")]
    pub ttl_seconds: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            sqlite_path: default_cache_path(),
            ttl_seconds: default_cache_ttl(),
        }
    }
}

fn default_cache_path() -> String { "~/.auto-tundra/cache.db".to_string() }
fn default_cache_ttl() -> u64 { 30 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    #[serde(default = "default_provider")]
    pub default: String,
}

fn default_provider() -> String { "openrouter".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCliConfig {
    pub binary: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_timeout() -> u64 { 600 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCliConfigs {
    #[serde(default = "default_claude_config")]
    pub claude: AgentCliConfig,
    #[serde(default = "default_codex_config")]
    pub codex: AgentCliConfig,
    #[serde(default = "default_gemini_config")]
    pub gemini: AgentCliConfig,
    #[serde(default = "default_opencode_config")]
    pub opencode: AgentCliConfig,
}

impl Default for AgentCliConfigs {
    fn default() -> Self {
        Self {
            claude: default_claude_config(),
            codex: default_codex_config(),
            gemini: default_gemini_config(),
            opencode: default_opencode_config(),
        }
    }
}

fn default_claude_config() -> AgentCliConfig {
    AgentCliConfig {
        binary: "claude".to_string(),
        args: vec!["--dangerously-skip-permissions".to_string()],
        timeout_seconds: 600,
    }
}

fn default_codex_config() -> AgentCliConfig {
    AgentCliConfig {
        binary: "codex".to_string(),
        args: vec!["--approval-mode".to_string(), "full-auto".to_string()],
        timeout_seconds: 600,
    }
}

fn default_gemini_config() -> AgentCliConfig {
    AgentCliConfig {
        binary: "gemini".to_string(),
        args: vec![],
        timeout_seconds: 600,
    }
}

fn default_opencode_config() -> AgentCliConfig {
    AgentCliConfig {
        binary: "opencode".to_string(),
        args: vec![],
        timeout_seconds: 600,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub tool_firewall: bool,
    #[serde(default = "default_true")]
    pub input_sanitization: bool,
    #[serde(default = "default_max_tool_calls")]
    pub max_tool_calls_per_turn: u32,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            tool_firewall: true,
            input_sanitization: true,
            max_tool_calls_per_turn: default_max_tool_calls(),
        }
    }
}

fn default_true() -> bool { true }
fn default_max_tool_calls() -> u32 { 10 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_heartbeat")]
    pub heartbeat_interval_seconds: u64,
    #[serde(default = "default_patrol")]
    pub patrol_interval_seconds: u64,
    #[serde(default = "default_kpi")]
    pub kpi_snapshot_interval_seconds: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_seconds: default_heartbeat(),
            patrol_interval_seconds: default_patrol(),
            kpi_snapshot_interval_seconds: default_kpi(),
        }
    }
}

fn default_heartbeat() -> u64 { 30 }
fn default_patrol() -> u64 { 60 }
fn default_kpi() -> u64 { 300 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_refresh")]
    pub refresh_interval_ms: u64,
    #[serde(default = "default_scrollback")]
    pub terminal_scrollback_lines: u64,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            refresh_interval_ms: default_refresh(),
            terminal_scrollback_lines: default_scrollback(),
        }
    }
}

fn default_theme() -> String { "dark".to_string() }
fn default_refresh() -> u64 { 2000 }
fn default_scrollback() -> u64 { 5000 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    #[serde(default)]
    pub kafka_enabled: bool,
    #[serde(default = "default_kafka_brokers")]
    pub kafka_brokers: String,
    #[serde(default)]
    pub webhook_enabled: bool,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            kafka_enabled: false,
            kafka_brokers: default_kafka_brokers(),
            webhook_enabled: false,
        }
    }
}

fn default_kafka_brokers() -> String { "localhost:9092".to_string() }

impl Config {
    /// Load config from default path (~/.auto-tundra/config.toml)
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let home = dirs::home_dir().ok_or("no home directory")?;
        let path = home.join(".auto-tundra").join("config.toml");
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Config::default())
        }
    }

    /// Load config from specific path
    pub fn load_from(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}
```

Update `crates/at-core/src/lib.rs`:

```rust
pub mod cache;
pub mod config;
pub mod types;
```

**Step 5: Run test to verify it passes**

Run: `cargo test -p at-core`
Expected: PASS (all 13 tests)

**Step 6: Commit**

```bash
git add crates/at-core/
git commit -m "feat(core): add TOML configuration with sensible defaults"
```

---

### Task 1.5: at-cli Scaffold with Status Command

**Files:**
- Create: `crates/at-cli/Cargo.toml`
- Create: `crates/at-cli/src/main.rs`
- Create: `crates/at-cli/src/commands/mod.rs`
- Create: `crates/at-cli/src/commands/status.rs`

**Step 1: Create Cargo.toml**

Create: `crates/at-cli/Cargo.toml`

```toml
[package]
name = "at-cli"
version.workspace = true
edition.workspace = true

[[bin]]
name = "at"
path = "src/main.rs"

[dependencies]
at-core = { path = "../at-core" }
at-telemetry = { path = "../at-telemetry" }
tokio = { workspace = true }
clap = { version = "4", features = ["derive"] }
anyhow = { workspace = true }
tracing = { workspace = true }
```

**Step 2: Write main.rs with clap**

Create: `crates/at-cli/src/main.rs`

```rust
use clap::{Parser, Subcommand};
use anyhow::Result;

mod commands;

#[derive(Parser)]
#[command(name = "at", about = "auto-tundra: multi-agent terminal orchestrator")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show system status
    Status,
    /// Assign work to an agent
    Sling {
        /// Bead ID or title
        bead: String,
        /// Agent name
        agent: String,
        /// Priority lane
        #[arg(short, long, default_value = "standard")]
        lane: String,
    },
    /// Pin work to an agent
    Hook {
        /// Bead title
        title: String,
        /// Agent name
        agent: String,
    },
    /// Complete work
    Done {
        /// Bead ID
        bead: String,
        /// Mark as failed
        #[arg(long)]
        fail: bool,
    },
    /// Notify an agent
    Nudge {
        /// Agent name
        agent: String,
        /// Message
        #[arg(short, long)]
        message: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    at_telemetry::logging::init_logging("at-cli", "info");
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status) | None => commands::status::run().await,
        Some(Commands::Sling { bead, agent, lane }) => {
            tracing::info!(bead = %bead, agent = %agent, lane = %lane, "sling");
            println!("Sling: {} -> {} (lane: {})", bead, agent, lane);
            Ok(())
        }
        Some(Commands::Hook { title, agent }) => {
            tracing::info!(title = %title, agent = %agent, "hook");
            println!("Hook: {} -> {}", title, agent);
            Ok(())
        }
        Some(Commands::Done { bead, fail }) => {
            let status = if fail { "failed" } else { "done" };
            tracing::info!(bead = %bead, status = %status, "done");
            println!("Done: {} ({})", bead, status);
            Ok(())
        }
        Some(Commands::Nudge { agent, message }) => {
            tracing::info!(agent = %agent, "nudge");
            println!("Nudge: {} <- {}", agent, message);
            Ok(())
        }
    }
}
```

**Step 3: Write status command**

Create: `crates/at-cli/src/commands/mod.rs`

```rust
pub mod status;
```

Create: `crates/at-cli/src/commands/status.rs`

```rust
use anyhow::Result;
use at_core::cache::CacheDb;
use at_core::config::Config;

pub async fn run() -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let rig = config.general.rig.as_deref().unwrap_or("unknown");

    // Try to connect to cache, create if needed
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))?;
    let cache_dir = home.join(".auto-tundra");
    std::fs::create_dir_all(&cache_dir)?;
    let cache_path = cache_dir.join("cache.db");
    let cache = CacheDb::new(cache_path.to_str().unwrap()).await?;

    let kpi = cache.compute_kpi_snapshot().await?;

    println!("auto-tundra status");
    println!("==================");
    println!("Rig:             {}", rig);
    println!("Agents active:   {}", kpi.agents_active);
    println!("Beads active:    {}", kpi.beads_active);
    println!("Beads completed: {}", kpi.beads_completed);
    println!("Beads failed:    {}", kpi.beads_failed);
    println!("Convoys active:  {}", kpi.convoys_active);
    println!("Cost today:      ${:.2}", kpi.total_cost_today);
    println!("Tokens today:    {}", kpi.total_tokens_today);

    Ok(())
}
```

Add `dirs` dependency to at-cli Cargo.toml:

```toml
dirs = "6"
```

**Step 4: Build and test**

Run: `cargo build -p at-cli`
Expected: Compiles successfully

Run: `cargo run -p at-cli -- status`
Expected: Prints status with all zeros (empty cache)

Run: `cargo run -p at-cli -- --help`
Expected: Shows help with all subcommands

**Step 5: Commit**

```bash
git add crates/at-cli/
git commit -m "feat(cli): add at CLI with status, sling, hook, done, nudge commands"
```

---

## Phase 2: Agent Sessions (at-session + at-harness)

### Task 2.1: at-session PTY Pool

**Files:**
- Create: `crates/at-session/Cargo.toml`
- Create: `crates/at-session/src/lib.rs`
- Create: `crates/at-session/src/pty_pool.rs`
- Test: `crates/at-session/tests/pty_pool_test.rs`

**Summary:** Create PTY pool using `portable-pty`. Each agent gets a managed PTY. Pool enforces `max_ptys` limit. Async read/write via tokio channels.

**Key deps:** `portable-pty`, `tokio`, `flume`

---

### Task 2.2: CLI Adapters (claude, codex, gemini, opencode)

**Files:**
- Create: `crates/at-session/src/cli_adapters/mod.rs`
- Create: `crates/at-session/src/cli_adapters/claude.rs`
- Create: `crates/at-session/src/cli_adapters/codex.rs`
- Create: `crates/at-session/src/cli_adapters/gemini.rs`
- Create: `crates/at-session/src/cli_adapters/opencode.rs`
- Test: `crates/at-session/tests/cli_adapter_test.rs`

**Summary:** Define `AgentCli` trait with `spawn()`, `send_command()`, `read_output()`, `status()`, `terminate()`. Each adapter knows how to launch and interact with its CLI tool. Use `expectrl` for expect-style automation.

**Key deps:** `expectrl`, `portable-pty`, `async-trait`

---

### Task 2.3: Terminal Emulation (alacritty_terminal)

**Files:**
- Create: `crates/at-session/src/terminal.rs`
- Test: `crates/at-session/tests/terminal_test.rs`

**Summary:** Wrap `alacritty_terminal::Term<T>` for headless terminal emulation. Parse VT sequences from PTY output. Expose `renderable_content()` for optional UI rendering.

**Key deps:** `alacritty_terminal`, `vte`

---

### Task 2.4: at-harness Provider Integration

**Files:**
- Create: `crates/at-harness/Cargo.toml`
- Create: `crates/at-harness/src/lib.rs`
- Create: `crates/at-harness/src/providers/multi.rs`
- Create: `crates/at-harness/src/providers/router.rs`
- Create: `crates/at-harness/src/circuit_breaker.rs` (port from rust-harness)
- Create: `crates/at-harness/src/rate_limiter.rs` (port from rust-harness)
- Create: `crates/at-harness/src/security.rs` (port from rust-harness)
- Test: `crates/at-harness/tests/`

**Summary:** Port rust-harness circuit breaker, rate limiter, and security modules. Add `genai` as unified provider. Add smart router with failover.

**Key deps:** `genai`, `rig-core`, `openrouter_api`

---

## Phase 3: Agent Taxonomy (at-agents + at-daemon)

### Task 3.1: Agent State Machine
### Task 3.2: Mayor Agent
### Task 3.3: Deacon Agent (Patrol Loops)
### Task 3.4: Witness Agent
### Task 3.5: Polecat Agent (Git Worktrees)
### Task 3.6: Supervisor (rust_supervisor)
### Task 3.7: Daemon Main Loop
### Task 3.8: Heartbeat Monitoring
### Task 3.9: KPI Snapshot Generator

**Summary:** Implement gastown's agent taxonomy in Rust. Each agent type has a state machine, lifecycle hooks, and supervisor integration. Daemon runs patrol loops, heartbeat checks, and KPI snapshots.

---

## Phase 4: TUI Dashboard (ratatui)

### Task 4.1: TUI App Scaffold (ratatui + crossterm)
### Task 4.2: Tab Navigation (1-9 keys)
### Task 4.3: Dashboard Tab (KPI cards + agent list + activity feed)
### Task 4.4: Agents Tab (agent list + terminal embed via tui-term)
### Task 4.5: Beads Tab (kanban board)
### Task 4.6: Sessions Tab
### Task 4.7: Costs Tab
### Task 4.8: Analytics Tab (heatmaps)
### Task 4.9: Help Modal + Command Palette
### Task 4.10: Approval Workflow (y/n keys)

**Summary:** Build 9-tab TUI following ccboard/tmuxcc patterns. Status glyphs, vim keybindings, live terminal embedding, kanban board.

---

## Phase 5: Tauri Desktop App

### Task 5.1: Tauri 2.x Scaffold
### Task 5.2: Leptos WASM Frontend Setup
### Task 5.3: tauri-specta Type-Safe IPC
### Task 5.4: PTY Commands (Tauri -> at-session)
### Task 5.5: xterm.js Terminal Component
### Task 5.6: Zellij Sidecar Integration
### Task 5.7: Dashboard Page (Leptos)
### Task 5.8: Agents Page with Terminal Grid
### Task 5.9: Beads Kanban Page
### Task 5.10: Costs + Analytics Pages
### Task 5.11: MCP Bridge (tauri-plugin-mcp-bridge)

**Summary:** Tauri 2.x app with Leptos WASM frontend. xterm.js terminals connected to PTY pool via Tauri events. Zellij web sidecar for multiplexed sessions. All 9 tabs from TUI ported to web UI.

---

## Phase 6: Advanced Features

### Task 6.1: Convoy Management
### Task 6.2: Mail System
### Task 6.3: Kafka Bridge (at-bridge)
### Task 6.4: SSE Live Updates
### Task 6.5: Cross-Agent Context Bridge
### Task 6.6: OpenTelemetry OTLP Export
### Task 6.7: OpenLineage Integration
### Task 6.8: Vector Configuration
### Task 6.9: Dolt DB Integration (replace SQLite as primary)
### Task 6.10: Theme System (7 themes)

**Summary:** Complete the feature set with distributed coordination (Kafka), full observability (OTel + Vector + OpenLineage), and Dolt DB as the primary versioned data store.

---

## Verification Checklist

After each phase:
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace` has zero warnings
- [ ] `cargo fmt --check` passes
- [ ] Binary builds: `cargo build --release -p at-cli`
- [ ] All new code has tests
- [ ] Commits are atomic and well-messaged
