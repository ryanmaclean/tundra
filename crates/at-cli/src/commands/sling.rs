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
        let err_msg = body["error"]
            .as_str()
            .unwrap_or("unknown error");
        anyhow::bail!("Failed to create bead: {err_msg} (HTTP {status})");
    }

    Ok(())
}
