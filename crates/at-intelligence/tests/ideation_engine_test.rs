//! Exhaustive integration tests for IdeationEngine (Ideation feature).

use std::pin::Pin;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use futures_util::Stream;
use uuid::Uuid;

use at_intelligence::ideation::{
    EffortLevel, Idea, IdeaCategory, IdeationEngine, IdeationResult, ImpactLevel,
};
use at_intelligence::llm::{LlmConfig, LlmError, LlmMessage, LlmProvider, LlmResponse, LlmRole};

// ---------------------------------------------------------------------------
// MockProvider
// ---------------------------------------------------------------------------

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

#[async_trait]
impl LlmProvider for MockProvider {
    async fn complete(
        &self,
        messages: &[LlmMessage],
        config: &LlmConfig,
    ) -> Result<LlmResponse, LlmError> {
        self.calls
            .lock()
            .unwrap()
            .push((messages.to_vec(), config.clone()));
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
        Err(LlmError::Unsupported(
            "mock does not support streaming".into(),
        ))
    }
}

// ===========================================================================
// Idea Generation (sync, deterministic)
// ===========================================================================

#[test]
fn test_generate_ideas_for_code_improvement() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::CodeImprovement, "refactor auth module");

    assert_eq!(result.ideas.len(), 1);
    assert_eq!(result.analysis_type, "code_improvement");
    assert!(result.ideas[0].title.contains("code_improvement"));
    assert!(result.ideas[0].description.contains("refactor auth module"));
    assert_eq!(result.ideas[0].category, IdeaCategory::CodeImprovement);
}

#[test]
fn test_generate_ideas_for_quality() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Quality, "add unit tests");

    assert_eq!(result.ideas.len(), 1);
    assert_eq!(result.analysis_type, "quality");
    assert_eq!(result.ideas[0].category, IdeaCategory::Quality);
}

#[test]
fn test_generate_ideas_for_performance() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Performance, "slow database queries");

    assert_eq!(result.ideas.len(), 1);
    assert_eq!(result.analysis_type, "performance");
    assert_eq!(result.ideas[0].category, IdeaCategory::Performance);
    assert!(result.ideas[0]
        .description
        .contains("slow database queries"));
}

#[test]
fn test_generate_ideas_for_security() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Security, "input validation");

    assert_eq!(result.ideas.len(), 1);
    assert_eq!(result.analysis_type, "security");
    assert_eq!(result.ideas[0].category, IdeaCategory::Security);
}

#[test]
fn test_generate_ideas_for_ui_ux() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::UiUx, "dashboard layout");

    assert_eq!(result.ideas.len(), 1);
    assert_eq!(result.analysis_type, "ui_ux");
    assert_eq!(result.ideas[0].category, IdeaCategory::UiUx);
}

#[test]
fn test_generate_ideas_for_documentation() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Documentation, "API docs missing");

    assert_eq!(result.ideas.len(), 1);
    assert_eq!(result.analysis_type, "documentation");
    assert_eq!(result.ideas[0].category, IdeaCategory::Documentation);
}

// ===========================================================================
// Idea Properties
// ===========================================================================

#[test]
fn test_idea_has_unique_id() {
    let mut engine = IdeationEngine::new();
    let r1 = engine.generate_ideas(&IdeaCategory::Performance, "ctx1");
    let r2 = engine.generate_ideas(&IdeaCategory::Security, "ctx2");

    assert_ne!(r1.ideas[0].id, r2.ideas[0].id);
    assert!(!r1.ideas[0].id.is_nil());
    assert!(!r2.ideas[0].id.is_nil());
}

#[test]
fn test_idea_has_title_and_description() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Quality, "test coverage");

    let idea = &result.ideas[0];
    assert!(!idea.title.is_empty());
    assert!(!idea.description.is_empty());
    assert!(idea.title.contains("quality"));
    assert!(idea.description.contains("test coverage"));
}

#[test]
fn test_idea_has_category_badge() {
    let mut engine = IdeationEngine::new();

    let categories = vec![
        (IdeaCategory::CodeImprovement, IdeaCategory::CodeImprovement),
        (IdeaCategory::Quality, IdeaCategory::Quality),
        (IdeaCategory::Performance, IdeaCategory::Performance),
        (IdeaCategory::Security, IdeaCategory::Security),
        (IdeaCategory::UiUx, IdeaCategory::UiUx),
        (IdeaCategory::Documentation, IdeaCategory::Documentation),
    ];

    for (input_cat, expected_cat) in categories {
        let result = engine.generate_ideas(&input_cat, "context");
        assert_eq!(result.ideas[0].category, expected_cat);
    }
}

