use std::collections::BTreeMap;

use crate::contracts::{DeliveryPackage, EvidenceRollup, PackageType};

/// Shared identifiers kept in sync with the meta-agent
/// `delivery_fixture_baseline.py` so integration checks consume
/// the same delivery cases:
///
/// - `dc-int2b-005-g-001` — shared delivery contract anchor
/// - `dp-int2b-005-00*`   — shared package identifiers
/// - `er-int2b-005-00*`   — shared evidence rollup identifiers

const DELIVERY_CONTRACT_ID: &str = "dc-int2b-005-g-001";

// ---------------------------------------------------------------------------
// DeliveryPackage fixtures — one per delivery status axis case
// ---------------------------------------------------------------------------

fn package_proposed() -> DeliveryPackage {
    DeliveryPackage {
        delivery_id: Some("dp-int2b-005-001".into()),
        source_domain: Some("task-graph".into()),
        target_domain: Some("human-review".into()),
        package_type: PackageType::Standard,
        delegation_contract_id: Some(DELIVERY_CONTRACT_ID.into()),
        ownership_ref: None,
        payload_summary: Some(
            "All acceptance criteria satisfied with minor risks noted.".into(),
        ),
        artifact_count: Some(1),
    }
}

fn package_accepted() -> DeliveryPackage {
    DeliveryPackage {
        delivery_id: Some("dp-int2b-005-002".into()),
        source_domain: Some("task-graph".into()),
        target_domain: Some("human-review".into()),
        package_type: PackageType::Expedited,
        delegation_contract_id: Some(DELIVERY_CONTRACT_ID.into()),
        ownership_ref: Some("own-canonical-derived".into()),
        payload_summary: Some(
            "Delivery accepted; all gates passed without blocking issues.".into(),
        ),
        artifact_count: Some(2),
    }
}

fn package_needs_revision() -> DeliveryPackage {
    DeliveryPackage {
        delivery_id: Some("dp-int2b-005-003".into()),
        source_domain: Some("task-graph".into()),
        target_domain: Some("human-review".into()),
        package_type: PackageType::Standard,
        delegation_contract_id: Some(DELIVERY_CONTRACT_ID.into()),
        ownership_ref: Some("own-canonical-derived".into()),
        payload_summary: Some(
            "Two acceptance criteria need revision before delivery can proceed.".into(),
        ),
        artifact_count: Some(2),
    }
}

fn package_rejected() -> DeliveryPackage {
    DeliveryPackage {
        delivery_id: Some("dp-int2b-005-004".into()),
        source_domain: Some("task-graph".into()),
        target_domain: Some("human-review".into()),
        package_type: PackageType::Batch,
        delegation_contract_id: Some(DELIVERY_CONTRACT_ID.into()),
        ownership_ref: Some("own-canonical-derived".into()),
        payload_summary: Some(
            "Delivery rejected due to non-recoverable upstream contract drift.".into(),
        ),
        artifact_count: Some(2),
    }
}

fn package_superseded() -> DeliveryPackage {
    DeliveryPackage {
        delivery_id: Some("dp-int2b-005-005".into()),
        source_domain: Some("task-graph".into()),
        target_domain: Some("human-review".into()),
        package_type: PackageType::Standard,
        delegation_contract_id: Some(DELIVERY_CONTRACT_ID.into()),
        ownership_ref: None,
        payload_summary: Some(
            "Delivery superseded by a newer package from the same contract.".into(),
        ),
        artifact_count: Some(1),
    }
}

fn package_minimal() -> DeliveryPackage {
    DeliveryPackage {
        delivery_id: Some("dp-int2b-005-006".into()),
        source_domain: None,
        target_domain: None,
        package_type: PackageType::Standard,
        delegation_contract_id: Some(DELIVERY_CONTRACT_ID.into()),
        ownership_ref: None,
        payload_summary: Some(
            "Minimal delivery with no auxiliary surface data.".into(),
        ),
        artifact_count: None,
    }
}

