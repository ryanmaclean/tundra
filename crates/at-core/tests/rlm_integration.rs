//! Integration tests for the RLM (Recursive Language Model) module.
//!
//! These tests pin the contract for end-to-end decomposition workflows:
//! constructing a `Decomposition`, adding subtasks, recording results,
//! synthesizing output, recursing into child decompositions, and
//! round-tripping through serde.
//!
//! Notes on the API:
//! - `Decomposition::new(task, max_depth)` is the public entry point. The
//!   crate does NOT auto-split a string into subtasks; callers (e.g.
//!   `at-agents/orchestrator`) supply the subtask descriptions. These tests
//!   therefore exercise the manual decomposition contract that downstream
//!   crates rely on.
//! - There is no fallible constructor, so no error-path test is included.
//! - Subtask IDs are random UUIDs; tests assert on structure (counts,
//!   relationships, sequences, status), never on specific UUID values.

use at_core::rlm::{
    ContextFold, Decomposition, ProgressiveRefinement, SubTaskStatus, SynthesisStrategy,
};

/// Trivial input: a single-step task produces a single-node decomposition
/// and is "complete" once that node is recorded.
#[test]
fn trivial_single_step_decomposition() {
    let mut dec = Decomposition::new("Run the linter", 3);
    let id = dec.add_subtask("cargo clippy --all-targets");

    assert_eq!(dec.subtasks.len(), 1);
    assert_eq!(dec.depth, 0);
    assert!(!dec.is_complete(), "pending subtask should not be complete");

    dec.record_result(&id, "ok");
    assert!(dec.is_complete());
    assert_eq!(dec.synthesize(), "ok");
}

/// Two-level decomposition: parent task with three children. Verifies count,
/// pending state, and that synthesis preserves insertion order regardless
/// of HashMap iteration order.
#[test]
fn two_level_decomposition_with_three_children() {
    let mut dec = Decomposition::new("Build feature with X, Y, Z", 3);
    let x = dec.add_subtask("Implement X");
    let y = dec.add_subtask("Implement Y");
    let z = dec.add_subtask("Implement Z");

    assert_eq!(dec.subtasks.len(), 3);
    assert_eq!(dec.pending_subtasks().len(), 3);

    // Record results out of insertion order — synthesis must still be ordered.
    dec.record_result(&z, "z-done");
    dec.record_result(&x, "x-done");
    dec.record_result(&y, "y-done");

    assert!(dec.is_complete());
    let synth = dec.synthesize();
    let xi = synth.find("x-done").expect("x present");
    let yi = synth.find("y-done").expect("y present");
    let zi = synth.find("z-done").expect("z present");
    assert!(xi < yi && yi < zi, "synthesis must respect insertion order");
}

/// Recursive child decomposition: depth increments, max_depth is inherited,
/// and the recursion guard fires at the boundary.
#[test]
fn recursive_child_decomposition_respects_max_depth() {
    let root = Decomposition::new("root", 2);
    assert_eq!(root.depth, 0);
    assert!(root.can_recurse());

    let level1 = root.child("level-1 work");
    assert_eq!(level1.depth, 1);
    assert_eq!(level1.max_depth, 2);
    assert!(level1.can_recurse());

    let level2 = level1.child("level-2 work");
    assert_eq!(level2.depth, 2);
    assert!(
        !level2.can_recurse(),
        "depth == max_depth must block further recursion"
    );
}

/// Termination on small input: a max_depth of 0 forbids any recursion at all.
#[test]
fn termination_below_recursion_threshold() {
    let dec = Decomposition::new("tiny task", 0);
    assert_eq!(dec.depth, 0);
    assert_eq!(dec.max_depth, 0);
    assert!(
        !dec.can_recurse(),
        "max_depth=0 should prevent any recursion"
    );
}

/// Idempotence (structural): building the same decomposition twice with the
/// same inputs yields the same observable shape (count, descriptions,
/// statuses, synthesized output). UUIDs are intentionally non-equal because
/// the API generates them fresh — that is documented behaviour.
#[test]
fn idempotent_structural_shape() {
    fn build() -> Decomposition {
        let mut d = Decomposition::new("repeatable task", 3);
        let a = d.add_subtask("step-a");
        let b = d.add_subtask("step-b");
        d.record_result(&a, "A");
        d.record_result(&b, "B");
        d
    }

    let one = build();
    let two = build();

    assert_eq!(one.task_description, two.task_description);
    assert_eq!(one.subtasks.len(), two.subtasks.len());
    assert_eq!(one.max_depth, two.max_depth);
    assert_eq!(one.depth, two.depth);
    assert_eq!(one.synthesize(), two.synthesize());

    let mut descs_one: Vec<&str> = one.subtasks.values().map(|s| s.description.as_str()).collect();
    let mut descs_two: Vec<&str> = two.subtasks.values().map(|s| s.description.as_str()).collect();
    descs_one.sort();
    descs_two.sort();
    assert_eq!(descs_one, descs_two);
}

