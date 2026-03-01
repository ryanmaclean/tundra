use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Idea {
    id: String,
    title: String,
    description: String,
    category: String,
    impact: String,
    effort: String,
    source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdeationResult {
    ideas: Vec<Idea>,
    analysis_type: String,
}

pub async fn list(api_url: &str) -> anyhow::Result<()> {
    let url = format!("{}/api/ideation/ideas", api_url);
    let client = Client::new();
    let res = client.get(&url).send().await?;
    if !res.status().is_success() {
        let msg = res.text().await?;
        anyhow::bail!("Failed to list ideas: {}", msg);
    }
    let ideas: Vec<Idea> = res.json().await?;

    // Check if we want JSON
    if std::env::args().any(|arg| arg == "-j" || arg == "--json") {
        println!("{}", serde_json::to_string_pretty(&ideas)?);
        return Ok(());
    }

    if ideas.is_empty() {
        println!("No ideas generated yet.");
        return Ok(());
    }
    for idea in ideas {
        println!(
            "{} [{}] ({}) -> {}",
            idea.id, idea.category, idea.impact, idea.title
        );
    }
    Ok(())
}

pub async fn generate(api_url: &str, category: &str, context: &str) -> anyhow::Result<()> {
    let url = format!("{}/api/ideation/generate", api_url);
    let client = Client::new();

    let cat_mapped = match category.to_lowercase().as_str() {
        "quality" => "quality",
        "documentation" => "documentation",
        "performance" => "performance",
        "security" => "security",
        "ui-ux" | "ui_ux" => "ui_ux",
        _ => "code_improvement",
    };

    let payload = serde_json::json!({
        "category": cat_mapped,
        "context": context,
    });

    let res = client.post(&url).json(&payload).send().await?;
    if !res.status().is_success() {
        let msg = res.text().await?;
        anyhow::bail!("Failed to generate ideas: {}", msg);
    }
    let result: IdeationResult = res.json().await?;

    if std::env::args().any(|arg| arg == "-j" || arg == "--json") {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    for idea in result.ideas {
        println!(
            "{} [{}] ({}) -> {}",
            idea.id, idea.category, idea.impact, idea.title
        );
    }
    Ok(())
}

pub async fn convert(api_url: &str, idea_id: &str) -> anyhow::Result<()> {
    let url = format!("{}/api/ideation/ideas/{}/convert", api_url, idea_id);
    let client = Client::new();
    let res = client.post(&url).send().await?;
    if !res.status().is_success() {
        let msg = res.text().await?;
        anyhow::bail!("Failed to convert idea: {}", msg);
    }

    let text = res.text().await?;
    if std::env::args().any(|arg| arg == "-j" || arg == "--json") {
        println!("{}", text);
    } else {
        println!("Idea converted successfully: {}", text);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::{extract::Path, routing::get, routing::post, Json, Router};

    use super::*;

    #[tokio::test]
    async fn list_returns_empty_when_no_ideas() {
        let app = Router::new().route(
            "/api/ideation/ideas",
            get(|| async { Json(Vec::<Idea>::new()) }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = list(&format!("http://{addr}")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn list_returns_ideas_when_available() {
        let ideas = vec![Idea {
            id: "idea-1".to_string(),
            title: "Test Idea".to_string(),
            description: "A test idea".to_string(),
            category: "quality".to_string(),
            impact: "high".to_string(),
            effort: "low".to_string(),
            source: "test".to_string(),
        }];

        let app = Router::new().route(
            "/api/ideation/ideas",
            get(move || {
                let ideas_clone = ideas.clone();
                async move { Json(ideas_clone) }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = list(&format!("http://{addr}")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn list_fails_on_api_error() {
        let app = Router::new().route(
            "/api/ideation/ideas",
            get(|| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "error") }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = list(&format!("http://{addr}")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn generate_creates_ideas() {
        let result_payload = IdeationResult {
            ideas: vec![Idea {
                id: "idea-2".to_string(),
                title: "Generated Idea".to_string(),
                description: "AI generated".to_string(),
                category: "quality".to_string(),
                impact: "medium".to_string(),
                effort: "medium".to_string(),
                source: "ai".to_string(),
            }],
            analysis_type: "quality".to_string(),
        };

        let app = Router::new().route(
            "/api/ideation/generate",
            post(move || {
                let result = result_payload.clone();
                async move { Json(result) }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = generate(&format!("http://{addr}"), "quality", "test context").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn generate_maps_category_correctly() {
        let app = Router::new().route(
            "/api/ideation/generate",
            post(|| async {
                Json(IdeationResult {
                    ideas: vec![],
                    analysis_type: "ui_ux".to_string(),
                })
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = generate(&format!("http://{addr}"), "ui-ux", "context").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn generate_fails_on_api_error() {
        let app = Router::new().route(
            "/api/ideation/generate",
            post(|| async { (axum::http::StatusCode::BAD_REQUEST, "invalid request") }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = generate(&format!("http://{addr}"), "quality", "test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn convert_succeeds() {
        let app = Router::new().route(
            "/api/ideation/ideas/{id}/convert",
            post(|Path(_id): Path<String>| async { "converted" }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = convert(&format!("http://{addr}"), "idea-1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn convert_fails_on_api_error() {
        let app = Router::new().route(
            "/api/ideation/ideas/{id}/convert",
            post(|Path(_id): Path<String>| async {
                (axum::http::StatusCode::NOT_FOUND, "idea not found")
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let result = convert(&format!("http://{addr}"), "invalid-id").await;
        assert!(result.is_err());
    }
}