// ---------------------------------------------------------------------------
// EvidenceRollup fixtures — covering density axes
// ---------------------------------------------------------------------------

fn rollup_standard() -> EvidenceRollup {
    EvidenceRollup {
        evidence_rollup_id: Some("er-int2b-005-001".into()),
        summary: Some(
            "Worker completed all acceptance criteria; no large payloads inlined.".into(),
        ),
        artifact_refs: Some(vec![
            "artifacts/attempt-001/runner.log".into(),
            "artifacts/attempt-001/diff.patch".into(),
        ]),
        source_domain: Some("task-graph".into()),
        evidence_count: Some(2),
        delivery_id: Some("dp-int2b-005-001".into()),
    }
}

fn rollup_summary_only() -> EvidenceRollup {
    EvidenceRollup {
        evidence_rollup_id: Some("er-int2b-005-002".into()),
        summary: Some(
            "Minimal summary with no artifacts or evidence refs.".into(),
        ),
        artifact_refs: Some(vec![]),
        source_domain: Some("task-graph".into()),
        evidence_count: None,
        delivery_id: Some("dp-int2b-005-006".into()),
    }
}

fn rollup_artifact_heavy() -> EvidenceRollup {
    EvidenceRollup {
        evidence_rollup_id: Some("er-int2b-005-003".into()),
        summary: Some(
            "Worker produced extensive artifacts; all large outputs referenced indirectly."
                .into(),
        ),
        artifact_refs: Some(vec![
            "artifacts/attempt-001/runner.log".into(),
            "artifacts/attempt-001/diff.patch".into(),
            "artifacts/attempt-001/provider_response.json".into(),
            "artifacts/attempt-001/stdout.txt".into(),
            "artifacts/attempt-001/stderr.txt".into(),
        ]),
        source_domain: Some("task-graph".into()),
        evidence_count: Some(5),
        delivery_id: Some("dp-int2b-005-002".into()),
    }
}

// ---------------------------------------------------------------------------
// Fixture collection
// ---------------------------------------------------------------------------

/// Accessor for the full set of delivery fixture records, keyed by
/// descriptive case name for integration-test parametrisation.
///
/// The six DeliveryPackage cases mirror the meta-agent
/// `delivery_fixture_baseline.py`:
///
/// | Case name          | delivery_id            | package_type | artifact_count |
/// |--------------------|------------------------|--------------|----------------|
/// | `proposed`         | dp-int2b-005-001       | Standard     | 1              |
/// | `accepted`         | dp-int2b-005-002       | Expedited    | 2              |
/// | `needs-revision`   | dp-int2b-005-003       | Standard     | 2              |
/// | `rejected`         | dp-int2b-005-004       | Batch        | 2              |
/// | `superseded`       | dp-int2b-005-005       | Standard     | 1              |
/// | `minimal`          | dp-int2b-005-006       | Standard     | —              |
///
/// The three EvidenceRollup cases mirror the density axes:
///
/// | Case name       | rollup_id            | evidence_count | artifact_refs |
/// |-----------------|----------------------|----------------|---------------|
/// | `standard`      | er-int2b-005-001     | 2              | 2             |
/// | `summary-only`  | er-int2b-005-002     | —              | 0             |
/// | `artifact-heavy`| er-int2b-005-003     | 5              | 5             |
pub struct DeliveryFixtures;

impl DeliveryFixtures {
    // -- DeliveryPackage accessors -------------------------------------------

