use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use at_core::types::{Bead, Lane};

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
// IdeationEngine
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct IdeationEngine {
    ideas: Vec<Idea>,
}

impl IdeationEngine {
    pub fn new() -> Self {
        Self { ideas: Vec::new() }
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
}

impl Default for IdeationEngine {
    fn default() -> Self {
        Self::new()
    }
}
