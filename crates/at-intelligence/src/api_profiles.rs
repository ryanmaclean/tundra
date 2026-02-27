//! API Profiles — multi-provider endpoint configuration and failover.
//!
//! Supports:
//! - **Anthropic** (direct API)
//! - **OpenRouter** (400+ models, unified API)
//! - **Custom** (any Anthropic-compatible endpoint)
//! - **Account failover**: Automatic switching on rate limits or errors
//! - **Cost tracking**: Per-profile usage and spend tracking
//! - **Circuit breaker**: Per-provider fault isolation with auto-failover
//! - **Rate limiting**: Per-provider token-bucket rate limiting

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use at_harness::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState,
};
use at_harness::rate_limiter::{RateLimitConfig, RateLimitError, RateLimiter};

// ---------------------------------------------------------------------------
// ApiProfile — a configured API endpoint
// ---------------------------------------------------------------------------

/// A configured API profile for an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiProfile {
    pub id: Uuid,
    pub name: String,
    pub provider: ProviderKind,
    /// Base URL for the API (e.g., "<https://api.anthropic.com>").
    pub base_url: String,
    /// Name of the environment variable holding the API key.
    pub api_key_env: String,
    /// Default model ID to use with this profile.
    pub default_model: String,
    /// Maximum requests per minute.
    pub rate_limit_rpm: Option<u32>,
    /// Maximum tokens per minute.
    pub rate_limit_tpm: Option<u32>,
    /// Priority for failover (lower = higher priority).
    pub priority: u32,
    /// Whether this profile is enabled.
    pub enabled: bool,
    /// Custom headers to send with requests.
    pub custom_headers: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Anthropic,
    OpenRouter,
    OpenAi,
    /// Local inference server (vllm.rs, llama.cpp, Ollama, candle, etc.).
    ///
    /// Speaks OpenAI-compatible chat completions protocol to localhost.
    /// API keys are optional for many local servers.
    Local,
    Custom,
}

impl ProviderKind {
    pub fn default_base_url(&self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "https://api.anthropic.com",
            ProviderKind::OpenRouter => "https://openrouter.ai/api",
            ProviderKind::OpenAi => "https://api.openai.com",
            ProviderKind::Local => "http://127.0.0.1:11434",
            ProviderKind::Custom => "http://localhost:8080",
        }
    }

    pub fn default_api_key_env(&self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "ANTHROPIC_API_KEY",
            ProviderKind::OpenRouter => "OPENROUTER_API_KEY",
            ProviderKind::OpenAi => "OPENAI_API_KEY",
            ProviderKind::Local => "LOCAL_API_KEY",
            ProviderKind::Custom => "CUSTOM_API_KEY",
        }
    }
}

impl ApiProfile {
    pub fn new(name: impl Into<String>, provider: ProviderKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            provider,
            base_url: provider.default_base_url().into(),
            api_key_env: provider.default_api_key_env().into(),
            default_model: default_model_for(provider),
            rate_limit_rpm: None,
            rate_limit_tpm: None,
            priority: 0,
            enabled: true,
            custom_headers: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Check if an API key is available in the environment.
    pub fn has_api_key(&self) -> bool {
        if self.provider == ProviderKind::Local {
            // Local inference typically runs without auth. If an env var is
            // configured it's treated as optional, not required.
            return true;
        }
        std::env::var(&self.api_key_env).is_ok()
    }

    /// Build a local provider profile from core providers config.
    pub fn local_from_providers(
        name: impl Into<String>,
        providers: &at_core::config::ProvidersConfig,
    ) -> Self {
        let mut profile = Self::new(name, ProviderKind::Local);
        profile.base_url = providers.local_base_url.clone();
        profile.default_model = providers.local_model.clone();
        profile.api_key_env = providers.local_api_key_env.clone();
        profile
    }
}

fn default_model_for(provider: ProviderKind) -> String {
    match provider {
        ProviderKind::Anthropic => "claude-sonnet-4-20250514".into(),
        ProviderKind::OpenRouter => "anthropic/claude-sonnet-4-20250514".into(),
        ProviderKind::OpenAi => "gpt-4o".into(),
        ProviderKind::Local => "qwen2.5-coder:14b".into(),
        ProviderKind::Custom => "default".into(),
    }
}

// ---------------------------------------------------------------------------
// ProfileUsage — per-profile usage tracking
// ---------------------------------------------------------------------------

/// Usage metrics for an API profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileUsage {
    pub profile_id: Uuid,
    pub total_requests: u64,
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    pub total_errors: u64,
    pub total_rate_limits: u64,
    /// Estimated spend in USD.
    pub estimated_spend_usd: f64,
    pub last_used: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

impl ProfileUsage {
    pub fn new(profile_id: Uuid) -> Self {
        Self {
            profile_id,
            total_requests: 0,
            total_tokens_in: 0,
            total_tokens_out: 0,
            total_errors: 0,
            total_rate_limits: 0,
            estimated_spend_usd: 0.0,
            last_used: None,
            last_error: None,
        }
    }

