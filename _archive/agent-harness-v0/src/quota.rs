use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

use crate::types::*;

/// Tracks API usage quotas across different models and providers.
#[derive(Debug)]
pub struct QuotaTracker {
    quotas: Arc<Mutex<HashMap<String, QuotaInfo>>>,
    free_models: Vec<String>,
}

impl Default for QuotaTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl QuotaTracker {
    pub fn new() -> Self {
        let free_models = vec![
            "meta-llama/llama-3.3-70b-instruct:free".to_string(),
            "arcee-ai/trinity-large-preview:free".to_string(),
            "stepfun/step-3.5-flash:free".to_string(),
            "z-ai/glm-4.5-air:free".to_string(),
            "deepseek/deepseek-r1-0528:free".to_string(),
            "nvidia/nemotron-3-nano-30b-a3b:free".to_string(),
            "openrouter/aurora-alpha".to_string(),
            "qwen/qwen3-235b-a22b-thinking-2507".to_string(),
            "openai/gpt-oss-120b:free".to_string(),
            "upstage/solar-pro-3:free".to_string(),
            "arcee-ai/trinity-mini:free".to_string(),
            "nvidia/nemotron-nano-9b-v2:free".to_string(),
            "nvidia/nemotron-nano-12b-v2-vl:free".to_string(),
        ];

        Self {
            quotas: Arc::new(Mutex::new(HashMap::new())),
            free_models,
        }
    }

    pub fn is_free_model(&self, model: &str) -> bool {
        self.free_models.contains(&model.to_string())
    }

    pub fn get_quota_info(&self, model: &str) -> QuotaInfo {
        let quotas = self.quotas.lock().unwrap();
        quotas.get(model).cloned().unwrap_or_else(|| QuotaInfo {
            requests_used: 0,
            requests_limit: if self.is_free_model(model) { 100 } else { 1000 },
            tokens_used: 0,
            tokens_limit: Some(if self.is_free_model(model) {
                10000
            } else {
                100000
            }),
            model_type: if self.is_free_model(model) {
                ModelType::Free
            } else {
                ModelType::Paid
            },
            is_free_tier: self.is_free_model(model),
            reset_time: Some(self.next_reset_time()),
        })
    }

    pub fn update_usage(&self, model: &str, tokens_used: u32) -> Result<(), QuotaError> {
        let mut quotas = self.quotas.lock().unwrap();

        let quota = quotas
            .entry(model.to_string())
            .or_insert_with(|| QuotaInfo {
                requests_used: 0,
                requests_limit: if self.is_free_model(model) { 100 } else { 1000 },
                tokens_used: 0,
                tokens_limit: Some(if self.is_free_model(model) {
                    10000
                } else {
                    100000
                }),
                model_type: if self.is_free_model(model) {
                    ModelType::Free
                } else {
                    ModelType::Paid
                },
                is_free_tier: self.is_free_model(model),
                reset_time: Some(self.next_reset_time()),
            });

        // Check if we can make a request
        if quota.requests_used >= quota.requests_limit {
            return Err(QuotaError::LimitExceeded {
                model: model.to_string(),
                current_usage: quota.requests_used,
                limit: quota.requests_limit,
                reset_time: quota.reset_time.clone(),
            });
        }

        // Update usage
        quota.requests_used += 1;
        quota.tokens_used += tokens_used;

        info!(
            "Updated quota for {}: {}/{} requests ({}%), {}/{} tokens ({:.1}%)",
            model,
            quota.requests_used,
            quota.requests_limit,
            quota.usage_percentage(),
            quota.tokens_used,
            quota.tokens_limit.unwrap_or(0),
            quota.tokens_usage_percentage().unwrap_or(0.0)
        );

        // Warn if near limit
        if quota.is_near_limit(80.0) {
            warn!(
                "Approaching quota limit for {}: {}% used ({}/{} requests)",
                model,
                quota.usage_percentage(),
                quota.requests_used,
                quota.requests_limit
            );
        }

        Ok(())
    }

    pub fn display_quota_status(&self) {
        let quotas = self.quotas.lock().unwrap();

        println!("📊 API QUOTA STATUS");
        println!("{}", "=".repeat(60));

        if quotas.is_empty() {
            println!("No usage recorded yet.");
            return;
        }

        for (model, quota) in quotas.iter() {
            let status = if quota.can_make_request() {
                "✅"
            } else {
                "❌"
            };
            let model_type = match quota.model_type {
                ModelType::Free => "🆓",
                ModelType::Paid => "💰",
                ModelType::Freemium => "🎁",
            };

            println!("\n{} {} {}", status, model_type, model);
            println!(
                "  Requests: {}/{} ({:.1}%)",
                quota.requests_used,
                quota.requests_limit,
                quota.usage_percentage()
            );

            if let Some(token_limit) = quota.tokens_limit {
                println!(
                    "  Tokens: {}/{} ({:.1}%)",
                    quota.tokens_used,
                    token_limit,
                    quota.tokens_usage_percentage().unwrap_or(0.0)
                );
            }

            if let Some(reset_time) = &quota.reset_time {
                println!("  Reset: {}", reset_time);
            }

            if quota.is_near_limit(80.0) {
                println!("  ⚠️  Warning: Approaching usage limit!");
            }
        }

        println!("\n{}\n", "=".repeat(60));
    }

    pub fn suggest_best_model(&self) -> Option<String> {
        let quotas = self.quotas.lock().unwrap();

        let mut best_model = None;
        let mut lowest_usage = 100.0;

        for model in &self.free_models {
            if let Some(quota) = quotas.get(model) {
                if quota.can_make_request() && quota.usage_percentage() < lowest_usage {
                    lowest_usage = quota.usage_percentage();
                    best_model = Some(model.clone());
                }
            } else {
                // Never used model - best choice
                return Some(model.clone());
            }
        }

        best_model
    }

    fn next_reset_time(&self) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Reset at midnight UTC
        let tomorrow = (now / 86400 + 1) * 86400;
        let reset_time = UNIX_EPOCH + Duration::from_secs(tomorrow);

        format!("{:?}", reset_time)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum QuotaError {
    #[error("Quota limit exceeded for model '{model}': {current_usage}/{limit} requests. Reset at {reset_time:?}")]
    LimitExceeded {
        model: String,
        current_usage: u32,
        limit: u32,
        reset_time: Option<String>,
    },
}
