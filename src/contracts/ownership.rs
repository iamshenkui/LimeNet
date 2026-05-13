use serde::{Deserialize, Serialize};

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
        // Promotion mode requires a non-empty promoted_from lineage reference
        if self.ownership_mode == Some(OwnershipMode::Promotion) {
            match &self.promoted_from {
                None => {
                    return Err(
                        "promoted_from is required when ownership_mode is promotion".to_string()
                    );
                }
                Some(v) if v.trim().is_empty() => {
                    return Err(
                        "promoted_from must not be empty when ownership_mode is promotion"
                            .to_string(),
                    );
                }
                _ => { /* lineage reference present and non-empty */ }
            }
        }

        // Mirror mode must not carry promotion lineage — check before the
        // generic promoted_from guard so mirror-specific errors surface first
        if self.ownership_mode == Some(OwnershipMode::Mirror) && self.promoted_from.is_some() {
            return Err(
                "invalid mirror-mode transition: promoted_from is not allowed for mirror ownership"
                    .to_string(),
            );
        }

        // promoted_from without promotion mode is a lineage inconsistency
        if self.promoted_from.is_some() && self.ownership_mode != Some(OwnershipMode::Promotion) {
            return Err("ownership_mode must be promotion when promoted_from is set".to_string());
        }

        // Mirror mode requires backend_kind to identify the upstream source
        if self.ownership_mode == Some(OwnershipMode::Mirror) && self.backend_kind.is_none() {
            return Err("backend_kind is required when ownership_mode is mirror".to_string());
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
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_mirror_ownership_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Workflow),
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_promotion_with_promoted_from_is_valid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            promoted_from: Some("task-abc-123".to_string()),
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_promotion_without_promoted_from_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            promoted_from: None,
        };
        let err = ownership.validate().unwrap_err();
        assert!(err.contains("promoted_from"), "error: {err}");
    }

    #[test]
    fn test_promoted_from_without_promotion_mode_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: None,
            promoted_from: Some("task-abc-123".to_string()),
        };
        let err = ownership.validate().unwrap_err();
        assert!(
            err.contains("ownership_mode must be promotion"),
            "error: {err}"
        );
    }

    #[test]
    fn test_all_fields_none_is_valid() {
        let ownership = Ownership {
            ownership_mode: None,
            backend_kind: None,
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
            promoted_from: Some("task-xyz".to_string()),
        };
        let json = serde_json::to_string(&ownership).unwrap();
        let deserialized: Ownership = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.ownership_mode, Some(OwnershipMode::Promotion));
        assert_eq!(deserialized.backend_kind, Some(BackendKind::Task));
        assert_eq!(deserialized.promoted_from, Some("task-xyz".to_string()));
    }

    #[test]
    fn test_serde_defaults_missing_fields() {
        let ownership: Ownership = serde_json::from_str("{}").unwrap();
        assert!(ownership.ownership_mode.is_none());
        assert!(ownership.backend_kind.is_none());
        assert!(ownership.promoted_from.is_none());
    }

    #[test]
    fn test_missing_optional_fields_in_partial_json() {
        let ownership: Ownership =
            serde_json::from_str(r#"{"ownership_mode":"canonical"}"#).unwrap();
        assert_eq!(ownership.ownership_mode, Some(OwnershipMode::Canonical));
        assert!(ownership.backend_kind.is_none());
        assert!(ownership.promoted_from.is_none());
    }

    #[test]
    fn test_promotion_with_empty_promoted_from_is_invalid() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
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
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }

    #[test]
    fn test_mirror_with_workflow_backend_kind_passes() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Workflow),
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
            promoted_from: None,
        };
        assert!(ownership.validate().is_ok());
    }
}
