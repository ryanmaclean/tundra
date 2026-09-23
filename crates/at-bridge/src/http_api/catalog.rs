//! Route registry and the `GET /api/catalog` discovery endpoint.
//!
//! Every HTTP route is registered through [`Domain::route`], which adds the
//! handler to the domain's axum router **and** records its [`RouteSpec`] in
//! the same call. The catalog is built from those specs, so a route cannot be
//! served without being catalogued (or catalogued without being served).

use std::sync::Arc;

use at_api_types::auth::{API_KEY_HEADER, AUTH_CONTRACT_VERSION, WS_API_KEY_QUERY_PARAM};
use at_api_types::catalog::{
    route_id, ApiCatalog, ApiCatalogAuth, ApiCatalogRoute, ApiCatalogService, RouteAuth,
    CATALOG_SCHEMA_VERSION,
};
use axum::extract::DefaultBodyLimit;
use axum::handler::Handler;
use axum::routing::{on, MethodFilter};
use axum::{Extension, Json, Router};

use super::state::ApiState;

/// Router type every domain builds (state is supplied by the top-level router).
pub(crate) type ApiRouter = Router<Arc<ApiState>>;

/// Body limit for small control/CRUD requests (status changes, toggles, ...).
pub(crate) const SMALL_BODY: usize = 256 * 1024;

/// HTTP methods the API uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Verb {
    fn filter(self) -> MethodFilter {
        match self {
            Verb::Get => MethodFilter::GET,
            Verb::Post => MethodFilter::POST,
            Verb::Put => MethodFilter::PUT,
            Verb::Patch => MethodFilter::PATCH,
            Verb::Delete => MethodFilter::DELETE,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Verb::Get => "GET",
            Verb::Post => "POST",
            Verb::Put => "PUT",
            Verb::Patch => "PATCH",
            Verb::Delete => "DELETE",
        }
    }
}

/// Declarative description of one `(method, path)` route.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RouteSpec {
    verb: Verb,
    /// Path relative to the owning [`Domain`]'s mount point.
    path: &'static str,
    description: &'static str,
    request: Option<&'static str>,
    response: Option<&'static str>,
    body_limit: Option<usize>,
    schemas: &'static [&'static str],
}

impl RouteSpec {
    const fn new(verb: Verb, path: &'static str, description: &'static str) -> Self {
        Self {
            verb,
            path,
            description,
            request: None,
            response: None,
            body_limit: None,
            schemas: &[],
        }
    }
    pub(crate) const fn get(path: &'static str, description: &'static str) -> Self {
        Self::new(Verb::Get, path, description)
    }
    pub(crate) const fn post(path: &'static str, description: &'static str) -> Self {
        Self::new(Verb::Post, path, description)
    }
    pub(crate) const fn put(path: &'static str, description: &'static str) -> Self {
        Self::new(Verb::Put, path, description)
    }
    pub(crate) const fn patch(path: &'static str, description: &'static str) -> Self {
        Self::new(Verb::Patch, path, description)
    }
    pub(crate) const fn delete(path: &'static str, description: &'static str) -> Self {
        Self::new(Verb::Delete, path, description)
    }
    /// Name of the JSON request body type.
    pub(crate) const fn req(mut self, ty: &'static str) -> Self {
        self.request = Some(ty);
        self
    }
    /// Name of the JSON response body type.
    pub(crate) const fn res(mut self, ty: &'static str) -> Self {
        self.response = Some(ty);
        self
    }
    /// Versioned JSON Schema ids (served at `/api/v1/schemas/{id}`) that
    /// this route's bodies conform to.
    pub(crate) const fn schemas(mut self, ids: &'static [&'static str]) -> Self {
        self.schemas = ids;
        self
    }
    /// Per-route request body limit overriding the global 2 MiB default.
    pub(crate) const fn limit(mut self, bytes: usize) -> Self {
        self.body_limit = Some(bytes);
        self
    }
}

/// A per-domain sub-router, nested under `prefix`, together with the specs
/// of every route in it.
pub(crate) struct Domain {
    name: &'static str,
    prefix: &'static str,
    /// Auth every route in this domain requires. [`RouteAuth::None`] domains
    /// are mounted outside the auth layer (see [`mount_all`]).
    auth: RouteAuth,
    router: ApiRouter,
    specs: Vec<RouteSpec>,
}

impl Domain {
    /// A domain nested under `prefix` (e.g. `/api/beads`). Spec paths are
    /// relative to it; `/` serves exactly `prefix` (axum 0.8 nesting rule).
    pub(crate) fn new(name: &'static str, prefix: &'static str) -> Self {
        Self {
            name,
            prefix,
            auth: RouteAuth::ApiKey,
            router: Router::new(),
            specs: Vec::new(),
        }
    }

    /// Serve this domain without the API key. Its routes are still rate
    /// limited; reserve this for cold-discovery endpoints (the catalog).
    pub(crate) fn unauthenticated(mut self) -> Self {
        self.auth = RouteAuth::None;
        self
    }

