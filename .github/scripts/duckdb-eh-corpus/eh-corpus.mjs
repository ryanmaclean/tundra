// Source-only installed WebKit diagnostic. Import from a test-only initialization
// script; never ship this module in the production asset bill.
// The primary cases use Tundra's bridge connection. Direct API cases create a
// second worker/connection from the *same installed local worker and Wasm URLs*.

const stable = value => JSON.stringify(value, (_key, item) =>
  typeof item === 'bigint' ? item.toString() : item);

async function digest(value) {
  const bytes = value instanceof Uint8Array ? value :
    new TextEncoder().encode(stable(value));
  const hash = await crypto.subtle.digest('SHA-256', bytes);
  return { sha256: Array.from(new Uint8Array(hash), byte =>
    byte.toString(16).padStart(2, '0')).join(''), bytes: bytes.length };
}

function classify(message) {
  return /(?:Parser|Catalog|Binder|Conversion|Invalid Input|IO|Internal) Error/i
    .exec(message)?.[0] || 'Error';
}

function canonicalRows(table) {
  return table.toArray().map(row => row && typeof row.toJSON === 'function'
    ? row.toJSON() : Object.assign({}, row));
}

async function probeMalformedArrowIpc(conn) {
  const bytes = new Uint8Array([255, 255, 255, 255, 255, 255, 255, 255]);
  const fixture = await digest(bytes);
  const fixturePinned = fixture.sha256 ===
    '12a3ae445661ce5dee78d0650d33362dec29c4f82af05e7e57fb595bbbacf0ca' &&
    fixture.bytes === 8;
  let message = '';
  try {
    await conn.insertArrowFromIPCStream(bytes,
      { name: 'eh_corpus_bad_arrow' });
  } catch (error) {
    message = String(error);
  }
  let absentMessage = '';
  try {
    const table = await conn.query('SELECT * FROM eh_corpus_bad_arrow LIMIT 1');
    table.toArray();
  } catch (error) {
    absentMessage = String(error);
  }
  let recovered = false;
  try {
    const table = await conn.query('SELECT 42 AS alive');
    recovered = stable(canonicalRows(table)) === '[{"alive":42}]';
  } catch (_error) { /* Recovery remains false. */ }
  const errorClass = classify(message);
  const marker = message.split(/\r?\n/, 1)[0].slice(0, 120);
  const rejectedHeader = marker === 'Error: Invalid IPC message: negative metadata length';
  const absent = classify(absentMessage) === 'Catalog Error' &&
    /Table with name eh_corpus_bad_arrow does not exist/i.test(absentMessage);
  return { id: 'direct-arrow-invalid-ipc', phase: 'direct',
    passed: fixturePinned && errorClass === 'Error' && rejectedHeader &&
      absent && recovered,
    errorClass, marker, rejectionStage: message ? 'registration' : 'NONE',
    fixtureSha256: fixture.sha256, fixtureBytes: fixture.bytes,
    absent, absenceClass: classify(absentMessage), recovered };
}

async function probeDirectConversion(conn) {
  const tableName = 'eh_corpus_direct_conversion';
  const sql = `SELECT CAST(s AS INTEGER) AS rejected FROM ${tableName} WHERE i = 7`;
  const failure = error => {
    const message = String(error);
    return { errorClass: classify(message), marker: message.split(/\r?\n/, 1)[0].slice(0, 120) };
  };
  const diagnostic = { status: 'SETUP', readback: null, query: null,
    rawRunQuery: null, recovered: false };
  try {
    await conn.query(`CREATE TABLE ${tableName} (i INTEGER, s VARCHAR)`);
    await conn.query(`INSERT INTO ${tableName} VALUES (7, 'O''Reilly')`);
    const rows = canonicalRows(await conn.query(
      `SELECT i,s FROM ${tableName} ORDER BY i`));
    diagnostic.readback = { matches: stable(rows) === stable([{ i: 7, s: "O'Reilly" }]),
      digest: await digest(rows) };
    if (!diagnostic.readback.matches) {
      diagnostic.status = 'READBACK_MISMATCH';
      return diagnostic;
    }
  } catch (error) {
    diagnostic.status = 'SETUP_ERROR';
    diagnostic.setupError = failure(error);
    return diagnostic;
  }
  diagnostic.status = 'PROBED';
  let table;
  try {
    table = await conn.query(sql);
  } catch (error) {
    diagnostic.query = { stage: 'query-rejected', ...failure(error) };
  }
  if (table) {
    const numRows = table.numRows;
    const fields = table.schema?.fields?.slice(0, 8).map(field =>
      String(field.name).slice(0, 40)) || [];
    diagnostic.query = { stage: 'query-resolved', numRows, fields };
    try {
      if (!Number.isSafeInteger(numRows) || numRows < 0 || numRows > 8) {
        diagnostic.query.rows = 'BOUNDED_REFUSAL';
      } else {
        const rows = canonicalRows(table);
        diagnostic.query.rowsDigest = await digest(rows);
        diagnostic.query.rowsLength = rows.length;
      }
    } catch (error) {
      diagnostic.query.rows = 'materialization-rejected';
      diagnostic.query.materializationError = failure(error);
    }
  } else if (!diagnostic.query) {
    diagnostic.query = { stage: 'query-falsy' };
  }
  try {
    const bytes = await conn.useUnsafe((bindings, id) => bindings.runQuery(id, sql));
    diagnostic.rawRunQuery = bytes instanceof Uint8Array ?
      { stage: 'raw-resolved', digest: await digest(bytes) } :
      { stage: 'raw-nonbytes', type: typeof bytes };
  } catch (error) {
    diagnostic.rawRunQuery = { stage: 'raw-rejected', ...failure(error) };
  }
  try {
    diagnostic.recovered = stable(canonicalRows(await conn.query('SELECT 42 AS alive'))) ===
      '[{"alive":42}]';
  } catch (error) {
    diagnostic.recoveryError = failure(error);
  }
  return diagnostic;
}

