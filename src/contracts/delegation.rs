use serde::{Deserialize, Serialize};
use std::fmt;

/// Structured delegation validation error for cross-repo integration comparison.
///
/// Each error carries a stable `error` discriminator (`"validation_failed"`),
/// a `reason` classifying the failure (`missing_field`, `empty_field`),
/// the `field` involved, and the `anchor` context (`upstream` or `downstream`)
/// identifying which identity anchor group caused the failure.
#[derive(Debug, Clone, Serialize)]
pub struct DelegationError {
    /// Stable error discriminator — always `"validation_failed"`.
    pub error: String,
    /// Structured reason: `"missing_field"` or `"empty_field"`.
    pub reason: String,
    /// The field that caused the validation failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// The identity anchor context: `"upstream"` or `"downstream"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
}

impl DelegationError {
    fn missing_field(field: &str, anchor: &str) -> Self {
        Self {
            error: "validation_failed".into(),
            reason: "missing_field".into(),
            field: Some(field.into()),
            anchor: Some(anchor.into()),
        }
    }

    fn empty_field(field: &str, anchor: &str) -> Self {
        Self {
            error: "validation_failed".into(),
            reason: "empty_field".into(),
            field: Some(field.into()),
            anchor: Some(anchor.into()),
        }
    }
}

impl fmt::Display for DelegationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.reason.as_str(), self.field.as_deref()) {
            ("missing_field", Some("upstream_backend_id")) => {
                write!(
                    f,
                    "upstream_backend_id is required when upstream_work_request_id is set"
                )
            }
            ("missing_field", Some("upstream_work_request_id")) => {
                write!(
                    f,
                    "upstream_work_request_id is required when upstream_task_id is set"
                )
            }
            ("empty_field", Some("downstream_domain_kind")) => {
                write!(
                    f,
                    "downstream_domain_kind must not be empty when downstream_graph_id is set"
                )
            }
            _ => write!(f, "delegation validation failed"),
        }
    }
}

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
        self.validate_structured().map_err(|e| e.to_string())
    }

    /// Validates delegation contract field consistency and returns a structured
    /// [`DelegationError`] on failure, suitable for cross-repo comparison.
    ///
    /// The structured error distinguishes missing fields from empty fields and
    /// surfaces the identity anchor context (`upstream` or `downstream`) involved.
    pub fn validate_structured(&self) -> Result<(), DelegationError> {
        // Upstream identity anchor: a work request must be traceable to a backend
        if self.upstream_work_request_id.is_some() && self.upstream_backend_id.is_none() {
            return Err(DelegationError::missing_field(
                "upstream_backend_id",
                "upstream",
            ));
        }

        // Upstream identity anchor: a task must belong to a work request
        if self.upstream_task_id.is_some() && self.upstream_work_request_id.is_none() {
            return Err(DelegationError::missing_field(
                "upstream_work_request_id",
                "upstream",
            ));
        }

        // Downstream identity anchor: graph id without a domain kind is ambiguous
        if self.downstream_graph_id.is_some() && self.downstream_domain_kind.trim().is_empty() {
            return Err(DelegationError::empty_field(
                "downstream_domain_kind",
                "downstream",
            ));
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

    // ------------------------------------------------------------------
    // validate_structured error detail tests
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_structured_missing_upstream_backend_id() {
        let contract = DelegationContract {
            delegation_id: Some("del-001".into()),
            upstream_work_request_id: Some("wr-001".into()),
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "graph".into(),
            downstream_graph_id: None,
        };
        let err = contract.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "missing_field");
        assert_eq!(err.field.as_deref(), Some("upstream_backend_id"));
        assert_eq!(err.anchor.as_deref(), Some("upstream"));
    }

    #[test]
    fn test_validate_structured_missing_upstream_work_request_id() {
        let contract = DelegationContract {
            delegation_id: Some("del-001".into()),
            upstream_work_request_id: None,
            upstream_task_id: Some("task-001".into()),
            upstream_backend_id: Some("backend-alpha".into()),
            downstream_domain_kind: "graph".into(),
            downstream_graph_id: None,
        };
        let err = contract.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "missing_field");
        assert_eq!(err.field.as_deref(), Some("upstream_work_request_id"));
        assert_eq!(err.anchor.as_deref(), Some("upstream"));
    }

    #[test]
    fn test_validate_structured_empty_downstream_domain_kind() {
        let contract = DelegationContract {
            delegation_id: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "".into(),
            downstream_graph_id: Some("g-001".into()),
        };
        let err = contract.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "empty_field");
        assert_eq!(err.field.as_deref(), Some("downstream_domain_kind"));
        assert_eq!(err.anchor.as_deref(), Some("downstream"));
    }

    #[test]
    fn test_validate_structured_display_matches_validate() {
        // Verify that DelegationError Display output matches the string
        // produced by the original validate() for every failure case.
        let cases: Vec<DelegationContract> = vec![
            DelegationContract {
                delegation_id: Some("del-001".into()),
                upstream_work_request_id: Some("wr-001".into()),
                upstream_task_id: None,
                upstream_backend_id: None,
                downstream_domain_kind: "graph".into(),
                downstream_graph_id: None,
            },
            DelegationContract {
                delegation_id: Some("del-001".into()),
                upstream_work_request_id: None,
                upstream_task_id: Some("task-001".into()),
                upstream_backend_id: Some("backend-alpha".into()),
                downstream_domain_kind: "graph".into(),
                downstream_graph_id: None,
            },
            DelegationContract {
                delegation_id: None,
                upstream_work_request_id: None,
                upstream_task_id: None,
                upstream_backend_id: None,
                downstream_domain_kind: "".into(),
                downstream_graph_id: Some("g-001".into()),
            },
        ];
        for contract in cases {
            let str_err = contract.validate().unwrap_err();
            let structured = contract.validate_structured().unwrap_err();
            assert_eq!(
                str_err,
                structured.to_string(),
                "Display must match validate() for {contract:?}"
            );
        }
    }
}
