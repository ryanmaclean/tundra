use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use super::state::ApiState;
use super::types::{
    KanbanColumnConfig, PlanningPokerPhase, PlanningPokerRevealStats, PlanningPokerSession,
    PlanningPokerSessionResponse, PlanningPokerVote, PlanningPokerVoteView,
    RevealPlanningPokerRequest, SimulatePlanningPokerRequest, StartPlanningPokerRequest,
    SubmitPlanningPokerVoteRequest,
};
use crate::api_error::ApiError;

/// GET /api/kanban/columns -- return the 8-column Kanban config (order, labels, optional width).
pub(crate) async fn get_kanban_columns(
    State(state): State<Arc<ApiState>>,
) -> Json<KanbanColumnConfig> {
    let cols = state.kanban_columns.read().await;
    Json(cols.clone())
}

/// PATCH /api/kanban/columns -- update column config (e.g. order, labels, width_px).
pub(crate) async fn patch_kanban_columns(
    State(state): State<Arc<ApiState>>,
    Json(patch): Json<KanbanColumnConfig>,
) -> Result<impl IntoResponse, ApiError> {
    let mut cols = state.kanban_columns.write().await;
    if patch.columns.is_empty() {
        return Err(ApiError::BadRequest("columns must not be empty".into()));
    }
    *cols = patch;
    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::to_value(cols.clone()).map_err(|e| ApiError::Internal(e.to_string()))?),
    ))
}

fn normalize_participants(raw: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for item in raw {
        let name = item.trim();
        if name.is_empty() {
            continue;
        }
        if seen.insert(name.to_string()) {
            out.push(name.to_string());
        }
    }
    out
}

fn normalize_cards(raw: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for item in raw {
        let card = item.trim();
        if card.is_empty() {
            continue;
        }
        if seen.insert(card.to_string()) {
            out.push(card.to_string());
        }
    }
    out
}

fn default_poker_deck() -> Vec<String> {
    [
        "0", "1", "2", "3", "5", "8", "13", "21", "34", "55", "89", "?", "coffee",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn deck_preset(preset: &str) -> Option<Vec<String>> {
    let key = preset.trim().to_ascii_lowercase();
    match key.as_str() {
        "fibonacci" => Some(
            [
                "0", "1", "2", "3", "5", "8", "13", "21", "34", "55", "89", "?", "coffee",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        ),
        "modified_fibonacci" => Some(
            [
                "0", "1", "2", "3", "5", "8", "13", "20", "40", "100", "?", "coffee",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        ),
        "powers_of_two" => Some(
            ["1", "2", "4", "8", "16", "32", "64", "128", "?", "coffee"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ),
        "tshirt" => Some(
            ["xs", "s", "m", "l", "xl", "xxl", "?", "coffee"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ),
        _ => None,
    }
}

fn resolve_poker_deck(
    poker_cfg: &at_core::config::PlanningPokerConfig,
    deck_preset_name: Option<&str>,
    custom_deck: Option<&[String]>,
) -> Result<Vec<String>, (axum::http::StatusCode, serde_json::Value)> {
    if let Some(custom) = custom_deck {
        if !poker_cfg.allow_custom_deck {
            return Err((
                axum::http::StatusCode::FORBIDDEN,
                serde_json::json!({"error": "custom deck is disabled by settings"}),
            ));
        }
        let normalized = normalize_cards(custom);
        if normalized.is_empty() {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "custom_deck must include at least one card"}),
            ));
        }
        return Ok(normalized);
    }

    if let Some(preset) = deck_preset_name {
        return match deck_preset(preset) {
            Some(deck) => Ok(deck),
            None => Err((
                axum::http::StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "unknown deck preset"}),
            )),
        };
    }

    if let Some(deck) = deck_preset(&poker_cfg.default_deck) {
        Ok(deck)
    } else {
        Ok(default_poker_deck())
    }
}

fn default_virtual_agent_name(index: usize) -> String {
    const NAMES: [&str; 8] = [
        "Planner",
        "Architect",
        "Coder",
        "Reviewer",
        "QA",
        "DevOps",
        "SRE",
        "Product",
    ];
    if index < NAMES.len() {
        NAMES[index].to_string()
    } else {
        format!("Agent {}", index + 1)
    }
}

fn poker_focus_card_from_priority(priority: i32) -> &'static str {
    match priority {
        i32::MIN..=0 => "2",
        1..=2 => "3",
        3..=4 => "5",
        5..=7 => "8",
        _ => "13",
    }
}

fn estimation_cards(deck: &[String]) -> Vec<String> {
    let filtered = deck
        .iter()
        .filter(|card| {
            let c = card.trim().to_ascii_lowercase();
            c != "?" && c != "coffee" && c != "\u{2615}"
        })
        .cloned()
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        deck.to_vec()
    } else {
        filtered
    }
}

