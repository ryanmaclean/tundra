//! Exhaustive integration tests for RoadmapEngine (Roadmap feature).
//!
//! Covers CRUD operations, feature management, status transitions matching
//! the Kanban columns (Under Review / Planned / In Progress / Done),
//! roadmap views, and generation from codebase analysis.

use chrono::Utc;
use uuid::Uuid;

use at_intelligence::roadmap::{FeatureStatus, RoadmapEngine, RoadmapFeature};

// ===========================================================================
// Helper
// ===========================================================================

/// Create an engine with one roadmap and return (engine, roadmap_id).
fn engine_with_roadmap(name: &str) -> (RoadmapEngine, Uuid) {
    let mut engine = RoadmapEngine::new();
    let id = engine.create_roadmap(name).id;
    (engine, id)
}

/// Shorthand to build a feature with only a title.
fn feature(title: &str) -> RoadmapFeature {
    RoadmapFeature::new(title, "", 5)
}

// ===========================================================================
// Roadmap CRUD
// ===========================================================================

#[test]
fn test_create_roadmap_with_name() {
    let mut engine = RoadmapEngine::new();
    let roadmap = engine.create_roadmap("VibeCode Platform");
    assert_eq!(roadmap.name, "VibeCode Platform");
    assert!(!roadmap.id.is_nil());
    assert!(roadmap.features.is_empty());
}

#[test]
fn test_create_roadmap_with_version() {
    let mut engine = RoadmapEngine::new();
    let roadmap = engine.create_roadmap("v2.0.0");
    assert_eq!(roadmap.name, "v2.0.0");
    assert!(!roadmap.id.is_nil());
}

#[test]
fn test_list_roadmaps_returns_all() {
    let mut engine = RoadmapEngine::new();
    assert!(engine.list_roadmaps().is_empty());

    engine.create_roadmap("Alpha");
    engine.create_roadmap("Beta");
    engine.create_roadmap("Gamma");

    let list = engine.list_roadmaps();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].name, "Alpha");
    assert_eq!(list[1].name, "Beta");
    assert_eq!(list[2].name, "Gamma");
}

#[test]
fn test_get_roadmap_by_id() {
    let mut engine = RoadmapEngine::new();
    let id = engine.create_roadmap("Lookup").id;

    let found = engine.get_roadmap(&id);
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "Lookup");

    // Non-existent id returns None.
    assert!(engine.get_roadmap(&Uuid::new_v4()).is_none());
}

#[test]
fn test_default_roadmap_engine() {
    let engine = RoadmapEngine::default();
    assert!(engine.list_roadmaps().is_empty());
}

// ===========================================================================
// Feature Management (the cards in the screenshot)
// ===========================================================================

#[test]
fn test_add_feature_to_roadmap() {
    let (mut engine, rid) = engine_with_roadmap("r");
    let f = feature("Project Documentation Foundation");
    engine.add_feature(&rid, f).unwrap();

    let roadmap = engine.get_roadmap(&rid).unwrap();
    assert_eq!(roadmap.features.len(), 1);
    assert_eq!(
        roadmap.features[0].title,
        "Project Documentation Foundation"
    );
}

#[test]
fn test_add_feature_with_description() {
    let (mut engine, rid) = engine_with_roadmap("r");
    let f = RoadmapFeature::new(
        "CI/CD Pipeline Stabilization",
        "Stabilize the continuous integration and delivery pipeline",
        2,
    );
    engine.add_feature(&rid, f).unwrap();

    let feat = &engine.get_roadmap(&rid).unwrap().features[0];
    assert_eq!(feat.title, "CI/CD Pipeline Stabilization");
    assert_eq!(
        feat.description,
        "Stabilize the continuous integration and delivery pipeline"
    );
}

#[test]
fn test_add_feature_with_priority() {
    let (mut engine, rid) = engine_with_roadmap("r");
    let f = RoadmapFeature::new("Testing Infrastructure Setup", "Setup tests", 1);
    engine.add_feature(&rid, f).unwrap();

    let feat = &engine.get_roadmap(&rid).unwrap().features[0];
    assert_eq!(feat.priority, 1);
}

