use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use at_core::types::{Bead, Lane};

use crate::llm::{LlmConfig, LlmMessage, LlmProvider};

// ---------------------------------------------------------------------------
// IdeaCategory
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdeaCategory {
    CodeImprovement,
    Quality,
    Documentation,
    Performance,
    Security,
    UiUx,
}

// ---------------------------------------------------------------------------
// ImpactLevel / EffortLevel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffortLevel {
    Trivial,
    Small,
    Medium,
    Large,
    Massive,
}

// ---------------------------------------------------------------------------
// Idea
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Idea {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub category: IdeaCategory,
    pub impact: ImpactLevel,
    pub effort: EffortLevel,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// IdeationResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeationResult {
    pub ideas: Vec<Idea>,
    pub analysis_type: String,
    pub generated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// JSON schema for parsing LLM responses
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LlmIdeaJson {
    title: String,
    description: String,
    #[serde(default = "default_impact")]
    impact: String,
    #[serde(default = "default_effort")]
    effort: String,
}

fn default_impact() -> String {
    "medium".to_string()
}

fn default_effort() -> String {
    "medium".to_string()
}

#[derive(Debug, Deserialize)]
struct LlmIdeasResponseJson {
    ideas: Vec<LlmIdeaJson>,
}

// ---------------------------------------------------------------------------
// IdeationEngine
// ---------------------------------------------------------------------------

pub struct IdeationEngine {
    ideas: Vec<Idea>,
    provider: Option<Arc<dyn LlmProvider>>,
    default_model: String,
}

impl std::fmt::Debug for IdeationEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdeationEngine")
            .field("ideas", &self.ideas)
            .field("has_provider", &self.provider.is_some())
            .finish()
    }
}

impl IdeationEngine {
    pub fn new() -> Self {
        Self {
            ideas: Vec::new(),
            provider: None,
            default_model: "claude-sonnet-4-20250514".into(),
        }
    }

    /// Create an engine **with** an LLM provider for AI-powered ideation.
    pub fn with_provider(provider: Arc<dyn LlmProvider>, default_model: impl Into<String>) -> Self {
        Self {
            ideas: Vec::new(),
            provider: Some(provider),
            default_model: default_model.into(),
        }
    }

    /// Generate ideas for a given category and context.
    ///
    /// In a production system this would call an LLM; here we produce
    /// deterministic placeholder ideas so the API surface is exercisable
    /// and testable without a network call.
    pub fn generate_ideas(
        &mut self,
        analysis_type: &IdeaCategory,
        context: &str,
    ) -> IdeationResult {
        let category_label = match analysis_type {
            IdeaCategory::CodeImprovement => "code_improvement",
            IdeaCategory::Quality => "quality",
            IdeaCategory::Documentation => "documentation",
            IdeaCategory::Performance => "performance",
            IdeaCategory::Security => "security",
            IdeaCategory::UiUx => "ui_ux",
        };

        let idea = Idea {
            id: Uuid::new_v4(),
            title: format!("Improve {category_label} based on analysis"),
            description: format!("Analysed context: {context}"),
            category: analysis_type.clone(),
            impact: ImpactLevel::Medium,
            effort: EffortLevel::Small,
            source: "auto-analysis".to_string(),
            created_at: Utc::now(),
        };

        self.ideas.push(idea.clone());

        IdeationResult {
            ideas: vec![idea],
            analysis_type: category_label.to_string(),
            generated_at: Utc::now(),
        }
    }

    // -----------------------------------------------------------------------
    // AI-powered ideation
    // -----------------------------------------------------------------------

    /// Use the configured LLM to analyse `context` and generate structured
    /// ideas for the given `category`.
    ///
    /// The method builds a prompt, calls the LLM, and then attempts to parse
    /// the response as JSON.  If JSON parsing fails it falls back to simple
    /// line-based text parsing so that the engine is resilient to varying
    /// LLM output formats.
    pub async fn generate_ideas_with_ai(
        &mut self,
        category: &IdeaCategory,
        context: &str,
    ) -> Result<IdeationResult, crate::IntelligenceError> {
        let provider = self
            .provider
            .as_ref()
            .ok_or_else(|| {
                crate::IntelligenceError::InvalidOperation(
                    "No LLM provider configured – use IdeationEngine::with_provider()".into(),
                )
            })?
            .clone();

        let category_label = Self::category_label(category);

        let system_prompt = format!(
            "You are a senior software engineer performing a codebase analysis.\n\
             Analyse the provided context and generate actionable improvement ideas \
             in the \"{category_label}\" category.\n\n\
             Respond with a JSON object containing an \"ideas\" array. Each idea object \
             must have:\n  \
             - \"title\": short title\n  \
             - \"description\": one-paragraph explanation\n  \
             - \"impact\": one of \"low\", \"medium\", \"high\", \"critical\"\n  \
             - \"effort\": one of \"trivial\", \"small\", \"medium\", \"large\", \"massive\"\n\n\
             Return ONLY valid JSON, no markdown fences."
        );

        let messages = vec![
            LlmMessage::system(system_prompt),
            LlmMessage::user(format!(
                "Category: {category_label}\n\nCodebase context:\n{context}"
            )),
        ];

        let config = LlmConfig {
            model: self.default_model.clone(),
            max_tokens: 1024,
            temperature: 0.7,
            system_prompt: None,
        };

        let response = provider
            .complete(&messages, &config)
            .await
            .map_err(|e| {
                crate::IntelligenceError::InvalidOperation(format!("LLM call failed: {e}"))
            })?;

        // Try JSON parsing first, fall back to text parsing.
        let ideas = self.parse_ideas_json(&response.content, category)
            .unwrap_or_else(|| self.parse_ideas_text(&response.content, category));

        // Store the generated ideas.
        for idea in &ideas {
            self.ideas.push(idea.clone());
        }

        Ok(IdeationResult {
            ideas,
            analysis_type: category_label.to_string(),
            generated_at: Utc::now(),
        })
    }