#[test]
fn test_idea_has_impact_level() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Performance, "ctx");

    // Deterministic generator uses Medium impact
    assert_eq!(result.ideas[0].impact, ImpactLevel::Medium);
}

#[test]
fn test_idea_has_effort_level() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Performance, "ctx");

    // Deterministic generator uses Small effort
    assert_eq!(result.ideas[0].effort, EffortLevel::Small);
}

#[test]
fn test_idea_has_source_attribution() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Quality, "ctx");

    assert_eq!(result.ideas[0].source, "auto-analysis");
}

// ===========================================================================
// AI-Powered Ideation
// ===========================================================================

#[tokio::test]
async fn test_generate_ideas_with_ai_json_response() {
    let json_response = r#"{"ideas":[
        {"title":"Add query indexes","description":"Adding indexes on frequently queried columns will reduce p99 latency.","impact":"high","effort":"small"},
        {"title":"Enable connection pooling","description":"Use pgbouncer to pool database connections.","impact":"medium","effort":"trivial"}
    ]}"#;

    let mock = Arc::new(MockProvider::new(json_response));
    let mut engine = IdeationEngine::with_provider(mock.clone(), "test-model");

    let result = engine
        .generate_ideas_with_ai(&IdeaCategory::Performance, "slow DB queries")
        .await
        .unwrap();

    assert_eq!(result.ideas.len(), 2);
    assert_eq!(result.analysis_type, "performance");
    assert_eq!(result.ideas[0].title, "Add query indexes");
    assert_eq!(result.ideas[0].impact, ImpactLevel::High);
    assert_eq!(result.ideas[0].effort, EffortLevel::Small);
    assert_eq!(result.ideas[0].category, IdeaCategory::Performance);
    assert_eq!(result.ideas[0].source, "llm-analysis");
    assert_eq!(result.ideas[1].title, "Enable connection pooling");
    assert_eq!(result.ideas[1].effort, EffortLevel::Trivial);

    // Verify ideas are stored in the engine
    assert_eq!(engine.list_ideas().len(), 2);

    // Verify prompt structure
    let calls = mock.captured_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0[0].role, LlmRole::System);
    assert!(calls[0].0[0].content.contains("performance"));
    assert_eq!(calls[0].0[1].role, LlmRole::User);
    assert!(calls[0].0[1].content.contains("slow DB queries"));
}

#[tokio::test]
async fn test_generate_ideas_with_ai_text_fallback() {
    // Non-JSON response — the engine should still produce ideas via text parsing.
    let text_response = "- Refactor the auth module\n- Add unit tests for login flow\n";

    let mock = Arc::new(MockProvider::new(text_response));
    let mut engine = IdeationEngine::with_provider(mock, "test-model");

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
    assert_eq!(result.ideas[0].source, "llm-analysis");
}

#[tokio::test]
async fn test_generate_ideas_with_ai_markdown_fenced() {
    let fenced = "```json\n{\"ideas\":[{\"title\":\"Improve caching\",\"description\":\"Add Redis.\",\"impact\":\"high\",\"effort\":\"medium\"}]}\n```";

    let mock = Arc::new(MockProvider::new(fenced));
    let mut engine = IdeationEngine::with_provider(mock, "test-model");

    let result = engine
        .generate_ideas_with_ai(&IdeaCategory::Performance, "caching")
        .await
        .unwrap();

    assert_eq!(result.ideas.len(), 1);
    assert_eq!(result.ideas[0].title, "Improve caching");
    assert_eq!(result.ideas[0].impact, ImpactLevel::High);
    assert_eq!(result.ideas[0].effort, EffortLevel::Medium);
}

#[tokio::test]
async fn test_generate_ideas_with_ai_no_provider_error() {
    let mut engine = IdeationEngine::new(); // no provider

    let result = engine
        .generate_ideas_with_ai(&IdeaCategory::Security, "context")
        .await;

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("No LLM provider"));
}

// ===========================================================================
// Idea Filtering
// ===========================================================================

