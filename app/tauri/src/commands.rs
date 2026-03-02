use tauri::State;
use uuid::Uuid;

use at_core::types::{Agent, Bead, BeadStatus, Lane};

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

// ---------------------------------------------------------------------------
// Agent management commands
// ---------------------------------------------------------------------------

/// List all agents registered in the system.
///
/// Returns all agents with their current status, role, CLI type, process information,
/// and metadata. Agents represent autonomous workers that execute tasks (e.g., coder,
/// QA, fixer roles).
#[tauri::command]
pub async fn cmd_list_agents(state: State<'_, AppState>) -> Result<Vec<Agent>, String> {
    let agents = state.daemon.api_state().agents.read().await;
    Ok(agents.values().cloned().collect())
}

/// Get a specific agent by ID.
///
/// Returns the agent's current status and metadata, or an error if not found.
///
/// # Arguments
/// * `id` - UUID string of the agent to retrieve
#[tauri::command]
pub async fn cmd_get_agent(state: State<'_, AppState>, id: String) -> Result<Agent, String> {
    let agent_id = Uuid::parse_str(&id).map_err(|e| format!("invalid UUID: {}", e))?;

    let agents = state.daemon.api_state().agents.read().await;
    let agent = agents
        .get(&agent_id)
        .ok_or_else(|| "agent not found".to_string())?;

    Ok(agent.clone())
}

// ---------------------------------------------------------------------------
// Worktree management commands
// ---------------------------------------------------------------------------

/// Represents a git worktree entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorktreeEntry {
    /// Stable identifier derived from path or branch name
    pub id: String,
    /// Absolute filesystem path to the worktree
    pub path: String,
    /// Git branch name (empty for detached HEAD)
    pub branch: String,
    /// Associated bead ID (currently unused, reserved for future)
    pub bead_id: String,
    /// Worktree status ("active" for all current worktrees)
    pub status: String,
}

/// Generates a stable, filesystem-safe identifier for a worktree.
fn stable_worktree_id(path: &str, branch: &str) -> String {
    let raw = if branch.is_empty() {
        format!("path:{path}")
    } else {
        format!("branch:{branch}")
    };
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// List all git worktrees with path and branch info.
#[tauri::command]
pub async fn cmd_list_worktrees(
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<WorktreeEntry>, String> {
    let output = tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut current_path = String::new();
    let mut current_branch = String::new();

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = path.to_string();
            current_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.is_empty() && !current_path.is_empty() {
            worktrees.push(WorktreeEntry {
                id: stable_worktree_id(&current_path, &current_branch),
                path: current_path.clone(),
                branch: current_branch.clone(),
                bead_id: String::new(),
                status: "active".into(),
            });
            current_path = String::new();
            current_branch = String::new();
        }
    }
    // Handle last entry if stdout doesn't end with empty line
    if !current_path.is_empty() {
        worktrees.push(WorktreeEntry {
            id: stable_worktree_id(&current_path, &current_branch),
            path: current_path,
            branch: current_branch,
            bead_id: String::new(),
            status: "active".into(),
        });
    }

    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);

    let paginated: Vec<WorktreeEntry> = worktrees.into_iter().skip(offset).take(limit).collect();

    Ok(paginated)
}

/// Create a new git worktree with the given path and branch name.
#[tauri::command]
pub async fn cmd_create_worktree(
    path: String,
    branch: String,
) -> Result<WorktreeEntry, String> {
    // Validate inputs
    if path.trim().is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if branch.trim().is_empty() {
        return Err("branch cannot be empty".to_string());
    }

    let output = tokio::process::Command::new("git")
        .args(["worktree", "add", "-b", &branch, &path])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(stderr);
    }

    Ok(WorktreeEntry {
        id: stable_worktree_id(&path, &branch),
        path,
        branch,
        bead_id: String::new(),
        status: "active".into(),
    })
}

/// Delete a git worktree by ID.
#[tauri::command]
pub async fn cmd_delete_worktree(id: String) -> Result<String, String> {
    let output = tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut current_path = String::new();
    let mut current_branch = String::new();
    let mut found_path: Option<String> = None;

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = path.to_string();
            current_branch = String::new();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = branch.to_string();
        } else if line.is_empty() && !current_path.is_empty() {
            let candidate_id = stable_worktree_id(&current_path, &current_branch);
            if candidate_id == id || current_branch.contains(&id) || current_path.contains(&id) {
                found_path = Some(current_path.clone());
                break;
            }
            current_path.clear();
            current_branch.clear();
        }
    }
    if found_path.is_none() && !current_path.is_empty() {
        let candidate_id = stable_worktree_id(&current_path, &current_branch);
        if candidate_id == id || current_branch.contains(&id) || current_path.contains(&id) {
            found_path = Some(current_path);
        }
    }

    let Some(path) = found_path else {
        return Err("worktree not found".to_string());
    };

    let rm = tokio::process::Command::new("git")
        .args(["worktree", "remove", "--force", &path])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !rm.status.success() {
        let stderr = String::from_utf8_lossy(&rm.stderr).to_string();
        return Err(stderr);
    }

    Ok(id)
}
