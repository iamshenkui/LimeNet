//! Cross-repo guardrail acceptance slice.
//!
//! Proves two invariants when ownership and delivery artifacts cross the repo
//! boundary from the Python meta-agent into LimeNet (Rust):
//!
//! ## Guardrail 1 — mirror does not advance canonical state
//!
//! Mirror backends replicate graphs for read-only access.  When a delivery
//! proposal carries an ownership record in mirror mode, the mirror must not
//! advance canonical state — neither through promotion lineage nor through
//! the delivery path.  This acceptance slice verifies that mirror-mode
//! ownership records carry empty `promoted_from` (no lineage advancement)
//! and that the delivery proposal composite preserves mirror semantics.
//!
//! ## Guardrail 2 — promotion does not degenerate into dual-write
//!
//! Promotion-mode ownership requires an explicit `promoted_from` declaration
//! identifying the source backend.  When delivery artifacts accompany a
//! promotion-mode ownership record, the delivery path must not create implicit
//! dual-write by allowing writes on two backends simultaneously.  This slice
//! verifies that promotion records carry non-empty `promoted_from`, that
//! `promoted_from` differs from `canonical_backend_id`, and that the explicit
//! promotion intent declares distinct source and target backends.
//!
//! ## Concrete artifacts referenced
//!
//! | Artifact | Wire directory | Purpose |
//! |----------|---------------|---------|
//! | mirror-original.json | ownership_wire/ | Mirror read-only baseline |
//! | mirror-derived.json | ownership_wire/ | Derived mirror read-only |
//! | promotion-transfer.json | ownership_wire/ | Promotion with source lineage |
//! | promotion-derived-transfer.json | ownership_wire/ | Derived promotion |
//! | promotion_intent.json | ownership_wire/ | Explicit transfer intent |
//! | frozen_delivery_proposal.json | delivery_wire/ | Composite ownership+delivery |
//! | all_baseline_records.json | ownership_wire/ | Full ownership baseline |
//! | all_baseline_packages.json | delivery_wire/ | Full delivery baseline |

use std::fs;
use std::path::PathBuf;

use limenet::contracts::{BackendKind, Ownership, OwnershipMode};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn artifacts_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .join(".state")
        .join("artifacts")
}

fn read_wire_json(wire_dir: &str, name: &str) -> serde_json::Value {
    let path = artifacts_root().join(wire_dir).join(name);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"));
    serde_json::from_str(&text).expect("fixture must be valid JSON")
}

fn assert_artifact_exists(wire_dir: &str, name: &str) {
    let path = artifacts_root().join(wire_dir).join(name);
    assert!(
        path.exists(),
        "required artifact must exist: {wire_dir}/{name}"
    );
}

// ---------------------------------------------------------------------------
// Guardrail 1 — mirror does not advance canonical state
// ---------------------------------------------------------------------------

/// Mirror-original ownership record: `ownership_mode=mirror` and
/// `promoted_from=""` prove the mirror is read-only (no lineage advancement
/// toward canonical state).
#[test]
fn mirror_read_only_original_no_promotion_lineage() {
    let v = read_wire_json("ownership_wire", "mirror-original.json");
    assert_eq!(v["ownership_mode"].as_str(), Some("mirror"));
    assert_eq!(
        v["promoted_from"].as_str(),
        Some(""),
        "mirror must have empty promoted_from — read-only, no canonical advancement"
    );
    assert_eq!(v["created_from"].as_str(), Some(""));
    assert!(v["canonical_backend_id"].as_str().is_some_and(|s| !s.is_empty()));
}

/// Mirror-derived ownership record: even a derived mirror (with `created_from`)
/// must still have empty `promoted_from` — the read-only invariant holds
/// regardless of derivation lineage.
#[test]
fn mirror_read_only_derived_no_promotion_lineage() {
    let v = read_wire_json("ownership_wire", "mirror-derived.json");
    assert_eq!(v["ownership_mode"].as_str(), Some("mirror"));
    assert_eq!(
        v["promoted_from"].as_str(),
        Some(""),
        "derived mirror must still have empty promoted_from — read-only invariant is universal"
    );
    assert_eq!(
        v["created_from"].as_str(),
        Some("parent-integration-graph"),
        "derived mirror must carry created_from for derivation tracking"
    );
}