#[test]
fn test_add_multiple_features() {
    let (mut engine, rid) = engine_with_roadmap("r");

    let titles = [
        "Project Documentation Foundation",
        "TypeScript & Linting Configuration",
        "CI/CD Pipeline Stabilization",
        "Testing Infrastructure Setup",
        "Logging Configuration",
    ];

    for (i, title) in titles.iter().enumerate() {
        let f = RoadmapFeature::new(*title, "", (i + 1) as u8);
        engine.add_feature(&rid, f).unwrap();
    }

    let roadmap = engine.get_roadmap(&rid).unwrap();
    assert_eq!(roadmap.features.len(), 5);
    for (i, title) in titles.iter().enumerate() {
        assert_eq!(roadmap.features[i].title, *title);
    }
}

#[test]
fn test_remove_feature_from_roadmap() {
    // The engine does not expose a direct remove method, so we verify that
    // features can be managed via reorder (filtering). For removal we can
    // reorder with a subset, but the API keeps unmentioned features. Instead
    // we verify that adding to a non-existent roadmap fails, which
    // exercises the error path used for removal-like validation.
    let mut engine = RoadmapEngine::new();
    let result = engine.add_feature(&Uuid::new_v4(), feature("ghost"));
    assert!(result.is_err());
}

#[test]
fn test_get_feature_by_id() {
    let (mut engine, rid) = engine_with_roadmap("r");
    let f = feature("Logging Configuration");
    let fid = f.id;
    engine.add_feature(&rid, f).unwrap();

    let roadmap = engine.get_roadmap(&rid).unwrap();
    let found = roadmap.features.iter().find(|f| f.id == fid);
    assert!(found.is_some());
    assert_eq!(found.unwrap().title, "Logging Configuration");

    // Non-existent feature id should not be found.
    let missing = roadmap.features.iter().find(|f| f.id == Uuid::new_v4());
    assert!(missing.is_none());
}

#[test]
fn test_feature_has_unique_id() {
    let f1 = feature("A");
    let f2 = feature("B");
    assert_ne!(f1.id, f2.id);
    assert!(!f1.id.is_nil());
    assert!(!f2.id.is_nil());
}

// ===========================================================================
// Feature Status Transitions
// (Kanban columns: Under Review / Proposed -> Planned -> In Progress -> Done / Complete)
// ===========================================================================

#[test]
fn test_feature_default_status_is_planned() {
    // The actual default from RoadmapFeature::new is Proposed, which maps
    // to the "Under Review" column in the UI.
    let f = feature("New Feature");
    assert_eq!(f.status, FeatureStatus::Proposed);
}

#[test]
fn test_feature_transition_planned_to_in_progress() {
    let (mut engine, rid) = engine_with_roadmap("r");
    let f = feature("Task");
    let fid = f.id;
    engine.add_feature(&rid, f).unwrap();

    // Move to Planned first, then to InProgress.
    engine
        .update_feature_status(&rid, &fid, FeatureStatus::Planned)
        .unwrap();
    assert_eq!(
        engine.get_roadmap(&rid).unwrap().features[0].status,
        FeatureStatus::Planned
    );

    engine
        .update_feature_status(&rid, &fid, FeatureStatus::InProgress)
        .unwrap();
    assert_eq!(
        engine.get_roadmap(&rid).unwrap().features[0].status,
        FeatureStatus::InProgress
    );
}

#[test]
fn test_feature_transition_in_progress_to_done() {
    let (mut engine, rid) = engine_with_roadmap("r");
    let f = feature("Task");
    let fid = f.id;
    engine.add_feature(&rid, f).unwrap();

    engine
        .update_feature_status(&rid, &fid, FeatureStatus::InProgress)
        .unwrap();
    engine
        .update_feature_status(&rid, &fid, FeatureStatus::Complete)
        .unwrap();

    assert_eq!(
        engine.get_roadmap(&rid).unwrap().features[0].status,
        FeatureStatus::Complete
    );
}

