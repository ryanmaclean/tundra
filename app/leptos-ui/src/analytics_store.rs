//! Fetches data from the daemon API and loads it into DuckDB WASM tables
//! for client-side analytical queries.

use serde::{Deserialize, Serialize};

use crate::api;
use crate::duckdb::DuckDbClient;

// ── Row types for DuckDB query results ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskRow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub completed_at: String,
    #[serde(default)]
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KpiRow {
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub metric: String,
    #[serde(default)]
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostRow {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub cost_usd: f64,
    #[serde(default)]
    pub timestamp: String,
}

// ── Aggregation result types ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhaseCount {
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AvgDuration {
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub avg_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderCost {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub total_tokens: i64,
    #[serde(default)]
    pub total_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCost {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub total_tokens: i64,
    #[serde(default)]
    pub total_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailyCost {
    #[serde(default)]
    pub day: String,
    #[serde(default)]
    pub total_cost: f64,
    #[serde(default)]
    pub cumulative_cost: f64,
}

// ── Schema creation ──

async fn ensure_tables(client: &DuckDbClient) -> Result<(), crate::duckdb::DuckDbError> {
    client
        .create_table(
            "tasks",
            "CREATE TABLE IF NOT EXISTS tasks (
                id VARCHAR,
                title VARCHAR,
                phase VARCHAR,
                created_at VARCHAR,
                completed_at VARCHAR,
                duration_seconds DOUBLE
            )",
        )
        .await?;

    client
        .create_table(
            "kpi_snapshots",
            "CREATE TABLE IF NOT EXISTS kpi_snapshots (
                timestamp VARCHAR,
                metric VARCHAR,
                value DOUBLE
            )",
        )
        .await?;

    client
        .create_table(
            "costs",
            "CREATE TABLE IF NOT EXISTS costs (
                provider VARCHAR,
                model VARCHAR,
                tokens BIGINT,
                cost_usd DOUBLE,
                timestamp VARCHAR
            )",
        )
        .await?;

    Ok(())
}

// ── Data loading ──

async fn load_tasks(client: &DuckDbClient) -> Result<(), crate::duckdb::DuckDbError> {
    let beads = api::fetch_beads().await.unwrap_or_default();
    if beads.is_empty() {
        return Ok(());
    }

    // Clear existing data to avoid duplicates on refresh
    client.execute("DELETE FROM tasks").await.ok();

    let now = chrono::Utc::now().to_rfc3339();
    let rows: Vec<TaskRow> = beads
        .iter()
        .map(|b| {
            let phase = b.status.clone();
            let is_done = phase == "done" || phase == "Done";
            TaskRow {
                id: b.id.clone(),
                title: b.title.clone(),
                phase,
                created_at: now.clone(),
                completed_at: if is_done { now.clone() } else { String::new() },
                duration_seconds: 0.0,
            }
        })
        .collect();

    let json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string());
    client.insert_json("tasks", &json).await?;
    Ok(())
}

async fn load_kpi(client: &DuckDbClient) -> Result<(), crate::duckdb::DuckDbError> {
    let kpi = match api::fetch_kpi().await {
        Ok(k) => k,
        Err(_) => return Ok(()),
    };

    client.execute("DELETE FROM kpi_snapshots").await.ok();

    let now = chrono::Utc::now().to_rfc3339();
    let rows = vec![
        KpiRow {
            timestamp: now.clone(),
            metric: "total_beads".into(),
            value: kpi.total_beads as f64,
        },
        KpiRow {
            timestamp: now.clone(),
            metric: "backlog".into(),
            value: kpi.backlog as f64,
        },
        KpiRow {
            timestamp: now.clone(),
            metric: "hooked".into(),
            value: kpi.hooked as f64,
        },
        KpiRow {
            timestamp: now.clone(),
            metric: "review".into(),
            value: kpi.review as f64,
        },
        KpiRow {
            timestamp: now.clone(),
            metric: "done".into(),
            value: kpi.done as f64,
        },
        KpiRow {
            timestamp: now.clone(),
            metric: "failed".into(),
            value: kpi.failed as f64,
        },
        KpiRow {
            timestamp: now.clone(),
            metric: "active_agents".into(),
            value: kpi.active_agents as f64,
        },
    ];

    let json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string());
    client.insert_json("kpi_snapshots", &json).await?;
    Ok(())
}

