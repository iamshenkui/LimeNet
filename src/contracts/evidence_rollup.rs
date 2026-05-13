use serde::{Deserialize, Serialize};

/// Summary-sized evidence rollup with indirect artifact references.
///
/// Aggregates evidence artifacts by reference only — no inline artifact
/// payloads are stored. The summary field provides a coarse-grained
/// description of the evidence set, keeping the rollup lightweight
/// and suitable for cross-domain delivery surfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRollup {
    /// Unique identifier for this evidence rollup
    #[serde(default)]
    pub evidence_rollup_id: Option<String>,

    /// Coarse-grained summary of the evidence being rolled up
    #[serde(default)]
    pub summary: Option<String>,

    /// Indirect references to evidence artifacts (URLs or resource IDs)
    #[serde(default)]
    pub artifact_refs: Option<Vec<String>>,

    /// Domain or system that originated this evidence rollup
    #[serde(default)]
    pub source_domain: Option<String>,

    /// Number of evidence artifacts included in this rollup
    #[serde(default)]
    pub evidence_count: Option<u32>,

    /// Reference to the delivery package this rollup belongs to
    #[serde(default)]
    pub delivery_id: Option<String>,
}

impl EvidenceRollup {
    /// Validates evidence rollup field consistency.
    ///
    /// Validation enforces the summary-sized contract: no inline artifact
    /// payloads are expected, and only field-level sanity is checked.
    /// Indirect artifact reference semantics are preserved — artifact_refs
    /// remain opaque identifiers and are not dereferenced.
    pub fn validate(&self) -> Result<(), String> {
        // An evidence rollup with evidence_count=0 is semantically
        // meaningless — the count describes packaged artifacts in transit
        if let Some(0) = self.evidence_count {
            return Err("evidence_count must be at least 1 when set".to_string());
        }

        // An empty summary violates the summary-sized contract:
        // the summary field must carry meaningful content when present
        if let Some(ref s) = self.summary {
            if s.trim().is_empty() {
                return Err("summary must not be empty when set".to_string());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Deserialization tests
    // ------------------------------------------------------------------

    #[test]
    fn test_deserialize_minimal() {
        let json = r#"{}"#;
        let rollup: EvidenceRollup = serde_json::from_str(json).unwrap();
        assert!(rollup.evidence_rollup_id.is_none());
        assert!(rollup.summary.is_none());
        assert!(rollup.artifact_refs.is_none());
        assert!(rollup.source_domain.is_none());
        assert!(rollup.evidence_count.is_none());
        assert!(rollup.delivery_id.is_none());
    }

    #[test]
    fn test_deserialize_partial() {
        let json = r#"{
            "evidence_rollup_id":"er-001",
            "source_domain":"task-graph"
        }"#;
        let rollup: EvidenceRollup = serde_json::from_str(json).unwrap();
        assert_eq!(rollup.evidence_rollup_id, Some("er-001".to_string()));
        assert_eq!(rollup.source_domain, Some("task-graph".to_string()));
        assert!(rollup.summary.is_none());
        assert!(rollup.artifact_refs.is_none());
        assert!(rollup.evidence_count.is_none());
        assert!(rollup.delivery_id.is_none());
    }

    #[test]
    fn test_deserialize_with_artifact_refs() {
        let json = r#"{
            "summary":"Review evidence for sprint-42",
            "artifact_refs":["https://artifacts.example.com/ev-001","ev-002"],
            "evidence_count":2
        }"#;
        let rollup: EvidenceRollup = serde_json::from_str(json).unwrap();
        assert_eq!(
            rollup.summary,
            Some("Review evidence for sprint-42".to_string())
        );
        let refs = rollup.artifact_refs.unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], "https://artifacts.example.com/ev-001");
        assert_eq!(refs[1], "ev-002");
        assert_eq!(rollup.evidence_count, Some(2));
    }

    #[test]
    fn test_serde_roundtrip() {
        let rollup = EvidenceRollup {
            evidence_rollup_id: Some("er-001".to_string()),
            summary: Some("Review evidence for sprint-42".to_string()),
            artifact_refs: Some(vec![
                "https://artifacts.example.com/ev-001".to_string(),
                "ev-002".to_string(),
            ]),
            source_domain: Some("task-graph".to_string()),
            evidence_count: Some(2),
            delivery_id: Some("del-001".to_string()),
        };
        let json = serde_json::to_string(&rollup).unwrap();
        let deserialized: EvidenceRollup = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.evidence_rollup_id, Some("er-001".to_string()));
        assert_eq!(
            deserialized.summary,
            Some("Review evidence for sprint-42".to_string())
        );
        let refs = deserialized.artifact_refs.unwrap();
        assert_eq!(refs[0], "https://artifacts.example.com/ev-001");
        assert_eq!(refs[1], "ev-002");
        assert_eq!(deserialized.source_domain, Some("task-graph".to_string()));
        assert_eq!(deserialized.evidence_count, Some(2));
        assert_eq!(deserialized.delivery_id, Some("del-001".to_string()));
    }

    // ------------------------------------------------------------------
    // Validation tests — evidence_count semantics
    // ------------------------------------------------------------------

    #[test]
    fn test_minimal_rollup_is_valid() {
        let rollup = EvidenceRollup {
            evidence_rollup_id: None,
            summary: None,
            artifact_refs: None,
            source_domain: None,
            evidence_count: None,
            delivery_id: None,
        };
        assert!(rollup.validate().is_ok());
    }

    #[test]
    fn test_fully_populated_rollup_is_valid() {
        let rollup = EvidenceRollup {
            evidence_rollup_id: Some("er-001".to_string()),
            summary: Some("Review evidence for sprint-42".to_string()),
            artifact_refs: Some(vec!["https://artifacts.example.com/ev-001".to_string()]),
            source_domain: Some("task-graph".to_string()),
            evidence_count: Some(1),
            delivery_id: Some("del-001".to_string()),
        };
        assert!(rollup.validate().is_ok());
    }

    #[test]
    fn test_evidence_count_zero_is_invalid() {
        let rollup = EvidenceRollup {
            evidence_rollup_id: None,
            summary: None,
            artifact_refs: None,
            source_domain: None,
            evidence_count: Some(0),
            delivery_id: None,
        };
        let err = rollup.validate().unwrap_err();
        assert!(err.contains("evidence_count"), "error: {err}");
    }

    #[test]
    fn test_evidence_count_one_is_valid() {
        let rollup = EvidenceRollup {
            evidence_rollup_id: None,
            summary: None,
            artifact_refs: None,
            source_domain: None,
            evidence_count: Some(1),
            delivery_id: None,
        };
        assert!(rollup.validate().is_ok());
    }

    // ------------------------------------------------------------------
    // Validation tests — summary-sized contract
    // ------------------------------------------------------------------

    #[test]
    fn test_empty_summary_is_invalid() {
        let rollup = EvidenceRollup {
            evidence_rollup_id: None,
            summary: Some("".to_string()),
            artifact_refs: None,
            source_domain: None,
            evidence_count: None,
            delivery_id: None,
        };
        let err = rollup.validate().unwrap_err();
        assert!(err.contains("summary"), "error: {err}");
    }

    #[test]
    fn test_whitespace_only_summary_is_invalid() {
        let rollup = EvidenceRollup {
            evidence_rollup_id: None,
            summary: Some("   ".to_string()),
            artifact_refs: None,
            source_domain: None,
            evidence_count: None,
            delivery_id: None,
        };
        let err = rollup.validate().unwrap_err();
        assert!(err.contains("summary"), "error: {err}");
    }

    #[test]
    fn test_non_empty_summary_is_valid() {
        let rollup = EvidenceRollup {
            evidence_rollup_id: None,
            summary: Some("Review evidence for sprint-42".to_string()),
            artifact_refs: None,
            source_domain: None,
            evidence_count: None,
            delivery_id: None,
        };
        assert!(rollup.validate().is_ok());
    }

    // ------------------------------------------------------------------
    // Validation tests — indirect artifact reference semantics
    // ------------------------------------------------------------------

    #[test]
    fn test_artifact_refs_are_opaque_identifiers() {
        // artifact_refs hold indirect references (URLs or resource IDs)
        // and are not dereferenced or validated for format — they
        // remain opaque at the rollup boundary
        let rollup = EvidenceRollup {
            evidence_rollup_id: None,
            summary: Some("Evidence summary".to_string()),
            artifact_refs: Some(vec![
                "urn:evidence:abc-123".to_string(),
                "https://storage.example.com/ev-001".to_string(),
                "simple-id".to_string(),
            ]),
            source_domain: None,
            evidence_count: Some(3),
            delivery_id: None,
        };
        assert!(rollup.validate().is_ok());
    }

    #[test]
    fn test_no_artifact_refs_is_valid() {
        // Indirect artifact references are optional — a rollup with
        // only a summary is semantically valid
        let rollup = EvidenceRollup {
            evidence_rollup_id: Some("er-001".to_string()),
            summary: Some("Summary only".to_string()),
            artifact_refs: None,
            source_domain: Some("task-graph".to_string()),
            evidence_count: None,
            delivery_id: None,
        };
        assert!(rollup.validate().is_ok());
    }

    #[test]
    fn test_local_subtask_details_not_required() {
        // The evidence rollup validates with only optional fields,
        // without requiring any local subtask details from either the
        // source or target domain.
        let variants = [None, Some("Summary only".to_string())];
        for summary in &variants {
            let rollup = EvidenceRollup {
                evidence_rollup_id: None,
                summary: summary.clone(),
                artifact_refs: None,
                source_domain: None,
                evidence_count: None,
                delivery_id: None,
            };
            assert!(
                rollup.validate().is_ok(),
                "expected summary={summary:?} to be valid"
            );
        }
    }
}
