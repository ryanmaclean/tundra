use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::types::Type;
use tokio_rusqlite::Connection;
use uuid::Uuid;

use crate::types::{Agent, Bead, BeadStatus, KpiSnapshot};

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

/// Map any parse error on column `idx` into a rusqlite conversion error so a
/// malformed row surfaces as `Err` instead of panicking inside the
/// tokio-rusqlite worker thread (which would permanently close the connection).
fn conversion_err<E>(idx: usize, e: E) -> rusqlite::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(e))
}

fn enum_from_sql<T: serde::de::DeserializeOwned>(idx: usize, raw: &str) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(raw.to_string()))
        .map_err(|e| conversion_err(idx, e))
}

fn parse_uuid(idx: usize, s: &str) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(s).map_err(|e| conversion_err(idx, e))
}

fn parse_ts(idx: usize, s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| conversion_err(idx, e))
}

fn parse_json(idx: usize, s: &str) -> rusqlite::Result<serde_json::Value> {
    serde_json::from_str(s).map_err(|e| conversion_err(idx, e))
}

/// Collect rows through `map`, skipping (and logging) rows that fail to
/// decode so one bad row cannot hide every other row from list callers.
/// Errors from the SQLite cursor itself still propagate.
fn collect_rows<T>(
    mut rows: rusqlite::Rows<'_>,
    table: &str,
    map: fn(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> rusqlite::Result<Vec<T>> {
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        match map(row) {
            Ok(v) => out.push(v),
            Err(e @ rusqlite::Error::FromSqlConversionFailure(..)) => {
                let id: Option<String> = row.get(0).ok();
                tracing::warn!(table, ?id, error = %e, "skipping undecodable cache row");
            }
            Err(e) => return Err(e),
        }
    }
    Ok(out)
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

    pub async fn get_bead(&self, id: Uuid) -> Result<Option<Bead>, tokio_rusqlite::Error> {
        let id_str = id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, title, description, status, lane, priority,
                            agent_id, convoy_id, created_at, updated_at,
                            hooked_at, slung_at, done_at, git_branch, metadata
                     FROM beads WHERE id = ?1",
                )?;
                let mut rows = stmt.query(rusqlite::params![id_str])?;
                match rows.next()? {
                    Some(row) => Ok(Some(row_to_bead(row)?)),
                    None => Ok(None),
                }
            })
            .await
    }

    pub async fn list_beads_by_status(
        &self,
        status: BeadStatus,
    ) -> Result<Vec<Bead>, tokio_rusqlite::Error> {
        let status_str = enum_to_sql(&status);
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, title, description, status, lane, priority,
                            agent_id, convoy_id, created_at, updated_at,
                            hooked_at, slung_at, done_at, git_branch, metadata
                     FROM beads WHERE status = ?1 ORDER BY priority DESC",
                )?;
                let rows = stmt.query(rusqlite::params![status_str])?;
                Ok(collect_rows(rows, "beads", row_to_bead)?)
            })
            .await
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

    pub async fn get_agent_by_name(
        &self,
        name: &str,
    ) -> Result<Option<Agent>, tokio_rusqlite::Error> {
        let name = name.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, role, cli_type, model, status,
                            rig, pid, session_id, created_at, last_seen, metadata
                     FROM agents WHERE name = ?1",
                )?;
                let mut rows = stmt.query(rusqlite::params![name])?;
                match rows.next()? {
                    Some(row) => Ok(Some(row_to_agent(row)?)),
                    None => Ok(None),
                }
            })
            .await
    }

    // -----------------------------------------------------------------------
    // KPI
    // -----------------------------------------------------------------------

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

fn row_to_bead(row: &rusqlite::Row<'_>) -> rusqlite::Result<Bead> {
    let id_str: String = row.get(0)?;
    let status_str: String = row.get(3)?;
    let lane_str: String = row.get(4)?;
    let agent_id_str: Option<String> = row.get(6)?;
    let convoy_id_str: Option<String> = row.get(7)?;
    let created_at_str: String = row.get(8)?;
    let updated_at_str: String = row.get(9)?;
    let hooked_at_str: Option<String> = row.get(10)?;
    let slung_at_str: Option<String> = row.get(11)?;
    let done_at_str: Option<String> = row.get(12)?;
    let metadata_str: Option<String> = row.get(14)?;

    Ok(Bead {
        id: parse_uuid(0, &id_str)?,
        title: row.get(1)?,
        description: row.get(2)?,
        status: enum_from_sql(3, &status_str)?,
        lane: enum_from_sql(4, &lane_str)?,
        priority: row.get(5)?,
        agent_id: agent_id_str.map(|s| parse_uuid(6, &s)).transpose()?,
        convoy_id: convoy_id_str.map(|s| parse_uuid(7, &s)).transpose()?,
        created_at: parse_ts(8, &created_at_str)?,
        updated_at: parse_ts(9, &updated_at_str)?,
        hooked_at: hooked_at_str.map(|s| parse_ts(10, &s)).transpose()?,
        slung_at: slung_at_str.map(|s| parse_ts(11, &s)).transpose()?,
        done_at: done_at_str.map(|s| parse_ts(12, &s)).transpose()?,
        git_branch: row.get(13)?,
        metadata: metadata_str.map(|s| parse_json(14, &s)).transpose()?,
    })
}