    /// Record a successful request.
    pub fn record_success(&mut self, tokens_in: u64, tokens_out: u64, cost_usd: f64) {
        self.total_requests += 1;
        self.total_tokens_in += tokens_in;
        self.total_tokens_out += tokens_out;
        self.estimated_spend_usd += cost_usd;
        self.last_used = Some(Utc::now());
    }

    /// Record an error (counts as a request for error-rate purposes).
    pub fn record_error(&mut self, error: impl Into<String>) {
        self.total_requests += 1;
        self.total_errors += 1;
        self.last_error = Some(error.into());
    }

    /// Record a rate limit hit.
    pub fn record_rate_limit(&mut self) {
        self.total_rate_limits += 1;
    }

    /// Error rate as a fraction.
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_errors as f64 / self.total_requests as f64
        }
    }
}

// ---------------------------------------------------------------------------
// ProfileRegistry — manages API profiles with failover
// ---------------------------------------------------------------------------

/// Registry of API profiles with automatic failover.
pub struct ProfileRegistry {
    profiles: HashMap<Uuid, ApiProfile>,
    usage: HashMap<Uuid, ProfileUsage>,
    /// Profiles sorted by priority for failover.
    priority_order: Vec<Uuid>,
}

impl ProfileRegistry {
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
            usage: HashMap::new(),
            priority_order: Vec::new(),
        }
    }

    /// Add a profile.
    pub fn add_profile(&mut self, profile: ApiProfile) -> Uuid {
        let id = profile.id;
        self.usage.insert(id, ProfileUsage::new(id));
        self.profiles.insert(id, profile);
        self.rebuild_priority_order();
        id
    }

    /// Get a profile by ID.
    pub fn get_profile(&self, id: &Uuid) -> Option<&ApiProfile> {
        self.profiles.get(id)
    }

    /// Get a profile by name.
    pub fn get_by_name(&self, name: &str) -> Option<&ApiProfile> {
        self.profiles.values().find(|p| p.name == name)
    }

    /// Get usage for a profile.
    pub fn get_usage(&self, id: &Uuid) -> Option<&ProfileUsage> {
        self.usage.get(id)
    }

    /// Get mutable usage for a profile.
    pub fn get_usage_mut(&mut self, id: &Uuid) -> Option<&mut ProfileUsage> {
        self.usage.get_mut(id)
    }

    /// List all profiles ordered by priority.
    pub fn list_profiles(&self) -> Vec<&ApiProfile> {
        self.priority_order
            .iter()
            .filter_map(|id| self.profiles.get(id))
            .collect()
    }

    /// Get the best available profile (enabled, has API key, lowest error rate).
    pub fn best_available(&self) -> Option<&ApiProfile> {
        for id in &self.priority_order {
            if let Some(profile) = self.profiles.get(id) {
                if profile.enabled && profile.has_api_key() {
                    // Check error rate isn't too high
                    if let Some(usage) = self.usage.get(id) {
                        if usage.error_rate() < 0.5 || usage.total_requests < 5 {
                            return Some(profile);
                        }
                    } else {
                        return Some(profile);
                    }
                }
            }
        }
        None
    }

    /// Get the next failover profile (skip the given profile).
    pub fn failover_for(&self, current_id: &Uuid) -> Option<&ApiProfile> {
        let mut found_current = false;
        for id in &self.priority_order {
            if id == current_id {
                found_current = true;
                continue;
            }
            if found_current {
                if let Some(profile) = self.profiles.get(id) {
                    if profile.enabled && profile.has_api_key() {
                        return Some(profile);
                    }
                }
            }
        }
        None
    }

    /// Remove a profile.
    pub fn remove_profile(&mut self, id: &Uuid) -> Option<ApiProfile> {
        self.usage.remove(id);
        let result = self.profiles.remove(id);
        if result.is_some() {
            self.rebuild_priority_order();
        }
        result
    }

    /// Number of profiles.
    pub fn count(&self) -> usize {
        self.profiles.len()
    }

    /// Enable or disable a profile.
    pub fn set_enabled(&mut self, id: &Uuid, enabled: bool) -> bool {
        if let Some(profile) = self.profiles.get_mut(id) {
            profile.enabled = enabled;
            true
        } else {
            false
        }
    }

    fn rebuild_priority_order(&mut self) {
        let mut entries: Vec<(Uuid, u32)> =
            self.profiles.values().map(|p| (p.id, p.priority)).collect();
        entries.sort_by_key(|(_, priority)| *priority);
        self.priority_order = entries.into_iter().map(|(id, _)| id).collect();
    }
}

