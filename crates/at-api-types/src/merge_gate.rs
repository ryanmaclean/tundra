//! Wire contract for merge-gate results (`at.merge_gate.report/v1`).
//!
//! These are wasm-safe mirrors of `at_core::merge_gate::MergeGateReport` and
//! the merge endpoints' JSON bodies, for clients (the Leptos UI, agents) that
//! cannot depend on `at-core`. The machine-readable contract is the JSON
//! Schema in [`MERGE_GATE_REPORT_SCHEMA_JSON`], served unauthenticated at
//! `GET /api/v1/schemas/at.merge_gate.report/v1` (see [`crate::schemas`]).
//!
//! Stability: fields are only ever added (with defaults) under `v1`; a
//! breaking change gets a new schema id. Unknown `blocked_by[].kind` values
//! must be treated as blocking.
//!
//! Endpoints returning these types:
//! - `POST /api/worktrees/{id}/merge`, `POST /api/tasks/{id}/merge` ->
//!   [`ApiMergeResponse`] (200 `success` / `nothing_to_merge` / `conflict`,
//!   409 `gate_failed` or `stale_head`)
//! - `GET /api/tasks/{id}/merge-gate` -> [`ApiMergeGateState`]
//!
//! Event types published on the event bus by the gate flow (stable strings):
//! [`EVENT_MERGE_GATE_FAILED`], [`EVENT_MERGE_SUCCESS`],
//! [`EVENT_MERGE_CONFLICT`].

use serde::{Deserialize, Serialize};

/// Schema id carried in [`ApiMergeGateReport::schema`].
pub const MERGE_GATE_SCHEMA_ID: &str = "at.merge_gate.report/v1";

/// Event type published when the gate refuses a branch; `message` is the
/// report summary.
pub const EVENT_MERGE_GATE_FAILED: &str = "merge_gate_failed";
/// Event type published when a gated merge landed on the target branch.
pub const EVENT_MERGE_SUCCESS: &str = "merge_success";
/// Event type published when a gated merge hit conflicts.
pub const EVENT_MERGE_CONFLICT: &str = "merge_conflict";

/// Mirror of `at_core::merge_gate::MergeGateReport`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMergeGateReport {
    /// Always [`MERGE_GATE_SCHEMA_ID`].
    pub schema: String,
    /// `true` only when nothing blocks the merge and every command exited 0.
    pub passed: bool,
    pub branch: String,
    pub target: String,
    pub worktree: String,
    #[serde(default)]
    pub blocked_by: Vec<ApiGateBlock>,
    #[serde(default)]
    pub results: Vec<ApiCommandResult>,
    /// Criteria the gate was asked to run (additive).
    #[serde(default)]
    pub criteria: Vec<String>,
    /// Worktree commit the gate verified (additive).
    #[serde(default)]
    pub head: Option<String>,
    /// RFC 3339 time the gate ran (additive).
    #[serde(default)]
    pub generated_at: Option<String>,
}

impl ApiMergeGateReport {
    /// One-line summary, matching `MergeGateReport::summary` on the server.
    pub fn summary(&self) -> String {
        if self.passed {
            return format!("merge gate passed ({} commands)", self.results.len());
        }
        if let Some(block) = self.blocked_by.first() {
            return format!("merge gate refused: {}", block.describe());
        }
        match self.results.iter().find(|r| !r.success()) {
            Some(r) if r.timed_out => format!("merge gate refused: `{}` timed out", r.cmd),
            Some(r) => match r.exit_code {
                Some(code) => format!("merge gate refused: `{}` exited {code}", r.cmd),
                None => format!("merge gate refused: `{}` did not exit normally", r.cmd),
            },
            None => "merge gate refused".to_string(),
        }
    }
}

/// Mirror of `at_core::merge_gate::GateBlock`, flattened so that kinds this
/// client does not know still deserialize (and count as blocking).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ApiGateBlock {
    /// `uncommitted_changes`, `behind_target`, `vcs_error`, or a newer kind.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behind: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_behind: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ApiGateBlock {
    /// Human description, matching `GateBlock::describe` on the server.
    pub fn describe(&self) -> String {
        match self.kind.as_str() {
            "uncommitted_changes" => format!(
                "{} {} has {} uncommitted tracked change(s)",
                self.location.as_deref().unwrap_or("?"),
                self.path.as_deref().unwrap_or("?"),
                self.files.len()
            ),
            "behind_target" => format!(
                "branch is {} commits behind target (max {}); rebase first",
                self.behind.unwrap_or(0),
                self.max_behind.unwrap_or(0)
            ),
            "vcs_error" => format!("vcs error: {}", self.message.as_deref().unwrap_or("")),
            _ => "unknown precondition (treated as blocking)".to_string(),
        }
    }
}

/// Mirror of `at_core::merge_gate::CommandResult`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiCommandResult {
    pub cmd: String,
    /// `null` when the command timed out, was killed, or failed to spawn.
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub timed_out: bool,
    pub duration_ms: u64,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

impl ApiCommandResult {
    pub fn success(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }
}

/// Body of `POST /api/worktrees/{id}/merge` and `POST /api/tasks/{id}/merge`
/// for every status code (the 409 body included).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ApiMergeResponse {
    /// `success`, `nothing_to_merge`, `conflict` (200); `gate_failed`,
    /// `stale_head` (409). Absent on plain errors (404/400/500).
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub branch: String,
    /// Conflicting files when `status == "conflict"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// Gate summary on `gate_failed`, or the error message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The gate report, present once the gate ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<ApiMergeGateReport>,
}

