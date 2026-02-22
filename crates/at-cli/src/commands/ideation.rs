use serde::{Deserialize, Serialize};
use reqwest::Client;

#[derive(Debug, Serialize, Deserialize)]
struct Idea {
    id: String,
    title: String,
    description: String,
    category: String,
    impact: String,
    effort: String,
    source: String,
}

#[derive(Debug, Serialize, Deserialize)]
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
        println!("{} [{}] ({}) -> {}", idea.id, idea.category, idea.impact, idea.title);
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
        println!("{} [{}] ({}) -> {}", idea.id, idea.category, idea.impact, idea.title);
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
