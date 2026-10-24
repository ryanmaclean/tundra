//! Protected browser WebSocket ticket issuance. The API master key is
//! authenticated by the router's AuthLayer before this handler is called.

use crate::origin_validation::OriginAllowlist;
use crate::ws_tickets::{single_origin, IssueError, WsTicketStore, TICKET_TTL};
use axum::{
    http::{header::CACHE_CONTROL, HeaderMap, HeaderValue, StatusCode},
    Extension, Json,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub(crate) struct IssueWsTicketRequest {
    path: String,
}

#[derive(Serialize)]
pub(crate) struct IssueWsTicketResponse {
    ticket: String,
    expires_in_seconds: u64,
}

pub(crate) async fn issue_ws_ticket(
    Extension(store): Extension<WsTicketStore>,
    Extension(origins): Extension<OriginAllowlist>,
    headers: HeaderMap,
    Json(body): Json<IssueWsTicketRequest>,
) -> Result<
    (
        [(axum::http::HeaderName, HeaderValue); 1],
        Json<IssueWsTicketResponse>,
    ),
    StatusCode,
> {
    let origin = single_origin(&headers).ok_or(StatusCode::FORBIDDEN)?;
    if !origins.allows(origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    let ticket = store.issue(&body.path, origin).await.map_err(|err| match err {
        IssueError::InvalidPath => StatusCode::BAD_REQUEST,
        IssueError::Full => StatusCode::TOO_MANY_REQUESTS,
    })?;
    Ok((
        [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        Json(IssueWsTicketResponse {
            ticket: ticket.to_string(),
            expires_in_seconds: TICKET_TTL.as_secs(),
        }),
    ))
}