/// Body of `GET /api/tasks/{id}/merge-gate`: the last report plus where the
/// task stands and what an agent can do next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMergeGateState {
    pub task_id: String,
    /// `unverified` (gate never ran), `passing`, `failing`, or `merged`.
    pub state: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub report: Option<ApiMergeGateReport>,
    /// Follow-up calls, e.g. `{"rel": "merge", "method": "POST", "href": ...}`.
    #[serde(default)]
    pub links: Vec<ApiLink>,
}

/// A hypermedia link to a follow-up call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiLink {
    pub rel: String,
    pub method: String,
    pub href: String,
}

/// JSON Schema (draft 2020-12) for `at.merge_gate.report/v1`.
pub const MERGE_GATE_REPORT_SCHEMA_JSON: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "at.merge_gate.report/v1",
  "title": "MergeGateReport",
  "description": "Result of running the merge gate (clean-tree preconditions, then acceptance-criteria shell commands) for one task branch. Fields are only added under v1; unknown blocked_by kinds must be treated as blocking.",
  "type": "object",
  "required": ["schema", "passed", "branch", "target", "worktree", "blocked_by", "results"],
  "properties": {
    "schema": { "const": "at.merge_gate.report/v1" },
    "passed": { "type": "boolean", "description": "true only when nothing blocks the merge and every command exited 0" },
    "branch": { "type": "string", "description": "task branch being merged" },
    "target": { "type": "string", "description": "branch it would be merged into" },
    "worktree": { "type": "string", "description": "worktree the criteria ran in" },
    "blocked_by": { "type": "array", "items": { "$ref": "#/$defs/GateBlock" } },
    "results": { "type": "array", "items": { "$ref": "#/$defs/CommandResult" }, "description": "one entry per command that ran; the gate stops at the first failure" },
    "criteria": { "type": "array", "items": { "type": "string" }, "description": "every criterion the gate was asked to run" },
    "head": { "type": ["string", "null"], "description": "worktree commit id the gate verified" },
    "generated_at": { "type": ["string", "null"], "format": "date-time" }
  },
  "$defs": {
    "GateBlock": {
      "type": "object",
      "required": ["kind"],
      "properties": {
        "kind": { "type": "string", "description": "uncommitted_changes | behind_target | vcs_error; any other value is blocking" },
        "location": { "type": "string", "enum": ["worktree", "base"] },
        "path": { "type": "string" },
        "files": { "type": "array", "items": { "type": "string" } },
        "behind": { "type": "integer", "minimum": 0 },
        "max_behind": { "type": "integer", "minimum": 0 },
        "message": { "type": "string" }
      }
    },
    "CommandResult": {
      "type": "object",
      "required": ["cmd", "exit_code", "timed_out", "duration_ms", "stdout_tail", "stderr_tail"],
      "properties": {
        "cmd": { "type": "string" },
        "exit_code": { "type": ["integer", "null"] },
        "timed_out": { "type": "boolean" },
        "duration_ms": { "type": "integer", "minimum": 0 },
        "stdout_tail": { "type": "string" },
        "stderr_tail": { "type": "string" }
      }
    }
  }
}"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn refused_body() -> serde_json::Value {
        serde_json::json!({
            "status": "gate_failed",
            "branch": "task/x",
            "error": "merge gate refused: `test -f ok` exited 1",
            "gate": {
                "schema": MERGE_GATE_SCHEMA_ID,
                "passed": false,
                "branch": "task/x",
                "target": "main",
                "worktree": "/r/.worktrees/x",
                "blocked_by": [],
                "results": [{
                    "cmd": "test -f ok", "exit_code": 1, "timed_out": false,
                    "duration_ms": 3, "stdout_tail": "", "stderr_tail": ""
                }],
                "criteria": ["test -f ok"],
                "head": "abc",
                "generated_at": "2026-09-23T00:00:00Z"
            }
        })
    }

    #[test]
    fn merge_response_parses_409_body() {
        let r: ApiMergeResponse = serde_json::from_value(refused_body()).unwrap();
        assert_eq!(r.status, "gate_failed");
        let gate = r.gate.unwrap();
        assert!(!gate.passed);
        assert_eq!(gate.summary(), r.error.unwrap());
        assert_eq!(gate.head.as_deref(), Some("abc"));
    }

    #[test]
    fn unknown_block_kind_is_kept_and_described_as_blocking() {
        let b: ApiGateBlock =
            serde_json::from_value(serde_json::json!({"kind": "new_thing", "extra": 1})).unwrap();
        assert_eq!(b.kind, "new_thing");
        assert!(b.describe().contains("blocking"));
    }

    #[test]
    fn schema_document_is_valid_json_with_matching_id() {
        let v: serde_json::Value = serde_json::from_str(MERGE_GATE_REPORT_SCHEMA_JSON).unwrap();
        assert_eq!(v["$id"], MERGE_GATE_SCHEMA_ID);
        assert_eq!(v["properties"]["schema"]["const"], MERGE_GATE_SCHEMA_ID);
        for key in v["required"].as_array().unwrap() {
            assert!(
                v["properties"].get(key.as_str().unwrap()).is_some(),
                "{key}"
            );
        }
    }
}
