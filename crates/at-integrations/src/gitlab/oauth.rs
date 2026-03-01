//! GitLab OAuth 2.0 flow support.
//!
//! Implements the GitLab OAuth web application flow:
//! 1. Redirect user to GitLab's authorization URL
//! 2. GitLab redirects back with a `code`
//! 3. Exchange the `code` for an access token
//! 4. Use the access token to call GitLab APIs

use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Errors that can occur during the GitLab OAuth 2.0 flow.
///
/// This enum represents failures that may happen when exchanging
/// authorization codes for tokens, refreshing tokens, or fetching
/// user profiles via the GitLab OAuth API.
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    /// An HTTP-level error occurred.
    ///
    /// This includes network failures, connection errors, timeouts,
    /// and other transport-layer issues when communicating with GitLab's
    /// OAuth endpoints.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// GitLab's OAuth endpoint returned an error response.
    ///
    /// This occurs when GitLab rejects an authorization code exchange,
    /// token refresh, or other OAuth operation. The `error` field contains
    /// the error code (e.g., "invalid_grant"), and `error_description`
    /// provides additional details.
    ///
    /// Common error codes include:
    /// - `invalid_grant`: The authorization code or refresh token is invalid or expired
    /// - `invalid_client`: Client authentication failed
    /// - `invalid_request`: Required parameters are missing or malformed
    /// - `unauthorized_client`: The client is not authorized for this grant type
    #[error("GitLab returned an error: {error} — {error_description}")]
    GitLabError {
        error: String,
        error_description: String,
    },

    /// Failed to parse GitLab's response.
    ///
    /// This occurs when the OAuth response body cannot be deserialized
    /// into the expected structure, typically indicating an unexpected
    /// response format or API change.
    #[error("failed to parse response: {0}")]
    Parse(String),
}

/// Result type alias for GitLab OAuth operations.
///
/// This is a convenience alias for `Result<T, OAuthError>` used throughout
/// the GitLab OAuth client implementation.
pub type Result<T> = std::result::Result<T, OAuthError>;

/// Configuration for the GitLab OAuth application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

/// The token response from GitLab's token endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub created_at: Option<u64>,
}

/// A GitLab user profile returned by the `/api/v4/user` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabUserProfile {
    pub id: u64,
    pub username: String,
    pub name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub web_url: String,
    pub state: String,
}

/// An error body returned by GitLab during the OAuth code exchange.
#[derive(Debug, Deserialize)]
struct GitLabOAuthError {
    error: String,
    #[serde(default)]
    error_description: String,
}

/// Client that handles the GitLab OAuth web application flow.
pub struct GitLabOAuthClient {
    config: GitLabOAuthConfig,
    http: Client,
}

impl GitLabOAuthClient {
    /// Create a new OAuth client from the given configuration.
    pub fn new(config: GitLabOAuthConfig) -> Self {
        let http = Client::builder()
            .user_agent("auto-tundra/1.0")
            .build()
            .expect("failed to build reqwest client");

        Self { config, http }
    }

