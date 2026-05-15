use crate::contracts::delivery::TraceContext;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// Deserialize an `Option<String>` from a JSON value, treating empty
/// strings as `None` so that Python's `""` sentinel for unset identity
/// anchors maps cleanly to Rust's `Option::None` (GAP-DEL-01).
fn deserialize_empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    Ok(opt.filter(|s| !s.is_empty()))
}

/// Visibility policy governing how delegated work is exposed across domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityPolicy {
    /// Delegated work is visible only within the upstream domain.
    Private,
    /// Delegated work is visible in both upstream and downstream domains.
    Shared,
    /// Delegated work is fully visible downstream.
    Public,
}

impl fmt::Display for VisibilityPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Private => f.write_str("private"),
            Self::Shared => f.write_str("shared"),
            Self::Public => f.write_str("public"),
        }
    }
}

/// Evidence rollup policy governing how evidence is aggregated and propagated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRollupPolicy {
    /// No evidence is rolled up across domains.
    None,
    /// Only summary-level evidence is rolled up.
    Summary,
    /// All evidence is rolled up in full.
    Full,
}

impl fmt::Display for EvidenceRollupPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("none"),
            Self::Summary => f.write_str("summary"),
            Self::Full => f.write_str("full"),
        }
    }
}

/// Status mapping policy governing how task statuses are translated across domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusMappingPolicy {
    /// 1:1 status mapping with exact correspondence.
    Strict,
    /// Best-effort mapping allowing partial correspondence.
    Loose,
    /// Pass through statuses as-is without translation.
    Passthrough,
}

impl fmt::Display for StatusMappingPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Strict => f.write_str("strict"),
            Self::Loose => f.write_str("loose"),
            Self::Passthrough => f.write_str("passthrough"),
        }
    }
}

/// Structured delegation validation error for cross-repo integration comparison.
///
/// Each error carries a stable `error` discriminator (`"validation_failed"`),
/// a `reason` classifying the failure (`missing_field`, `empty_field`,
/// `unsupported_policy`), the `field` involved, and the `anchor` context
/// (`upstream` or `downstream`) identifying which identity anchor group
/// caused the failure.
#[derive(Debug, Clone, Serialize)]
pub struct DelegationError {
    /// Stable error discriminator — always `"validation_failed"`.
    pub error: String,
    /// Structured reason: `"missing_field"`, `"empty_field"`, or `"unsupported_policy"`.
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

