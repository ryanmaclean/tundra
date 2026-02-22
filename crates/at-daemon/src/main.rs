//! auto-tundra daemon — starts the API server, patrol loops, and
//! serves the Leptos WASM frontend.

use anyhow::{Context, Result};
use at_core::config::Config;
use tracing::info;

mod profiling;
mod metrics;
mod environment;
mod profiling_tests;
mod benchmarks;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
    let _main_span = traced_span!("main_execution", 
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
    profiling::record_event("daemon_startup", &[
        ("version", env!("CARGO_PKG_VERSION")),
        ("pid", &std::process::id().to_string()),
        ("architecture", std::env::consts::ARCH),
    ]);

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

    // Standalone mode: use port 9090 (unless overridden in config.toml)
    if config.daemon.port == 9876 {
        // Default was never overridden — use the canonical standalone port.
        config.daemon.port = 9090;
    }
    let api_port = config.daemon.port;

    // Spawn the Leptos static file server on port 3001
    let frontend_handle = tokio::spawn(serve_frontend());

    // Create and run the daemon
    let daemon = profile_async!("daemon_initialization", at_daemon::daemon::Daemon::new(config)).await?;
    
    // Record LLM profile bootstrap metrics after daemon creation
    let reg = at_intelligence::ResilientRegistry::from_config(&daemon.config());
    let total_count = reg.count();
    if let Some(best) = reg.registry.best_available() {
        let provider_name = format!("{:?}", best.provider);
        metrics::AppMetrics::llm_profile_bootstrap(
            total_count as u32,
            &best.name,
            &provider_name
        ).await;
    }
    
    let shutdown = daemon.shutdown_handle();

    // Wire ctrl-c to trigger graceful shutdown.
    tokio::spawn(async move {
        let _span = traced_span!("signal_handler", signal = "ctrl_c");
        
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "failed to listen for ctrl-c");
            profiling::record_event("signal_handler_error", &[("error", &e.to_string())]);
            return;
        }
        info!("ctrl-c received, initiating shutdown");
        profiling::record_event("shutdown_triggered", &[("signal", "ctrl_c")]);
        shutdown.trigger();
    });

    info!("dashboard: http://localhost:3001");
    info!("API server: http://localhost:{api_port}");
    profiling::record_event("daemon_ready", &[
        ("frontend_port", "3001"),
        ("api_port", &api_port.to_string())
    ]);
    
    if let Err(e) = profile_async!("daemon_main_loop", daemon.run()).await {
        tracing::error!(error = %e, "daemon execution failed");
        profiling::record_event("daemon_execution_error", &[("error", &e.to_string())]);
        return Err(e);
    }

    // After daemon stops, abort frontend server.
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
    let _span = traced_span!("serve_frontend", 
        port = 3001,
        component = "http_server"
    );
    
    profiling::record_event("frontend_server_start", &[("port", "3001")]);
    
    use axum::Router;
    use tower_http::services::{ServeDir, ServeFile};

    let dist_dir = find_dist_dir();
    
    profiling::add_span_tags(&[("dist_dir", dist_dir.to_str().unwrap_or("unknown"))]);

    let app = Router::new().fallback_service(
        ServeDir::new(&dist_dir).fallback(ServeFile::new(dist_dir.join("index.html"))),
    );

    // Bind to IPv6 wildcard — on macOS dual-stack accepts both IPv4 and IPv6
    let listener = match tokio::net::TcpListener::bind("[::]:3001").await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "failed to bind frontend on port 3001");
            profiling::record_event("frontend_server_bind_error", &[
                ("error", &e.to_string()),
                ("port", "3001")
            ]);
            return;
        }
    };

    info!("frontend server listening on http://localhost:3001");
    profiling::record_event("frontend_server_listening", &[
        ("port", "3001"),
        ("protocol", "http")
    ]);
    
    if let Err(e) = profile_async!("serve_frontend_requests", axum::serve(listener, app)).await {
        tracing::error!(error = %e, "frontend server error");
        profiling::record_event("frontend_server_error", &[
            ("error", &e.to_string())
        ]);
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
            profiling::record_event("frontend_dist_found", &[
                ("path", dir.to_str().unwrap_or("invalid")),
                ("candidate_index", &index.to_string())
            ]);
            return dir.clone();
        }
    }

    tracing::warn!("frontend dist/ not found, falling back to app/leptos-ui/dist");
    profiling::record_event("frontend_dist_fallback", &[
        ("fallback_path", "app/leptos-ui/dist")
    ]);
    candidates[0].clone()
}
