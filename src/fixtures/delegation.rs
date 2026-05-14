use std::collections::BTreeMap;

use crate::contracts::DelegationContract;

// Shared identity anchor identifiers kept in sync with the meta-agent
// `delegation_fixture_baseline.py` so integration checks consume
// the same upstream/downstream identity cases:
//
// - `backend-integration-alpha` — common upstream backend anchor
// - `wr-integration-001`         — common upstream work request anchor
// - `task-integration-001`       — common upstream task anchor
// - `parent-integration-graph`   — common downstream graph target
// - `graph` / `mesh`             — common downstream domain kinds
// ---------------------------------------------------------------------------
// Full delegation chain fixtures
// ---------------------------------------------------------------------------

fn full_delegation_chain_graph() -> DelegationContract {
    DelegationContract {
        delegation_id: Some("del-full-chain-graph".into()),
        upstream_domain_id: Some("limenet".into()),
        downstream_domain_id: Some("local-meta-agent".into()),
        delivery_contract_id: Some("dc-integration-001".into()),
        visibility_policy: Some(
            crate::contracts::VisibilityPolicy::Shared,
        ),
        evidence_rollup_policy: Some(
            crate::contracts::EvidenceRollupPolicy::Summary,
        ),
        status_mapping_policy: Some(
            crate::contracts::StatusMappingPolicy::Strict,
        ),
        trace_context: None,
        upstream_work_request_id: Some("wr-integration-001".into()),
        upstream_task_id: Some("task-integration-001".into()),
        upstream_backend_id: Some("backend-integration-alpha".into()),
        downstream_domain_kind: "graph".into(),
        downstream_graph_id: Some("parent-integration-graph".into()),
    }
}

fn full_delegation_chain_mesh() -> DelegationContract {
    DelegationContract {
        delegation_id: Some("del-full-chain-mesh".into()),
        upstream_domain_id: Some("limenet".into()),
        downstream_domain_id: Some("local-meta-agent".into()),
        delivery_contract_id: Some("dc-integration-001".into()),
        visibility_policy: Some(
            crate::contracts::VisibilityPolicy::Shared,
        ),
        evidence_rollup_policy: Some(
            crate::contracts::EvidenceRollupPolicy::Summary,
        ),
        status_mapping_policy: Some(
            crate::contracts::StatusMappingPolicy::Strict,
        ),
        trace_context: None,
        upstream_work_request_id: Some("wr-integration-001".into()),
        upstream_task_id: Some("task-integration-001".into()),
        upstream_backend_id: Some("backend-integration-alpha".into()),
        downstream_domain_kind: "mesh".into(),
        downstream_graph_id: Some("parent-integration-graph".into()),
    }
}

// ---------------------------------------------------------------------------
// Upstream identity anchor fixtures
// ---------------------------------------------------------------------------

fn upstream_identity_anchors() -> DelegationContract {
    DelegationContract {
        delegation_id: Some("del-upstream-identity".into()),
        upstream_domain_id: Some("limenet".into()),
        downstream_domain_id: Some("local-meta-agent".into()),
        delivery_contract_id: Some("dc-integration-001".into()),
        visibility_policy: Some(
            crate::contracts::VisibilityPolicy::Shared,
        ),
        evidence_rollup_policy: Some(
            crate::contracts::EvidenceRollupPolicy::Summary,
        ),
        status_mapping_policy: Some(
            crate::contracts::StatusMappingPolicy::Strict,
        ),
        trace_context: None,
        upstream_work_request_id: Some("wr-integration-001".into()),
        upstream_task_id: Some("task-integration-001".into()),
        upstream_backend_id: Some("backend-integration-alpha".into()),
        downstream_domain_kind: "graph".into(),
        downstream_graph_id: None,
    }
}

