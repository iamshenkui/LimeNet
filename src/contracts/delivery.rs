use serde::{Deserialize, Serialize};

/// Lifecycle status of a delivery in the LimeNet review surface.
///
/// Mirrors the shared cross-domain vocabulary so that every
/// participant observes the same set of states regardless of
/// the originating domain's internal representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// Delivery has been proposed but not yet accepted by the target
    Proposed,
    /// Delivery has been accepted and is being processed
    Accepted,
    /// Delivery requires revisions before it can proceed
    NeedsRevision,
    /// Delivery has been rejected and will not be processed
    Rejected,
    /// Delivery has been superseded by a newer delivery
    Superseded,
}

/// Maps an incoming string to the corresponding [`DeliveryStatus`].
///
/// # Mapping rules
///
/// LimeNet interprets each incoming delivery state string as follows:
///
/// | Incoming string     | Mapped variant      | Interpretation                                      |
/// |---------------------|---------------------|-----------------------------------------------------|
/// | `"proposed"`        | `Proposed`          | Delivery has been proposed but not yet accepted      |
/// | `"accepted"`        | `Accepted`          | Delivery has been accepted and is being processed    |
/// | `"needs_revision"`  | `NeedsRevision`     | Delivery requires revisions before it can proceed    |
/// | `"rejected"`        | `Rejected`          | Delivery has been rejected and will not be processed |
/// | `"superseded"`      | `Superseded`        | Delivery has been superseded by a newer delivery     |
///
/// Any string that does not exactly match one of the five known values
/// (including case variations, empty strings, or values from other
/// vocabularies) is rejected as unsupported.
impl TryFrom<&str> for DeliveryStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "proposed" => Ok(Self::Proposed),
            "accepted" => Ok(Self::Accepted),
            "needs_revision" => Ok(Self::NeedsRevision),
            "rejected" => Ok(Self::Rejected),
            "superseded" => Ok(Self::Superseded),
            _ => Err(format!("unknown delivery status: {value}")),
        }
    }
}

/// Supported package types for a delivery in the LimeNet system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageType {
    /// Standard delivery of review artifacts
    Standard,
    /// Expedited delivery for time-sensitive reviews
    Expedited,
    /// Batch delivery aggregating multiple review artifacts
    Batch,
}

/// Coarse-grained delivery package for cross-domain review surfaces.
///
/// Wraps the delivery identity, origin/target domains, and indirect
/// references to the delegation contract and ownership record.
/// Review surfaces remain indirect and do not require local subtask
/// details from either the source or target domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryPackage {
    /// Unique identifier for this delivery package
    #[serde(default)]
    pub delivery_id: Option<String>,

    /// Domain or system that originated this delivery
    #[serde(default)]
    pub source_domain: Option<String>,

    /// Domain or system that is the target of this delivery
    #[serde(default)]
    pub target_domain: Option<String>,

    /// Type of delivery package (standard, expedited, or batch)
    pub package_type: PackageType,

    /// Reference to the delegation contract governing this delivery
    #[serde(default)]
    pub delegation_contract_id: Option<String>,

    /// Reference to the ownership record for the delivered artifacts
    #[serde(default)]
    pub ownership_ref: Option<String>,

    /// Coarse-grained summary of the payload being delivered
    #[serde(default)]
    pub payload_summary: Option<String>,

    /// Number of artifacts included in this delivery package
    #[serde(default)]
    pub artifact_count: Option<u32>,
}

impl DeliveryPackage {
    /// Validates delivery package field consistency.
    ///
    /// Validation is intentionally coarse-grained to preserve
    /// review-surface semantics: only field-level sanity is
    /// checked, and no local subtask details are required.
    pub fn validate(&self) -> Result<(), String> {
        // A delivery package with artifact_count=0 is semantically
        // meaningless — the count describes packaged artifacts in transit
        if let Some(0) = self.artifact_count {
            return Err(
                "artifact_count must be at least 1 when set".to_string(),
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_standard() {
        let json = r#"{"package_type":"standard"}"#;
        let pkg: DeliveryPackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.package_type, PackageType::Standard);
        assert!(pkg.delivery_id.is_none());
        assert!(pkg.source_domain.is_none());
        assert!(pkg.target_domain.is_none());
        assert!(pkg.delegation_contract_id.is_none());
        assert!(pkg.ownership_ref.is_none());
        assert!(pkg.payload_summary.is_none());
        assert!(pkg.artifact_count.is_none());
    }

    #[test]
    fn test_deserialize_expedited() {
        let json = r#"{"package_type":"expedited"}"#;
        let pkg: DeliveryPackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.package_type, PackageType::Expedited);
    }

    #[test]
    fn test_deserialize_batch() {
        let json = r#"{"package_type":"batch"}"#;
        let pkg: DeliveryPackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.package_type, PackageType::Batch);
    }