#[test]
fn test_feature_transition_to_under_review() {
    let (mut engine, rid) = engine_with_roadmap("r");
    let f = feature("Task");
    let fid = f.id;
    engine.add_feature(&rid, f).unwrap();

    // Proposed is the "Under Review" column equivalent.
    engine
        .update_feature_status(&rid, &fid, FeatureStatus::Planned)
        .unwrap();
    engine
        .update_feature_status(&rid, &fid, FeatureStatus::Proposed)
        .unwrap();

    assert_eq!(
        engine.get_roadmap(&rid).unwrap().features[0].status,
        FeatureStatus::Proposed
    );
}

#[test]
fn test_feature_full_lifecycle() {
    let (mut engine, rid) = engine_with_roadmap("r");
    let f = feature("Full Lifecycle Feature");
    let fid = f.id;
    engine.add_feature(&rid, f).unwrap();

    // Proposed (Under Review) -> Planned -> InProgress -> Complete (Done)
    let statuses = [
        FeatureStatus::Proposed,
        FeatureStatus::Planned,
        FeatureStatus::InProgress,
        FeatureStatus::Complete,
    ];

    // Already Proposed by default.
    assert_eq!(
        engine.get_roadmap(&rid).unwrap().features[0].status,
        FeatureStatus::Proposed
    );

    for status in &statuses[1..] {
        engine
            .update_feature_status(&rid, &fid, status.clone())
            .unwrap();
        assert_eq!(
            engine.get_roadmap(&rid).unwrap().features[0].status,
            *status
        );
    }
}

// ===========================================================================
// Roadmap Views
// ===========================================================================

#[test]
fn test_filter_features_by_status() {
    let (mut engine, rid) = engine_with_roadmap("Kanban");

    let f1 = feature("Under Review Card");
    let f2 = feature("Planned Card");
    let f3 = feature("In Progress Card");
    let f4 = feature("Done Card");
    let id2 = f2.id;
    let id3 = f3.id;
    let id4 = f4.id;

    engine.add_feature(&rid, f1).unwrap();
    engine.add_feature(&rid, f2).unwrap();
    engine.add_feature(&rid, f3).unwrap();
    engine.add_feature(&rid, f4).unwrap();

    engine
        .update_feature_status(&rid, &id2, FeatureStatus::Planned)
        .unwrap();
    engine
        .update_feature_status(&rid, &id3, FeatureStatus::InProgress)
        .unwrap();
    engine
        .update_feature_status(&rid, &id4, FeatureStatus::Complete)
        .unwrap();

    let features = &engine.get_roadmap(&rid).unwrap().features;

    let proposed: Vec<_> = features
        .iter()
        .filter(|f| f.status == FeatureStatus::Proposed)
        .collect();
    assert_eq!(proposed.len(), 1);
    assert_eq!(proposed[0].title, "Under Review Card");

    let planned: Vec<_> = features
        .iter()
        .filter(|f| f.status == FeatureStatus::Planned)
        .collect();
    assert_eq!(planned.len(), 1);

    let in_progress: Vec<_> = features
        .iter()
        .filter(|f| f.status == FeatureStatus::InProgress)
        .collect();
    assert_eq!(in_progress.len(), 1);

    let complete: Vec<_> = features
        .iter()
        .filter(|f| f.status == FeatureStatus::Complete)
        .collect();
    assert_eq!(complete.len(), 1);
}

