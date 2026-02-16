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
        let err_msg = body["error"]
            .as_str()
            .unwrap_or("unknown error");
        anyhow::bail!("Failed to hook bead {bead_id}: {err_msg} (HTTP {status})");
    }

    Ok(())
}
