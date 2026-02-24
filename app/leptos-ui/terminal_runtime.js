/* global Terminal, FitAddon */
(function () {
  const terminals = new Map();
  let seq = 0;

  function nextId() {
    seq += 1;
    return `tundra-term-${seq}`;
  }

  function hasXterm() {
    return typeof Terminal !== "undefined" && typeof FitAddon !== "undefined";
  }

  function defaultTheme() {
    return {
      background: "#09070f",
      foreground: "#d7d3f6",
      cursor: "#c084fc",
      cursorAccent: "#09070f",
      black: "#0b0a12",
      red: "#f87171",
      green: "#4ade80",
      yellow: "#facc15",
      blue: "#60a5fa",
      magenta: "#c084fc",
      cyan: "#22d3ee",
      white: "#f1ecff",
      brightBlack: "#4b4a5a",
      brightRed: "#fda4af",
      brightGreen: "#86efac",
      brightYellow: "#fde047",
      brightBlue: "#93c5fd",
      brightMagenta: "#d8b4fe",
      brightCyan: "#67e8f9",
      brightWhite: "#ffffff",
      selectionBackground: "rgba(192,132,252,0.28)",
    };
  }

  function createTerminal(el, options = {}) {
    if (!hasXterm() || !el) {
      return null;
    }

    const id = nextId();
    const term = new Terminal({
      convertEol: true,
      cursorBlink: options.cursorBlink ?? true,
      cursorStyle: options.cursorStyle ?? "block",
      fontFamily:
        options.fontFamily ??
        '"Iosevka Term","JetBrains Mono","SF Mono","Menlo",monospace',
      fontSize: options.fontSize ?? 12,
      lineHeight: options.lineHeight ?? 1.02,
      letterSpacing: options.letterSpacing ?? 0.15,
      allowTransparency: true,
      scrollback: options.scrollback ?? 5000,
      macOptionIsMeta: true,
      theme: options.theme ?? defaultTheme(),
    });

    const fitAddon = new FitAddon.FitAddon();
    term.loadAddon(fitAddon);
    term.open(el);
    fitAddon.fit();
    term.focus();

    const ro = new ResizeObserver(() => {
      try {
        fitAddon.fit();
      } catch (_) {}
    });
    ro.observe(el);

    terminals.set(id, {
      term,
      fitAddon,
      resizeObserver: ro,
      disposeData: null,
      disposeResize: null,
    });
    return id;
  }

  function getEntry(id) {
    return terminals.get(id) ?? null;
  }

  function attachOnData(id, cb) {
    const entry = getEntry(id);
    if (!entry || typeof cb !== "function") return false;
    if (entry.disposeData) {
      entry.disposeData.dispose();
    }
    entry.disposeData = entry.term.onData((data) => cb(data));
    return true;
  }

  function attachOnResize(id, cb) {
    const entry = getEntry(id);
    if (!entry || typeof cb !== "function") return false;
    if (entry.disposeResize) {
      entry.disposeResize.dispose();
    }
    entry.disposeResize = entry.term.onResize(({ cols, rows }) => cb(cols, rows));
    return true;
  }

  function write(id, text) {
    const entry = getEntry(id);
    if (!entry) return false;
    entry.term.write(text ?? "");
    return true;
  }

  function focus(id) {
    const entry = getEntry(id);
    if (!entry) return false;
    entry.term.focus();
    return true;
  }

  function fit(id) {
    const entry = getEntry(id);
    if (!entry) return null;
    try {
      entry.fitAddon.fit();
      return { cols: entry.term.cols, rows: entry.term.rows };
    } catch (_) {
      return null;
    }
  }

  function resize(id, cols, rows) {
    const entry = getEntry(id);
    if (!entry) return false;
    entry.term.resize(cols, rows);
    return true;
  }

  function dispose(id) {
    const entry = getEntry(id);
    if (!entry) return false;
    try {
      if (entry.disposeData) entry.disposeData.dispose();
      if (entry.disposeResize) entry.disposeResize.dispose();
      entry.resizeObserver.disconnect();
      entry.term.dispose();
    } finally {
      terminals.delete(id);
    }
    return true;
  }

  window.tundraCreateTerminal = createTerminal;
  window.tundraAttachOnData = attachOnData;
  window.tundraAttachOnResize = attachOnResize;
  window.tundraWriteTerminal = write;
  window.tundraFocusTerminal = focus;
  window.tundraFitTerminal = fit;
  window.tundraResizeTerminal = resize;
  window.tundraDisposeTerminal = dispose;
})();