fn nearest_card_index(cards: &[String], target: &str) -> usize {
    if cards.is_empty() {
        return 0;
    }

    if let Some(index) = cards
        .iter()
        .position(|card| card.eq_ignore_ascii_case(target.trim()))
    {
        return index;
    }

    if let Ok(target_num) = target.trim().parse::<f64>() {
        if let Some((idx, _)) = cards
            .iter()
            .enumerate()
            .filter_map(|(idx, card)| card.trim().parse::<f64>().ok().map(|n| (idx, n)))
            .min_by(|(_, a), (_, b)| {
                (a - target_num)
                    .abs()
                    .partial_cmp(&(b - target_num).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            return idx;
        }
    }

    cards.len() / 2
}

fn parse_numeric_card(card: &str) -> Option<f64> {
    card.trim().parse::<f64>().ok()
}

fn reveal_stats(votes: &[PlanningPokerVote]) -> Option<PlanningPokerRevealStats> {
    let mut numbers = votes
        .iter()
        .filter_map(|v| parse_numeric_card(&v.card))
        .collect::<Vec<_>>();
    if numbers.is_empty() {
        return None;
    }
    numbers.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum = numbers.iter().sum::<f64>();
    let min = numbers.first().copied();
    let max = numbers.last().copied();
    let average = Some(sum / numbers.len() as f64);
    let median = if numbers.len() % 2 == 1 {
        Some(numbers[numbers.len() / 2])
    } else {
        let a = numbers[(numbers.len() / 2) - 1];
        let b = numbers[numbers.len() / 2];
        Some((a + b) / 2.0)
    };

    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for n in &numbers {
        *counts.entry(n.to_string()).or_insert(0) += 1;
    }
    let mut best: Option<(f64, usize)> = None;
    let mut tie = false;
    for (key, count) in counts {
        let parsed = key.parse::<f64>().ok();
        if parsed.is_none() {
            continue;
        }
        match best {
            None => {
                best = Some((parsed.unwrap_or_default(), count));
                tie = false;
            }
            Some((_, best_count)) if count > best_count => {
                best = Some((parsed.unwrap_or_default(), count));
                tie = false;
            }
            Some((_, best_count)) if count == best_count => {
                tie = true;
            }
            _ => {}
        }
    }
    let mode = if tie {
        None
    } else {
        best.map(|(value, _)| value)
    };

    Some(PlanningPokerRevealStats {
        min,
        max,
        average,
        median,
        mode,
        numeric_vote_count: numbers.len(),
    })
}

fn planning_poker_response(session: &PlanningPokerSession) -> PlanningPokerSessionResponse {
    let mut voted = std::collections::HashMap::<String, String>::new();
    for vote in &session.votes {
        voted.insert(vote.voter.clone(), vote.card.clone());
    }

    let mut participants = session.participants.clone();
    for vote in &session.votes {
        if !participants.iter().any(|p| p == &vote.voter) {
            participants.push(vote.voter.clone());
        }
    }
    participants.sort_by(|a, b| {
        let a_voted = voted.contains_key(a);
        let b_voted = voted.contains_key(b);
        match (a_voted, b_voted) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        }
    });

    let revealed = matches!(session.phase, PlanningPokerPhase::Revealed);
    let votes = participants
        .iter()
        .map(|participant| {
            let card = voted.get(participant).cloned();
            PlanningPokerVoteView {
                voter: participant.clone(),
                has_voted: card.is_some(),
                card: if revealed { card } else { None },
            }
        })
        .collect::<Vec<_>>();

    PlanningPokerSessionResponse {
        bead_id: session.bead_id,
        phase: session.phase,
        revealed,
        deck: session.deck.clone(),
        round_duration_seconds: session.round_duration_seconds,
        vote_count: session.votes.len(),
        votes,
        consensus_card: session.consensus_card.clone(),
        stats: if revealed {
            reveal_stats(&session.votes)
        } else {
            None
        },
        started_at: session.started_at,
        updated_at: session.updated_at,
    }
}

fn consensus_card_from_votes(votes: &[PlanningPokerVote]) -> Option<String> {
    if votes.is_empty() {
        return None;
    }
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for vote in votes {
        *counts.entry(vote.card.clone()).or_insert(0) += 1;
    }

    let mut best: Option<(String, usize)> = None;
    let mut tie = false;
    for (card, count) in counts {
        match &best {
            None => {
                best = Some((card, count));
                tie = false;
            }
            Some((_, best_count)) if count > *best_count => {
                best = Some((card, count));
                tie = false;
            }
            Some((_, best_count)) if count == *best_count => {
                tie = true;
            }
            _ => {}
        }
    }

    if tie {
        None
    } else {
        best.map(|(card, _)| card)
    }
}

/// Simulate a planning poker session for a bead (used by intelligence_api).
pub(crate) async fn simulate_planning_poker_for_bead(
    state: &Arc<ApiState>,
    req: SimulatePlanningPokerRequest,
) -> Result<PlanningPokerSessionResponse, (axum::http::StatusCode, serde_json::Value)> {
    let cfg = state.settings_manager.load_or_default();
    let poker_cfg = &cfg.kanban.planning_poker;
    if !poker_cfg.enabled {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            serde_json::json!({"error": "planning poker is disabled by settings"}),
        ));
    }

    let bead = {
        let beads = state.beads.read().await;
        beads.get(&req.bead_id).cloned()
    }
    .ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            serde_json::json!({"error": "bead not found"}),
        )
    })?;

    let deck = resolve_poker_deck(
        poker_cfg,
        req.deck_preset.as_deref(),
        req.custom_deck.as_deref(),
    )?;

    let mut participants = normalize_participants(&req.virtual_agents);
    let desired_count = req.agent_count.unwrap_or({
        if participants.is_empty() {
            5
        } else {
            participants.len()
        }
    });

    if desired_count == 0 || desired_count > 64 {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            serde_json::json!({"error": "agent_count must be between 1 and 64"}),
        ));
    }

    if participants.len() > desired_count {
        participants.truncate(desired_count);
    }
    while participants.len() < desired_count {
        participants.push(default_virtual_agent_name(participants.len()));
    }

    let estimate_deck = estimation_cards(&deck);
    let focus_card = req
        .focus_card
        .unwrap_or_else(|| poker_focus_card_from_priority(bead.priority).to_string());
    let base_idx = nearest_card_index(&estimate_deck, &focus_card);

    let mut votes = Vec::with_capacity(participants.len());
    let seed = req.seed.unwrap_or_else(|| {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        req.bead_id.hash(&mut hasher);
        bead.title.hash(&mut hasher);
        hasher.finish()
    });

    for (index, voter) in participants.iter().enumerate() {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        seed.hash(&mut hasher);
        req.bead_id.hash(&mut hasher);
        index.hash(&mut hasher);
        voter.hash(&mut hasher);
        let jitter = (hasher.finish() % 3) as i32 - 1; // -1, 0, +1
        let card_idx = (base_idx as i32 + jitter).clamp(0, estimate_deck.len() as i32 - 1) as usize;
        votes.push(PlanningPokerVote {
            voter: voter.clone(),
            card: estimate_deck[card_idx].clone(),
        });
    }

    let now = chrono::Utc::now();
    let mut session = PlanningPokerSession {
        bead_id: req.bead_id,
        phase: PlanningPokerPhase::Voting,
        votes,
        participants,
        deck,
        round_duration_seconds: req
            .round_duration_seconds
            .or(Some(poker_cfg.round_duration_seconds)),
        consensus_card: None,
        started_at: now,
        updated_at: now,
    };

    if req.auto_reveal {
        session.phase = PlanningPokerPhase::Revealed;
        session.consensus_card = consensus_card_from_votes(&session.votes);
    }

    let response = planning_poker_response(&session);
    state
        .planning_poker_sessions
        .write()
        .await
        .insert(req.bead_id, session);

    Ok(response)
}

