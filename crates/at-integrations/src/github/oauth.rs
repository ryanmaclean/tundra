//! GitHub OAuth 2.0 flow support.
//!
//! Implements the GitHub OAuth web application flow:
//! 1. Redirect user to GitHub's authorization URL
//! 2. GitHub redirects back with a `code`
//! 3. Exchange the `code` for an access token
//! 4. Use the access token to call GitHub APIs

use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Errors that can occur during the GitHub OAuth 2.0 flow.
///
/// This enum represents failures that may happen when exchanging
/// authorization codes for tokens, refreshing tokens, or fetching
/// user profiles via the GitHub OAuth API.
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    /// An HTTP-level error occurred.
    ///
    /// This includes network failures, connection errors, timeouts,
    /// and other transport-layer issues when communicating with GitHub's
    /// OAuth endpoints.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// GitHub's OAuth endpoint returned an error response.
    ///
    /// This occurs when GitHub rejects an authorization code exchange,
    /// token refresh, or other OAuth operation. The `error` field contains
    /// the error code (e.g., "invalid_grant"), and `error_description`
    /// provides additional details.
    ///
    /// Common error codes include:
    /// - `invalid_grant`: The authorization code or refresh token is invalid or expired
    /// - `invalid_client`: Client authentication failed
    /// - `unsupported_grant_type`: The grant type is not supported
    #[error("GitHub returned an error: {error} — {error_description}")]
    GitHubError {
        error: String,
        error_description: String,
    },

    /// Failed to parse GitHub's response.
    ///
    /// This occurs when the OAuth response body cannot be deserialized
    /// into the expected structure, typically indicating an unexpected
    /// response format or API change.
    #[error("failed to parse response: {0}")]
    Parse(String),
}

/// Result type alias for GitHub OAuth operations.
///
/// This is a convenience alias for `Result<T, OAuthError>` used throughout
/// the GitHub OAuth client implementation.
pub type Result<T> = std::result::Result<T, OAuthError>;

/// Configuration for the GitHub OAuth application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

/// The token response from GitHub's token endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
}

/// A GitHub user profile returned by the `/user` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUser {
    pub id: u64,
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: String,
}

/// An error body returned by GitHub during the OAuth code exchange.
#[derive(Debug, Deserialize)]
struct GitHubOAuthError {
    error: String,
    error_description: String,
}

/// Client that handles the GitHub OAuth web application flow.
pub struct GitHubOAuthClient {
    config: GitHubOAuthConfig,
    http: Client,
}

impl GitHubOAuthClient {
    /// Create a new OAuth client from the given configuration.
    pub fn new(config: GitHubOAuthConfig) -> Self {
        let http = Client::builder()
            .user_agent("auto-tundra/1.0")
            .build()
            .expect("failed to build reqwest client");

        Self { config, http }
    }

    /// Build the GitHub authorization URL the user should be redirected to.
    ///
    /// `state` is an opaque CSRF token that GitHub will echo back in the
    /// redirect so the caller can verify the request originated from here.
    pub fn authorization_url(&self, state: &str) -> String {
        let scopes = self.config.scopes.join(" ");
        format!(
            "https://github.com/login/oauth/authorize\
             ?client_id={client_id}\
             &redirect_uri={redirect_uri}\
             &scope={scope}\
             &state={state}",
            client_id = urlencoding::encode(&self.config.client_id),
            redirect_uri = urlencoding::encode(&self.config.redirect_uri),
            scope = urlencoding::encode(&scopes),
            state = urlencoding::encode(state),
        )
    }