/// The composite delivery proposal carries an ownership_record in mirror mode.
/// Even when delivered across the repo boundary inside a delivery artifact,
/// the mirror must not advance canonical state.
#[test]
fn delivery_proposal_preserves_mirror_read_only() {
    let v = read_wire_json("delivery_wire", "frozen_delivery_proposal.json");
    let own = &v["ownership_record"];
    assert_eq!(
        own["ownership_mode"].as_str(),
        Some("mirror"),
        "delivery proposal ownership_record must be mirror"
    );
    assert_eq!(
        own["promoted_from"].as_str(),
        Some(""),
        "mirror inside delivery proposal must have empty promoted_from — delivery does not advance canonical state"
    );
    // The delivery_package and evidence_rollup sub-documents must be present
    // for the proposal to be a complete cross-boundary artifact.
    assert!(
        v["delivery_package"].is_object(),
        "delivery proposal must include delivery_package"
    );
    assert!(
        v["evidence_rollup"].is_object(),
        "delivery proposal must include evidence_rollup"
    );
}

/// Every mirror record in the full baseline must have empty `promoted_from`.
/// This is a bulk assert that the mirror read-only invariant holds for all
/// mirror records emitted by the Python fixture exporter.
#[test]
fn all_baseline_mirror_records_read_only() {
    let v = read_wire_json("ownership_wire", "all_baseline_records.json");
    let arr = v.as_array().expect("all_baseline_records must be a JSON array");
    let mirror_records: Vec<_> = arr
        .iter()
        .filter(|r| r["ownership_mode"].as_str() == Some("mirror"))
        .collect();
    assert!(
        !mirror_records.is_empty(),
        "baseline must include at least one mirror record"
    );
    for record in &mirror_records {
        let graph_id = record["graph_id"].as_str().unwrap_or("?");
        let pf = record["promoted_from"].as_str().unwrap_or("NOT_PRESENT");
        assert!(
            pf.is_empty(),
            "mirror record {graph_id} must have empty promoted_from (got {pf:?}) — mirror read-only invariant"
        );
    }
}

/// Tightened acceptance slice: deserialize the ownership_record embedded in
/// the frozen delivery proposal through the full `Ownership` struct and
/// verify the mirror read-only guardrail at the type level.
///
/// This test goes beyond raw JSON Value inspection — it exercises the
/// `deserialize_empty_as_none` normalizer (GAP-OWN-03 resolution),
/// `OwnershipMode` enum deserialization, and `validate_structured()`
/// enforcement.  It proves that a mirror ownership record crossing the
/// repo boundary inside a delivery proposal carries no promotion lineage
/// (`promoted_from` normalizes to `None`) and that the ownership mode
/// correctly deserializes as `Mirror`.
#[test]
fn mirror_delivery_ownership_deserializes_as_structured_mirror() {
    let v = read_wire_json("delivery_wire", "frozen_delivery_proposal.json");
    let own_json = &v["ownership_record"];

    // Deserialize the ownership_record sub-document through the full
    // Ownership struct, not just raw JSON field access.
    let own: Ownership = serde_json::from_value(own_json.clone())
        .expect("ownership_record must deserialize as Ownership");

    // Mirror mode survives the cross-repo structured deserialization
    assert_eq!(
        own.ownership_mode,
        Some(OwnershipMode::Mirror),
        "ownership_record from delivery proposal must deserialize as Mirror"
    );

    // promoted_from normalizes empty-string "" → None (GAP-OWN-03 resolved)
    assert!(
        own.promoted_from.is_none(),
        "mirror promoted_from must be None after empty-string normalization — no promotion lineage"
    );

    // created_from preserves the derivation lineage across the boundary
    assert_eq!(
        own.created_from.as_deref(),
        Some("parent-integration-graph"),
        "mirror created_from must survive cross-repo deserialization"
    );
}

