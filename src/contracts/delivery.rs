use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Structured delivery validation error for cross-repo integration comparison.
///
/// Each error carries a stable `error` discriminator (`"validation_failed"`),
/// a `reason` classifying the failure (`unsupported_status`, `missing_anchor`,
/// `invalid_value`, `missing_field`), the `field` and `value` involved, and the `anchor`
/// context identifying which reference caused the failure.
#[derive(Debug, Clone, Serialize)]
pub struct DeliveryError {
    /// Stable error discriminator — always `"validation_failed"`.
    pub error: String,
    /// Structured reason: `"unsupported_status"`, `"missing_anchor"`,
    /// `"invalid_value"`, or `"missing_field"`.
    pub reason: String,
    /// The field that caused the validation failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// The invalid or unsupported value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The anchor context: `"delegation"`, `"ownership"`, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
}

impl DeliveryError {
    fn unsupported_status(value: &str) -> Self {
        Self {
            error: "validation_failed".into(),
            reason: "unsupported_status".into(),
            field: None,
            value: Some(value.into()),
            anchor: None,
        }
    }

    fn missing_anchor(field: &str, anchor: &str) -> Self {
        Self {
            error: "validation_failed".into(),
            reason: "missing_anchor".into(),
            field: Some(field.into()),
            value: None,
            anchor: Some(anchor.into()),
        }
    }

    fn invalid_value(field: &str) -> Self {
        Self {
            error: "validation_failed".into(),
            reason: "invalid_value".into(),
            field: Some(field.into()),
            value: None,
            anchor: None,
        }
    }

    fn missing_field(field: &str, anchor: &str) -> Self {
        Self {
            error: "validation_failed".into(),
            reason: "missing_field".into(),
            field: Some(field.into()),
            value: None,
            anchor: Some(anchor.into()),
        }
    }
}

impl fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reason.as_str() {
            "unsupported_status" => {
                let val = self.value.as_deref().unwrap_or("");
                write!(f, "unknown delivery status: {val}")
            }
            "missing_anchor" => {
                let field = self.field.as_deref().unwrap_or("field");
                let anchor = self.anchor.as_deref().unwrap_or("unknown");
                write!(f, "{field} is required ({anchor} anchor)")
            }
            "invalid_value" => {
                let field = self.field.as_deref().unwrap_or("field");
                write!(f, "{field} must be non-empty when set")
            }
            "missing_field" => {
                let field = self.field.as_deref().unwrap_or("field");
                let anchor = self.anchor.as_deref().unwrap_or("unknown");
                write!(f, "{field} is required ({anchor})")
            }
            _ => write!(f, "delivery validation failed"),
        }
    }
}

/// Lifecycle status of a delivery in the LimeNet review surface.
///
/// Mirrors the shared cross-domain vocabulary so that every
/// participant observes the same set of states regardless of
/// the originating domain's internal representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

impl DeliveryStatus {
    /// Returns the canonical snake_case string representation of this status.
    ///
    /// This is the same string that Serde uses for serialization and that
    /// `TryFrom<&str>` accepts, provided here in O(1) without any
    /// allocation or serialization overhead.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::NeedsRevision => "needs_revision",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
        }
    }

    /// Validates an incoming status string and returns a structured
    /// [`DeliveryError`] on failure, suitable for cross-repo comparison.
    ///
    /// Unsupported status values are surfaced explicitly via the
    /// `"unsupported_status"` reason code and the rejected value
    /// in the `value` field.
    pub fn validate_status(value: &str) -> Result<Self, DeliveryError> {
        Self::try_from(value).map_err(|_| DeliveryError::unsupported_status(value))
    }
}

impl fmt::Display for DeliveryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DeliveryStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

