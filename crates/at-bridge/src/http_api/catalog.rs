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
            router: Router::new(),
            specs: Vec::new(),
        }
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
                    auth: RouteAuth::ApiKey,
                    request: spec.request.map(str::to_string),
                    response: spec.response.map(str::to_string),
                    body_limit: spec.body_limit,
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

/// Mount every domain onto a fresh router and build the matching catalog.
///
/// Every route passes the same top-level middleware stack (auth, rate limit,
/// CORS), so every card is [`RouteAuth::ApiKey`]; `auth_enforced` records
/// whether a key is actually configured.
pub(crate) fn mount_all(domains: Vec<Domain>, auth_enforced: bool) -> (ApiRouter, ApiCatalog) {
    let mut app = Router::new();
    let mut cards = Vec::new();
    for domain in domains {
        let (next, domain_cards) = domain.mount(app);
        app = next;
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
    (app, catalog)
}

/// GET /api/catalog, GET /api/v1/catalog -- the route catalog.
pub(crate) async fn get_catalog(
    Extension(catalog): Extension<Arc<ApiCatalog>>,
) -> Json<ApiCatalog> {
    Json((*catalog).clone())
}
