//! Tests for GitHub OAuth types and URL generation.

use at_integrations::github::oauth::{
    GitHubOAuthClient, GitHubOAuthConfig, GitHubUser, OAuthTokenResponse,
};

// ---------------------------------------------------------------------------
// Authorization URL
// ---------------------------------------------------------------------------

#[test]
fn authorization_url_contains_required_params() {
    let config = GitHubOAuthConfig {
        client_id: "Iv1.abc123".into(),
        client_secret: "secret".into(),
        redirect_uri: "http://localhost:3000/callback".into(),
        scopes: vec!["repo".into(), "read:user".into()],
    };

    let client = GitHubOAuthClient::new(config);
    let url = client.authorization_url("csrf_token_42");

    assert!(
        url.starts_with("https://github.com/login/oauth/authorize"),
        "URL should point to GitHub's authorize endpoint"
    );
    assert!(url.contains("client_id=Iv1.abc123"));
    assert!(url.contains("state=csrf_token_42"));
    assert!(url.contains("redirect_uri="));
    // Scopes are space-separated, URL-encoded
    assert!(url.contains("scope=repo%20read%3Auser"));
}

#[test]
fn authorization_url_encodes_special_characters() {
    let config = GitHubOAuthConfig {
        client_id: "id with spaces".into(),
        client_secret: "secret".into(),
        redirect_uri: "http://localhost:3000/cb?foo=bar&baz=1".into(),
        scopes: vec!["admin:org".into()],
    };

    let client = GitHubOAuthClient::new(config);
    let url = client.authorization_url("state&special=true");

    // Ensure special chars are encoded
    assert!(url.contains("client_id=id%20with%20spaces"));
    assert!(url.contains("state=state%26special%3Dtrue"));
}

// ---------------------------------------------------------------------------
// Token response deserialization
// ---------------------------------------------------------------------------

#[test]
fn token_response_minimal() {
    let json = r#"{
        "access_token": "gho_16C7e42F292c6912E7710c838347Ae178B4a",
        "token_type": "bearer",
        "scope": "repo,gist"
    }"#;

    let resp: OAuthTokenResponse = serde_json::from_str(json).unwrap();
    assert_eq!(
        resp.access_token,
        "gho_16C7e42F292c6912E7710c838347Ae178B4a"
    );
    assert_eq!(resp.token_type, "bearer");
    assert_eq!(resp.scope, "repo,gist");
    assert!(resp.refresh_token.is_none());
    assert!(resp.expires_in.is_none());
}

#[test]
fn token_response_with_refresh_and_expiry() {
    let json = r#"{
        "access_token": "gho_token",
        "token_type": "bearer",
        "scope": "repo",
        "refresh_token": "ghr_refresh",
        "expires_in": 28800
    }"#;

    let resp: OAuthTokenResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.refresh_token.as_deref(), Some("ghr_refresh"));
    assert_eq!(resp.expires_in, Some(28800));
}

// ---------------------------------------------------------------------------
// User info deserialization
// ---------------------------------------------------------------------------

#[test]
fn github_user_full() {
    let json = r#"{
        "id": 583231,
        "login": "octocat",
        "name": "The Octocat",
        "email": "octocat@github.com",
        "avatar_url": "https://avatars.githubusercontent.com/u/583231?v=4"
    }"#;

    let user: GitHubUser = serde_json::from_str(json).unwrap();
    assert_eq!(user.id, 583231);
    assert_eq!(user.login, "octocat");
    assert_eq!(user.name.as_deref(), Some("The Octocat"));
    assert_eq!(user.email.as_deref(), Some("octocat@github.com"));
    assert!(user.avatar_url.contains("avatars"));
}

#[test]
fn github_user_nullable_fields() {
    let json = r#"{
        "id": 1,
        "login": "bot-account",
        "name": null,
        "email": null,
        "avatar_url": "https://avatars.githubusercontent.com/u/1"
    }"#;

    let user: GitHubUser = serde_json::from_str(json).unwrap();
    assert_eq!(user.login, "bot-account");
    assert!(user.name.is_none());
    assert!(user.email.is_none());
}

#[test]
fn token_response_roundtrip() {
    let original = OAuthTokenResponse {
        access_token: "gho_test".into(),
        token_type: "bearer".into(),
        scope: "repo".into(),
        refresh_token: Some("ghr_test".into()),
        expires_in: Some(3600),
    };

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: OAuthTokenResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.access_token, original.access_token);
    assert_eq!(deserialized.refresh_token, original.refresh_token);
    assert_eq!(deserialized.expires_in, original.expires_in);
}
