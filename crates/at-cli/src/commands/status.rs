use super::{api_client, friendly_error};

/// Run the `status` subcommand: call the API and pretty-print system status.
pub async fn run(api_url: &str) -> anyhow::Result<()> {
    let client = api_client();

    // Fetch overall system status
    let status_url = format!("{api_url}/api/status");
    let status_resp = client
        .get(&status_url)
        .send()
        .await
        .map_err(friendly_error)?;

    if !status_resp.status().is_success() {
        anyhow::bail!("Failed to fetch status (HTTP {})", status_resp.status());
    }
    let status: serde_json::Value = status_resp.json().await.map_err(friendly_error)?;

    // Fetch beads list
    let beads_url = format!("{api_url}/api/beads");
    let beads_resp = client
        .get(&beads_url)
        .send()
        .await
        .map_err(friendly_error)?;

    let beads: Vec<serde_json::Value> = if beads_resp.status().is_success() {
        beads_resp.json().await.unwrap_or_default()
    } else {
        Vec::new()
    };

    // Count beads per status
    let mut backlog: u64 = 0;
    let mut hooked: u64 = 0;
    let mut slung: u64 = 0;
    let mut review: u64 = 0;
    let mut done: u64 = 0;
    let mut failed: u64 = 0;
    let mut escalated: u64 = 0;

    for bead in &beads {
        match bead["status"].as_str().unwrap_or("") {
            "backlog" => backlog += 1,
            "hooked" => hooked += 1,
            "slung" => slung += 1,
            "review" => review += 1,
            "done" => done += 1,
            "failed" => failed += 1,
            "escalated" => escalated += 1,
            _ => {}
        }
    }

    let total = beads.len() as u64;
    let version = status["version"].as_str().unwrap_or("unknown");
    let uptime = status["uptime_seconds"].as_u64().unwrap_or(0);
    let agent_count = status["agent_count"].as_u64().unwrap_or(0);

    println!("auto-tundra status  (v{version})");
    println!("{}", "-".repeat(40));
    println!("Uptime:         {}s", uptime);
    println!("Active agents:  {}", agent_count);
    println!("Total beads:    {}", total);
    println!("  backlog:      {}", backlog);
    println!("  hooked:       {}", hooked);
    println!("  slung:        {}", slung);
    println!("  review:       {}", review);
    println!("  done:         {}", done);
    println!("  failed:       {}", failed);
    println!("  escalated:    {}", escalated);

    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::{routing::get, Json, Router};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn status_displays_system_info() {
        let app = Router::new()
            .route(
                "/api/status",
                get(|| async {
                    Json(json!({
                        "version": "1.2.3",
                        "uptime_seconds": 3600,
                        "agent_count": 2
                    }))
                }),
            )
            .route(
                "/api/beads",
                get(|| async {
                    Json(json!([
                        {"status": "backlog"},
                        {"status": "hooked"},
                        {"status": "slung"},
                        {"status": "review"},
                        {"status": "done"},
                        {"status": "failed"}
                    ]))
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = run(&format!("http://{addr}")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn status_handles_empty_beads() {
        let app = Router::new()
            .route(
                "/api/status",
                get(|| async {
                    Json(json!({
                        "version": "1.0.0",
                        "uptime_seconds": 0,
                        "agent_count": 0
                    }))
                }),
            )
            .route("/api/beads", get(|| async { Json(json!([])) }));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = run(&format!("http://{addr}")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn status_counts_bead_statuses_correctly() {
        let app = Router::new()
            .route(
                "/api/status",
                get(|| async {
                    Json(json!({
                        "version": "2.0.0",
                        "uptime_seconds": 7200,
                        "agent_count": 5
                    }))
                }),
            )
            .route(
                "/api/beads",
                get(|| async {
                    Json(json!([
                        {"status": "backlog"},
                        {"status": "backlog"},
                        {"status": "hooked"},
                        {"status": "slung"},
                        {"status": "slung"},
                        {"status": "slung"},
                        {"status": "review"},
                        {"status": "done"},
                        {"status": "done"},
                        {"status": "done"},
                        {"status": "done"},
                        {"status": "failed"},
                        {"status": "escalated"}
                    ]))
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = run(&format!("http://{addr}")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn status_fails_on_api_error() {
        let app = Router::new().route(
            "/api/status",
            get(|| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "") }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = run(&format!("http://{addr}")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn status_handles_beads_fetch_failure() {
        let app = Router::new()
            .route(
                "/api/status",
                get(|| async {
                    Json(json!({
                        "version": "1.0.0",
                        "uptime_seconds": 100,
                        "agent_count": 1
                    }))
                }),
            )
            .route(
                "/api/beads",
                get(|| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "") }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = run(&format!("http://{addr}")).await;
        assert!(result.is_ok());
    }
}
