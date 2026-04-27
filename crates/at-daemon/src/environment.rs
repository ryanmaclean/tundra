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
        if !(0.0..=1.0).contains(&rate) {
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
    use std::sync::Mutex;

    // Mutex to serialize tests that modify environment variables
    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_env_vars() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        // Save original values to restore later
        let orig_service = env::var("DD_SERVICE").ok();
        let orig_env = env::var("DD_ENV").ok();
        let orig_version = env::var("DD_VERSION").ok();

        // Clear existing env vars
        env::remove_var("DD_SERVICE");
        env::remove_var("DD_ENV");
        env::remove_var("DD_VERSION");

        set_default_env_vars();

        assert_eq!(env::var("DD_SERVICE").unwrap(), "at-daemon");
        assert_eq!(env::var("DD_ENV").unwrap(), "development");
        assert_eq!(env::var("DD_VERSION").unwrap(), "0.1.0");

        // Restore original values
        if let Some(v) = orig_service {
            env::set_var("DD_SERVICE", v);
        } else {
            env::remove_var("DD_SERVICE");
        }
        if let Some(v) = orig_env {
            env::set_var("DD_ENV", v);
        } else {
            env::remove_var("DD_ENV");
        }
        if let Some(v) = orig_version {
            env::set_var("DD_VERSION", v);
        } else {
            env::remove_var("DD_VERSION");
        }
    }

    #[test]
    fn test_validate_environment() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();

        // Save original values
        let orig_service = env::var("DD_SERVICE").ok();
        let orig_env = env::var("DD_ENV").ok();
        let orig_version = env::var("DD_VERSION").ok();
        let orig_url = env::var("DD_TRACE_AGENT_URL").ok();

        env::set_var("DD_SERVICE", "test-service");
        env::set_var("DD_ENV", "test");
        env::set_var("DD_VERSION", "1.0.0");
        env::set_var("DD_TRACE_AGENT_URL", "http://localhost:8126");

        assert!(validate_environment().is_ok());

        env::remove_var("DD_SERVICE");
        assert!(validate_environment().is_err());

        // Restore original values
        if let Some(v) = orig_service {
            env::set_var("DD_SERVICE", v);
        } else {
            env::remove_var("DD_SERVICE");
        }
        if let Some(v) = orig_env {
            env::set_var("DD_ENV", v);
        } else {
            env::remove_var("DD_ENV");
        }
        if let Some(v) = orig_version {
            env::set_var("DD_VERSION", v);
        } else {
            env::remove_var("DD_VERSION");
        }
        if let Some(v) = orig_url {
            env::set_var("DD_TRACE_AGENT_URL", v);
        } else {
            env::remove_var("DD_TRACE_AGENT_URL");
        }
    }

    /// Capture the current value of an env var so we can restore it after a
    /// test mutates it. Returns the previous value (or `None` if it was unset).
    fn capture(var: &str) -> Option<String> {
        env::var(var).ok()
    }

    /// Restore a previously-captured env var to its original state.
    fn restore(var: &str, original: Option<String>) {
        match original {
            Some(v) => env::set_var(var, v),
            None => env::remove_var(var),
        }
    }

    /// List of env vars that the module under test reads or sets. Helper tests
    /// snapshot all of these and restore them at the end.
    const ALL_VARS: &[&str] = &[
        "DD_SERVICE",
        "DD_ENV",
        "DD_VERSION",
        "DD_TRACE_AGENT_URL",
        "DD_TRACE_AGENT_PORT",
        "DD_TRACE_ENABLED",
        "DD_TRACE_SAMPLE_RATE",
        "DD_PROFILING_ENABLED",
        "DD_API_KEY",
        "RUST_LOG",
    ];

    fn snapshot_all() -> Vec<(String, Option<String>)> {
        ALL_VARS
            .iter()
            .map(|v| (v.to_string(), capture(v)))
            .collect()
    }

    fn restore_all(snap: Vec<(String, Option<String>)>) {
        for (k, v) in snap {
            restore(&k, v);
        }
    }

    #[test]
    fn set_default_env_vars_preserves_preexisting_values() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        env::set_var("DD_SERVICE", "custom-service");
        env::set_var("DD_ENV", "staging");
        env::set_var("DD_VERSION", "9.9.9");

        set_default_env_vars();

        // Pre-existing values must NOT be overwritten by defaults.
        assert_eq!(env::var("DD_SERVICE").unwrap(), "custom-service");
        assert_eq!(env::var("DD_ENV").unwrap(), "staging");
        assert_eq!(env::var("DD_VERSION").unwrap(), "9.9.9");

        restore_all(snap);
    }

    #[test]
    fn set_default_env_vars_populates_all_expected_keys() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        for v in ALL_VARS.iter().filter(|v| **v != "DD_API_KEY") {
            env::remove_var(v);
        }

        set_default_env_vars();

        assert_eq!(
            env::var("DD_TRACE_AGENT_URL").unwrap(),
            "http://localhost:8126"
        );
        assert_eq!(env::var("DD_TRACE_AGENT_PORT").unwrap(), "8126");
        assert_eq!(env::var("DD_TRACE_ENABLED").unwrap(), "true");
        assert_eq!(env::var("DD_TRACE_SAMPLE_RATE").unwrap(), "1.0");
        assert_eq!(env::var("DD_PROFILING_ENABLED").unwrap(), "true");
        assert_eq!(env::var("RUST_LOG").unwrap(), "info,at_daemon=debug");

        restore_all(snap);
    }

    #[test]
    fn validate_environment_accepts_sample_rate_at_lower_bound() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        env::set_var("DD_SERVICE", "svc");
        env::set_var("DD_ENV", "test");
        env::set_var("DD_VERSION", "0.0.1");
        env::set_var("DD_TRACE_AGENT_URL", "http://localhost:8126");
        env::set_var("DD_TRACE_SAMPLE_RATE", "0.0");

        assert!(validate_environment().is_ok());

        restore_all(snap);
    }

    #[test]
    fn validate_environment_accepts_sample_rate_at_upper_bound() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        env::set_var("DD_SERVICE", "svc");
        env::set_var("DD_ENV", "test");
        env::set_var("DD_VERSION", "0.0.1");
        env::set_var("DD_TRACE_AGENT_URL", "http://localhost:8126");
        env::set_var("DD_TRACE_SAMPLE_RATE", "1.0");

        assert!(validate_environment().is_ok());

        restore_all(snap);
    }

    #[test]
    fn validate_environment_rejects_sample_rate_above_one() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        env::set_var("DD_SERVICE", "svc");
        env::set_var("DD_ENV", "test");
        env::set_var("DD_VERSION", "0.0.1");
        env::set_var("DD_TRACE_AGENT_URL", "http://localhost:8126");
        env::set_var("DD_TRACE_SAMPLE_RATE", "1.5");

        assert!(validate_environment().is_err());

        restore_all(snap);
    }

    #[test]
    fn validate_environment_rejects_sample_rate_below_zero() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        env::set_var("DD_SERVICE", "svc");
        env::set_var("DD_ENV", "test");
        env::set_var("DD_VERSION", "0.0.1");
        env::set_var("DD_TRACE_AGENT_URL", "http://localhost:8126");
        env::set_var("DD_TRACE_SAMPLE_RATE", "-0.1");

        assert!(validate_environment().is_err());

        restore_all(snap);
    }

    #[test]
    fn validate_environment_rejects_malformed_sample_rate() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        env::set_var("DD_SERVICE", "svc");
        env::set_var("DD_ENV", "test");
        env::set_var("DD_VERSION", "0.0.1");
        env::set_var("DD_TRACE_AGENT_URL", "http://localhost:8126");
        env::set_var("DD_TRACE_SAMPLE_RATE", "not-a-number");

        assert!(validate_environment().is_err());

        restore_all(snap);
    }

    #[test]
    fn validate_environment_treats_empty_sample_rate_as_invalid() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        env::set_var("DD_SERVICE", "svc");
        env::set_var("DD_ENV", "test");
        env::set_var("DD_VERSION", "0.0.1");
        env::set_var("DD_TRACE_AGENT_URL", "http://localhost:8126");
        env::set_var("DD_TRACE_SAMPLE_RATE", "");

        assert!(validate_environment().is_err());

        restore_all(snap);
    }

    #[test]
    fn validate_environment_omitting_sample_rate_is_ok() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        env::set_var("DD_SERVICE", "svc");
        env::set_var("DD_ENV", "test");
        env::set_var("DD_VERSION", "0.0.1");
        env::set_var("DD_TRACE_AGENT_URL", "http://localhost:8126");
        // No DD_TRACE_SAMPLE_RATE — code path should be skipped entirely.
        env::remove_var("DD_TRACE_SAMPLE_RATE");

        assert!(validate_environment().is_ok());

        restore_all(snap);
    }

    #[test]
    fn validate_environment_missing_each_required_var_fails() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        // All four required vars set → ok.
        env::set_var("DD_SERVICE", "svc");
        env::set_var("DD_ENV", "test");
        env::set_var("DD_VERSION", "0.0.1");
        env::set_var("DD_TRACE_AGENT_URL", "http://localhost:8126");
        env::remove_var("DD_TRACE_SAMPLE_RATE");
        assert!(validate_environment().is_ok());

        // Removing each in turn must fail validation.
        for required in ["DD_SERVICE", "DD_ENV", "DD_VERSION", "DD_TRACE_AGENT_URL"] {
            let saved = env::var(required).ok();
            env::remove_var(required);
            assert!(
                validate_environment().is_err(),
                "expected error when {required} is missing",
            );
            if let Some(v) = saved {
                env::set_var(required, v);
            }
        }

        restore_all(snap);
    }

    #[test]
    fn get_environment_returns_default_when_no_arg() {
        // get_environment uses std::env::args(), which during cargo test is
        // the test harness invocation. We assert it returns *some* non-empty
        // string — either an arg or the documented "development" default.
        let env = get_environment();
        assert!(!env.is_empty());
    }

    #[test]
    fn load_environment_config_does_not_error_on_missing_file() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        let snap = snapshot_all();

        // A name unlikely to correspond to any real environment file.
        let result = load_environment_config("nonexistent-env-for-tests");
        assert!(result.is_ok(), "expected graceful fallback, got {result:?}");

        // Defaults should still be set after fallback.
        assert!(env::var("DD_SERVICE").is_ok());
        assert!(env::var("DD_TRACE_AGENT_URL").is_ok());

        restore_all(snap);
    }
}
