use std::path::Path;

use chrono::Utc;
use tokio_rusqlite::Connection;
use uuid::Uuid;

use crate::types::{Agent, Bead, BeadStatus, KpiSnapshot};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when reading from or writing to the cache database.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// An underlying SQLite / tokio-rusqlite operation failed.
    #[error("database error: {0}")]
    Db(#[from] tokio_rusqlite::Error),

    /// A database row contained a value that could not be decoded into the
    /// expected Rust type (corrupt schema, post-migration rename, etc.).
    ///
    /// The `context` field names the column and, where safe, echoes the bad
    /// value so callers can log a meaningful diagnostic without panicking.
    #[error("invalid row data — {context}: {source}")]
    InvalidRow {
        /// Human-readable description of what was being decoded and what went
        /// wrong (e.g., `"beads.id: not a valid UUID"`).
        context: String,
        /// The underlying parse / deserialise error.
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

impl CacheError {
    fn invalid_row(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        CacheError::InvalidRow {
            context: context.into(),
            source: Box::new(source),
        }
    }
}

/// Async SQLite-backed cache for beads, agents, and events.
pub struct CacheDb {
    conn: Connection,
}

// ---------------------------------------------------------------------------
// helpers – enum <-> SQLite string
// ---------------------------------------------------------------------------

fn enum_to_sql<T: serde::Serialize>(val: &T) -> String {
    let s = serde_json::to_string(val).expect("serialize enum");
    s.trim_matches('"').to_string()
}

/// Deserialise a plain SQL string (e.g. `"backlog"`) into a serde-friendly
/// Rust enum.  Returns `Err(CacheError::InvalidRow)` instead of panicking
/// when the string is not a recognised variant.
fn enum_from_sql<T: serde::de::DeserializeOwned>(raw: &str, column: &str) -> Result<T, CacheError> {
    let quoted = format!("\"{}\"", raw);
    serde_json::from_str(&quoted)
        .map_err(|e| CacheError::invalid_row(format!("{column}: unrecognised value {:?}", raw), e))
}

impl CacheDb {
    /// Open (or create) a database at the given file path.
    pub async fn new(path: impl AsRef<Path>) -> Result<Self, tokio_rusqlite::Error> {
        let conn = Connection::open(path.as_ref()).await?;
        let db = Self { conn };
        db.init_schema().await?;
        Ok(db)
    }

    /// Create a purely in-memory database (useful for tests).
    pub async fn new_in_memory() -> Result<Self, tokio_rusqlite::Error> {
        let conn = Connection::open_in_memory().await?;
        let db = Self { conn };
        db.init_schema().await?;
        Ok(db)
    }

    // -----------------------------------------------------------------------
    // Schema
    // -----------------------------------------------------------------------

    async fn init_schema(&self) -> Result<(), tokio_rusqlite::Error> {
        self.conn
            .call(|conn| {
                conn.execute_batch(
                    "
                    -- M-series unified memory optimizations
                    PRAGMA journal_mode=WAL;
                    PRAGMA synchronous=NORMAL;
                    PRAGMA cache_size=-64000;
                    PRAGMA mmap_size=268435456;
                    PRAGMA temp_store=MEMORY;
                    PRAGMA busy_timeout=5000;

                    CREATE TABLE IF NOT EXISTS beads (
                        id          TEXT PRIMARY KEY,
                        title       TEXT NOT NULL,
                        description TEXT,
                        status      TEXT NOT NULL,
                        lane        TEXT NOT NULL,
                        priority    INTEGER NOT NULL DEFAULT 0,
                        agent_id    TEXT,
                        convoy_id   TEXT,
                        created_at  TEXT NOT NULL,
                        updated_at  TEXT NOT NULL,
                        hooked_at   TEXT,
                        slung_at    TEXT,
                        done_at     TEXT,
                        git_branch  TEXT,
                        metadata    TEXT
                    );

                    CREATE INDEX IF NOT EXISTS idx_beads_status ON beads(status);
                    CREATE INDEX IF NOT EXISTS idx_beads_lane   ON beads(lane);

                    CREATE TABLE IF NOT EXISTS agents (
                        id          TEXT PRIMARY KEY,
                        name        TEXT NOT NULL UNIQUE,
                        role        TEXT NOT NULL,
                        cli_type    TEXT NOT NULL,
                        model       TEXT,
                        status      TEXT NOT NULL,
                        rig         TEXT,
                        pid         INTEGER,
                        session_id  TEXT,
                        created_at  TEXT NOT NULL,
                        last_seen   TEXT NOT NULL,
                        metadata    TEXT
                    );

                    CREATE INDEX IF NOT EXISTS idx_agents_name   ON agents(name);
                    CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);

                    CREATE TABLE IF NOT EXISTS events (
                        id        TEXT PRIMARY KEY,
                        kind      TEXT NOT NULL,
                        source    TEXT NOT NULL,
                        payload   TEXT NOT NULL,
                        timestamp TEXT NOT NULL
                    );

                    CREATE INDEX IF NOT EXISTS idx_events_kind ON events(kind);
                    ",
                )?;
                Ok(())
            })
            .await
    }

    // -----------------------------------------------------------------------
    // Bead CRUD
    // -----------------------------------------------------------------------

    pub async fn upsert_bead(&self, bead: &Bead) -> Result<(), tokio_rusqlite::Error> {
        let id = bead.id.to_string();
        let title = bead.title.clone();
        let description = bead.description.clone();
        let status = enum_to_sql(&bead.status);
        let lane = enum_to_sql(&bead.lane);
        let priority = bead.priority;
        let agent_id = bead.agent_id.map(|u| u.to_string());
        let convoy_id = bead.convoy_id.map(|u| u.to_string());
        let created_at = bead.created_at.to_rfc3339();
        let updated_at = bead.updated_at.to_rfc3339();
        let hooked_at = bead.hooked_at.map(|d| d.to_rfc3339());
        let slung_at = bead.slung_at.map(|d| d.to_rfc3339());
        let done_at = bead.done_at.map(|d| d.to_rfc3339());
        let git_branch = bead.git_branch.clone();
        let metadata = bead.metadata.as_ref().map(|v| v.to_string());

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO beads (id, title, description, status, lane, priority,
                        agent_id, convoy_id, created_at, updated_at, hooked_at, slung_at,
                        done_at, git_branch, metadata)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
                     ON CONFLICT(id) DO UPDATE SET
                        title=excluded.title, description=excluded.description,
                        status=excluded.status, lane=excluded.lane, priority=excluded.priority,
                        agent_id=excluded.agent_id, convoy_id=excluded.convoy_id,
                        updated_at=excluded.updated_at, hooked_at=excluded.hooked_at,
                        slung_at=excluded.slung_at, done_at=excluded.done_at,
                        git_branch=excluded.git_branch, metadata=excluded.metadata",
                    rusqlite::params![
                        id,
                        title,
                        description,
                        status,
                        lane,
                        priority,
                        agent_id,
                        convoy_id,
                        created_at,
                        updated_at,
                        hooked_at,
                        slung_at,
                        done_at,
                        git_branch,
                        metadata,
                    ],
                )?;
                Ok(())
            })
            .await
    }

    /// Fetch a single bead by id.
    ///
    /// Returns `Err(CacheError::InvalidRow)` if the stored row contains data
    /// that cannot be decoded (corrupt UUID, unknown status string, bad date,
    /// etc.) rather than panicking.
    pub async fn get_bead(&self, id: Uuid) -> Result<Option<Bead>, CacheError> {
        let id_str = id.to_string();
        // conn.call() closures must return Result<T, tokio_rusqlite::Error>.
        // We smuggle a CacheError out as Ok(Err(e)) and flatten it after await.
        let outer: Result<Option<Result<Bead, CacheError>>, tokio_rusqlite::Error> = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, title, description, status, lane, priority,
                            agent_id, convoy_id, created_at, updated_at,
                            hooked_at, slung_at, done_at, git_branch, metadata
                     FROM beads WHERE id = ?1",
                )?;
                let mut rows = stmt.query(rusqlite::params![id_str])?;
                match rows.next()? {
                    Some(row) => Ok(Some(row_to_bead(row))),
                    None => Ok(None),
                }
            })
            .await;

        match outer {
            Ok(Some(Ok(bead))) => Ok(Some(bead)),
            Ok(Some(Err(e))) => Err(e),
            Ok(None) => Ok(None),
            Err(e) => Err(CacheError::Db(e)),
        }
    }

    /// List all beads with a given status.
    ///
    /// Returns `Err(CacheError::InvalidRow)` if any stored row cannot be
    /// decoded, rather than panicking.
    pub async fn list_beads_by_status(&self, status: BeadStatus) -> Result<Vec<Bead>, CacheError> {
        let status_str = enum_to_sql(&status);
        let outer: Result<Result<Vec<Bead>, CacheError>, tokio_rusqlite::Error> = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, title, description, status, lane, priority,
                            agent_id, convoy_id, created_at, updated_at,
                            hooked_at, slung_at, done_at, git_branch, metadata
                     FROM beads WHERE status = ?1 ORDER BY priority DESC",
                )?;
                let mut rows = stmt.query(rusqlite::params![status_str])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    match row_to_bead(row) {
                        Ok(bead) => out.push(bead),
                        Err(e) => return Ok(Err(e)),
                    }
                }
                Ok(Ok(out))
            })
            .await;

        match outer {
            Ok(Ok(beads)) => Ok(beads),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(CacheError::Db(e)),
        }
    }

    // -----------------------------------------------------------------------
    // Agent CRUD
    // -----------------------------------------------------------------------

    pub async fn upsert_agent(&self, agent: &Agent) -> Result<(), tokio_rusqlite::Error> {
        let id = agent.id.to_string();
        let name = agent.name.clone();
        let role = enum_to_sql(&agent.role);
        let cli_type = enum_to_sql(&agent.cli_type);
        let model = agent.model.clone();
        let status = enum_to_sql(&agent.status);
        let rig = agent.rig.clone();
        let pid = agent.pid.map(|p| p as i64);
        let session_id = agent.session_id.clone();
        let created_at = agent.created_at.to_rfc3339();
        let last_seen = agent.last_seen.to_rfc3339();
        let metadata = agent.metadata.as_ref().map(|v| v.to_string());

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agents (id, name, role, cli_type, model, status,
                        rig, pid, session_id, created_at, last_seen, metadata)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
                     ON CONFLICT(id) DO UPDATE SET
                        name=excluded.name, role=excluded.role, cli_type=excluded.cli_type,
                        model=excluded.model, status=excluded.status, rig=excluded.rig,
                        pid=excluded.pid, session_id=excluded.session_id,
                        last_seen=excluded.last_seen, metadata=excluded.metadata",
                    rusqlite::params![
                        id, name, role, cli_type, model, status, rig, pid, session_id, created_at,
                        last_seen, metadata,
                    ],
                )?;
                Ok(())
            })
            .await
    }

    /// Fetch an agent by name.
    ///
    /// Returns `Err(CacheError::InvalidRow)` if the stored row contains data
    /// that cannot be decoded, rather than panicking.
    pub async fn get_agent_by_name(&self, name: &str) -> Result<Option<Agent>, CacheError> {
        let name = name.to_string();
        let outer: Result<Option<Result<Agent, CacheError>>, tokio_rusqlite::Error> = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, role, cli_type, model, status,
                            rig, pid, session_id, created_at, last_seen, metadata
                     FROM agents WHERE name = ?1",
                )?;
                let mut rows = stmt.query(rusqlite::params![name])?;
                match rows.next()? {
                    Some(row) => Ok(Some(row_to_agent(row))),
                    None => Ok(None),
                }
            })
            .await;

        match outer {
            Ok(Some(Ok(agent))) => Ok(Some(agent)),
            Ok(Some(Err(e))) => Err(e),
            Ok(None) => Ok(None),
            Err(e) => Err(CacheError::Db(e)),
        }
    }

    // -----------------------------------------------------------------------
    // KPI
    // -----------------------------------------------------------------------

    /// Insert a raw row into the `beads` table using arbitrary string values,
    /// bypassing type validation.
    ///
    /// **Only available when the `test-utils` feature (or the crate's own
    /// `#[cfg(test)]`) is active.**  Intended for injecting deliberately-corrupt
    /// rows (bad UUIDs, unknown enum variants, invalid dates) so that callers
    /// in other crates can exercise the `CacheError::InvalidRow` code paths
    /// without needing access to the private `conn` field.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_raw_bead_for_test(
        &self,
        id: &str,
        status: &str,
        lane: &str,
    ) -> Result<(), tokio_rusqlite::Error> {
        let id = id.to_string();
        let status = status.to_string();
        let lane = lane.to_string();
        const GOOD_DATE: &str = "2024-01-01T00:00:00+00:00";
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO beads \
                     (id, title, status, lane, priority, created_at, updated_at) \
                     VALUES (?1, 'raw-test', ?2, ?3, 0, ?4, ?5)",
                    rusqlite::params![id, status, lane, GOOD_DATE, GOOD_DATE],
                )?;
                Ok(())
            })
            .await
    }

    /// Insert a raw row into the `agents` table using arbitrary string values,
    /// bypassing type validation.
    ///
    /// **Only available when the `test-utils` feature (or the crate's own
    /// `#[cfg(test)]`) is active.**  Intended for injecting deliberately-corrupt
    /// rows so that callers in other crates can exercise the
    /// `CacheError::InvalidRow` code paths.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_raw_agent_for_test(
        &self,
        name: &str,
        role: &str,
        cli_type: &str,
    ) -> Result<(), tokio_rusqlite::Error> {
        let name = name.to_string();
        let role = role.to_string();
        let cli_type = cli_type.to_string();
        const GOOD_DATE: &str = "2024-01-01T00:00:00+00:00";
        let id = uuid::Uuid::new_v4().to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agents \
                     (id, name, role, cli_type, model, status, rig, pid, session_id, \
                      created_at, last_seen, metadata) \
                     VALUES (?1, ?2, ?3, ?4, 'test-model', 'active', NULL, NULL, NULL, \
                             ?5, ?6, NULL)",
                    rusqlite::params![id, name, role, cli_type, GOOD_DATE, GOOD_DATE],
                )?;
                Ok(())
            })
            .await
    }

    /// Compute a point-in-time KPI snapshot.
    ///
    /// **Unknown-status policy (lenient + visible):** rows whose `status`
    /// column does not map to a known `BeadStatus` variant are counted in
    /// `total_beads` but not attributed to any named bucket, so the total
    /// always equals the real row count.  A `tracing::warn!` is emitted for
    /// each unknown value so operators can detect schema drift without the
    /// daemon crashing or silently understating totals.
    pub async fn compute_kpi_snapshot(&self) -> Result<KpiSnapshot, tokio_rusqlite::Error> {
        self.conn
            .call(|conn| {
                // Single GROUP BY query replaces 9 separate COUNT queries.
                let mut counts = std::collections::HashMap::<String, u64>::new();
                let mut stmt =
                    conn.prepare_cached("SELECT status, COUNT(*) FROM beads GROUP BY status")?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
                })?;
                for row in rows {
                    let (status, count) = row?;
                    // Warn on unknown status variants so operators notice schema
                    // drift; still include them in total_beads so the total
                    // always equals the real row count (lenient + visible policy).
                    let quoted = format!("\"{}\"", status);
                    if serde_json::from_str::<BeadStatus>(&quoted).is_err() {
                        tracing::warn!(
                            status = %status,
                            count = count,
                            "compute_kpi_snapshot: unknown bead status in database; \
                             counted in total_beads but not in any named bucket"
                        );
                    }
                    counts.insert(status, count);
                }

                let get = |key: &str| -> u64 { counts.get(key).copied().unwrap_or(0) };
                let total_beads: u64 = counts.values().sum();

                let active_agents: u64 = conn
                    .prepare_cached("SELECT COUNT(*) FROM agents WHERE status = 'active'")?
                    .query_row([], |r| r.get(0))?;

                Ok(KpiSnapshot {
                    total_beads,
                    backlog: get("backlog"),
                    hooked: get("hooked"),
                    slung: get("slung"),
                    review: get("review"),
                    done: get("done"),
                    failed: get("failed"),
                    escalated: get("escalated"),
                    active_agents,
                    timestamp: Utc::now(),
                })
            })
            .await
    }
}

