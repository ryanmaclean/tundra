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
    if let Some(existing) = DaemonLockfile::read_valid() {
        eprintln!(
            "auto-tundra daemon already running (pid={}, api={}, frontend={})",
            existing.pid,
            existing.api_url(),
            existing.frontend_url(),
        );
        std::process::exit(1);
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

    // --- Spawn the frontend server with dynamic port ---
    let (frontend_port_tx, frontend_port_rx) = tokio::sync::oneshot::channel::<u16>();
    let frontend_handle = tokio::spawn(serve_frontend(api_port, frontend_port_tx));

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
    let reg = at_intelligence::ResilientRegistry::from_config(&daemon.config());
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

/// Serve the Leptos dist/ directory as static files on a dynamic port.
///
/// Injects `<script>window.__TUNDRA_API_PORT__={api_port};</script>` into
/// index.html so the WASM frontend discovers the API server automatically.
/// Sends the bound port back to main via the oneshot channel.
async fn serve_frontend(api_port: u16, port_tx: tokio::sync::oneshot::Sender<u16>) {
    let _span = traced_span!("serve_frontend", component = "http_server");

    profiling::record_event("frontend_server_start", &[]);

    use axum::response::Html;
    use axum::routing::get;
    use axum::Router;
    use tower_http::services::ServeDir;

    let dist_dir = find_dist_dir();

    profiling::add_span_tags(&[("dist_dir", dist_dir.to_str().unwrap_or("unknown"))]);

    // Read and patch index.html with API port injection.
    let index_path = dist_dir.join("index.html");
    let index_html = match std::fs::read_to_string(&index_path) {
        Ok(html) => html.replace(
            "</head>",
            &format!("<script>window.__TUNDRA_API_PORT__={api_port};</script></head>"),
        ),
        Err(e) => {
            tracing::warn!(error = %e, "could not read index.html, using fallback");
            format!(
                "<html><head><script>window.__TUNDRA_API_PORT__={api_port};</script></head>\
                 <body>frontend not built — run <code>cd app/leptos-ui && trunk build</code></body></html>"
            )
        }
    };

    // Serve patched index.html for root and SPA fallback; ServeDir for all other assets.
    let index_for_handler = index_html.clone();
    let app = Router::new()
        .route(
            "/",
            get({
                let html = index_for_handler.clone();
                move || {
                    let html = html.clone();
                    async move { Html(html) }
                }
            }),
        )
        .fallback_service(
            ServeDir::new(&dist_dir).fallback(axum::routing::get(move || {
                let html = index_html.clone();
                async move { Html(html) }
            })),
        );

    // Bind to port 0 — OS assigns an ephemeral port.
    let listener = match tokio::net::TcpListener::bind("[::]:0").await {
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

    if let Err(e) = profile_async!("serve_frontend_requests", axum::serve(listener, app)).await {
        tracing::error!(error = %e, "frontend server error");
        profiling::record_event("frontend_server_error", &[("error", &e.to_string())]);
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
