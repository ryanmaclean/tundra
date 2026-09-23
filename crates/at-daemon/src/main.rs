#![allow(dead_code)]

//! auto-tundra daemon — starts the API server, patrol loops, and
//! serves the Leptos WASM frontend.

use anyhow::{Context, Result};
use at_core::config::Config;
use at_core::lockfile::DaemonLockfile;
use tracing::info;

mod benchmarks;
mod environment;
mod metrics;
mod profiling;
mod profiling_tests;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
    // Before any TLS client is built: aws-lc-rs provider, X25519MLKEM768 first.
    at_core::tls::install_default_crypto_provider();
    let _main_span = traced_span!(
        "main_execution",
        binary = "at-daemon",
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id()
    );

    // Load environment configuration
    environment::configure_app().context("failed to configure application environment")?;

    // Initialize tracing
    at_telemetry::logging::init_logging("at-daemon", "info");

    // Initialize enhanced Datadog OpenTelemetry
    profiling::init_datadog_telemetry().context("failed to initialize Datadog OpenTelemetry")?;

    info!("auto-tundra daemon starting");
    profiling::record_event(
        "daemon_startup",
        &[
            ("version", env!("CARGO_PKG_VERSION")),
            ("pid", &std::process::id().to_string()),
            ("architecture", std::env::consts::ARCH),
        ],
    );

    // Record startup metrics
    metrics::AppMetrics::daemon_started().await;

    // Ensure data directory exists
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let data_dir = std::path::Path::new(&home).join(".auto-tundra");
    std::fs::create_dir_all(&data_dir).ok();

    // Load config (or use defaults), expanding ~ in cache path
    let mut config = load_config(&home).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to load config, using defaults");
        Config::default()
    });

    // Expand ~ in cache path
    if config.cache.path.starts_with("~/") {
        config.cache.path = config.cache.path.replacen("~", &home, 1);
    }

    // --- Startup guard: check if a daemon is already running ---
    let replace_mode = std::env::args().any(|a| a == "--replace" || a == "-r");
    if let Some(existing) = DaemonLockfile::read_valid() {
        if replace_mode {
            info!(pid = existing.pid, "replacing existing daemon (--replace)");
            #[cfg(unix)]
            unsafe {
                libc::kill(existing.pid as i32, libc::SIGTERM);
            }
            // Give old daemon a moment to clean up, then force-remove stale lockfile.
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            DaemonLockfile::remove();
        } else {
            eprintln!(
                "auto-tundra daemon already running (pid={}, api={}, frontend={})\n\
                 \n  Dashboard: {}\n\
                 \n  Hint: use --replace to restart it.",
                existing.pid,
                existing.api_url(),
                existing.frontend_url(),
                existing.frontend_url(),
            );
            std::process::exit(1);
        }
    }

    // --- Bind API listener ---
    // If the config port is the default sentinel (9876), use port 0 for OS-assigned.
    // Otherwise honor the explicit config value.
    let api_bind_addr = if config.daemon.port == 9876 {
        format!("{}:0", config.daemon.host)
    } else {
        format!("{}:{}", config.daemon.host, config.daemon.port)
    };
    let api_listener = tokio::net::TcpListener::bind(&api_bind_addr)
        .await
        .with_context(|| format!("failed to bind API listener on {api_bind_addr}"))?;
    let api_port = api_listener.local_addr()?.port();
    info!(api_port, "API listener bound");

    // --- Make sure the API key exists before any client can discover us ---
    // Clients read the lockfile and then the key file; creating the key first
    // means a client never sees a lockfile without a matching key.
    let api_key = at_core::config::CredentialProvider::ensure_daemon_api_key();
    let api_key_file = at_core::config::CredentialProvider::daemon_api_key()
        .is_none()
        .then(|| {
            at_core::config::CredentialProvider::daemon_key_path()
                .to_string_lossy()
                .into_owned()
        });

    // --- Spawn the frontend server with dynamic port ---
    let (frontend_port_tx, frontend_port_rx) = tokio::sync::oneshot::channel::<u16>();
    let frontend_handle = tokio::spawn(serve_frontend(
        api_port,
        api_key,
        config.daemon.host.clone(),
        frontend_port_tx,
    ));

    // Wait for the frontend to report its bound port.
    let frontend_port = frontend_port_rx
        .await
        .context("frontend server failed to report its port")?;

    // --- Write lockfile after both ports are known ---
    let lockfile = DaemonLockfile {
        pid: std::process::id(),
        api_port,
        frontend_port,
        host: config.daemon.host.clone(),
        started_at: chrono::Utc::now().to_rfc3339(),
        project_path: std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_key_file,
    };
    if let Err(msg) = lockfile.acquire_or_fail() {
        eprintln!("failed to acquire lockfile: {msg}");
        std::process::exit(1);
    }
    info!("lockfile written to {}", DaemonLockfile::path().display());

    // Create and run the daemon
    let daemon = profile_async!(
        "daemon_initialization",
        at_daemon::daemon::Daemon::new(config)
    )
    .await?;

    // Record LLM profile bootstrap metrics after daemon creation
    let reg = at_intelligence::ResilientRegistry::from_config(daemon.config());
    let total_count = reg.count();
    if let Some(best) = reg.registry.best_available() {
        let provider_name = format!("{:?}", best.provider);
        metrics::AppMetrics::llm_profile_bootstrap(total_count as u32, &best.name, &provider_name)
            .await;
    }

    let shutdown = daemon.shutdown_handle();

    // Wire ctrl-c to trigger graceful shutdown + remove lockfile.
    tokio::spawn(async move {
        let _span = traced_span!("signal_handler", signal = "ctrl_c");

        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "failed to listen for ctrl-c");
            profiling::record_event("signal_handler_error", &[("error", &e.to_string())]);
            return;
        }
        info!("ctrl-c received, initiating shutdown");
        profiling::record_event("shutdown_triggered", &[("signal", "ctrl_c")]);
        DaemonLockfile::remove();
        shutdown.trigger();
    });

    info!("dashboard: http://localhost:{frontend_port}");
    info!("API server: http://localhost:{api_port}");
    profiling::record_event(
        "daemon_ready",
        &[
            ("frontend_port", &frontend_port.to_string()),
            ("api_port", &api_port.to_string()),
        ],
    );

    if let Err(e) = profile_async!("daemon_main_loop", daemon.run_with_listener(api_listener)).await
    {
        tracing::error!(error = %e, "daemon execution failed");
        profiling::record_event("daemon_execution_error", &[("error", &e.to_string())]);
        DaemonLockfile::remove();
        return Err(e);
    }

    // After daemon stops, clean up.
    DaemonLockfile::remove();
    frontend_handle.abort();
    info!("frontend server stopped");
    profiling::record_event("daemon_shutdown_complete", &[]);

    // Record shutdown metrics
    metrics::AppMetrics::daemon_stopped().await;

    Ok(())
}