    /// Exchange an authorization `code` for an access token.
    pub async fn exchange_code(&self, code: &str) -> Result<OAuthTokenResponse> {
        let resp = self
            .http
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "client_id": self.config.client_id,
                "client_secret": self.config.client_secret,
                "code": code,
                "redirect_uri": self.config.redirect_uri,
            }))
            .send()
            .await?
            .text()
            .await?;

        // GitHub returns 200 even on errors, so we must inspect the body.
        if let Ok(err) = serde_json::from_str::<GitHubOAuthError>(&resp) {
            if !err.error.is_empty() {
                return Err(OAuthError::GitHubError {
                    error: err.error,
                    error_description: err.error_description,
                });
            }
        }

        serde_json::from_str::<OAuthTokenResponse>(&resp)
            .map_err(|e| OAuthError::Parse(format!("{e}: {resp}")))
    }

    /// Refresh an expired token using a refresh token (GitHub Apps only).
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<OAuthTokenResponse> {
        let resp = self
            .http
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "client_id": self.config.client_id,
                "client_secret": self.config.client_secret,
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }))
            .send()
            .await?
            .text()
            .await?;

        if let Ok(err) = serde_json::from_str::<GitHubOAuthError>(&resp) {
            if !err.error.is_empty() {
                return Err(OAuthError::GitHubError {
                    error: err.error,
                    error_description: err.error_description,
                });
            }
        }

        serde_json::from_str::<OAuthTokenResponse>(&resp)
            .map_err(|e| OAuthError::Parse(format!("{e}: {resp}")))
    }

    /// Fetch the authenticated user's profile.
    pub async fn get_user(&self, access_token: &str) -> Result<GitHubUser> {
        let user = self
            .http
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {access_token}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "auto-tundra/1.0")
            .send()
            .await?
            .json::<GitHubUser>()
            .await?;

        Ok(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_url_includes_all_params() {
        let config = GitHubOAuthConfig {
            client_id: "test_client_id".into(),
            client_secret: "test_secret".into(),
            redirect_uri: "http://localhost:3000/callback".into(),
            scopes: vec!["repo".into(), "read:user".into()],
        };

        let client = GitHubOAuthClient::new(config);
        let url = client.authorization_url("random_state_123");

        assert!(url.starts_with("https://github.com/login/oauth/authorize"));
        assert!(url.contains("client_id=test_client_id"));
        assert!(url.contains("state=random_state_123"));
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("scope=repo%20read%3Auser"));
    }

    #[test]
    fn token_response_deserialization() {
        let json = r#"{
            "access_token": "gho_abc123",
            "token_type": "bearer",
            "scope": "repo,read:user"
        }"#;

        let resp: OAuthTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "gho_abc123");
        assert_eq!(resp.token_type, "bearer");
        assert_eq!(resp.scope, "repo,read:user");
        assert!(resp.refresh_token.is_none());
        assert!(resp.expires_in.is_none());
    }

    #[test]
    fn token_response_with_refresh() {
        let json = r#"{
            "access_token": "gho_abc123",
            "token_type": "bearer",
            "scope": "repo",
            "refresh_token": "ghr_refresh456",
            "expires_in": 28800
        }"#;

        let resp: OAuthTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.refresh_token.as_deref(), Some("ghr_refresh456"));
        assert_eq!(resp.expires_in, Some(28800));
    }

    #[test]
    fn github_user_deserialization() {
        let json = r#"{
            "id": 12345,
            "login": "octocat",
            "name": "The Octocat",
            "email": "octocat@github.com",
            "avatar_url": "https://avatars.githubusercontent.com/u/12345"
        }"#;

        let user: GitHubUser = serde_json::from_str(json).unwrap();
        assert_eq!(user.id, 12345);
        assert_eq!(user.login, "octocat");
        assert_eq!(user.name.as_deref(), Some("The Octocat"));
        assert_eq!(user.email.as_deref(), Some("octocat@github.com"));
        assert!(user.avatar_url.contains("avatars"));
    }

    #[test]
    fn github_user_deserialization_optional_fields() {
        let json = r#"{
            "id": 99,
            "login": "bot",
            "name": null,
            "email": null,
            "avatar_url": "https://avatars.githubusercontent.com/u/99"
        }"#;

        let user: GitHubUser = serde_json::from_str(json).unwrap();
        assert_eq!(user.id, 99);
        assert!(user.name.is_none());
        assert!(user.email.is_none());
    }
}