// ---------------------------------------------------------------------------
// Row mapping helpers
// ---------------------------------------------------------------------------

/// Parse an RFC-3339 timestamp string, returning `CacheError::InvalidRow` on
/// failure.  The `column` argument is embedded in the error message.
fn parse_datetime(s: &str, column: &str) -> Result<chrono::DateTime<Utc>, CacheError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| CacheError::invalid_row(format!("{column}: invalid RFC-3339 date {:?}", s), e))
}

/// Parse a UUID string, returning `CacheError::InvalidRow` on failure.
fn parse_uuid(s: &str, column: &str) -> Result<Uuid, CacheError> {
    Uuid::parse_str(s)
        .map_err(|e| CacheError::invalid_row(format!("{column}: invalid UUID {:?}", s), e))
}

/// Parse a JSON value string, returning `CacheError::InvalidRow` on failure.
fn parse_json(s: &str, column: &str) -> Result<serde_json::Value, CacheError> {
    serde_json::from_str(s)
        .map_err(|e| CacheError::invalid_row(format!("{column}: invalid JSON"), e))
}

fn row_to_bead(row: &rusqlite::Row<'_>) -> Result<Bead, CacheError> {
    let id_str: String = row.get(0).map_err(|e| CacheError::Db(e.into()))?;
    let status_str: String = row.get(3).map_err(|e| CacheError::Db(e.into()))?;
    let lane_str: String = row.get(4).map_err(|e| CacheError::Db(e.into()))?;
    let agent_id_str: Option<String> = row.get(6).map_err(|e| CacheError::Db(e.into()))?;
    let convoy_id_str: Option<String> = row.get(7).map_err(|e| CacheError::Db(e.into()))?;
    let created_at_str: String = row.get(8).map_err(|e| CacheError::Db(e.into()))?;
    let updated_at_str: String = row.get(9).map_err(|e| CacheError::Db(e.into()))?;
    let hooked_at_str: Option<String> = row.get(10).map_err(|e| CacheError::Db(e.into()))?;
    let slung_at_str: Option<String> = row.get(11).map_err(|e| CacheError::Db(e.into()))?;
    let done_at_str: Option<String> = row.get(12).map_err(|e| CacheError::Db(e.into()))?;
    let metadata_str: Option<String> = row.get(14).map_err(|e| CacheError::Db(e.into()))?;

    Ok(Bead {
        id: parse_uuid(&id_str, "beads.id")?,
        title: row.get(1).map_err(|e| CacheError::Db(e.into()))?,
        description: row.get(2).map_err(|e| CacheError::Db(e.into()))?,
        status: enum_from_sql(&status_str, "beads.status")?,
        lane: enum_from_sql(&lane_str, "beads.lane")?,
        priority: row.get(5).map_err(|e| CacheError::Db(e.into()))?,
        agent_id: agent_id_str
            .map(|s| parse_uuid(&s, "beads.agent_id"))
            .transpose()?,
        convoy_id: convoy_id_str
            .map(|s| parse_uuid(&s, "beads.convoy_id"))
            .transpose()?,
        created_at: parse_datetime(&created_at_str, "beads.created_at")?,
        updated_at: parse_datetime(&updated_at_str, "beads.updated_at")?,
        hooked_at: hooked_at_str
            .map(|s| parse_datetime(&s, "beads.hooked_at"))
            .transpose()?,
        slung_at: slung_at_str
            .map(|s| parse_datetime(&s, "beads.slung_at"))
            .transpose()?,
        done_at: done_at_str
            .map(|s| parse_datetime(&s, "beads.done_at"))
            .transpose()?,
        git_branch: row.get(13).map_err(|e| CacheError::Db(e.into()))?,
        metadata: metadata_str
            .map(|s| parse_json(&s, "beads.metadata"))
            .transpose()?,
    })
}

