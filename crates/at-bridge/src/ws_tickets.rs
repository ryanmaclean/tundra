//! Process-local, one-use browser WebSocket tickets.
//!
//! Tickets are bearer values in Sec-WebSocket-Protocol, never the API master
//! key. Each ticket is bound to its route and single issuing Origin. Do not
//! format either a ticket or an offered protocol into a log.

use axum::http::{header::{ORIGIN, SEC_WEBSOCKET_PROTOCOL}, HeaderMap};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use uuid::Uuid;

pub const SELECTED_PROTOCOL: &str = "tundra.v2";
pub const TICKET_PREFIX: &str = "tundra.ticket.";
pub const TICKET_TTL: Duration = Duration::from_secs(10);
const MAX_OUTSTANDING: usize = 64;

#[derive(Clone, Default)]
pub struct WsTicketStore(Arc<Mutex<HashMap<Uuid, Ticket>>>);

struct Ticket {
    path: String,
    origin: String,
    expires_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueError {
    InvalidPath,
    Full,
}

impl WsTicketStore {
    /// Mint only for a canonical route and a validated issuing Origin. All outstanding tickets disappear
    /// when this store is dropped on daemon restart/router replacement.
    pub async fn issue(&self, path: &str, origin: &str) -> Result<Uuid, IssueError> {
        if !canonical_ws_path(path) {
            return Err(IssueError::InvalidPath);
        }
        let mut tickets = self.0.lock().await;
        let now = Instant::now();
        tickets.retain(|_, t| t.expires_at > now);
        if tickets.len() >= MAX_OUTSTANDING {
            return Err(IssueError::Full);
        }
        let id = loop {
            let candidate = Uuid::new_v4();
            if !tickets.contains_key(&candidate) {
                break candidate;
            }
        };
        tickets.insert(
            id,
            Ticket {
                path: path.to_owned(),
                origin: origin.to_owned(),
                expires_at: now + TICKET_TTL,
            },
        );
        Ok(id)
    }

    /// Authenticate and consume before the HTTP 101 response. A wrong route
    /// does not consume the ticket; a successful match is removed atomically.
    pub async fn consume(&self, headers: &HeaderMap, path: &str) -> bool {
        if !canonical_ws_path(path) {
            return false;
        }
        let Some(origin) = single_origin(headers) else {
            return false;
        };
        let Some(id) = parse_ticket_offer(headers) else {
            return false;
        };
        let mut tickets = self.0.lock().await;
        let now = Instant::now();
        let matches = tickets
            .get(&id)
            .is_some_and(|t| t.path == path && t.origin == origin && t.expires_at > now);
        if matches {
            tickets.remove(&id);
        } else if tickets.get(&id).is_some_and(|t| t.expires_at <= now) {
            tickets.remove(&id);
        }
        matches
    }
}

/// Ticket requests and upgrades must carry exactly one readable Origin.
/// Preserve its exact bytes so one allowlisted origin cannot redeem another's
/// ticket, even when both match the router's global allowlist.
pub(crate) fn single_origin(headers: &HeaderMap) -> Option<&str> {
    let mut fields = headers.get_all(ORIGIN).iter();
    let origin = fields.next()?.to_str().ok()?;
    if origin.is_empty() || fields.next().is_some() {
        return None;
    }
    Some(origin)
}

/// Exact registered path, including canonical lowercase UUID for a terminal.
pub fn canonical_ws_path(path: &str) -> bool {
    if path == "/ws" || path == "/api/events/ws" {
        return true;
    }
    let Some(id) = path.strip_prefix("/ws/terminal/") else {
        return false;
    };
    Uuid::parse_str(id).is_ok_and(|parsed| parsed.hyphenated().to_string() == id)
}

fn parse_ticket_offer(headers: &HeaderMap) -> Option<Uuid> {
    let mut fields = headers.get_all(SEC_WEBSOCKET_PROTOCOL).iter();
    let field = fields.next()?.to_str().ok()?;
    if fields.next().is_some() {
        return None;
    }
    let mut offers = field.split(',').map(str::trim);
    let first = offers.next()?;
    let second = offers.next()?;
    if offers.next().is_some() {
        return None;
    }
    let ticket_value = if first == SELECTED_PROTOCOL {
        second.strip_prefix(TICKET_PREFIX)?
    } else if second == SELECTED_PROTOCOL {
        first.strip_prefix(TICKET_PREFIX)?
    } else {
        return None;
    };
    let id = Uuid::parse_str(ticket_value).ok()?;
    (id.hyphenated().to_string() == ticket_value).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn offered(id: Uuid) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, HeaderValue::from_static("http://localhost:3000"));
        headers.insert(
            SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_str(&format!("{SELECTED_PROTOCOL}, {TICKET_PREFIX}{id}")).unwrap(),
        );
        headers
    }

    #[tokio::test]
    async fn path_replay_expiry_and_capacity() {
        let store = WsTicketStore::default();
        let id = store.issue("/ws", "http://localhost:3000").await.unwrap();
        let offer = offered(id);
        assert!(!store.consume(&offer, "/api/events/ws").await);
        let mut other_allowed_origin = offer.clone();
        other_allowed_origin.insert(
            ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:4000"),
        );
        assert!(!store.consume(&other_allowed_origin, "/ws").await);
        let mut duplicate_origin = offer.clone();
        duplicate_origin.append(ORIGIN, HeaderValue::from_static("http://localhost:3000"));
        assert!(!store.consume(&duplicate_origin, "/ws").await);
        assert!(store.consume(&offer, "/ws").await);
        assert!(!store.consume(&offer, "/ws").await);
        let expired = store.issue("/ws", "http://localhost:3000").await.unwrap();
        {
            let mut tickets = store.0.lock().await;
            tickets.get_mut(&expired).unwrap().expires_at = Instant::now() - Duration::from_secs(1);
        }
        assert!(!store.consume(&offered(expired), "/ws").await);
        assert_eq!(
            store.issue("/ws/terminal/nope", "http://localhost:3000").await,
            Err(IssueError::InvalidPath)
        );
        for _ in 0..MAX_OUTSTANDING {
            store.issue("/ws", "http://localhost:3000").await.unwrap();
        }
        assert_eq!(store.issue("/ws", "http://localhost:3000").await, Err(IssueError::Full));
    }

    #[tokio::test]
    async fn concurrent_consumers_cannot_both_use_one_ticket() {
        let store = WsTicketStore::default();
        let id = store.issue("/ws", "http://localhost:3000").await.unwrap();
        let offer = offered(id);
        let (first, second) = tokio::join!(
            store.consume(&offer, "/ws"),
            store.consume(&offer, "/ws"),
        );
        assert_ne!(first, second);
    }

    #[test]
    fn rejects_ambiguous_offers() {
        let id = Uuid::new_v4();
        let mut headers = offered(id);
        headers.append(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static("other"));
        assert_eq!(parse_ticket_offer(&headers), None);
        assert!(!canonical_ws_path(
            "/ws/terminal/00000000-0000-0000-0000-000000000000/"
        ));
    }
}
