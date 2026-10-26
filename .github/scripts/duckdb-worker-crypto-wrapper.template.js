/* Diagnostic-only classic worker. Serve beside the two unchanged installed workers. */
(() => {
  "use strict";
  const schema = "tundra-duckdb-worker-crypto-probe/v1";
  const variant = "__VARIANT__";
  const observed = { requireCalls: 0, messages: 0, errors: 0 };
  const nodeSignature = () => {
    const process = globalThis.process;
    return {
    processType: typeof process,
    nodeVersion: !!process && !!process.versions &&
      typeof process.versions.node === "string",
    nodeRelease: !!process && !!process.release &&
      process.release.name === "node",
    bufferType: typeof globalThis.Buffer,
    moduleType: typeof globalThis.module,
    exportsType: typeof globalThis.exports,
    };
  };
  const webCrypto = () => ({
    cryptoType: typeof globalThis.crypto,
    subtleType: globalThis.crypto ? typeof globalThis.crypto.subtle : "undefined",
    digestType: globalThis.crypto && globalThis.crypto.subtle ?
      typeof globalThis.crypto.subtle.digest : "undefined",
    getRandomValuesType: globalThis.crypto ?
      typeof globalThis.crypto.getRandomValues : "undefined",
  });
  const report = (phase, error, digest) => self.postMessage({
    schema, variant, phase, node: nodeSignature(), webCrypto: webCrypto(),
    requireCalls: observed.requireCalls, messages: observed.messages,
    errors: observed.errors, error: error ? String(error).slice(0, 160) : null,
    webCryptoDigest: digest ? digest.webCrypto : null,
    jsSha256Digest: digest ? digest.jsSha256 : null,
  });
  // The harness sends this only after the SQL reply. Capture it before the
  // upstream DuckDB handler and return a worker-side, post-query snapshot.
  self.addEventListener("message", async event => {
    if (!event.data || event.data.__tundraCryptoProbe !== true) return;
    event.stopImmediatePropagation();
    try {
      assertBrowserCrypto("after-sql");
      const digestBytes = await globalThis.crypto.subtle.digest(
        "SHA-256", new TextEncoder().encode("abc"));
      const webCryptoDigest = Array.from(new Uint8Array(digestBytes), byte =>
        byte.toString(16).padStart(2, "0")).join("");
      // The pinned worker wraps js-sha256 as a lexical CommonJS module. It
      // has no global export; a global invocation would test different code.
      report("after-sql", null, {
          webCrypto: webCryptoDigest, jsSha256: null,
        });
    } catch (error) {
      observed.errors += 1;
      report("after-sql-failed", error);
    }
  }, true);
  const assertBrowserCrypto = phase => {
    const node = nodeSignature();
    const crypto = webCrypto();
    if (node.nodeVersion || node.nodeRelease || node.bufferType !== "undefined" ||
        node.moduleType !== "undefined" || node.exportsType !== "undefined") {
      report(phase + "-failed", "Node signature present");
      throw new Error("Tundra diagnostic found Node worker signature");
    }
    if (crypto.digestType !== "function" ||
        crypto.getRandomValuesType !== "function") {
      report(phase + "-failed", "WebCrypto absent");
      throw new Error("Tundra diagnostic requires worker WebCrypto");
    }
  };
  self.addEventListener("message", () => { observed.messages += 1; }, true);
  self.addEventListener("error", event => {
    observed.errors += 1;
    report("worker-error", event.message);
  });
  self.addEventListener("unhandledrejection", event => {
    observed.errors += 1;
    report("worker-rejection", event.reason);
  });
  Object.defineProperty(globalThis, "require", {
    configurable: false, writable: false, value: function diagnosticRequireTrap() {
      observed.requireCalls += 1;
      report("require-call", "blocked global require");
      throw new Error("Tundra diagnostic blocked worker global require");
    },
  });
  report("before-import");
  assertBrowserCrypto("before-import");
  try {
    importScripts("./duckdb-browser-__VARIANT__.worker.js");
    report("after-import");
    assertBrowserCrypto("after-import");
  } catch (error) {
    observed.errors += 1;
    report("import-failure", error);
    throw error;
  }
})();