fn row_to_agent(row: &rusqlite::Row<'_>) -> Result<Agent, CacheError> {
    let id_str: String = row.get(0).map_err(|e| CacheError::Db(e.into()))?;
    let role_str: String = row.get(2).map_err(|e| CacheError::Db(e.into()))?;
    let cli_type_str: String = row.get(3).map_err(|e| CacheError::Db(e.into()))?;
    let status_str: String = row.get(5).map_err(|e| CacheError::Db(e.into()))?;
    let pid_val: Option<i64> = row.get(7).map_err(|e| CacheError::Db(e.into()))?;
    let created_at_str: String = row.get(9).map_err(|e| CacheError::Db(e.into()))?;
    let last_seen_str: String = row.get(10).map_err(|e| CacheError::Db(e.into()))?;
    let metadata_str: Option<String> = row.get(11).map_err(|e| CacheError::Db(e.into()))?;

    Ok(Agent {
        id: parse_uuid(&id_str, "agents.id")?,
        name: row.get(1).map_err(|e| CacheError::Db(e.into()))?,
        role: enum_from_sql(&role_str, "agents.role")?,
        cli_type: enum_from_sql(&cli_type_str, "agents.cli_type")?,
        model: row.get(4).map_err(|e| CacheError::Db(e.into()))?,
        status: enum_from_sql(&status_str, "agents.status")?,
        rig: row.get(6).map_err(|e| CacheError::Db(e.into()))?,
        pid: pid_val.map(|p| p as u32),
        session_id: row.get(8).map_err(|e| CacheError::Db(e.into()))?,
        created_at: parse_datetime(&created_at_str, "agents.created_at")?,
        last_seen: parse_datetime(&last_seen_str, "agents.last_seen")?,
        metadata: metadata_str
            .map(|s| parse_json(&s, "agents.metadata"))
            .transpose()?,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: raw INSERT into beads with arbitrary strings, bypassing type
    // validation so we can inject corrupt data.
    async fn insert_raw_bead(
        db: &CacheDb,
        id: &str,
        status: &str,
        lane: &str,
        created_at: &str,
        updated_at: &str,
    ) {
        let id = id.to_string();
        let status = status.to_string();
        let lane = lane.to_string();
        let created_at = created_at.to_string();
        let updated_at = updated_at.to_string();
        db.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO beads \
                     (id, title, status, lane, priority, created_at, updated_at) \
                     VALUES (?1, 'test', ?2, ?3, 0, ?4, ?5)",
                    rusqlite::params![id, status, lane, created_at, updated_at],
                )?;
                Ok(())
            })
            .await
            .unwrap();
    }

    const GOOD_UUID: &str = "550e8400-e29b-41d4-a716-446655440000";
    const GOOD_DATE: &str = "2024-01-01T00:00:00Z";
    const GOOD_STATUS: &str = "backlog";
    const GOOD_LANE: &str = "standard";

    /// Test 1: a bead with an unknown status string must return
    /// `Err(CacheError::InvalidRow)` — not panic.
    #[tokio::test]
    async fn cache_returns_error_on_invalid_status_enum() {
        let db = CacheDb::new_in_memory().await.unwrap();
        insert_raw_bead(
            &db,
            GOOD_UUID,
            "totally_unknown_status",
            GOOD_LANE,
            GOOD_DATE,
            GOOD_DATE,
        )
        .await;

        let result = db.get_bead(Uuid::parse_str(GOOD_UUID).unwrap()).await;
        assert!(
            matches!(result, Err(CacheError::InvalidRow { .. })),
            "expected Err(CacheError::InvalidRow), got: {:?}",
            result
        );
    }

    /// Test 2: a bead whose `id` column is not a valid UUID must return
    /// `Err(CacheError::InvalidRow)` — not panic.
    ///
    /// We use `list_beads_by_status` (a full-scan query) because `get_bead`
    /// takes a `Uuid` argument and constructs the lookup key itself, so it
    /// would never find the deliberately-corrupt row.
    #[tokio::test]
    async fn cache_returns_error_on_invalid_uuid() {
        let db = CacheDb::new_in_memory().await.unwrap();

        // Insert via raw SQL so we can bypass the UUID type.
        db.conn
            .call(|conn| {
                conn.execute(
                    "INSERT INTO beads \
                     (id, title, status, lane, priority, created_at, updated_at) \
                     VALUES ('not-a-uuid', 'test', 'backlog', 'standard', 0, \
                             '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
                    [],
                )?;
                Ok(())
            })
            .await
            .unwrap();

        // list_beads_by_status scans all rows with status='backlog', hitting
        // the bad id.
        let result = db.list_beads_by_status(BeadStatus::Backlog).await;
        assert!(
            matches!(result, Err(CacheError::InvalidRow { .. })),
            "expected Err(CacheError::InvalidRow), got: {:?}",
            result
        );
    }

    /// Test 3: a bead whose `created_at` column is not a valid RFC-3339 date
    /// must return `Err(CacheError::InvalidRow)` — not panic.
    #[tokio::test]
    async fn cache_returns_error_on_invalid_date() {
        let db = CacheDb::new_in_memory().await.unwrap();
        insert_raw_bead(
            &db,
            GOOD_UUID,
            GOOD_STATUS,
            GOOD_LANE,
            "not-a-date", // bad created_at
            GOOD_DATE,
        )
        .await;

        let result = db.get_bead(Uuid::parse_str(GOOD_UUID).unwrap()).await;
        assert!(
            matches!(result, Err(CacheError::InvalidRow { .. })),
            "expected Err(CacheError::InvalidRow), got: {:?}",
            result
        );
    }

    /// Test 4: `compute_kpi_snapshot` with a mix of valid and unknown statuses
    /// must account for every row in `total_beads` (lenient + visible policy).
    #[tokio::test]
    async fn compute_kpi_snapshot_handles_unknown_status_per_chosen_policy() {
        let db = CacheDb::new_in_memory().await.unwrap();

        // 2 known-status beads.
        insert_raw_bead(
            &db,
            "550e8400-e29b-41d4-a716-446655440001",
            "backlog",
            GOOD_LANE,
            GOOD_DATE,
            GOOD_DATE,
        )
        .await;
        insert_raw_bead(
            &db,
            "550e8400-e29b-41d4-a716-446655440002",
            "done",
            GOOD_LANE,
            GOOD_DATE,
            GOOD_DATE,
        )
        .await;

        // 1 bead with an unknown/migrated status string.
        insert_raw_bead(
            &db,
            "550e8400-e29b-41d4-a716-446655440003",
            "totally_unknown_status",
            GOOD_LANE,
            GOOD_DATE,
            GOOD_DATE,
        )
        .await;

        let snapshot = db
            .compute_kpi_snapshot()
            .await
            .expect("compute_kpi_snapshot must not return an error (lenient policy)");

        // Lenient policy: total_beads must equal the real row count (3), even
        // though one status is unrecognised.
        assert_eq!(
            snapshot.total_beads, 3,
            "total_beads should equal real row count including unknown-status rows"
        );

        // Named buckets for known statuses are correct.
        assert_eq!(snapshot.backlog, 1);
        assert_eq!(snapshot.done, 1);
    }
}
