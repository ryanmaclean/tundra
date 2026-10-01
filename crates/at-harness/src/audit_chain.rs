//! Hash-chained, tamper-evident audit log stored as JSONL.
//!
//! Each line is one [`AuditEntry`]. Its `entry_hash` is
//! `sha256(prev_hash || canonical_json(entry without entry_hash))`, and its
//! `prev_hash` is the previous line's `entry_hash` (the first line uses
//! [`GENESIS_PREV_HASH`]). Editing, reordering or deleting any line breaks the
//! chain at that point, which [`verify`] reports with the 1-based line number.
//!
//! `canonical_json` sorts object keys and has no whitespace, so hashes do not
//! depend on serde_json's `preserve_order` feature or on field order.
//!
//! # Provenance
//!
//! The chain construction, restart recovery and verifier are ported from
//! irclaw-v2 `src/security/audit.rs` (`compute_entry_hash`,
//! `recover_chain_state`, `verify_chain`). irclaw-v2 is licensed
//! `MIT OR Apache-2.0`, Copyright (c) 2025 ZeroClaw Labs, and is used here
//! under the MIT terms (the full notice is reproduced in
//! [`crate::output_guard`]). Dropped from upstream: HMAC signing, log rotation,
//! the command-execution event schema, and the config/buffer plumbing.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// `prev_hash` of the first entry in every chain.
pub const GENESIS_PREV_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Schema tag written into every entry.
pub const SCHEMA: &str = "at.audit_chain/v1";

/// One line of the audit log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub schema: String,
    /// 0-based, contiguous.
    pub sequence: u64,
    /// RFC 3339, UTC, millisecond precision.
    pub timestamp: String,
    pub event_id: String,
    /// Event kind, e.g. `approval.approved`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub payload: Value,
    pub prev_hash: String,
    pub entry_hash: String,
}

/// Errors from appending to or verifying an audit chain.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("audit log I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("audit log serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("line {line}: unreadable entry: {message}")]
    Parse { line: usize, message: String },
    #[error("line {line}: sequence gap: expected {expected}, found {found}")]
    SequenceGap {
        line: usize,
        expected: u64,
        found: u64,
    },
    #[error("line {line}: prev_hash does not link to the previous entry (expected {expected}, found {found})")]
    BrokenLink {
        line: usize,
        expected: String,
        found: String,
    },
    #[error(
        "line {line}: entry_hash mismatch, entry was modified (expected {expected}, found {found})"
    )]
    HashMismatch {
        line: usize,
        expected: String,
        found: String,
    },
}

impl AuditError {
    /// 1-based line where verification failed, if this is a chain violation.
    pub fn line(&self) -> Option<usize> {
        match self {
            Self::Parse { line, .. }
            | Self::SequenceGap { line, .. }
            | Self::BrokenLink { line, .. }
            | Self::HashMismatch { line, .. } => Some(*line),
            Self::Io(_) | Self::Serialize(_) => None,
        }
    }
}

/// Result of a successful [`verify`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    /// Number of entries checked.
    pub entries: u64,
    /// `entry_hash` of the last entry ([`GENESIS_PREV_HASH`] when empty).
    pub head_hash: String,
}

struct ChainState {
    prev_hash: String,
    sequence: u64,
}

/// Append-only handle on a JSONL audit chain. Appends are serialized by an
/// internal mutex and fsynced.
pub struct AuditChain {
    path: PathBuf,
    state: Mutex<ChainState>,
}

impl std::fmt::Debug for AuditChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditChain")
            .field("path", &self.path)
            .finish()
    }
}

impl AuditChain {
    /// Open (or create) the chain at `path`, creating parent directories and
    /// continuing from the last readable entry.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AuditError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let state = recover_state(&path)?;
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append an event and return the entry as written.
    pub fn append(
        &self,
        kind: &str,
        actor: Option<&str>,
        payload: Value,
    ) -> Result<AuditEntry, AuditError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut entry = AuditEntry {
            schema: SCHEMA.to_string(),
            sequence: state.sequence,
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            event_id: uuid::Uuid::new_v4().to_string(),
            kind: kind.to_string(),
            actor: actor.map(str::to_string),
            payload,
            prev_hash: state.prev_hash.clone(),
            entry_hash: String::new(),
        };
        // Hash what verify() will see: the value as re-parsed from JSON text.
        let reparsed: Value = serde_json::from_str(&serde_json::to_string(&entry)?)?;
        entry.entry_hash = entry_hash(&entry.prev_hash, &reparsed);

