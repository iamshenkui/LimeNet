use serde::{Deserialize, Serialize};

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