/// Uses the same [`TryFrom`] logic as the ingest boundary so that Serde
/// rejects unknown variants with a consistent error message regardless
/// of whether the value arrives via a standalone status string or embedded
/// inside a [`ReviewSurface`](crate::contracts::ReviewSurface).
impl<'de> serde::Deserialize<'de> for DeliveryStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        struct DeliveryStatusVisitor;

        impl<'de> de::Visitor<'de> for DeliveryStatusVisitor {
            type Value = DeliveryStatus;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str(
                    "one of: proposed, accepted, needs_revision, rejected, superseded",
                )
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<DeliveryStatus, E> {
                DeliveryStatus::try_from(value)
                    .map_err(|msg| E::custom(msg))
            }
        }

        deserializer.deserialize_str(DeliveryStatusVisitor)
    }
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

/// A reference to a piece of evidence in the run artifacts.
///
/// Matches the shared Phase 2B wire shape emitted by the meta-agent
/// Python `EvidenceRef` dataclass.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceRef {
    /// The artifact category (e.g. "run", "decision", "progress")
    #[serde(default)]
    pub artifact: String,

    /// Dotted path within the artifact (e.g. "run.summary")
    #[serde(default)]
    pub path: String,

    /// The actual evidence value
    #[serde(default)]
    pub value: String,
}

/// In-process trace context for causation chains.
///
/// Matches the shared Phase 2B wire shape emitted by the meta-agent
/// Python `TraceContext` dataclass. All fields are optional for
/// deserialization flexibility; the wire format omits empty fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceContext {
    /// Links events across a single run / session
    #[serde(default)]
    pub correlation_id: Option<String>,

    /// Canonical task identifier (empty for graph-level operations)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,

    /// Identifies one worker / review attempt within a task
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,

    /// In-process cursor — the event_id of the most recent event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_event_id: Option<String>,
}

/// Coarse-grained delivery package for cross-domain review surfaces.
///
/// Wraps the delivery identity, result summary, evidence references,
/// review surface anchors, risks, unresolved items, and the delegation
/// contract anchor. Review surfaces remain indirect and do not require
/// local subtask details from either the source or target domain.
///
/// Matches the shared Phase 2B wire shape emitted by the meta-agent
/// Python `DeliveryPackage` dataclass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryPackage {
    /// Stable identifier for this delivery package
    #[serde(default)]
    pub package_id: Option<String>,

    /// Links the package to the DelegationContract that governed
    /// the cross-domain boundary
    #[serde(default)]
    pub delivery_contract_id: Option<String>,

    /// Human-readable summary of the upstream result
    #[serde(default)]
    pub result_summary: Option<String>,

    /// References to evidence supporting the result
    #[serde(default)]
    pub evidence_refs: Option<Vec<EvidenceRef>>,

    /// Coarse-grained references to review surfaces (artifact paths,
    /// report URIs) — no inline content, no per-subtask detail
    #[serde(default)]
    pub review_surface_refs: Option<Vec<String>>,

    /// Risks identified during upstream work that are still open
    /// at delivery time
    #[serde(default)]
    pub open_risks: Option<Vec<String>>,

    /// Items left unresolved by upstream work
    #[serde(default)]
    pub unresolved_items: Option<Vec<String>>,

    /// Suggested next step for the downstream domain
    #[serde(default)]
    pub recommended_next_action: Option<String>,

    /// Lifecycle status of the delivery
    #[serde(default)]
    pub delivery_status: Option<DeliveryStatus>,

    /// In-process trace context for causation tracking
    #[serde(default)]
    pub trace_context: Option<TraceContext>,
}

