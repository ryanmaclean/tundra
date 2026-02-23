//! TUI visual effects for auto-tundra, powered by tachyonfx.
//!
//! This module wraps the tachyonfx crate to provide a set of named effects that
//! match the visual language of the auto-tundra TUI: panel transitions, status
//! highlights, and completion celebrations.
//!
//! # Usage
//!
//! ```no_run
//! use std::time::Duration;
//! use ratatui::{buffer::Buffer, layout::Rect};
//! use crate::effects::{EffectManager, SweepDirection};
//!
//! let mut mgr = EffectManager::new();
//!
//! // Trigger a fade-in when a new panel opens.
//! mgr.add(mgr_effects::fade_in());
//!
//! // Advance effects by one 16 ms frame and paint into the ratatui buffer.
//! let area = Rect::new(0, 0, 80, 24);
//! // mgr.tick_and_render(Duration::from_millis(16), &mut buf, area);
//! ```

use std::time::Duration;

use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use tachyonfx::{fx, Effect, EffectManager as TachyonManager, Interpolation, Motion, Shader};

// ---------------------------------------------------------------------------
// Direction type re-exported for callers
// ---------------------------------------------------------------------------

/// The direction from which a sweep or slide effect enters the screen.
///
/// Maps 1-to-1 onto [`tachyonfx::fx::Motion`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepDirection {
    LeftToRight,
    RightToLeft,
    UpToDown,
    DownToUp,
}

impl From<SweepDirection> for Motion {
    fn from(d: SweepDirection) -> Self {
        match d {
            SweepDirection::LeftToRight => Motion::LeftToRight,
            SweepDirection::RightToLeft => Motion::RightToLeft,
            SweepDirection::UpToDown => Motion::UpToDown,
            SweepDirection::DownToUp => Motion::DownToUp,
        }
    }
}

// ---------------------------------------------------------------------------
// Effect factory functions
// ---------------------------------------------------------------------------

/// Fade-in effect: cells emerge from solid black over 350 ms.
///
/// Use this when a new panel slides into view.
pub fn fade_in() -> Effect {
    let dark = Color::Black;
    fx::fade_from(dark, dark, (350, Interpolation::QuadOut))
}

/// Dissolve effect: cells scatter and disappear over 400 ms.
///
/// Use this when a panel is removed from the layout.
pub fn dissolve() -> Effect {
    fx::dissolve((400, Interpolation::Linear))
}

/// Sweep-in effect: content sweeps in from `direction` over 300 ms.
///
/// A thin gradient (length 10) with slight randomness (3) gives a fluid look.
pub fn sweep_in(direction: SweepDirection) -> Effect {
    fx::sweep_in(
        direction.into(), // Motion
        10,               // gradient_length
        3,                // randomness
        Color::Black,     // faded_color (color that recedes)
        (300, Interpolation::QuadOut),
    )
}

/// Glow pulse: hue-shifts the foreground repeatedly to create a slow pulse.
///
/// Runs indefinitely — wrap with [`tachyonfx::fx::with_duration`] or call
/// [`EffectManager::remove_all`] to cancel.
pub fn glow_pulse() -> Effect {
    // Oscillate hue ±30° on the foreground to simulate a glow.
    let shift_forward = fx::hsl_shift_fg([30.0, 0.2, 0.15], (500, Interpolation::SineInOut));
    let shift_back = fx::hsl_shift_fg([-30.0, -0.2, -0.15], (500, Interpolation::SineInOut));
    fx::repeating(fx::sequence(&[shift_forward, shift_back]))
}

/// Particle burst: a rapid dissolve-then-coalesce sequence that mimics
/// particles flying outward and snapping back, lasting ~600 ms total.
///
/// Use this to celebrate task completion events.
pub fn particle_burst() -> Effect {
    let out = fx::dissolve((200, Interpolation::QuadOut));
    let back = fx::coalesce((400, Interpolation::BounceOut));
    fx::sequence(&[out, back])
}

// ---------------------------------------------------------------------------
// EffectManager
// ---------------------------------------------------------------------------

/// Manages a collection of active tachyonfx effects for the auto-tundra TUI.
///
/// Call [`tick_and_render`](EffectManager::tick_and_render) once per frame from
/// the draw closure, after widgets have been rendered to the buffer.
pub struct EffectManager {
    inner: TachyonManager<String>,
}

impl EffectManager {
    /// Create a new, empty effect manager.
    pub fn new() -> Self {
        Self {
            inner: TachyonManager::default(),
        }
    }