impl Default for ProfileRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ProviderState — per-provider circuit breaker and rate limiter
// ---------------------------------------------------------------------------

/// Per-provider resilience state: circuit breaker for fault isolation and
/// rate limiter for respecting API quotas.
pub struct ProviderState {
    pub profile: ApiProfile,
    pub breaker: CircuitBreaker,
    pub rpm_limiter: Option<RateLimiter>,
    pub tpm_limiter: Option<RateLimiter>,
}

impl ProviderState {
    /// Create a new `ProviderState` from an `ApiProfile` with default circuit
    /// breaker settings (5 failures to open, 60s timeout, 30s call timeout).
    pub fn new(profile: ApiProfile) -> Self {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            call_timeout: Duration::from_secs(30),
        });

        let rpm_limiter = profile
            .rate_limit_rpm
            .map(|rpm| RateLimiter::new(RateLimitConfig::per_minute(rpm as u64)));

        let tpm_limiter = profile
            .rate_limit_tpm
            .map(|tpm| RateLimiter::new(RateLimitConfig::per_minute(tpm as u64)));

        Self {
            profile,
            breaker,
            rpm_limiter,
            tpm_limiter,
        }
    }

    /// Create with a custom circuit breaker config.
    pub fn with_breaker_config(profile: ApiProfile, config: CircuitBreakerConfig) -> Self {
        let breaker = CircuitBreaker::new(config);

        let rpm_limiter = profile
            .rate_limit_rpm
            .map(|rpm| RateLimiter::new(RateLimitConfig::per_minute(rpm as u64)));

        let tpm_limiter = profile
            .rate_limit_tpm
            .map(|tpm| RateLimiter::new(RateLimitConfig::per_minute(tpm as u64)));

        Self {
            profile,
            breaker,
            rpm_limiter,
            tpm_limiter,
        }
    }

    /// Check the rate limiter for a single request. Returns `Ok(())` if allowed,
    /// or `Err` with the retry-after duration.
    pub fn check_rate_limit(&self) -> Result<(), RateLimitError> {
        if let Some(ref limiter) = self.rpm_limiter {
            limiter.check(&self.profile.name)?;
        }
        Ok(())
    }

    /// Check the token-per-minute rate limiter with a token cost.
    pub fn check_token_rate_limit(&self, token_cost: f64) -> Result<(), RateLimitError> {
        if let Some(ref limiter) = self.tpm_limiter {
            limiter.check_with_cost(&self.profile.name, token_cost)?;
        }
        Ok(())
    }

    /// Check if the circuit breaker is currently open (rejecting calls).
    pub async fn is_circuit_open(&self) -> bool {
        self.breaker.state().await == CircuitState::Open
    }
}

// ---------------------------------------------------------------------------
// ResilientRegistry — ProfileRegistry + CircuitBreaker + RateLimiter
// ---------------------------------------------------------------------------

/// A registry that combines `ProfileRegistry` with per-provider circuit
/// breakers and rate limiters. Provides `call_with_failover` for resilient
/// LLM API calls.
pub struct ResilientRegistry {
    pub registry: ProfileRegistry,
    states: HashMap<Uuid, ProviderState>,
}

