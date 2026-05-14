use crate::contracts::delivery::{EvidenceRef, TraceContext};
use serde::{Deserialize, Serialize};

/// Summary-sized evidence rollup with indirect artifact references.
///
/// Aggregates evidence artifacts by reference only — no inline artifact
/// payloads are stored. The summary field provides a coarse-grained
/// description of the evidence set, keeping the rollup lightweight
/// and suitable for cross-domain delivery surfaces.
///
/// Matches the shared Phase 2B wire shape emitted by the meta-agent
/// Python `EvidenceRollup` dataclass. Python uses `rollup_id` while
/// Rust uses `evidence_rollup_id`; the `#[serde(alias)]` attribute
/// ensures both keys deserialize correctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRollup {
    /// Unique identifier for this evidence rollup.
    /// Accepts both `evidence_rollup_id` (Rust-native) and `rollup_id` (Python wire).
    #[serde(default, alias = "rollup_id")]
    pub evidence_rollup_id: Option<String>,

    /// Task identifier linking this rollup to its originating task.
    #[serde(default)]
    pub task_id: Option<String>,

    /// Coarse-grained summary of the evidence being rolled up
    #[serde(default)]
    pub summary: Option<String>,

    /// Inline evidence references supporting the rollup summary
    #[serde(default)]
    pub evidence_refs: Option<Vec<EvidenceRef>>,

    /// Indirect references to evidence artifacts (URLs or resource IDs)
    #[serde(default)]
    pub artifact_refs: Option<Vec<String>>,

    /// Conclusion drawn from the evidence set
    #[serde(default)]
    pub conclusion: Option<String>,

    /// In-process trace context for causation tracking
    #[serde(default)]
    pub trace_context: Option<TraceContext>,

    /// Domain or system that originated this evidence rollup
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_domain: Option<String>,

    /// Number of evidence artifacts included in this rollup
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_count: Option<u32>,

    /// Reference to the delivery package this rollup belongs to
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
        assert!(rollup.task_id.is_none());
        assert!(rollup.summary.is_none());
        assert!(rollup.evidence_refs.is_none());
        assert!(rollup.artifact_refs.is_none());
        assert!(rollup.conclusion.is_none());
        assert!(rollup.trace_context.is_none());
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
    fn test_deserialize_rollup_id_alias() {
        // Python emits "rollup_id" — Rust must accept it via the alias
        let json = r#"{
            "rollup_id":"er-from-python",
            "summary":"Python wire rollup"
        }"#;
        let rollup: EvidenceRollup = serde_json::from_str(json).unwrap();
        assert_eq!(
            rollup.evidence_rollup_id,
            Some("er-from-python".to_string()),
            "rollup_id alias must map to evidence_rollup_id"
        );
        assert_eq!(rollup.summary, Some("Python wire rollup".to_string()));
    }

    #[test]
    fn test_deserialize_python_wire_fields() {
        let json = r#"{
            "rollup_id":"er-py-001",
            "task_id":"T-001",
            "summary":"Python wire rollup",
            "evidence_refs":[{"artifact":"run","path":"run.summary","value":"passed"}],
            "artifact_refs":["artifacts/run.log"],
            "conclusion":"proceed",
            "trace_context":{"correlation_id":"corr-001"}
        }"#;
        let rollup: EvidenceRollup = serde_json::from_str(json).unwrap();
        assert_eq!(rollup.evidence_rollup_id, Some("er-py-001".to_string()));
        assert_eq!(rollup.task_id, Some("T-001".to_string()));
        assert_eq!(rollup.summary, Some("Python wire rollup".to_string()));
        assert!(rollup.evidence_refs.is_some_and(|r| r.len() == 1));
        assert!(rollup.artifact_refs.is_some_and(|r| r.len() == 1));
        assert_eq!(rollup.conclusion, Some("proceed".to_string()));
        assert!(rollup.trace_context.is_some());
        // Rust-only fields absent from Python wire → None
        assert!(rollup.source_domain.is_none());
        assert!(rollup.evidence_count.is_none());
        assert!(rollup.delivery_id.is_none());
    }

    #[test]
    fn test_serde_roundtrip() {
        let rollup = EvidenceRollup {
            evidence_rollup_id: Some("er-001".to_string()),
            task_id: Some("T-001".to_string()),
            summary: Some("Review evidence for sprint-42".to_string()),
            evidence_refs: Some(vec![EvidenceRef {
                artifact: "run".into(),
                path: "run.summary".into(),
                value: "passed".into(),
            }]),
            artifact_refs: Some(vec![
                "https://artifacts.example.com/ev-001".to_string(),
                "ev-002".to_string(),
            ]),
            conclusion: Some("proceed".to_string()),
            trace_context: Some(TraceContext {
                correlation_id: Some("corr-001".into()),
                task_id: None,
                attempt_id: None,
                last_event_id: None,
            }),
            source_domain: Some("task-graph".to_string()),
            evidence_count: Some(2),
            delivery_id: Some("del-001".to_string()),
        };
        let json = serde_json::to_string(&rollup).unwrap();
        let deserialized: EvidenceRollup = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.evidence_rollup_id, Some("er-001".to_string()));
        assert_eq!(deserialized.task_id, Some("T-001".to_string()));
        assert_eq!(
            deserialized.summary,
            Some("Review evidence for sprint-42".to_string())
        );
        let refs = deserialized.artifact_refs.unwrap();
        assert_eq!(refs[0], "https://artifacts.example.com/ev-001");
        assert_eq!(refs[1], "ev-002");
        assert_eq!(deserialized.conclusion, Some("proceed".to_string()));
        assert!(deserialized.trace_context.is_some());
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
            task_id: None,
            summary: None,
            evidence_refs: None,
            artifact_refs: None,
            conclusion: None,
            trace_context: None,
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
            task_id: Some("T-001".to_string()),
            summary: Some("Review evidence for sprint-42".to_string()),
            evidence_refs: Some(vec![EvidenceRef {
                artifact: "run".into(),
                path: "run.summary".into(),
                value: "passed".into(),
            }]),
            artifact_refs: Some(vec!["https://artifacts.example.com/ev-001".to_string()]),
            conclusion: Some("proceed".to_string()),
            trace_context: Some(TraceContext {
                correlation_id: Some("corr-001".into()),
                task_id: None,
                attempt_id: None,
                last_event_id: None,
            }),
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
            task_id: None,
            summary: None,
            evidence_refs: None,
            artifact_refs: None,
            conclusion: None,
            trace_context: None,
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
            task_id: None,
            summary: None,
            evidence_refs: None,
            artifact_refs: None,
            conclusion: None,
            trace_context: None,
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
            task_id: None,
            summary: Some("".to_string()),
            evidence_refs: None,
            artifact_refs: None,
            conclusion: None,
            trace_context: None,
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
            task_id: None,
            summary: Some("   ".to_string()),
            evidence_refs: None,
            artifact_refs: None,
            conclusion: None,
            trace_context: None,
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
            task_id: None,
            summary: Some("Review evidence for sprint-42".to_string()),
            evidence_refs: None,
            artifact_refs: None,
            conclusion: None,
            trace_context: None,
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
            task_id: None,
            summary: Some("Evidence summary".to_string()),
            evidence_refs: None,
            artifact_refs: Some(vec![
                "urn:evidence:abc-123".to_string(),
                "https://storage.example.com/ev-001".to_string(),
                "simple-id".to_string(),
            ]),
            conclusion: None,
            trace_context: None,
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
            task_id: None,
            summary: Some("Summary only".to_string()),
            evidence_refs: None,
            artifact_refs: None,
            conclusion: None,
            trace_context: None,
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
                task_id: None,
                summary: summary.clone(),
                evidence_refs: None,
                artifact_refs: None,
                conclusion: None,
                trace_context: None,
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
