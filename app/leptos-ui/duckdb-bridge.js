/* DuckDB WASM bridge for auto-tundra Leptos frontend.
 *
 * Exposes a global `duckdb` namespace with async functions that the Rust
 * wasm_bindgen bindings in duckdb.rs call via `#[wasm_bindgen(js_namespace = duckdb)]`.
 *
 * Functions:
 *   duckdb.init_duckdb()                          -> void
 *   duckdb.query_duckdb(sql)                      -> JSON string (array of row objects)
 *   duckdb.insert_json_duckdb(table_name, json)   -> JSON string ({ ok: true } or { error })
 *   duckdb.create_table_duckdb(table_name, sql)   -> JSON string ({ ok: true } or { error })
 */
(function () {
  "use strict";

  let db = null;
  let conn = null;
  let initPromise = null;
  let runtimeInfo = null;

  const RUNTIME_BASE = new URL("./duckdb-runtime/", document.baseURI);

  async function doInit() {
    if (db) return;

    const duckdbLib = await import(new URL("duckdb-browser.mjs", RUNTIME_BASE).href);
    const local = (name) => new URL(name, RUNTIME_BASE).href;
    const bundles = {
      mvp: {
        mainModule: local("duckdb-mvp.wasm"),
        mainWorker: local("duckdb-browser-mvp.worker.js"),
      },
      eh: {
        mainModule: local("duckdb-eh.wasm"),
        mainWorker: local("duckdb-browser-eh.worker.js"),
      },
    };
    const bundle = await duckdbLib.selectBundle(bundles);
    if (!bundle || !bundle.mainWorker || !bundle.mainModule) {
      throw new Error("No supported local DuckDB WASM bundle");
    }
    const worker = new Worker(bundle.mainWorker);
    let candidate;
    try {
      candidate = new duckdbLib.AsyncDuckDB(new duckdbLib.ConsoleLogger(), worker);
      await candidate.instantiate(bundle.mainModule);
      const connection = await candidate.connect();
      db = candidate;
      conn = connection;
      runtimeInfo = { workerUrl: bundle.mainWorker, wasmUrl: bundle.mainModule };
    } catch (error) {
      worker.terminate();
      throw error;
    }
  }

  // ── Helpers to normalize query results ──

  function resultToJson(result) {
    try {
      if (!result) return "[]";
      // AsyncDuckDB returns Arrow tables with .toArray()
      if (typeof result.toArray === "function") {
        const arr = result.toArray();
        const rows = arr.map((row) => {
          if (typeof row.toJSON === "function") return row.toJSON();
          return Object.assign({}, row);
        });
        return JSON.stringify(rows, (_key, value) =>
          typeof value === "bigint" ?
            (Number.isSafeInteger(Number(value)) ? Number(value) : value.toString()) : value);
      }
      // DuckDBClient.of() returns iterables
      if (Symbol.iterator in Object(result)) {
        return JSON.stringify(Array.from(result));
      }
      return JSON.stringify(result);
    } catch (e) {
      return JSON.stringify({ error: String(e) });
    }
  }

  // ── Public API ──

  async function init_duckdb() {
    if (!initPromise) {
      initPromise = doInit().catch((err) => {
        db = null;
        conn = null;
        runtimeInfo = null;
        initPromise = null;
        throw err;
      });
    }
    await initPromise;
  }

  async function query_duckdb(sql) {
    await init_duckdb();
    try {
      const result = await conn.query(sql);
      return resultToJson(result);
    } catch (e) {
      return JSON.stringify({ error: String(e) });
    }
  }

  async function insert_json_duckdb(tableName, jsonString) {
    await init_duckdb();
    try {
      const rows = JSON.parse(jsonString);
      if (!Array.isArray(rows) || rows.length === 0) {
        return JSON.stringify({ ok: true, inserted: 0 });
      }

      // Use insertJSON if available in a future DuckDB build.
      if (typeof conn.insertJSON === "function") {
        await conn.insertJSON(tableName, rows);
        return JSON.stringify({ ok: true, inserted: rows.length });
      }

      // Fallback: build INSERT statements
      for (const row of rows) {
        const cols = Object.keys(row);
        const vals = cols.map((c) => {
          const v = row[c];
          if (v === null || v === undefined) return "NULL";
          if (typeof v === "number") return String(v);
          return "'" + String(v).replace(/'/g, "''") + "'";
        });
        const sql = `INSERT INTO ${tableName} (${cols.join(",")}) VALUES (${vals.join(",")})`;
        await conn.query(sql);
      }
      return JSON.stringify({ ok: true, inserted: rows.length });
    } catch (e) {
      return JSON.stringify({ error: String(e) });
    }
  }

  async function create_table_duckdb(tableName, schemaSql) {
    await init_duckdb();
    try {
      await conn.query(schemaSql);
      return JSON.stringify({ ok: true, table: tableName });
    } catch (e) {
      return JSON.stringify({ error: String(e) });
    }
  }

  function csp_denial_probe() {
    try {
      Function("return 1")();
      return false;
    } catch (error) {
      return error instanceof EvalError;
    }
  }

  function get_runtime_info() {
    return runtimeInfo && { ...runtimeInfo };
  }

  // Expose on global `duckdb` namespace for wasm_bindgen
  globalThis.duckdb = globalThis.duckdb || {};
  Object.assign(globalThis.duckdb, {
    init_duckdb,
    query_duckdb,
    insert_json_duckdb,
    create_table_duckdb,
    csp_denial_probe,
    get_runtime_info,
  });
})();