    pub fn packages_by_case() -> BTreeMap<&'static str, DeliveryPackage> {
        let mut m = BTreeMap::new();
        m.insert("proposed", package_proposed());
        m.insert("accepted", package_accepted());
        m.insert("needs-revision", package_needs_revision());
        m.insert("rejected", package_rejected());
        m.insert("superseded", package_superseded());
        m.insert("minimal", package_minimal());
        m
    }

    pub fn all_baseline_packages() -> Vec<DeliveryPackage> {
        vec![
            package_proposed(),
            package_accepted(),
            package_needs_revision(),
            package_rejected(),
            package_superseded(),
            package_minimal(),
        ]
    }

    // -- EvidenceRollup accessors --------------------------------------------

    pub fn rollups_by_case() -> BTreeMap<&'static str, EvidenceRollup> {
        let mut m = BTreeMap::new();
        m.insert("standard", rollup_standard());
        m.insert("summary-only", rollup_summary_only());
        m.insert("artifact-heavy", rollup_artifact_heavy());
        m
    }

    pub fn all_baseline_rollups() -> Vec<EvidenceRollup> {
        vec![rollup_standard(), rollup_summary_only(), rollup_artifact_heavy()]
    }

    // -- Validation ----------------------------------------------------------

    /// Validate every package and rollup in the baseline, returning
    /// `Ok(())` or the first validation error.
    pub fn validate_baseline() -> Result<(), String> {
        for package in Self::all_baseline_packages() {
            package.validate()?;
        }
        for rollup in Self::all_baseline_rollups() {
            rollup.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Baseline validation
    // ------------------------------------------------------------------

    #[test]
    fn test_fixture_baseline_validates() {
        DeliveryFixtures::validate_baseline()
            .expect("all baseline delivery fixtures must validate");
    }

    // ------------------------------------------------------------------
    // Package counts
    // ------------------------------------------------------------------

    #[test]
    fn test_all_six_package_cases_present() {
        let cases = DeliveryFixtures::packages_by_case();
        assert_eq!(cases.len(), 6);
        let expected_keys: Vec<&str> = vec![
            "accepted",
            "minimal",
            "needs-revision",
            "proposed",
            "rejected",
            "superseded",
        ];
        let actual_keys: Vec<&str> = cases.keys().copied().collect();
        assert_eq!(actual_keys, expected_keys);
    }

    #[test]
    fn test_all_three_rollup_cases_present() {
        let cases = DeliveryFixtures::rollups_by_case();
        assert_eq!(cases.len(), 3);
        let expected_keys: Vec<&str> =
            vec!["artifact-heavy", "standard", "summary-only"];
        let actual_keys: Vec<&str> = cases.keys().copied().collect();
        assert_eq!(actual_keys, expected_keys);
    }

    // ------------------------------------------------------------------
    // Shared contract identifier
    // ------------------------------------------------------------------

    #[test]
    fn test_all_packages_share_contract_id() {
        for package in DeliveryFixtures::all_baseline_packages() {
            assert_eq!(
                package.delegation_contract_id.as_deref(),
                Some(DELIVERY_CONTRACT_ID),
                "package {:?} must use shared contract anchor",
                package.delivery_id,
            );
        }
    }

    // ------------------------------------------------------------------
    // Package case tests
    // ------------------------------------------------------------------

    #[test]
    fn test_package_proposed() {
        let p = package_proposed();
        assert_eq!(p.delivery_id.as_deref(), Some("dp-int2b-005-001"));
        assert_eq!(p.package_type, PackageType::Standard);
        assert_eq!(
            p.delegation_contract_id.as_deref(),
            Some(DELIVERY_CONTRACT_ID)
        );
        assert_eq!(p.artifact_count, Some(1));
        assert!(p.ownership_ref.is_none());
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_package_accepted() {
        let p = package_accepted();
        assert_eq!(p.delivery_id.as_deref(), Some("dp-int2b-005-002"));
        assert_eq!(p.package_type, PackageType::Expedited);
        assert_eq!(p.artifact_count, Some(2));
        assert!(p.ownership_ref.is_some());
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_package_needs_revision() {
        let p = package_needs_revision();
        assert_eq!(p.delivery_id.as_deref(), Some("dp-int2b-005-003"));
        assert_eq!(p.package_type, PackageType::Standard);
        assert_eq!(p.artifact_count, Some(2));
        assert!(p.payload_summary.as_deref().unwrap().contains("revision"));
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_package_rejected() {
        let p = package_rejected();
        assert_eq!(p.delivery_id.as_deref(), Some("dp-int2b-005-004"));
        assert_eq!(p.package_type, PackageType::Batch);
        assert_eq!(p.artifact_count, Some(2));
        assert!(p.payload_summary.as_deref().unwrap().contains("rejected"));
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_package_superseded() {
        let p = package_superseded();
        assert_eq!(p.delivery_id.as_deref(), Some("dp-int2b-005-005"));
        assert_eq!(p.package_type, PackageType::Standard);
        assert_eq!(p.artifact_count, Some(1));
        assert!(p.ownership_ref.is_none());
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_package_minimal() {
        let p = package_minimal();
        assert_eq!(p.delivery_id.as_deref(), Some("dp-int2b-005-006"));
        assert_eq!(p.package_type, PackageType::Standard);
        assert!(p.source_domain.is_none());
        assert!(p.target_domain.is_none());
        assert!(p.ownership_ref.is_none());
        assert!(p.artifact_count.is_none());
        assert!(p.validate().is_ok());
    }

    // ------------------------------------------------------------------
    // Rollup case tests
    // ------------------------------------------------------------------

    #[test]
    fn test_rollup_standard() {
        let r = rollup_standard();
        assert_eq!(r.evidence_rollup_id.as_deref(), Some("er-int2b-005-001"));
        assert_eq!(r.evidence_count, Some(2));
        assert_eq!(r.artifact_refs.as_ref().map(Vec::len), Some(2));
        assert_eq!(r.delivery_id.as_deref(), Some("dp-int2b-005-001"));
        assert!(r.validate().is_ok());
    }

    #[test]
    fn test_rollup_summary_only() {
        let r = rollup_summary_only();
        assert_eq!(r.evidence_rollup_id.as_deref(), Some("er-int2b-005-002"));
        assert_eq!(r.artifact_refs.as_ref().map(Vec::len), Some(0));
        assert!(r.evidence_count.is_none());
        assert_eq!(r.delivery_id.as_deref(), Some("dp-int2b-005-006"));
        assert!(r.validate().is_ok());
    }

    #[test]
    fn test_rollup_artifact_heavy() {
        let r = rollup_artifact_heavy();
        assert_eq!(r.evidence_rollup_id.as_deref(), Some("er-int2b-005-003"));
        assert_eq!(r.evidence_count, Some(5));
        let refs = r.artifact_refs.as_ref().unwrap();
        assert!(refs.len() >= 5);
        assert!(refs.iter().all(|a| a.starts_with("artifacts/")));
        assert_eq!(r.delivery_id.as_deref(), Some("dp-int2b-005-002"));
        assert!(r.validate().is_ok());
    }

    // ------------------------------------------------------------------
    // Cross-consistency checks
    // ------------------------------------------------------------------

    #[test]
    fn test_rollup_delivery_ids_reference_known_packages() {
        let package_ids: Vec<Option<String>> = DeliveryFixtures::all_baseline_packages()
            .into_iter()
            .map(|p| p.delivery_id)
            .collect();

        for rollup in DeliveryFixtures::all_baseline_rollups() {
            if let Some(ref did) = rollup.delivery_id {
                assert!(
                    package_ids.contains(&Some(did.clone())),
                    "rollup delivery_id {did} must reference a known baseline package",
                );
            }
        }
    }

    #[test]
    fn test_artifact_refs_are_indirect() {
        for rollup in DeliveryFixtures::all_baseline_rollups() {
            if let Some(ref refs) = rollup.artifact_refs {
                for art in refs {
                    assert!(
                        art.starts_with("artifacts/"),
                        "artifact_ref must remain indirect (artifacts/ prefix): {art}",
                    );
                    assert!(
                        art.len() < 200,
                        "artifact_ref must remain compact: {art}",
                    );
                }
            }
        }
    }
}
