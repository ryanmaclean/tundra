//! auto-tundra daemon — starts the API server, patrol loops, and
//! serves the Leptos WASM frontend.

use anyhow::{Context, Result};
use at_core::config::Config;
use tracing::info;

mod profiling;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    at_telemetry::logging::init_logging("at-daemon", "info");

    // Initialize Datadog APM and profiling
    profiling::init_datadog().context("failed to initialize Datadog profiling")?;

    info!("auto-tundra daemon starting");

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

    // Standalone mode: use port 9090 (unless overridden in config.toml)
    if config.daemon.port == 9876 {
        // Default was never overridden — use the canonical standalone port.
        config.daemon.port = 9090;
    }
    let api_port = config.daemon.port;

    // Spawn the Leptos static file server on port 3001
    let frontend_handle = tokio::spawn(serve_frontend());

    // Create and run the daemon
    let daemon = at_daemon::daemon::Daemon::new(config).await?;
    let shutdown = daemon.shutdown_handle();

    // Wire ctrl-c to trigger graceful shutdown.
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "failed to listen for ctrl-c");
            return;
        }
        info!("ctrl-c received, initiating shutdown");
        shutdown.trigger();
    });

    info!("dashboard: http://localhost:3001");
    info!("API server: http://localhost:{api_port}");
    daemon.run().await?;

    // After daemon stops, abort frontend server.
    frontend_handle.abort();
    info!("frontend server stopped");

    Ok(())
}

fn load_config(home: &str) -> Result<Config> {
    let path = std::path::Path::new(home)
        .join(".auto-tundra")
        .join("config.toml");
    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Config =
            toml::from_str(&content).context("failed to parse config.toml")?;
        Ok(config)
    } else {
        info!("no config file found at {}, using defaults", path.display());
        Ok(Config::default())
    }
}

/// Serve the Leptos dist/ directory as static files on port 3001.
async fn serve_frontend() {
    let _span = traced_span!("serve_frontend");
    
    use axum::Router;
    use tower_http::services::{ServeDir, ServeFile};

    let dist_dir = find_dist_dir();

    let app = Router::new().fallback_service(
        ServeDir::new(&dist_dir).fallback(ServeFile::new(dist_dir.join("index.html"))),
    );

    // Bind to IPv6 wildcard — on macOS dual-stack accepts both IPv4 and IPv6
    let listener = match tokio::net::TcpListener::bind("[::]:3001").await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "failed to bind frontend on port 3001");
            return;
        }
    };

    info!("frontend server listening on http://localhost:3001");
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!(error = %e, "frontend server error");
    }
}

fn find_dist_dir() -> std::path::PathBuf {
    let candidates = [
        std::path::PathBuf::from("app/leptos-ui/dist"),
        std::path::PathBuf::from("../app/leptos-ui/dist"),
        std::path::PathBuf::from("../../app/leptos-ui/dist"),
    ];

    for dir in &candidates {
        if dir.join("index.html").exists() {
            info!(path = %dir.display(), "found frontend dist directory");
            return dir.clone();
        }
    }

    tracing::warn!("frontend dist/ not found, falling back to app/leptos-ui/dist");
    candidates[0].clone()
}
