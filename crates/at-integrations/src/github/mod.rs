pub mod client;
pub mod issues;
pub mod oauth;
pub mod pr_automation;
pub mod pull_requests;
pub mod sync;

/// Convert a timestamp from an octocrab model (chrono) without naming chrono.
pub(crate) fn timestamp_from(time: impl Into<std::time::SystemTime>) -> at_time::Timestamp {
    at_time::Timestamp::from(time.into())
}
