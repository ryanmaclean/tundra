//! Black-box integration smoke tests for the `at` CLI binary.
//!
//! These tests follow the `tundra-cli-smoke` and `cli-task-lifecycle` skill
//! methodology: prefer `--dry-run` and `--out <tempfile>` over real side
//! effects, exercise one shape per command, and never start the daemon or
//! make network calls.
//!
//! Hard rules enforced here:
//!  * No tests start a daemon.
//!  * No tests rely on network reachability.
//!  * Every invocation passes `-u http://127.0.0.1:1` so the daemon-lockfile
//!    fallback path in `main.rs` does not vary based on host state.
//!  * Tests are independent — each one allocates its own `tempfile` paths.

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str as pstr;
use tempfile::TempDir;

/// Bogus API URL used for every command. Dry-run / help / parse-error code
/// paths never reach this URL, and any path that *would* reach it is not
/// exercised by these tests.
const FAKE_API: &str = "http://127.0.0.1:1";

/// Construct the `at` binary command with deterministic argv prefix.
fn at() -> Command {
    let mut cmd = Command::cargo_bin("at").expect("at binary built");
    // Always pin api-url so we never read the host's daemon lockfile.
    cmd.args(["-u", FAKE_API]);
    cmd
}

/// `at` without the `-u` prefix, used when we need to assert raw argv parse
/// behavior (`--help`, `--version`, unknown subcommand, missing args).
fn at_raw() -> Command {
    Command::cargo_bin("at").expect("at binary built")
}

/// Allocate a uniquely-named, RAII-guarded temporary directory for a test.
///
/// The returned [`TempDir`] is auto-removed when dropped, replacing the
/// previous `(pid, SystemTime nanos)` filename scheme that could collide on
/// coarse-clock systems or under parallel execution.
fn unique_tmp_dir(prefix: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("at-cli-it-{prefix}-"))
        .tempdir()
        .expect("tempdir creation")
}

// ---------------------------------------------------------------------------
// Argv & help (always safe, no daemon)
// ---------------------------------------------------------------------------

#[test]
fn help_top_level_succeeds() {
    at_raw()
        .arg("--help")
        .assert()
        .success()
        .stdout(pstr::contains("auto-tundra"))
        .stdout(pstr::contains("sling"))
        .stdout(pstr::contains("run"));
}

#[test]
fn version_prints_semver_ish() {
    at_raw()
        .arg("--version")
        .assert()
        .success()
        // clap prints "<bin> <version>"; match a digit.major.minor pattern.
        .stdout(predicate::function(|s: &str| {
            s.chars().filter(|c| *c == '.').count() >= 2
                && s.chars().any(|c| c.is_ascii_digit())
        }));
}

#[test]
fn no_args_invokes_status_and_attempts_daemon() {
    // With no subcommand, main.rs falls through to `commands::status::run`,
    // which hits the API. We pin a bogus URL, so the call fails — but the
    // CLI should exit non-zero with a friendly error, NOT panic.
    at().assert().failure().stderr(
        pstr::contains("Could not connect")
            .or(pstr::contains("API request failed"))
            .or(pstr::contains("error")),
    );
}

#[test]
fn unknown_subcommand_errors() {
    at_raw()
        .arg("definitely-not-a-real-subcommand")
        .assert()
        .failure()
        .stderr(pstr::contains("unrecognized").or(pstr::contains("error")));
}

#[test]
fn each_subcommand_help_succeeds() {
    // Every top-level subcommand the CLI exposes today.
    let subs: &[&[&str]] = &[
        &["status"],
        &["sling"],
        &["hook"],
        &["done"],
        &["nudge"],
        &["skill"],
        &["skill", "list"],
        &["skill", "show"],
        &["skill", "validate"],
        &["run"],
        &["exec"],
        &["agent"],
        &["agent", "run"],
        &["doctor"],
        &["ideation"],
        &["ideation", "list"],
        &["ideation", "generate"],
        &["ideation", "convert"],
        &["smoke"],
    ];

    for sub in subs {
        let mut cmd = at_raw();
        cmd.args(*sub).arg("--help");
        let assertion = cmd.assert();
        assertion.success();
    }
}

