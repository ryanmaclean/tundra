use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_bridge::notifications::NotificationLevel;
use at_core::config::Config;
use at_core::settings::SettingsManager;
use serde_json::{json, Value};

/// Spin up an API server backed by a temp settings file on a random port.
async fn start_test_server() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let tmp_path = std::env::temp_dir()
        .join(format!("at-settings-api-test-{}", uuid::Uuid::new_v4()))
        .join("settings.toml");
    let settings_manager = Arc::new(SettingsManager::new(&tmp_path));

    let mut state = ApiState::new(event_bus).with_relaxed_rate_limits();
    // Replace the settings_manager with our temp one.
    state.settings_manager = settings_manager;
    let state = Arc::new(state);

    let router = api_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://{addr}"), state)
}

// ===========================================================================
// GET /api/settings
// ===========================================================================

#[tokio::test]
async fn test_get_settings_returns_defaults() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/settings")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["general"]["project_name"], "auto-tundra");
    assert_eq!(body["display"]["theme"], "dark");
    assert_eq!(body["display"]["font_size"], 14);
    assert_eq!(body["terminal"]["font_family"], "JetBrains Mono");
    assert_eq!(body["terminal"]["cursor_style"], "block");
    assert_eq!(body["security"]["sandbox_mode"], true);
}

#[tokio::test]
async fn test_get_settings_returns_saved_values() {
    let (base, state) = start_test_server().await;

    // Save custom settings to disk first
    let mut cfg = Config::default();
    cfg.display.theme = "ocean".into();
    cfg.terminal.font_size = 20;
    cfg.general.project_name = "saved-test".into();
    state.settings_manager.save(&cfg).unwrap();

    let resp = reqwest::get(format!("{base}/api/settings")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["general"]["project_name"], "saved-test");
    assert_eq!(body["display"]["theme"], "ocean");
    assert_eq!(body["terminal"]["font_size"], 20);
}

// ===========================================================================
// PUT /api/settings
// ===========================================================================

#[tokio::test]
async fn test_put_settings_full_replace() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let mut cfg = Config::default();
    cfg.display.theme = "lime".into();
    cfg.terminal.font_family = "Fira Code".into();
    cfg.general.project_name = "put-test".into();

    let resp = client
        .put(format!("{base}/api/settings"))
        .json(&cfg)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Verify via GET
    let resp = reqwest::get(format!("{base}/api/settings")).await.unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["display"]["theme"], "lime");
    assert_eq!(body["terminal"]["font_family"], "Fira Code");
    assert_eq!(body["general"]["project_name"], "put-test");
}

#[tokio::test]
async fn test_put_settings_persists_to_disk() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let mut cfg = Config::default();
    cfg.display.theme = "forest".into();
    cfg.terminal.font_size = 22;

    let resp = client
        .put(format!("{base}/api/settings"))
        .json(&cfg)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Read directly from disk using the settings manager
    let disk_cfg = state.settings_manager.load().unwrap();
    assert_eq!(disk_cfg.display.theme, "forest");
    assert_eq!(disk_cfg.terminal.font_size, 22);
}

#[tokio::test]
async fn test_put_settings_validates_input() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Send invalid JSON body
    let resp = client
        .put(format!("{base}/api/settings"))
        .header("content-type", "application/json")
        .body("{invalid json}")
        .send()
        .await
        .unwrap();

    // Should return a 4xx error (422 for deserialization failure in axum)
    assert!(
        resp.status().is_client_error(),
        "Expected client error, got {}",
        resp.status()
    );
}

// ===========================================================================
// PATCH /api/settings
// ===========================================================================

#[tokio::test]
async fn test_patch_settings_deep_merge() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // First set some initial values via PUT
    let mut cfg = Config::default();
    cfg.display.theme = "dark".into();
    cfg.display.font_size = 14;
    cfg.terminal.font_family = "JetBrains Mono".into();
    cfg.terminal.font_size = 14;

    client
        .put(format!("{base}/api/settings"))
        .json(&cfg)
        .send()
        .await
        .unwrap();

    // Patch only display.theme — terminal should be preserved
    let patch = json!({
        "display": {
            "theme": "neo"
        }
    });

    let resp = client
        .patch(format!("{base}/api/settings"))
        .json(&patch)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["display"]["theme"], "neo");
    // Terminal should still have its original values
    assert_eq!(body["terminal"]["font_family"], "JetBrains Mono");
    assert_eq!(body["terminal"]["font_size"], 14);
}

#[tokio::test]
async fn test_patch_settings_partial_update() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let patch = json!({
        "terminal": {
            "font_size": 18
        }
    });

    let resp = client
        .patch(format!("{base}/api/settings"))
        .json(&patch)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["terminal"]["font_size"], 18);
    // Other terminal fields should still be defaults
    assert_eq!(body["terminal"]["font_family"], "JetBrains Mono");
    assert_eq!(body["terminal"]["cursor_style"], "block");
}

#[tokio::test]
async fn test_patch_settings_preserves_unmodified_fields() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Set initial values
    let mut cfg = Config::default();
    cfg.integrations.github_token_env = "CUSTOM_GH_TOKEN".into();
    cfg.integrations.github_owner = Some("test-org".into());
    cfg.display.theme = "dusk".into();
    cfg.terminal.font_size = 16;
    state.settings_manager.save(&cfg).unwrap();

    // Patch only display theme
    let patch = json!({
        "display": {
            "theme": "retro"
        }
    });

    let resp = client
        .patch(format!("{base}/api/settings"))
        .json(&patch)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["display"]["theme"], "retro");
    // These should be preserved
    assert_eq!(body["integrations"]["github_token_env"], "CUSTOM_GH_TOKEN");
    assert_eq!(body["integrations"]["github_owner"], "test-org");
    assert_eq!(body["terminal"]["font_size"], 16);
}