/// Prove that the Rust-side `validate_structured()` guardrail correctly
/// accepts a mirror Ownership record with valid BackendKind and no
/// promotion lineage.
///
/// This test constructs the Ownership struct directly (bypassing the
/// Python fixture deserialization) to isolate the validator logic from
/// the GAP-OWN-01 BackendKind mismatch.  Together with the fixture
/// deserialization test above, it proves that the only remaining barrier
/// to end-to-end Python → Rust mirror validation is the BackendKind
/// enum gap.
#[test]
fn mirror_structured_validator_accepts_valid_mirror() {
    let own = Ownership {
        ownership_mode: Some(OwnershipMode::Mirror),
        backend_kind: Some(BackendKind::Task),
        created_from: Some("parent-integration-graph".to_string()),
        promoted_from: None,
    };
    let result = own.validate_structured();
    assert!(
        result.is_ok(),
        "valid mirror ownership must pass validate_structured(): {:?}",
        result.err()
    );
}

/// Prove that the mirror guardrail rejects a mirror Ownership record
/// that carries a real `promoted_from` lineage reference — the mirror
/// must not advance canonical state through promotion lineage.
#[test]
fn mirror_structured_validator_rejects_promotion_lineage() {
    let own = Ownership {
        ownership_mode: Some(OwnershipMode::Mirror),
        backend_kind: Some(BackendKind::Workflow),
        created_from: None,
        promoted_from: Some("/state/backends/legacy-sqlite".to_string()),
    };
    let err = own.validate_structured().unwrap_err();
    assert_eq!(err.reason, "invalid_transition");
    assert_eq!(err.field.as_deref(), Some("promoted_from"));
    assert_eq!(err.ownership_mode.as_deref(), Some("mirror"));
}

// ---------------------------------------------------------------------------
// Guardrail 2 — promotion does not degenerate into dual-write
// ---------------------------------------------------------------------------

/// Promotion-transfer ownership record: `ownership_mode=promotion` and
/// non-empty `promoted_from` prevent dual-write by declaring the source
/// backend explicitly.
#[test]
fn promotion_transfer_declares_source_backend() {
    let v = read_wire_json("ownership_wire", "promotion-transfer.json");
    assert_eq!(v["ownership_mode"].as_str(), Some("promotion"));
    let promoted_from = v["promoted_from"].as_str().unwrap_or("");
    assert!(
        !promoted_from.is_empty(),
        "promotion must declare non-empty promoted_from — dual-write prevention"
    );
    assert_eq!(
        promoted_from, "/state/backends/legacy-sqlite",
        "promoted_from must match the frozen legacy backend identifier"
    );
}

/// Promotion-derived-transfer ownership record: even a derived promotion
/// (with `created_from`) must declare `promoted_from` to prevent dual-write.
#[test]
fn promotion_derived_transfer_declares_source_backend() {
    let v = read_wire_json("ownership_wire", "promotion-derived-transfer.json");
    assert_eq!(v["ownership_mode"].as_str(), Some("promotion"));
    let promoted_from = v["promoted_from"].as_str().unwrap_or("");
    assert!(
        !promoted_from.is_empty(),
        "derived promotion must still declare promoted_from — dual-write prevention is universal"
    );
    assert_eq!(
        v["created_from"].as_str(),
        Some("parent-integration-graph"),
        "derived promotion must carry created_from for derivation tracking"
    );
}

/// Promotion `promoted_from` must differ from `canonical_backend_id`.
/// Equality would mean the promotion targets the same backend it claims to
/// leave — that is dual-write by another name.
#[test]
fn promotion_cannot_promote_from_own_backend() {
    let v = read_wire_json("ownership_wire", "promotion-transfer.json");
    let promoted_from = v["promoted_from"].as_str().unwrap();
    let canonical = v["canonical_backend_id"].as_str().unwrap();
    assert_ne!(
        promoted_from, canonical,
        "promoted_from ({promoted_from}) must differ from canonical_backend_id ({canonical}): \
         equality implies dual-write on the same backend"
    );
}

/// Promotion intent declares distinct source and target backends.
/// When source == target, promotion would be a no-op dual-write on the
/// same backend.
#[test]
fn promotion_intent_source_differs_from_target() {
    let v = read_wire_json("ownership_wire", "promotion_intent.json");
    assert_eq!(v["intent_type"].as_str(), Some("transfer"));
    let source = v["source_backend_id"].as_str().unwrap();
    let target = v["target_backend_id"].as_str().unwrap();
    assert_ne!(
        source, target,
        "promotion intent source ({source}) must differ from target ({target}): \
         equality implies dual-write on the same backend"
    );
    // Backend kinds must also differ to confirm cross-kind transfer
    assert_eq!(v["source_backend_kind"].as_str(), Some("json"));
    assert_eq!(v["target_backend_kind"].as_str(), Some("postgres"));
}