    /// Register `handler` for `spec` and record the spec for the catalog.
    pub(crate) fn route<H, T>(mut self, spec: RouteSpec, handler: H) -> Self
    where
        H: Handler<T, Arc<ApiState>>,
        T: 'static,
    {
        let mut method_router = on(spec.verb.filter(), handler);
        if let Some(bytes) = spec.body_limit {
            method_router = method_router.layer(DefaultBodyLimit::max(bytes));
        }
        self.router = self.router.route(spec.path, method_router);
        self.specs.push(spec);
        self
    }

    /// Nest this domain into `app` and return its catalog cards.
    pub(crate) fn mount(self, app: ApiRouter) -> (ApiRouter, Vec<ApiCatalogRoute>) {
        let version = env!("CARGO_PKG_VERSION");
        let cards = self
            .specs
            .iter()
            .map(|spec| {
                let path = self.full_path(spec.path);
                let method = spec.verb.as_str();
                ApiCatalogRoute {
                    id: route_id(method, &path),
                    version: version.to_string(),
                    title: format!("{method} {path}"),
                    description: spec.description.to_string(),
                    domain: self.name.to_string(),
                    method: method.to_string(),
                    auth: self.auth,
                    request: spec.request.map(str::to_string),
                    response: spec.response.map(str::to_string),
                    body_limit: spec.body_limit,
                    schemas: spec.schemas.iter().map(|s| s.to_string()).collect(),
                    path,
                }
            })
            .collect();
        (app.nest(self.prefix, self.router), cards)
    }

    /// Absolute path of a spec path, mirroring axum 0.8's nesting rule.
    fn full_path(&self, path: &str) -> String {
        if path == "/" {
            self.prefix.to_string()
        } else {
            format!("{}{path}", self.prefix)
        }
    }
}

/// Routers returned by [`mount_all`], split by the auth their routes need.
pub(crate) struct Mounted {
    /// [`RouteAuth::None`] domains: the caller adds rate limiting only.
    pub(crate) public: ApiRouter,
    /// [`RouteAuth::ApiKey`] domains: the caller adds rate limiting and auth.
    pub(crate) protected: ApiRouter,
    pub(crate) catalog: ApiCatalog,
}

/// Mount every domain onto one of two fresh routers, by the domain's auth,
/// and build the matching catalog. Each card's `auth` is its domain's auth;
/// `auth_enforced` records whether a key is actually configured.
pub(crate) fn mount_all(domains: Vec<Domain>, auth_enforced: bool) -> Mounted {
    let mut public = Router::new();
    let mut protected = Router::new();
    let mut cards = Vec::new();
    for domain in domains {
        let domain_cards = match domain.auth {
            RouteAuth::None => {
                let (next, c) = domain.mount(public);
                public = next;
                c
            }
            RouteAuth::ApiKey => {
                let (next, c) = domain.mount(protected);
                protected = next;
                c
            }
        };
        cards.extend(domain_cards);
    }
    cards.sort_by(|a, b| (&a.path, &a.method).cmp(&(&b.path, &b.method)));

    let catalog = ApiCatalog {
        schema_version: CATALOG_SCHEMA_VERSION.to_string(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        source: "at-bridge".to_string(),
        service: ApiCatalogService {
            name: "auto-tundra".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        auth: ApiCatalogAuth {
            scheme: "api_key".to_string(),
            enforced: auth_enforced,
            header: API_KEY_HEADER.to_string(),
            bearer: true,
            ws_query_param: WS_API_KEY_QUERY_PARAM.to_string(),
            contract_version: AUTH_CONTRACT_VERSION.to_string(),
        },
        cards,
    };
    Mounted {
        public,
        protected,
        catalog,
    }
}

/// GET /api/v1/schemas -- ids of every published JSON Schema document.
pub(crate) async fn list_schemas() -> Json<serde_json::Value> {
    let ids: Vec<&str> = at_api_types::schemas::ALL.iter().map(|(id, _)| *id).collect();
    Json(serde_json::json!({
        "schemas": ids
            .iter()
            .map(|id| serde_json::json!({
                "id": id,
                "href": at_api_types::schemas::path_for(id),
            }))
            .collect::<Vec<_>>(),
    }))
}

/// GET /api/v1/schemas/{*id} -- one JSON Schema document by id, e.g.
/// `/api/v1/schemas/at.merge_gate.report/v1`. 404 `{"error", "id"}` when the
/// id is not published.
pub(crate) async fn get_schema(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let id = id.trim_start_matches('/');
    match at_api_types::schemas::lookup(id) {
        Some(doc) => (
            [(
                axum::http::header::CONTENT_TYPE,
                "application/schema+json",
            )],
            doc,
        )
            .into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "unknown schema id", "id": id})),
        )
            .into_response(),
    }
}

/// GET /api/catalog, GET /api/v1/catalog -- the route catalog.
pub(crate) async fn get_catalog(
    Extension(catalog): Extension<Arc<ApiCatalog>>,
) -> Json<ApiCatalog> {
    Json((*catalog).clone())
}
