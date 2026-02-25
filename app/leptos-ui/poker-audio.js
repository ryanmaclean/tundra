(function () {
  "use strict";

  const MAX_QUEUE = 128;
  let audioCtx = null;
  let workletNode = null;
  let initPromise = null;
  const queuedCues = [];

  function jsonResult(payload) {
    try {
      return JSON.stringify(payload);
    } catch (_err) {
      return JSON.stringify({ ok: false, error: "serialize failed" });
    }
  }

  function workletUrl() {
    return new URL("poker-audio-worklet.js", window.location.href).toString();
  }

  async function ensureInit() {
    if (workletNode) return;
    if (initPromise) {
      await initPromise;
      return;
    }

    initPromise = (async () => {
      const AudioContextCtor = window.AudioContext || window.webkitAudioContext;
      if (!AudioContextCtor) {
        throw new Error("AudioContext not supported");
      }
      audioCtx = new AudioContextCtor({ latencyHint: "interactive" });
      await audioCtx.audioWorklet.addModule(workletUrl());
      workletNode = new AudioWorkletNode(audioCtx, "tundra-poker-cue", {
        numberOfInputs: 0,
        numberOfOutputs: 1,
        outputChannelCount: [2],
      });
      workletNode.connect(audioCtx.destination);
    })();

    try {
      await initPromise;
      initPromise = null;
    } catch (err) {
      initPromise = null;
      throw err;
    }
  }

  async function warmup() {
    try {
      await ensureInit();
      if (audioCtx && audioCtx.state !== "running") {
        await audioCtx.resume();
      }
      return jsonResult({ ok: true, state: audioCtx ? audioCtx.state : "unknown" });
    } catch (err) {
      return jsonResult({ ok: false, error: String(err) });
    }
  }

  function enqueue(cue) {
    if (queuedCues.length >= MAX_QUEUE) {
      queuedCues.shift();
    }
    queuedCues.push(cue);
  }

  function drainQueue() {
    if (!workletNode) return;
    while (queuedCues.length > 0) {
      const cue = queuedCues.shift();
      workletNode.port.postMessage({ type: "cue", cue });
    }
  }

  async function play_cue(cueName) {
    const cue = typeof cueName === "string" && cueName.length > 0 ? cueName : "deal";
    enqueue(cue);

    try {
      await ensureInit();
      if (audioCtx && audioCtx.state !== "running") {
        await audioCtx.resume();
      }
      drainQueue();
      return jsonResult({ ok: true, cue });
    } catch (err) {
      return jsonResult({ ok: false, cue, error: String(err) });
    }
  }

  globalThis.pokerAudio = globalThis.pokerAudio || {};
  Object.assign(globalThis.pokerAudio, {
    warmup,
    play_cue,
  });
})();
