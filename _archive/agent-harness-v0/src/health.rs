use axum::{
    extract::State,
    http::StatusCode,
    response::{Json, IntoResponse},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{info, warn, error};

use crate::circuit_breaker::CircuitBreaker;
use crate::provider::LlmProvider;
use crate::quota::QuotaTracker;
use crate::types::ProviderKind;

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub version: String,
    pub services: ServiceHealth,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HealthStatus {
    #[serde(rename = "healthy")]
    Healthy,
    #[serde(rename = "degraded")]
    Degraded,
    #[serde(rename = "unhealthy")]
    Unhealthy,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceHealth {
    pub llm_provider: ProviderHealth,
    pub circuit_breaker: CircuitHealth,
    pub quota_tracker: QuotaHealth,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub status: HealthStatus,
    pub model: String,
    pub last_check: u64,
    pub error_count: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CircuitHealth {
    pub status: HealthStatus,
    pub state: String,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_failure: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuotaHealth {
    pub status: HealthStatus,
    pub models_checked: usize,
    pub total_requests: u32,
    pub total_tokens: u32,
    pub models: Vec<ModelQuota>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelQuota {
    pub model: String,
    pub requests_used: u32,
    pub requests_limit: u32,
    pub tokens_used: u32,
    pub tokens_limit: Option<u32>,
    pub usage_percentage: f32,
}

/// Health check server
pub struct HealthServer {
    start_time: Instant,
    provider: Arc<dyn LlmProvider>,
}

impl HealthServer {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            start_time: Instant::now(),
            provider,
        }
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/health", get(health_check))
            .route("/health/ready", get(readiness_check))
            .route("/health/live", get(liveness_check))
            .with_state(self)
    }

    async fn start(&self, port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let app = self.router();
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        info!("Health check server starting on port {}", port);
        
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn health_check(
    State(health_server): State<HealthServer>,
) -> impl axum::response::IntoResponse {
    let health = health_server.check_health().await;
    
    let status_code = match health.status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded => StatusCode::OK,
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };

    (status_code, Json(health)).into_response()
}

async fn readiness_check(
    State(health_server): State<HealthServer>,
) -> impl axum::response::IntoResponse {
    let health = health_server.check_health().await;
    
    let status_code = match health.status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded => StatusCode::OK,
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };

    status_code.into_response()
}

async fn liveness_check(
    State(_health_server): State<HealthServer>,
) -> impl axum::response::IntoResponse {
    StatusCode::OK.into_response()
}

impl HealthServer {
    async fn check_health(&self) -> HealthResponse {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let uptime = self.start_time.elapsed().as_secs();

        let services = self.check_services().await;
        
        let overall_status = self.determine_overall_status(&services);

        HealthResponse {
            status: overall_status,
            timestamp: now,
            uptime_seconds: uptime,
            version: env!("CARGO_PKG_VERSION").to_string(),
            services,
        }
    }

    async fn check_services(&self) -> ServiceHealth {
        // Check LLM provider
        let provider_health = self.check_provider().await;
        
        // Check circuit breaker (if available)
        let circuit_health = self.check_circuit_breaker().await;
        
        // Check quota tracker
        let quota_health = self.check_quota_tracker().await;

        ServiceHealth {
            llm_provider: provider_health,
            circuit_breaker: circuit_health,
            quota_tracker: quota_health,
        }
    }

    async fn check_provider(&self) -> ProviderHealth {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Try a simple health check request
        let test_messages = vec![
            crate::types::Message::system("You are a health check assistant."),
            crate::types::Message::user("Respond with 'OK' to indicate health."),
        ];

        let result = self.provider.chat_completion(&test_messages, &[]).await;

        match result {
            Ok(_response) => ProviderHealth {
                status: HealthStatus::Healthy,
                model: self.provider.get_model().await.unwrap_or("unknown".to_string()),
                last_check: now,
                error_count: 0,
                last_error: None,
            },
            Err(e) => ProviderHealth {
                status: HealthStatus::Degraded,
                model: self.provider.get_model().await.unwrap_or("unknown".to_string()),
                last_check: now,
                error_count: 1,
                last_error: Some(format!("{:?}", e)),
            },
        }
    }

    async fn check_circuit_breaker(&self) -> CircuitHealth {
        // Get circuit breaker from provider if it has one
        let circuit = self.provider.circuit_breaker();
        let metrics = circuit.metrics().await;
        
        let status = match metrics.state {
            crate::circuit_breaker::CircuitState::Closed => HealthStatus::Healthy,
            crate::circuit_breaker::CircuitState::HalfOpen => HealthStatus::Degraded,
            crate::circuit_breaker::CircuitState::Open => HealthStatus::Unhealthy,
        };

        CircuitHealth {
            status,
            state: format!("{:?}", metrics.state),
            failure_count: metrics.failure_count,
            success_count: metrics.success_count,
            last_failure: metrics.last_failure_time,
        }
    }

    async fn check_quota_tracker(&self) -> QuotaHealth {
        let quota_tracker = self.provider.quota_tracker();
        
        // Get quota info for all models
        let models = vec![
            "meta-llama/llama-3.3-70b-instruct:free",
            "arcee-ai/trinity-large-preview:free",
            "deepseek/deepseek-r1-0528:free",
        ];

        let mut model_quotas = Vec::new();
        let mut total_requests = 0;
        let mut total_tokens = 0;

        for model in models {
            let quota_info = quota_tracker.get_quota_info(model);
            total_requests += quota_info.requests_used;
            total_tokens += quota_info.tokens_used;
            
            model_quotas.push(ModelQuota {
                model: model.to_string(),
                requests_used: quota_info.requests_used,
                requests_limit: quota_info.requests_limit,
                tokens_used: quota_info.tokens_used,
                tokens_limit: quota_info.tokens_limit,
                usage_percentage: quota_info.usage_percentage(),
            });
        }

        // Determine overall quota health
        let status = if total_requests == 0 {
            HealthStatus::Healthy
        } else if model_quotas.iter().any(|m| m.usage_percentage > 80.0) {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        QuotaHealth {
            status,
            models_checked: model_quotas.len(),
            total_requests,
            total_tokens,
            models: model_quotas,
        }
    }

    fn determine_overall_status(&self, services: &ServiceHealth) -> HealthStatus {
        let statuses = vec![
            &services.llm_provider.status,
            &services.circuit_breaker.status,
            &services.quota_tracker.status,
        ];

        if statuses.iter().any(|s| matches!(s, HealthStatus::Unhealthy)) {
            HealthStatus::Unhealthy
        } else if statuses.iter().any(|s| matches!(s, HealthStatus::Degraded)) {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::OpenRouterProvider;
    use crate::types::ProviderConfig;

    #[tokio::test]
    async fn test_health_check_basic() {
        let config = ProviderConfig {
            api_key: "test-key".to_string(),
            base_url: "https://api.test.com".to_string(),
            model: "test-model".to_string(),
            extra_headers: std::collections::HashMap::new(),
            max_tokens: 100,
            temperature: 0.7,
        };

        let provider = Arc::new(OpenRouterProvider::new(config));
        let health_server = HealthServer::new(provider);

        let health = health_server.check_health().await;
        
        assert!(matches!(health.status, HealthStatus::Degraded | HealthStatus::Unhealthy));
        assert_eq!(health.version, env!("CARGO_PKG_VERSION"));
        assert!(health.uptime_seconds > 0);
    }

    #[tokio::test]
    async fn test_service_health_determination() {
        let health_server = HealthServer::new(
            Arc::new(OpenRouterProvider::new(ProviderConfig {
                api_key: "test-key".to_string(),
                base_url: "https://api.test.com".to_string(),
                model: "test-model".to_string(),
                extra_headers: std::collections::HashMap::new(),
                max_tokens: 100,
                temperature: 0.7,
            }))
        );

        let services = health_server.check_services().await;
        let overall = health_server.determine_overall_status(&services);

        // Should be degraded or unhealthy since we're using a test key
        assert!(matches!(overall, HealthStatus::Degraded | HealthStatus::Unhealthy));
    }
}