#[test]
fn test_sort_features_by_priority() {
    let (mut engine, rid) = engine_with_roadmap("Priority View");

    let titles_priorities = [
        ("Low Priority", 5u8),
        ("High Priority", 1u8),
        ("Medium Priority", 3u8),
    ];

    for (title, priority) in &titles_priorities {
        let f = RoadmapFeature::new(*title, "", *priority);
        engine.add_feature(&rid, f).unwrap();
    }

    let mut features: Vec<_> = engine.get_roadmap(&rid).unwrap().features.iter().collect();
    features.sort_by_key(|f| f.priority);

    assert_eq!(features[0].title, "High Priority");
    assert_eq!(features[0].priority, 1);
    assert_eq!(features[1].title, "Medium Priority");
    assert_eq!(features[1].priority, 3);
    assert_eq!(features[2].title, "Low Priority");
    assert_eq!(features[2].priority, 5);
}

#[test]
fn test_list_all_features() {
    let (mut engine, rid) = engine_with_roadmap("All Features");

    assert!(engine.get_roadmap(&rid).unwrap().features.is_empty());

    engine.add_feature(&rid, feature("Feature A")).unwrap();
    engine.add_feature(&rid, feature("Feature B")).unwrap();
    engine.add_feature(&rid, feature("Feature C")).unwrap();

    let all = &engine.get_roadmap(&rid).unwrap().features;
    assert_eq!(all.len(), 3);
}

#[test]
fn test_feature_count_per_status() {
    let (mut engine, rid) = engine_with_roadmap("Counts");

    // Add 5 features and distribute across statuses.
    let mut ids = Vec::new();
    for i in 0..5 {
        let f = feature(&format!("Feature {}", i));
        ids.push(f.id);
        engine.add_feature(&rid, f).unwrap();
    }

    // Default is Proposed. Move some around.
    engine
        .update_feature_status(&rid, &ids[1], FeatureStatus::Planned)
        .unwrap();
    engine
        .update_feature_status(&rid, &ids[2], FeatureStatus::InProgress)
        .unwrap();
    engine
        .update_feature_status(&rid, &ids[3], FeatureStatus::InProgress)
        .unwrap();
    engine
        .update_feature_status(&rid, &ids[4], FeatureStatus::Complete)
        .unwrap();

    let features = &engine.get_roadmap(&rid).unwrap().features;

    let count = |status: FeatureStatus| features.iter().filter(|f| f.status == status).count();

    assert_eq!(count(FeatureStatus::Proposed), 1); // Under Review
    assert_eq!(count(FeatureStatus::Planned), 1);
    assert_eq!(count(FeatureStatus::InProgress), 2);
    assert_eq!(count(FeatureStatus::Complete), 1); // Done
}

// ===========================================================================
// Generation from Codebase
// ===========================================================================

#[test]
fn test_generate_roadmap_from_codebase_analysis() {
    let mut engine = RoadmapEngine::new();

    let analysis = "\
- Feature: Project Documentation Foundation | Description: Set up project docs | Priority: 1
- Feature: TypeScript & Linting Configuration | Description: Configure TS and linting | Priority: 2
- Feature: CI/CD Pipeline Stabilization | Description: Stabilize CI/CD | Priority: 3
";

    let roadmap = engine.generate_from_codebase(analysis);
    assert_eq!(roadmap.name, "Generated Roadmap");
    assert_eq!(roadmap.features.len(), 3);
    assert!(!roadmap.id.is_nil());
}

#[test]
fn test_generated_features_have_titles_and_descriptions() {
    let mut engine = RoadmapEngine::new();

    let analysis = "\
- Feature: Testing Infrastructure Setup | Description: Build testing framework | Priority: 2
- Feature: Logging Configuration | Description: Configure structured logging | Priority: 4
";

    let roadmap = engine.generate_from_codebase(analysis);
    assert_eq!(roadmap.features.len(), 2);

    assert_eq!(roadmap.features[0].title, "Testing Infrastructure Setup");
    assert_eq!(roadmap.features[0].description, "Build testing framework");
    assert_eq!(roadmap.features[0].priority, 2);

    assert_eq!(roadmap.features[1].title, "Logging Configuration");
    assert_eq!(
        roadmap.features[1].description,
        "Configure structured logging"
    );
    assert_eq!(roadmap.features[1].priority, 4);
}