/// Error type for resilient call operations.
#[derive(Debug, thiserror::Error)]
pub enum ResilientCallError {
    #[error("circuit breaker: {0}")]
    CircuitBreaker(#[from] CircuitBreakerError),

    #[error("rate limited: {0}")]
    RateLimited(#[from] RateLimitError),

    #[error("all providers exhausted")]
    AllProvidersExhausted,

    #[error("inner error: {0}")]
    Inner(String),
}

impl ResilientRegistry {
    pub fn new() -> Self {
        Self {
            registry: ProfileRegistry::new(),
            states: HashMap::new(),
        }
    }

    /// Build a profile registry from runtime config.
    ///
    /// Bootstrap order:
    /// 1. Local provider profile from `providers.*` (priority 0)
    /// 2. Anthropic/OpenAI defaults (with env overrides from providers config)
    /// 3. Custom entries from `api_profiles.profiles`
    pub fn from_config(config: &at_core::config::Config) -> Self {
        let mut reg = Self::new();

        let mut local = ApiProfile::local_from_providers("local-runtime", &config.providers);
        local.priority = 0;
        reg.add_profile(local);

        let mut anthropic = ApiProfile::new("anthropic-primary", ProviderKind::Anthropic);
        anthropic.priority = 10;
        if let Some(env) = &config.providers.anthropic_key_env {
            anthropic.api_key_env = env.clone();
        }
        reg.add_profile(anthropic);

        let mut openai = ApiProfile::new("openai-primary", ProviderKind::OpenAi);
        openai.priority = 20;
        if let Some(env) = &config.providers.openai_key_env {
            openai.api_key_env = env.clone();
        }
        reg.add_profile(openai);

        for (idx, entry) in config.api_profiles.profiles.iter().enumerate() {
            let name = if entry.name.trim().is_empty() {
                format!("custom-{}", idx + 1)
            } else {
                entry.name.clone()
            };
            let mut profile = ApiProfile::new(name, ProviderKind::Custom);
            profile.priority = 100 + idx as u32;
            if !entry.base_url.trim().is_empty() {
                profile.base_url = entry.base_url.clone();
            }
            if !entry.api_key_env.trim().is_empty() {
                profile.api_key_env = entry.api_key_env.clone();
            }
            reg.add_profile(profile);
        }

        reg
    }

    /// Add a profile with default resilience settings.
    pub fn add_profile(&mut self, profile: ApiProfile) -> Uuid {
        let id = profile.id;
        let state = ProviderState::new(profile.clone());
        self.states.insert(id, state);
        self.registry.add_profile(profile);
        id
    }

    /// Add a profile with a custom circuit breaker config.
    pub fn add_profile_with_config(
        &mut self,
        profile: ApiProfile,
        breaker_config: CircuitBreakerConfig,
    ) -> Uuid {
        let id = profile.id;
        let state = ProviderState::with_breaker_config(profile.clone(), breaker_config);
        self.states.insert(id, state);
        self.registry.add_profile(profile);
        id
    }

    /// Remove a profile and its resilience state.
    pub fn remove_profile(&mut self, id: &Uuid) -> Option<ApiProfile> {
        self.states.remove(id);
        self.registry.remove_profile(id)
    }

    /// Get the provider state for a profile.
    pub fn get_state(&self, id: &Uuid) -> Option<&ProviderState> {
        self.states.get(id)
    }

    /// Get the provider state mutably.
    pub fn get_state_mut(&mut self, id: &Uuid) -> Option<&mut ProviderState> {
        self.states.get_mut(id)
    }

    /// Execute `f` through the best available provider's circuit breaker and
    /// rate limiter. On failure (circuit open, rate limited, or inner error),
    /// automatically fails over to the next priority provider.
    ///
    /// Returns the result of the first successful call, or
    /// `ResilientCallError::AllProvidersExhausted` if every provider was
    /// tried and failed.
    pub async fn call_with_failover<F, Fut, T, E>(
        &self,
        mut make_call: F,
    ) -> Result<(Uuid, T), ResilientCallError>
    where
        F: FnMut(&ApiProfile) -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let profiles = self.registry.list_profiles();
        for profile in profiles {
            if !profile.enabled || !profile.has_api_key() {
                continue;
            }

            let state = match self.states.get(&profile.id) {
                Some(s) => s,
                None => continue,
            };

            // Check rate limiter first (sync check, no async needed).
            if let Err(_rate_err) = state.check_rate_limit() {
                tracing::warn!(
                    profile = %profile.name,
                    "rate limit exceeded, trying next provider"
                );
                continue;
            }

            // Attempt the call through the circuit breaker.
            let profile_clone = profile.clone();
            let result = state.breaker.call(|| make_call(&profile_clone)).await;

            match result {
                Ok(value) => return Ok((profile.id, value)),
                Err(CircuitBreakerError::Open) => {
                    tracing::warn!(
                        profile = %profile.name,
                        "circuit open, trying next provider"
                    );
                    continue;
                }
                Err(CircuitBreakerError::Timeout(d)) => {
                    tracing::warn!(
                        profile = %profile.name,
                        timeout = ?d,
                        "call timed out, trying next provider"
                    );
                    continue;
                }
                Err(CircuitBreakerError::Inner(msg)) => {
                    tracing::warn!(
                        profile = %profile.name,
                        error = %msg,
                        "call failed, trying next provider"
                    );
                    continue;
                }
            }
        }

        Err(ResilientCallError::AllProvidersExhausted)
    }

    /// Number of profiles in the registry.
    pub fn count(&self) -> usize {
        self.registry.count()
    }

    /// Reset the circuit breaker for a specific profile.
    pub async fn reset_breaker(&self, id: &Uuid) {
        if let Some(state) = self.states.get(id) {
            state.breaker.reset().await;
        }
    }
}

impl Default for ResilientRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that modify CUSTOM_API_KEY environment variable
    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn api_profile_creation() {
        let profile = ApiProfile::new("main", ProviderKind::Anthropic);
        assert_eq!(profile.name, "main");
        assert_eq!(profile.provider, ProviderKind::Anthropic);
        assert!(profile.base_url.contains("anthropic"));
        assert!(profile.enabled);
    }

