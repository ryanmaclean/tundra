const expectedSchema = "tundra-duckdb-worker-crypto-probe/v1";
const expectedHash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const results = [];
const cspViolations = [];
window.addEventListener("securitypolicyviolation", event => {
  if (cspViolations.length < 16) cspViolations.push(event.effectiveDirective);
});
const failed = error => ({ pass: false, error: String(error).slice(0, 160), results });

async function runVariant(variant, duckdb) {
  const packets = [];
  const worker = new Worker(`./duckdb-crypto-${variant}.wrapper.js`);
  // Register first. A diagnostic packet must never enter AsyncDuckDB's
  // protocol handler, including packets emitted during importScripts.
  worker.addEventListener("message", event => {
    if (event.data?.schema !== expectedSchema) return;
    event.stopImmediatePropagation();
    packets.push(event.data);
  }, true);
  const database = new duckdb.AsyncDuckDB(new duckdb.ConsoleLogger(), worker);
  let connection;
  try {
    await database.instantiate(`./duckdb-${variant}.wasm`);
    connection = await database.connect();
    const answer = await connection.query("SELECT CAST(42 AS INTEGER) AS answer");
    const hash = await connection.query("SELECT sha256('abc') AS digest");
    const answerRows = answer.toArray().map(row => row.toJSON());
    const hashRows = hash.toArray().map(row => row.toJSON());
    worker.postMessage({ __tundraCryptoProbe: true });
    for (let attempt = 0; attempt < 50 &&
      !packets.some(packet => packet.phase === "after-sql" ||
        packet.phase === "after-sql-failed"); attempt += 1) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    const before = packets.find(packet => packet.phase === "before-import");
    const after = packets.find(packet => packet.phase === "after-import");
    const final = packets.find(packet => packet.phase === "after-sql");
    const failures = packets.filter(packet =>
      packet.phase === "require-call" || packet.phase.endsWith("-failed") ||
      packet.phase === "worker-error" || packet.phase === "worker-rejection" ||
      packet.phase === "import-failure");
    const clean = packet => packet && packet.requireCalls === 0 && packet.errors === 0 &&
      packet.node.nodeVersion === false && packet.node.nodeRelease === false &&
      packet.node.bufferType === "undefined" && packet.node.moduleType === "undefined" &&
      packet.node.exportsType === "undefined" &&
      packet.webCrypto.digestType === "function" &&
      packet.webCrypto.getRandomValuesType === "function";
    return {
      variant, pass: failures.length === 0 && clean(before) && clean(after) &&
        clean(final) && final.error === null &&
        final.webCryptoDigest === expectedHash &&
        answerRows.length === 1 && answerRows[0].answer === 42 &&
        hashRows.length === 1 && hashRows[0].digest === expectedHash,
      packets, answerRows, hashRows,
      jsSha256GuardBranch: "unproven-lexical-commonjs-module",
    };
  } finally {
    if (connection) await connection.close();
    await database.terminate();
    worker.terminate();
  }
}

try {
  const duckdb = await import("./duckdb-browser.mjs");
  for (const variant of ["mvp", "eh"]) results.push(await runVariant(variant, duckdb));
  const remoteRequests = performance.getEntriesByType("resource").filter(entry => {
    const url = new URL(entry.name);
    return ["http:", "https:"].includes(url.protocol) &&
      url.origin !== location.origin;
  }).length;
  window.__tundraCryptoProbe = {
    schema: "tundra-duckdb-webkit-crypto-harness/v1",
    pass: results.length === 2 && results.every(result => result.pass) &&
      cspViolations.length === 0 && remoteRequests === 0,
    scope: "browser-only-diagnostic",
    installedTauriWebview: false,
    jsSha256GuardBranchProven: false,
    cspViolations,
    remoteRequests,
    results,
  };
} catch (error) {
  window.__tundraCryptoProbe = failed(error);
}
