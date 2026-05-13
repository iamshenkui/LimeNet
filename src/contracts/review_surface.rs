use serde::{Deserialize, Serialize};

/// Coarse-grained review surface for a delivery in the LimeNet system.
///
/// Projects the state of a delivery review at the boundary layer without
/// exposing local subtask internals from either the source or target
/// domain. References to deliveries, evidence, and delegation contracts
/// are indirect, keeping the surface lightweight and suitable for
/// cross-domain review workflows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSurface {
    /// Unique identifier for this review surface
    #[serde(default)]
    pub review_id: Option<String>,

    /// Reference to the delivery being reviewed
    #[serde(default)]
    pub delivery_id: Option<String>,

    /// Current review status of the delivery
    pub status: Option<super::DeliveryStatus>,

    /// Human-readable summary of the delivery under review
    #[serde(default)]
    pub review_summary: Option<String>,

    /// References to evidence rollups supporting this review
    #[serde(default)]
    pub evidence_rollup_ids: Option<Vec<String>>,

    /// Reference to the delegation contract for this delivery
    #[serde(default)]
    pub delegation_contract_id: Option<String>,

    /// Domain or system that originated the delivery
    #[serde(default)]
    pub source_domain: Option<String>,

    /// Domain or system that is the target of this review
    #[serde(default)]
    pub target_domain: Option<String>,

    /// Timestamp when the delivery was submitted for review
    #[serde(default)]
    pub submitted_at: Option<String>,

    /// Timestamp when the review decision was made
    #[serde(default)]
    pub reviewed_at: Option<String>,
}

