import * as duckdb from 'https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.28.0/+esm';

let db = null;
let conn = null;

export async function init_duckdb() {
    if (db) return;
    const JSDELIVR_BUNDLES = duckdb.getJsDelivrBundles();
    const bundle = await duckdb.selectBundle(JSDELIVR_BUNDLES);
    const worker_url = URL.createObjectURL(
        new Blob([`importScripts("${bundle.mainWorker}");`], { type: 'text/javascript' })
    );
    const worker = new Worker(worker_url);
    const logger = new duckdb.ConsoleLogger();
    db = new duckdb.AsyncDuckDB(logger, worker);
    await db.instantiate(bundle.mainModule, bundle.pthreadWorker);
    URL.revokeObjectURL(worker_url);
    conn = await db.connect();
    console.log("DuckDB initialized.");
}

export async function query_duckdb(query) {
    if (!conn) throw new Error("DuckDB not initialized");
    const result = await conn.query(query);
    // Convert Arrow table to an array of objects
    return JSON.stringify(result.toArray().map(row => row.toJSON()));
}

export async function insert_json_duckdb(table_name, json_string) {
    if (!db) throw new Error("DuckDB not initialized");
    await db.registerFileText(`${table_name}.json`, json_string);
    await conn.query(`CREATE OR REPLACE TABLE ${table_name} AS SELECT * FROM read_json_auto('${table_name}.json')`);
    return true;
}
