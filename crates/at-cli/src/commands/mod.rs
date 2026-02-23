pub mod agent;
pub mod doctor;
pub mod done;
pub mod hook;
pub mod nudge;
pub mod run_task;
pub mod skill;
pub mod sling;
pub mod status;

/// Build a reqwest client, handling connection errors with a friendly message.
pub fn api_client() -> reqwest::Client {
    reqwest::Client::new()
}

/// Map common reqwest errors to user-friendly messages.
pub fn friendly_error(err: reqwest::Error) -> anyhow::Error {
    if err.is_connect() {
        anyhow::anyhow!(
            "Could not connect to the auto-tundra daemon. Is it running?\n  \
             (hint: start it with `at-daemon` or check --api-url)"
        )
    } else if err.is_timeout() {
        anyhow::anyhow!("Request timed out. The daemon may be overloaded.")
    } else {
        anyhow::anyhow!("API request failed: {err}")
    }
}
pub mod ideation;