#[test]
fn test_filter_ideas_by_category() {
    let mut engine = IdeationEngine::new();
    engine.generate_ideas(&IdeaCategory::Performance, "perf ctx");
    engine.generate_ideas(&IdeaCategory::Security, "sec ctx");
    engine.generate_ideas(&IdeaCategory::Performance, "perf ctx 2");
    engine.generate_ideas(&IdeaCategory::Quality, "quality ctx");

    let all = engine.list_ideas();
    assert_eq!(all.len(), 4);

    // Filter by category manually (the engine provides list_ideas, filtering is
    // done at the caller level or API level)
    let perf_ideas: Vec<&Idea> = all
        .iter()
        .filter(|i| i.category == IdeaCategory::Performance)
        .collect();
    assert_eq!(perf_ideas.len(), 2);

    let sec_ideas: Vec<&Idea> = all
        .iter()
        .filter(|i| i.category == IdeaCategory::Security)
        .collect();
    assert_eq!(sec_ideas.len(), 1);

    let quality_ideas: Vec<&Idea> = all
        .iter()
        .filter(|i| i.category == IdeaCategory::Quality)
        .collect();
    assert_eq!(quality_ideas.len(), 1);
}

#[test]
fn test_filter_ideas_by_impact_level() {
    let mut engine = IdeationEngine::new();
    // All deterministic ideas have Medium impact
    engine.generate_ideas(&IdeaCategory::Performance, "ctx1");
    engine.generate_ideas(&IdeaCategory::Security, "ctx2");

    let all = engine.list_ideas();
    let medium: Vec<&Idea> = all
        .iter()
        .filter(|i| i.impact == ImpactLevel::Medium)
        .collect();
    assert_eq!(medium.len(), 2);

    let high: Vec<&Idea> = all
        .iter()
        .filter(|i| i.impact == ImpactLevel::High)
        .collect();
    assert_eq!(high.len(), 0);
}

#[test]
fn test_list_all_ideas() {
    let mut engine = IdeationEngine::new();
    assert!(engine.list_ideas().is_empty());

    engine.generate_ideas(&IdeaCategory::Performance, "ctx1");
    engine.generate_ideas(&IdeaCategory::Security, "ctx2");
    engine.generate_ideas(&IdeaCategory::Quality, "ctx3");

    assert_eq!(engine.list_ideas().len(), 3);
}

#[test]
fn test_get_idea_by_id() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Security, "auth");
    let id = result.ideas[0].id;

    let idea = engine.get_idea(&id);
    assert!(idea.is_some());
    assert_eq!(idea.unwrap().id, id);
    assert_eq!(idea.unwrap().category, IdeaCategory::Security);

    // Non-existent id returns None
    assert!(engine.get_idea(&Uuid::new_v4()).is_none());
}

// ===========================================================================
// Idea-to-Task Conversion
// ===========================================================================

#[test]
fn test_convert_idea_to_bead() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::CodeImprovement, "refactor");
    let id = result.ideas[0].id;

    let bead = engine.convert_to_task(&id);
    assert!(bead.is_some());

    let bead = bead.unwrap();
    assert!(!bead.id.is_nil());
}

#[test]
fn test_converted_bead_has_idea_title() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::CodeImprovement, "refactor");
    let id = result.ideas[0].id;

    let bead = engine.convert_to_task(&id).unwrap();
    assert!(bead.title.contains("code_improvement"));
}

#[test]
fn test_converted_bead_has_idea_description() {
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Performance, "slow queries");
    let id = result.ideas[0].id;

    let bead = engine.convert_to_task(&id).unwrap();
    assert!(bead.description.is_some());
    assert!(bead.description.unwrap().contains("slow queries"));
}

#[test]
fn test_convert_nonexistent_idea_returns_none() {
    let engine = IdeationEngine::new();
    assert!(engine.convert_to_task(&Uuid::new_v4()).is_none());
}

// ===========================================================================
// Additional coverage
// ===========================================================================

#[test]
fn test_default_creates_empty_engine() {
    let engine = IdeationEngine::default();
    assert!(engine.list_ideas().is_empty());
}

#[test]
fn test_serde_roundtrip_idea() {
    let idea = Idea {
        id: Uuid::new_v4(),
        title: "Optimize DB".to_string(),
        description: "Add indexes".to_string(),
        category: IdeaCategory::Performance,
        impact: ImpactLevel::High,
        effort: EffortLevel::Medium,
        source: "analysis".to_string(),
        created_at: Utc::now(),
    };
    let json = serde_json::to_string(&idea).unwrap();
    let deserialized: Idea = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.title, "Optimize DB");
    assert_eq!(deserialized.category, IdeaCategory::Performance);
    assert_eq!(deserialized.impact, ImpactLevel::High);
    assert_eq!(deserialized.effort, EffortLevel::Medium);
}