// ---------------------------------------------------------------------------
// Error handling (no daemon)
// ---------------------------------------------------------------------------

#[test]
fn run_missing_required_task_arg_errors() {
    // `at run` without `-t/--task` should fail at clap parsing with a
    // helpful message — never panic.
    at_raw()
        .arg("run")
        .assert()
        .failure()
        .stderr(pstr::contains("--task").or(pstr::contains("required")));
}

#[test]
fn agent_run_missing_required_role_errors() {
    at_raw()
        .arg("agent")
        .arg("run")
        .args(["-t", "do something"])
        .assert()
        .failure()
        .stderr(pstr::contains("--role").or(pstr::contains("required")));
}

#[test]
fn run_unknown_skill_errors_with_path_in_message() {
    // Dry-run still loads skills first; unknown skill should produce a
    // graceful error, not a panic.
    at()
        .args([
            "run",
            "--dry-run",
            "-t",
            "smoke task",
            "-s",
            "definitely-not-a-real-skill-xyz",
            "-p",
            ".",
        ])
        .assert()
        .failure()
        .stderr(
            pstr::contains("Unknown skills")
                .or(pstr::contains("definitely-not-a-real-skill-xyz")),
        );
}

#[test]
fn skill_list_missing_project_path_errors() {
    // `skill list -p <missing-path>` should fail with a path-mentioning error.
    // Build a path inside a tempdir that we deliberately never create.
    let td = unique_tmp_dir("missing-project");
    let bogus = td.path().join("does-not-exist");
    assert!(!bogus.exists(), "child path must not exist");
    at_raw()
        .args(["skill", "list", "-p"])
        .arg(&bogus)
        .assert()
        .failure()
        .stderr(pstr::contains("Project path does not exist"));
}

#[test]
fn skill_show_unknown_skill_errors() {
    // Use a temp project root with no skills — `show` of any name must fail.
    let td = unique_tmp_dir("skill-show-empty");
    let root = td.path();

    at_raw()
        .args(["skill", "show", "-s", "no-such-skill", "-p"])
        .arg(root)
        .assert()
        .failure()
        .stderr(pstr::contains("Skill not found").or(pstr::contains("no-such-skill")));
}

// ---------------------------------------------------------------------------
// Dry-run smoke (no daemon)
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_prints_marker_and_writes_artifact() {
    let td = unique_tmp_dir("run-dry");
    let out = td.path().join("out.json");
    at()
        .args([
            "run",
            "--dry-run",
            "-t",
            "smoke task title",
            "-p",
            ".",
            "-o",
        ])
        .arg(&out)
        .assert()
        .success()
        .stdout(pstr::contains("dry-run"))
        .stdout(pstr::contains("smoke task title"));

    let body = std::fs::read_to_string(&out).expect("artifact written");
    assert!(!body.is_empty(), "artifact must be non-empty");
    let v: serde_json::Value = serde_json::from_str(&body).expect("artifact parses as JSON");
    assert_eq!(v["mode"], "dry-run");
    assert_eq!(v["task"], "smoke task title");
}

#[test]
fn run_dry_run_json_output_is_parseable() {
    at()
        .args([
            "run",
            "--dry-run",
            "--json",
            "-t",
            "json mode",
            "-p",
            ".",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|s: &str| {
            // Strip leading daemon-lockfile warning lines, find first '{'.
            if let Some(idx) = s.find('{') {
                serde_json::from_str::<serde_json::Value>(&s[idx..]).is_ok()
            } else {
                false
            }
        }));
}

