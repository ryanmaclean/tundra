//! Registry of JSON Schema documents served at `GET /api/v1/schemas/{id}`.
//!
//! Every schema id that appears on a catalog card (`ApiCatalogRoute::schemas`)
//! must resolve here; the at-bridge route-surface test enforces that closure.

/// URL prefix the documents are served under; append the schema id.
pub const SCHEMAS_PATH_PREFIX: &str = "/api/v1/schemas/";

/// `(schema id, JSON Schema document)` for every published schema.
pub const ALL: &[(&str, &str)] = &[(
    crate::merge_gate::MERGE_GATE_SCHEMA_ID,
    crate::merge_gate::MERGE_GATE_REPORT_SCHEMA_JSON,
)];

/// The JSON Schema document for `id`, if published.
pub fn lookup(id: &str) -> Option<&'static str> {
    ALL.iter().find(|(k, _)| *k == id).map(|(_, doc)| *doc)
}

/// Path serving the document for `id`.
pub fn path_for(id: &str) -> String {
    format!("{SCHEMAS_PATH_PREFIX}{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_document_parses_and_carries_its_id() {
        for (id, doc) in ALL {
            let v: serde_json::Value = serde_json::from_str(doc).unwrap();
            assert_eq!(v["$id"], *id);
        }
        assert!(lookup("at.merge_gate.report/v1").is_some());
        assert!(lookup("nope/v1").is_none());
        assert_eq!(
            path_for("at.merge_gate.report/v1"),
            "/api/v1/schemas/at.merge_gate.report/v1"
        );
    }
}