    #[test]
    fn provider_defaults() {
        assert!(ProviderKind::Anthropic
            .default_base_url()
            .contains("anthropic"));
        assert!(ProviderKind::OpenRouter
            .default_base_url()
            .contains("openrouter"));
        assert_eq!(
            ProviderKind::Anthropic.default_api_key_env(),
            "ANTHROPIC_API_KEY"
        );
        assert_eq!(ProviderKind::Local.default_api_key_env(), "LOCAL_API_KEY");
    }

    #[test]
    fn local_profile_does_not_require_api_key() {
        std::env::remove_var("LOCAL_API_KEY");
        let profile = ApiProfile::new("local", ProviderKind::Local);
        assert!(profile.has_api_key());
    }

    #[test]
    fn local_profile_from_providers_config() {
        let cfg = at_core::config::ProvidersConfig {
            local_base_url: "http://127.0.0.1:11434".into(),
            local_model: "qwen2.5-coder-7b".into(),
            local_api_key_env: "LOCAL_LLM_API_KEY".into(),
            ..Default::default()
        };

        let profile = ApiProfile::local_from_providers("local-dev", &cfg);
        assert_eq!(profile.provider, ProviderKind::Local);
        assert_eq!(profile.base_url, "http://127.0.0.1:11434");
        assert_eq!(profile.default_model, "qwen2.5-coder-7b");
        assert_eq!(profile.api_key_env, "LOCAL_LLM_API_KEY");
    }

    #[test]
    fn resilient_registry_from_config_bootstraps_local_and_defaults() {
        let mut cfg = at_core::config::Config::default();
        cfg.providers.local_base_url = "http://127.0.0.1:9000".into();
        cfg.providers.local_model = "qwen2.5-coder-32b".into();
        cfg.providers.local_api_key_env = "LOCAL_RUNTIME_KEY".into();
        cfg.providers.anthropic_key_env = Some("ANTHROPIC_PROD_KEY".into());
        cfg.providers.openai_key_env = Some("OPENAI_PROD_KEY".into());

        let reg = ResilientRegistry::from_config(&cfg);
        assert!(reg.count() >= 3);

        let local = reg.registry.get_by_name("local-runtime").unwrap();
        assert_eq!(local.provider, ProviderKind::Local);
        assert_eq!(local.base_url, "http://127.0.0.1:9000");
        assert_eq!(local.default_model, "qwen2.5-coder-32b");
        assert_eq!(local.api_key_env, "LOCAL_RUNTIME_KEY");

        let anthropic = reg.registry.get_by_name("anthropic-primary").unwrap();
        assert_eq!(anthropic.api_key_env, "ANTHROPIC_PROD_KEY");
        let openai = reg.registry.get_by_name("openai-primary").unwrap();
        assert_eq!(openai.api_key_env, "OPENAI_PROD_KEY");
    }

    #[test]
    fn resilient_registry_from_config_imports_custom_profiles() {
        let mut cfg = at_core::config::Config::default();
        cfg.api_profiles.profiles = vec![
            at_core::config::ApiProfileEntry {
                name: "edge-a".into(),
                base_url: "https://edge-a.example/v1".into(),
                api_key_env: "EDGE_A_KEY".into(),
            },
            at_core::config::ApiProfileEntry {
                name: String::new(),
                base_url: "https://fallback.example/v1".into(),
                api_key_env: "FALLBACK_KEY".into(),
            },
        ];

        let reg = ResilientRegistry::from_config(&cfg);
        let edge = reg.registry.get_by_name("edge-a").unwrap();
        assert_eq!(edge.provider, ProviderKind::Custom);
        assert_eq!(edge.base_url, "https://edge-a.example/v1");
        assert_eq!(edge.api_key_env, "EDGE_A_KEY");

        let auto_named = reg.registry.get_by_name("custom-2").unwrap();
        assert_eq!(auto_named.provider, ProviderKind::Custom);
        assert_eq!(auto_named.base_url, "https://fallback.example/v1");
        assert_eq!(auto_named.api_key_env, "FALLBACK_KEY");
    }

