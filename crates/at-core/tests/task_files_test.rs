//! Exhaustive tests for TaskFile / TaskFiles types — the file management
//! layer that backs the "Files" tab in the UI.

use at_core::types::*;
use uuid::Uuid;

// ===========================================================================
// Helpers
// ===========================================================================

fn make_task_file(path: &str, ft: TaskFileType, phase: TaskPhase) -> TaskFile {
    TaskFile::new(Uuid::new_v4(), path, ft, phase)
}

fn sample_task_id() -> Uuid {
    Uuid::new_v4()
}

// ===========================================================================
// Tests (20+)
// ===========================================================================

#[test]
fn create_task_file_with_spec_type() {
    let f = make_task_file("docs/spec.md", TaskFileType::Spec, TaskPhase::SpecCreation);
    assert_eq!(f.file_type, TaskFileType::Spec);
    assert_eq!(f.path, "docs/spec.md");
    assert_eq!(f.phase_added, TaskPhase::SpecCreation);
}

#[test]
fn task_file_list_for_a_task() {
    let tid = sample_task_id();
    let mut files = TaskFiles::new();
    files.add(TaskFile::new(tid, "a.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "b.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "spec.md", TaskFileType::Spec, TaskPhase::SpecCreation));

    assert_eq!(files.count(), 3);
    assert!(files.files.iter().all(|f| f.task_id == tid));
}

#[test]
fn task_file_content_spec_markdown() {
    let tid = sample_task_id();
    let mut f = TaskFile::new(tid, "spec.md", TaskFileType::Spec, TaskPhase::SpecCreation);
    f.content = Some("# Specification\n\n- Acceptance criteria 1\n- Acceptance criteria 2".into());

    assert!(f.content.as_ref().unwrap().starts_with("# Specification"));
}

#[test]
fn task_file_types_all_variants() {
    let types = vec![
        TaskFileType::Spec,
        TaskFileType::Implementation,
        TaskFileType::Test,
        TaskFileType::Config,
        TaskFileType::Documentation,
    ];
    for ft in &types {
        let json = serde_json::to_string(ft).unwrap();
        let back: TaskFileType = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, ft);
    }
}

#[test]
fn task_file_size_tracking() {
    let tid = sample_task_id();
    let mut f = TaskFile::new(tid, "big.bin", TaskFileType::Implementation, TaskPhase::Coding);
    f.size_bytes = Some(1_048_576); // 1 MiB
    f.content = None; // metadata only

    assert_eq!(f.size_bytes, Some(1_048_576));
    assert!(f.content.is_none());
}

