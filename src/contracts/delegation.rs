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
        assert_eq!(deserialized.upstream_work_request_id, Some("wr-001".to_string()));
        assert_eq!(deserialized.upstream_task_id, Some("task-001".to_string()));
        assert_eq!(deserialized.upstream_backend_id, Some("backend-alpha".to_string()));
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
}