        let line = serde_json::to_string(&entry)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")?;
        file.sync_data()?;

        state.prev_hash = entry.entry_hash.clone();
        state.sequence += 1;
        Ok(entry)
    }

    /// Verify the whole file. See [`verify`].
    pub fn verify(&self) -> Result<VerifyReport, AuditError> {
        verify(&self.path)
    }
}

/// Walk the chain at `path` and check sequence continuity, `prev_hash`
/// linkage and every `entry_hash`. A missing or empty file is a valid empty
/// chain. Blank lines are ignored.
pub fn verify(path: &Path) -> Result<VerifyReport, AuditError> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(VerifyReport {
                entries: 0,
                head_hash: GENESIS_PREV_HASH.to_string(),
            })
        }
        Err(e) => return Err(e.into()),
    };

    let mut expected_prev = GENESIS_PREV_HASH.to_string();
    let mut expected_seq: u64 = 0;
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line_no = idx + 1;
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parse_err = |message: String| AuditError::Parse {
            line: line_no,
            message,
        };
        let value: Value = serde_json::from_str(&line).map_err(|e| parse_err(e.to_string()))?;
        let field = |name: &str| {
            value
                .get(name)
                .ok_or_else(|| parse_err(format!("missing {name}")))
        };
        let sequence = field("sequence")?
            .as_u64()
            .ok_or_else(|| parse_err("sequence is not an integer".into()))?;
        let prev_hash = field("prev_hash")?
            .as_str()
            .ok_or_else(|| parse_err("prev_hash is not a string".into()))?;
        let stored_hash = field("entry_hash")?
            .as_str()
            .ok_or_else(|| parse_err("entry_hash is not a string".into()))?;

        if sequence != expected_seq {
            return Err(AuditError::SequenceGap {
                line: line_no,
                expected: expected_seq,
                found: sequence,
            });
        }
        if prev_hash != expected_prev {
            return Err(AuditError::BrokenLink {
                line: line_no,
                expected: expected_prev,
                found: prev_hash.to_string(),
            });
        }
        let recomputed = entry_hash(prev_hash, &value);
        if recomputed != stored_hash {
            return Err(AuditError::HashMismatch {
                line: line_no,
                expected: recomputed,
                found: stored_hash.to_string(),
            });
        }
        expected_prev = recomputed;
        expected_seq += 1;
    }
    Ok(VerifyReport {
        entries: expected_seq,
        head_hash: expected_prev,
    })
}