export async function runInstalledEhCorpus(bridge) {
  const cases = [];
  const fixtures = {};
  const conversionDiagnostics = { bridge: null, direct: { status: 'NOT_STARTED' } };
  const record = async (id, phase, operation, check) => {
    try {
      const observed = await operation();
      const passed = !!check(observed);
      const canonical = await digest(observed);
      cases.push({ id, phase, passed, canonicalSha256: canonical.sha256,
        canonicalBytes: canonical.bytes });
      return observed;
    } catch (error) {
      cases.push({ id, phase, passed: false,
        errorClass: classify(String(error)), error: String(error).slice(0, 160) });
      return undefined;
    }
  };
  const query = async sql => {
    const value = JSON.parse(await bridge.query_duckdb(sql));
    if (!Array.isArray(value)) throw new Error(value?.error || `non-row result: ${stable(value)}`);
    return value;
  };
  const expectBridgeError = async (id, sql, expectedClass) => {
    const value = JSON.parse(await bridge.query_duckdb(sql));
    const message = typeof value?.error === 'string' ? value.error : '';
    const diagnostic = id === 'bridge-conversion-error' ?
      { responseShape: Array.isArray(value) ? 'array' :
          (value === null ? 'null' : typeof value),
        responseDigest: await digest(value),
        responseLength: Array.isArray(value) ? value.length : null } : {};
    cases.push({ id, phase: 'bridge', passed: !!message &&
      classify(message).toLowerCase().includes(expectedClass.toLowerCase()),
      errorClass: classify(message), marker: message.split(/\r?\n/, 1)[0].slice(0, 120),
      ...diagnostic });
    if (id === 'bridge-conversion-error') {
      conversionDiagnostics.bridge = { errorClass: classify(message),
        marker: message.split(/\r?\n/, 1)[0].slice(0, 120), ...diagnostic };
    }
  };
  const expectDirectError = async (id, operation, expectedClass) => {
    let message = '';
    try { await operation(); } catch (error) { message = String(error); }
    const errorClass = classify(message);
    cases.push({ id, phase: 'direct', passed: !!message &&
      errorClass.toLowerCase().includes(expectedClass.toLowerCase()),
      errorClass, marker: message.split(/\r?\n/, 1)[0].slice(0, 120) });
  };
  const response = async (method, ...args) => {
    const value = JSON.parse(await bridge[method](...args));
    if (typeof value?.error === 'string') throw new Error(value.error);
    return value;
  };

  // App bridge: all reads below use its retained connection.
  await record('bridge-create', 'bridge', () => response('create_table_duckdb',
    'eh_corpus_bridge', 'CREATE TABLE eh_corpus_bridge (i INTEGER, s VARCHAR)'),
  v => v.ok === true);
  await record('bridge-json-insert', 'bridge', () => response('insert_json_duckdb',
    'eh_corpus_bridge', stable([{ i: 7, s: "O'Reilly" }, { i: 11, s: null }])),
  v => v.ok === true && v.inserted === 2);
  await record('bridge-empty-insert', 'bridge', () => response('insert_json_duckdb',
    'eh_corpus_bridge', '[]'), v => v.ok === true && v.inserted === 0);
  await record('bridge-persist-null-utf8', 'bridge', () => query(
    'SELECT i,s FROM eh_corpus_bridge ORDER BY i'),
  v => stable(v) === stable([{ i: 7, s: "O'Reilly" }, { i: 11, s: null }]));
  await record('bridge-macro-create', 'bridge', () => query(
    'CREATE MACRO eh_corpus_macro(x) AS TABLE SELECT x AS v'), Array.isArray);
  await record('bridge-macro-query', 'bridge', () => query(
    'SELECT * FROM eh_corpus_macro(7)'), v => stable(v) === '[{"v":7}]');
  await expectBridgeError('bridge-parser-error', 'SELECT FROM', 'Parser');
  await record('bridge-parser-recovery', 'bridge', () => query(
    'SELECT count(*) AS n FROM eh_corpus_bridge'), v => v[0]?.n === 2);
  await expectBridgeError('bridge-catalog-error',
    'SELECT * FROM eh_corpus_missing_table', 'Catalog');
  await record('bridge-catalog-recovery', 'bridge', () => query(
    'SELECT * FROM eh_corpus_macro(11)'), v => stable(v) === '[{"v":11}]');
  await expectBridgeError('bridge-binder-error',
    'SELECT eh_corpus_missing_column FROM eh_corpus_bridge', 'Binder');
  await expectBridgeError('bridge-conversion-error',
    'SELECT CAST(s AS INTEGER) AS rejected FROM eh_corpus_bridge WHERE i = 7',
    'Conversion');
  await record('bridge-after-errors', 'bridge', () => query(
    'SELECT 42 AS alive'), v => stable(v) === '[{"alive":42}]');
  await record('bridge-arrow-json-types', 'bridge', () => query(
    "SELECT CAST(NULL AS INTEGER) AS absent, true AS flag, " +
    "CAST(2147483648 AS BIGINT) AS big, 'héllo' AS utf8"),
  v => v.length === 1 && v[0].absent === null && v[0].flag === true &&
    [2147483648, '2147483648'].includes(v[0].big) && v[0].utf8 === 'héllo');
  await record('bridge-large-bigint-string', 'bridge', () => query(
    'SELECT CAST(9007199254740993 AS BIGINT) AS big'),
  v => v.length === 1 && v[0].big === '9007199254740993');

  const runtime = bridge.get_runtime_info();
  const local = [runtime?.workerUrl, runtime?.wasmUrl].every(value => {
    if (!value) return false;
    const url = new URL(value);
    return url.origin === location.origin && url.pathname.includes('/duckdb-runtime/');
  });
  const moduleUrl = runtime?.workerUrl ?
    new URL('duckdb-browser.mjs', runtime.workerUrl).href : '';
  const selected = { workerUrl: runtime?.workerUrl || '',
    wasmUrl: runtime?.wasmUrl || '', moduleUrl };
  cases.push({ id: 'installed-local-pair-urls', phase: 'provenance', passed: local,
    ...selected });
  if (!local) return { schema: 'tundra-installed-eh-corpus/v1', cases,
    passed: false, directConnection: 'NOT_STARTED', selected,
    fixtures, conversionDiagnostics,
    unsupported: ['arrow-positive-ipc', 'multibatch-stream', 'upstream-full-suite'],
    skipped: ['direct-adapter'] };

  await record('installed-local-pair-bytes', 'provenance', async () => {
    const hashes = {};
    for (const [kind, url] of Object.entries({ module: moduleUrl,
      worker: runtime.workerUrl, wasm: runtime.wasmUrl })) {
      const response = await fetch(url, { cache: 'no-store' });
      if (!response.ok) throw new Error(`local ${kind} fetch ${response.status}`);
      hashes[kind] = await digest(new Uint8Array(await response.arrayBuffer()));
    }
    Object.assign(selected, { moduleSha256: hashes.module.sha256,
      workerSha256: hashes.worker.sha256, wasmSha256: hashes.wasm.sha256 });
    return hashes;
  }, hashes => Object.values(hashes).length === 3 &&
    Object.values(hashes).every(value => value.sha256.length === 64 && value.bytes > 0));

  // Direct API adapter: same installed pair bytes, independent worker and
  // connection. Do not count its recovery as proof about the app connection.
  let worker, db, conn;
  try {
    const duckdb = await import(moduleUrl);
    worker = new Worker(runtime.workerUrl);
    db = new duckdb.AsyncDuckDB(new duckdb.ConsoleLogger(), worker);
    await db.instantiate(runtime.wasmUrl);
    conn = await db.connect();
    const direct = sql => conn.query(sql).then(canonicalRows);
    await record('direct-arrow-schema', 'direct', async () => {
      const table = await conn.query('SELECT 7::INTEGER AS i, NULL::VARCHAR AS s');
      return { fields: table.schema.fields.map(f => f.name), rows: canonicalRows(table) };
    }, v => stable(v.fields) === '["i","s"]' && v.rows.length === 1 &&
      v.rows[0].i === 7 && v.rows[0].s === null);

    const csv = new TextEncoder().encode('i,s\n7,"a,b"\n11,"héllo"\n');
    fixtures.csv = await digest(csv);
    await record('direct-csv-register', 'direct', () =>
      db.registerFileBuffer('eh_corpus_input.csv', csv).then(() => csv.length),
    v => v === csv.length);
  await record('direct-csv-scan', 'direct', () => direct(
      "SELECT i,s FROM read_csv_auto('eh_corpus_input.csv') ORDER BY i"),
    v => stable(v) === stable([{ i: '7', s: 'a,b' }, { i: '11', s: 'héllo' }]));
    await record('direct-csv-insert', 'direct', async () => {
      await conn.insertCSVFromPath('eh_corpus_input.csv', { name: 'eh_corpus_csv' });
      return direct('SELECT i,s FROM eh_corpus_csv ORDER BY i');
    }, v => stable(v) === stable([{ i: '7', s: 'a,b' }, { i: '11', s: 'héllo' }]));

    const json = new TextEncoder().encode('[{"i":2,"s":"j"},{"i":3,"s":null}]');
    fixtures.json = await digest(json);
    await record('direct-json-register', 'direct', () =>
      db.registerFileBuffer('eh_corpus_input.json', json).then(() => json.length),
    v => v === json.length);
    await record('direct-json-scan', 'direct', () => direct(
      "SELECT i::INTEGER AS i,s FROM read_json_auto('eh_corpus_input.json') ORDER BY i"),
    v => stable(v) === stable([{ i: 2, s: 'j' }, { i: 3, s: null }]));
    await record('direct-json-insert', 'direct', async () => {
      await conn.insertJSONFromPath('eh_corpus_input.json', { name: 'eh_corpus_json' });
      return direct('SELECT i,s FROM eh_corpus_json ORDER BY i');
    }, v => stable(v) === stable([{ i: 2, s: 'j' }, { i: 3, s: null }]));

  await record('direct-parquet-copy', 'direct', () => direct(
      "COPY (SELECT 7::INTEGER AS i, 'p'::VARCHAR AS s) TO 'eh_corpus_out.parquet' (FORMAT PARQUET)"),
    v => stable(v) === '[{"Count":"1"}]');
    const parquet = await record('direct-parquet-buffer', 'direct', () =>
      db.copyFileToBuffer('eh_corpus_out.parquet'),
    v => v instanceof Uint8Array && v.length > 8);
    if (parquet instanceof Uint8Array) {
      await record('direct-parquet-reregister', 'direct', () =>
        db.registerFileBuffer('eh_corpus_in.parquet', parquet).then(() => parquet.length),
      v => v === parquet.length);
      await record('direct-parquet-scan', 'direct', async () => {
        const table = await conn.query("SELECT i,s FROM parquet_scan('eh_corpus_in.parquet')");
        return { fields: table.schema.fields.map(field => field.name),
          rows: canonicalRows(table) };
      }, v => stable(v.fields) === '["i","s"]' &&
        stable(v.rows) === '[{"i":7,"s":"p"}]');
    } else {
      cases.push({ id: 'direct-parquet-scan', phase: 'direct', passed: false,
        error: 'Parquet buffer unavailable' });
    }
    await expectDirectError('direct-file-error', () =>
      direct("SELECT * FROM parquet_scan('eh_corpus_missing.parquet')"), 'IO');
    await record('direct-after-error', 'direct', () => direct('SELECT 42 AS alive'),
      v => stable(v) === '[{"alive":42}]');
    cases.push(await probeMalformedArrowIpc(conn));
    await record('direct-after-arrow-error', 'direct', () => direct('SELECT 42 AS alive'),
      v => stable(v) === '[{"alive":42}]');
    try {
      conversionDiagnostics.direct = await probeDirectConversion(conn);
    } catch (error) {
      const message = String(error);
      conversionDiagnostics.direct = { status: 'PROBE_ERROR',
        errorClass: classify(message), marker: message.split(/\r?\n/, 1)[0].slice(0, 120) };
    }
  } catch (error) {
    cases.push({ id: 'direct-adapter-init', phase: 'direct', passed: false,
      errorClass: classify(String(error)), error: String(error).slice(0, 160) });
  } finally {
    if (conn) await conn.close().catch(() => {});
    if (worker) worker.terminate();
  }
  return { schema: 'tundra-installed-eh-corpus/v1',
    cases, passed: cases.every(item => item.passed),
    directConnection: 'SEPARATE_SAME_INSTALLED_PAIR',
    selected, fixtures, conversionDiagnostics, skipped: [],
    unsupported: ['arrow-positive-ipc', 'multibatch-stream', 'upstream-full-suite'] };
}