/// Every promotion record in the full baseline must have non-empty
/// `promoted_from`.  This is a bulk assert that the dual-write prevention
/// invariant holds for all promotion records emitted by Python.
#[test]
fn all_baseline_promotion_records_declare_source() {
    let v = read_wire_json("ownership_wire", "all_baseline_records.json");
    let arr = v.as_array().expect("all_baseline_records must be a JSON array");
    let promo_records: Vec<_> = arr
        .iter()
        .filter(|r| r["ownership_mode"].as_str() == Some("promotion"))
        .collect();
    assert!(
        !promo_records.is_empty(),
        "baseline must include at least one promotion record"
    );
    for record in &promo_records {
        let graph_id = record["graph_id"].as_str().unwrap_or("?");
        let pf = record["promoted_from"].as_str().unwrap_or("");
        assert!(
            !pf.is_empty(),
            "promotion record {graph_id} must declare non-empty promoted_from — dual-write prevention"
        );
    }
}

/// Tightened acceptance slice: deserialize the promotion-transfer ownership
/// record through the full `Ownership` struct and verify the promotion
/// non-dual-write guardrail at the type level.
///
/// This test goes beyond raw JSON Value inspection — it exercises the
/// `deserialize_empty_as_none` normalizer (GAP-OWN-03 resolution),
/// `OwnershipMode` enum deserialization, and `validate_structured()`
/// enforcement.  It proves that a promotion ownership record crossing the
/// repo boundary carries an explicit `promoted_from` lineage reference
/// (the source-backend declaration that prevents dual-write) and that the
/// ownership mode correctly deserializes as `Promotion`.
#[test]
fn promotion_ownership_deserializes_as_structured_promotion() {
    let v = read_wire_json("ownership_wire", "promotion-transfer.json");

    // Deserialize the promotion fixture through the full Ownership struct
    let own: Ownership = serde_json::from_value(v)
        .expect("promotion-transfer.json must deserialize as Ownership");

    // Promotion mode survives the cross-repo structured deserialization
    assert_eq!(
        own.ownership_mode,
        Some(OwnershipMode::Promotion),
        "promotion ownership_record must deserialize as Promotion"
    );

    // promoted_from carries the source-backend declaration — the key
    // dual-write prevention mechanism
    assert_eq!(
        own.promoted_from.as_deref(),
        Some("/state/backends/legacy-sqlite"),
        "promoted_from must survive cross-repo deserialization — source-backend declaration"
    );

    // backend_kind normalizes to None (GAP-OWN-01: Python "json" is unknown
    // to Rust BackendKind)
    assert!(
        own.backend_kind.is_none(),
        "backend_kind 'json' normalizes to None across repo boundary (GAP-OWN-01)"
    );

    // validate_structured() must accept the deserialized promotion record —
    // the structured guardrail confirms this is a valid promotion, not a
    // dual-write
    let result = own.validate_structured();
    assert!(
        result.is_ok(),
        "deserialized promotion must pass validate_structured(): {:?}",
        result.err()
    );
}

/// Tightened acceptance slice: deserialize the promotion-derived-transfer
/// ownership record through the full `Ownership` struct and verify that
/// promotion with both `created_from` (derivation lineage) and
/// `promoted_from` (source-backend declaration) passes validation.
///
/// A derived promotion must still carry `promoted_from` — derivation does
/// not grant implicit write capability on the source backend.
#[test]
fn promotion_derived_ownership_deserializes_as_structured_promotion() {
    let v = read_wire_json("ownership_wire", "promotion-derived-transfer.json");

    let own: Ownership = serde_json::from_value(v)
        .expect("promotion-derived-transfer.json must deserialize as Ownership");

    assert_eq!(own.ownership_mode, Some(OwnershipMode::Promotion));

    // created_from carries derivation lineage (normalized from "" to None
    // in the original fixture, but promotion-derived-transfer has a real value)
    assert_eq!(
        own.created_from.as_deref(),
        Some("parent-integration-graph"),
        "created_from must survive cross-repo deserialization"
    );

    // promoted_from carries the source-backend declaration
    assert_eq!(
        own.promoted_from.as_deref(),
        Some("/state/backends/legacy-sqlite"),
        "promoted_from must survive cross-repo deserialization"
    );

    // Validation passes: derived promotion with explicit source declaration
    // is not dual-write
    assert!(own.validate_structured().is_ok());
}

