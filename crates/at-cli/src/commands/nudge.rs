use super::{api_client, friendly_error};

/// Nudge a stuck agent (send restart signal).
pub async fn run(api_url: &str, agent_id: &str) -> anyhow::Result<()> {
    let client = api_client();
    let url = format!("{api_url}/api/agents/{agent_id}/nudge");

    let resp = client.post(&url).send().await.map_err(friendly_error)?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(friendly_error)?;

    if status.is_success() {
        println!("Agent {agent_id} nudged.");
        if let Some(name) = body["name"].as_str() {
            println!("  name:   {name}");
        }
        if let Some(agent_status) = body["status"].as_str() {
            println!("  status: {agent_status}");
        }
    } else {
        let err_msg = body["error"].as_str().unwrap_or("unknown error");
        anyhow::bail!("Failed to nudge agent {agent_id}: {err_msg} (HTTP {status})");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::{extract::Path as AxPath, http::StatusCode, routing::post, Json, Router};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn nudges_agent_successfully() {
        let app = Router::new().route(
            "/api/agents/{id}/nudge",
            post(|AxPath(id): AxPath<String>| async move {
                assert_eq!(id, "agent-123");
                (
                    StatusCode::OK,
                    Json(json!({"id": "agent-123", "name": "test-agent", "status": "running"})),
                )
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let api_url = format!("http://{addr}");
        let result = run(&api_url, "agent-123").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn handles_error_response_from_api() {
        let app = Router::new().route(
            "/api/agents/{id}/nudge",
            post(|AxPath(_id): AxPath<String>| async move {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "agent not found"})),
                )
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let api_url = format!("http://{addr}");
        let result = run(&api_url, "nonexistent").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("agent not found"));
    }
}