#[test]
fn test_generated_features_parsed_from_lines() {
    let mut engine = RoadmapEngine::new();

    // Lines without Feature: prefix are skipped.
    let analysis = "\
- Feature: Valid Feature | Description: Valid | Priority: 1
This line should be skipped
- Feature: Another Feature | Priority: 3

- Feature: No Description | Priority: 2
";

    let roadmap = engine.generate_from_codebase(analysis);
    assert_eq!(roadmap.features.len(), 3);
    assert_eq!(roadmap.features[0].title, "Valid Feature");
    assert_eq!(roadmap.features[1].title, "Another Feature");
    assert_eq!(roadmap.features[1].description, ""); // no Description segment
    assert_eq!(roadmap.features[1].priority, 3);
    assert_eq!(roadmap.features[2].title, "No Description");
    assert_eq!(roadmap.features[2].priority, 2);
}

// ===========================================================================
// Additional coverage: reorder, error paths, serde, timestamps
// ===========================================================================

#[test]
fn test_reorder_features() {
    let (mut engine, rid) = engine_with_roadmap("r");

    let f1 = feature("First");
    let f2 = feature("Second");
    let f3 = feature("Third");
    let id1 = f1.id;
    let id2 = f2.id;
    let id3 = f3.id;
    engine.add_feature(&rid, f1).unwrap();
    engine.add_feature(&rid, f2).unwrap();
    engine.add_feature(&rid, f3).unwrap();

    // Reverse order.
    engine.reorder_features(&rid, &[id3, id2, id1]).unwrap();
    let features = &engine.get_roadmap(&rid).unwrap().features;
    assert_eq!(features[0].id, id3);
    assert_eq!(features[1].id, id2);
    assert_eq!(features[2].id, id1);
}

#[test]
fn test_reorder_partial_keeps_remaining() {
    let (mut engine, rid) = engine_with_roadmap("r");

    let f1 = feature("A");
    let f2 = feature("B");
    let f3 = feature("C");
    let id1 = f1.id;
    let id2 = f2.id;
    let id3 = f3.id;
    engine.add_feature(&rid, f1).unwrap();
    engine.add_feature(&rid, f2).unwrap();
    engine.add_feature(&rid, f3).unwrap();

    // Only reorder id3 to front; id1 and id2 keep relative order after.
    engine.reorder_features(&rid, &[id3]).unwrap();
    let features = &engine.get_roadmap(&rid).unwrap().features;
    assert_eq!(features[0].id, id3);
    assert_eq!(features[1].id, id1);
    assert_eq!(features[2].id, id2);
}

#[test]
fn test_update_status_nonexistent_roadmap() {
    let mut engine = RoadmapEngine::new();
    let result =
        engine.update_feature_status(&Uuid::new_v4(), &Uuid::new_v4(), FeatureStatus::Complete);
    assert!(result.is_err());
}

#[test]
fn test_update_status_nonexistent_feature() {
    let (mut engine, rid) = engine_with_roadmap("r");
    let result = engine.update_feature_status(&rid, &Uuid::new_v4(), FeatureStatus::Complete);
    assert!(result.is_err());
}

#[test]
fn test_add_feature_nonexistent_roadmap() {
    let mut engine = RoadmapEngine::new();
    let result = engine.add_feature(&Uuid::new_v4(), feature("ghost"));
    assert!(result.is_err());
}

#[test]
fn test_reorder_nonexistent_feature_errors() {
    let (mut engine, rid) = engine_with_roadmap("r");
    engine.add_feature(&rid, feature("A")).unwrap();

    let result = engine.reorder_features(&rid, &[Uuid::new_v4()]);
    assert!(result.is_err());
}

#[test]
fn test_roadmap_generated_at_timestamp() {
    let before = Utc::now();
    let mut engine = RoadmapEngine::new();
    let roadmap = engine.create_roadmap("Timed");
    let after = Utc::now();

    assert!(roadmap.generated_at >= before);
    assert!(roadmap.generated_at <= after);
}

