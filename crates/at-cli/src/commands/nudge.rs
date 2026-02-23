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