/// POST /api/kanban/poker/start -- start a planning poker session for a bead.
pub(crate) async fn start_planning_poker(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<StartPlanningPokerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let cfg = state.settings_manager.load_or_default();
    let poker_cfg = &cfg.kanban.planning_poker;
    if !poker_cfg.enabled {
        return Err(ApiError::ServiceUnavailable(
            "planning poker is disabled by settings".into(),
        ));
    }

    let bead_exists = {
        let beads = state.beads.read().await;
        beads.contains_key(&req.bead_id)
    };
    if !bead_exists {
        return Err(ApiError::NotFound("bead not found".into()));
    }

    let deck = match resolve_poker_deck(
        poker_cfg,
        req.deck_preset.as_deref(),
        req.custom_deck.as_deref(),
    ) {
        Ok(deck) => deck,
        Err((_status, body)) => {
            return Err(ApiError::BadRequest(
                body.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("invalid deck configuration")
                    .to_string(),
            ))
        }
    };

    let now = chrono::Utc::now();
    let session = PlanningPokerSession {
        bead_id: req.bead_id,
        phase: PlanningPokerPhase::Voting,
        votes: Vec::new(),
        participants: normalize_participants(&req.participants),
        deck,
        round_duration_seconds: req
            .round_duration_seconds
            .or(Some(poker_cfg.round_duration_seconds)),
        consensus_card: None,
        started_at: now,
        updated_at: now,
    };
    let response = planning_poker_response(&session);

    state
        .planning_poker_sessions
        .write()
        .await
        .insert(req.bead_id, session);

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::to_value(response).map_err(|e| ApiError::Internal(e.to_string()))?),
    ))
}

