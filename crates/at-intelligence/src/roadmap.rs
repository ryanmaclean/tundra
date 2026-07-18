use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::IntelligenceError;

// ---------------------------------------------------------------------------
// FeatureStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureStatus {
    Proposed,
    Planned,
    InProgress,
    Complete,
    Deferred,
}

// ---------------------------------------------------------------------------
// RoadmapFeature
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadmapFeature {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub status: FeatureStatus,
    pub priority: u8,
    pub estimated_effort: String,
    pub dependencies: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl RoadmapFeature {
    pub fn new(title: impl Into<String>, description: impl Into<String>, priority: u8) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            description: description.into(),
            status: FeatureStatus::Proposed,
            priority,
            estimated_effort: String::new(),
            dependencies: Vec::new(),
            created_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// Roadmap
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Roadmap {
    pub id: Uuid,
    pub name: String,
    pub features: Vec<RoadmapFeature>,
    pub generated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// RoadmapEngine
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct RoadmapEngine {
    roadmaps: Vec<Roadmap>,
}

impl RoadmapEngine {
    pub fn new() -> Self {
        Self {
            roadmaps: Vec::new(),
        }
    }

    pub fn create_roadmap(&mut self, name: &str) -> &Roadmap {
        let roadmap = Roadmap {
            id: Uuid::new_v4(),
            name: name.to_string(),
            features: Vec::new(),
            generated_at: Utc::now(),
        };
        self.roadmaps.push(roadmap);
        self.roadmaps.last().unwrap()
    }

    pub fn add_feature(
        &mut self,
        roadmap_id: &Uuid,
        feature: RoadmapFeature,
    ) -> Result<(), IntelligenceError> {
        let roadmap = self
            .roadmaps
            .iter_mut()
            .find(|r| r.id == *roadmap_id)
            .ok_or(IntelligenceError::NotFound {
                entity: "roadmap".into(),
                id: *roadmap_id,
            })?;

        roadmap.features.push(feature);
        Ok(())
    }

    pub fn update_feature_status(
        &mut self,
        roadmap_id: &Uuid,
        feature_id: &Uuid,
        status: FeatureStatus,
    ) -> Result<(), IntelligenceError> {
        let roadmap = self
            .roadmaps
            .iter_mut()
            .find(|r| r.id == *roadmap_id)
            .ok_or(IntelligenceError::NotFound {
                entity: "roadmap".into(),
                id: *roadmap_id,
            })?;

        let feature = roadmap
            .features
            .iter_mut()
            .find(|f| f.id == *feature_id)
            .ok_or(IntelligenceError::NotFound {
                entity: "feature".into(),
                id: *feature_id,
            })?;

        feature.status = status;
        Ok(())
    }

    pub fn get_roadmap(&self, id: &Uuid) -> Option<&Roadmap> {
        self.roadmaps.iter().find(|r| r.id == *id)
    }

    pub fn list_roadmaps(&self) -> &[Roadmap] {
        &self.roadmaps
    }

    /// Parse a structured analysis string into a `Roadmap`.
    ///
    /// Each line is expected to follow the format:
    ///
    /// ```text
    /// - Feature: <title> | Description: <desc> | Priority: <N>
    /// ```
    ///
    /// Lines that do not match this pattern are silently skipped. The
    /// resulting `Roadmap` is stored in the engine and also returned by
    /// reference.
    ///
    /// This is a **synchronous parser** — the actual LLM call that produces
    /// the analysis string happens in the API layer.
    pub fn generate_from_codebase(&mut self, analysis: &str) -> &Roadmap {
        let mut features = Vec::new();

        for line in analysis.lines() {
            let trimmed = line.trim().trim_start_matches('-').trim();
            if trimmed.is_empty() {
                continue;
            }

            // Split on '|' and look for the expected key-value segments.
            let parts: Vec<&str> = trimmed.split('|').collect();
            let mut title: Option<&str> = None;
            let mut description: Option<&str> = None;
            let mut priority: u8 = 5; // default mid-range

            for part in &parts {
                let part = part.trim();
                if let Some(val) = part.strip_prefix("Feature:") {
                    title = Some(val.trim());
                } else if let Some(val) = part.strip_prefix("Description:") {
                    description = Some(val.trim());
                } else if let Some(val) = part.strip_prefix("Priority:") {
                    priority = val.trim().parse::<u8>().unwrap_or(5);
                }
            }

            if let Some(t) = title {
                let desc = description.unwrap_or("");
                features.push(RoadmapFeature::new(t, desc, priority));
            }
        }

        let roadmap = Roadmap {
            id: Uuid::new_v4(),
            name: "Generated Roadmap".to_string(),
            features,
            generated_at: Utc::now(),
        };
        self.roadmaps.push(roadmap);
        self.roadmaps.last().unwrap()
    }

    pub fn reorder_features(
        &mut self,
        roadmap_id: &Uuid,
        feature_ids: &[Uuid],
    ) -> Result<(), IntelligenceError> {
        let roadmap = self
            .roadmaps
            .iter_mut()
            .find(|r| r.id == *roadmap_id)
            .ok_or(IntelligenceError::NotFound {
                entity: "roadmap".into(),
                id: *roadmap_id,
            })?;

        // Validate that all provided IDs exist in the roadmap
        for id in feature_ids {
            if !roadmap.features.iter().any(|f| f.id == *id) {
                return Err(IntelligenceError::NotFound {
                    entity: "feature".into(),
                    id: *id,
                });
            }
        }

        // Reorder: features matching the provided order come first,
        // any remaining features keep their relative order after.
        let mut reordered = Vec::with_capacity(roadmap.features.len());
        for id in feature_ids {
            if let Some(pos) = roadmap.features.iter().position(|f| f.id == *id) {
                reordered.push(roadmap.features[pos].clone());
            }
        }
        for feature in &roadmap.features {
            if !feature_ids.contains(&feature.id) {
                reordered.push(feature.clone());
            }
        }
        roadmap.features = reordered;
        Ok(())
    }
}

impl Default for RoadmapEngine {
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

    // --- RoadmapEngine::new / create_roadmap / list_roadmaps / get_roadmap --

    #[test]
    fn test_new_creates_empty_engine() {
        let engine = RoadmapEngine::new();
        assert!(engine.list_roadmaps().is_empty());
    }

    #[test]
    fn test_create_roadmap_stores_with_correct_name() {
        let mut engine = RoadmapEngine::new();
        let roadmap = engine.create_roadmap("Q3 Plan");
        assert_eq!(roadmap.name, "Q3 Plan");
        assert!(roadmap.features.is_empty());

        assert_eq!(engine.list_roadmaps().len(), 1);
        assert_eq!(engine.list_roadmaps()[0].name, "Q3 Plan");
    }

    #[test]
    fn test_list_roadmaps_returns_all_in_order() {
        let mut engine = RoadmapEngine::new();
        engine.create_roadmap("Alpha");
        engine.create_roadmap("Beta");
        engine.create_roadmap("Gamma");

        let names: Vec<&str> = engine.list_roadmaps().iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "Beta", "Gamma"]);
    }

    #[test]
    fn test_get_roadmap_found_returns_correct_roadmap() {
        let mut engine = RoadmapEngine::new();
        let id = engine.create_roadmap("My Roadmap").id;

        let found = engine.get_roadmap(&id).unwrap();
        assert_eq!(found.name, "My Roadmap");
        assert_eq!(found.id, id);
    }

    #[test]
    fn test_get_roadmap_not_found_returns_none() {
        let engine = RoadmapEngine::new();
        assert!(engine.get_roadmap(&Uuid::new_v4()).is_none());
    }

    // --- add_feature ---------------------------------------------------------

    #[test]
    fn test_add_feature_to_existing_roadmap_succeeds() {
        let mut engine = RoadmapEngine::new();
        let roadmap_id = engine.create_roadmap("Plan").id;
        let feature = RoadmapFeature::new("Dark mode", "Support dark theme", 3);

        let result = engine.add_feature(&roadmap_id, feature);
        assert!(result.is_ok());

        let roadmap = engine.get_roadmap(&roadmap_id).unwrap();
        assert_eq!(roadmap.features.len(), 1);
        assert_eq!(roadmap.features[0].title, "Dark mode");
        assert_eq!(roadmap.features[0].priority, 3);
        assert_eq!(roadmap.features[0].status, FeatureStatus::Proposed);
    }

    #[test]
    fn test_add_feature_unknown_roadmap_returns_not_found_error() {
        let mut engine = RoadmapEngine::new();
        let unknown_id = Uuid::new_v4();
        let feature = RoadmapFeature::new("Feature X", "Desc", 1);

        let err = engine.add_feature(&unknown_id, feature).unwrap_err();
        assert!(matches!(err, IntelligenceError::NotFound { entity, .. } if entity == "roadmap"));
    }

    // --- update_feature_status -----------------------------------------------

    #[test]
    fn test_update_feature_status_success() {
        let mut engine = RoadmapEngine::new();
        let roadmap_id = engine.create_roadmap("Plan").id;
        let feature = RoadmapFeature::new("Feature A", "Desc", 1);
        let feature_id = feature.id;
        engine.add_feature(&roadmap_id, feature).unwrap();

        let result = engine.update_feature_status(&roadmap_id, &feature_id, FeatureStatus::InProgress);
        assert!(result.is_ok());

        let roadmap = engine.get_roadmap(&roadmap_id).unwrap();
        assert_eq!(roadmap.features[0].status, FeatureStatus::InProgress);
    }

    #[test]
    fn test_update_feature_status_to_complete() {
        let mut engine = RoadmapEngine::new();
        let roadmap_id = engine.create_roadmap("Plan").id;
        let feature = RoadmapFeature::new("Feature B", "Desc", 2);
        let feature_id = feature.id;
        engine.add_feature(&roadmap_id, feature).unwrap();

        engine.update_feature_status(&roadmap_id, &feature_id, FeatureStatus::Complete).unwrap();

        let roadmap = engine.get_roadmap(&roadmap_id).unwrap();
        assert_eq!(roadmap.features[0].status, FeatureStatus::Complete);
    }

    #[test]
    fn test_update_feature_status_unknown_roadmap_returns_error() {
        let mut engine = RoadmapEngine::new();
        let unknown_roadmap = Uuid::new_v4();
        let unknown_feature = Uuid::new_v4();

        let err = engine
            .update_feature_status(&unknown_roadmap, &unknown_feature, FeatureStatus::Planned)
            .unwrap_err();
        assert!(matches!(err, IntelligenceError::NotFound { entity, .. } if entity == "roadmap"));
    }

    #[test]
    fn test_update_feature_status_unknown_feature_returns_error() {
        let mut engine = RoadmapEngine::new();
        let roadmap_id = engine.create_roadmap("Plan").id;
        let unknown_feature = Uuid::new_v4();

        let err = engine
            .update_feature_status(&roadmap_id, &unknown_feature, FeatureStatus::Planned)
            .unwrap_err();
        assert!(matches!(err, IntelligenceError::NotFound { entity, .. } if entity == "feature"));
    }

    // --- generate_from_codebase ----------------------------------------------

    #[test]
    fn test_generate_from_codebase_parses_valid_line() {
        let mut engine = RoadmapEngine::new();
        let analysis = "- Feature: Dark mode | Description: Add dark theme support | Priority: 2";
        let roadmap = engine.generate_from_codebase(analysis);

        assert_eq!(roadmap.features.len(), 1);
        assert_eq!(roadmap.features[0].title, "Dark mode");
        assert_eq!(roadmap.features[0].description, "Add dark theme support");
        assert_eq!(roadmap.features[0].priority, 2);
    }

    #[test]
    fn test_generate_from_codebase_default_priority_when_missing() {
        let mut engine = RoadmapEngine::new();
        let analysis = "- Feature: SSO login | Description: Add single sign-on";
        let roadmap = engine.generate_from_codebase(analysis);

        assert_eq!(roadmap.features[0].priority, 5);
    }

    #[test]
    fn test_generate_from_codebase_default_priority_when_invalid() {
        let mut engine = RoadmapEngine::new();
        let analysis = "- Feature: X | Description: Y | Priority: notanumber";
        let roadmap = engine.generate_from_codebase(analysis);

        assert_eq!(roadmap.features[0].priority, 5);
    }

    #[test]
    fn test_generate_from_codebase_skips_lines_without_feature_key() {
        let mut engine = RoadmapEngine::new();
        let analysis = "This line has no feature key\n- Feature: Valid | Description: OK | Priority: 1";
        let roadmap = engine.generate_from_codebase(analysis);

        assert_eq!(roadmap.features.len(), 1);
        assert_eq!(roadmap.features[0].title, "Valid");
    }

    #[test]
    fn test_generate_from_codebase_skips_empty_lines() {
        let mut engine = RoadmapEngine::new();
        let analysis = "\n\n- Feature: Good | Description: Desc | Priority: 3\n\n";
        let roadmap = engine.generate_from_codebase(analysis);

        assert_eq!(roadmap.features.len(), 1);
    }

    #[test]
    fn test_generate_from_codebase_multiple_features() {
        let mut engine = RoadmapEngine::new();
        let analysis = "- Feature: Alpha | Description: First | Priority: 1\n\
                        - Feature: Beta | Description: Second | Priority: 8";
        let roadmap = engine.generate_from_codebase(analysis);

        assert_eq!(roadmap.features.len(), 2);
        assert_eq!(roadmap.features[0].title, "Alpha");
        assert_eq!(roadmap.features[1].title, "Beta");
        assert_eq!(roadmap.features[1].priority, 8);
    }

    #[test]
    fn test_generate_from_codebase_stores_roadmap_in_engine() {
        let mut engine = RoadmapEngine::new();
        engine.generate_from_codebase("- Feature: X | Description: Y | Priority: 1");

        assert_eq!(engine.list_roadmaps().len(), 1);
        assert_eq!(engine.list_roadmaps()[0].name, "Generated Roadmap");
    }

    #[test]
    fn test_generate_from_codebase_features_start_as_proposed() {
        let mut engine = RoadmapEngine::new();
        let roadmap = engine.generate_from_codebase("- Feature: F | Description: D | Priority: 5");

        assert_eq!(roadmap.features[0].status, FeatureStatus::Proposed);
    }

    // --- reorder_features ----------------------------------------------------

    #[test]
    fn test_reorder_features_puts_specified_ids_first() {
        let mut engine = RoadmapEngine::new();
        let roadmap_id = engine.create_roadmap("Plan").id;

        let f1 = RoadmapFeature::new("Alpha", "a", 1);
        let f2 = RoadmapFeature::new("Beta", "b", 2);
        let f3 = RoadmapFeature::new("Gamma", "c", 3);
        let id1 = f1.id;
        let id2 = f2.id;
        let id3 = f3.id;

        engine.add_feature(&roadmap_id, f1).unwrap();
        engine.add_feature(&roadmap_id, f2).unwrap();
        engine.add_feature(&roadmap_id, f3).unwrap();

        // Reverse order: Gamma, Beta, Alpha
        engine.reorder_features(&roadmap_id, &[id3, id2, id1]).unwrap();

        let roadmap = engine.get_roadmap(&roadmap_id).unwrap();
        assert_eq!(roadmap.features[0].id, id3);
        assert_eq!(roadmap.features[1].id, id2);
        assert_eq!(roadmap.features[2].id, id1);
    }

    #[test]
    fn test_reorder_features_partial_order_appends_remainder() {
        let mut engine = RoadmapEngine::new();
        let roadmap_id = engine.create_roadmap("Plan").id;

        let f1 = RoadmapFeature::new("Alpha", "a", 1);
        let f2 = RoadmapFeature::new("Beta", "b", 2);
        let f3 = RoadmapFeature::new("Gamma", "c", 3);
        let id1 = f1.id;
        let id3 = f3.id;

        engine.add_feature(&roadmap_id, f1).unwrap();
        engine.add_feature(&roadmap_id, f2.clone()).unwrap();
        engine.add_feature(&roadmap_id, f3).unwrap();

        // Only specify id3 and id1; f2 should be appended afterwards
        engine.reorder_features(&roadmap_id, &[id3, id1]).unwrap();

        let roadmap = engine.get_roadmap(&roadmap_id).unwrap();
        assert_eq!(roadmap.features[0].id, id3);
        assert_eq!(roadmap.features[1].id, id1);
        assert_eq!(roadmap.features[2].title, "Beta");
    }

    #[test]
    fn test_reorder_features_unknown_roadmap_returns_error() {
        let mut engine = RoadmapEngine::new();
        let err = engine.reorder_features(&Uuid::new_v4(), &[]).unwrap_err();
        assert!(matches!(err, IntelligenceError::NotFound { entity, .. } if entity == "roadmap"));
    }

    #[test]
    fn test_reorder_features_unknown_feature_returns_error() {
        let mut engine = RoadmapEngine::new();
        let roadmap_id = engine.create_roadmap("Plan").id;
        let unknown_feature = Uuid::new_v4();

        let err = engine.reorder_features(&roadmap_id, &[unknown_feature]).unwrap_err();
        assert!(matches!(err, IntelligenceError::NotFound { entity, .. } if entity == "feature"));
    }
}