fn load_config(home: &str) -> Result<Config> {
    let path = std::path::Path::new(home)
        .join(".auto-tundra")
        .join("config.toml");
    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Config = toml::from_str(&content).context("failed to parse config.toml")?;
        Ok(config)
    } else {
        info!("no config file found at {}, using defaults", path.display());
        Ok(Config::default())
    }
}

async fn frontend_isolation_headers_middleware(
    request: axum::extract::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
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
    response
}

/// Serve the Leptos dist/ directory as static files on a dynamic port.
///
/// Injects the daemon connection (see
/// `at_api_types::auth::browser_bootstrap_script`) into index.html so the
/// WASM frontend discovers the API server and can authenticate to it. The API
/// key is only injected for loopback peers addressing the server by a
/// loopback host name (see [`may_receive_api_key`]).
/// Sends the bound port back to main via the oneshot channel.
///
/// **Hot-reload friendly**: index.html is read from disk on every request so
/// that `trunk build` takes effect immediately without restarting the daemon.
async fn serve_frontend(
    api_port: u16,
    api_key: String,
    host: String,
    port_tx: tokio::sync::oneshot::Sender<u16>,
) {
    let _span = traced_span!("serve_frontend", component = "http_server");

    profiling::record_event("frontend_server_start", &[]);

    use axum::extract::{ConnectInfo, State};
    use axum::http::HeaderMap;
    use axum::response::Html;
    use axum::routing::get;
    use axum::Router;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tower_http::services::ServeDir;

    let dist_dir = find_dist_dir();

    profiling::add_span_tags(&[("dist_dir", dist_dir.to_str().unwrap_or("unknown"))]);

    struct FrontendCtx {
        dist: std::path::PathBuf,
        api_port: u16,
        api_key: String,
    }
    let ctx = Arc::new(FrontendCtx {
        dist: dist_dir.clone(),
        api_port,
        api_key,
    });

    // Read index.html from disk on each request and inject the connection.
    // This means `trunk build` takes effect immediately without daemon restart.
    async fn index(
        State(ctx): State<Arc<FrontendCtx>>,
        ConnectInfo(peer): ConnectInfo<SocketAddr>,
        headers: HeaderMap,
    ) -> Html<String> {
        let host = headers
            .get(axum::http::header::HOST)
            .and_then(|v| v.to_str().ok());
        let key = may_receive_api_key(peer.ip(), host).then_some(ctx.api_key.as_str());
        let raw = std::fs::read_to_string(ctx.dist.join("index.html")).ok();
        Html(render_index(raw, ctx.api_port, key))
    }

    // Serve live index.html for root and SPA fallback; ServeDir for all other assets.
    let index_route = get(index).with_state(ctx.clone());
    let app = Router::new()
        .route("/", index_route.clone())
        .fallback_service(ServeDir::new(&dist_dir).fallback(index_route))
        .layer(axum::middleware::from_fn(
            frontend_isolation_headers_middleware,
        ));

    // Bind to port 0 — OS assigns an ephemeral port.
    let frontend_bind_addr = format!("{}:0", host);
    let listener = match tokio::net::TcpListener::bind(&frontend_bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "failed to bind frontend server");
            profiling::record_event("frontend_server_bind_error", &[("error", &e.to_string())]);
            // Signal failure — drop the sender so the receiver gets an error.
            drop(port_tx);
            return;
        }
    };

    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    // Send the bound port back to main.
    let _ = port_tx.send(port);

    info!(port, "frontend server listening");
    profiling::record_event(
        "frontend_server_listening",
        &[("port", &port.to_string()), ("protocol", "http")],
    );

    let service = app.into_make_service_with_connect_info::<std::net::SocketAddr>();
    if let Err(e) = profile_async!("serve_frontend_requests", axum::serve(listener, service)).await
    {
        tracing::error!(error = %e, "frontend server error");
        profiling::record_event("frontend_server_error", &[("error", &e.to_string())]);
    }
}