    #[test]
    fn test_deserialize_partial() {
        let json = r#"{
            "delivery_id":"del-001",
            "package_type":"standard",
            "source_domain":"task-graph",
            "target_domain":"human-review"
        }"#;
        let pkg: DeliveryPackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.delivery_id, Some("del-001".to_string()));
        assert_eq!(pkg.package_type, PackageType::Standard);
        assert_eq!(pkg.source_domain, Some("task-graph".to_string()));
        assert_eq!(pkg.target_domain, Some("human-review".to_string()));
        assert!(pkg.delegation_contract_id.is_none());
        assert!(pkg.ownership_ref.is_none());
        assert!(pkg.payload_summary.is_none());
        assert!(pkg.artifact_count.is_none());
    }

    #[test]
    fn test_serde_roundtrip() {
        let pkg = DeliveryPackage {
            delivery_id: Some("del-001".to_string()),
            source_domain: Some("task-graph".to_string()),
            target_domain: Some("human-review".to_string()),
            package_type: PackageType::Batch,
            delegation_contract_id: Some("dc-001".to_string()),
            ownership_ref: Some("own-001".to_string()),
            payload_summary: Some("Review batch for sprint-42".to_string()),
            artifact_count: Some(3),
        };
        let json = serde_json::to_string(&pkg).unwrap();
        let deserialized: DeliveryPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.delivery_id, Some("del-001".to_string()));
        assert_eq!(deserialized.source_domain, Some("task-graph".to_string()));
        assert_eq!(deserialized.target_domain, Some("human-review".to_string()));
        assert_eq!(deserialized.package_type, PackageType::Batch);
        assert_eq!(deserialized.delegation_contract_id, Some("dc-001".to_string()));
        assert_eq!(deserialized.ownership_ref, Some("own-001".to_string()));
        assert_eq!(deserialized.payload_summary, Some("Review batch for sprint-42".to_string()));
        assert_eq!(deserialized.artifact_count, Some(3));
    }

    #[test]
    fn test_serde_rejects_unknown_package_type() {
        let result: Result<DeliveryPackage, _> =
            serde_json::from_str(r#"{"package_type":"unknown"}"#);
        assert!(result.is_err(), "expected deserialization error for unknown package_type");
    }

    #[test]
    fn test_missing_optional_fields_in_partial_json() {
        let pkg: DeliveryPackage = serde_json::from_str(
            r#"{"package_type":"expedited","source_domain":"mesh"}"#,
        )
        .unwrap();
        assert_eq!(pkg.package_type, PackageType::Expedited);
        assert_eq!(pkg.source_domain, Some("mesh".to_string()));
        assert!(pkg.target_domain.is_none());
        assert!(pkg.delivery_id.is_none());
        assert!(pkg.delegation_contract_id.is_none());
        assert!(pkg.ownership_ref.is_none());
        assert!(pkg.payload_summary.is_none());
        assert!(pkg.artifact_count.is_none());
    }

    // ------------------------------------------------------------------
    // Validation tests
    // ------------------------------------------------------------------

    #[test]
    fn test_minimal_package_is_valid() {
        let pkg = DeliveryPackage {
            delivery_id: None,
            source_domain: None,
            target_domain: None,
            package_type: PackageType::Standard,
            delegation_contract_id: None,
            ownership_ref: None,
            payload_summary: None,
            artifact_count: None,
        };
        assert!(pkg.validate().is_ok());
    }

    #[test]
    fn test_fully_populated_package_is_valid() {
        let pkg = DeliveryPackage {
            delivery_id: Some("del-001".to_string()),
            source_domain: Some("task-graph".to_string()),
            target_domain: Some("human-review".to_string()),
            package_type: PackageType::Expedited,
            delegation_contract_id: Some("dc-001".to_string()),
            ownership_ref: Some("own-001".to_string()),
            payload_summary: Some("Review batch for sprint-42".to_string()),
            artifact_count: Some(3),
        };
        assert!(pkg.validate().is_ok());
    }

    #[test]
    fn test_artifact_count_zero_is_invalid() {
        let pkg = DeliveryPackage {
            delivery_id: None,
            source_domain: None,
            target_domain: None,
            package_type: PackageType::Batch,
            delegation_contract_id: None,
            ownership_ref: None,
            payload_summary: None,
            artifact_count: Some(0),
        };
        let err = pkg.validate().unwrap_err();
        assert!(err.contains("artifact_count"), "error: {err}");
    }

    // ------------------------------------------------------------------
    // DeliveryStatus tests
    // ------------------------------------------------------------------

    #[test]
    fn test_deserialize_delivery_status_proposed() {
        let json = r#""proposed""#;
        let status: DeliveryStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status, DeliveryStatus::Proposed);
    }

    #[test]
    fn test_deserialize_delivery_status_accepted() {
        let json = r#""accepted""#;
        let status: DeliveryStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status, DeliveryStatus::Accepted);
    }

    #[test]
    fn test_deserialize_delivery_status_needs_revision() {
        let json = r#""needs_revision""#;
        let status: DeliveryStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status, DeliveryStatus::NeedsRevision);
    }

    #[test]
    fn test_deserialize_delivery_status_rejected() {
        let json = r#""rejected""#;
        let status: DeliveryStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status, DeliveryStatus::Rejected);
    }

    #[test]
    fn test_deserialize_delivery_status_superseded() {
        let json = r#""superseded""#;
        let status: DeliveryStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status, DeliveryStatus::Superseded);
    }

    #[test]
    fn test_serde_roundtrip_delivery_status() {
        for status in &[
            DeliveryStatus::Proposed,
            DeliveryStatus::Accepted,
            DeliveryStatus::NeedsRevision,
            DeliveryStatus::Rejected,
            DeliveryStatus::Superseded,
        ] {
            let json = serde_json::to_string(status).unwrap();
            let deserialized: DeliveryStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, *status);
        }
    }

    #[test]
    fn test_deserialize_unknown_delivery_status_is_error() {
        let result: Result<DeliveryStatus, _> = serde_json::from_str(r#""unknown""#);
        assert!(result.is_err(), "expected deserialization error for unknown delivery status");
    }

    #[test]
    fn test_delivery_status_is_copy() {
        // Verify Copy semantics — assigning does not move
        let a = DeliveryStatus::Accepted;
        let b = a;
        assert_eq!(a, b);
    }

    // ------------------------------------------------------------------
    // DeliveryStatus mapping tests
    // ------------------------------------------------------------------

    #[test]
    fn test_mapping_supported_values() {
        assert_eq!(
            DeliveryStatus::try_from("proposed"),
            Ok(DeliveryStatus::Proposed),
        );
        assert_eq!(
            DeliveryStatus::try_from("accepted"),
            Ok(DeliveryStatus::Accepted),
        );
        assert_eq!(
            DeliveryStatus::try_from("needs_revision"),
            Ok(DeliveryStatus::NeedsRevision),
        );
        assert_eq!(
            DeliveryStatus::try_from("rejected"),
            Ok(DeliveryStatus::Rejected),
        );
        assert_eq!(
            DeliveryStatus::try_from("superseded"),
            Ok(DeliveryStatus::Superseded),
        );
    }

    #[test]
    fn test_mapping_unsupported_values() {
        assert!(
            DeliveryStatus::try_from("unknown").is_err(),
            "expected error for unknown status",
        );
        assert!(
            DeliveryStatus::try_from("").is_err(),
            "expected error for empty string",
        );
        assert!(
            DeliveryStatus::try_from("Proposed").is_err(),
            "expected error for case variation",
        );
        assert!(
            DeliveryStatus::try_from("pending").is_err(),
            "expected error for pending (from another vocabulary)",
        );
        assert!(
            DeliveryStatus::try_from("cancelled").is_err(),
            "expected error for cancelled",
        );
        assert!(
            DeliveryStatus::try_from("in_review").is_err(),
            "expected error for in_review",
        );
        assert!(
            DeliveryStatus::try_from("completed").is_err(),
            "expected error for completed",
        );
    }

    #[test]
    fn test_mapping_error_message() {
        let err = DeliveryStatus::try_from("bogus").unwrap_err();
        assert!(
            err.contains("bogus"),
            "error message should include the unrecognized value: {err}",
        );
    }

    #[test]
    fn test_mapping_roundtrip() {
        // Every supported status maps back to the same string via serde
        for (text, status) in &[
            ("proposed", DeliveryStatus::Proposed),
            ("accepted", DeliveryStatus::Accepted),
            ("needs_revision", DeliveryStatus::NeedsRevision),
            ("rejected", DeliveryStatus::Rejected),
            ("superseded", DeliveryStatus::Superseded),
        ] {
            let mapped = DeliveryStatus::try_from(*text).unwrap();
            assert_eq!(mapped, *status);
            // Serde roundtrip confirms the text form matches
            let serialized = serde_json::to_string(status).unwrap();
            assert_eq!(serialized, format!("\"{text}\""));
        }
    }

    #[test]
    fn test_local_subtask_details_not_required() {
        // The delivery package validates with only the package_type field,
        // without requiring any local subtask details from either the
        // source or target domain.
        for ptype in &[
            PackageType::Standard,
            PackageType::Expedited,
            PackageType::Batch,
        ] {
            let pkg = DeliveryPackage {
                delivery_id: None,
                source_domain: None,
                target_domain: None,
                package_type: *ptype,
                delegation_contract_id: None,
                ownership_ref: None,
                payload_summary: None,
                artifact_count: None,
            };
            assert!(
                pkg.validate().is_ok(),
                "expected package_type={ptype:?} (only field) to be valid"
            );
        }
    }
}
