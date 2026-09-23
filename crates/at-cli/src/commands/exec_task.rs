use std::time::Duration;

use anyhow::Context;
use at_core::types::TaskPhase;
use serde_json::json;

use super::{api_client, friendly_error};
use crate::commands::run_task::{self, RunOptions};

#[derive(Debug, Clone)]
pub struct ExecOptions {
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
    pub wait: bool,
    pub timeout_secs: u64,
    pub poll_ms: u64,
    pub strict: bool,
    pub json_output: bool,
    pub out_path: Option<String>,
}

async fn wait_for_task_terminal_state(
    api_url: &str,
    task_id: &str,
    timeout_secs: u64,
    poll_ms: u64,
) -> anyhow::Result<serde_json::Value> {
    let client = api_client();
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs.max(1));
    let poll = Duration::from_millis(poll_ms.max(100));

    loop {
        let resp = client
            .get(format!("{api_url}/api/tasks/{task_id}"))
            .send()
            .await
            .map_err(friendly_error)?;
        let status = resp.status();
        let body = resp.text().await.map_err(friendly_error)?;
        let json_body: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|_| {
            if body.trim().is_empty() {
                json!({})
            } else {
                json!({ "raw": body })
            }
        });

        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "failed to poll task status (HTTP {}): {}",
                status,
                json_body
            ));
        }

        // Deserialize into the server's own enum so wire-name drift is caught
        // here rather than silently polling until the deadline.
        let phase = serde_json::from_value::<TaskPhase>(json_body["phase"].clone()).ok();
        if let Some(phase) = phase.filter(TaskPhase::is_terminal) {
            let wire = serde_json::to_value(&phase)?;
            return Ok(json!({
                "task": json_body,
                "terminal_phase": wire,
                "success": phase.is_success(),
            }));
        }

        if std::time::Instant::now() >= deadline {
            return Ok(json!({
                "task": json_body,
                "terminal_phase": "timeout",
            }));
        }

        tokio::time::sleep(poll).await;
    }
}

/// Terminal phases that fail `--strict`: anything but a successful
/// `complete` once we actually waited ("not_waited" is not a failure).
fn strict_failure(terminal_phase: &str) -> bool {
    matches!(terminal_phase, "error" | "stopped" | "timeout")
}

