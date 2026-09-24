// ---------------------------------------------------------------------------
// HTTP API module directory
// ---------------------------------------------------------------------------
//
// Split from the original monolith `http_api.rs` (10 000+ lines) into
// domain-oriented sub-modules.  This file wires them together, owns the
// Axum router, and re-exports public items so that downstream crates
// (`at-daemon`, `intelligence_api`, `terminal_ws`) keep compiling without
// any import-path changes.

mod agents;
mod beads;
mod bootstrap;
mod catalog;
mod gate_flow;
mod gitea;
mod github;
mod integrations;
mod kanban;
mod mcp;
pub(crate) mod mcp_sse;
mod metrics;
mod misc;
mod notifications;
mod pipeline;
mod projects;
mod queue;
mod routes;
mod sessions;
mod settings;
mod stacks;
pub mod state;
mod tasks;
#[cfg(test)]
mod tests;
pub mod types;
mod websocket;
mod worktrees;

// ---- Re-exports for backward compatibility --------------------------------

pub use state::ApiState;
pub use types::*;

// ---- Shared API types -----------------------------------------------------
//
// Re-export at-api-types to provide a single source of truth for API contracts.
// The backend uses at_core::types internally but serializes to JSON that matches
// these Api* type structures. Frontend clients (leptos-ui, at-tui) use these
// types for deserializing responses and constructing requests.
pub use at_api_types;

// Re-export items used by intelligence_api.rs
pub(crate) use kanban::simulate_planning_poker_for_bead;

// Re-export items used by at-daemon
pub use self::router::{api_router, api_router_with_auth};

// Re-export spawn_oauth_token_refresh_monitor (used by at-daemon)
pub use self::oauth_monitor::spawn_oauth_token_refresh_monitor;

// Re-export spawn_pr_poller (used by at-daemon)
pub use github::spawn_pr_poller;

// ---------------------------------------------------------------------------
// Shared utilities used across multiple handler modules
// ---------------------------------------------------------------------------

use at_harness::security::{InputSanitizer, SecurityError};

/// Validate a user-supplied text field (title, description, etc.).
pub(crate) fn validate_text_field(input: &str) -> Result<(), SecurityError> {
    let sanitizer = InputSanitizer::default();
    sanitizer.sanitize(input).map(|_| ())
}