    #[test]
    fn profile_registry_add_and_get() {
        let mut reg = ProfileRegistry::new();
        let profile = ApiProfile::new("test", ProviderKind::Anthropic);
        let id = reg.add_profile(profile);

        assert_eq!(reg.count(), 1);
        assert!(reg.get_profile(&id).is_some());
    }

    #[test]
    fn profile_registry_get_by_name() {
        let mut reg = ProfileRegistry::new();
        reg.add_profile(ApiProfile::new("prod", ProviderKind::Anthropic));

        assert!(reg.get_by_name("prod").is_some());
        assert!(reg.get_by_name("nonexistent").is_none());
    }

    #[test]
    fn profile_registry_list_by_priority() {
        let mut reg = ProfileRegistry::new();

        let mut low = ApiProfile::new("low", ProviderKind::Custom);
        low.priority = 10;
        let mut high = ApiProfile::new("high", ProviderKind::Anthropic);
        high.priority = 0;

        reg.add_profile(low);
        reg.add_profile(high);

        let list = reg.list_profiles();
        assert_eq!(list[0].name, "high");
        assert_eq!(list[1].name, "low");
    }

    #[test]
    fn profile_registry_remove() {
        let mut reg = ProfileRegistry::new();
        let profile = ApiProfile::new("temp", ProviderKind::Custom);
        let id = reg.add_profile(profile);

        assert!(reg.remove_profile(&id).is_some());
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn profile_registry_set_enabled() {
        let mut reg = ProfileRegistry::new();
        let profile = ApiProfile::new("test", ProviderKind::Custom);
        let id = reg.add_profile(profile);

        assert!(reg.set_enabled(&id, false));
        assert!(!reg.get_profile(&id).unwrap().enabled);
    }

    #[test]
    fn profile_usage_tracking() {
        let mut usage = ProfileUsage::new(Uuid::new_v4());
        assert_eq!(usage.total_requests, 0);
        assert_eq!(usage.error_rate(), 0.0);

        usage.record_success(100, 200, 0.01);
        assert_eq!(usage.total_requests, 1);
        assert_eq!(usage.total_tokens_in, 100);
        assert_eq!(usage.total_tokens_out, 200);

        usage.record_error("timeout");
        assert_eq!(usage.total_errors, 1);
        assert_eq!(usage.error_rate(), 0.5);

        usage.record_rate_limit();
        assert_eq!(usage.total_rate_limits, 1);
    }

    #[test]
    fn profile_usage_serialization() {
        let usage = ProfileUsage::new(Uuid::new_v4());
        let json = serde_json::to_string(&usage).unwrap();
        let deser: ProfileUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.total_requests, 0);
    }