    /// Build the GitLab authorization URL the user should be redirected to.
    ///
    /// `state` is an opaque CSRF token that GitLab will echo back in the
    /// redirect so the caller can verify the request originated from here.
    pub fn authorization_url(&self, state: &str) -> String {
        let scopes = self.config.scopes.join(" ");
        format!(
            "https://gitlab.com/oauth/authorize\
             ?client_id={client_id}\
             &redirect_uri={redirect_uri}\
             &response_type=code\
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
            .post("https://gitlab.com/oauth/token")
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "client_id": self.config.client_id,
                "client_secret": self.config.client_secret,
                "code": code,
                "grant_type": "authorization_code",
                "redirect_uri": self.config.redirect_uri,
            }))
            .send()
            .await?
            .text()
            .await?;

        // GitLab may return 200 even on errors, so we must inspect the body.
        if let Ok(err) = serde_json::from_str::<GitLabOAuthError>(&resp) {
            if !err.error.is_empty() {
                return Err(OAuthError::GitLabError {
                    error: err.error,
                    error_description: err.error_description,
                });
            }
        }

        serde_json::from_str::<OAuthTokenResponse>(&resp)
            .map_err(|e| OAuthError::Parse(format!("{e}: {resp}")))
    }

    /// Refresh an expired token using a refresh token.
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<OAuthTokenResponse> {
        let resp = self
            .http
            .post("https://gitlab.com/oauth/token")
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

        if let Ok(err) = serde_json::from_str::<GitLabOAuthError>(&resp) {
            if !err.error.is_empty() {
                return Err(OAuthError::GitLabError {
                    error: err.error,
                    error_description: err.error_description,
                });
            }
        }

        serde_json::from_str::<OAuthTokenResponse>(&resp)
            .map_err(|e| OAuthError::Parse(format!("{e}: {resp}")))
    }

    /// Fetch the authenticated user's profile.
    pub async fn get_user(&self, access_token: &str) -> Result<GitLabUserProfile> {
        let user = self
            .http
            .get("https://gitlab.com/api/v4/user")
            .header("Authorization", format!("Bearer {access_token}"))
            .header("Accept", "application/json")
            .header("User-Agent", "auto-tundra/1.0")
            .send()
            .await?
            .json::<GitLabUserProfile>()
            .await?;

        Ok(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> GitLabOAuthConfig {
        GitLabOAuthConfig {
            client_id: "test_client_id".into(),
            client_secret: "test_secret".into(),
            redirect_uri: "http://localhost:3000/callback".into(),
            scopes: vec!["read_user".into(), "api".into()],
        }
    }

    #[test]
    fn authorization_url_includes_all_params() {
        let client = GitLabOAuthClient::new(test_config());
        let url = client.authorization_url("random_state_123");

        assert!(url.starts_with("https://gitlab.com/oauth/authorize"));
        assert!(url.contains("client_id=test_client_id"));
        assert!(url.contains("state=random_state_123"));
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("scope=read_user%20api"));
    }

    #[test]
    fn authorization_url_encodes_special_chars() {
        let mut config = test_config();
        config.client_id = "id with spaces".into();
        let client = GitLabOAuthClient::new(config);
        let url = client.authorization_url("state&evil=true");

        assert!(url.contains("client_id=id%20with%20spaces"));
        assert!(url.contains("state=state%26evil%3Dtrue"));
    }

    #[test]
    fn token_response_deserialization() {
        let json = r#"{
            "access_token": "glpat-abc123",
            "token_type": "Bearer",
            "scope": "read_user api",
            "created_at": 1700000000
        }"#;

        let resp: OAuthTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "glpat-abc123");
        assert_eq!(resp.token_type, "Bearer");
        assert_eq!(resp.scope, "read_user api");
        assert!(resp.refresh_token.is_none());
        assert!(resp.expires_in.is_none());
        assert_eq!(resp.created_at, Some(1700000000));
    }

    #[test]
    fn token_response_with_refresh() {
        let json = r#"{
            "access_token": "glpat-abc123",
            "token_type": "Bearer",
            "scope": "api",
            "refresh_token": "glrt-refresh456",
            "expires_in": 7200,
            "created_at": 1700000000
        }"#;

        let resp: OAuthTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.refresh_token.as_deref(), Some("glrt-refresh456"));
        assert_eq!(resp.expires_in, Some(7200));
    }

    #[test]
    fn token_response_serde_roundtrip() {
        let token = OAuthTokenResponse {
            access_token: "glpat-test".into(),
            token_type: "Bearer".into(),
            scope: "api".into(),
            refresh_token: Some("glrt-ref".into()),
            expires_in: Some(7200),
            created_at: Some(1700000000),
        };

        let json = serde_json::to_string(&token).unwrap();
        let de: OAuthTokenResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(de.access_token, "glpat-test");
        assert_eq!(de.refresh_token.as_deref(), Some("glrt-ref"));
    }

    #[test]
    fn gitlab_user_deserialization() {
        let json = r#"{
            "id": 12345,
            "username": "tanuki",
            "name": "The Tanuki",
            "email": "tanuki@gitlab.com",
            "avatar_url": "https://gitlab.com/uploads/-/system/user/avatar/12345/avatar.png",
            "web_url": "https://gitlab.com/tanuki",
            "state": "active"
        }"#;

        let user: GitLabUserProfile = serde_json::from_str(json).unwrap();
        assert_eq!(user.id, 12345);
        assert_eq!(user.username, "tanuki");
        assert_eq!(user.name, "The Tanuki");
        assert_eq!(user.email.as_deref(), Some("tanuki@gitlab.com"));
        assert_eq!(user.state, "active");
    }

    #[test]
    fn gitlab_user_deserialization_optional_fields() {
        let json = r#"{
            "id": 99,
            "username": "bot",
            "name": "Bot User",
            "email": null,
            "avatar_url": null,
            "web_url": "https://gitlab.com/bot",
            "state": "active"
        }"#;

        let user: GitLabUserProfile = serde_json::from_str(json).unwrap();
        assert_eq!(user.id, 99);
        assert!(user.email.is_none());
        assert!(user.avatar_url.is_none());
    }

    #[test]
    fn gitlab_user_serde_roundtrip() {
        let user = GitLabUserProfile {
            id: 42,
            username: "dev".into(),
            name: "Developer".into(),
            email: Some("dev@example.com".into()),
            avatar_url: None,
            web_url: "https://gitlab.com/dev".into(),
            state: "active".into(),
        };

        let json = serde_json::to_string(&user).unwrap();
        let de: GitLabUserProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(de.id, 42);
        assert_eq!(de.username, "dev");
    }

    #[test]
    fn oauth_config_serde_roundtrip() {
        let config = test_config();
        let json = serde_json::to_string(&config).unwrap();
        let de: GitLabOAuthConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(de.client_id, "test_client_id");
        assert_eq!(de.scopes.len(), 2);
    }
}