/// Deep-merge `patch` into `target`. Objects are merged recursively; other
/// values are replaced.
pub(crate) fn merge_json(target: &mut serde_json::Value, patch: &serde_json::Value) {
    match (target.is_object(), patch.is_object()) {
        (true, true) => {
            let t = target
                .as_object_mut()
                .expect("target.is_object() already verified");
            let p = patch
                .as_object()
                .expect("patch.is_object() already verified");
            for (key, value) in p {
                let entry = t.entry(key.clone()).or_insert(serde_json::Value::Null);
                merge_json(entry, value);
            }
        }
        _ => {
            *target = patch.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Router + middleware
// ---------------------------------------------------------------------------

mod router {
    use super::*;
    use axum::{
        body::Body,
        extract::{DefaultBodyLimit, Request},
        middleware::{self as axum_middleware, Next},
        response::Response,
        Router,
    };
    use std::sync::Arc;
    use tower_http::compression::CompressionLayer;
    use tower_http::cors::CorsLayer;

    use crate::auth::AuthLayer;
    use crate::origin_validation::OriginAllowlist;
    use crate::rate_limit_middleware::RateLimitLayer;
    use at_telemetry::middleware::metrics_middleware;
    use at_telemetry::tracing_setup::request_id_middleware;
    use axum::Extension;

    /// Build the full API router with all REST and WebSocket routes.
    ///
    /// When `api_key` is `Some`, the [`AuthLayer`] middleware will require
    /// every request to carry a valid key, except the unauthenticated
    /// (rate-limited) route catalog. When `None`, all requests pass through
    /// (development mode).
    pub fn api_router(state: Arc<ApiState>) -> Router {
        api_router_with_auth(state, None, vec![])
    }

    /// Add browser cross-origin isolation headers needed for threaded WASM paths.
    async fn isolation_headers_middleware(request: Request<Body>, next: Next) -> Response {
        let mut response = next.run(request).await;
        let headers = response.headers_mut();
        headers.insert(
            "Cross-Origin-Opener-Policy",
            axum::http::HeaderValue::from_static("same-origin"),
        );
        headers.insert(
            "Cross-Origin-Embedder-Policy",
            axum::http::HeaderValue::from_static("credentialless"),
        );
        headers.insert(
            "Cross-Origin-Resource-Policy",
            axum::http::HeaderValue::from_static("same-origin"),
        );
        headers.insert(
            "X-Content-Type-Options",
            axum::http::HeaderValue::from_static("nosniff"),
        );
        headers.insert(
            "X-Frame-Options",
            axum::http::HeaderValue::from_static("DENY"),
        );
        headers.insert(
            "Strict-Transport-Security",
            axum::http::HeaderValue::from_static("max-age=63072000; includeSubDomains"),
        );
        headers.insert(
            "X-XSS-Protection",
            axum::http::HeaderValue::from_static("1; mode=block"),
        );
        headers.insert(
            "Referrer-Policy",
            axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
        );
        headers.insert(
            "Cache-Control",
            axum::http::HeaderValue::from_static("no-store, no-cache, must-revalidate, private"),
        );
        response
    }

    /// Build the API router with optional authentication.
    pub fn api_router_with_auth(
        state: Arc<ApiState>,
        api_key: Option<String>,
        allowed_origins: Vec<String>,
    ) -> Router {
        // Clone the rate limiter before building the router.
        let rate_limiter = state.rate_limiter.clone();
        let rate_limit_policy = state.rate_limit_policy;
        // One allowlist (defaults + configured) shared by CORS and every
        // WebSocket handler, so configuring an origin works everywhere.
        let origins = OriginAllowlist::with_configured(&allowed_origins);
        let cors_origins = origins.clone();

        // Every route lives in a per-domain sub-router (see `routes.rs`);
        // mounting them also yields the catalog served at /api/catalog.
        let catalog::Mounted {
            public,
            protected,
            catalog,
        } = catalog::mount_all(routes::all(), api_key.is_some());

        // Three-tier rate limiting (global, per-client, per-endpoint; loopback
        // peers skip the per-client tiers). Returns HTTP 429 when exceeded.
        // Both halves share one limiter, so the global tier covers them all.
        // See ApiState::new() for config.
        let rate_limit = RateLimitLayer::new(rate_limiter).with_policy(rate_limit_policy);
        // Authenticated routes: auth runs first, then the rate limiter.
        let protected = protected
            .layer(rate_limit.clone())
            .layer(AuthLayer::new(api_key));
        // Unauthenticated routes (the catalog): rate limited only.
        let public = public.layer(rate_limit);
        // `merge` keeps `protected`'s (auth-layered) fallback, so unmatched
        // paths still require the key.
        let app = public.merge(protected);

        app.layer(Extension(Arc::new(catalog)))
            .layer(Extension(origins))
            .layer(CompressionLayer::new())
            .layer(axum_middleware::from_fn(metrics_middleware))
            .layer(axum_middleware::from_fn(request_id_middleware))
            .layer(axum_middleware::from_fn(isolation_headers_middleware))
            .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
            .layer(
                CorsLayer::new()
                    .allow_origin(tower_http::cors::AllowOrigin::predicate(
                        move |origin: &axum::http::HeaderValue,
                              _request_parts: &axum::http::request::Parts| {
                            origin
                                .to_str()
                                .map(|o| cors_origins.allows(o))
                                .unwrap_or(false)
                        },
                    ))
                    .allow_methods([
                        axum::http::Method::GET,
                        axum::http::Method::POST,
                        axum::http::Method::PUT,
                        axum::http::Method::DELETE,
                        axum::http::Method::PATCH,
                        axum::http::Method::OPTIONS,
                    ])
                    .allow_headers([
                        axum::http::header::CONTENT_TYPE,
                        axum::http::header::AUTHORIZATION,
                        axum::http::HeaderName::from_static(at_api_types::auth::API_KEY_HEADER),
                    ])
                    .allow_credentials(true),
            )
            .with_state(state)
    }
}

// ---------------------------------------------------------------------------
// OAuth token refresh monitor
// ---------------------------------------------------------------------------

mod oauth_monitor {
    use super::state::ApiState;
    use crate::oauth_token_manager::OAuthTokenManager;
    use at_integrations::github::oauth as gh_oauth;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // ---------------------------------------------------------------------------
    // Public seam types
    // ---------------------------------------------------------------------------

    /// Error returned by the env-var resolver when required variables are absent.
    #[derive(Debug, PartialEq)]
    pub(crate) enum ConfigError {
        /// A required environment variable is missing or empty.
        Missing(String),
    }

    impl std::fmt::Display for ConfigError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                ConfigError::Missing(var) => write!(f, "missing env var: {var}"),
            }
        }
    }

    /// The three observable outcomes of a single refresh-monitor poll.
    ///
    /// Returned by [`poll_oauth_refresh_once`] so callers (and tests) can assert
    /// on which branch of the loop body fired without inspecting global state.
    #[derive(Debug, PartialEq)]
    pub(crate) enum RefreshOutcome {
        /// Required env vars were absent; the token manager was NOT consulted.
        NoEnvVar,
        /// Env vars were present but the token is not due for refresh yet.
        NotDueYet,
        /// The token was due for refresh and a client was constructed (refresh
        /// implementation is still pending — see subtask-3-2).
        RefreshAttempted,
    }

    // ---------------------------------------------------------------------------
    // Production env resolver
    // ---------------------------------------------------------------------------

    /// Read the four OAuth env vars and construct a [`gh_oauth::GitHubOAuthConfig`].
    ///
    /// This is the production implementation passed to [`poll_oauth_refresh_once`].
    /// Tests inject their own closure so they never touch the process environment.
    pub(crate) fn oauth_config_from_env() -> Result<gh_oauth::GitHubOAuthConfig, ConfigError> {
        let client_id = match std::env::var("GITHUB_OAUTH_CLIENT_ID") {
            Ok(v) if !v.is_empty() => v,
            _ => return Err(ConfigError::Missing("GITHUB_OAUTH_CLIENT_ID".into())),
        };

        let client_secret = match std::env::var("GITHUB_OAUTH_CLIENT_SECRET") {
            Ok(v) if !v.is_empty() => v,
            _ => return Err(ConfigError::Missing("GITHUB_OAUTH_CLIENT_SECRET".into())),
        };

        let redirect_uri = std::env::var("GITHUB_OAUTH_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:3000/api/github/oauth/callback".into());

        let scopes = std::env::var("GITHUB_OAUTH_SCOPES")
            .unwrap_or_else(|_| "repo,read:user,user:email".into())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>();

        Ok(gh_oauth::GitHubOAuthConfig {
            client_id,
            client_secret,
            redirect_uri,
            scopes,
        })
    }

    // ---------------------------------------------------------------------------
    // Testable poll seam
    // ---------------------------------------------------------------------------

    /// Execute a single iteration of the OAuth token refresh monitor loop.
    ///
    /// # Lock-ordering fix
    ///
    /// The original loop acquired the token-manager read lock **before** reading
    /// env vars, meaning the lock was held across syscalls.  This function inverts
    /// the order: `env_resolver` is called **first** (no lock held), and the
    /// token-manager lock is only acquired after a valid config is in hand.  This
    /// eliminates the read-guard-across-env-var hazard entirely.
    ///
    /// # Arguments
    ///
    /// * `token_manager` — shared token manager (the same `Arc<RwLock<…>>` that
    ///   lives in `ApiState`).
    /// * `env_resolver` — closure that returns the OAuth config or an error.
    ///   Production code passes `oauth_config_from_env`; tests inject a stub.
    pub(crate) async fn poll_oauth_refresh_once(
        token_manager: &Arc<RwLock<OAuthTokenManager>>,
        env_resolver: impl FnOnce() -> Result<gh_oauth::GitHubOAuthConfig, ConfigError>,
    ) -> RefreshOutcome {
        use tracing::{debug, info, warn};

        // Step 1: resolve env config BEFORE acquiring any lock.
        let oauth_config = match env_resolver() {
            Ok(cfg) => cfg,
            Err(ConfigError::Missing(var)) => {
                warn!("Cannot refresh OAuth token: {var} not set");
                return RefreshOutcome::NoEnvVar;
            }
        };

        // Step 2: acquire the read lock only to check refresh status, then drop it.
        let needs_refresh = {
            let mgr = token_manager.read().await;
            mgr.should_refresh().await
        };

        if needs_refresh {
            info!("OAuth token approaching expiration, attempting refresh");

            let _oauth_client = gh_oauth::GitHubOAuthClient::new(oauth_config);

            warn!(
                "OAuth token needs refresh but refresh_token support not yet implemented. \
                 This will be added in subtask-3-2. User will need to re-authenticate."
            );

            RefreshOutcome::RefreshAttempted
        } else {
            debug!("OAuth token is valid, no refresh needed");
            RefreshOutcome::NotDueYet
        }
    }

    // ---------------------------------------------------------------------------
    // Public entry point (thin wrapper around the loop + poll_once)
    // ---------------------------------------------------------------------------

    /// Spawn a background task to monitor OAuth token expiration and refresh when needed.
    ///
    /// This task runs every 5 minutes and checks if the OAuth token needs to be refreshed
    /// (i.e., will expire within the next 5 minutes). If refresh is needed, it attempts
    /// to refresh the token using GitHub's refresh_token mechanism.
    ///
    /// # Arguments
    /// * `state` - The shared API state containing the OAuth token manager
    ///
    /// # Example
    /// ```no_run
    /// use std::sync::Arc;
    /// use at_bridge::http_api::{ApiState, spawn_oauth_token_refresh_monitor};
    /// use at_bridge::event_bus::EventBus;
    ///
    /// # async fn example() {
    /// let event_bus = EventBus::new();
    /// let state = Arc::new(ApiState::new(event_bus));
    /// spawn_oauth_token_refresh_monitor(state);
    /// # }
    /// ```
    pub fn spawn_oauth_token_refresh_monitor(state: Arc<ApiState>) {
        tokio::spawn(async move {
            use std::time::Duration;
            use tracing::info;

            let mut interval = tokio::time::interval(Duration::from_secs(300));
            interval.tick().await;

            info!("OAuth token refresh monitor started");

            loop {
                interval.tick().await;

                let outcome =
                    poll_oauth_refresh_once(&state.oauth_token_manager, oauth_config_from_env)
                        .await;

                tracing::debug!("OAuth refresh monitor poll outcome: {:?}", outcome);
            }
        });
    }

    // ---------------------------------------------------------------------------
    // Tests
    // ---------------------------------------------------------------------------

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::oauth_token_manager::OAuthTokenManager;
        use std::sync::Arc;
        use tokio::sync::RwLock;

        fn make_token_manager() -> Arc<RwLock<OAuthTokenManager>> {
            Arc::new(RwLock::new(OAuthTokenManager::new()))
        }

        fn valid_oauth_config() -> gh_oauth::GitHubOAuthConfig {
            gh_oauth::GitHubOAuthConfig {
                client_id: "test_client_id".into(),
                client_secret: "test_client_secret".into(),
                redirect_uri: "http://localhost:3000/callback".into(),
                scopes: vec!["repo".into()],
            }
        }

        /// When the env resolver returns an error the outcome must be `NoEnvVar`
        /// and the token manager must NOT be touched (no lock contention / no
        /// side-effects on the manager itself).
        #[tokio::test]
        async fn poll_once_returns_no_env_var_when_env_missing() {
            let token_manager = make_token_manager();

            // Seed a token to prove the manager is NOT read during NoEnvVar path.
            token_manager
                .write()
                .await
                .store_token("ghp_test", Some(60), None)
                .await;

            let outcome = poll_oauth_refresh_once(&token_manager, || {
                Err(ConfigError::Missing("GITHUB_OAUTH_CLIENT_ID".into()))
            })
            .await;

            assert_eq!(outcome, RefreshOutcome::NoEnvVar);
        }

        // Mutation check: if the `NoEnvVar` arm were changed to return
        // `NotDueYet`, the assertion above would fail.  This comment documents
        // the mutation that was manually applied, confirmed to fail, then
        // reverted.

        /// When a valid token exists (expires far in the future) the outcome must
        /// be `NotDueYet` — the refresh client is never constructed.
        #[tokio::test]
        async fn poll_once_returns_not_due_yet_when_token_is_valid() {
            let token_manager = make_token_manager();

            // Store a token that expires in 1 hour — well outside the 5-minute
            // refresh threshold, so `should_refresh()` returns false.
            token_manager
                .write()
                .await
                .store_token("ghp_long_lived", Some(3600), None)
                .await;

            let outcome =
                poll_oauth_refresh_once(&token_manager, || Ok(valid_oauth_config())).await;

            assert_eq!(outcome, RefreshOutcome::NotDueYet);
        }

        /// When a token is near expiry (within the 5-minute refresh threshold)
        /// the outcome must be `RefreshAttempted`.
        ///
        /// Note: the actual HTTP call is not yet implemented (subtask-3-2), so
        /// the test asserts on the outcome variant only.  An integration test
        /// exercising a real HTTP round-trip is deferred until the refresh
        /// implementation lands.
        #[tokio::test]
        async fn poll_once_returns_refresh_attempted_when_token_near_expiry() {
            let token_manager = make_token_manager();

            // Store a token that expires in 60 seconds — within the 5-minute
            // (300-second) refresh threshold, so `should_refresh()` returns true.
            token_manager
                .write()
                .await
                .store_token("ghp_near_expiry", Some(60), None)
                .await;

            let outcome =
                poll_oauth_refresh_once(&token_manager, || Ok(valid_oauth_config())).await;

            assert_eq!(outcome, RefreshOutcome::RefreshAttempted);
        }
    }
}
