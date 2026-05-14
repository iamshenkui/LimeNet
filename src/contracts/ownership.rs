use serde::{Deserialize, Serialize};
use std::fmt;

/// Structured ownership validation error for cross-repo integration comparison.
///
/// Each error carries a stable `error` discriminator (`"validation_failed"`),
/// a `reason` classifying the failure (`missing_field`, `empty_field`,
/// `invalid_transition`), and the `field` and `ownership_mode` involved.
#[derive(Debug, Clone, Serialize)]
pub struct OwnershipError {
    /// Stable error discriminator — always `"validation_failed"`.
    pub error: String,
    /// Structured reason: `"missing_field"`, `"empty_field"`, or `"invalid_transition"`.
    pub reason: String,
    /// The field that caused the validation failure (e.g. `"backend_kind"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// The ownership mode context (e.g. `"mirror"`, `"promotion"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership_mode: Option<String>,
}

impl OwnershipError {
    fn missing_field(field: &str, mode: &str) -> Self {
        Self {
            error: "validation_failed".into(),
            reason: "missing_field".into(),
            field: Some(field.into()),
            ownership_mode: Some(mode.into()),
        }
    }

    fn empty_field(field: &str, mode: Option<&str>) -> Self {
        Self {
            error: "validation_failed".into(),
            reason: "empty_field".into(),
            field: Some(field.into()),
            ownership_mode: mode.map(String::from),
        }
    }

    fn invalid_transition(field: &str, mode: &str) -> Self {
        Self {
            error: "validation_failed".into(),
            reason: "invalid_transition".into(),
            field: Some(field.into()),
            ownership_mode: Some(mode.into()),
        }
    }
}

impl fmt::Display for OwnershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reason.as_str() {
            "missing_field" => {
                let mode = self.ownership_mode.as_deref().unwrap_or("unknown");
                write!(
                    f,
                    "{} is required when ownership_mode is {}",
                    self.field.as_deref().unwrap_or("field"),
                    mode
                )
            }
            "empty_field" => {
                if self.field.as_deref() == Some("created_from") {
                    write!(f, "created_from must not be empty when set")
                } else {
                    let mode = self.ownership_mode.as_deref().unwrap_or("unknown");
                    write!(
                        f,
                        "{} must not be empty when ownership_mode is {}",
                        self.field.as_deref().unwrap_or("field"),
                        mode
                    )
                }
            }
            "invalid_transition" => {
                let mode = self.ownership_mode.as_deref().unwrap_or("unknown");
                let field = self.field.as_deref().unwrap_or("field");
                write!(
                    f,
                    "invalid {mode}-mode transition: {field} is not allowed for {mode} ownership"
                )
            }
            _ => write!(f, "ownership validation failed"),
        }
    }
}

/// Supported ownership modes for a task in the LimeNet system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipMode {
    /// Task originated in this system under its own scheduling
    Canonical,
    /// Task is a mirror of an upstream task from another backend
    Mirror,
    /// Task was promoted from a subtask or inner graph node
    Promotion,
}

/// Supported backend kinds for ownership tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// Standard task backend
    Task,
    /// Workflow-level backend
    Workflow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ownership {
    /// The ownership mode for this task (canonical, mirror, or promotion)
    #[serde(default)]
    pub ownership_mode: Option<OwnershipMode>,

    /// The kind of backend system this task belongs to
    #[serde(default)]
    pub backend_kind: Option<BackendKind>,

    /// Graph or task identifier this ownership was derived from
    /// (empty when the graph is original)
    #[serde(default)]
    pub created_from: Option<String>,

    /// Reference to the source this task was promoted from
    #[serde(default)]
    pub promoted_from: Option<String>,
}