/// Whether an index.html request may receive the API key.
///
/// Only a loopback peer that addresses us by a loopback host name gets it:
/// the peer check keeps the key off the LAN when `daemon.host` is not
/// loopback, and the Host check defeats DNS-rebinding pages (their Host is
/// the attacker's domain even though the TCP peer is local).
fn may_receive_api_key(peer: std::net::IpAddr, host_header: Option<&str>) -> bool {
    if !peer.is_loopback() {
        return false;
    }
    let Some(host) = host_header else {
        return false;
    };
    // Strip the port: "[::1]:8080" -> "[::1]", "localhost:80" -> "localhost".
    let name = if host.starts_with('[') {
        host.split_once(']')
            .map(|(h, _)| &host[..h.len() + 1])
            .unwrap_or(host)
    } else {
        host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host)
    };
    matches!(name, "localhost" | "127.0.0.1" | "[::1]")
}

/// Inject the browser bootstrap script into index.html (or a placeholder page
/// when the frontend has not been built).
fn render_index(raw: Option<String>, api_port: u16, api_key: Option<&str>) -> String {
    let script = format!(
        "<script>{}</script>",
        at_bridge::http_api::at_api_types::auth::browser_bootstrap_script(api_port, api_key)
    );
    match raw {
        Some(raw) => raw.replacen("</head>", &format!("{script}</head>"), 1),
        None => format!(
            "<html><head>{script}</head>\
             <body>frontend not built — run <code>cd app/leptos-ui && trunk build</code></body></html>"
        ),
    }
}

fn find_dist_dir() -> std::path::PathBuf {
    let _span = traced_span!("find_dist_dir");

    let candidates = [
        std::path::PathBuf::from("app/leptos-ui/dist"),
        std::path::PathBuf::from("../app/leptos-ui/dist"),
        std::path::PathBuf::from("../../app/leptos-ui/dist"),
    ];

    for (index, dir) in candidates.iter().enumerate() {
        let index_path = dir.join("index.html");
        if index_path.exists() {
            info!(path = %dir.display(), "found frontend dist directory");
            profiling::record_event(
                "frontend_dist_found",
                &[
                    ("path", dir.to_str().unwrap_or("invalid")),
                    ("candidate_index", &index.to_string()),
                ],
            );
            return dir.clone();
        }
    }

    tracing::warn!("frontend dist/ not found, falling back to app/leptos-ui/dist");
    profiling::record_event(
        "frontend_dist_fallback",
        &[("fallback_path", "app/leptos-ui/dist")],
    );
    candidates[0].clone()
}

#[cfg(test)]
mod frontend_tests {
    use super::*;
    use std::net::IpAddr;

    const V4_LO: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
    const V6_LO: IpAddr = IpAddr::V6(std::net::Ipv6Addr::LOCALHOST);

    #[test]
    fn key_only_for_loopback_peer_and_loopback_host() {
        assert!(may_receive_api_key(V4_LO, Some("127.0.0.1:5173")));
        assert!(may_receive_api_key(V4_LO, Some("localhost:5173")));
        assert!(may_receive_api_key(V4_LO, Some("localhost")));
        assert!(may_receive_api_key(V6_LO, Some("[::1]:5173")));

        // DNS rebinding: local TCP peer, attacker Host.
        assert!(!may_receive_api_key(V4_LO, Some("evil.example:5173")));
        assert!(!may_receive_api_key(V4_LO, Some("localhost.evil.example")));
        assert!(!may_receive_api_key(V4_LO, None));
        // LAN peer.
        let lan: IpAddr = "192.168.1.20".parse().unwrap();
        assert!(!may_receive_api_key(lan, Some("127.0.0.1:5173")));
    }

    #[test]
    fn render_index_injects_port_and_key_into_head() {
        let html = render_index(
            Some("<html><head><title>t</title></head><body></body></html>".into()),
            4242,
            Some("k-1"),
        );
        assert!(html.contains(
            "<script>window.__TUNDRA_API_PORT__=4242;window.__TUNDRA_API_KEY__=\"k-1\";</script></head>"
        ));

        let no_key = render_index(None, 4242, None);
        assert!(no_key.contains("window.__TUNDRA_API_PORT__=4242;"));
        assert!(!no_key.contains("__TUNDRA_API_KEY__"));
    }
}
