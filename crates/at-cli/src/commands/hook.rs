use super::{api_client, friendly_error};
use serde_json::json;

/// Move a bead to the "hooked" (active/assigned) state.
pub async fn run(api_url: &str, bead_id: &str) -> anyhow::Result<()> {
    let client = api_client();
    let url = format!("{api_url}/api/beads/{bead_id}/status");

    let resp = client
        .post(&url)
        .json(&json!({ "status": "hooked" }))
        .send()
        .await
        .map_err(friendly_error)?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(friendly_error)?;

    if status.is_success() {
        println!("Bead {bead_id} is now hooked.");
        if let Some(title) = body["title"].as_str() {
            println!("  title: {title}");
        }
    } else {
        let err_msg = body["error"].as_str().unwrap_or("unknown error");
        anyhow::bail!("Failed to hook bead {bead_id}: {err_msg} (HTTP {status})");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::{extract::Path as AxPath, http::StatusCode, routing::post, Json, Router};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn marks_bead_as_hooked_successfully() {
        let app = Router::new().route(
            "/api/beads/{id}/status",
            post(|AxPath(id): AxPath<String>, Json(body): Json<serde_json::Value>| async move {
                assert_eq!(id, "bead-123");
                assert_eq!(body["status"], "hooked");
                (
                    StatusCode::OK,
                    Json(json!({"id": "bead-123", "title": "Fix bug", "status": "hooked"})),
                )
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let api_url = format!("http://{addr}");
        let result = run(&api_url, "bead-123").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn handles_error_response_from_api() {
        let app = Router::new().route(
            "/api/beads/{id}/status",
            post(|AxPath(_id): AxPath<String>| async move {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "bead not found"})),
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
        assert!(err.contains("bead not found"));
    }
}
