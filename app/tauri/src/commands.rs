use tauri::State;

use crate::sounds::{SoundEffect, SoundEngine};
use crate::state::AppState;

/// Return the dynamically-assigned API port so the frontend can
/// discover it via Tauri IPC as a fallback to the init script.
#[tauri::command]
pub fn cmd_get_api_port(state: State<'_, AppState>) -> u16 {
    state.api_port
}

/// Play a sound effect. Accepts: click, success, error, notify, whoosh, chip.
#[tauri::command]
pub fn cmd_play_sound(engine: State<'_, Option<SoundEngine>>, effect: SoundEffect) {
    if let Some(e) = engine.inner().as_ref() {
        e.play(effect);
    }
}

/// Enable or disable sound effects.
#[tauri::command]
pub fn cmd_set_sound_enabled(engine: State<'_, Option<SoundEngine>>, enabled: bool) {
    if let Some(e) = engine.inner().as_ref() {
        e.set_enabled(enabled);
    }
}

/// Set sound volume (0.0–1.0).
#[tauri::command]
pub fn cmd_set_sound_volume(engine: State<'_, Option<SoundEngine>>, volume: f32) {
    if let Some(e) = engine.inner().as_ref() {
        e.set_volume(volume);
    }
}

/// Get current sound settings.
#[tauri::command]
pub fn cmd_get_sound_settings(engine: State<'_, Option<SoundEngine>>) -> (bool, f32) {
    match engine.inner().as_ref() {
        Some(e) => (e.is_enabled(), e.volume()),
        None => (false, 0.0),
    }
}
