use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

// ── Raw JS bindings (duckdb-bridge.js) ──

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = duckdb)]
    pub async fn init_duckdb();

    #[wasm_bindgen(js_namespace = duckdb)]
    pub async fn query_duckdb(query: &str) -> JsValue;

    #[wasm_bindgen(js_namespace = duckdb)]
    pub async fn insert_json_duckdb(table_name: &str, json_string: &str) -> JsValue;

    #[wasm_bindgen(js_namespace = duckdb)]
    pub async fn create_table_duckdb(table_name: &str, schema_sql: &str) -> JsValue;
}

// ── Error type ──

#[derive(Debug, Clone)]
pub struct DuckDbError(pub String);

impl std::fmt::Display for DuckDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DuckDB: {}", self.0)
    }
}

impl From<String> for DuckDbError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

fn parse_js_result(val: JsValue) -> Result<String, DuckDbError> {
    let s = val.as_string().unwrap_or_else(|| {
        js_sys::JSON::stringify(&val)
            .map(|v| v.into())
            .unwrap_or_default()
    });
    // Check for error field in the JSON response
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            return Err(DuckDbError(err.to_string()));
        }
    }
    Ok(s)
}

// ── High-level client ──

/// A thin wrapper around the DuckDB WASM JS bridge.
/// All methods are async because they cross the JS boundary.
pub struct DuckDbClient {
    _private: (),
}

impl DuckDbClient {
    /// Initialize DuckDB WASM and return a client handle.
    pub async fn init() -> Result<Self, DuckDbError> {
        init_duckdb().await;
        Ok(Self { _private: () })
    }

    /// Execute a CREATE TABLE statement.
    pub async fn create_table(
        &self,
        table_name: &str,
        schema_sql: &str,
    ) -> Result<(), DuckDbError> {
        let result = create_table_duckdb(table_name, schema_sql).await;
        parse_js_result(result)?;
        Ok(())
    }

    /// Run a SQL query and deserialize the rows into `Vec<T>`.
    pub async fn query<T: DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>, DuckDbError> {
        let result = query_duckdb(sql).await;
        let json_str = parse_js_result(result)?;
        serde_json::from_str::<Vec<T>>(&json_str)
            .map_err(|e| DuckDbError(format!("deserialize: {e}")))
    }

    /// Run a SQL query and return the raw JSON string.
    pub async fn query_raw(&self, sql: &str) -> Result<String, DuckDbError> {
        let result = query_duckdb(sql).await;
        parse_js_result(result)
    }

    /// Insert rows (as a JSON array string) into a table.
    pub async fn insert_json(&self, table_name: &str, json: &str) -> Result<usize, DuckDbError> {
        let result = insert_json_duckdb(table_name, json).await;
        let s = parse_js_result(result)?;
        // Parse { ok: true, inserted: N }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            if let Some(n) = v.get("inserted").and_then(|n| n.as_u64()) {
                return Ok(n as usize);
            }
        }
        Ok(0)
    }

    /// Execute a raw SQL statement (CREATE, INSERT, DROP, etc.) without
    /// expecting row results.
    pub async fn execute(&self, sql: &str) -> Result<(), DuckDbError> {
        let result = query_duckdb(sql).await;
        parse_js_result(result)?;
        Ok(())
    }
}