#[test]
fn agent_run_dry_run_writes_role_prefixed_artifact() {
    let td = unique_tmp_dir("agent-dry");
    let out = td.path().join("out.json");
    at()
        .args([
            "agent",
            "run",
            "--dry-run",
            "-r",
            "qa-reviewer",
            "-t",
            "audit changes",
            "-p",
            ".",
            "-o",
        ])
        .arg(&out)
        .assert()
        .success()
        .stdout(pstr::contains("dry-run"));

    let body = std::fs::read_to_string(&out).expect("artifact written");
    let v: serde_json::Value = serde_json::from_str(&body).expect("artifact parses as JSON");
    assert_eq!(v["mode"], "dry-run");
    assert_eq!(v["role"], "qa-reviewer");
    // category_for_role("qa-reviewer") -> "testing"
    assert_eq!(v["category"], "testing");
    assert!(
        v["task_title"]
            .as_str()
            .map(|s| s.starts_with("[qa-reviewer]"))
            .unwrap_or(false),
        "title should be role-prefixed: {:?}",
        v["task_title"]
    );
}

#[test]
fn skill_list_empty_project_succeeds_with_no_skills_message() {
    // `skill list` does not need a daemon; on an empty project root it
    // should succeed and print the "No skills found" message.
    let td = unique_tmp_dir("skill-list-empty");
    let root = td.path();

    at_raw()
        .args(["skill", "list", "-p"])
        .arg(root)
        .assert()
        .success()
        .stdout(pstr::contains("No skills found"));
}

#[test]
fn skill_validate_strict_fails_when_skills_dir_missing() {
    let td = unique_tmp_dir("skill-validate-empty");
    let root = td.path();

    at_raw()
        .args(["skill", "validate", "--strict", "-p"])
        .arg(root)
        .assert()
        .failure()
        .stderr(pstr::contains("skill validation failed").or(pstr::contains("Missing")));
}

// ---------------------------------------------------------------------------
// Lifecycle (cli-task-lifecycle skill)
// ---------------------------------------------------------------------------
//
// The `at` CLI exposes a manual lifecycle (`sling` -> `hook` -> `done`) and
// a high-level `run`/`exec` lifecycle. ONLY `run` honors `--dry-run` today;
// `sling`/`hook`/`done`/`exec` always POST to the daemon and have no offline
// simulation. Per the cli-task-lifecycle skill methodology, we cover the
// dry-run lifecycle here and leave the full sling->hook->done walk to a
// follow-up that mocks or stands up the daemon.
//
// TODO(follow-up): exercise `sling -> hook -> done` once the test harness
// gains a mockable daemon entry point. Do NOT spin up the real daemon here.

#[test]
fn run_dry_run_lifecycle_artifact_is_self_describing() {
    // The "lifecycle" we can express purely in dry-run mode is:
    //   1. Compile prompt + skills locally (`run --dry-run --emit-prompt`)
    //   2. Persist the dry-run payload to an artifact (`--out`)
    //   3. Re-read the artifact and confirm it is internally consistent.
    let td = unique_tmp_dir("lifecycle");
    let out = td.path().join("out.json");
    at()
        .args([
            "run",
            "--dry-run",
            "--emit-prompt",
            "-t",
            "lifecycle smoke",
            "-l",
            "experimental",
            "-c",
            "feature",
            "-P",
            "high",
            "-x",
            "low",
            "-p",
            ".",
            "-o",
        ])
        .arg(&out)
        .assert()
        .success()
        .stdout(pstr::contains("dry-run"))
        .stdout(pstr::contains("--- prompt ---"));

    let body = std::fs::read_to_string(&out).expect("artifact written");
    let v: serde_json::Value = serde_json::from_str(&body).expect("artifact parses as JSON");
    assert_eq!(v["mode"], "dry-run");
    assert_eq!(v["lane"], "experimental");
    assert_eq!(v["priority"], "high");
    assert_eq!(v["complexity"], "low");
    assert!(v["description"].as_str().unwrap_or("").contains("lifecycle smoke"));
}