impl ReviewSurface {
    /// Validates review surface field consistency.
    ///
    /// Validation is intentionally coarse-grained to preserve review-surface
    /// semantics: only field-level sanity is checked, and no local subtask
    /// details are required from either domain.
    pub fn validate(&self) -> Result<(), String> {
        // A review surface with a summary that is present but empty
        // violates the coarse-grained contract
        if let Some(ref s) = self.review_summary {
            if s.trim().is_empty() {
                return Err("review_summary must not be empty when set".to_string());
            }
        }

        // reviewed_at without submitted_at is a temporal inconsistency
        if self.reviewed_at.is_some() && self.submitted_at.is_none() {
            return Err(
                "reviewed_at requires submitted_at to establish review timeline".to_string(),
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::DeliveryStatus;

    // ------------------------------------------------------------------
    // Deserialization tests
    // ------------------------------------------------------------------

    #[test]
    fn test_deserialize_minimal() {
        let json = r#"{}"#;
        let surface: ReviewSurface = serde_json::from_str(json).unwrap();
        assert!(surface.review_id.is_none());
        assert!(surface.delivery_id.is_none());
        assert!(surface.status.is_none());
        assert!(surface.review_summary.is_none());
        assert!(surface.evidence_rollup_ids.is_none());
        assert!(surface.delegation_contract_id.is_none());
        assert!(surface.source_domain.is_none());
        assert!(surface.target_domain.is_none());
        assert!(surface.submitted_at.is_none());
        assert!(surface.reviewed_at.is_none());
    }

    #[test]
    fn test_deserialize_with_delivery_status() {
        let json = r#"{"status":"needs_revision"}"#;
        let surface: ReviewSurface = serde_json::from_str(json).unwrap();
        assert_eq!(surface.status, Some(DeliveryStatus::NeedsRevision));
    }

    #[test]
    fn test_deserialize_partial() {
        let json = r#"{
            "review_id":"rs-001",
            "delivery_id":"del-001",
            "source_domain":"task-graph",
            "target_domain":"human-review"
        }"#;
        let surface: ReviewSurface = serde_json::from_str(json).unwrap();
        assert_eq!(surface.review_id, Some("rs-001".to_string()));
        assert_eq!(surface.delivery_id, Some("del-001".to_string()));
        assert_eq!(surface.source_domain, Some("task-graph".to_string()));
        assert_eq!(surface.target_domain, Some("human-review".to_string()));
        assert!(surface.status.is_none());
        assert!(surface.evidence_rollup_ids.is_none());
        assert!(surface.delegation_contract_id.is_none());
    }

    #[test]
    fn test_deserialize_with_evidence_and_delegation() {
        let json = r#"{
            "review_id":"rs-001",
            "delivery_id":"del-001",
            "status":"accepted",
            "evidence_rollup_ids":["er-001","er-002"],
            "delegation_contract_id":"dc-001"
        }"#;
        let surface: ReviewSurface = serde_json::from_str(json).unwrap();
        assert_eq!(surface.review_id, Some("rs-001".to_string()));
        assert_eq!(surface.delivery_id, Some("del-001".to_string()));
        assert_eq!(surface.status, Some(DeliveryStatus::Accepted));
        let ids = surface.evidence_rollup_ids.unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], "er-001");
        assert_eq!(ids[1], "er-002");
        assert_eq!(surface.delegation_contract_id, Some("dc-001".to_string()));
    }

    #[test]
    fn test_serde_roundtrip() {
        let surface = ReviewSurface {
            review_id: Some("rs-001".to_string()),
            delivery_id: Some("del-001".to_string()),
            status: Some(DeliveryStatus::Proposed),
            review_summary: Some("Review for sprint-42 delivery".to_string()),
            evidence_rollup_ids: Some(vec!["er-001".to_string()]),
            delegation_contract_id: Some("dc-001".to_string()),
            source_domain: Some("task-graph".to_string()),
            target_domain: Some("human-review".to_string()),
            submitted_at: Some("2025-01-15T10:00:00Z".to_string()),
            reviewed_at: None,
        };
        let json = serde_json::to_string(&surface).unwrap();
        let deserialized: ReviewSurface = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.review_id, Some("rs-001".to_string()));
        assert_eq!(deserialized.delivery_id, Some("del-001".to_string()));
        assert_eq!(deserialized.status, Some(DeliveryStatus::Proposed));
        assert_eq!(
            deserialized.review_summary,
            Some("Review for sprint-42 delivery".to_string())
        );
        let ids = deserialized.evidence_rollup_ids.unwrap();
        assert_eq!(ids[0], "er-001");
        assert_eq!(
            deserialized.delegation_contract_id,
            Some("dc-001".to_string())
        );
        assert_eq!(deserialized.source_domain, Some("task-graph".to_string()));
        assert_eq!(
            deserialized.target_domain,
            Some("human-review".to_string())
        );
        assert_eq!(
            deserialized.submitted_at,
            Some("2025-01-15T10:00:00Z".to_string())
        );
        assert!(deserialized.reviewed_at.is_none());
    }

    // ------------------------------------------------------------------
    // Validation tests
    // ------------------------------------------------------------------

    #[test]
    fn test_minimal_surface_is_valid() {
        let surface = ReviewSurface {
            review_id: None,
            delivery_id: None,
            status: None,
            review_summary: None,
            evidence_rollup_ids: None,
            delegation_contract_id: None,
            source_domain: None,
            target_domain: None,
            submitted_at: None,
            reviewed_at: None,
        };
        assert!(surface.validate().is_ok());
    }

    #[test]
    fn test_fully_populated_surface_is_valid() {
        let surface = ReviewSurface {
            review_id: Some("rs-001".to_string()),
            delivery_id: Some("del-001".to_string()),
            status: Some(DeliveryStatus::Accepted),
            review_summary: Some("All artifacts verified".to_string()),
            evidence_rollup_ids: Some(vec!["er-001".to_string()]),
            delegation_contract_id: Some("dc-001".to_string()),
            source_domain: Some("task-graph".to_string()),
            target_domain: Some("human-review".to_string()),
            submitted_at: Some("2025-01-15T10:00:00Z".to_string()),
            reviewed_at: Some("2025-01-16T14:30:00Z".to_string()),
        };
        assert!(surface.validate().is_ok());
    }

    #[test]
    fn test_reviewed_at_without_submitted_at_is_invalid() {
        let surface = ReviewSurface {
            review_id: None,
            delivery_id: None,
            status: None,
            review_summary: None,
            evidence_rollup_ids: None,
            delegation_contract_id: None,
            source_domain: None,
            target_domain: None,
            submitted_at: None,
            reviewed_at: Some("2025-01-16T14:30:00Z".to_string()),
        };
        let err = surface.validate().unwrap_err();
        assert!(
            err.contains("reviewed_at requires submitted_at"),
            "error: {err}"
        );
    }

    #[test]
    fn test_empty_review_summary_is_invalid() {
        let surface = ReviewSurface {
            review_id: None,
            delivery_id: None,
            status: None,
            review_summary: Some("".to_string()),
            evidence_rollup_ids: None,
            delegation_contract_id: None,
            source_domain: None,
            target_domain: None,
            submitted_at: None,
            reviewed_at: None,
        };
        let err = surface.validate().unwrap_err();
        assert!(err.contains("review_summary"), "error: {err}");
    }

    #[test]
    fn test_whitespace_only_review_summary_is_invalid() {
        let surface = ReviewSurface {
            review_id: None,
            delivery_id: None,
            status: None,
            review_summary: Some("   ".to_string()),
            evidence_rollup_ids: None,
            delegation_contract_id: None,
            source_domain: None,
            target_domain: None,
            submitted_at: None,
            reviewed_at: None,
        };
        let err = surface.validate().unwrap_err();
        assert!(err.contains("review_summary"), "error: {err}");
    }

    #[test]
    fn test_submitted_at_without_reviewed_at_is_valid() {
        let surface = ReviewSurface {
            review_id: Some("rs-001".to_string()),
            delivery_id: Some("del-001".to_string()),
            status: Some(DeliveryStatus::Proposed),
            review_summary: Some("Pending review".to_string()),
            evidence_rollup_ids: None,
            delegation_contract_id: None,
            source_domain: None,
            target_domain: None,
            submitted_at: Some("2025-01-15T10:00:00Z".to_string()),
            reviewed_at: None,
        };
        assert!(surface.validate().is_ok());
    }

    // ------------------------------------------------------------------
    // Coarse-grained boundary proof
    // ------------------------------------------------------------------

    #[test]
    fn test_local_subtask_details_not_required() {
        // The review surface validates with only optional fields,
        // without requiring any local subtask details from either the
        // source or target domain.
        let variants = [
            None,
            Some(DeliveryStatus::Proposed),
            Some(DeliveryStatus::Accepted),
            Some(DeliveryStatus::NeedsRevision),
        ];
        for status in &variants {
            let surface = ReviewSurface {
                review_id: None,
                delivery_id: None,
                status: *status,
                review_summary: None,
                evidence_rollup_ids: None,
                delegation_contract_id: None,
                source_domain: None,
                target_domain: None,
                submitted_at: None,
                reviewed_at: None,
            };
            assert!(
                surface.validate().is_ok(),
                "expected status={status:?} (only field) to be valid"
            );
        }
    }
}
