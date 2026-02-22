use std::path::{Path, PathBuf};

use anyhow::Context;
use at_core::context_engine::{ProjectContextLoader, SkillDefinition};
use serde_json::json;

use super::{api_client, friendly_error};

async fn response_json_or_raw(resp: reqwest::Response) -> anyhow::Result<(reqwest::StatusCode, serde_json::Value)> {
    let status = resp.status();
    let text = resp.text().await.map_err(friendly_error)?;
    let body = serde_json::from_str(&text).unwrap_or_else(|_| {
        if text.trim().is_empty() {
            json!({})
        } else {
            json!({ "raw": text })
        }
    });
    Ok((status, body))
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub task: String,
    pub skills: Vec<String>,
    pub project_path: String,
    pub model: Option<String>,
    pub max_agents: Option<u32>,
    pub lane: String,
    pub category: String,
    pub priority: String,
    pub complexity: String,
    pub no_execute: bool,
    pub dry_run: bool,
    pub emit_prompt: bool,
    pub json_output: bool,
    pub out_path: Option<String>,
    pub role: Option<String>,
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

fn load_selected_skills(project_path: &str, selected: &[String]) -> anyhow::Result<Vec<SkillDefinition>> {
    if selected.is_empty() {
        return Ok(Vec::new());
    }

    let root = Path::new(project_path);
    if !root.exists() {
        anyhow::bail!("Project path does not exist: {}", root.display());
    }

    let loader = ProjectContextLoader::new(root);
    let available = loader.load_skill_definitions();

    let mut resolved = Vec::new();
    let mut missing = Vec::new();
    for name in selected {
        match available.iter().find(|s| s.name == *name) {
            Some(skill) => resolved.push(skill.clone()),
            None => missing.push(name.clone()),
        }
    }

    if !missing.is_empty() {
        let known = available
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::bail!(
            "Unknown skills: {}. Available: {}",
            missing.join(", "),
            if known.is_empty() { "<none>".to_string() } else { known }
        );
    }

    Ok(resolved)
}

fn title_from_task(task: &str, role: Option<&str>) -> String {
    let trimmed = task.trim();
    let base = if trimmed.is_empty() { "Untitled task" } else { trimmed };
    let mut out = if let Some(r) = role {
        format!("[{}] {}", r, base)
    } else {
        base.to_string()
    };
    if out.len() > 120 {
        out.truncate(120);
    }
    out
}

fn build_description(opts: &RunOptions, skills: &[SkillDefinition]) -> String {
    let mut parts = Vec::new();
    parts.push(format!("Task: {}", opts.task.trim()));
    if let Some(role) = &opts.role {
        parts.push(format!("Requested role: {}", role));
    }
    if let Some(model) = &opts.model {
        parts.push(format!("Model preference: {}", model));
    }
    if let Some(n) = opts.max_agents {
        parts.push(format!("Max agents: {}", n));
    }

    if !skills.is_empty() {
        parts.push("".to_string());
        parts.push("Skills context:".to_string());
        for s in skills {
            parts.push(format!("## {}", s.name));
            if !s.description.trim().is_empty() {
                parts.push(s.description.trim().to_string());
            }
            parts.push(s.body.trim().to_string());
        }
    }

    parts.join("\n")
}

fn dry_run_payload(opts: &RunOptions, title: &str, description: &str, skills: &[SkillDefinition]) -> serde_json::Value {
    json!({
        "mode": "dry-run",
        "task_title": title,
        "task": opts.task,
        "role": opts.role,
        "lane": opts.lane,
        "category": opts.category,
        "priority": opts.priority,
        "complexity": opts.complexity,
        "model": opts.model,
        "max_agents": opts.max_agents,
        "skills": skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "description": description,
        "description_len": description.len(),
    })
}

pub async fn run(api_url: &str, opts: RunOptions) -> anyhow::Result<()> {
    let client = api_client();
    let selected_skills = load_selected_skills(&opts.project_path, &opts.skills)?;
    let title = title_from_task(&opts.task, opts.role.as_deref());
    let description = build_description(&opts, &selected_skills);

    if opts.dry_run {
        let payload = dry_run_payload(&opts, &title, &description, &selected_skills);
        if let Some(path) = &opts.out_path {
            write_json_artifact(path, &payload)?;
        }
        if opts.json_output {
            println!("{}", serde_json::to_string_pretty(&payload)?);
        } else {
            println!("dry-run: no API calls made");
            println!("  title: {}", payload["task_title"].as_str().unwrap_or("<untitled>"));
            println!("  lane/category: {}/{}", payload["lane"].as_str().unwrap_or("?"), payload["category"].as_str().unwrap_or("?"));
            println!("  priority/complexity: {}/{}", payload["priority"].as_str().unwrap_or("?"), payload["complexity"].as_str().unwrap_or("?"));
            println!("  skills: {}", selected_skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>().join(", "));
            println!("  description_len: {}", payload["description_len"].as_u64().unwrap_or(0));
            if opts.emit_prompt {
                println!("\n--- prompt ---\n{}", description);
            }
        }
        return Ok(());
    }

    let bead_resp = client
        .post(format!("{api_url}/api/beads"))
        .json(&json!({
            "title": title,
            "description": description,
            "lane": opts.lane,
        }))
        .send()
        .await
        .map_err(friendly_error)?;

    let (bead_status, bead) = response_json_or_raw(bead_resp).await?;
    if !bead_status.is_success() {
        let err_msg = bead["error"]
            .as_str()
            .or_else(|| bead["raw"].as_str())
            .unwrap_or("unknown error");
        anyhow::bail!("Failed to create bead: {err_msg} (HTTP {bead_status})");
    }

    let bead_id = bead["id"]
        .as_str()
        .map(str::to_string)
        .context("create bead response missing id")?;

    let task_resp = client
        .post(format!("{api_url}/api/tasks"))
        .json(&json!({
            "title": title_from_task(&opts.task, opts.role.as_deref()),
            "description": description,
            "bead_id": bead_id,
            "category": opts.category,
            "priority": opts.priority,
            "complexity": opts.complexity,
        }))
        .send()
        .await
        .map_err(friendly_error)?;

    let (task_status, task) = response_json_or_raw(task_resp).await?;
    if !task_status.is_success() {
        let err_msg = task["error"]
            .as_str()
            .or_else(|| task["raw"].as_str())
            .unwrap_or("unknown error");
        anyhow::bail!("Failed to create task: {err_msg} (HTTP {task_status})");
    }

    let task_id = task["id"]
        .as_str()
        .map(str::to_string)
        .context("create task response missing id")?;

    let mut execute_result = None;
    if !opts.no_execute {
        let execute_resp = client
            .post(format!("{api_url}/api/tasks/{task_id}/execute"))
            .send()
            .await
            .map_err(friendly_error)?;
        let (execute_status, execute_body) = response_json_or_raw(execute_resp).await?;
        execute_result = Some(json!({
            "status": execute_status.as_u16(),
            "body": execute_body,
        }));
    }

    if opts.json_output {
        let payload = json!({
            "task_id": task_id,
            "bead_id": bead["id"],
            "title": task["title"],
            "skills": selected_skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
            "executed": !opts.no_execute,
            "execute_result": execute_result,
        });
        if let Some(path) = &opts.out_path {
            write_json_artifact(path, &payload)?;
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if let Some(path) = &opts.out_path {
        let payload = json!({
            "task_id": task_id,
            "bead_id": bead["id"],
            "title": task["title"],
            "skills": selected_skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
            "executed": !opts.no_execute,
            "execute_result": execute_result,
        });
        write_json_artifact(path, &payload)?;
    }

    println!("Task created: {}", task_id);
    println!("  bead: {}", bead["id"].as_str().unwrap_or("<unknown>"));
    println!("  title: {}", task["title"].as_str().unwrap_or("<untitled>"));
    if !selected_skills.is_empty() {
        let names = selected_skills
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        println!("  skills: {}", names);
    }

    if opts.no_execute {
        println!("  execution: skipped (--no-execute)");
    } else if let Some(exec) = execute_result {
        let code = exec["status"].as_u64().unwrap_or(0);
        println!("  execution: requested (HTTP {})", code);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::{
        extract::Path as AxPath,
        http::StatusCode,
        routing::post,
        Json, Router,
    };

    use super::*;

    fn sample_skill(name: &str, desc: &str, body: &str) -> SkillDefinition {
        SkillDefinition {
            name: name.to_string(),
            description: desc.to_string(),
            allowed_tools: vec!["Read".to_string(), "Bash".to_string()],
            body: body.to_string(),
            path: PathBuf::from(format!(".claude/skills/{name}")),
            references: vec![],
        }
    }

    fn sample_opts() -> RunOptions {
        RunOptions {
            task: "Implement CLI smoke checks".to_string(),
            skills: vec!["wave-execution".to_string()],
            project_path: ".".to_string(),
            model: Some("sonnet".to_string()),
            max_agents: Some(3),
            lane: "standard".to_string(),
            category: "feature".to_string(),
            priority: "medium".to_string(),
            complexity: "medium".to_string(),
            no_execute: false,
            dry_run: false,
            emit_prompt: false,
            json_output: false,
            out_path: None,
            role: None,
        }
    }

    #[test]
    fn title_truncates_to_120_chars() {
        let long = "x".repeat(200);
        let t = title_from_task(&long, None);
        assert_eq!(t.len(), 120);
    }

    #[test]
    fn title_includes_role_prefix() {
        let t = title_from_task("Investigate flaky tests", Some("qa-reviewer"));
        assert!(t.starts_with("[qa-reviewer] "));
        assert!(t.contains("Investigate flaky tests"));
    }

    #[test]
    fn description_contains_skill_sections() {
        let opts = sample_opts();
        let skills = vec![sample_skill(
            "wave-execution",
            "Execute in lanes",
            "1. Plan\n2. Execute\n3. Verify",
        )];
        let desc = build_description(&opts, &skills);
        assert!(desc.contains("Task: Implement CLI smoke checks"));
        assert!(desc.contains("Model preference: sonnet"));
        assert!(desc.contains("Max agents: 3"));
        assert!(desc.contains("## wave-execution"));
        assert!(desc.contains("Execute in lanes"));
    }

    #[test]
    fn dry_run_payload_contains_expected_fields() {
        let opts = sample_opts();
        let skills = vec![sample_skill("integration-hardening", "Keep creds real", "Rules...")];
        let title = title_from_task(&opts.task, None);
        let desc = build_description(&opts, &skills);
        let payload = dry_run_payload(&opts, &title, &desc, &skills);
        assert_eq!(payload["mode"], "dry-run");
        assert_eq!(payload["task_title"], title);
        assert_eq!(payload["category"], "feature");
        assert_eq!(payload["skills"][0], "integration-hardening");
        assert!(payload["description_len"].as_u64().unwrap_or(0) > 0);
    }

    #[tokio::test]
    async fn dry_run_can_write_artifact_file() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let out = std::env::temp_dir()
            .join(format!("at-cli-dry-run-{}-{}.json", std::process::id(), nanos));
        let opts = RunOptions {
            task: "Artifact run".to_string(),
            skills: vec![],
            project_path: ".".to_string(),
            model: None,
            max_agents: None,
            lane: "standard".to_string(),
            category: "feature".to_string(),
            priority: "medium".to_string(),
            complexity: "medium".to_string(),
            no_execute: true,
            dry_run: true,
            emit_prompt: false,
            json_output: false,
            out_path: Some(out.display().to_string()),
            role: None,
        };

        run("http://127.0.0.1:65534", opts).await.unwrap();
        let data = std::fs::read_to_string(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(v["mode"], "dry-run");
        let _ = std::fs::remove_file(out);
    }

    #[tokio::test]
    async fn run_executes_against_mock_api() {
        let app = Router::new()
            .route(
                "/api/beads",
                post(|Json(body): Json<serde_json::Value>| async move {
                    (
                        StatusCode::CREATED,
                        Json(json!({
                            "id": "bead-test-1",
                            "title": body["title"].as_str().unwrap_or("untitled"),
                        })),
                    )
                }),
            )
            .route(
                "/api/tasks",
                post(|Json(body): Json<serde_json::Value>| async move {
                    (
                        StatusCode::CREATED,
                        Json(json!({
                            "id": "task-test-1",
                            "title": body["title"].as_str().unwrap_or("untitled"),
                        })),
                    )
                }),
            )
            .route(
                "/api/tasks/{id}/execute",
                post(|AxPath(_id): AxPath<String>| async move {
                    (StatusCode::ACCEPTED, "accepted")
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let opts = RunOptions {
            task: "Run against mock server".to_string(),
            skills: vec![],
            project_path: ".".to_string(),
            model: None,
            max_agents: None,
            lane: "standard".to_string(),
            category: "feature".to_string(),
            priority: "medium".to_string(),
            complexity: "medium".to_string(),
            no_execute: false,
            dry_run: false,
            emit_prompt: false,
            json_output: true,
            out_path: None,
            role: None,
        };

        run(&format!("http://{addr}"), opts).await.unwrap();
    }

    #[tokio::test]
    async fn run_fails_cleanly_on_bead_create_error_without_writing_artifact() {
        let app = Router::new().route(
            "/api/beads",
            post(|| async move {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "bead create failed" })),
                )
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let out = std::env::temp_dir()
            .join(format!("at-cli-run-fail-{}-{}.json", std::process::id(), nanos));

        let opts = RunOptions {
            task: "should fail".to_string(),
            skills: vec![],
            project_path: ".".to_string(),
            model: None,
            max_agents: None,
            lane: "standard".to_string(),
            category: "feature".to_string(),
            priority: "medium".to_string(),
            complexity: "medium".to_string(),
            no_execute: false,
            dry_run: false,
            emit_prompt: false,
            json_output: true,
            out_path: Some(out.display().to_string()),
            role: None,
        };

        let err = run(&format!("http://{addr}"), opts).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Failed to create bead"));
        assert!(!out.exists());
    }
}
