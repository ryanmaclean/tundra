use tauri::State;
use uuid::Uuid;

use at_core::types::{Bead, BeadStatus, Lane};

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

// ---------------------------------------------------------------------------
// Bead management commands
// ---------------------------------------------------------------------------

/// List all beads, optionally filtered by status.
#[tauri::command]
pub async fn cmd_list_beads(
    state: State<'_, AppState>,
    status: Option<BeadStatus>,
) -> Result<Vec<Bead>, String> {
    let beads = state.daemon.api_state().beads.read().await;
    let filtered: Vec<Bead> = match status {
        Some(s) => beads
            .values()
            .filter(|b| b.status == s)
            .cloned()
            .collect(),
        None => beads.values().cloned().collect(),
    };
    Ok(filtered)
}

/// Create a new bead with the given title, description, lane, and tags.
#[tauri::command]
pub async fn cmd_create_bead(
    state: State<'_, AppState>,
    title: String,
    description: Option<String>,
    lane: Option<Lane>,
    tags: Option<Vec<String>>,
) -> Result<Bead, String> {
    // Validate title
    if title.trim().is_empty() {
        return Err("title cannot be empty".to_string());
    }
    if title.len() > 1000 {
        return Err("title too long (max 1000 characters)".to_string());
    }

    // Validate description if present
    if let Some(ref desc) = description {
        if desc.len() > 10000 {
            return Err("description too long (max 10000 characters)".to_string());
        }
    }

    let lane = lane.unwrap_or(Lane::Standard);
    let mut bead = Bead::new(title, lane);
    bead.description = description;
    if let Some(tags) = tags {
        bead.metadata = Some(serde_json::json!({ "tags": tags }));
    }

    let mut beads = state.daemon.api_state().beads.write().await;
    beads.insert(bead.id, bead.clone());

    // Publish event
    state
        .daemon
        .event_bus()
        .publish(at_bridge::protocol::BridgeMessage::BeadCreated(
            bead.clone(),
        ));

    Ok(bead)
}

/// Update a bead's status by ID.
#[tauri::command]
pub async fn cmd_update_bead_status(
    state: State<'_, AppState>,
    id: String,
    status: BeadStatus,
) -> Result<Bead, String> {
    let bead_id = Uuid::parse_str(&id).map_err(|e| format!("invalid UUID: {}", e))?;

    let mut beads = state.daemon.api_state().beads.write().await;
    let bead = beads
        .get_mut(&bead_id)
        .ok_or_else(|| "bead not found".to_string())?;

    if !bead.status.can_transition_to(&status) {
        return Err(format!(
            "invalid transition from {:?} to {:?}",
            bead.status, status
        ));
    }

    bead.status = status;
    bead.updated_at = chrono::Utc::now();

    let bead_snapshot = bead.clone();

    // Publish event
    state
        .daemon
        .event_bus()
        .publish(at_bridge::protocol::BridgeMessage::BeadUpdated(
            bead_snapshot.clone(),
        ));

    Ok(bead_snapshot)
}

/// Delete a bead by ID.
#[tauri::command]
pub async fn cmd_delete_bead(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let bead_id = Uuid::parse_str(&id).map_err(|e| format!("invalid UUID: {}", e))?;

    let mut beads = state.daemon.api_state().beads.write().await;
    if beads.remove(&bead_id).is_none() {
        return Err("bead not found".to_string());
    }

    // Publish updated bead list event
    state
        .daemon
        .event_bus()
        .publish(at_bridge::protocol::BridgeMessage::BeadList(
            beads.values().cloned().collect(),
        ));

    Ok(bead_id.to_string())
}
