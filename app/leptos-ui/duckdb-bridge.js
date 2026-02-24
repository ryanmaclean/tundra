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

  // CDN URLs for DuckDB WASM
  const DUCKDB_CDN = "https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.28.0/dist";

  function loadScript(src) {
    return new Promise((resolve, reject) => {
      const s = document.createElement("script");
      s.src = src;
      s.onload = resolve;
      s.onerror = () => reject(new Error("Failed to load " + src));
      document.head.appendChild(s);
    });
  }

  async function doInit() {
    if (db) return;

    // Load the DuckDB WASM bundles from CDN
    await loadScript(DUCKDB_CDN + "/duckdb-browser-blocking.js");

    // Use the UMD global `duckdb_wasm` that the CDN script exposes as `duckdb`
    // We need to be careful not to shadow our own namespace.
    const duckdbLib = globalThis.duckdb_wasm || globalThis.duckdb_library;

    // Fallback: use the ESM worker-based approach via jsdelivr
    // DuckDB WASM exposes AsyncDuckDB for the browser.
    if (typeof duckdbLib !== "undefined" && duckdbLib.AsyncDuckDB) {
      const MANUAL_BUNDLES = {
        mvp: {
          mainModule: DUCKDB_CDN + "/duckdb-mvp.wasm",
          mainWorker: DUCKDB_CDN + "/duckdb-browser-mvp.worker.js",
        },
        eh: {
          mainModule: DUCKDB_CDN + "/duckdb-eh.wasm",
          mainWorker: DUCKDB_CDN + "/duckdb-browser-eh.worker.js",
        },
      };

      const bundle = await duckdbLib.selectBundle(MANUAL_BUNDLES);
      const worker = new Worker(bundle.mainWorker);
      const logger = new duckdbLib.ConsoleLogger();
      db = new duckdbLib.AsyncDuckDB(logger, worker);
      await db.instantiate(bundle.mainModule);
      conn = await db.connect();
      return;
    }

    // Simplest fallback: use the blocking (synchronous) API in-thread.
    // This works without a web worker and is fine for analytics queries.
    if (typeof globalThis.duckdb !== "undefined" && globalThis.duckdb.DuckDBClient) {
      // duckdb-browser-blocking exposes DuckDBClient
      db = await globalThis.duckdb.DuckDBClient.of();
      conn = db;
      return;
    }

    // If none of the above worked, create a minimal stub that stores data
    // in-memory using plain JS objects so the UI still functions.
    console.warn("[duckdb-bridge] DuckDB WASM could not be loaded; using in-memory stub");
    db = createStubDb();
    conn = db;
  }

  // ── Stub implementation for graceful degradation ──

  function createStubDb() {
    const tables = {};

    return {
      _stub: true,
      _tables: tables,

      async query(sql) {
        // Very basic SQL parser for SELECT ... FROM <table>
        const selectMatch = sql.match(
          /SELECT\s+(.+?)\s+FROM\s+(\w+)/i
        );
        if (selectMatch) {
          const tableName = selectMatch[2];
          const rows = tables[tableName] || [];
          return { toArray: () => rows.map((r) => ({ toJSON: () => r })) };
        }
        // For CREATE TABLE, just register the table name
        const createMatch = sql.match(/CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)/i);
        if (createMatch) {
          const name = createMatch[1];
          if (!tables[name]) tables[name] = [];
          return { toArray: () => [] };
        }
        return { toArray: () => [] };
      },

      async run(sql) {
        return this.query(sql);
      },

      async insertJSON(tableName, rows) {
        if (!tables[tableName]) tables[tableName] = [];
        if (Array.isArray(rows)) {
          tables[tableName].push(...rows);
        }
      },
    };
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
        return JSON.stringify(rows);
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
        console.error("[duckdb-bridge] init failed, falling back to stub:", err);
        db = createStubDb();
        conn = db;
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

      // Use insertJSON if available (stub and some DuckDB builds)
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

  // Expose on global `duckdb` namespace for wasm_bindgen
  globalThis.duckdb = globalThis.duckdb || {};
  Object.assign(globalThis.duckdb, {
    init_duckdb,
    query_duckdb,
    insert_json_duckdb,
    create_table_duckdb,
  });
})();