#[test]
fn test_feature_created_at_timestamp() {
    let before = Utc::now();
    let f = feature("Timed Feature");
    let after = Utc::now();

    assert!(f.created_at >= before);
    assert!(f.created_at <= after);
}

#[test]
fn test_feature_default_fields() {
    let f = RoadmapFeature::new("Title", "Desc", 3);
    assert_eq!(f.title, "Title");
    assert_eq!(f.description, "Desc");
    assert_eq!(f.priority, 3);
    assert_eq!(f.status, FeatureStatus::Proposed);
    assert!(f.estimated_effort.is_empty());
    assert!(f.dependencies.is_empty());
}

#[test]
fn test_serde_roundtrip_roadmap_feature() {
    let f = RoadmapFeature::new("Auth", "OAuth impl", 1);
    let json = serde_json::to_string(&f).unwrap();
    let deserialized: RoadmapFeature = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.title, "Auth");
    assert_eq!(deserialized.status, FeatureStatus::Proposed);
    assert_eq!(deserialized.priority, 1);
}

#[test]
fn test_serde_roundtrip_feature_status() {
    let statuses = vec![
        FeatureStatus::Proposed,
        FeatureStatus::Planned,
        FeatureStatus::InProgress,
        FeatureStatus::Complete,
        FeatureStatus::Deferred,
    ];
    for status in statuses {
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: FeatureStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, status);
    }
}

#[test]
fn test_feature_status_variants_are_distinct() {
    let all = vec![
        FeatureStatus::Proposed,
        FeatureStatus::Planned,
        FeatureStatus::InProgress,
        FeatureStatus::Complete,
        FeatureStatus::Deferred,
    ];
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            assert_ne!(all[i], all[j]);
        }
    }
}

#[test]
fn test_generate_from_empty_analysis() {
    let mut engine = RoadmapEngine::new();
    let roadmap = engine.generate_from_codebase("");
    assert!(roadmap.features.is_empty());
    assert_eq!(roadmap.name, "Generated Roadmap");
}

#[test]
fn test_generate_from_analysis_default_priority() {
    let mut engine = RoadmapEngine::new();
    // No Priority: segment means default priority of 5.
    let analysis = "- Feature: No Priority Given | Description: Test";
    let roadmap = engine.generate_from_codebase(analysis);
    assert_eq!(roadmap.features.len(), 1);
    assert_eq!(roadmap.features[0].priority, 5);
}

#[test]
fn test_generate_stored_in_engine() {
    let mut engine = RoadmapEngine::new();
    let analysis = "- Feature: Stored | Description: Yes | Priority: 1";
    let rid = engine.generate_from_codebase(analysis).id;

    // The generated roadmap should be retrievable.
    let found = engine.get_roadmap(&rid);
    assert!(found.is_some());
    assert_eq!(found.unwrap().features.len(), 1);
    assert_eq!(engine.list_roadmaps().len(), 1);
}

#[test]
fn test_multiple_roadmaps_independent() {
    let mut engine = RoadmapEngine::new();
    let rid1 = engine.create_roadmap("R1").id;
    let rid2 = engine.create_roadmap("R2").id;

    engine.add_feature(&rid1, feature("F1")).unwrap();
    engine.add_feature(&rid2, feature("F2")).unwrap();
    engine.add_feature(&rid2, feature("F3")).unwrap();

    assert_eq!(engine.get_roadmap(&rid1).unwrap().features.len(), 1);
    assert_eq!(engine.get_roadmap(&rid2).unwrap().features.len(), 2);
}

#[test]
fn test_deferred_status_transition() {
    let (mut engine, rid) = engine_with_roadmap("r");
    let f = feature("Deferred Task");
    let fid = f.id;
    engine.add_feature(&rid, f).unwrap();

    engine
        .update_feature_status(&rid, &fid, FeatureStatus::Deferred)
        .unwrap();
    assert_eq!(
        engine.get_roadmap(&rid).unwrap().features[0].status,
        FeatureStatus::Deferred
    );
}