    #[test]
    fn api_profile_serialization() {
        let profile = ApiProfile::new("test", ProviderKind::OpenRouter);
        let json = serde_json::to_string(&profile).unwrap();
        let deser: ApiProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "test");
        assert_eq!(deser.provider, ProviderKind::OpenRouter);
    }

    #[test]
    fn failover_skips_current() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        // Require API key so failover_for returns a candidate
        std::env::set_var("CUSTOM_API_KEY", "test-key");
        let mut reg = ProfileRegistry::new();
        let mut p1 = ApiProfile::new("primary", ProviderKind::Custom);
        p1.priority = 0;
        let mut p2 = ApiProfile::new("secondary", ProviderKind::Custom);
        p2.priority = 1;

        let id1 = reg.add_profile(p1);
        reg.add_profile(p2);

        // Failover from primary should give secondary
        let failover = reg.failover_for(&id1);
        std::env::remove_var("CUSTOM_API_KEY");
        assert!(failover.is_some());
        assert_eq!(failover.unwrap().name, "secondary");
    }

    #[test]
    fn failover_none_when_no_alternatives() {
        let mut reg = ProfileRegistry::new();
        let p = ApiProfile::new("only", ProviderKind::Custom);
        let id = reg.add_profile(p);

        assert!(reg.failover_for(&id).is_none());
    }

    // -----------------------------------------------------------------------
    // ProviderState tests
    // -----------------------------------------------------------------------

    #[test]
    fn provider_state_creates_breaker_and_limiters() {
        let mut profile = ApiProfile::new("test", ProviderKind::Custom);
        profile.rate_limit_rpm = Some(100);
        profile.rate_limit_tpm = Some(50_000);

        let state = ProviderState::new(profile);
        assert!(state.rpm_limiter.is_some());
        assert!(state.tpm_limiter.is_some());
    }

    #[test]
    fn provider_state_no_limiters_when_none() {
        let profile = ApiProfile::new("test", ProviderKind::Custom);
        let state = ProviderState::new(profile);
        assert!(state.rpm_limiter.is_none());
        assert!(state.tpm_limiter.is_none());
    }

    #[test]
    fn provider_state_check_rate_limit_passes_without_limiter() {
        let profile = ApiProfile::new("test", ProviderKind::Custom);
        let state = ProviderState::new(profile);
        assert!(state.check_rate_limit().is_ok());
    }

    #[test]
    fn provider_state_check_rate_limit_with_rpm() {
        let mut profile = ApiProfile::new("test", ProviderKind::Custom);
        profile.rate_limit_rpm = Some(1000);
        let state = ProviderState::new(profile);

        // Should pass on first check.
        assert!(state.check_rate_limit().is_ok());
    }

    #[test]
    fn provider_state_check_token_rate_limit() {
        let mut profile = ApiProfile::new("test", ProviderKind::Custom);
        profile.rate_limit_tpm = Some(100);
        let state = ProviderState::new(profile);

        // First check with small cost should pass.
        assert!(state.check_token_rate_limit(1.0).is_ok());
    }

    #[tokio::test]
    async fn provider_state_circuit_starts_closed() {
        let profile = ApiProfile::new("test", ProviderKind::Custom);
        let state = ProviderState::new(profile);
        assert!(!state.is_circuit_open().await);
    }

    #[test]
    fn provider_state_custom_breaker_config() {
        let profile = ApiProfile::new("test", ProviderKind::Custom);
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 1,
            timeout: Duration::from_secs(30),
            call_timeout: Duration::from_secs(10),
        };
        let _state = ProviderState::with_breaker_config(profile, config);
        // Just verify it doesn't panic.
    }

    // -----------------------------------------------------------------------
    // ResilientRegistry tests
    // -----------------------------------------------------------------------

    #[test]
    fn resilient_registry_add_and_count() {
        let mut reg = ResilientRegistry::new();
        let profile = ApiProfile::new("test", ProviderKind::Custom);
        let id = reg.add_profile(profile);

        assert_eq!(reg.count(), 1);
        assert!(reg.get_state(&id).is_some());
    }

    #[test]
    fn resilient_registry_remove() {
        let mut reg = ResilientRegistry::new();
        let profile = ApiProfile::new("test", ProviderKind::Custom);
        let id = reg.add_profile(profile);

        assert!(reg.remove_profile(&id).is_some());
        assert_eq!(reg.count(), 0);
        assert!(reg.get_state(&id).is_none());
    }

    #[test]
    fn resilient_registry_add_with_config() {
        let mut reg = ResilientRegistry::new();
        let profile = ApiProfile::new("test", ProviderKind::Custom);
        let config = CircuitBreakerConfig {
            failure_threshold: 10,
            success_threshold: 3,
            timeout: Duration::from_secs(120),
            call_timeout: Duration::from_secs(60),
        };
        let id = reg.add_profile_with_config(profile, config);
        assert!(reg.get_state(&id).is_some());
    }

    #[tokio::test]
    async fn resilient_registry_call_with_failover_success() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var("CUSTOM_API_KEY", "test-key");
        let mut reg = ResilientRegistry::new();
        let mut p = ApiProfile::new("primary", ProviderKind::Custom);
        p.priority = 0;
        reg.add_profile(p);

        let result = reg
            .call_with_failover(|profile| {
                let name = profile.name.clone();
                async move { Ok::<String, String>(format!("hello from {}", name)) }
            })
            .await;

        std::env::remove_var("CUSTOM_API_KEY");
        assert!(result.is_ok());
        let (_, value) = result.unwrap();
        assert_eq!(value, "hello from primary");
    }

    #[tokio::test]
    async fn resilient_registry_call_with_failover_to_secondary() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var("CUSTOM_API_KEY", "test-key");
        let mut reg = ResilientRegistry::new();

        let mut p1 = ApiProfile::new("primary", ProviderKind::Custom);
        p1.priority = 0;
        reg.add_profile(p1);

        let mut p2 = ApiProfile::new("secondary", ProviderKind::Custom);
        p2.priority = 1;
        let id2 = reg.add_profile(p2);

        // Make a call that fails for "primary" but succeeds for "secondary".
        let result = reg
            .call_with_failover(|profile| {
                let name = profile.name.clone();
                async move {
                    if name == "primary" {
                        Err::<String, String>("primary is down".into())
                    } else {
                        Ok(format!("hello from {}", name))
                    }
                }
            })
            .await;

        std::env::remove_var("CUSTOM_API_KEY");
        assert!(result.is_ok());
        let (used_id, value) = result.unwrap();
        assert_eq!(used_id, id2);
        assert_eq!(value, "hello from secondary");
    }

    #[tokio::test]
    async fn resilient_registry_all_providers_exhausted() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var("CUSTOM_API_KEY", "test-key");
        let mut reg = ResilientRegistry::new();

        let mut p = ApiProfile::new("only", ProviderKind::Custom);
        p.priority = 0;
        reg.add_profile(p);

        let result = reg
            .call_with_failover(|_profile| async { Err::<String, String>("always fail".into()) })
            .await;

        std::env::remove_var("CUSTOM_API_KEY");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResilientCallError::AllProvidersExhausted
        ));
    }

    #[tokio::test]
    async fn resilient_registry_exhausted_when_no_api_key() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        // Don't set any API key, so all profiles should be skipped.
        std::env::remove_var("CUSTOM_API_KEY");
        let mut reg = ResilientRegistry::new();
        let p = ApiProfile::new("no-key", ProviderKind::Custom);
        reg.add_profile(p);

        let result = reg
            .call_with_failover(|_| async { Ok::<String, String>("should not reach".into()) })
            .await;

        assert!(matches!(
            result.unwrap_err(),
            ResilientCallError::AllProvidersExhausted
        ));
    }

    #[tokio::test]
    async fn resilient_registry_rate_limit_causes_failover() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var("CUSTOM_API_KEY", "test-key");
        let mut reg = ResilientRegistry::new();

        // Primary with very low rate limit (1 RPM).
        let mut p1 = ApiProfile::new("primary", ProviderKind::Custom);
        p1.priority = 0;
        p1.rate_limit_rpm = Some(1);
        let id1 = reg.add_profile(p1);

        let mut p2 = ApiProfile::new("secondary", ProviderKind::Custom);
        p2.priority = 1;
        let id2 = reg.add_profile(p2);

        // First call should succeed with primary (consumes the 1 RPM token).
        let result1 = reg
            .call_with_failover(|profile| {
                let name = profile.name.clone();
                async move { Ok::<String, String>(name) }
            })
            .await;
        assert!(result1.is_ok());
        assert_eq!(result1.unwrap().0, id1);

        // Second call should failover to secondary (primary rate limited).
        let result2 = reg
            .call_with_failover(|profile| {
                let name = profile.name.clone();
                async move { Ok::<String, String>(name) }
            })
            .await;
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap().0, id2);

        std::env::remove_var("CUSTOM_API_KEY");
    }

    #[tokio::test]
    async fn resilient_registry_reset_breaker() {
        let mut reg = ResilientRegistry::new();
        let profile = ApiProfile::new("test", ProviderKind::Custom);
        let id = reg.add_profile(profile);

        // Just verify reset doesn't panic.
        reg.reset_breaker(&id).await;
        let state = reg.get_state(&id).unwrap();
        assert!(!state.is_circuit_open().await);
    }

    #[tokio::test]
    async fn resilient_registry_circuit_opens_after_failures() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var("CUSTOM_API_KEY", "test-key");
        let mut reg = ResilientRegistry::new();

        let mut p = ApiProfile::new("fragile", ProviderKind::Custom);
        p.priority = 0;
        // Use a config with low threshold for quick testing.
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 1,
            timeout: Duration::from_secs(60),
            call_timeout: Duration::from_secs(30),
        };
        let id = reg.add_profile_with_config(p, config);

        // Fail twice to trip the circuit breaker.
        for _ in 0..2 {
            let _ = reg
                .call_with_failover(|_| async { Err::<String, String>("fail".into()) })
                .await;
        }

        // Circuit should now be open.
        let state = reg.get_state(&id).unwrap();
        assert!(state.is_circuit_open().await);

        std::env::remove_var("CUSTOM_API_KEY");
    }
}
