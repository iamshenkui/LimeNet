use std::collections::BTreeMap;

use crate::contracts::{BackendKind, Ownership, OwnershipMode};

/// Shared lineage identifiers kept in sync with the meta-agent
/// `ownership_fixture_baseline.py` so integration checks consume
/// the same lineage cases:
///
/// - `parent-integration-graph`   — common `created_from` ancestor
/// - `/state/backends/legacy-sqlite` — common `promoted_from` source

// ---------------------------------------------------------------------------
// Mirror-mode lineage fixtures
// ---------------------------------------------------------------------------

fn mirror_original() -> Ownership {
    Ownership {
        ownership_mode: Some(OwnershipMode::Mirror),
        backend_kind: Some(BackendKind::Workflow),
        created_from: None,
        promoted_from: None,
    }
}

fn mirror_derived() -> Ownership {
    Ownership {
        ownership_mode: Some(OwnershipMode::Mirror),
        backend_kind: Some(BackendKind::Workflow),
        created_from: Some("parent-integration-graph".into()),
        promoted_from: None,
    }
}

// ---------------------------------------------------------------------------
// Promotion-mode lineage fixtures
// ---------------------------------------------------------------------------

fn promotion_transfer() -> Ownership {
    Ownership {
        ownership_mode: Some(OwnershipMode::Promotion),
        backend_kind: Some(BackendKind::Task),
        created_from: None,
        promoted_from: Some("/state/backends/legacy-sqlite".into()),
    }
}

fn promotion_derived_transfer() -> Ownership {
    Ownership {
        ownership_mode: Some(OwnershipMode::Promotion),
        backend_kind: Some(BackendKind::Task),
        created_from: Some("parent-integration-graph".into()),
        promoted_from: Some("/state/backends/legacy-sqlite".into()),
    }
}

// ---------------------------------------------------------------------------
// Canonical-mode lineage fixtures (baseline / control)
// ---------------------------------------------------------------------------

fn canonical_original() -> Ownership {
    Ownership {
        ownership_mode: Some(OwnershipMode::Canonical),
        backend_kind: Some(BackendKind::Task),
        created_from: None,
        promoted_from: None,
    }
}

fn canonical_derived() -> Ownership {
    Ownership {
        ownership_mode: Some(OwnershipMode::Canonical),
        backend_kind: Some(BackendKind::Task),
        created_from: Some("parent-integration-graph".into()),
        promoted_from: None,
    }
}

fn canonical_derived_promoted() -> Ownership {
    Ownership {
        ownership_mode: Some(OwnershipMode::Canonical),
        backend_kind: Some(BackendKind::Task),
        created_from: Some("parent-integration-graph".into()),
        promoted_from: Some("/state/backends/legacy-sqlite".into()),
    }
}

// ---------------------------------------------------------------------------
// Fixture collection
// ---------------------------------------------------------------------------

/// Accessor for the full set of ownership fixture records, keyed by
/// descriptive lineage case name for integration-test parametrisation.
///
/// The seven cases mirror the meta-agent `ownership_fixture_baseline.py`:
///
/// | Case name                     | Mode        | created_from       | promoted_from      |
/// |-------------------------------|-------------|--------------------|--------------------|
/// | `mirror-original`            | Mirror      | —                  | —                  |
/// | `mirror-derived`             | Mirror      | parent-integration-graph | —             |
/// | `promotion-transfer`         | Promotion   | —                  | /state/backends/legacy-sqlite |
/// | `promotion-derived-transfer` | Promotion   | parent-integration-graph | /state/backends/legacy-sqlite |
/// | `canonical-original`         | Canonical   | —                  | —                  |
/// | `canonical-derived`          | Canonical   | parent-integration-graph | —             |
/// | `canonical-derived-promoted` | Canonical   | parent-integration-graph | /state/backends/legacy-sqlite |
pub struct OwnershipFixtures;

impl OwnershipFixtures {
    pub fn records_by_lineage_case() -> BTreeMap<&'static str, Ownership> {
        let mut m = BTreeMap::new();
        m.insert("mirror-original", mirror_original());
        m.insert("mirror-derived", mirror_derived());
        m.insert("promotion-transfer", promotion_transfer());
        m.insert("promotion-derived-transfer", promotion_derived_transfer());
        m.insert("canonical-original", canonical_original());
        m.insert("canonical-derived", canonical_derived());
        m.insert("canonical-derived-promoted", canonical_derived_promoted());
        m
    }

    pub fn all_baseline_records() -> Vec<Ownership> {
        vec![
            mirror_original(),
            mirror_derived(),
            promotion_transfer(),
            promotion_derived_transfer(),
            canonical_original(),
            canonical_derived(),
            canonical_derived_promoted(),
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
        OwnershipFixtures::validate_baseline().expect("all baseline fixtures must validate");
    }

    #[test]
    fn test_all_seven_lineage_cases_present() {
        let cases = OwnershipFixtures::records_by_lineage_case();
        assert_eq!(cases.len(), 7);
        let expected_keys: Vec<&str> = vec![
            "canonical-derived",
            "canonical-derived-promoted",
            "canonical-original",
            "mirror-derived",
            "mirror-original",
            "promotion-derived-transfer",
            "promotion-transfer",
        ];
        let actual_keys: Vec<&str> = cases.keys().copied().collect();
        assert_eq!(actual_keys, expected_keys);
    }

    #[test]
    fn test_mirror_original_lineage() {
        let r = mirror_original();
        assert_eq!(r.ownership_mode, Some(OwnershipMode::Mirror));
        assert!(r.created_from.is_none());
        assert!(r.promoted_from.is_none());
    }

    #[test]
    fn test_mirror_derived_lineage() {
        let r = mirror_derived();
        assert_eq!(r.ownership_mode, Some(OwnershipMode::Mirror));
        assert_eq!(r.created_from.as_deref(), Some("parent-integration-graph"));
        assert!(r.promoted_from.is_none());
    }

    #[test]
    fn test_promotion_transfer_lineage() {
        let r = promotion_transfer();
        assert_eq!(r.ownership_mode, Some(OwnershipMode::Promotion));
        assert!(r.created_from.is_none());
        assert_eq!(
            r.promoted_from.as_deref(),
            Some("/state/backends/legacy-sqlite")
        );
    }

    #[test]
    fn test_promotion_derived_transfer_lineage() {
        let r = promotion_derived_transfer();
        assert_eq!(r.ownership_mode, Some(OwnershipMode::Promotion));
        assert_eq!(r.created_from.as_deref(), Some("parent-integration-graph"));
        assert_eq!(
            r.promoted_from.as_deref(),
            Some("/state/backends/legacy-sqlite")
        );
    }

    #[test]
    fn test_canonical_original_lineage() {
        let r = canonical_original();
        assert_eq!(r.ownership_mode, Some(OwnershipMode::Canonical));
        assert!(r.created_from.is_none());
        assert!(r.promoted_from.is_none());
    }

    #[test]
    fn test_canonical_derived_lineage() {
        let r = canonical_derived();
        assert_eq!(r.ownership_mode, Some(OwnershipMode::Canonical));
        assert_eq!(r.created_from.as_deref(), Some("parent-integration-graph"));
        assert!(r.promoted_from.is_none());
    }

    #[test]
    fn test_canonical_derived_promoted_lineage() {
        let r = canonical_derived_promoted();
        assert_eq!(r.ownership_mode, Some(OwnershipMode::Canonical));
        assert_eq!(r.created_from.as_deref(), Some("parent-integration-graph"));
        assert_eq!(
            r.promoted_from.as_deref(),
            Some("/state/backends/legacy-sqlite")
        );
    }
}
