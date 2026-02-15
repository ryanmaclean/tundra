use anyhow::Context;
use at_core::cache::CacheDb;
use at_core::config::Config;

/// Run the `status` subcommand: load config, open cache, print KPI snapshot.
pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("failed to load config")?;

    at_telemetry::logging::init_logging("at-cli", &cfg.general.log_level);
    tracing::debug!(project = %cfg.general.project_name, "config loaded");

    // Resolve cache path, expanding `~` to the real home directory.
    let cache_path = expand_tilde(&cfg.cache.path);

    // Ensure the parent directory exists.
    if let Some(parent) = std::path::Path::new(&cache_path).parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let db = CacheDb::new(&cache_path)
        .await
        .with_context(|| format!("failed to open cache db at {cache_path}"))?;

    let kpi = db
        .compute_kpi_snapshot()
        .await
        .context("failed to compute KPI snapshot")?;

    println!("auto-tundra status  ({})", cfg.general.project_name);
    println!("{}", "-".repeat(40));
    println!("Total beads:    {}", kpi.total_beads);
    println!("  backlog:      {}", kpi.backlog);
    println!("  hooked:       {}", kpi.hooked);
    println!("  slung:        {}", kpi.slung);
    println!("  review:       {}", kpi.review);
    println!("  done:         {}", kpi.done);
    println!("  failed:       {}", kpi.failed);
    println!("  escalated:    {}", kpi.escalated);
    println!("Active agents:  {}", kpi.active_agents);
    println!("Snapshot at:    {}", kpi.timestamp.format("%Y-%m-%d %H:%M:%S UTC"));

    Ok(())
}

/// Expand a leading `~` or `~/` to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs::home_dir() {
            return path.replacen('~', &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}