fn backend_work_request_chain() -> DelegationContract {
    DelegationContract {
        delegation_id: Some("del-backend-wr-chain".into()),
        upstream_domain_id: Some("limenet".into()),
        downstream_domain_id: Some("local-meta-agent".into()),
        delivery_contract_id: Some("dc-integration-001".into()),
        visibility_policy: Some(
            crate::contracts::VisibilityPolicy::Shared,
        ),
        evidence_rollup_policy: Some(
            crate::contracts::EvidenceRollupPolicy::Summary,
        ),
        status_mapping_policy: Some(
            crate::contracts::StatusMappingPolicy::Strict,
        ),
        trace_context: None,
        upstream_work_request_id: Some("wr-integration-001".into()),
        upstream_task_id: None,
        upstream_backend_id: Some("backend-integration-alpha".into()),
        downstream_domain_kind: "graph".into(),
        downstream_graph_id: None,
    }
}

fn backend_anchored() -> DelegationContract {
    DelegationContract {
        delegation_id: Some("del-backend-anchored".into()),
        upstream_domain_id: Some("limenet".into()),
        downstream_domain_id: Some("local-meta-agent".into()),
        delivery_contract_id: Some("dc-integration-001".into()),
        visibility_policy: Some(
            crate::contracts::VisibilityPolicy::Shared,
        ),
        evidence_rollup_policy: Some(
            crate::contracts::EvidenceRollupPolicy::Summary,
        ),
        status_mapping_policy: Some(
            crate::contracts::StatusMappingPolicy::Strict,
        ),
        trace_context: None,
        upstream_work_request_id: None,
        upstream_task_id: None,
        upstream_backend_id: Some("backend-integration-alpha".into()),
        downstream_domain_kind: "graph".into(),
        downstream_graph_id: None,
    }
}

// ---------------------------------------------------------------------------
// Downstream target fixtures
// ---------------------------------------------------------------------------

fn downstream_target_graph() -> DelegationContract {
    DelegationContract {
        delegation_id: Some("del-downstream-graph".into()),
        upstream_domain_id: None,
        downstream_domain_id: Some("local-meta-agent".into()),
        delivery_contract_id: Some("dc-integration-001".into()),
        visibility_policy: Some(
            crate::contracts::VisibilityPolicy::Shared,
        ),
        evidence_rollup_policy: Some(
            crate::contracts::EvidenceRollupPolicy::Summary,
        ),
        status_mapping_policy: Some(
            crate::contracts::StatusMappingPolicy::Strict,
        ),
        trace_context: None,
        upstream_work_request_id: None,
        upstream_task_id: None,
        upstream_backend_id: None,
        downstream_domain_kind: "graph".into(),
        downstream_graph_id: Some("parent-integration-graph".into()),
    }
}

fn downstream_target_mesh() -> DelegationContract {
    DelegationContract {
        delegation_id: Some("del-downstream-mesh".into()),
        upstream_domain_id: None,
        downstream_domain_id: Some("local-meta-agent".into()),
        delivery_contract_id: Some("dc-integration-001".into()),
        visibility_policy: Some(
            crate::contracts::VisibilityPolicy::Shared,
        ),
        evidence_rollup_policy: Some(
            crate::contracts::EvidenceRollupPolicy::Summary,
        ),
        status_mapping_policy: Some(
            crate::contracts::StatusMappingPolicy::Strict,
        ),
        trace_context: None,
        upstream_work_request_id: None,
        upstream_task_id: None,
        upstream_backend_id: None,
        downstream_domain_kind: "mesh".into(),
        downstream_graph_id: None,
    }
}

// ---------------------------------------------------------------------------
// Minimal delegation fixture
// ---------------------------------------------------------------------------

fn minimal_delegation() -> DelegationContract {
    DelegationContract {
        delegation_id: None,
        upstream_domain_id: None,
        downstream_domain_id: None,
        delivery_contract_id: None,
        visibility_policy: None,
        evidence_rollup_policy: None,
        status_mapping_policy: None,
        trace_context: None,
        upstream_work_request_id: None,
        upstream_task_id: None,
        upstream_backend_id: None,
        downstream_domain_kind: "graph".into(),
        downstream_graph_id: None,
    }
}