/// POST /api/kanban/poker/vote -- submit or update a vote in an active planning poker session.
pub(crate) async fn submit_planning_poker_vote(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<SubmitPlanningPokerVoteRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if req.voter.trim().is_empty() || req.card.trim().is_empty() {
        return Err(ApiError::BadRequest("voter and card are required".into()));
    }

    let mut sessions = state.planning_poker_sessions.write().await;
    let Some(session) = sessions.get_mut(&req.bead_id) else {
        return Err(ApiError::NotFound("planning poker session not found".into()));
    };

    if !matches!(session.phase, PlanningPokerPhase::Voting) {
        return Err(ApiError::Conflict("session is not in voting phase".into()));
    }

    if !session.deck.iter().any(|card| card == &req.card) {
        return Err(ApiError::BadRequest("card is not in active deck".into()));
    }

    if let Some(existing) = session.votes.iter_mut().find(|v| v.voter == req.voter) {
        existing.card = req.card;
    } else {
        let voter = req.voter.clone();
        session.votes.push(PlanningPokerVote {
            voter,
            card: req.card,
        });
    }
    if !session.participants.iter().any(|p| p == &req.voter) {
        session.participants.push(req.voter);
    }
    session.updated_at = chrono::Utc::now();

    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::to_value(planning_poker_response(session)).map_err(|e| ApiError::Internal(e.to_string()))?),
    ))
}

/// POST /api/kanban/poker/reveal -- reveal all votes and calculate consensus.
pub(crate) async fn reveal_planning_poker(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<RevealPlanningPokerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let cfg = state.settings_manager.load_or_default();
    let poker_cfg = &cfg.kanban.planning_poker;

    let mut sessions = state.planning_poker_sessions.write().await;
    let Some(session) = sessions.get_mut(&req.bead_id) else {
        return Err(ApiError::NotFound("planning poker session not found".into()));
    };

    if matches!(session.phase, PlanningPokerPhase::Revealed) {
        return Err(ApiError::Conflict("session already revealed".into()));
    }
    if poker_cfg.reveal_requires_all_votes && !session.participants.is_empty() {
        let voted: std::collections::BTreeSet<String> =
            session.votes.iter().map(|v| v.voter.clone()).collect();
        let missing = session
            .participants
            .iter()
            .filter(|p| !voted.contains((*p).as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(ApiError::Conflict(format!(
                "not all participants have voted: missing {:?}",
                missing
            )));
        }
    }

    session.phase = PlanningPokerPhase::Revealed;
    session.consensus_card = consensus_card_from_votes(&session.votes);
    session.updated_at = chrono::Utc::now();

    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::to_value(planning_poker_response(session)).map_err(|e| ApiError::Internal(e.to_string()))?),
    ))
}

/// POST /api/kanban/poker/simulate -- run a simulated planning poker session.
pub(crate) async fn simulate_planning_poker(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<SimulatePlanningPokerRequest>,
) -> Result<impl IntoResponse, ApiError> {
    match simulate_planning_poker_for_bead(&state, req).await {
        Ok(response) => Ok((
            axum::http::StatusCode::OK,
            Json(serde_json::to_value(response).map_err(|e| ApiError::Internal(e.to_string()))?),
        )),
        Err((status, body)) => {
            let message = body
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("simulation failed")
                .to_string();
            match status {
                axum::http::StatusCode::NOT_FOUND => Err(ApiError::NotFound(message)),
                axum::http::StatusCode::BAD_REQUEST => Err(ApiError::BadRequest(message)),
                axum::http::StatusCode::FORBIDDEN => Err(ApiError::ServiceUnavailable(message)),
                _ => Err(ApiError::Internal(message)),
            }
        }
    }
}

/// GET /api/kanban/poker/{bead_id} -- retrieve current state of a planning poker session.
pub(crate) async fn get_planning_poker_session(
    State(state): State<Arc<ApiState>>,
    Path(bead_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let sessions = state.planning_poker_sessions.read().await;
    let Some(session) = sessions.get(&bead_id) else {
        return Err(ApiError::NotFound("planning poker session not found".into()));
    };

    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::to_value(planning_poker_response(session)).map_err(|e| ApiError::Internal(e.to_string()))?),
    ))
}
