use octocrab::Octocrab;
use thiserror::Error;

use crate::types::GitHubConfig;

/// Errors that can occur when interacting with the GitHub API.
///
/// This enum wraps various error types that may occur during GitHub client
/// operations, including API errors from octocrab, missing credentials,
/// and serialization failures.
#[derive(Debug, Error)]
pub enum GitHubError {
    /// An error returned by the GitHub API via the octocrab client.
    ///
    /// This includes network failures, HTTP errors, rate limiting,
    /// and invalid API responses.
    #[error("GitHub API error: {0}")]
    Api(#[from] octocrab::Error),

    /// GitHub token was not provided.
    ///
    /// This occurs when attempting to create a client without a valid token.
    /// Set the `GITHUB_TOKEN` environment variable or provide the token
    /// directly in [`GitHubConfig`].
    #[error("missing GitHub token — set GITHUB_TOKEN or pass it in GitHubConfig")]
    MissingToken,

    /// Failed to read an environment variable.
    ///
    /// This occurs when using [`GitHubClient::new_from_env`] and a required
    /// environment variable (GITHUB_TOKEN, GITHUB_OWNER, or GITHUB_REPO)
    /// is missing or invalid.
    #[error("environment variable error: {0}")]
    Env(#[from] std::env::VarError),

    /// Failed to serialize or deserialize JSON data.
    ///
    /// This may occur when parsing GitHub API responses or constructing
    /// request bodies.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Result type alias for GitHub operations.
///
/// This is a convenience alias for `Result<T, GitHubError>` used throughout
/// the GitHub client implementation.
pub type Result<T> = std::result::Result<T, GitHubError>;

#[derive(Debug, Clone)]
pub struct GitHubClient {
    pub(crate) octocrab: Octocrab,
    pub(crate) owner: String,
    pub(crate) repo: String,
}

impl GitHubClient {
    /// Create a new `GitHubClient` from an explicit [`GitHubConfig`].
    pub fn new(config: GitHubConfig) -> Result<Self> {
        let token = config.token.ok_or(GitHubError::MissingToken)?;

        let octocrab = Octocrab::builder().personal_token(token).build()?;

        Ok(Self {
            octocrab,
            owner: config.owner,
            repo: config.repo,
        })
    }

    /// Create a new `GitHubClient` by reading `GITHUB_TOKEN`, `GITHUB_OWNER`,
    /// and `GITHUB_REPO` from the environment.
    pub fn new_from_env() -> Result<Self> {
        let token = std::env::var("GITHUB_TOKEN")?;
        let owner = std::env::var("GITHUB_OWNER")?;
        let repo = std::env::var("GITHUB_REPO")?;

        let config = GitHubConfig {
            token: Some(token),
            owner,
            repo,
        };

        Self::new(config)
    }

    /// Returns a reference to the inner `Octocrab` instance.
    pub fn inner(&self) -> &Octocrab {
        &self.octocrab
    }

    /// Returns the configured owner (org or user).
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// Returns the configured repository name.
    pub fn repo(&self) -> &str {
        &self.repo
    }
}