#[test]
fn adding_files_during_different_phases() {
    let tid = sample_task_id();
    let mut files = TaskFiles::new();

    files.add(TaskFile::new(tid, "spec.md", TaskFileType::Spec, TaskPhase::SpecCreation));
    files.add(TaskFile::new(tid, "plan.md", TaskFileType::Documentation, TaskPhase::Planning));
    files.add(TaskFile::new(tid, "main.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "test.rs", TaskFileType::Test, TaskPhase::Qa));

    assert_eq!(files.by_phase(&TaskPhase::SpecCreation).len(), 1);
    assert_eq!(files.by_phase(&TaskPhase::Coding).len(), 1);
    assert_eq!(files.by_phase(&TaskPhase::Qa).len(), 1);
}

#[test]
fn file_path_validation_no_empty() {
    let f = make_task_file("", TaskFileType::Implementation, TaskPhase::Coding);
    // Path is technically empty — callers should validate
    assert_eq!(f.path, "");
}

#[test]
fn task_file_serialization_roundtrip() {
    let tid = sample_task_id();
    let mut f = TaskFile::new(tid, "src/lib.rs", TaskFileType::Implementation, TaskPhase::Coding);
    f.content = Some("fn main() {}".into());
    f.size_bytes = Some(12);
    f.subtask_id = Some(Uuid::new_v4());

    let json = serde_json::to_string(&f).unwrap();
    let back: TaskFile = serde_json::from_str(&json).unwrap();

    assert_eq!(back.task_id, tid);
    assert_eq!(back.path, "src/lib.rs");
    assert_eq!(back.file_type, TaskFileType::Implementation);
    assert_eq!(back.content.as_deref(), Some("fn main() {}"));
    assert_eq!(back.size_bytes, Some(12));
    assert!(back.subtask_id.is_some());
}

#[test]
fn empty_files_list() {
    let files = TaskFiles::new();
    assert_eq!(files.count(), 0);
    assert!(files.files.is_empty());
    assert!(files.by_type(&TaskFileType::Spec).is_empty());
}

#[test]
fn files_associated_with_specific_subtasks() {
    let tid = sample_task_id();
    let sub_a = Uuid::new_v4();
    let sub_b = Uuid::new_v4();
    let mut files = TaskFiles::new();

    let mut f1 = TaskFile::new(tid, "a.rs", TaskFileType::Implementation, TaskPhase::Coding);
    f1.subtask_id = Some(sub_a);
    let mut f2 = TaskFile::new(tid, "b.rs", TaskFileType::Implementation, TaskPhase::Coding);
    f2.subtask_id = Some(sub_b);
    let mut f3 = TaskFile::new(tid, "a_test.rs", TaskFileType::Test, TaskPhase::Qa);
    f3.subtask_id = Some(sub_a);

    files.add(f1);
    files.add(f2);
    files.add(f3);

    assert_eq!(files.by_subtask(sub_a).len(), 2);
    assert_eq!(files.by_subtask(sub_b).len(), 1);
}

#[test]
fn file_type_filtering() {
    let tid = sample_task_id();
    let mut files = TaskFiles::new();
    files.add(TaskFile::new(tid, "spec.md", TaskFileType::Spec, TaskPhase::SpecCreation));
    files.add(TaskFile::new(tid, "main.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "helper.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "test.rs", TaskFileType::Test, TaskPhase::Qa));

    assert_eq!(files.by_type(&TaskFileType::Spec).len(), 1);
    assert_eq!(files.by_type(&TaskFileType::Implementation).len(), 2);
    assert_eq!(files.by_type(&TaskFileType::Test).len(), 1);
    assert_eq!(files.by_type(&TaskFileType::Config).len(), 0);
}

#[test]
fn spec_file_present_after_planning_phase() {
    let tid = sample_task_id();
    let mut files = TaskFiles::new();

    // Simulate: spec created during SpecCreation phase
    files.add(TaskFile::new(tid, "spec.md", TaskFileType::Spec, TaskPhase::SpecCreation));

    // After planning, spec should exist
    let specs = files.by_type(&TaskFileType::Spec);
    assert!(!specs.is_empty(), "Spec file should exist after planning phase");
}

#[test]
fn implementation_files_added_during_coding_phase() {
    let tid = sample_task_id();
    let mut files = TaskFiles::new();

    files.add(TaskFile::new(tid, "feature.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "mod.rs", TaskFileType::Implementation, TaskPhase::Coding));

    let coding_files = files.by_phase(&TaskPhase::Coding);
    assert_eq!(coding_files.len(), 2);
    assert!(coding_files.iter().all(|f| f.file_type == TaskFileType::Implementation));
}

#[test]
fn test_files_added_during_qa_phase() {
    let tid = sample_task_id();
    let mut files = TaskFiles::new();

    files.add(TaskFile::new(tid, "test_feature.rs", TaskFileType::Test, TaskPhase::Qa));

    let qa_files = files.by_phase(&TaskPhase::Qa);
    assert_eq!(qa_files.len(), 1);
    assert_eq!(qa_files[0].file_type, TaskFileType::Test);
}

#[test]
fn file_count_per_phase() {
    let tid = sample_task_id();
    let mut files = TaskFiles::new();

    files.add(TaskFile::new(tid, "spec.md", TaskFileType::Spec, TaskPhase::SpecCreation));
    files.add(TaskFile::new(tid, "plan.md", TaskFileType::Documentation, TaskPhase::Planning));
    files.add(TaskFile::new(tid, "a.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "b.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "c.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "t.rs", TaskFileType::Test, TaskPhase::Qa));
    files.add(TaskFile::new(tid, "t2.rs", TaskFileType::Test, TaskPhase::Qa));

    assert_eq!(files.by_phase(&TaskPhase::SpecCreation).len(), 1);
    assert_eq!(files.by_phase(&TaskPhase::Planning).len(), 1);
    assert_eq!(files.by_phase(&TaskPhase::Coding).len(), 3);
    assert_eq!(files.by_phase(&TaskPhase::Qa).len(), 2);
    assert_eq!(files.count(), 7);
}

#[test]
fn duplicate_file_path_detection() {
    let tid = sample_task_id();
    let mut files = TaskFiles::new();

    files.add(TaskFile::new(tid, "src/lib.rs", TaskFileType::Implementation, TaskPhase::Coding));
    assert!(files.has_path("src/lib.rs"));
    assert!(!files.has_path("src/main.rs"));

    // Adding a duplicate path is allowed at the data level, but has_path detects it
    files.add(TaskFile::new(tid, "src/lib.rs", TaskFileType::Implementation, TaskPhase::Fixing));
    assert_eq!(files.files.iter().filter(|f| f.path == "src/lib.rs").count(), 2);
}

#[test]
fn file_with_none_content_metadata_only() {
    let tid = sample_task_id();
    let mut f = TaskFile::new(tid, "binary.wasm", TaskFileType::Implementation, TaskPhase::Coding);
    f.size_bytes = Some(524_288);
    f.content = None;

    assert!(f.content.is_none());
    assert_eq!(f.size_bytes, Some(524_288));
}

#[test]
fn large_file_handling_size_bytes_without_content() {
    let tid = sample_task_id();
    let mut f = TaskFile::new(tid, "model.bin", TaskFileType::Config, TaskPhase::Coding);
    f.size_bytes = Some(10_000_000_000); // 10 GB
    f.content = None;

    assert_eq!(f.size_bytes, Some(10_000_000_000));
    assert!(f.content.is_none());

    // Should serialize fine
    let json = serde_json::to_string(&f).unwrap();
    let back: TaskFile = serde_json::from_str(&json).unwrap();
    assert_eq!(back.size_bytes, Some(10_000_000_000));
}

#[test]
fn task_file_display_ordering_by_path() {
    let tid = sample_task_id();
    let mut files = TaskFiles::new();

    files.add(TaskFile::new(tid, "z_last.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "a_first.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "m_middle.rs", TaskFileType::Implementation, TaskPhase::Coding));

    // Sort by path for display
    let mut sorted: Vec<&TaskFile> = files.files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));

    assert_eq!(sorted[0].path, "a_first.rs");
    assert_eq!(sorted[1].path, "m_middle.rs");
    assert_eq!(sorted[2].path, "z_last.rs");
}

#[test]
fn file_path_normalization() {
    let tid = sample_task_id();

    let f1 = TaskFile::new(tid, "./src/lib.rs", TaskFileType::Implementation, TaskPhase::Coding);
    assert_eq!(f1.normalized_path(), "src/lib.rs");

    let f2 = TaskFile::new(tid, "src//lib.rs", TaskFileType::Implementation, TaskPhase::Coding);
    assert_eq!(f2.normalized_path(), "src/lib.rs");

    let f3 = TaskFile::new(tid, "src/lib.rs", TaskFileType::Implementation, TaskPhase::Coding);
    assert_eq!(f3.normalized_path(), "src/lib.rs");
}

#[test]
fn task_files_collection_serialization_roundtrip() {
    let tid = sample_task_id();
    let mut files = TaskFiles::new();
    files.add(TaskFile::new(tid, "a.rs", TaskFileType::Implementation, TaskPhase::Coding));
    files.add(TaskFile::new(tid, "spec.md", TaskFileType::Spec, TaskPhase::SpecCreation));

    let json = serde_json::to_string(&files).unwrap();
    let back: TaskFiles = serde_json::from_str(&json).unwrap();

    assert_eq!(back.count(), 2);
    assert!(back.has_path("a.rs"));
    assert!(back.has_path("spec.md"));
}

#[test]
fn task_files_default_is_empty() {
    let files = TaskFiles::default();
    assert_eq!(files.count(), 0);
    assert!(files.files.is_empty());
}

#[test]
fn task_file_new_sets_defaults() {
    let tid = sample_task_id();
    let f = TaskFile::new(tid, "hello.rs", TaskFileType::Implementation, TaskPhase::Coding);

    assert!(f.content.is_none());
    assert!(f.size_bytes.is_none());
    assert!(f.subtask_id.is_none());
    assert_eq!(f.task_id, tid);
}