    fn unsupported_policy(field: &str, value: &str) -> Self {
        Self {
            error: "validation_failed".into(),
            reason: "unsupported_policy".into(),
            field: Some(field.into()),
            anchor: Some(value.into()),
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
            ("unsupported_policy", Some(field)) => {
                let value = self.anchor.as_deref().unwrap_or("");
                write!(f, "unsupported {field}: {value}")
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
///
/// Matches the shared Phase 2B wire shape emitted by the meta-agent
/// Python `DelegationContract` dataclass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationContract {
    /// Unique identifier for this delegation
    #[serde(default, deserialize_with = "deserialize_empty_string_as_none")]
    pub delegation_id: Option<String>,

    /// Upstream domain identifier (e.g. "limenet")
    #[serde(default, deserialize_with = "deserialize_empty_string_as_none")]
    pub upstream_domain_id: Option<String>,

    /// Downstream domain identifier (e.g. "local-meta-agent")
    #[serde(default, deserialize_with = "deserialize_empty_string_as_none")]
    pub downstream_domain_id: Option<String>,

    /// Stable identifier for the delivery contract instance
    #[serde(default, deserialize_with = "deserialize_empty_string_as_none")]
    pub delivery_contract_id: Option<String>,

    /// Visibility policy governing how delegated work is exposed across domains
    #[serde(default)]
    pub visibility_policy: Option<VisibilityPolicy>,

    /// Evidence rollup policy governing how evidence is aggregated and propagated
    #[serde(default)]
    pub evidence_rollup_policy: Option<EvidenceRollupPolicy>,

    /// Status mapping policy governing how task statuses are translated across domains
    #[serde(default)]
    pub status_mapping_policy: Option<StatusMappingPolicy>,

    /// In-process trace context for causation tracking
    #[serde(default)]
    pub trace_context: Option<TraceContext>,

    /// Upstream work request that triggered this delegation
    #[serde(default, deserialize_with = "deserialize_empty_string_as_none")]
    pub upstream_work_request_id: Option<String>,

    /// Upstream task that produced this delegation
    #[serde(default, deserialize_with = "deserialize_empty_string_as_none")]
    pub upstream_task_id: Option<String>,

    /// Upstream backend that originated this delegation
    #[serde(default, deserialize_with = "deserialize_empty_string_as_none")]
    pub upstream_backend_id: Option<String>,

    /// Target domain kind for this delegation (e.g. "graph", "mesh")
    #[serde(default)]
    pub downstream_domain_kind: String,

    /// Target graph within the downstream domain
    #[serde(default, deserialize_with = "deserialize_empty_string_as_none")]
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
        assert!(contract.upstream_domain_id.is_none());
        assert!(contract.downstream_domain_id.is_none());
        assert!(contract.delivery_contract_id.is_none());
        assert!(contract.visibility_policy.is_none());
        assert!(contract.evidence_rollup_policy.is_none());
        assert!(contract.status_mapping_policy.is_none());
        assert!(contract.trace_context.is_none());
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
        assert!(contract.upstream_domain_id.is_none());
        assert!(contract.downstream_domain_id.is_none());
        assert!(contract.delivery_contract_id.is_none());
        assert!(contract.visibility_policy.is_none());
        assert!(contract.evidence_rollup_policy.is_none());
        assert!(contract.status_mapping_policy.is_none());
        assert!(contract.trace_context.is_none());
        assert!(contract.upstream_work_request_id.is_none());
        assert!(contract.upstream_task_id.is_none());
        assert!(contract.upstream_backend_id.is_none());
        assert_eq!(contract.downstream_domain_kind, "mesh");
        assert!(contract.downstream_graph_id.is_none());
    }

    #[test]
    fn test_deserialize_with_policy_fields() {
        let json = r#"{
            "delegation_id":"del-001",
            "upstream_domain_id":"limenet",
            "downstream_domain_id":"local-meta-agent",
            "delivery_contract_id":"dc-001",
            "visibility_policy":"shared",
            "evidence_rollup_policy":"summary",
            "status_mapping_policy":"strict",
            "trace_context":{"correlation_id":"corr-001"},
            "downstream_domain_kind":"meta-agent"
        }"#;
        let contract: DelegationContract = serde_json::from_str(json).unwrap();
        assert_eq!(contract.delegation_id, Some("del-001".to_string()));
        assert_eq!(contract.upstream_domain_id, Some("limenet".to_string()));
        assert_eq!(contract.downstream_domain_id, Some("local-meta-agent".to_string()));
        assert_eq!(contract.delivery_contract_id, Some("dc-001".to_string()));
        assert_eq!(contract.visibility_policy, Some(VisibilityPolicy::Shared));
        assert_eq!(
            contract.evidence_rollup_policy,
            Some(EvidenceRollupPolicy::Summary)
        );
        assert_eq!(
            contract.status_mapping_policy,
            Some(StatusMappingPolicy::Strict)
        );
        assert!(contract.trace_context.is_some());
        assert_eq!(
            contract.trace_context.as_ref().unwrap().correlation_id,
            Some("corr-001".to_string())
        );
        assert_eq!(contract.downstream_domain_kind, "meta-agent");
    }

    #[test]
    fn test_deserialize_all_policy_variants() {
        let cases = [
            ("private", VisibilityPolicy::Private),
            ("shared", VisibilityPolicy::Shared),
            ("public", VisibilityPolicy::Public),
        ];
        for (json_val, expected) in cases {
            let json = format!(r#"{{"visibility_policy":"{json_val}","downstream_domain_kind":"g"}}"#);
            let contract: DelegationContract = serde_json::from_str(&json).unwrap();
            assert_eq!(contract.visibility_policy, Some(expected), "for {json_val}");
        }

        let evidence_cases = [
            ("none", EvidenceRollupPolicy::None),
            ("summary", EvidenceRollupPolicy::Summary),
            ("full", EvidenceRollupPolicy::Full),
        ];
        for (json_val, expected) in evidence_cases {
            let json = format!(r#"{{"evidence_rollup_policy":"{json_val}","downstream_domain_kind":"g"}}"#);
            let contract: DelegationContract = serde_json::from_str(&json).unwrap();
            assert_eq!(
                contract.evidence_rollup_policy,
                Some(expected),
                "for {json_val}"
            );
        }

        let status_cases = [
            ("strict", StatusMappingPolicy::Strict),
            ("loose", StatusMappingPolicy::Loose),
            ("passthrough", StatusMappingPolicy::Passthrough),
        ];
        for (json_val, expected) in status_cases {
            let json = format!(r#"{{"status_mapping_policy":"{json_val}","downstream_domain_kind":"g"}}"#);
            let contract: DelegationContract = serde_json::from_str(&json).unwrap();
            assert_eq!(
                contract.status_mapping_policy,
                Some(expected),
                "for {json_val}"
            );
        }
    }

    #[test]
    fn test_serde_roundtrip() {
        let contract = DelegationContract {
            delegation_id: Some("del-001".to_string()),
            upstream_domain_id: Some("limenet".to_string()),
            downstream_domain_id: Some("local-meta-agent".to_string()),
            delivery_contract_id: Some("dc-001".to_string()),
            visibility_policy: Some(VisibilityPolicy::Shared),
            evidence_rollup_policy: Some(EvidenceRollupPolicy::Summary),
            status_mapping_policy: Some(StatusMappingPolicy::Strict),
            trace_context: Some(TraceContext {
                correlation_id: Some("corr-001".into()),
                task_id: None,
                attempt_id: None,
                last_event_id: None,
            }),
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
            deserialized.upstream_domain_id,
            Some("limenet".to_string())
        );
        assert_eq!(
            deserialized.downstream_domain_id,
            Some("local-meta-agent".to_string())
        );
        assert_eq!(
            deserialized.delivery_contract_id,
            Some("dc-001".to_string())
        );
        assert_eq!(deserialized.visibility_policy, Some(VisibilityPolicy::Shared));
        assert_eq!(
            deserialized.evidence_rollup_policy,
            Some(EvidenceRollupPolicy::Summary)
        );
        assert_eq!(
            deserialized.status_mapping_policy,
            Some(StatusMappingPolicy::Strict)
        );
        assert!(deserialized.trace_context.is_some());
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
            upstream_domain_id: None,
            downstream_domain_id: None,
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
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
            upstream_domain_id: None,
            downstream_domain_id: None,
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
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
            upstream_domain_id: None,
            downstream_domain_id: None,
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
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
            upstream_domain_id: None,
            downstream_domain_id: None,
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
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
            downstream_domain_kind: "mesh".to_string(),
            downstream_graph_id: Some("g-001".to_string()),
        };
        assert!(contract.validate().is_ok());
    }

    #[test]
    fn test_no_downstream_graph_id_with_empty_domain_kind_is_valid() {
        let contract = DelegationContract {
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
            upstream_domain_id: None,
            downstream_domain_id: None,
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
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
            upstream_domain_id: None,
            downstream_domain_id: None,
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
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
                upstream_domain_id: None,
                downstream_domain_id: None,
                delivery_contract_id: None,
                visibility_policy: None,
                evidence_rollup_policy: None,
                status_mapping_policy: None,
                trace_context: None,
                upstream_work_request_id: Some("wr-001".into()),
                upstream_task_id: None,
                upstream_backend_id: None,
                downstream_domain_kind: "graph".into(),
                downstream_graph_id: None,
            },
            DelegationContract {
                delegation_id: Some("del-001".into()),
                upstream_domain_id: None,
                downstream_domain_id: None,
                delivery_contract_id: None,
                visibility_policy: None,
                evidence_rollup_policy: None,
                status_mapping_policy: None,
                trace_context: None,
                upstream_work_request_id: None,
                upstream_task_id: Some("task-001".into()),
                upstream_backend_id: Some("backend-alpha".into()),
                downstream_domain_kind: "graph".into(),
                downstream_graph_id: None,
            },
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

    // ------------------------------------------------------------------
    // Policy field display tests
    // ------------------------------------------------------------------

    #[test]
    fn test_visibility_policy_display() {
        assert_eq!(VisibilityPolicy::Private.to_string(), "private");
        assert_eq!(VisibilityPolicy::Shared.to_string(), "shared");
        assert_eq!(VisibilityPolicy::Public.to_string(), "public");
    }

    #[test]
    fn test_evidence_rollup_policy_display() {
        assert_eq!(EvidenceRollupPolicy::None.to_string(), "none");
        assert_eq!(EvidenceRollupPolicy::Summary.to_string(), "summary");
        assert_eq!(EvidenceRollupPolicy::Full.to_string(), "full");
    }

    #[test]
    fn test_status_mapping_policy_display() {
        assert_eq!(StatusMappingPolicy::Strict.to_string(), "strict");
        assert_eq!(StatusMappingPolicy::Loose.to_string(), "loose");
        assert_eq!(StatusMappingPolicy::Passthrough.to_string(), "passthrough");
    }

    #[test]
    fn test_policy_serde_roundtrip() {
        for vp in [VisibilityPolicy::Private, VisibilityPolicy::Shared, VisibilityPolicy::Public] {
            let json = serde_json::to_string(&vp).unwrap();
            let rt: VisibilityPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(rt, vp);
        }
        for erp in [EvidenceRollupPolicy::None, EvidenceRollupPolicy::Summary, EvidenceRollupPolicy::Full] {
            let json = serde_json::to_string(&erp).unwrap();
            let rt: EvidenceRollupPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(rt, erp);
        }
        for smp in [StatusMappingPolicy::Strict, StatusMappingPolicy::Loose, StatusMappingPolicy::Passthrough] {
            let json = serde_json::to_string(&smp).unwrap();
            let rt: StatusMappingPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(rt, smp);
        }
    }

    // ------------------------------------------------------------------
    // Coarse-grained boundary proof
    // ------------------------------------------------------------------

    #[test]
    fn test_local_subtask_details_not_required() {
        // The delegation contract validates with only optional fields,
        // without requiring any local subtask details from either the
        // source or target domain.
        let contract = DelegationContract {
            delegation_id: None,
            upstream_domain_id: Some("limenet".to_string()),
            downstream_domain_id: Some("local-meta-agent".to_string()),
            delivery_contract_id: Some("dc-001".to_string()),
            visibility_policy: Some(VisibilityPolicy::Shared),
            evidence_rollup_policy: Some(EvidenceRollupPolicy::Summary),
            status_mapping_policy: Some(StatusMappingPolicy::Strict),
            trace_context: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "meta-agent".to_string(),
            downstream_graph_id: None,
        };
        assert!(contract.validate().is_ok());
    }
}