/// Prove that the Rust-side `validate_structured()` guardrail correctly
/// accepts a promotion Ownership record with a valid `promoted_from`.
///
/// This test constructs the Ownership struct directly (bypassing the
/// Python fixture deserialization) to isolate the validator logic from
/// the GAP-OWN-01 BackendKind mismatch.  Together with the fixture
/// deserialization test above, it proves that the only remaining barrier
/// to end-to-end Python → Rust promotion validation is the BackendKind
/// enum gap.
#[test]
fn promotion_structured_validator_accepts_valid_promotion() {
    let own = Ownership {
        ownership_mode: Some(OwnershipMode::Promotion),
        backend_kind: Some(BackendKind::Task),
        created_from: None,
        promoted_from: Some("/state/backends/legacy-sqlite".to_string()),
    };
    let result = own.validate_structured();
    assert!(
        result.is_ok(),
        "valid promotion ownership must pass validate_structured(): {:?}",
        result.err()
    );
}

/// Prove that the promotion guardrail rejects a promotion Ownership record
/// that lacks `promoted_from` — the absence of a source-backend declaration
/// is the dual-write degenerate case.
#[test]
fn promotion_structured_validator_rejects_missing_promoted_from() {
    let own = Ownership {
        ownership_mode: Some(OwnershipMode::Promotion),
        backend_kind: Some(BackendKind::Task),
        created_from: None,
        promoted_from: None,
    };
    let err = own.validate_structured().unwrap_err();
    assert_eq!(err.error, "validation_failed");
    assert_eq!(err.reason, "missing_field");
    assert_eq!(err.field.as_deref(), Some("promoted_from"));
    assert_eq!(err.ownership_mode.as_deref(), Some("promotion"));
}

/// Prove that the promotion guardrail rejects a promotion Ownership record
/// with an empty `promoted_from` — an empty source declaration is
/// semantically equivalent to no declaration at all.
#[test]
fn promotion_structured_validator_rejects_empty_promoted_from() {
    let own = Ownership {
        ownership_mode: Some(OwnershipMode::Promotion),
        backend_kind: Some(BackendKind::Task),
        created_from: None,
        promoted_from: Some("".to_string()),
    };
    let err = own.validate_structured().unwrap_err();
    assert_eq!(err.error, "validation_failed");
    assert_eq!(err.reason, "empty_field");
    assert_eq!(err.field.as_deref(), Some("promoted_from"));
    assert_eq!(err.ownership_mode.as_deref(), Some("promotion"));
}

// ---------------------------------------------------------------------------
// Cross-boundary delivery: ownership → delivery artifact coherence
// ---------------------------------------------------------------------------

/// The delivery proposal composite carries both an ownership_record and a
/// delivery_package.  When the ownership_record is in mirror mode, the
/// delivery_package must not be interpretable as a write-authorization
/// surface — the delivery path cannot advance canonical state through
/// the delivery_package side channel.
#[test]
fn mirror_delivery_proposal_ownership_delivery_coherent() {
    let v = read_wire_json("delivery_wire", "frozen_delivery_proposal.json");
    let own = &v["ownership_record"];
    let pkg = &v["delivery_package"];

    // Ownership is mirror → read-only
    assert_eq!(own["ownership_mode"].as_str(), Some("mirror"));
    assert_eq!(own["promoted_from"].as_str(), Some(""));

    // Delivery package carries status but does not carry ownership-mode
    // overrides or write-authorization flags — the delivery surface is
    // coarse-grained and the ownership guardrails are the sole authority.
    assert!(
        pkg["package_id"].as_str().is_some_and(|s| !s.is_empty()),
        "delivery_package must have a package_id"
    );
    // Verify that the delivery_package does NOT carry an ownership_mode
    // override that could subvert the mirror guardrail.
    assert!(
        pkg["ownership_mode"].is_null(),
        "delivery_package must not carry ownership_mode — ownership authority is separate"
    );
}