// ---------------------------------------------------------------------------
// Fixture collection
// ---------------------------------------------------------------------------

/// Accessor for the full set of delegation fixture records, keyed by
/// descriptive case name for integration-test parametrisation.
///
/// The eight cases mirror the meta-agent `delegation_fixture_baseline.py`:
///
/// | Case name                     | Upstream          | Downstream                 |
/// |-------------------------------|-------------------|----------------------------|
/// | `full-delegation-chain-graph` | full chain        | graph + graph_id           |
/// | `full-delegation-chain-mesh`  | full chain        | mesh + graph_id            |
/// | `upstream-identity-anchors`   | full chain        | domain only                |
/// | `backend-work-request-chain`  | backend + wr      | domain only                |
/// | `backend-anchored`            | backend only      | domain only                |
/// | `downstream-target-graph`     | —                 | graph + graph_id           |
/// | `downstream-target-mesh`      | —                 | mesh only                  |
/// | `minimal-delegation`          | —                 | domain only                |
pub struct DelegationFixtures;

impl DelegationFixtures {
    pub fn records_by_case() -> BTreeMap<&'static str, DelegationContract> {
        let mut m = BTreeMap::new();
        m.insert("full-delegation-chain-graph", full_delegation_chain_graph());
        m.insert("full-delegation-chain-mesh", full_delegation_chain_mesh());
        m.insert("upstream-identity-anchors", upstream_identity_anchors());
        m.insert("backend-work-request-chain", backend_work_request_chain());
        m.insert("backend-anchored", backend_anchored());
        m.insert("downstream-target-graph", downstream_target_graph());
        m.insert("downstream-target-mesh", downstream_target_mesh());
        m.insert("minimal-delegation", minimal_delegation());
        m
    }

    pub fn all_baseline_records() -> Vec<DelegationContract> {
        vec![
            full_delegation_chain_graph(),
            full_delegation_chain_mesh(),
            upstream_identity_anchors(),
            backend_work_request_chain(),
            backend_anchored(),
            downstream_target_graph(),
            downstream_target_mesh(),
            minimal_delegation(),
        ]
    }

    /// Validate every record in the baseline, returning `Ok(())` or the
    /// first validation error.
    pub fn validate_baseline() -> Result<(), String> {
        for record in Self::all_baseline_records() {
            record.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_baseline_validates() {
        DelegationFixtures::validate_baseline()
            .expect("all baseline delegation fixtures must validate");
    }

    #[test]
    fn test_all_eight_cases_present() {
        let cases = DelegationFixtures::records_by_case();
        assert_eq!(cases.len(), 8);
        let expected_keys: Vec<&str> = vec![
            "backend-anchored",
            "backend-work-request-chain",
            "downstream-target-graph",
            "downstream-target-mesh",
            "full-delegation-chain-graph",
            "full-delegation-chain-mesh",
            "minimal-delegation",
            "upstream-identity-anchors",
        ];
        let actual_keys: Vec<&str> = cases.keys().copied().collect();
        assert_eq!(actual_keys, expected_keys);
    }

    // -- full delegation chain tests ---------------------------------------

    #[test]
    fn test_full_delegation_chain_graph() {
        let c = full_delegation_chain_graph();
        assert_eq!(c.delegation_id.as_deref(), Some("del-full-chain-graph"));
        assert_eq!(
            c.upstream_backend_id.as_deref(),
            Some("backend-integration-alpha")
        );
        assert_eq!(
            c.upstream_work_request_id.as_deref(),
            Some("wr-integration-001")
        );
        assert_eq!(
            c.upstream_task_id.as_deref(),
            Some("task-integration-001")
        );
        assert_eq!(c.downstream_domain_kind, "graph");
        assert_eq!(
            c.downstream_graph_id.as_deref(),
            Some("parent-integration-graph")
        );
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_full_delegation_chain_mesh() {
        let c = full_delegation_chain_mesh();
        assert_eq!(c.downstream_domain_kind, "mesh");
        assert_eq!(
            c.downstream_graph_id.as_deref(),
            Some("parent-integration-graph")
        );
        assert!(c.validate().is_ok());
    }

    // -- upstream identity anchor tests ------------------------------------

    #[test]
    fn test_upstream_identity_anchors() {
        let c = upstream_identity_anchors();
        assert_eq!(
            c.upstream_backend_id.as_deref(),
            Some("backend-integration-alpha")
        );
        assert_eq!(
            c.upstream_work_request_id.as_deref(),
            Some("wr-integration-001")
        );
        assert_eq!(
            c.upstream_task_id.as_deref(),
            Some("task-integration-001")
        );
        assert!(c.downstream_graph_id.is_none());
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_backend_work_request_chain() {
        let c = backend_work_request_chain();
        assert_eq!(
            c.upstream_backend_id.as_deref(),
            Some("backend-integration-alpha")
        );
        assert_eq!(
            c.upstream_work_request_id.as_deref(),
            Some("wr-integration-001")
        );
        assert!(c.upstream_task_id.is_none());
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_backend_anchored() {
        let c = backend_anchored();
        assert_eq!(
            c.upstream_backend_id.as_deref(),
            Some("backend-integration-alpha")
        );
        assert!(c.upstream_work_request_id.is_none());
        assert!(c.upstream_task_id.is_none());
        assert!(c.validate().is_ok());
    }

    // -- downstream target tests -------------------------------------------

    #[test]
    fn test_downstream_target_graph() {
        let c = downstream_target_graph();
        assert!(c.upstream_backend_id.is_none());
        assert_eq!(c.downstream_domain_kind, "graph");
        assert_eq!(
            c.downstream_graph_id.as_deref(),
            Some("parent-integration-graph")
        );
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_downstream_target_mesh() {
        let c = downstream_target_mesh();
        assert!(c.upstream_backend_id.is_none());
        assert_eq!(c.downstream_domain_kind, "mesh");
        assert!(c.downstream_graph_id.is_none());
        assert!(c.validate().is_ok());
    }

    // -- minimal delegation test -------------------------------------------

    #[test]
    fn test_minimal_delegation() {
        let c = minimal_delegation();
        assert!(c.delegation_id.is_none());
        assert!(c.upstream_backend_id.is_none());
        assert!(c.upstream_work_request_id.is_none());
        assert!(c.upstream_task_id.is_none());
        assert_eq!(c.downstream_domain_kind, "graph");
        assert!(c.downstream_graph_id.is_none());
        assert!(c.validate().is_ok());
    }

    // -- identity anchor compatibility tests -------------------------------

    #[test]
    fn test_upstream_identity_anchors_use_shared_integration_ids() {
        // Identity anchors must remain compatible with meta-agent fixtures
        for record in DelegationFixtures::all_baseline_records() {
            if let Some(ref backend_id) = record.upstream_backend_id {
                assert!(
                    backend_id.contains("integration"),
                    "backend_id must use shared integration anchor: {backend_id}"
                );
            }
            if let Some(ref wr_id) = record.upstream_work_request_id {
                assert!(
                    wr_id.contains("integration"),
                    "work_request_id must use shared integration anchor: {wr_id}"
                );
            }
            if let Some(ref task_id) = record.upstream_task_id {
                assert!(
                    task_id.contains("integration"),
                    "task_id must use shared integration anchor: {task_id}"
                );
            }
        }
    }

    #[test]
    fn test_downstream_targets_use_shared_graph_anchor() {
        for record in DelegationFixtures::all_baseline_records() {
            if let Some(ref graph_id) = record.downstream_graph_id {
                assert_eq!(
                    graph_id, "parent-integration-graph",
                    "downstream_graph_id must use shared integration anchor"
                );
            }
        }
    }
}