impl Ownership {
    /// Validates ownership DTO lineage semantics.
    ///
    /// Returns `Ok(())` if the ownership fields are consistent,
    /// or a descriptive error string if validation fails.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_structured().map_err(|e| e.to_string())
    }

    /// Validates ownership DTO lineage semantics and returns a structured
    /// [`OwnershipError`] on failure, suitable for cross-repo comparison.
    ///
    /// The structured error distinguishes missing fields, empty fields, and
    /// invalid mode/field transitions, and surfaces the field and ownership
    /// mode involved.
    pub fn validate_structured(&self) -> Result<(), OwnershipError> {
        // Promotion mode requires a non-empty promoted_from lineage reference
        if self.ownership_mode == Some(OwnershipMode::Promotion) {
            match &self.promoted_from {
                None => {
                    return Err(OwnershipError::missing_field("promoted_from", "promotion"));
                }
                Some(v) if v.trim().is_empty() => {
                    return Err(OwnershipError::empty_field(
                        "promoted_from",
                        Some("promotion"),
                    ));
                }
                _ => { /* lineage reference present and non-empty */ }
            }
        }

        // Mirror mode must not carry promotion lineage
        if self.ownership_mode == Some(OwnershipMode::Mirror) && self.promoted_from.is_some() {
            return Err(OwnershipError::invalid_transition("promoted_from", "mirror"));
        }

        // Mirror mode requires backend_kind to identify the upstream source
        if self.ownership_mode == Some(OwnershipMode::Mirror) && self.backend_kind.is_none() {
            return Err(OwnershipError::missing_field("backend_kind", "mirror"));
        }

        // created_from when set must carry non-empty lineage
        if let Some(ref v) = self.created_from {
            if v.trim().is_empty() {
                return Err(OwnershipError::empty_field("created_from", None));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_ownership_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_mirror_ownership_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Workflow),
            created_from: None,
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_promotion_with_promoted_from_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: Some("task-abc-123".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_promotion_without_promoted_from_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: None,
        };
        let err = ownership.validate().unwrap_err();
        assert!(err.contains("promoted_from"), "error: {err}");
    }

    #[test]
    fn test_canonical_with_promoted_from_is_valid() {
        // Per meta-agent baseline: canonical mode may carry promoted_from
        // for historical lineage tracking (e.g. LOCAL_CANONICAL_DERIVED_PROMOTED)
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: None,
            created_from: None,
            promoted_from: Some("task-abc-123".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_all_fields_none_is_valid() {
        let ownership = Ownership {
            ownership_mode: None,
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_serde_rejects_unknown_ownership_mode() {
        let result: Result<Ownership, _> = serde_json::from_str(r#"{"ownership_mode":"unknown"}"#);
        assert!(
            result.is_err(),
            "expected deserialization error for unknown ownership_mode"
        );
    }

    #[test]
    fn test_serde_rejects_unknown_backend_kind() {
        let result: Result<Ownership, _> = serde_json::from_str(r#"{"backend_kind":"unknown"}"#);
        assert!(
            result.is_err(),
            "expected deserialization error for unknown backend_kind"
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: Some("parent-graph".to_string()),
            promoted_from: Some("task-xyz".to_string()),
        };
        let json = serde_json::to_string(&ownership).unwrap();
        let deserialized: Ownership = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.ownership_mode, Some(OwnershipMode::Promotion));
        assert_eq!(deserialized.backend_kind, Some(BackendKind::Task));
        assert_eq!(deserialized.created_from, Some("parent-graph".to_string()));
        assert_eq!(deserialized.promoted_from, Some("task-xyz".to_string()));
    }

    #[test]
    fn test_serde_defaults_missing_fields() {
        let ownership: Ownership = serde_json::from_str("{}").unwrap();
        assert!(ownership.ownership_mode.is_none());
        assert!(ownership.backend_kind.is_none());
        assert!(ownership.created_from.is_none());
        assert!(ownership.promoted_from.is_none());
    }

    #[test]
    fn test_missing_optional_fields_in_partial_json() {
        let ownership: Ownership =
            serde_json::from_str(r#"{"ownership_mode":"canonical"}"#).unwrap();
        assert_eq!(ownership.ownership_mode, Some(OwnershipMode::Canonical));
        assert!(ownership.backend_kind.is_none());
        assert!(ownership.created_from.is_none());
        assert!(ownership.promoted_from.is_none());
    }

    #[test]
    fn test_promotion_with_empty_promoted_from_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: Some("".to_string()),
        };
        let err = ownership.validate().unwrap_err();
        assert!(
            err.contains("promoted_from must not be empty"),
            "error: {err}",
        );
    }

    #[test]
    fn test_promotion_with_whitespace_promoted_from_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: Some("   ".to_string()),
        };
        let err = ownership.validate().unwrap_err();
        assert!(
            err.contains("promoted_from must not be empty"),
            "error: {err}",
        );
    }

    // -- mirror-specific validation ---------------------------------------

    #[test]
    fn test_mirror_without_backend_kind_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        let err = ownership.validate().unwrap_err();
        assert!(
            err.contains("backend_kind is required when ownership_mode is mirror"),
            "error: {err}",
        );
    }

    #[test]
    fn test_mirror_with_promoted_from_is_invalid_transition() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Workflow),
            created_from: None,
            promoted_from: Some("task-abc".to_string()),
        };
        let err = ownership.validate().unwrap_err();
        assert!(
            err.contains("invalid mirror-mode transition"),
            "error: {err}",
        );
    }

    #[test]
    fn test_mirror_with_valid_backend_kind_passes() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_mirror_with_workflow_backend_kind_passes() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Workflow),
            created_from: None,
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_canonical_without_backend_kind_passes() {
        // Canonical ownership does not require backend_kind — only mirror does
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    // -- created_from lineage tests ---------------------------------------

    #[test]
    fn test_created_from_with_canonical_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: Some(BackendKind::Task),
            created_from: Some("parent-integration-graph".to_string()),
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_created_from_with_mirror_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Workflow),
            created_from: Some("parent-integration-graph".to_string()),
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_created_from_with_promotion_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: Some("parent-integration-graph".to_string()),
            promoted_from: Some("task-abc".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_created_from_empty_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: None,
            created_from: Some("".to_string()),
            promoted_from: None,
        };
        let err = ownership.validate().unwrap_err();
        assert!(err.contains("created_from"), "error: {err}");
    }

    #[test]
    fn test_created_from_whitespace_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: None,
            created_from: Some("   ".to_string()),
            promoted_from: None,
        };
        let err = ownership.validate().unwrap_err();
        assert!(err.contains("created_from"), "error: {err}");
    }

    #[test]
    fn test_full_lineage_baseline_case() {
        // Corresponds to meta-agent LOCAL_CANONICAL_DERIVED_PROMOTED:
        // canonical mode with both created_from and promoted_from set
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: Some(BackendKind::Task),
            created_from: Some("parent-integration-graph".to_string()),
            promoted_from: Some("/state/backends/legacy-sqlite".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_promotion_derived_transfer_baseline_case() {
        // Corresponds to meta-agent PROMOTION_DERIVED_TRANSFER:
        // promotion mode with both created_from and promoted_from set
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: Some("parent-integration-graph".to_string()),
            promoted_from: Some("/state/backends/legacy-sqlite".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    // -- validate_structured error detail tests -----------------------------

    #[test]
    fn test_validate_structured_missing_promoted_from() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: None,
        };
        let err = ownership.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "missing_field");
        assert_eq!(err.field.as_deref(), Some("promoted_from"));
        assert_eq!(err.ownership_mode.as_deref(), Some("promotion"));
    }

    #[test]
    fn test_validate_structured_empty_promoted_from() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: Some("".to_string()),
        };
        let err = ownership.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "empty_field");
        assert_eq!(err.field.as_deref(), Some("promoted_from"));
        assert_eq!(err.ownership_mode.as_deref(), Some("promotion"));
    }

    #[test]
    fn test_validate_structured_missing_backend_kind() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        let err = ownership.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "missing_field");
        assert_eq!(err.field.as_deref(), Some("backend_kind"));
        assert_eq!(err.ownership_mode.as_deref(), Some("mirror"));
    }

    #[test]
    fn test_validate_structured_invalid_transition() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Workflow),
            created_from: None,
            promoted_from: Some("task-abc".to_string()),
        };
        let err = ownership.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "invalid_transition");
        assert_eq!(err.field.as_deref(), Some("promoted_from"));
        assert_eq!(err.ownership_mode.as_deref(), Some("mirror"));
    }

    #[test]
    fn test_validate_structured_empty_created_from() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: None,
            created_from: Some("".to_string()),
            promoted_from: None,
        };
        let err = ownership.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "empty_field");
        assert_eq!(err.field.as_deref(), Some("created_from"));
        assert!(err.ownership_mode.is_none());
    }

    #[test]
    fn test_validate_structured_display_matches_validate() {
        // Verify that OwnershipError Display output matches the string
        // produced by the original validate() for every failure case.
        let cases: Vec<Ownership> = vec![
            Ownership {
                ownership_mode: Some(OwnershipMode::Promotion),
                backend_kind: Some(BackendKind::Task),
                created_from: None,
                promoted_from: None,
            },
            Ownership {
                ownership_mode: Some(OwnershipMode::Promotion),
                backend_kind: Some(BackendKind::Task),
                created_from: None,
                promoted_from: Some("".to_string()),
            },
            Ownership {
                ownership_mode: Some(OwnershipMode::Mirror),
                backend_kind: None,
                created_from: None,
                promoted_from: None,
            },
            Ownership {
                ownership_mode: Some(OwnershipMode::Mirror),
                backend_kind: Some(BackendKind::Task),
                created_from: None,
                promoted_from: Some("x".to_string()),
            },
            Ownership {
                ownership_mode: Some(OwnershipMode::Canonical),
                backend_kind: None,
                created_from: Some("".to_string()),
                promoted_from: None,
            },
        ];
        for own in cases {
            let str_err = own.validate().unwrap_err();
            let structured = own.validate_structured().unwrap_err();
            assert_eq!(str_err, structured.to_string(), "Display must match validate() for {own:?}");
        }
    }
}