async fn load_costs(client: &DuckDbClient) -> Result<(), crate::duckdb::DuckDbError> {
    let costs = match api::fetch_costs().await {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    client.execute("DELETE FROM costs").await.ok();

    let now = chrono::Utc::now().to_rfc3339();
    let input_cost_per_m = 3.0_f64;
    let output_cost_per_m = 15.0_f64;

    let mut rows = Vec::new();

    // Per-session breakdown
    for s in &costs.sessions {
        let tokens = s.input_tokens + s.output_tokens;
        let cost = s.input_tokens as f64 / 1_000_000.0 * input_cost_per_m
            + s.output_tokens as f64 / 1_000_000.0 * output_cost_per_m;
        rows.push(CostRow {
            provider: "anthropic".into(),
            model: s.agent_name.clone(),
            tokens,
            cost_usd: cost,
            timestamp: now.clone(),
        });
    }

    // If no sessions, create a summary row from totals
    if rows.is_empty() && (costs.input_tokens > 0 || costs.output_tokens > 0) {
        let cost = costs.input_tokens as f64 / 1_000_000.0 * input_cost_per_m
            + costs.output_tokens as f64 / 1_000_000.0 * output_cost_per_m;
        rows.push(CostRow {
            provider: "anthropic".into(),
            model: "claude".into(),
            tokens: costs.input_tokens + costs.output_tokens,
            cost_usd: cost,
            timestamp: now,
        });
    }

    if !rows.is_empty() {
        let json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string());
        client.insert_json("costs", &json).await?;
    }
    Ok(())
}

/// Initialize DuckDB, create tables, and load all data from the API.
/// Returns a `DuckDbClient` for running analytical queries.
pub async fn init_and_load() -> Result<DuckDbClient, crate::duckdb::DuckDbError> {
    let client = DuckDbClient::init().await?;
    ensure_tables(&client).await?;

    // Load data in parallel-ish fashion (all are independent)
    load_tasks(&client).await.ok();
    load_kpi(&client).await.ok();
    load_costs(&client).await.ok();

    Ok(client)
}

/// Refresh all DuckDB tables with fresh API data.
pub async fn refresh(client: &DuckDbClient) -> Result<(), crate::duckdb::DuckDbError> {
    load_tasks(client).await.ok();
    load_kpi(client).await.ok();
    load_costs(client).await.ok();
    Ok(())
}

// ── Pre-built analytical queries ──

pub async fn tasks_by_phase(client: &DuckDbClient) -> Vec<PhaseCount> {
    client
        .query::<PhaseCount>(
            "SELECT phase, COUNT(*) as count FROM tasks GROUP BY phase ORDER BY count DESC",
        )
        .await
        .unwrap_or_default()
}

pub async fn avg_duration_by_phase(client: &DuckDbClient) -> Vec<AvgDuration> {
    client
        .query::<AvgDuration>(
            "SELECT phase, AVG(duration_seconds) as avg_seconds FROM tasks WHERE duration_seconds > 0 GROUP BY phase",
        )
        .await
        .unwrap_or_default()
}

pub async fn cost_by_provider(client: &DuckDbClient) -> Vec<ProviderCost> {
    client
        .query::<ProviderCost>(
            "SELECT provider, SUM(tokens) as total_tokens, SUM(cost_usd) as total_cost FROM costs GROUP BY provider ORDER BY total_cost DESC",
        )
        .await
        .unwrap_or_default()
}

pub async fn cost_by_model(client: &DuckDbClient) -> Vec<ModelCost> {
    client
        .query::<ModelCost>(
            "SELECT model, SUM(tokens) as total_tokens, SUM(cost_usd) as total_cost FROM costs GROUP BY model ORDER BY total_cost DESC",
        )
        .await
        .unwrap_or_default()
}

pub async fn daily_cost_trend(client: &DuckDbClient) -> Vec<DailyCost> {
    client
        .query::<DailyCost>(
            "SELECT
                SUBSTRING(timestamp, 1, 10) as day,
                SUM(cost_usd) as total_cost,
                SUM(SUM(cost_usd)) OVER (ORDER BY SUBSTRING(timestamp, 1, 10)) as cumulative_cost
            FROM costs
            GROUP BY SUBSTRING(timestamp, 1, 10)
            ORDER BY day",
        )
        .await
        .unwrap_or_default()
}
