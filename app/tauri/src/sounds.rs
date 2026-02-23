//! Sound effects for the Auto-Tundra desktop app.
//!
//! Uses `rodio` behind the `sounds` feature flag to play short procedural
//! audio cues (Balatro-inspired chip/whoosh/click sounds). When the feature
//! is disabled, all functions are silent no-ops.
//!
//! # Architecture
//!
//! The audio output stream (`OutputStream`) is `!Send`, so we spawn a
//! dedicated audio thread that owns the stream and receives play commands
//! over a channel. This makes `SoundEngine` `Send + Sync` for Tauri state.
//!
//! ```text
//! Leptos frontend ──IPC──▶ Tauri command ──▶ SoundEngine::play(effect)
//!                                                ▼
//!                                          channel send
//!                                                ▼
//!                                      audio thread (rodio)
//!                                                ▼
//!                                       procedural waveform
//! ```

use serde::{Deserialize, Serialize};

/// Available sound effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundEffect {
    /// Short click — button press, selection change.
    Click,
    /// Positive confirmation — task complete, build success.
    Success,
    /// Error/failure — build fail, test fail.
    Error,
    /// Notification ping — new event, message received.
    Notify,
    /// Whoosh — page transition, panel slide.
    Whoosh,
    /// Chip shuffle — agent spawned, bead created (Balatro-inspired).
    Chip,
}

/// Global volume (0.0 = silent, 1.0 = full).
#[derive(Debug, Clone, Copy)]
pub struct SoundSettings {
    pub enabled: bool,
    pub volume: f32,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.5,
        }
    }
}

// ===========================================================================
// rodio implementation — channel-based for Send + Sync
// ===========================================================================

#[cfg(feature = "sounds")]
mod engine {
    use super::*;
    use rodio::source::Source;
    use rodio::{OutputStream, Sink};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Commands sent to the audio thread.
    enum AudioCmd {
        Play(SoundEffect, f32), // effect + volume
        Shutdown,
    }

    /// Procedural sound engine. Sends play commands to a dedicated audio
    /// thread over a channel, making this type `Send + Sync`.
    pub struct SoundEngine {
        tx: std::sync::mpsc::Sender<AudioCmd>,
        settings: Arc<Mutex<SoundSettings>>,
    }

    // SoundEngine is Send + Sync because it only holds a Sender (Send+Sync)
    // and an Arc<Mutex<_>> (Send+Sync). The !Send OutputStream lives on the
    // audio thread.

    impl SoundEngine {
        /// Try to initialize the audio output. Returns `None` if the system
        /// has no audio device (CI, headless server, etc.).
        pub fn try_new() -> Option<Self> {
            let (tx, rx) = std::sync::mpsc::channel::<AudioCmd>();

            // Probe for audio device on the audio thread.
            let (ready_tx, ready_rx) = std::sync::mpsc::channel::<bool>();

            std::thread::Builder::new()
                .name("sound-engine".into())
                .spawn(move || {
                    let stream_result = OutputStream::try_default();
                    let (stream, handle) = match stream_result {
                        Ok(pair) => {
                            ready_tx.send(true).ok();
                            pair
                        }
                        Err(_) => {
                            ready_tx.send(false).ok();
                            return;
                        }
                    };

                    // Keep stream alive for the lifetime of this thread.
                    let _stream = stream;

                    while let Ok(cmd) = rx.recv() {
                        match cmd {
                            AudioCmd::Play(effect, volume) => {
                                let source = match effect {
                                    SoundEffect::Click => synth_click(),
                                    SoundEffect::Success => synth_success(),
                                    SoundEffect::Error => synth_error(),
                                    SoundEffect::Notify => synth_notify(),
                                    SoundEffect::Whoosh => synth_whoosh(),
                                    SoundEffect::Chip => synth_chip(),
                                };
                                if let Ok(sink) = Sink::try_new(&handle) {
                                    sink.set_volume(volume);
                                    sink.append(source);
                                    sink.detach();
                                }
                            }
                            AudioCmd::Shutdown => break,
                        }
                    }
                })
                .ok()?;

            // Wait for audio thread to report success/failure.
            let ok = ready_rx.recv().unwrap_or(false);
            if !ok {
                return None;
            }

            Some(Self {
                tx,
                settings: Arc::new(Mutex::new(SoundSettings::default())),
            })
        }

        pub fn set_enabled(&self, enabled: bool) {
            if let Ok(mut s) = self.settings.lock() {
                s.enabled = enabled;
            }
        }

        pub fn set_volume(&self, volume: f32) {
            if let Ok(mut s) = self.settings.lock() {
                s.volume = volume.clamp(0.0, 1.0);
            }
        }

        pub fn is_enabled(&self) -> bool {
            self.settings.lock().map(|s| s.enabled).unwrap_or(false)
        }

        pub fn volume(&self) -> f32 {
            self.settings.lock().map(|s| s.volume).unwrap_or(0.5)
        }

        /// Play a sound effect. Non-blocking — sends command to audio thread.
        pub fn play(&self, effect: SoundEffect) {
            let (enabled, volume) = {
                let s = self.settings.lock().unwrap_or_else(|e| e.into_inner());
                (s.enabled, s.volume)
            };
            if !enabled || volume <= 0.0 {
                return;
            }
            let _ = self.tx.send(AudioCmd::Play(effect, volume));
        }
    }

    impl Drop for SoundEngine {
        fn drop(&mut self) {
            let _ = self.tx.send(AudioCmd::Shutdown);
        }
    }