fn row_to_agent(row: &rusqlite::Row<'_>) -> rusqlite::Result<Agent> {
    let id_str: String = row.get(0)?;
    let role_str: String = row.get(2)?;
    let cli_type_str: String = row.get(3)?;
    let status_str: String = row.get(5)?;
    let pid_val: Option<i64> = row.get(7)?;
    let created_at_str: String = row.get(9)?;
    let last_seen_str: String = row.get(10)?;
    let metadata_str: Option<String> = row.get(11)?;

    Ok(Agent {
        id: parse_uuid(0, &id_str)?,
        name: row.get(1)?,
        role: enum_from_sql(2, &role_str)?,
        cli_type: enum_from_sql(3, &cli_type_str)?,
        model: row.get(4)?,
        status: enum_from_sql(5, &status_str)?,
        rig: row.get(6)?,
        pid: pid_val.map(|p| p as u32),
        session_id: row.get(8)?,
        created_at: parse_ts(9, &created_at_str)?,
        last_seen: parse_ts(10, &last_seen_str)?,
        metadata: metadata_str.map(|s| parse_json(11, &s)).transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Lane;

    async fn insert_raw_bead(db: &CacheDb, id: &str, status: &str, lane: &str, created: &str) {
        let (id, status, lane, created) = (
            id.to_string(),
            status.to_string(),
            lane.to_string(),
            created.to_string(),
        );
        db.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO beads (id, title, status, lane, priority, created_at, updated_at)
                     VALUES (?1, 'raw', ?2, ?3, 0, ?4, ?4)",
                    rusqlite::params![id, status, lane, created],
                )?;
                Ok(())
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn malformed_bead_row_errors_without_killing_connection() {
        let db = CacheDb::new_in_memory().await.unwrap();
        let now = Utc::now().to_rfc3339();
        let bad_enum = Uuid::new_v4();
        insert_raw_bead(&db, &bad_enum.to_string(), "from_the_future", "standard", &now).await;
        let bad_date = Uuid::new_v4();
        insert_raw_bead(&db, &bad_date.to_string(), "backlog", "standard", "yesterday").await;
        insert_raw_bead(&db, "not-a-uuid", "backlog", "standard", &now).await;

        let good = Bead::new("good", Lane::Standard);
        db.upsert_bead(&good).await.unwrap();

        // Single-row lookups surface a conversion error instead of panicking.
        assert!(db.get_bead(bad_enum).await.is_err());
        assert!(db.get_bead(bad_date).await.is_err());

        // List queries skip undecodable rows and still return the good ones.
        let backlog = db.list_beads_by_status(BeadStatus::Backlog).await.unwrap();
        assert_eq!(backlog.len(), 1);
        assert_eq!(backlog[0].id, good.id);

        // The connection is still usable after the failures.
        assert!(db.get_bead(good.id).await.unwrap().is_some());
        db.compute_kpi_snapshot().await.unwrap();
    }

    #[tokio::test]
    async fn malformed_agent_row_errors_without_killing_connection() {
        let db = CacheDb::new_in_memory().await.unwrap();
        let now = Utc::now().to_rfc3339();
        db.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO agents (id, name, role, cli_type, status, created_at, last_seen, metadata)
                     VALUES (?1, 'ghost', 'retired_role', 'claude', 'active', ?2, ?2, '{bad json')",
                    rusqlite::params![Uuid::new_v4().to_string(), now],
                )?;
                Ok(())
            })
            .await
            .unwrap();

        assert!(db.get_agent_by_name("ghost").await.is_err());
        assert!(db.get_agent_by_name("nobody").await.unwrap().is_none());
    }
}
