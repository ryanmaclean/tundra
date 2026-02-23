use std::path::{Path, PathBuf};

use at_core::context_engine::ProjectContextLoader;
use serde::Deserialize;
use serde_json::json;

use super::{api_client, friendly_error};

#[derive(Debug, Deserialize)]
struct IntegrationSettings {
    #[serde(default)]
    github_token_env: String,
    #[serde(default)]
    gitlab_token_env: String,
    #[serde(default)]
    linear_api_key_env: String,
    #[serde(default)]
    openai_api_key_env: String,
}

#[derive(Debug, Deserialize)]
struct SettingsResponse {
    integrations: IntegrationSettings,
}

pub async fn run(
    api_url: &str,
    project_path: &str,
    strict: bool,
    json_output: bool,
    out_path: Option<&str>,
) -> anyhow::Result<()> {
    let client = api_client();
    let mut failures = 0usize;

    // API status
    let status_url = format!("{api_url}/api/status");
    let api_check = match client.get(&status_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await.map_err(friendly_error)?;
            json!({
                "ok": true,
                "version": body["version"],
                "uptime_seconds": body["uptime_seconds"],
            })
        }
        Ok(resp) => {
            failures += 1;
            json!({
                "ok": false,
                "status": resp.status().as_u16(),
                "error": "daemon returned non-success",
            })
        }
        Err(e) => {
            failures += 1;
            json!({
                "ok": false,
                "error": friendly_error(e).to_string(),
            })
        }
    };

    // Settings for env var names
    let settings_url = format!("{api_url}/api/settings");
    let env_names = match client.get(&settings_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let s: SettingsResponse = resp.json().await.map_err(friendly_error)?;
            vec![
                s.integrations.github_token_env,
                s.integrations.gitlab_token_env,
                s.integrations.linear_api_key_env,
                s.integrations.openai_api_key_env,
            ]
        }
        _ => vec![
            "GITHUB_TOKEN".to_string(),
            "GITLAB_TOKEN".to_string(),
            "LINEAR_API_KEY".to_string(),
            "OPENAI_API_KEY".to_string(),
        ],
    };

    let env_checks = env_names
        .into_iter()
        .filter(|name| !name.trim().is_empty())
        .map(|name| {
            let is_set = std::env::var(&name)
                .ok()
                .is_some_and(|v| !v.trim().is_empty());
            if !is_set {
                failures += 1;
            }
            json!({
                "name": name,
                "set": is_set,
            })
        })
        .collect::<Vec<_>>();

    // Project + skills
    let project_exists = Path::new(project_path).exists();
    if !project_exists {
        failures += 1;
    }

    let skill_count = if project_exists {
        let loader = ProjectContextLoader::new(project_path);
        loader.load_skill_definitions().len()
    } else {
        0
    };
    if skill_count == 0 {
        failures += 1;
    }

    let result = json!({
        "api": api_check,
        "project_path": project_path,
        "project_exists": project_exists,
        "skill_count": skill_count,
        "env": env_checks,
        "failures": failures,
    });

    if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("doctor report");
        println!("{}", "-".repeat(40));
        println!(
            "API: {}",
            if result["api"]["ok"].as_bool().unwrap_or(false) {
                "ok"
            } else {
                "failed"
            }
        );
        if let Some(v) = result["api"]["version"].as_str() {
            println!("  version: {v}");
        }
        println!(
            "Project: {} ({})",
            project_path,
            if project_exists { "exists" } else { "missing" }
        );
        println!("Skills: {}", skill_count);
        println!("Env vars:");
        if let Some(items) = result["env"].as_array() {
            for item in items {
                let name = item["name"].as_str().unwrap_or("<unknown>");
                let set = item["set"].as_bool().unwrap_or(false);
                println!("  - {:<24} {}", name, if set { "set" } else { "missing" });
            }
        }
        println!("Failures: {}", failures);
    }

    if let Some(path) = out_path {
        write_json_artifact(path, &result)?;
    }

    if strict && failures > 0 {
        anyhow::bail!("doctor checks failed ({failures} issues)");
    }

    Ok(())
}

fn write_json_artifact(path: &str, value: &serde_json::Value) -> anyhow::Result<()> {
    let out_path = PathBuf::from(path);
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(out_path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::{routing::get, Json, Router};
    use serde_json::json;

    use super::*;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[tokio::test]
    async fn doctor_writes_artifact_file() {
        let settings = json!({
            "integrations": {
                "github_token_env": "AT_TEST_GH_TOKEN",
                "gitlab_token_env": "AT_TEST_GITLAB_TOKEN",
                "linear_api_key_env": "AT_TEST_LINEAR_API_KEY",
                "openai_api_key_env": "AT_TEST_OPENAI_API_KEY"
            }
        });

        let app = Router::new()
            .route(
                "/api/status",
                get(|| async { Json(json!({"version":"1.2.3","uptime_seconds":7})) }),
            )
            .route(
                "/api/settings",
                get(move || {
                    let payload = settings.clone();
                    async move { Json(payload) }
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let project_root = unique_temp_dir("at-cli-doctor-project");
        write_file(
            &project_root.join(".claude/skills/test-skill/SKILL.md"),
            r#"---
name: test-skill
description: smoke
allowed_tools:
  - Bash
---

# Skill
Body
"#,
        );

        let out = unique_temp_dir("at-cli-doctor-out").with_extension("json");
        run(
            &format!("http://{addr}"),
            &project_root.display().to_string(),
            false,
            true,
            Some(&out.display().to_string()),
        )
        .await
        .unwrap();

        let written = std::fs::read_to_string(&out).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(payload["api"]["ok"], true);
        assert_eq!(payload["project_exists"], true);
        assert_eq!(payload["skill_count"], 1);

        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_dir_all(project_root);
    }

    #[tokio::test]
    async fn doctor_strict_fails_with_missing_project() {
        let result = run(
            "http://127.0.0.1:65534",
            "/definitely/missing/project/path",
            true,
            true,
            None,
        )
        .await;
        assert!(result.is_err());
    }
}