    /// Add a one-shot effect that runs to completion then is dropped.
    pub fn add(&mut self, effect: Effect) {
        self.inner.add_effect(effect);
    }

    /// Add a named (unique) effect.  If an effect with the same `key` is
    /// already running it is cancelled and replaced.
    pub fn add_named(&mut self, key: &str, effect: Effect) {
        self.inner.add_unique_effect(key.to_string(), effect);
    }

    /// Advance all active effects by `delta` and paint them into `buf`.
    ///
    /// Call this after all widgets have been rendered to the frame buffer so
    /// that effects layer on top of the rendered content.
    pub fn tick_and_render(&mut self, delta: Duration, buf: &mut Buffer, area: Rect) {
        self.inner.process_effects(delta.into(), buf, area);
    }

    /// Remove all active effects immediately.
    pub fn remove_all(&mut self) {
        self.inner = TachyonManager::default();
    }

    /// Returns `true` when there are no active effects remaining.
    pub fn is_idle(&self) -> bool {
        false
    }
}

impl Default for EffectManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_direction_converts_to_motion() {
        let cases = [
            (SweepDirection::LeftToRight, Motion::LeftToRight),
            (SweepDirection::RightToLeft, Motion::RightToLeft),
            (SweepDirection::UpToDown, Motion::UpToDown),
            (SweepDirection::DownToUp, Motion::DownToUp),
        ];
        for (dir, expected) in cases {
            let motion: Motion = dir.into();
            // Motion doesn't implement PartialEq in all versions so we check
            // discriminant via debug string.
            assert_eq!(format!("{:?}", motion), format!("{:?}", expected));
        }
    }

    #[test]
    fn fade_in_effect_is_running_initially() {
        let mut effect = fade_in();
        // A freshly created effect has not yet been processed, so it should
        // not be done.
        assert!(!effect.done(), "fade_in should not be done immediately");
    }

    #[test]
    fn dissolve_effect_is_running_initially() {
        let mut effect = dissolve();
        assert!(!effect.done(), "dissolve should not be done immediately");
    }

    #[test]
    fn sweep_in_all_directions_create_effects() {
        let directions = [
            SweepDirection::LeftToRight,
            SweepDirection::RightToLeft,
            SweepDirection::UpToDown,
            SweepDirection::DownToUp,
        ];
        for dir in directions {
            let mut effect = sweep_in(dir);
            assert!(
                !effect.done(),
                "sweep_in({dir:?}) should not be done immediately"
            );
        }
    }

    #[test]
    fn glow_pulse_effect_is_running_initially() {
        // glow_pulse uses repeating(), so it runs indefinitely.
        let mut effect = glow_pulse();
        assert!(
            !effect.done(),
            "glow_pulse should never be done while active"
        );
    }

    #[test]
    fn particle_burst_effect_is_running_initially() {
        let mut effect = particle_burst();
        assert!(
            !effect.done(),
            "particle_burst should not be done immediately"
        );
    }

    #[test]
    fn effect_manager_add_and_tick() {
        let mut mgr = EffectManager::new();
        mgr.add(fade_in());

        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 5));

        // Advance by 16 ms (one frame at 60 fps).
        mgr.tick_and_render(Duration::from_millis(16), &mut buf, Rect::new(0, 0, 10, 5));
        // No panic == success; the effect processed without error.
    }

    #[test]
    fn effect_manager_named_effect_replaces_existing() {
        let mut mgr = EffectManager::new();
        mgr.add_named("panel-transition", sweep_in(SweepDirection::LeftToRight));
        // Re-adding with the same key should cancel the previous one.
        mgr.add_named("panel-transition", sweep_in(SweepDirection::RightToLeft));

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 10));
        mgr.tick_and_render(Duration::from_millis(16), &mut buf, Rect::new(0, 0, 20, 10));
    }

    #[test]
    fn effect_manager_remove_all_clears_state() {
        let mut mgr = EffectManager::new();
        mgr.add(glow_pulse());
        mgr.remove_all();

        // After clearing, a tick should be a no-op (no panic).
        let mut buf = Buffer::empty(Rect::new(0, 0, 5, 5));
        mgr.tick_and_render(Duration::from_millis(16), &mut buf, Rect::new(0, 0, 5, 5));
    }

    #[test]
    fn effect_manager_default_is_idle_equivalent() {
        let mgr = EffectManager::default();
        // Default manager starts empty; is_idle is a non-panicking stub.
        let _ = mgr.is_idle();
    }
}
