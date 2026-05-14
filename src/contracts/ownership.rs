use serde::{Deserialize, Deserializer, Serialize};
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
///
/// Matches the shared Phase 2B ownership vocabulary so that every
/// participant observes the same set of states regardless of the
/// originating domain's internal representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipMode {
    /// This backend is the single source of truth for the graph.
    LocalCanonical,
    /// Authority lives on a remote backend; this local copy is read-only
    /// or a staging area.
    RemoteCanonical,
    /// Task is a mirror of an upstream task from another backend.
    Mirror,
    /// Task was promoted from a subtask or inner graph node.
    Promotion,
}

impl OwnershipMode {
    /// Returns true for modes that are canonical (local or remote).
    ///
    /// Canonical modes may carry `promoted_from` for historical lineage
    /// tracking, and do not require `backend_kind`.
    pub fn is_canonical(self) -> bool {
        matches!(self, Self::LocalCanonical | Self::RemoteCanonical)
    }
}

/// Supported backend kinds for ownership tracking.
///
/// Matches the shared Phase 2B backend vocabulary so that every
/// participant names backends with the same identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// JSON file backend
    Json,
    /// Local LimeNet instance
    LocalLimenet,
    /// Remote LimeNet instance
    RemoteLimenet,
    /// SQLite database backend
    Sqlite,
    /// PostgreSQL database backend
    Postgres,
}

/// Custom deserializer for `Option<BackendKind>` that maps unknown backend_kind
/// strings to `None` instead of failing deserialization.
///
/// GAP-OWN-01: Python's BackendKind includes `json`, `local_limenet`,
/// `remote_limenet`, `sqlite`, and `postgres` — none of which map to
/// Rust's `Task` or `Workflow` variants.  Rather than rejecting the entire
/// `Ownership` record, we silently drop the unknown backend_kind so that
/// `ownership_mode`, `promoted_from`, and `created_from` can still be
/// validated across the repo boundary.
fn deserialize_lenient_backend_kind<'de, D>(deserializer: D) -> Result<Option<BackendKind>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt_str: Option<String> = Option::deserialize(deserializer)?;
    Ok(opt_str.and_then(|s| match s.as_str() {
        "task" => Some(BackendKind::Task),
        "workflow" => Some(BackendKind::Workflow),
        _ => None,
    }))
}