/// The delivery baseline packages carry `delivery_status` values.  These
/// status values are coarse-grained review-surface signals, not ownership
/// transitions.  A `proposed` or `accepted` delivery does not imply
/// promotion — the promotion intent and ownership record are the sole
/// promotion authorities.
#[test]
fn delivery_status_does_not_imply_promotion() {
    let v = read_wire_json("delivery_wire", "all_baseline_packages.json");
    let arr = v.as_array().expect("all_baseline_packages must be a JSON array");
    for pkg in arr {
        let status = pkg["delivery_status"].as_str().unwrap_or("?");
        let pkg_id = pkg["package_id"].as_str().unwrap_or("?");
        // No delivery status value should be "promotion" — promotion is
        // an ownership concept, not a delivery status.
        assert_ne!(
            status, "promotion",
            "package {pkg_id}: delivery_status must not be 'promotion' — promotion is an ownership concept"
        );
        // Verify status is one of the known 5 delivery status values
        assert!(
            matches!(status, "proposed" | "accepted" | "needs_revision" | "rejected" | "superseded"),
            "package {pkg_id}: unknown delivery_status {status:?}"
        );
    }
}

/// Promotion intent and delivery package are separate artifact families.
/// A promotion requires an explicit PromotionIntent; a delivery does not
/// carry implicit promotion semantics.
#[test]
fn promotion_intent_is_separate_from_delivery_artifacts() {
    // The promotion_intent.json is in ownership_wire, not delivery_wire
    let intent = read_wire_json("ownership_wire", "promotion_intent.json");
    assert_eq!(intent["graph_id"].as_str(), Some("demo-project-graph"));
    assert_eq!(intent["intent_type"].as_str(), Some("transfer"));

    // Delivery packages live in delivery_wire and carry delivery_status,
    // not promotion intent.  These are separate concerns.
    let pkg = read_wire_json("delivery_wire", "package_proposed.json");
    assert_eq!(pkg["delivery_status"].as_str(), Some("proposed"));
    // The delivery package does not carry promotion intent fields
    assert!(pkg["intent_type"].is_null());
    assert!(pkg["source_backend_id"].is_null());
    assert!(pkg["target_backend_id"].is_null());
}

// ---------------------------------------------------------------------------
// Concrete artifact existence — verifiable evidence
// ---------------------------------------------------------------------------

/// Every ownership_wire artifact required for guardrail validation must
/// exist on disk.  This smoke test prevents silent regressions where a
/// fixture is deleted or renamed.
#[test]
fn required_ownership_artifacts_exist() {
    let required = [
        "mirror-original.json",
        "mirror-derived.json",
        "promotion-transfer.json",
        "promotion-derived-transfer.json",
        "promotion_intent.json",
        "all_baseline_records.json",
    ];
    for name in &required {
        assert_artifact_exists("ownership_wire", name);
    }
}

/// Every delivery_wire artifact required for guardrail validation must
/// exist on disk.
#[test]
fn required_delivery_artifacts_exist() {
    let required = [
        "frozen_delivery_proposal.json",
        "all_baseline_packages.json",
        "package_proposed.json",
    ];
    for name in &required {
        assert_artifact_exists("delivery_wire", name);
    }
}

/// The frozen_delivery_proposal.json composite must contain all four
/// sub-documents: ownership_record, delegation_contract, delivery_package,
/// evidence_rollup.  Missing sub-documents would break the combined
/// ownership+delivery boundary assertion.
#[test]
fn frozen_delivery_proposal_has_all_sub_documents() {
    let v = read_wire_json("delivery_wire", "frozen_delivery_proposal.json");
    let keys = ["ownership_record", "delegation_contract", "delivery_package", "evidence_rollup"];
    for key in &keys {
        assert!(
            v[key].is_object(),
            "frozen_delivery_proposal.json must contain sub-document '{key}'"
        );
    }
}
