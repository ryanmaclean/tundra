use super::{api_client, friendly_error};
use serde_json::json;

/// Create a new bead and place it in the "slung" state.
pub async fn run(api_url: &str, title: &str, lane: &str) -> anyhow::Result<()> {
    let client = api_client();
    let url = format!("{api_url}/api/beads");

    let resp = client
        .post(&url)
        .json(&json!({
            "title": title,
            "lane": lane,
        }))
        .send()
        .await
        .map_err(friendly_error)?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(friendly_error)?;

    if status.is_success() {
        let id = body["id"].as_str().unwrap_or("unknown");
        let bead_title = body["title"].as_str().unwrap_or(title);
        println!("Bead created: {id}");
        println!("  title: {bead_title}");
        println!("  lane:  {lane}");
        println!("  status: slung");
    } else {
        let err_msg = body["error"].as_str().unwrap_or("unknown error");
        anyhow::bail!("Failed to create bead: {err_msg} (HTTP {status})");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::{extract::Json, http::StatusCode, routing::post, Router};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn creates_bead_successfully() {
        let app = Router::new().route(
            "/api/beads",
            post(|Json(body): Json<serde_json::Value>| async move {
                assert_eq!(body["title"], "Fix bug");
                assert_eq!(body["lane"], "backlog");
                (
                    StatusCode::OK,
                    Json(json!({"id": "bead-456", "title": "Fix bug", "lane": "backlog", "status": "slung"})),
                )
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let api_url = format!("http://{addr}");
        let result = run(&api_url, "Fix bug", "backlog").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn handles_error_response_from_api() {
        let app = Router::new().route(
            "/api/beads",
            post(|Json(_body): Json<serde_json::Value>| async move {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "invalid lane"})),
                )
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let api_url = format!("http://{addr}");
        let result = run(&api_url, "Fix bug", "invalid").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid lane"));
    }
}
