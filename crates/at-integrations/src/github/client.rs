use octocrab::Octocrab;
use thiserror::Error;

use crate::types::GitHubConfig;

#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("GitHub API error: {0}")]
    Api(#[from] octocrab::Error),

    #[error("missing GitHub token — set GITHUB_TOKEN or pass it in GitHubConfig")]
    MissingToken,

    #[error("environment variable error: {0}")]
    Env(#[from] std::env::VarError),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

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

        let octocrab = Octocrab::builder()
            .personal_token(token)
            .build()?;

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
