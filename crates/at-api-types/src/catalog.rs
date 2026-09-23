//! Wire contract for the daemon's route catalog (`GET /api/catalog`).
//!
//! The catalog is the single discovery endpoint for the at-bridge HTTP API:
//! one call returns every route with its method, path, auth requirement,
//! one-line description and request/response type names. The top-level shape
//! follows the bop `catalog.v1.json` schema (`schema_version`,
//! `generated_at`, `source`, `cards[]` with `id` + `version`), so the
//! `bop-catalog` recipes (`jq '.cards[] | {id, title}'`) work unchanged; each
//! card additionally carries the HTTP fields below.
//!
//! Breaking changes to this shape bump [`CATALOG_SCHEMA_VERSION`] and add a
//! new `/api/vN/catalog` path; `v1` keeps working.

use serde::{Deserialize, Serialize};

/// Schema version of [`ApiCatalog`].
pub const CATALOG_SCHEMA_VERSION: &str = "v1";

/// Unversioned catalog path (always serves the current schema).
pub const CATALOG_PATH: &str = "/api/catalog";

/// Versioned catalog path pinned to schema `v1`.
pub const CATALOG_V1_PATH: &str = "/api/v1/catalog";

/// The route catalog returned by `GET /api/catalog`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiCatalog {
    /// Always [`CATALOG_SCHEMA_VERSION`].
    pub schema_version: String,
    /// RFC 3339 timestamp of when the router (and so this catalog) was built.
    pub generated_at: String,
    /// Component that generated the catalog (`at-bridge`).
    pub source: String,
    pub service: ApiCatalogService,
    pub auth: ApiCatalogAuth,
    /// One card per `(method, path)` pair, sorted by path then method.
    pub cards: Vec<ApiCatalogRoute>,
}

/// Identity of the service that serves the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiCatalogService {
    pub name: String,
    /// SemVer of the serving crate.
    pub version: String,
}

/// How to authenticate to routes whose [`ApiCatalogRoute::auth`] is
/// [`RouteAuth::ApiKey`]. See [`crate::auth`] for the full contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiCatalogAuth {
    /// Always `api_key`.
    pub scheme: String,
    /// `false` only when the daemon runs without a key (development mode).
    pub enforced: bool,
    /// Header carrying the key ([`crate::auth::API_KEY_HEADER`]).
    pub header: String,
    /// `Authorization: Bearer <key>` is accepted as an alternative.
    pub bearer: bool,
    /// Query parameter accepted on WebSocket upgrades only.
    pub ws_query_param: String,
    /// [`crate::auth::AUTH_CONTRACT_VERSION`].
    pub contract_version: String,
}

/// Authentication a single route requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteAuth {
    /// Requires the daemon API key (when [`ApiCatalogAuth::enforced`]).
    ApiKey,
    /// Reachable without credentials.
    None,
}

/// One routable `(method, path)` pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiCatalogRoute {
    /// Stable kebab-case id derived from method and path, e.g.
    /// `post-api-beads-id-status`.
    pub id: String,
    /// SemVer of the route contract (the serving crate's version).
    pub version: String,
    /// `METHOD /path`, for humans and trigger matching.
    pub title: String,
    /// One-line description of what the route does.
    pub description: String,
    /// Router domain the route belongs to (`beads`, `tasks`, `github`, ...).
    pub domain: String,
    /// Upper-case HTTP method.
    pub method: String,
    /// axum 0.8 path template (`{param}` segments).
    pub path: String,
    pub auth: RouteAuth,
    /// Request body type name, when the route takes a JSON body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    /// Response body type name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
    /// Per-route request body limit in bytes, when it differs from the
    /// global 2 MiB default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_limit: Option<usize>,
}

/// Derive the stable card id for a route: lower-case method and path with
/// every run of non-alphanumeric characters collapsed to `-`.
pub fn route_id(method: &str, path: &str) -> String {
    let mut id = method.to_ascii_lowercase();
    let mut dash = true;
    for c in path.chars() {
        if c.is_ascii_alphanumeric() {
            if dash {
                id.push('-');
                dash = false;
            }
            id.push(c.to_ascii_lowercase());
        } else {
            dash = true;
        }
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_id_is_kebab_case() {
        assert_eq!(
            route_id("POST", "/api/beads/{id}/status"),
            "post-api-beads-id-status"
        );
        assert_eq!(route_id("GET", "/ws"), "get-ws");
        assert_eq!(
            route_id("GET", "/api/cli/available"),
            "get-api-cli-available"
        );
    }

    #[test]
    fn route_auth_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&RouteAuth::ApiKey).unwrap(),
            "\"api_key\""
        );
        assert_eq!(serde_json::to_string(&RouteAuth::None).unwrap(), "\"none\"");
    }

    #[test]
    fn optional_fields_are_omitted() {
        let card = ApiCatalogRoute {
            id: "get-ws".into(),
            version: "0.1.0".into(),
            title: "GET /ws".into(),
            description: "Event stream".into(),
            domain: "websocket".into(),
            method: "GET".into(),
            path: "/ws".into(),
            auth: RouteAuth::ApiKey,
            request: None,
            response: None,
            body_limit: None,
        };
        let v = serde_json::to_value(&card).unwrap();
        assert!(v.get("request").is_none());
        assert!(v.get("body_limit").is_none());
        let back: ApiCatalogRoute = serde_json::from_value(v).unwrap();
        assert_eq!(back, card);
    }
}