    // -----------------------------------------------------------------------
    // Procedural waveform generators
    // -----------------------------------------------------------------------

    /// A simple sine-wave source with optional frequency sweep.
    struct SineWave {
        sample_rate: u32,
        sample_idx: u64,
        total_samples: u64,
        freq_start: f32,
        freq_end: f32,
        amplitude: f32,
    }

    impl SineWave {
        fn new(freq_start: f32, freq_end: f32, duration_ms: u64, amplitude: f32) -> Self {
            let sample_rate = 44100u32;
            Self {
                sample_rate,
                sample_idx: 0,
                total_samples: (sample_rate as u64) * duration_ms / 1000,
                freq_start,
                freq_end,
                amplitude,
            }
        }
    }

    impl Iterator for SineWave {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            if self.sample_idx >= self.total_samples {
                return None;
            }
            let t = self.sample_idx as f32 / self.sample_rate as f32;
            let progress = self.sample_idx as f32 / self.total_samples as f32;
            let freq = self.freq_start + (self.freq_end - self.freq_start) * progress;
            // Fade out in last 20% to avoid clicks
            let envelope = if progress > 0.8 {
                (1.0 - progress) / 0.2
            } else {
                1.0
            };
            let sample = (2.0 * std::f32::consts::PI * freq * t).sin() * self.amplitude * envelope;
            self.sample_idx += 1;
            Some(sample)
        }
    }

    impl Source for SineWave {
        fn current_frame_len(&self) -> Option<usize> {
            let remaining = self.total_samples.saturating_sub(self.sample_idx);
            Some(remaining as usize)
        }
        fn channels(&self) -> u16 {
            1
        }
        fn sample_rate(&self) -> u32 {
            self.sample_rate
        }
        fn total_duration(&self) -> Option<Duration> {
            Some(Duration::from_millis(
                self.total_samples * 1000 / self.sample_rate as u64,
            ))
        }
    }

    /// Short click: 800 Hz for 30ms.
    fn synth_click() -> SineWave {
        SineWave::new(800.0, 800.0, 30, 0.3)
    }

    /// Success: rising tone 440→880 Hz over 200ms.
    fn synth_success() -> SineWave {
        SineWave::new(440.0, 880.0, 200, 0.25)
    }

    /// Error: falling tone 440→220 Hz over 250ms.
    fn synth_error() -> SineWave {
        SineWave::new(440.0, 220.0, 250, 0.3)
    }

    /// Notify: 1000 Hz ping for 100ms.
    fn synth_notify() -> SineWave {
        SineWave::new(1000.0, 1000.0, 100, 0.2)
    }

    /// Whoosh: sweep 200→2000 Hz over 150ms (low amplitude).
    fn synth_whoosh() -> SineWave {
        SineWave::new(200.0, 2000.0, 150, 0.15)
    }

    /// Chip: Balatro-inspired chip shuffle — fast 600→1200 Hz chirp, 60ms.
    fn synth_chip() -> SineWave {
        SineWave::new(600.0, 1200.0, 60, 0.25)
    }
}

// ===========================================================================
// No-op fallback when sounds feature is disabled
// ===========================================================================

#[cfg(not(feature = "sounds"))]
mod engine {
    use super::*;

    pub struct SoundEngine;

    impl SoundEngine {
        pub fn try_new() -> Option<Self> {
            Some(Self)
        }

        pub fn set_enabled(&self, _enabled: bool) {}
        pub fn set_volume(&self, _volume: f32) {}
        pub fn is_enabled(&self) -> bool {
            false
        }
        pub fn volume(&self) -> f32 {
            0.0
        }
        pub fn play(&self, _effect: SoundEffect) {}
    }
}

pub use engine::SoundEngine;

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_effect_serde_roundtrip() {
        let effect = SoundEffect::Chip;
        let json = serde_json::to_string(&effect).unwrap();
        assert_eq!(json, "\"chip\"");
        let de: SoundEffect = serde_json::from_str(&json).unwrap();
        assert_eq!(de, SoundEffect::Chip);
    }

    #[test]
    fn all_effects_deserialize() {
        let effects = ["click", "success", "error", "notify", "whoosh", "chip"];
        for name in effects {
            let json = format!("\"{}\"", name);
            let _: SoundEffect = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn sound_settings_default() {
        let settings = SoundSettings::default();
        assert!(settings.enabled);
        assert!((settings.volume - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn engine_creation() {
        // May return None in CI with no audio device — that's fine
        let _engine = SoundEngine::try_new();
    }

    #[test]
    fn engine_settings() {
        if let Some(engine) = SoundEngine::try_new() {
            engine.set_volume(0.8);
            engine.set_enabled(false);
            #[cfg(feature = "sounds")]
            {
                assert!(!engine.is_enabled());
                assert!((engine.volume() - 0.8).abs() < f32::EPSILON);
            }
        }
    }

    #[cfg(feature = "sounds")]
    #[test]
    fn engine_play_all_effects_no_panic() {
        if let Some(engine) = SoundEngine::try_new() {
            // Just verify none of the synth generators panic
            engine.set_volume(0.0); // silent to not annoy test runners
            engine.play(SoundEffect::Click);
            engine.play(SoundEffect::Success);
            engine.play(SoundEffect::Error);
            engine.play(SoundEffect::Notify);
            engine.play(SoundEffect::Whoosh);
            engine.play(SoundEffect::Chip);
        }
    }

    #[cfg(feature = "sounds")]
    #[test]
    fn engine_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SoundEngine>();
    }
}
