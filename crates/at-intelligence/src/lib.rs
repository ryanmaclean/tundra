pub mod changelog;
pub mod cost_tracker;
pub mod ideation;
pub mod insights;
pub mod llm;
pub mod memory;
pub mod model_router;
pub mod roadmap;
pub mod token_cache;

// Re-export canonical LLM types for convenience.
pub use llm::{
    AnthropicProvider, LlmConfig, LlmError, LlmMessage, LlmProvider, LlmResponse, LlmRole,
    LlmUsageTracker, MockProvider as LlmMockProvider, OpenAiProvider,
};

// Re-export optimization types.
pub use cost_tracker::{CostTracker, LetsMetrics, ModelPricing, QcaScore, TokenBudget};
pub use model_router::{ComplexityLevel, ModelRouter, RouteDecision, RoutingStrategy};
pub use token_cache::{CacheStats, TokenCache, TokenCacheConfig};

use thiserror::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Crate-level error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum IntelligenceError {
    #[error("{entity} with id {id} not found")]
    NotFound { entity: String, id: Uuid },

    #[error("invalid operation: {0}")]
    InvalidOperation(String),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use changelog::*;
    use chrono::Utc;
    use ideation::*;
    use insights::*;
    use memory::*;
    use roadmap::*;
    use uuid::Uuid;

    // -----------------------------------------------------------------------
    // InsightsEngine tests
    // -----------------------------------------------------------------------

    #[test]
    fn insights_create_and_list_sessions() {
        let mut engine = InsightsEngine::new();
        assert!(engine.list_sessions().is_empty());

        engine.create_session("Test Session", "gpt-4");
        assert_eq!(engine.list_sessions().len(), 1);
        assert_eq!(engine.list_sessions()[0].title, "Test Session");
        assert_eq!(engine.list_sessions()[0].model, "gpt-4");
    }

    #[test]
    fn insights_get_session() {
        let mut engine = InsightsEngine::new();
        let id = engine.create_session("Lookup", "claude-3").id;

        assert!(engine.get_session(&id).is_some());
        assert!(engine.get_session(&Uuid::new_v4()).is_none());
    }

    #[test]
    fn insights_add_message() {
        let mut engine = InsightsEngine::new();
        let id = engine.create_session("Chat", "claude-3").id;

        engine
            .add_message(&id, ChatRole::User, "Hello")
            .unwrap();
        engine
            .add_message(&id, ChatRole::Assistant, "Hi there")
            .unwrap();

        let session = engine.get_session(&id).unwrap();
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, ChatRole::User);
        assert_eq!(session.messages[1].content, "Hi there");
    }

    #[test]
    fn insights_add_message_not_found() {
        let mut engine = InsightsEngine::new();
        let result = engine.add_message(&Uuid::new_v4(), ChatRole::User, "oops");
        assert!(result.is_err());
    }

    #[test]
    fn insights_delete_session() {
        let mut engine = InsightsEngine::new();
        let id = engine.create_session("Doomed", "model").id;
        assert!(engine.delete_session(&id));
        assert!(engine.list_sessions().is_empty());
        assert!(!engine.delete_session(&id)); // already gone
    }

    // -----------------------------------------------------------------------
    // RoadmapEngine tests
    // -----------------------------------------------------------------------

    #[test]
    fn roadmap_create_and_list() {
        let mut engine = RoadmapEngine::new();
        engine.create_roadmap("v1.0");
        assert_eq!(engine.list_roadmaps().len(), 1);
        assert_eq!(engine.list_roadmaps()[0].name, "v1.0");
    }

    #[test]
    fn roadmap_add_feature_and_update_status() {
        let mut engine = RoadmapEngine::new();
        let rid = engine.create_roadmap("v2.0").id;

        let feature = RoadmapFeature::new("Auth system", "Add OAuth", 1);
        let fid = feature.id;
        engine.add_feature(&rid, feature).unwrap();

        assert_eq!(engine.get_roadmap(&rid).unwrap().features.len(), 1);

        engine
            .update_feature_status(&rid, &fid, FeatureStatus::InProgress)
            .unwrap();
        let f = &engine.get_roadmap(&rid).unwrap().features[0];
        assert_eq!(f.status, FeatureStatus::InProgress);
    }

    #[test]
    fn roadmap_update_feature_not_found() {
        let mut engine = RoadmapEngine::new();
        let rid = engine.create_roadmap("r").id;
        let result = engine.update_feature_status(&rid, &Uuid::new_v4(), FeatureStatus::Complete);
        assert!(result.is_err());
    }

    #[test]
    fn roadmap_reorder_features() {
        let mut engine = RoadmapEngine::new();
        let rid = engine.create_roadmap("r").id;

        let f1 = RoadmapFeature::new("First", "d1", 1);
        let f2 = RoadmapFeature::new("Second", "d2", 2);
        let id1 = f1.id;
        let id2 = f2.id;
        engine.add_feature(&rid, f1).unwrap();
        engine.add_feature(&rid, f2).unwrap();

        // Reverse order
        engine.reorder_features(&rid, &[id2, id1]).unwrap();
        let features = &engine.get_roadmap(&rid).unwrap().features;
        assert_eq!(features[0].id, id2);
        assert_eq!(features[1].id, id1);
    }

    // -----------------------------------------------------------------------
    // IdeationEngine tests
    // -----------------------------------------------------------------------

    #[test]
    fn ideation_generate_and_list() {
        let mut engine = IdeationEngine::new();
        let result = engine.generate_ideas(&IdeaCategory::Performance, "slow queries");

        assert_eq!(result.ideas.len(), 1);
        assert_eq!(result.analysis_type, "performance");
        assert_eq!(engine.list_ideas().len(), 1);
    }

    #[test]
    fn ideation_get_idea() {
        let mut engine = IdeationEngine::new();
        let result = engine.generate_ideas(&IdeaCategory::Security, "auth");
        let id = result.ideas[0].id;

        assert!(engine.get_idea(&id).is_some());
        assert!(engine.get_idea(&Uuid::new_v4()).is_none());
    }

    #[test]
    fn ideation_convert_to_task() {
        let mut engine = IdeationEngine::new();
        let result = engine.generate_ideas(&IdeaCategory::CodeImprovement, "refactor");
        let id = result.ideas[0].id;

        let bead = engine.convert_to_task(&id).unwrap();
        assert!(bead.title.contains("code_improvement"));
        assert!(bead.description.is_some());

        // Non-existent id returns None
        assert!(engine.convert_to_task(&Uuid::new_v4()).is_none());
    }

    // -----------------------------------------------------------------------
    // MemoryStore tests
    // -----------------------------------------------------------------------

    #[test]
    fn memory_add_and_get_entry() {
        let mut store = MemoryStore::new();
        let entry = MemoryEntry::new("api_url", "http://localhost:3000", MemoryCategory::ApiRoute, "config");
        let id = entry.id;
        store.add_entry(entry);

        let retrieved = store.get_entry(&id).unwrap();
        assert_eq!(retrieved.key, "api_url");
    }

    #[test]
    fn memory_search() {
        let mut store = MemoryStore::new();
        store.add_entry(MemoryEntry::new("db_url", "postgres://localhost", MemoryCategory::ServiceEndpoint, "env"));
        store.add_entry(MemoryEntry::new("cache_url", "redis://localhost", MemoryCategory::ServiceEndpoint, "env"));
        store.add_entry(MemoryEntry::new("log_level", "debug", MemoryCategory::EnvVar, "env"));

        let results = store.search("localhost");
        assert_eq!(results.len(), 2);

        let results = store.search("debug");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn memory_list_by_category() {
        let mut store = MemoryStore::new();
        store.add_entry(MemoryEntry::new("k1", "v1", MemoryCategory::Pattern, "src"));
        store.add_entry(MemoryEntry::new("k2", "v2", MemoryCategory::Pattern, "src"));
        store.add_entry(MemoryEntry::new("k3", "v3", MemoryCategory::EnvVar, "env"));

        assert_eq!(store.list_by_category(&MemoryCategory::Pattern).len(), 2);
        assert_eq!(store.list_by_category(&MemoryCategory::EnvVar).len(), 1);
        assert_eq!(store.list_by_category(&MemoryCategory::Architecture).len(), 0);
    }

    #[test]
    fn memory_link_entries() {
        let mut store = MemoryStore::new();
        let e1 = MemoryEntry::new("a", "v", MemoryCategory::Pattern, "s");
        let e2 = MemoryEntry::new("b", "v", MemoryCategory::Pattern, "s");
        let id1 = e1.id;
        let id2 = e2.id;
        store.add_entry(e1);
        store.add_entry(e2);

        store.link_entries(&id1, &id2).unwrap();
        assert!(store.get_entry(&id1).unwrap().related.contains(&id2));

        // Linking to non-existent entry should error
        assert!(store.link_entries(&id1, &Uuid::new_v4()).is_err());
    }

    #[test]
    fn memory_update_and_delete() {
        let mut store = MemoryStore::new();
        let entry = MemoryEntry::new("key", "old_value", MemoryCategory::Convention, "s");
        let id = entry.id;
        store.add_entry(entry);

        store.update_entry(&id, "new_value").unwrap();
        assert_eq!(store.get_entry(&id).unwrap().value, "new_value");

        assert!(store.delete_entry(&id));
        assert!(store.get_entry(&id).is_none());
        assert!(!store.delete_entry(&id)); // already deleted
    }

    // -----------------------------------------------------------------------
    // ChangelogEngine tests
    // -----------------------------------------------------------------------

    #[test]
    fn changelog_add_and_list() {
        let mut engine = ChangelogEngine::new();
        let entry = ChangelogEntry {
            id: Uuid::new_v4(),
            version: "0.1.0".to_string(),
            date: Utc::now(),
            sections: vec![ChangelogSection {
                category: ChangeCategory::Added,
                items: vec!["Initial release".to_string()],
            }],
        };
        let id = entry.id;
        engine.add_entry(entry);

        assert_eq!(engine.list_entries().len(), 1);
        assert!(engine.get_entry(&id).is_some());
    }

    #[test]
    fn changelog_generate_markdown() {
        let mut engine = ChangelogEngine::new();
        engine.add_entry(ChangelogEntry {
            id: Uuid::new_v4(),
            version: "0.2.0".to_string(),
            date: Utc::now(),
            sections: vec![
                ChangelogSection {
                    category: ChangeCategory::Added,
                    items: vec!["New feature A".to_string(), "New feature B".to_string()],
                },
                ChangelogSection {
                    category: ChangeCategory::Fixed,
                    items: vec!["Bug fix C".to_string()],
                },
            ],
        });

        let md = engine.generate_markdown();
        assert!(md.contains("# Changelog"));
        assert!(md.contains("## [0.2.0]"));
        assert!(md.contains("### Added"));
        assert!(md.contains("- New feature A"));
        assert!(md.contains("### Fixed"));
        assert!(md.contains("- Bug fix C"));
    }

    // -----------------------------------------------------------------------
    // Serde roundtrip tests
    // -----------------------------------------------------------------------

    #[test]
    fn serde_roundtrip_chat_message() {
        let msg = ChatMessage {
            role: ChatRole::Assistant,
            content: "Hello world".to_string(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, msg.role);
        assert_eq!(deserialized.content, msg.content);
    }

    #[test]
    fn serde_roundtrip_roadmap_feature() {
        let feature = RoadmapFeature::new("Auth", "OAuth impl", 1);
        let json = serde_json::to_string(&feature).unwrap();
        let deserialized: RoadmapFeature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.title, "Auth");
        assert_eq!(deserialized.status, FeatureStatus::Proposed);
    }

    #[test]
    fn serde_roundtrip_memory_entry() {
        let entry = MemoryEntry::new("key", "value", MemoryCategory::Keyword, "test");
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.key, "key");
        assert_eq!(deserialized.category, MemoryCategory::Keyword);
    }

    #[test]
    fn serde_roundtrip_changelog_entry() {
        let entry = ChangelogEntry {
            id: Uuid::new_v4(),
            version: "1.0.0".to_string(),
            date: Utc::now(),
            sections: vec![ChangelogSection {
                category: ChangeCategory::Security,
                items: vec!["Patched CVE".to_string()],
            }],
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: ChangelogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.version, "1.0.0");
        assert_eq!(deserialized.sections[0].category, ChangeCategory::Security);
    }

    #[test]
    fn serde_roundtrip_idea() {
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
    }
}