    // -----------------------------------------------------------------------
    // Existing read-only methods
    // -----------------------------------------------------------------------

    pub fn list_ideas(&self) -> &[Idea] {
        &self.ideas
    }

    pub fn get_idea(&self, id: &Uuid) -> Option<&Idea> {
        self.ideas.iter().find(|i| i.id == *id)
    }

    /// Convert an idea to an at-core Bead so it can be tracked in the
    /// orchestrator pipeline.
    pub fn convert_to_task(&self, idea_id: &Uuid) -> Option<Bead> {
        let idea = self.get_idea(idea_id)?;
        let mut bead = Bead::new(&idea.title, Lane::Standard);
        bead.description = Some(idea.description.clone());
        Some(bead)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn category_label(cat: &IdeaCategory) -> &'static str {
        match cat {
            IdeaCategory::CodeImprovement => "code_improvement",
            IdeaCategory::Quality => "quality",
            IdeaCategory::Documentation => "documentation",
            IdeaCategory::Performance => "performance",
            IdeaCategory::Security => "security",
            IdeaCategory::UiUx => "ui_ux",
        }
    }

    /// Attempt to parse the LLM response as our expected JSON schema.
    fn parse_ideas_json(&self, text: &str, category: &IdeaCategory) -> Option<Vec<Idea>> {
        // Strip markdown code fences if the LLM wrapped output in them.
        let cleaned = text
            .trim()
            .strip_prefix("```json")
            .or_else(|| text.trim().strip_prefix("```"))
            .unwrap_or(text.trim());
        let cleaned = cleaned
            .strip_suffix("```")
            .unwrap_or(cleaned)
            .trim();

        let parsed: LlmIdeasResponseJson = serde_json::from_str(cleaned).ok()?;
        if parsed.ideas.is_empty() {
            return None;
        }

        let ideas = parsed
            .ideas
            .into_iter()
            .map(|raw| Idea {
                id: Uuid::new_v4(),
                title: raw.title,
                description: raw.description,
                category: category.clone(),
                impact: Self::parse_impact(&raw.impact),
                effort: Self::parse_effort(&raw.effort),
                source: "llm-analysis".to_string(),
                created_at: Utc::now(),
            })
            .collect();

        Some(ideas)
    }

    /// Fallback: treat each non-empty line as an idea title.
    fn parse_ideas_text(&self, text: &str, category: &IdeaCategory) -> Vec<Idea> {
        text.lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|line| {
                // Strip leading list markers like "- ", "* ", "1. ", etc.
                let cleaned = line
                    .trim_start_matches(|c: char| c == '-' || c == '*' || c.is_ascii_digit() || c == '.')
                    .trim();
                Idea {
                    id: Uuid::new_v4(),
                    title: cleaned.to_string(),
                    description: String::new(),
                    category: category.clone(),
                    impact: ImpactLevel::Medium,
                    effort: EffortLevel::Medium,
                    source: "llm-analysis".to_string(),
                    created_at: Utc::now(),
                }
            })
            .collect()
    }

    fn parse_impact(s: &str) -> ImpactLevel {
        match s.to_lowercase().as_str() {
            "low" => ImpactLevel::Low,
            "high" => ImpactLevel::High,
            "critical" => ImpactLevel::Critical,
            _ => ImpactLevel::Medium,
        }
    }

    fn parse_effort(s: &str) -> EffortLevel {
        match s.to_lowercase().as_str() {
            "trivial" => EffortLevel::Trivial,
            "small" => EffortLevel::Small,
            "large" => EffortLevel::Large,
            "massive" => EffortLevel::Massive,
            _ => EffortLevel::Medium,
        }
    }
}

