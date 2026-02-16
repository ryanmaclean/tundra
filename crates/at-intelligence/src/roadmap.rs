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
