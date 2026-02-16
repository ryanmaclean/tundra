//! API Profiles — multi-provider endpoint configuration and failover.
//!
//! Supports:
//! - **Anthropic** (direct API)
//! - **OpenRouter** (400+ models, unified API)
//! - **Custom** (any Anthropic-compatible endpoint)
//! - **Account failover**: Automatic switching on rate limits or errors
//! - **Cost tracking**: Per-profile usage and spend tracking

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ApiProfile — a configured API endpoint
// ---------------------------------------------------------------------------

/// A configured API profile for an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiProfile {
    pub id: Uuid,
    pub name: String,
    pub provider: ProviderKind,
    /// Base URL for the API (e.g., "https://api.anthropic.com").
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
    Custom,
}

impl ProviderKind {
    pub fn default_base_url(&self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "https://api.anthropic.com",
            ProviderKind::OpenRouter => "https://openrouter.ai/api",
            ProviderKind::OpenAi => "https://api.openai.com",
            ProviderKind::Custom => "http://localhost:8080",
        }
    }

    pub fn default_api_key_env(&self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "ANTHROPIC_API_KEY",
            ProviderKind::OpenRouter => "OPENROUTER_API_KEY",
            ProviderKind::OpenAi => "OPENAI_API_KEY",
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
        std::env::var(&self.api_key_env).is_ok()
    }
}

fn default_model_for(provider: ProviderKind) -> String {
    match provider {
        ProviderKind::Anthropic => "claude-sonnet-4-20250514".into(),
        ProviderKind::OpenRouter => "anthropic/claude-sonnet-4-20250514".into(),
        ProviderKind::OpenAi => "gpt-4o".into(),
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
        let mut entries: Vec<(Uuid, u32)> = self
            .profiles
            .values()
            .map(|p| (p.id, p.priority))
            .collect();
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
}