impl DeliveryPackage {
    /// Validates delivery package field consistency.
    ///
    /// Validation is intentionally coarse-grained to preserve
    /// review-surface semantics: only field-level sanity is
    /// checked, and no local subtask details are required.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_structured().map_err(|e| e.to_string())
    }

    /// Validates delivery package field consistency and returns a structured
    /// [`DeliveryError`] on failure, suitable for cross-repo comparison.
    ///
    /// The structured error distinguishes unsupported status values from
    /// missing anchors, and surfaces the field and anchor context involved.
    pub fn validate_structured(&self) -> Result<(), DeliveryError> {
        // The delivery contract anchor is required — every delivery
        // package must trace back to an authorizing delegation contract.
        if self.delivery_contract_id.as_ref().is_none_or(|s| s.is_empty()) {
            return Err(DeliveryError::missing_anchor(
                "delivery_contract_id",
                "delegation",
            ));
        }

        // package_id is required — every delivery package must have a
        // stable identity.
        if self.package_id.as_ref().is_none_or(|s| s.is_empty()) {
            return Err(DeliveryError::missing_field(
                "package_id",
                "identity",
            ));
        }

        // result_summary must be non-empty when set
        if let Some(ref s) = self.result_summary {
            if s.trim().is_empty() {
                return Err(DeliveryError::invalid_value("result_summary"));
            }
        }

        // recommended_next_action must be non-empty when set
        if let Some(ref s) = self.recommended_next_action {
            if s.trim().is_empty() {
                return Err(DeliveryError::invalid_value("recommended_next_action"));
            }
        }

        // review_surface_refs must be present when evidence_refs are
        // populated — review-surface anchors are required for each
        // evidence point
        if self.evidence_refs.as_ref().is_some_and(|refs| !refs.is_empty()) {
            if self.review_surface_refs.as_ref().is_none_or(|refs| refs.is_empty()) {
                return Err(DeliveryError::missing_field(
                    "review_surface_refs",
                    "review_surface",
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_minimal() {
        let json = r#"{}"#;
        let pkg: DeliveryPackage = serde_json::from_str(json).unwrap();
        assert!(pkg.package_id.is_none());
        assert!(pkg.delivery_contract_id.is_none());
        assert!(pkg.result_summary.is_none());
        assert!(pkg.evidence_refs.is_none());
        assert!(pkg.review_surface_refs.is_none());
        assert!(pkg.open_risks.is_none());
        assert!(pkg.unresolved_items.is_none());
        assert!(pkg.recommended_next_action.is_none());
        assert!(pkg.delivery_status.is_none());
        assert!(pkg.trace_context.is_none());
    }

    #[test]
    fn test_deserialize_partial() {
        let json = r#"{
            "package_id":"dp-001",
            "delivery_contract_id":"dc-001",
            "result_summary":"All checks passed"
        }"#;
        let pkg: DeliveryPackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.package_id, Some("dp-001".to_string()));
        assert_eq!(pkg.delivery_contract_id, Some("dc-001".to_string()));
        assert_eq!(pkg.result_summary, Some("All checks passed".to_string()));
        assert!(pkg.evidence_refs.is_none());
        assert!(pkg.review_surface_refs.is_none());
        assert!(pkg.delivery_status.is_none());
        assert!(pkg.trace_context.is_none());
    }

    #[test]
    fn test_deserialize_with_evidence_refs() {
        let json = r#"{
            "package_id":"dp-001",
            "delivery_contract_id":"dc-001",
            "result_summary":"All checks passed",
            "evidence_refs":[
                {"artifact":"run","path":"run.summary","value":"All acceptance criteria satisfied"}
            ],
            "review_surface_refs":["review/surface-001.md"],
            "delivery_status":"proposed"
        }"#;
        let pkg: DeliveryPackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.package_id, Some("dp-001".to_string()));
        let refs = pkg.evidence_refs.unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].artifact, "run");
        assert_eq!(refs[0].path, "run.summary");
        assert_eq!(refs[0].value, "All acceptance criteria satisfied");
        assert_eq!(pkg.review_surface_refs.unwrap().len(), 1);
        assert_eq!(pkg.delivery_status, Some(DeliveryStatus::Proposed));
    }

    #[test]
    fn test_deserialize_with_trace_context() {
        let json = r#"{
            "package_id":"dp-001",
            "delivery_contract_id":"dc-001",
            "result_summary":"Done",
            "recommended_next_action":"proceed",
            "delivery_status":"accepted",
            "trace_context":{
                "correlation_id":"corr-001",
                "task_id":"T-001",
                "attempt_id":"att-001"
            }
        }"#;
        let pkg: DeliveryPackage = serde_json::from_str(json).unwrap();
        let tc = pkg.trace_context.unwrap();
        assert_eq!(tc.correlation_id, Some("corr-001".to_string()));
        assert_eq!(tc.task_id, Some("T-001".to_string()));
        assert_eq!(tc.attempt_id, Some("att-001".to_string()));
        assert_eq!(tc.last_event_id, None);
    }

    #[test]
    fn test_deserialize_status_variants() {
        for (json_str, expected) in &[
            (r#"{"delivery_status":"proposed"}"#, DeliveryStatus::Proposed),
            (r#"{"delivery_status":"accepted"}"#, DeliveryStatus::Accepted),
            (r#"{"delivery_status":"needs_revision"}"#, DeliveryStatus::NeedsRevision),
            (r#"{"delivery_status":"rejected"}"#, DeliveryStatus::Rejected),
            (r#"{"delivery_status":"superseded"}"#, DeliveryStatus::Superseded),
        ] {
            let pkg: DeliveryPackage = serde_json::from_str(json_str).unwrap();
            assert_eq!(pkg.delivery_status, Some(*expected));
        }
    }

    #[test]
    fn test_serde_roundtrip() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".to_string()),
            delivery_contract_id: Some("dc-001".to_string()),
            result_summary: Some("All checks passed".to_string()),
            evidence_refs: Some(vec![EvidenceRef {
                artifact: "run".to_string(),
                path: "run.summary".to_string(),
                value: "All criteria met".to_string(),
            }]),
            review_surface_refs: Some(vec!["review/surface-001.md".to_string()]),
            open_risks: Some(vec!["Edge-case risk".to_string()]),
            unresolved_items: Some(vec!["Pending benchmark".to_string()]),
            recommended_next_action: Some("proceed".to_string()),
            delivery_status: Some(DeliveryStatus::Proposed),
            trace_context: Some(TraceContext {
                correlation_id: Some("corr-001".to_string()),
                task_id: Some("T-001".to_string()),
                attempt_id: None,
                last_event_id: None,
            }),
        };
        let json = serde_json::to_string(&pkg).unwrap();
        let deserialized: DeliveryPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.package_id, Some("dp-001".to_string()));
        assert_eq!(deserialized.delivery_contract_id, Some("dc-001".to_string()));
        assert_eq!(deserialized.result_summary, Some("All checks passed".to_string()));
        assert_eq!(deserialized.delivery_status, Some(DeliveryStatus::Proposed));
        let refs = deserialized.evidence_refs.unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].artifact, "run");
        assert_eq!(deserialized.review_surface_refs.unwrap().len(), 1);
        assert_eq!(deserialized.open_risks.unwrap().len(), 1);
        assert_eq!(deserialized.unresolved_items.unwrap().len(), 1);
        assert_eq!(deserialized.recommended_next_action, Some("proceed".to_string()));
        let tc = deserialized.trace_context.unwrap();
        assert_eq!(tc.correlation_id, Some("corr-001".to_string()));
        assert_eq!(tc.task_id, Some("T-001".to_string()));
        assert!(tc.attempt_id.is_none());
        assert!(tc.last_event_id.is_none());
    }

    #[test]
    fn test_serde_rejects_unknown_delivery_status() {
        let result: Result<DeliveryPackage, _> =
            serde_json::from_str(r#"{"delivery_status":"unknown"}"#);
        assert!(
            result.is_err(),
            "expected deserialization error for unknown delivery_status"
        );
    }

    // ------------------------------------------------------------------
    // Validation tests
    // ------------------------------------------------------------------

    #[test]
    fn test_minimal_package_fails_validation_missing_contract() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: None,
            result_summary: None,
            evidence_refs: None,
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: Some(DeliveryStatus::Proposed),
            trace_context: None,
        };
        let err = pkg.validate().unwrap_err();
        assert!(err.contains("delivery_contract_id"), "error: {err}");
    }

    #[test]
    fn test_minimal_package_fails_validation_missing_package_id() {
        let pkg = DeliveryPackage {
            package_id: None,
            delivery_contract_id: Some("dc-001".into()),
            result_summary: None,
            evidence_refs: None,
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: Some(DeliveryStatus::Proposed),
            trace_context: None,
        };
        let err = pkg.validate().unwrap_err();
        assert!(err.contains("package_id"), "error: {err}");
    }

    #[test]
    fn test_fully_populated_package_is_valid() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: Some("dc-001".into()),
            result_summary: Some("All gates passed".into()),
            evidence_refs: Some(vec![EvidenceRef {
                artifact: "run".into(),
                path: "run.summary".into(),
                value: "All checks passed".into(),
            }]),
            review_surface_refs: Some(vec!["review/approved-001.md".into()]),
            open_risks: Some(vec![]),
            unresolved_items: Some(vec![]),
            recommended_next_action: Some("proceed".into()),
            delivery_status: Some(DeliveryStatus::Accepted),
            trace_context: Some(TraceContext {
                correlation_id: Some("corr-001".into()),
                task_id: Some("T-001".into()),
                attempt_id: None,
                last_event_id: None,
            }),
        };
        assert!(pkg.validate().is_ok());
    }

    #[test]
    fn test_empty_result_summary_is_invalid() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: Some("dc-001".into()),
            result_summary: Some("   ".into()),
            evidence_refs: None,
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: None,
            trace_context: None,
        };
        let err = pkg.validate().unwrap_err();
        assert!(err.contains("result_summary"), "error: {err}");
    }

    #[test]
    fn test_empty_recommended_next_action_is_invalid() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: Some("dc-001".into()),
            result_summary: None,
            evidence_refs: None,
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: Some("".into()),
            delivery_status: None,
            trace_context: None,
        };
        let err = pkg.validate().unwrap_err();
        assert!(err.contains("recommended_next_action"), "error: {err}");
    }

    #[test]
    fn test_evidence_without_review_surface_is_invalid() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: Some("dc-001".into()),
            result_summary: None,
            evidence_refs: Some(vec![EvidenceRef {
                artifact: "run".into(),
                path: "run.summary".into(),
                value: "passed".into(),
            }]),
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: None,
            trace_context: None,
        };
        let err = pkg.validate().unwrap_err();
        assert!(err.contains("review_surface_refs"), "error: {err}");
    }

    #[test]
    fn test_evidence_with_empty_review_surface_is_invalid() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: Some("dc-001".into()),
            result_summary: None,
            evidence_refs: Some(vec![EvidenceRef {
                artifact: "run".into(),
                path: "run.summary".into(),
                value: "passed".into(),
            }]),
            review_surface_refs: Some(vec![]),
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: None,
            trace_context: None,
        };
        let err = pkg.validate().unwrap_err();
        assert!(err.contains("review_surface_refs"), "error: {err}");
    }

    #[test]
    fn test_local_subtask_details_not_required() {
        // The delivery package validates with only the required identity
        // and delegation contract anchor, without requiring any local
        // subtask details from either the source or target domain.
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: Some("dc-001".into()),
            result_summary: None,
            evidence_refs: None,
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: Some(DeliveryStatus::Proposed),
            trace_context: None,
        };
        assert!(pkg.validate().is_ok());
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
        assert!(
            result.is_err(),
            "expected deserialization error for unknown delivery status"
        );
    }

    #[test]
    fn test_delivery_status_is_copy() {
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
        for (text, status) in &[
            ("proposed", DeliveryStatus::Proposed),
            ("accepted", DeliveryStatus::Accepted),
            ("needs_revision", DeliveryStatus::NeedsRevision),
            ("rejected", DeliveryStatus::Rejected),
            ("superseded", DeliveryStatus::Superseded),
        ] {
            let mapped = DeliveryStatus::try_from(*text).unwrap();
            assert_eq!(mapped, *status);
            let serialized = serde_json::to_string(status).unwrap();
            assert_eq!(serialized, format!("\"{text}\""));
        }
    }

    // ------------------------------------------------------------------
    // FromStr tests
    // ------------------------------------------------------------------

    #[test]
    fn test_from_str_supported_values() {
        assert_eq!(
            "proposed".parse::<DeliveryStatus>().unwrap(),
            DeliveryStatus::Proposed,
        );
        assert_eq!(
            "accepted".parse::<DeliveryStatus>().unwrap(),
            DeliveryStatus::Accepted,
        );
        assert_eq!(
            "needs_revision".parse::<DeliveryStatus>().unwrap(),
            DeliveryStatus::NeedsRevision,
        );
        assert_eq!(
            "rejected".parse::<DeliveryStatus>().unwrap(),
            DeliveryStatus::Rejected,
        );
        assert_eq!(
            "superseded".parse::<DeliveryStatus>().unwrap(),
            DeliveryStatus::Superseded,
        );
    }

    #[test]
    fn test_from_str_unsupported_values() {
        assert!("unknown".parse::<DeliveryStatus>().is_err());
        assert!("".parse::<DeliveryStatus>().is_err());
        assert!("Proposed".parse::<DeliveryStatus>().is_err());
        assert!("pending".parse::<DeliveryStatus>().is_err());
    }

    // ------------------------------------------------------------------
    // validate_structured error detail tests
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_structured_missing_delegation_anchor() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: None,
            result_summary: None,
            evidence_refs: None,
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: Some(DeliveryStatus::Proposed),
            trace_context: None,
        };
        let err = pkg.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "missing_anchor");
        assert_eq!(err.field.as_deref(), Some("delivery_contract_id"));
        assert_eq!(err.anchor.as_deref(), Some("delegation"));
    }

    #[test]
    fn test_validate_structured_missing_package_id() {
        let pkg = DeliveryPackage {
            package_id: None,
            delivery_contract_id: Some("dc-001".into()),
            result_summary: None,
            evidence_refs: None,
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: None,
            trace_context: None,
        };
        let err = pkg.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "missing_field");
        assert_eq!(err.field.as_deref(), Some("package_id"));
        assert_eq!(err.anchor.as_deref(), Some("identity"));
    }

    #[test]
    fn test_validate_structured_empty_result_summary() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: Some("dc-001".into()),
            result_summary: Some("".into()),
            evidence_refs: None,
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: None,
            trace_context: None,
        };
        let err = pkg.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "invalid_value");
        assert_eq!(err.field.as_deref(), Some("result_summary"));
    }

    #[test]
    fn test_validate_structured_missing_review_surface() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: Some("dc-001".into()),
            result_summary: None,
            evidence_refs: Some(vec![EvidenceRef {
                artifact: "run".into(),
                path: "run.summary".into(),
                value: "passed".into(),
            }]),
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: None,
            trace_context: None,
        };
        let err = pkg.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "missing_field");
        assert_eq!(err.field.as_deref(), Some("review_surface_refs"));
        assert_eq!(err.anchor.as_deref(), Some("review_surface"));
    }

    #[test]
    fn test_validate_structured_valid_passes() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: Some("dc-001".into()),
            result_summary: Some("All gates passed".into()),
            evidence_refs: Some(vec![EvidenceRef {
                artifact: "run".into(),
                path: "run.summary".into(),
                value: "All checks passed".into(),
            }]),
            review_surface_refs: Some(vec!["review/approved-001.md".into()]),
            open_risks: Some(vec![]),
            unresolved_items: Some(vec![]),
            recommended_next_action: Some("proceed".into()),
            delivery_status: Some(DeliveryStatus::Accepted),
            trace_context: Some(TraceContext {
                correlation_id: Some("corr-001".into()),
                task_id: Some("T-001".into()),
                attempt_id: None,
                last_event_id: None,
            }),
        };
        assert!(pkg.validate_structured().is_ok());
    }

    // ------------------------------------------------------------------
    // DeliveryError tests
    // ------------------------------------------------------------------

    #[test]
    fn test_delivery_error_unsupported_status_display() {
        let err = DeliveryError::unsupported_status("bogus");
        assert_eq!(err.to_string(), "unknown delivery status: bogus");
    }

    #[test]
    fn test_delivery_error_missing_anchor_display() {
        let err = DeliveryError::missing_anchor("delivery_contract_id", "delegation");
        assert_eq!(
            err.to_string(),
            "delivery_contract_id is required (delegation anchor)"
        );
    }

    #[test]
    fn test_delivery_error_invalid_value_display() {
        let err = DeliveryError::invalid_value("result_summary");
        assert_eq!(err.to_string(), "result_summary must be non-empty when set");
    }

    #[test]
    fn test_delivery_error_missing_field_display() {
        let err = DeliveryError::missing_field("review_surface_refs", "review_surface");
        assert_eq!(err.to_string(), "review_surface_refs is required (review_surface)");
    }

    #[test]
    fn test_validate_structured_display_matches_validate() {
        let cases: Vec<DeliveryPackage> = vec![
            // Missing delegation anchor
            DeliveryPackage {
                package_id: Some("dp-001".into()),
                delivery_contract_id: None,
                result_summary: None,
                evidence_refs: None,
                review_surface_refs: None,
                open_risks: None,
                unresolved_items: None,
                recommended_next_action: None,
                delivery_status: None,
                trace_context: None,
            },
            // Empty result_summary
            DeliveryPackage {
                package_id: Some("dp-001".into()),
                delivery_contract_id: Some("dc-001".into()),
                result_summary: Some("".into()),
                evidence_refs: None,
                review_surface_refs: None,
                open_risks: None,
                unresolved_items: None,
                recommended_next_action: None,
                delivery_status: None,
                trace_context: None,
            },
        ];
        for pkg in cases {
            let str_err = pkg.validate().unwrap_err();
            let structured = pkg.validate_structured().unwrap_err();
            assert_eq!(
                str_err,
                structured.to_string(),
                "Display must match validate() for {pkg:?}"
            );
        }
    }

    // ------------------------------------------------------------------
    // DeliveryStatus validate_status tests
    // ------------------------------------------------------------------

    #[test]
    fn test_validate_status_unsupported_value() {
        let err = DeliveryStatus::validate_status("bogus").unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "unsupported_status");
        assert_eq!(err.value.as_deref(), Some("bogus"));
    }

    #[test]
    fn test_validate_status_known_value_ok() {
        let status = DeliveryStatus::validate_status("accepted").unwrap();
        assert_eq!(status, DeliveryStatus::Accepted);
    }

    // ------------------------------------------------------------------
    // EvidenceRef and TraceContext tests
    // ------------------------------------------------------------------

    #[test]
    fn test_evidence_ref_deserialize() {
        let json = r#"{"artifact":"run","path":"run.summary","value":"passed"}"#;
        let r: EvidenceRef = serde_json::from_str(json).unwrap();
        assert_eq!(r.artifact, "run");
        assert_eq!(r.path, "run.summary");
        assert_eq!(r.value, "passed");
    }

    #[test]
    fn test_evidence_ref_defaults() {
        let json = r#"{}"#;
        let r: EvidenceRef = serde_json::from_str(json).unwrap();
        assert_eq!(r.artifact, "");
        assert_eq!(r.path, "");
        assert_eq!(r.value, "");
    }

    #[test]
    fn test_trace_context_deserialize() {
        let json = r#"{"correlation_id":"corr-001","task_id":"T-001"}"#;
        let tc: TraceContext = serde_json::from_str(json).unwrap();
        assert_eq!(tc.correlation_id, Some("corr-001".to_string()));
        assert_eq!(tc.task_id, Some("T-001".to_string()));
        assert!(tc.attempt_id.is_none());
        assert!(tc.last_event_id.is_none());
    }

    #[test]
    fn test_trace_context_defaults() {
        let json = r#"{}"#;
        let tc: TraceContext = serde_json::from_str(json).unwrap();
        assert!(tc.correlation_id.is_none());
        assert!(tc.task_id.is_none());
        assert!(tc.attempt_id.is_none());
        assert!(tc.last_event_id.is_none());
    }

    #[test]
    fn test_trace_context_roundtrip() {
        let tc = TraceContext {
            correlation_id: Some("corr-001".into()),
            task_id: Some("T-001".into()),
            attempt_id: Some("att-001".into()),
            last_event_id: None,
        };
        let json = serde_json::to_string(&tc).unwrap();
        let rt: TraceContext = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.correlation_id, Some("corr-001".to_string()));
        assert_eq!(rt.task_id, Some("T-001".to_string()));
        assert_eq!(rt.attempt_id, Some("att-001".to_string()));
        assert!(rt.last_event_id.is_none());
    }
}