/// Custom deserializer for `Option<String>` that normalizes empty and
/// whitespace-only strings to `None`.
///
/// GAP-OWN-03: Python emits `"promoted_from": ""` for mirror records
/// (semantically "no promotion lineage").  Without this normalizer, Rust
/// serde maps `""` to `Some("")`, which triggers the mirror
/// `invalid_transition` guardrail — a false positive that rejects
/// semantically valid Python mirror fixtures.
fn deserialize_empty_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt_str: Option<String> = Option::deserialize(deserializer)?;
    Ok(opt_str.filter(|s| !s.trim().is_empty()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ownership {
    /// The ownership mode for this task (canonical, mirror, or promotion)
    #[serde(default)]
    pub ownership_mode: Option<OwnershipMode>,

    /// The kind of backend system this task belongs to
    #[serde(default, deserialize_with = "deserialize_lenient_backend_kind")]
    pub backend_kind: Option<BackendKind>,

    /// Graph or task identifier this ownership was derived from
    /// (empty when the graph is original).
    /// Normalizes empty/whitespace strings to `None` (GAP-OWN-03).
    #[serde(default, deserialize_with = "deserialize_empty_as_none")]
    pub created_from: Option<String>,

    /// Reference to the source this task was promoted from.
    /// Normalizes empty/whitespace strings to `None` (GAP-OWN-03).
    #[serde(default, deserialize_with = "deserialize_empty_as_none")]
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

        // Remote canonical mode requires backend_kind to identify the remote source
        if self.ownership_mode == Some(OwnershipMode::RemoteCanonical)
            && self.backend_kind.is_none()
        {
            return Err(OwnershipError::missing_field(
                "backend_kind",
                "remote_canonical",
            ));
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

    // -- local_canonical tests --------------------------------------------

    #[test]
    fn test_local_canonical_ownership_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::LocalCanonical),
            backend_kind: Some(BackendKind::Json),
            created_from: None,
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_local_canonical_without_backend_kind_passes() {
        // Canonical ownership does not require backend_kind
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::LocalCanonical),
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_local_canonical_with_promoted_from_is_valid() {
        // Per meta-agent baseline: local_canonical may carry promoted_from
        // for historical lineage tracking (e.g. LOCAL_CANONICAL_DERIVED_PROMOTED)
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::LocalCanonical),
            backend_kind: None,
            created_from: None,
            promoted_from: Some("task-abc-123".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    // -- remote_canonical tests -------------------------------------------

    #[test]
    fn test_remote_canonical_ownership_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::RemoteCanonical),
            backend_kind: Some(BackendKind::RemoteLimenet),
            created_from: None,
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_remote_canonical_without_backend_kind_is_invalid() {
        // Remote canonical requires backend_kind to identify the remote source
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::RemoteCanonical),
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        let err = ownership.validate().unwrap_err();
        assert!(
            err.contains("backend_kind is required when ownership_mode is remote_canonical"),
            "error: {err}",
        );
    }

    #[test]
    fn test_remote_canonical_with_promoted_from_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::RemoteCanonical),
            backend_kind: Some(BackendKind::RemoteLimenet),
            created_from: None,
            promoted_from: Some("task-abc-123".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    // -- mirror tests -----------------------------------------------------

    #[test]
    fn test_mirror_ownership_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Json),
            created_from: None,
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

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
            backend_kind: Some(BackendKind::Json),
            created_from: None,
            promoted_from: Some("task-abc".to_string()),
        };
        let err = ownership.validate().unwrap_err();
        assert!(
            err.contains("invalid mirror-mode transition"),
            "error: {err}",
        );
    }

    // -- promotion tests --------------------------------------------------

    #[test]
    fn test_promotion_with_promoted_from_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Json),
            created_from: None,
            promoted_from: Some("task-abc-123".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_promotion_without_promoted_from_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Json),
            created_from: None,
            promoted_from: None,
        };
        let err = ownership.validate().unwrap_err();
        assert!(err.contains("promoted_from"), "error: {err}");
    }

    #[test]
    fn test_promotion_with_empty_promoted_from_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Json),
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
            backend_kind: Some(BackendKind::Json),
            created_from: None,
            promoted_from: Some("   ".to_string()),
        };
        let err = ownership.validate().unwrap_err();
        assert!(
            err.contains("promoted_from must not be empty"),
            "error: {err}",
        );
    }

    // -- serde tests ------------------------------------------------------

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
    fn test_unknown_backend_kind_defaults_to_none() {
        // GAP-OWN-01: Python backend_kind values (json, local_limenet, etc.)
        // do not map to Rust BackendKind, so the lenient deserializer maps
        // unknown values to None instead of failing.
        let ownership: Ownership = serde_json::from_str(r#"{"backend_kind":"unknown"}"#).unwrap();
        assert!(
            ownership.backend_kind.is_none(),
            "unknown backend_kind must default to None per GAP-OWN-01 lenient deserializer"
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Json),
            created_from: Some("parent-graph".to_string()),
            promoted_from: Some("task-xyz".to_string()),
        };
        let json = serde_json::to_string(&ownership).unwrap();
        let deserialized: Ownership = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.ownership_mode, Some(OwnershipMode::Promotion));
        assert_eq!(deserialized.backend_kind, Some(BackendKind::Json));
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
    fn test_serde_deserializes_local_canonical() {
        let ownership: Ownership =
            serde_json::from_str(r#"{"ownership_mode":"local_canonical"}"#).unwrap();
        assert_eq!(ownership.ownership_mode, Some(OwnershipMode::LocalCanonical));
    }

    #[test]
    fn test_serde_deserializes_remote_canonical() {
        let ownership: Ownership =
            serde_json::from_str(r#"{"ownership_mode":"remote_canonical"}"#).unwrap();
        assert_eq!(ownership.ownership_mode, Some(OwnershipMode::RemoteCanonical));
    }

    #[test]
    fn test_serde_deserializes_backend_kind_json() {
        let ownership: Ownership =
            serde_json::from_str(r#"{"backend_kind":"json"}"#).unwrap();
        assert_eq!(ownership.backend_kind, Some(BackendKind::Json));
    }

    #[test]
    fn test_serde_deserializes_backend_kind_local_limenet() {
        let ownership: Ownership =
            serde_json::from_str(r#"{"backend_kind":"local_limenet"}"#).unwrap();
        assert_eq!(ownership.backend_kind, Some(BackendKind::LocalLimenet));
    }

    #[test]
    fn test_serde_deserializes_backend_kind_remote_limenet() {
        let ownership: Ownership =
            serde_json::from_str(r#"{"backend_kind":"remote_limenet"}"#).unwrap();
        assert_eq!(ownership.backend_kind, Some(BackendKind::RemoteLimenet));
    }

    #[test]
    fn test_serde_deserializes_backend_kind_sqlite() {
        let ownership: Ownership =
            serde_json::from_str(r#"{"backend_kind":"sqlite"}"#).unwrap();
        assert_eq!(ownership.backend_kind, Some(BackendKind::Sqlite));
    }

    #[test]
    fn test_serde_deserializes_backend_kind_postgres() {
        let ownership: Ownership =
            serde_json::from_str(r#"{"backend_kind":"postgres"}"#).unwrap();
        assert_eq!(ownership.backend_kind, Some(BackendKind::Postgres));
    }

    // -- created_from lineage tests ---------------------------------------

    #[test]
    fn test_created_from_with_local_canonical_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::LocalCanonical),
            backend_kind: Some(BackendKind::Json),
            created_from: Some("parent-integration-graph".to_string()),
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_created_from_with_remote_canonical_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::RemoteCanonical),
            backend_kind: Some(BackendKind::RemoteLimenet),
            created_from: Some("parent-integration-graph".to_string()),
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_created_from_with_mirror_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Json),
            created_from: Some("parent-integration-graph".to_string()),
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_created_from_with_promotion_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Json),
            created_from: Some("parent-integration-graph".to_string()),
            promoted_from: Some("task-abc".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_created_from_empty_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::LocalCanonical),
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
            ownership_mode: Some(OwnershipMode::LocalCanonical),
            backend_kind: None,
            created_from: Some("   ".to_string()),
            promoted_from: None,
        };
        let err = ownership.validate().unwrap_err();
        assert!(err.contains("created_from"), "error: {err}");
    }

    // -- baseline fixture cases -------------------------------------------

    #[test]
    fn test_local_canonical_derived_promoted_baseline_case() {
        // Corresponds to meta-agent LOCAL_CANONICAL_DERIVED_PROMOTED:
        // local_canonical mode with both created_from and promoted_from set
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::LocalCanonical),
            backend_kind: Some(BackendKind::Json),
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
            backend_kind: Some(BackendKind::Json),
            created_from: Some("parent-integration-graph".to_string()),
            promoted_from: Some("/state/backends/legacy-sqlite".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    // -- split-brain guardrail tests --------------------------------------

    #[test]
    fn test_mirror_must_not_carry_promoted_from() {
        // Split-brain guard: mirror mode + promoted_from is an invalid
        // transition because a mirror is a read-only replica, not a
        // promotion target.
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Json),
            created_from: None,
            promoted_from: Some("/state/backends/legacy-sqlite".to_string()),
        };
        let err = ownership.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "invalid_transition");
        assert_eq!(err.field.as_deref(), Some("promoted_from"));
        assert_eq!(err.ownership_mode.as_deref(), Some("mirror"));
    }

    #[test]
    fn test_promotion_requires_promoted_from() {
        // Split-brain guard: promotion mode without promoted_from is
        // invalid because promotion must carry lineage to prevent
        // implicit dual-write.
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Json),
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
    fn test_remote_canonical_requires_backend_kind() {
        // Split-brain guard: remote_canonical must identify the remote
        // source via backend_kind so authority is unambiguous.
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::RemoteCanonical),
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        let err = ownership.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "missing_field");
        assert_eq!(err.field.as_deref(), Some("backend_kind"));
        assert_eq!(err.ownership_mode.as_deref(), Some("remote_canonical"));
    }

    #[test]
    fn test_mirror_requires_backend_kind() {
        // Split-brain guard: mirror must identify the upstream source
        // via backend_kind so reads are routed correctly.
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

    // -- validate_structured error detail tests -----------------------------

    #[test]
    fn test_validate_structured_missing_promoted_from() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Json),
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
            backend_kind: Some(BackendKind::Json),
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
    fn test_validate_structured_missing_backend_kind_mirror() {
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
    fn test_validate_structured_missing_backend_kind_remote_canonical() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::RemoteCanonical),
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        let err = ownership.validate_structured().unwrap_err();
        assert_eq!(err.error, "validation_failed");
        assert_eq!(err.reason, "missing_field");
        assert_eq!(err.field.as_deref(), Some("backend_kind"));
        assert_eq!(err.ownership_mode.as_deref(), Some("remote_canonical"));
    }

    #[test]
    fn test_validate_structured_invalid_transition_mirror() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Json),
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
            ownership_mode: Some(OwnershipMode::LocalCanonical),
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
                backend_kind: Some(BackendKind::Json),
                created_from: None,
                promoted_from: None,
            },
            Ownership {
                ownership_mode: Some(OwnershipMode::Promotion),
                backend_kind: Some(BackendKind::Json),
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
                backend_kind: Some(BackendKind::Json),
                created_from: None,
                promoted_from: Some("x".to_string()),
            },
            Ownership {
                ownership_mode: Some(OwnershipMode::RemoteCanonical),
                backend_kind: None,
                created_from: None,
                promoted_from: None,
            },
            Ownership {
                ownership_mode: Some(OwnershipMode::LocalCanonical),
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

    // -- OwnershipMode::is_canonical helper --------------------------------

    #[test]
    fn test_is_canonical_for_local_and_remote() {
        assert!(OwnershipMode::LocalCanonical.is_canonical());
        assert!(OwnershipMode::RemoteCanonical.is_canonical());
        assert!(!OwnershipMode::Mirror.is_canonical());
        assert!(!OwnershipMode::Promotion.is_canonical());
    }
}
