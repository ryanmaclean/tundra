use anyhow::{Context, Result};
use std::env;
use tracing::{info, warn};

/// Load environment configuration for specific environment
pub fn load_environment_config(env: &str) -> Result<()> {
    let env_file = format!("environment/{}.env", env);

    // Try to load the environment file
    match dotenv::from_filename(&env_file) {
        Ok(_) => info!("Loaded environment configuration from {}", env_file),
        Err(e) => {
            warn!("Failed to load environment file {}: {}", env_file, e);
            info!("Falling back to environment variables and defaults");
        }
    }

    // Set required defaults
    set_default_env_vars();

    info!("Environment configuration loaded for: {}", env);
    log_datadog_config();

    Ok(())
}

/// Set default environment variables if not present
fn set_default_env_vars() {
    // Service identification
    env::set_var(
        "DD_SERVICE",
        env::var("DD_SERVICE").unwrap_or_else(|_| "at-daemon".to_string()),
    );
    env::set_var(
        "DD_ENV",
        env::var("DD_ENV").unwrap_or_else(|_| "development".to_string()),
    );
    env::set_var(
        "DD_VERSION",
        env::var("DD_VERSION").unwrap_or_else(|_| "0.1.0".to_string()),
    );

    // Agent configuration
    env::set_var(
        "DD_TRACE_AGENT_URL",
        env::var("DD_TRACE_AGENT_URL").unwrap_or_else(|_| "http://localhost:8126".to_string()),
    );
    env::set_var(
        "DD_TRACE_AGENT_PORT",
        env::var("DD_TRACE_AGENT_PORT").unwrap_or_else(|_| "8126".to_string()),
    );

    // Tracing configuration
    env::set_var(
        "DD_TRACE_ENABLED",
        env::var("DD_TRACE_ENABLED").unwrap_or_else(|_| "true".to_string()),
    );
    env::set_var(
        "DD_TRACE_SAMPLE_RATE",
        env::var("DD_TRACE_SAMPLE_RATE").unwrap_or_else(|_| "1.0".to_string()),
    );

    // Profiling configuration
    env::set_var(
        "DD_PROFILING_ENABLED",
        env::var("DD_PROFILING_ENABLED").unwrap_or_else(|_| "true".to_string()),
    );

    // Application configuration
    env::set_var(
        "RUST_LOG",
        env::var("RUST_LOG").unwrap_or_else(|_| "info,at_daemon=debug".to_string()),
    );
}

/// Log current Datadog configuration
fn log_datadog_config() {
    info!("Datadog Configuration:");
    info!("  Service: {}", env::var("DD_SERVICE").unwrap_or_default());
    info!("  Environment: {}", env::var("DD_ENV").unwrap_or_default());
    info!("  Version: {}", env::var("DD_VERSION").unwrap_or_default());
    info!(
        "  Agent URL: {}",
        env::var("DD_TRACE_AGENT_URL").unwrap_or_default()
    );
    info!(
        "  Trace Enabled: {}",
        env::var("DD_TRACE_ENABLED").unwrap_or_default()
    );
    info!(
        "  Sample Rate: {}",
        env::var("DD_TRACE_SAMPLE_RATE").unwrap_or_default()
    );
    info!(
        "  Profiling Enabled: {}",
        env::var("DD_PROFILING_ENABLED").unwrap_or_default()
    );

    if let Ok(api_key) = env::var("DD_API_KEY") {
        info!(
            "  API Key: {}***",
            &api_key[..std::cmp::min(8, api_key.len())]
        );
    } else {
        info!("  API Key: Not set (local agent mode)");
    }
}

/// Validate required environment variables
pub fn validate_environment() -> Result<()> {
    let required_vars = vec!["DD_SERVICE", "DD_ENV", "DD_VERSION", "DD_TRACE_AGENT_URL"];

    for var in required_vars {
        if env::var(var).is_err() {
            return Err(anyhow::anyhow!(
                "Required environment variable {} is not set",
                var
            ));
        }
    }

    // Validate sample rate
    if let Ok(sample_rate) = env::var("DD_TRACE_SAMPLE_RATE") {
        let rate: f64 = sample_rate
            .parse()
            .context("DD_TRACE_SAMPLE_RATE must be a valid number")?;
        if rate < 0.0 || rate > 1.0 {
            return Err(anyhow::anyhow!(
                "DD_TRACE_SAMPLE_RATE must be between 0.0 and 1.0"
            ));
        }
    }

    info!("Environment validation passed");
    Ok(())
}

/// Get current environment from command line args or default
pub fn get_environment() -> String {
    env::args()
        .nth(1)
        .unwrap_or_else(|| "development".to_string())
}

/// Configure application based on environment
pub fn configure_app() -> Result<()> {
    let env = get_environment();

    info!("Configuring application for environment: {}", env);

    load_environment_config(&env)?;
    validate_environment()?;

    match env.as_str() {
        "production" => {
            info!("Production configuration applied");
            env::set_var("RUST_LOG", "info,at_daemon=warn");
        }
        "staging" => {
            info!("Staging configuration applied");
            env::set_var("RUST_LOG", "info,at_daemon=debug");
        }
        "development" => {
            info!("Development configuration applied");
            env::set_var("RUST_LOG", "info,at_daemon=debug,at_core=debug");
            env::set_var("DD_TRACE_DEBUG", "true");
        }
        _ => {
            warn!("Unknown environment: {}, using development defaults", env);
            env::set_var("RUST_LOG", "info,at_daemon=debug");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_default_env_vars() {
        // Clear existing env vars
        env::remove_var("DD_SERVICE");
        env::remove_var("DD_ENV");
        env::remove_var("DD_VERSION");

        set_default_env_vars();

        assert_eq!(env::var("DD_SERVICE").unwrap(), "at-daemon");
        assert_eq!(env::var("DD_ENV").unwrap(), "development");
        assert_eq!(env::var("DD_VERSION").unwrap(), "0.1.0");
    }

    #[test]
    fn test_validate_environment() {
        env::set_var("DD_SERVICE", "test-service");
        env::set_var("DD_ENV", "test");
        env::set_var("DD_VERSION", "1.0.0");
        env::set_var("DD_TRACE_AGENT_URL", "http://localhost:8126");

        assert!(validate_environment().is_ok());

        env::remove_var("DD_SERVICE");
        assert!(validate_environment().is_err());
    }
}