pub async fn run(api_url: &str, opts: ExecOptions) -> anyhow::Result<()> {
    let artifact_path = opts.out_path.clone().unwrap_or_else(|| {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("at-exec-{ts}.json"))
            .display()
            .to_string()
    });

    let run_opts = RunOptions {
        task: opts.task,
        skills: opts.skills,
        project_path: opts.project_path,
        model: opts.model,
        max_agents: opts.max_agents,
        lane: opts.lane,
        category: opts.category,
        priority: opts.priority,
        complexity: opts.complexity,
        no_execute: opts.no_execute,
        dry_run: false,
        emit_prompt: false,
        json_output: false,
        out_path: Some(artifact_path.clone()),
        role: None,
    };

    run_task::run(api_url, run_opts).await?;

    let artifact = std::fs::read_to_string(&artifact_path)
        .with_context(|| format!("failed to read execution artifact at {artifact_path}"))?;
    let mut payload: serde_json::Value = serde_json::from_str(&artifact)
        .with_context(|| format!("failed to parse execution artifact at {artifact_path}"))?;

    if opts.wait {
        let task_id = payload["task_id"]
            .as_str()
            .context("execution artifact missing task_id")?
            .to_string();
        let wait_result =
            wait_for_task_terminal_state(api_url, &task_id, opts.timeout_secs, opts.poll_ms)
                .await?;
        payload["wait_result"] = wait_result;
    }

    let terminal_phase = payload["wait_result"]["terminal_phase"]
        .as_str()
        .unwrap_or("not_waited");

    if opts.strict && strict_failure(terminal_phase) {
        return Err(anyhow::anyhow!(
            "exec finished in terminal phase '{}' (strict mode)",
            terminal_phase
        ));
    }

    if opts.json_output {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!(
            "Exec task id: {}",
            payload["task_id"].as_str().unwrap_or("<unknown>")
        );
        println!(
            "  executed: {}",
            payload["executed"].as_bool().unwrap_or(false)
        );
        println!("  terminal_phase: {terminal_phase}");
        println!("  artifact: {artifact_path}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::Path as AxPath,
        http::StatusCode,
        routing::{get, post},
        Json, Router,
    };

    #[tokio::test]
    async fn exec_waits_until_done() {
        let app = Router::new()
            .route(
                "/api/beads",
                post(|Json(_body): Json<serde_json::Value>| async move {
                    (
                        StatusCode::CREATED,
                        Json(json!({"id": "bead-exec-1", "title": "exec"})),
                    )
                }),
            )
            .route(
                "/api/tasks",
                post(|Json(_body): Json<serde_json::Value>| async move {
                    (
                        StatusCode::CREATED,
                        Json(json!({"id": "task-exec-1", "title": "exec"})),
                    )
                }),
            )
            .route(
                "/api/tasks/{id}/execute",
                post(
                    |AxPath(_id): AxPath<String>| async move { (StatusCode::ACCEPTED, "accepted") },
                ),
            )
            .route(
                "/api/tasks/{id}",
                get(|AxPath(_id): AxPath<String>| async move {
                    (
                        StatusCode::OK,
                        Json(json!({"id":"task-exec-1","phase":"complete"})),
                    )
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let opts = ExecOptions {
            task: "Exec test".to_string(),
            skills: vec![],
            project_path: ".".to_string(),
            model: None,
            max_agents: None,
            lane: "standard".to_string(),
            category: "feature".to_string(),
            priority: "medium".to_string(),
            complexity: "medium".to_string(),
            no_execute: false,
            wait: true,
            timeout_secs: 2,
            poll_ms: 100,
            strict: true,
            json_output: true,
            out_path: None,
        };

        run(&format!("http://{addr}"), opts).await.unwrap();
    }

    /// Mock `GET /api/tasks/{id}` that reports `coding` for the first
    /// `busy_polls` calls and then `final_phase` — the real server's wire names.
    async fn start_phase_server(final_phase: &'static str, busy_polls: usize) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let calls = Arc::new(AtomicUsize::new(0));
        let app = Router::new().route(
            "/api/tasks/{id}",
            get(move |AxPath(id): AxPath<String>| {
                let calls = calls.clone();
                async move {
                    let n = calls.fetch_add(1, Ordering::SeqCst);
                    let phase = if n < busy_polls { "coding" } else { final_phase };
                    Json(json!({"id": id, "phase": phase}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn wait_stops_on_real_terminal_phases() {
        for (phase, success) in [("complete", true), ("error", false), ("stopped", false)] {
            let base = start_phase_server(phase, 2).await;
            let started = std::time::Instant::now();
            let out = wait_for_task_terminal_state(&base, "t1", 10, 100)
                .await
                .unwrap();
            assert_eq!(out["terminal_phase"], phase);
            assert_eq!(out["success"], success);
            assert!(started.elapsed() < Duration::from_secs(5), "{phase} waited too long");
        }
    }

    #[tokio::test]
    async fn wait_times_out_on_non_terminal_phase() {
        let base = start_phase_server("coding", usize::MAX).await;
        let out = wait_for_task_terminal_state(&base, "t1", 1, 100)
            .await
            .unwrap();
        assert_eq!(out["terminal_phase"], "timeout");
    }

    #[test]
    fn strict_mode_only_fails_unsuccessful_phases() {
        assert!(!strict_failure("complete"));
        assert!(!strict_failure("not_waited"));
        assert!(strict_failure("error"));
        assert!(strict_failure("stopped"));
        assert!(strict_failure("timeout"));
    }
}