/// Serde round-trip: pins the wire format used by `at-agents/orchestrator`
/// when it stores decompositions across boundaries.
#[test]
fn decomposition_serde_round_trip() {
    let mut dec = Decomposition::new("ship feature", 4);
    dec.synthesis = SynthesisStrategy::Refine;
    let a = dec.add_subtask("design");
    let b = dec.add_subtask("implement");
    dec.add_subtask("test");
    dec.record_result(&a, "design-doc");
    dec.record_result(&b, "code-merged");

    let json = serde_json::to_string(&dec).expect("serialize");
    let back: Decomposition = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(back.task_description, dec.task_description);
    assert_eq!(back.subtasks.len(), 3);
    assert_eq!(back.synthesis, SynthesisStrategy::Refine);
    assert_eq!(back.max_depth, dec.max_depth);
    assert_eq!(back.synthesize(), dec.synthesize());

    // Status of the recorded subtask survives the round-trip.
    assert_eq!(
        back.subtasks.get(&a).expect("a present").status,
        SubTaskStatus::Complete
    );
}

/// Failure propagation through a partially-complete decomposition.
#[test]
fn failure_blocks_completion() {
    let mut dec = Decomposition::new("flaky build", 2);
    let ok = dec.add_subtask("compile");
    let bad = dec.add_subtask("integration-test");
    dec.record_result(&ok, "compiled");
    assert!(dec.mark_failed(&bad));

    assert!(dec.has_failures());
    assert!(
        !dec.is_complete(),
        "a failed sibling must prevent overall completion"
    );
}

/// Large input bounded: a long task description and many subtasks complete
/// in linear time and bounded memory; synthesize() returns a string that
/// includes every result and the structure has the expected shape (no
/// implicit recursion blew the stack).
#[test]
fn large_input_bounded_decomposition() {
    let big_desc = "x".repeat(50_000);
    let mut dec = Decomposition::new(big_desc, 5);

    // 200 sibling subtasks at depth 0 — well under any sane recursion limit.
    let mut ids = Vec::with_capacity(200);
    for i in 0..200 {
        ids.push(dec.add_subtask(format!("step-{i}")));
    }
    assert_eq!(dec.subtasks.len(), 200);

    for (i, id) in ids.iter().enumerate() {
        dec.record_result(id, format!("r{i}"));
    }
    assert!(dec.is_complete());

    let synth = dec.synthesize();
    assert!(synth.contains("r0"));
    assert!(synth.contains("r199"));
    // Tree depth stays at 0 — caller did not recurse, so depth must not drift.
    assert_eq!(dec.depth, 0);
}

/// ContextFold integration: build a fold from a multi-section markdown
/// document, auto-detect sections, and verify slice/search behaviour
/// end-to-end. This is the "context folding" half of RLM.
#[test]
fn context_fold_end_to_end() {
    let doc = "# Overview\nIntro line\n## Step One\nDo X\n## Step Two\nDo Y";
    let mut fold = ContextFold::new("plan", doc);
    fold.auto_detect_sections();

    assert!(fold.line_count() >= 6);
    assert!(fold.sections.contains_key("overview"));
    assert!(fold.sections.contains_key("step_one"));
    assert!(fold.sections.contains_key("step_two"));

    let step_one = fold.get_section("step_one").expect("step_one present");
    assert!(step_one.contains("Do X"));

    let hits = fold.search("Do");
    assert_eq!(hits.len(), 2, "should find 'Do X' and 'Do Y'");

    // Round-trip preserves sections.
    let json = serde_json::to_string(&fold).unwrap();
    let back: ContextFold = serde_json::from_str(&json).unwrap();
    assert_eq!(back.sections.len(), fold.sections.len());
}

/// ProgressiveRefinement end-to-end: revise up to the cap, finalize, and
/// verify the latest revision wins.
#[test]
fn progressive_refinement_end_to_end() {
    let mut pr = ProgressiveRefinement::new("draft answer", 3);
    assert!(pr.can_revise());

    assert!(pr.revise("v1", None, 0.4));
    assert!(pr.revise("v2", Some("clarified".into()), 0.7));
    assert!(pr.revise("v3", Some("polished".into()), 0.95));
    assert!(
        !pr.revise("v4", None, 1.0),
        "should refuse revisions past the cap"
    );

    assert_eq!(pr.revision_count(), 3);
    assert!(pr.is_confident(0.9));

    let latest = pr.latest().expect("latest");
    assert_eq!(latest.version, 3);
    assert_eq!(latest.content, "v3");

    pr.finalize();
    assert!(!pr.can_revise());
    assert!(!pr.revise("v5", None, 1.0));
}