#[test]
fn test_serde_roundtrip_ideation_result() {
    let result = IdeationResult {
        ideas: vec![Idea {
            id: Uuid::new_v4(),
            title: "Test".to_string(),
            description: "Desc".to_string(),
            category: IdeaCategory::Security,
            impact: ImpactLevel::Critical,
            effort: EffortLevel::Massive,
            source: "test".to_string(),
            created_at: Utc::now(),
        }],
        analysis_type: "security".to_string(),
        generated_at: Utc::now(),
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: IdeationResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.ideas.len(), 1);
    assert_eq!(deserialized.analysis_type, "security");
    assert_eq!(deserialized.ideas[0].impact, ImpactLevel::Critical);
    assert_eq!(deserialized.ideas[0].effort, EffortLevel::Massive);
}

#[test]
fn test_idea_category_variants_distinct() {
    let cats = [
        IdeaCategory::CodeImprovement,
        IdeaCategory::Quality,
        IdeaCategory::Performance,
        IdeaCategory::Security,
        IdeaCategory::UiUx,
        IdeaCategory::Documentation,
    ];
    for i in 0..cats.len() {
        for j in (i + 1)..cats.len() {
            assert_ne!(cats[i], cats[j]);
        }
    }
}

#[test]
fn test_impact_level_variants() {
    assert_ne!(ImpactLevel::Low, ImpactLevel::Medium);
    assert_ne!(ImpactLevel::Medium, ImpactLevel::High);
    assert_ne!(ImpactLevel::High, ImpactLevel::Critical);
}

#[test]
fn test_effort_level_variants() {
    assert_ne!(EffortLevel::Trivial, EffortLevel::Small);
    assert_ne!(EffortLevel::Small, EffortLevel::Medium);
    assert_ne!(EffortLevel::Medium, EffortLevel::Large);
    assert_ne!(EffortLevel::Large, EffortLevel::Massive);
}

#[tokio::test]
async fn test_ai_ideas_stored_in_engine() {
    let json_response = r#"{"ideas":[
        {"title":"Idea A","description":"Desc A","impact":"low","effort":"large"},
        {"title":"Idea B","description":"Desc B","impact":"critical","effort":"massive"}
    ]}"#;

    let mock = Arc::new(MockProvider::new(json_response));
    let mut engine = IdeationEngine::with_provider(mock, "test-model");

    engine
        .generate_ideas_with_ai(&IdeaCategory::Quality, "ctx")
        .await
        .unwrap();

    assert_eq!(engine.list_ideas().len(), 2);

    // Verify we can retrieve each by id
    let ideas = engine.list_ideas().to_vec();
    for idea in &ideas {
        let found = engine.get_idea(&idea.id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().title, idea.title);
    }
}

#[tokio::test]
async fn test_ai_ideas_impact_effort_parsing() {
    let json_response = r#"{"ideas":[
        {"title":"A","description":"D","impact":"low","effort":"trivial"},
        {"title":"B","description":"D","impact":"high","effort":"small"},
        {"title":"C","description":"D","impact":"critical","effort":"large"},
        {"title":"D","description":"D","impact":"unknown","effort":"unknown"}
    ]}"#;

    let mock = Arc::new(MockProvider::new(json_response));
    let mut engine = IdeationEngine::with_provider(mock, "test-model");

    let result = engine
        .generate_ideas_with_ai(&IdeaCategory::Performance, "ctx")
        .await
        .unwrap();

    assert_eq!(result.ideas[0].impact, ImpactLevel::Low);
    assert_eq!(result.ideas[0].effort, EffortLevel::Trivial);
    assert_eq!(result.ideas[1].impact, ImpactLevel::High);
    assert_eq!(result.ideas[1].effort, EffortLevel::Small);
    assert_eq!(result.ideas[2].impact, ImpactLevel::Critical);
    assert_eq!(result.ideas[2].effort, EffortLevel::Large);
    // Unknown values default to Medium
    assert_eq!(result.ideas[3].impact, ImpactLevel::Medium);
    assert_eq!(result.ideas[3].effort, EffortLevel::Medium);
}

#[test]
fn test_idea_created_at_timestamp() {
    let before = Utc::now();
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Quality, "ctx");
    let after = Utc::now();

    let ts = result.ideas[0].created_at;
    assert!(ts >= before);
    assert!(ts <= after);
}

#[test]
fn test_generated_at_timestamp() {
    let before = Utc::now();
    let mut engine = IdeationEngine::new();
    let result = engine.generate_ideas(&IdeaCategory::Quality, "ctx");
    let after = Utc::now();

    assert!(result.generated_at >= before);
    assert!(result.generated_at <= after);
}