// ===========================================================================
// Notification Settings API
// ===========================================================================

#[tokio::test]
async fn test_get_notification_settings() {
    let (base, _state) = start_test_server().await;

    // Notification count endpoint returns defaults (0 unread, 0 total)
    let resp = reqwest::get(format!("{base}/api/notifications/count"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["unread"], 0);
    assert_eq!(body["total"], 0);
}

#[tokio::test]
async fn test_update_notification_settings() {
    let (base, state) = start_test_server().await;

    // Add a notification and verify it shows up
    {
        let mut store = state.notification_store.write().await;
        store.add(
            "Task Complete",
            "Build finished",
            NotificationLevel::Success,
            "system",
        );
        store.add(
            "Task Failed",
            "Tests failed",
            NotificationLevel::Error,
            "system",
        );
    }

    let resp = reqwest::get(format!("{base}/api/notifications/count"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["unread"], 2);
    assert_eq!(body["total"], 2);

    // List all notifications
    let resp = reqwest::get(format!("{base}/api/notifications"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 2);

    // Verify notification fields
    let titles: Vec<&str> = body.iter().map(|n| n["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Task Complete"));
    assert!(titles.contains(&"Task Failed"));
}

// These new controls retain/join their server task; they exercise the actual router.
struct EditTestServer {
    base: String,
    state: Arc<ApiState>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for EditTestServer {
    fn drop(&mut self) { self.task.abort(); }
}
impl EditTestServer {
    async fn start() -> Self {
        let path = std::env::temp_dir().join(format!("at-settings-edit-http-{}", uuid::Uuid::new_v4())).join("settings.toml");
        let mut state = ApiState::new(EventBus::new()).with_relaxed_rate_limits();
        state.settings_manager = Arc::new(SettingsManager::new(path));
        let state = Arc::new(state);
        let router = api_router(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
        Self { base, state, task }
    }
    async fn finish(mut self) {
        self.task.abort();
        let result = (&mut self.task).await;
        assert!(result.is_ok() || result.unwrap_err().is_cancelled());
        if self.state.settings_manager.path().parent().unwrap().exists() {
            std::fs::remove_dir_all(self.state.settings_manager.path().parent().unwrap()).unwrap();
        }
    }
}

#[tokio::test]
async fn editor_http_patch_preserves_profiles_and_put_still_resets() {
    let server = EditTestServer::start().await;
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build().unwrap();
    let url = format!("{}/api/settings", server.base);
    let mut config = Config::default();
    config.api_profiles.profiles.push(at_core::config::ApiProfileEntry {
        name: "custom".into(), base_url: "https://example.invalid".into(), api_key_env: "CUSTOM_KEY".into(),
    });
    config.general.project_name = "keep-project".into();
    config.integrations.github_owner = Some("clear-me".into());
    config.terminal.font_size = 23;
    server.state.settings_manager.save(&config).unwrap();
    let response = client.patch(&url).json(&json!({"notifications":{"sound_enabled":false}, "integrations":{"github_owner":null}})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["api_profiles"]["profiles"][0]["name"], "custom");
    assert_eq!(body["general"]["project_name"], "keep-project");
    assert_eq!(body["terminal"]["font_size"], 23);
    assert!(body["integrations"]["github_owner"].is_null());
    assert_eq!(server.state.settings_manager.load().unwrap().api_profiles.profiles[0].base_url, "https://example.invalid");
    // Deliberately clearing an array remains supported by PATCH.
    let removed = client.patch(&url).json(&json!({"api_profiles":{"profiles":[]}})).send().await.unwrap();
    assert_eq!(removed.status(), 200);
    assert!(server.state.settings_manager.load().unwrap().api_profiles.profiles.is_empty());
    server.state.settings_manager.save(&config).unwrap();
    let reset = client.put(&url).json(&Config::default()).send().await.unwrap();
    assert_eq!(reset.status(), 200);
    let reset_disk = server.state.settings_manager.load().unwrap();
    assert!(reset_disk.api_profiles.profiles.is_empty());
    assert_eq!(reset_disk.general.project_name, Config::default().general.project_name);
    server.finish().await;
}

#[tokio::test]
async fn editor_http_get_and_patch_refuse_bad_stored_state() {
    let server = EditTestServer::start().await;
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build().unwrap();
    let url = format!("{}/api/settings", server.base);
    let path = server.state.settings_manager.path();
    assert_eq!(client.get(&url).send().await.unwrap().status(), 200);
    assert!(!path.exists());
    assert_eq!(client.patch(&url).json(&json!({"display":{"font_size":18}})).send().await.unwrap().status(), 200);
    assert_eq!(server.state.settings_manager.load().unwrap().display.font_size, 18);
    for text in ["not valid = [", "[kanban]\ncolumn_mode = \"\"\n"] {
        std::fs::write(path, text).unwrap();
        let get = client.get(&url).send().await.unwrap();
        assert_eq!(get.status(), 500);
        assert!(get.json::<Value>().await.unwrap()["error"].is_string());
        let patch = client.patch(&url).json(&json!({"display":{"font_size":20}})).send().await.unwrap();
        assert_eq!(patch.status(), 500);
        assert_eq!(std::fs::read_to_string(path).unwrap(), text);
    }
    std::fs::remove_file(path).unwrap();
    std::fs::create_dir(path).unwrap();
    assert_eq!(client.get(&url).send().await.unwrap().status(), 500);
    assert_eq!(client.patch(&url).json(&json!({})).send().await.unwrap().status(), 500);
    assert!(path.is_dir());
    server.finish().await;
}