/// `sha256(prev_hash || canonical_json(entry minus "entry_hash"))`, lowercase hex.
pub fn entry_hash(prev_hash: &str, entry: &Value) -> String {
    let content = match entry {
        Value::Object(map) => {
            let mut map = map.clone();
            map.remove("entry_hash");
            Value::Object(map)
        }
        other => other.clone(),
    };
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(canonical_json(&content).as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Compact JSON with object keys sorted recursively.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(v, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(k.clone()).to_string());
                out.push(':');
                write_canonical(&map[k], out);
            }
            out.push('}');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

/// Continue from the last parseable entry (upstream behaviour); a corrupt
/// tail is left for [`verify`] to report.
fn recover_state(path: &Path) -> Result<ChainState, AuditError> {
    let genesis = ChainState {
        prev_hash: GENESIS_PREV_HASH.to_string(),
        sequence: 0,
    };
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(genesis),
        Err(e) => return Err(e.into()),
    };
    let mut state = genesis;
    for line in BufReader::new(file).lines() {
        let line = line?;
        if let Ok(entry) = serde_json::from_str::<AuditEntry>(&line) {
            state = ChainState {
                prev_hash: entry.entry_hash,
                sequence: entry.sequence + 1,
            };
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TempLog(PathBuf);
    impl TempLog {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("at-audit-{}", uuid::Uuid::new_v4()));
            Self(dir.join("nested").join("audit.jsonl"))
        }
    }
    impl Drop for TempLog {
        fn drop(&mut self) {
            if let Some(root) = self.0.parent().and_then(Path::parent) {
                let _ = std::fs::remove_dir_all(root);
            }
        }
    }

    fn write_n(chain: &AuditChain, n: usize) {
        for i in 0..n {
            chain
                .append(
                    "approval.approved",
                    Some("tester"),
                    json!({"i": i, "tool": "git_push"}),
                )
                .unwrap();
        }
    }

    fn lines(path: &Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn rewrite(path: &Path, lines: &[String]) {
        std::fs::write(path, lines.join("\n") + "\n").unwrap();
    }

    #[test]
    fn append_verify_roundtrip_and_restart() {
        let tmp = TempLog::new();
        let chain = AuditChain::open(&tmp.0).unwrap();
        write_n(&chain, 5);
        let report = chain.verify().unwrap();
        assert_eq!(report.entries, 5);

        let first: AuditEntry = serde_json::from_str(&lines(&tmp.0)[0]).unwrap();
        assert_eq!(first.prev_hash, GENESIS_PREV_HASH);
        assert_eq!(first.sequence, 0);
        assert_eq!(first.schema, SCHEMA);

        // A new handle continues the same chain.
        drop(chain);
        let chain = AuditChain::open(&tmp.0).unwrap();
        let e = chain
            .append("approval.denied", None, json!({"x": 1.5}))
            .unwrap();
        assert_eq!(e.sequence, 5);
        assert_eq!(e.prev_hash, report.head_hash);
        let report = verify(&tmp.0).unwrap();
        assert_eq!(report.entries, 6);
        assert_eq!(report.head_hash, e.entry_hash);
    }

    #[test]
    fn tampered_middle_line_is_detected() {
        let tmp = TempLog::new();
        let chain = AuditChain::open(&tmp.0).unwrap();
        write_n(&chain, 5);
        let mut ls = lines(&tmp.0);
        ls[2] = ls[2].replace("git_push", "file_read");
        rewrite(&tmp.0, &ls);
        let err = verify(&tmp.0).unwrap_err();
        assert!(
            matches!(err, AuditError::HashMismatch { line: 3, .. }),
            "{err}"
        );
        assert_eq!(err.line(), Some(3));
    }

    #[test]
    fn rehashed_tamper_breaks_the_next_link() {
        let tmp = TempLog::new();
        let chain = AuditChain::open(&tmp.0).unwrap();
        write_n(&chain, 5);
        let mut ls = lines(&tmp.0);
        let mut v: Value = serde_json::from_str(&ls[2]).unwrap();
        v["payload"]["tool"] = json!("file_read");
        let h = entry_hash(v["prev_hash"].as_str().unwrap(), &v);
        v["entry_hash"] = json!(h);
        ls[2] = v.to_string();
        rewrite(&tmp.0, &ls);
        let err = verify(&tmp.0).unwrap_err();
        assert!(
            matches!(err, AuditError::BrokenLink { line: 4, .. }),
            "{err}"
        );
    }

    #[test]
    fn deleted_line_is_a_sequence_gap() {
        let tmp = TempLog::new();
        let chain = AuditChain::open(&tmp.0).unwrap();
        write_n(&chain, 4);
        let mut ls = lines(&tmp.0);
        ls.remove(1);
        rewrite(&tmp.0, &ls);
        let err = verify(&tmp.0).unwrap_err();
        assert!(
            matches!(
                err,
                AuditError::SequenceGap {
                    line: 2,
                    expected: 1,
                    found: 2
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn garbage_line_is_a_parse_error() {
        let tmp = TempLog::new();
        let chain = AuditChain::open(&tmp.0).unwrap();
        write_n(&chain, 2);
        let mut ls = lines(&tmp.0);
        ls.push("{not json".into());
        rewrite(&tmp.0, &ls);
        assert!(matches!(
            verify(&tmp.0),
            Err(AuditError::Parse { line: 3, .. })
        ));
    }

    #[test]
    fn empty_and_missing_files_are_empty_chains() {
        let tmp = TempLog::new();
        let missing = verify(&tmp.0).unwrap();
        assert_eq!(missing.entries, 0);
        assert_eq!(missing.head_hash, GENESIS_PREV_HASH);

        let chain = AuditChain::open(&tmp.0).unwrap();
        std::fs::write(&tmp.0, "").unwrap();
        let empty = chain.verify().unwrap();
        assert_eq!(empty, missing);
    }

    #[test]
    fn canonical_json_sorts_keys_recursively() {
        let v = json!({"b": 1, "a": {"d": [true, null], "c": "x"}});
        assert_eq!(
            canonical_json(&v),
            r#"{"a":{"c":"x","d":[true,null]},"b":1}"#
        );
        let reordered = json!({"a": {"c": "x", "d": [true, null]}, "b": 1});
        assert_eq!(entry_hash("p", &v), entry_hash("p", &reordered));
        assert_ne!(entry_hash("p", &v), entry_hash("q", &v));
    }
}
