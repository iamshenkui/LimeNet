use serde::{Deserialize, Serialize};

/// Coarse-grained delegation contract for cross-domain task handoffs.
///
/// Upstream identity anchors tie this delegation back to the original
/// work request and backend that spawned it.  Downstream fields
/// describe the target domain and (optionally) the target graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationContract {
    /// Unique identifier for this delegation
    #[serde(default)]
    pub delegation_id: Option<String>,

    /// Upstream work request that triggered this delegation
    #[serde(default)]
    pub upstream_work_request_id: Option<String>,

    /// Upstream task that produced this delegation
    #[serde(default)]
    pub upstream_task_id: Option<String>,

    /// Upstream backend that originated this delegation
    #[serde(default)]
    pub upstream_backend_id: Option<String>,

    /// Target domain kind for this delegation (e.g. "graph", "mesh")
    pub downstream_domain_kind: String,

    /// Target graph within the downstream domain
    #[serde(default)]
    pub downstream_graph_id: Option<String>,
}

impl DelegationContract {
    /// Validates delegation contract field consistency.
    ///
    /// Returns `Ok(())` if all fields are consistent,
    /// or a descriptive error string if validation fails.
    pub fn validate(&self) -> Result<(), String> {
        // Upstream identity anchor: a work request must be traceable to a backend
        if self.upstream_work_request_id.is_some() && self.upstream_backend_id.is_none() {
            return Err(
                "upstream_backend_id is required when upstream_work_request_id is set".to_string(),
            );
        }

        // Upstream identity anchor: a task must belong to a work request
        if self.upstream_task_id.is_some() && self.upstream_work_request_id.is_none() {
            return Err(
                "upstream_work_request_id is required when upstream_task_id is set".to_string(),
            );
        }

        // Downstream identity anchor: graph id without a domain kind is ambiguous
        if self.downstream_graph_id.is_some() && self.downstream_domain_kind.trim().is_empty() {
            return Err(
                "downstream_domain_kind must not be empty when downstream_graph_id is set"
                    .to_string(),
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_required_only() {
        let json = r#"{"downstream_domain_kind":"graph"}"#;
        let contract: DelegationContract = serde_json::from_str(json).unwrap();
        assert!(contract.delegation_id.is_none());
        assert!(contract.upstream_work_request_id.is_none());
        assert!(contract.upstream_task_id.is_none());
        assert!(contract.upstream_backend_id.is_none());
        assert_eq!(contract.downstream_domain_kind, "graph");
        assert!(contract.downstream_graph_id.is_none());
    }

    #[test]
    fn test_deserialize_partial() {
        let json = r#"{"delegation_id":"del-001","downstream_domain_kind":"mesh"}"#;
        let contract: DelegationContract = serde_json::from_str(json).unwrap();
        assert_eq!(contract.delegation_id, Some("del-001".to_string()));
        assert!(contract.upstream_work_request_id.is_none());
        assert!(contract.upstream_task_id.is_none());
        assert!(contract.upstream_backend_id.is_none());
        assert_eq!(contract.downstream_domain_kind, "mesh");
        assert!(contract.downstream_graph_id.is_none());
    }

    #[test]
    fn test_serde_roundtrip() {
        let contract = DelegationContract {
            delegation_id: Some("del-001".to_string()),
            upstream_work_request_id: Some("wr-001".to_string()),
            upstream_task_id: Some("task-001".to_string()),
            upstream_backend_id: Some("backend-alpha".to_string()),
            downstream_domain_kind: "graph".to_string(),
            downstream_graph_id: Some("g-001".to_string()),
        };
        let json = serde_json::to_string(&contract).unwrap();
        let deserialized: DelegationContract = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.delegation_id, Some("del-001".to_string()));
        assert_eq!(
            deserialized.upstream_work_request_id,
            Some("wr-001".to_string())
        );
        assert_eq!(deserialized.upstream_task_id, Some("task-001".to_string()));
        assert_eq!(
            deserialized.upstream_backend_id,
            Some("backend-alpha".to_string())
        );
        assert_eq!(deserialized.downstream_domain_kind, "graph");
        assert_eq!(deserialized.downstream_graph_id, Some("g-001".to_string()));
    }

    #[test]
    fn test_deserialize_with_graph_id() {
        let json = r#"{"downstream_domain_kind":"graph","downstream_graph_id":"g-002"}"#;
        let contract: DelegationContract = serde_json::from_str(json).unwrap();
        assert_eq!(contract.downstream_domain_kind, "graph");
        assert_eq!(contract.downstream_graph_id, Some("g-002".to_string()));
    }

    // ------------------------------------------------------------------
    // Validation tests — upstream identity anchors
    // ------------------------------------------------------------------

    #[test]
    fn test_upstream_work_request_requires_backend_id() {
        let contract = DelegationContract {
            delegation_id: Some("del-001".to_string()),
            upstream_work_request_id: Some("wr-001".to_string()),
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "graph".to_string(),
            downstream_graph_id: None,
        };
        let err = contract.validate().unwrap_err();
        assert!(
            err.contains("upstream_backend_id"),
            "expected error about missing upstream_backend_id, got: {err}"
        );
    }

    #[test]
    fn test_upstream_task_requires_work_request_id() {
        let contract = DelegationContract {
            delegation_id: Some("del-001".to_string()),
            upstream_work_request_id: None,
            upstream_task_id: Some("task-001".to_string()),
            upstream_backend_id: Some("backend-alpha".to_string()),
            downstream_domain_kind: "graph".to_string(),
            downstream_graph_id: None,
        };
        let err = contract.validate().unwrap_err();
        assert!(
            err.contains("upstream_work_request_id"),
            "expected error about missing upstream_work_request_id, got: {err}"
        );
    }

    #[test]
    fn test_all_upstream_fields_present_is_valid() {
        let contract = DelegationContract {
            delegation_id: Some("del-001".to_string()),
            upstream_work_request_id: Some("wr-001".to_string()),
            upstream_task_id: Some("task-001".to_string()),
            upstream_backend_id: Some("backend-alpha".to_string()),
            downstream_domain_kind: "graph".to_string(),
            downstream_graph_id: None,
        };
        assert!(contract.validate().is_ok());
    }

    #[test]
    fn test_upstream_backend_id_alone_is_valid() {
        let contract = DelegationContract {
            delegation_id: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: Some("backend-alpha".to_string()),
            downstream_domain_kind: "graph".to_string(),
            downstream_graph_id: None,
        };
        assert!(contract.validate().is_ok());
    }

    #[test]
    fn test_no_upstream_fields_is_valid() {
        let contract = DelegationContract {
            delegation_id: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "graph".to_string(),
            downstream_graph_id: None,
        };
        assert!(contract.validate().is_ok());
    }

    // ------------------------------------------------------------------
    // Validation tests — downstream identity anchors
    // ------------------------------------------------------------------

    #[test]
    fn test_downstream_graph_id_requires_non_empty_domain_kind() {
        let contract = DelegationContract {
            delegation_id: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "".to_string(),
            downstream_graph_id: Some("g-001".to_string()),
        };
        let err = contract.validate().unwrap_err();
        assert!(
            err.contains("downstream_domain_kind"),
            "expected error about empty downstream_domain_kind, got: {err}"
        );
    }

    #[test]
    fn test_downstream_graph_id_with_valid_domain_kind_is_valid() {
        let contract = DelegationContract {
            delegation_id: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "mesh".to_string(),
            downstream_graph_id: Some("g-001".to_string()),
        };
        assert!(contract.validate().is_ok());
    }

    #[test]
    fn test_no_downstream_graph_id_with_empty_domain_kind_is_valid() {
        let contract = DelegationContract {
            delegation_id: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "".to_string(),
            downstream_graph_id: None,
        };
        assert!(contract.validate().is_ok());
    }
}