impl Default for IdeationEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmConfig, LlmError, LlmMessage, LlmProvider, LlmResponse, LlmRole};
    use std::pin::Pin;
    use std::sync::Mutex;
    use futures_util::Stream;

    // ---- MockProvider --------------------------------------------------------

    struct MockProvider {
        response: String,
        calls: Mutex<Vec<(Vec<LlmMessage>, LlmConfig)>>,
    }

    impl MockProvider {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn captured_calls(&self) -> Vec<(Vec<LlmMessage>, LlmConfig)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(
            &self,
            messages: &[LlmMessage],
            config: &LlmConfig,
        ) -> Result<LlmResponse, LlmError> {
            self.calls.lock().unwrap().push((messages.to_vec(), config.clone()));
            Ok(LlmResponse {
                content: self.response.clone(),
                model: "mock".to_string(),
                input_tokens: 10,
                output_tokens: 5,
                finish_reason: "end_turn".to_string(),
            })
        }

        async fn stream(
            &self,
            _messages: &[LlmMessage],
            _config: &LlmConfig,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
            Err(LlmError::Unsupported("mock does not support streaming".into()))
        }
    }

    // ---- Tests ---------------------------------------------------------------

    #[tokio::test]
    async fn generate_ideas_with_ai_parses_json_response() {
        let json_response = r#"{"ideas":[
            {"title":"Add query indexes","description":"Adding indexes on frequently queried columns will reduce p99 latency.","impact":"high","effort":"small"},
            {"title":"Enable connection pooling","description":"Use pgbouncer to pool database connections.","impact":"medium","effort":"trivial"}
        ]}"#;

        let mock = Arc::new(MockProvider::new(json_response));
        let mut engine = IdeationEngine::with_provider(mock.clone(), "mock");

        let result = engine
            .generate_ideas_with_ai(&IdeaCategory::Performance, "slow DB queries")
            .await
            .unwrap();

        assert_eq!(result.ideas.len(), 2);
        assert_eq!(result.analysis_type, "performance");
        assert_eq!(result.ideas[0].title, "Add query indexes");
        assert_eq!(result.ideas[0].impact, ImpactLevel::High);
        assert_eq!(result.ideas[0].effort, EffortLevel::Small);
        assert_eq!(result.ideas[1].title, "Enable connection pooling");
        assert_eq!(result.ideas[1].effort, EffortLevel::Trivial);

        // All ideas should be stored in the engine
        assert_eq!(engine.list_ideas().len(), 2);

        // Verify the prompt was sent correctly
        let calls = mock.captured_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0[0].role, LlmRole::System);
        assert!(calls[0].0[0].content.contains("performance"));
        assert_eq!(calls[0].0[1].role, LlmRole::User);
        assert!(calls[0].0[1].content.contains("slow DB queries"));
    }

    #[tokio::test]
    async fn generate_ideas_with_ai_falls_back_to_text_parsing() {
        // Non-JSON response -- the engine should still produce ideas.
        let text_response = "- Refactor the auth module\n- Add unit tests for login flow\n";

        let mock = Arc::new(MockProvider::new(text_response));
        let mut engine = IdeationEngine::with_provider(mock, "mock-model");

        let result = engine
            .generate_ideas_with_ai(&IdeaCategory::CodeImprovement, "auth code")
            .await
            .unwrap();

        assert_eq!(result.ideas.len(), 2);
        assert_eq!(result.ideas[0].title, "Refactor the auth module");
        assert_eq!(result.ideas[1].title, "Add unit tests for login flow");
        // Fallback defaults
        assert_eq!(result.ideas[0].impact, ImpactLevel::Medium);
        assert_eq!(result.ideas[0].effort, EffortLevel::Medium);
    }

    #[tokio::test]
    async fn generate_ideas_with_ai_handles_markdown_fenced_json() {
        let fenced = "```json\n{\"ideas\":[{\"title\":\"Improve caching\",\"description\":\"Add Redis.\",\"impact\":\"high\",\"effort\":\"medium\"}]}\n```";

        let mock = Arc::new(MockProvider::new(fenced));
        let mut engine = IdeationEngine::with_provider(mock, "mock");

        let result = engine
            .generate_ideas_with_ai(&IdeaCategory::Performance, "caching")
            .await
            .unwrap();

        assert_eq!(result.ideas.len(), 1);
        assert_eq!(result.ideas[0].title, "Improve caching");
    }

    #[tokio::test]
    async fn generate_ideas_with_ai_no_provider_returns_error() {
        let mut engine = IdeationEngine::new();

        let result = engine
            .generate_ideas_with_ai(&IdeaCategory::Security, "context")
            .await;

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("No LLM provider"));
    }

    #[test]
    fn sync_generate_ideas_still_works() {
        let mut engine = IdeationEngine::new();
        let result = engine.generate_ideas(&IdeaCategory::Performance, "slow queries");

        assert_eq!(result.ideas.len(), 1);
        assert_eq!(result.analysis_type, "performance");
        assert_eq!(engine.list_ideas().len(), 1);
    }

    #[test]
    fn backward_compat_convert_to_task() {
        let mut engine = IdeationEngine::new();
        let result = engine.generate_ideas(&IdeaCategory::CodeImprovement, "refactor");
        let id = result.ideas[0].id;

        let bead = engine.convert_to_task(&id).unwrap();
        assert!(bead.title.contains("code_improvement"));
        assert!(bead.description.is_some());
        assert!(engine.convert_to_task(&Uuid::new_v4()).is_none());
    }
}
