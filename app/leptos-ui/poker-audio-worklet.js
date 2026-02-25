class TundraPokerCueProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.voices = [];
    this.sampleRateHz = sampleRate;

    this.port.onmessage = (event) => {
      const msg = event.data || {};
      if (msg.type !== "cue") return;
      this.enqueueCue(msg.cue || "deal");
    };
  }

  enqueueCue(cue) {
    const palette = this.cuePalette(cue);
    for (const note of palette) {
      this.voices.push({
        freq: note.freq,
        amp: note.amp,
        total: Math.max(1, Math.floor(note.duration * this.sampleRateHz)),
        elapsed: 0,
        phase: 0,
      });
    }
  }

  cuePalette(cue) {
    switch (cue) {
      case "success":
        return [
          { freq: 523.25, duration: 0.16, amp: 0.08 },
          { freq: 659.25, duration: 0.18, amp: 0.07 },
          { freq: 783.99, duration: 0.22, amp: 0.06 },
        ];
      case "consensus":
        return [
          { freq: 392.0, duration: 0.12, amp: 0.07 },
          { freq: 493.88, duration: 0.14, amp: 0.07 },
          { freq: 587.33, duration: 0.16, amp: 0.06 },
          { freq: 783.99, duration: 0.2, amp: 0.05 },
        ];
      case "error":
        return [
          { freq: 246.94, duration: 0.16, amp: 0.08 },
          { freq: 220.0, duration: 0.18, amp: 0.07 },
          { freq: 174.61, duration: 0.2, amp: 0.06 },
        ];
      case "deal":
      default:
        return [
          { freq: 440.0, duration: 0.08, amp: 0.06 },
          { freq: 554.37, duration: 0.1, amp: 0.055 },
          { freq: 659.25, duration: 0.12, amp: 0.05 },
        ];
    }
  }

  voiceSample(voice) {
    const progress = voice.elapsed / voice.total;
    const attack = Math.min(1.0, progress / 0.1);
    const decay = 1.0 - Math.min(1.0, progress);
    const envelope = attack * decay;
    const sample = Math.sin(voice.phase) * voice.amp * envelope;
    voice.phase += (2 * Math.PI * voice.freq) / this.sampleRateHz;
    voice.elapsed += 1;
    return sample;
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    if (!out || out.length === 0) return true;
    const frames = out[0].length;

    for (let i = 0; i < frames; i += 1) {
      let mixed = 0;
      for (let v = 0; v < this.voices.length; v += 1) {
        mixed += this.voiceSample(this.voices[v]);
      }
      mixed = Math.max(-0.4, Math.min(0.4, mixed));
      for (let ch = 0; ch < out.length; ch += 1) {
        out[ch][i] = mixed;
      }
    }

    this.voices = this.voices.filter((voice) => voice.elapsed < voice.total);
    return true;
  }
}

registerProcessor("tundra-poker-cue", TundraPokerCueProcessor);
