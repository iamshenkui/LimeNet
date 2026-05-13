use serde::{Deserialize, Serialize};

/// Summary-sized evidence rollup with indirect artifact references.
///
/// Aggregates evidence artifacts by reference only — no inline artifact
/// payloads are stored. The summary field provides a coarse-grained
/// description of the evidence set, keeping the rollup lightweight
/// and suitable for cross-domain delivery surfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRollup {
    /// Unique identifier for this evidence rollup
    #[serde(default)]
    pub evidence_rollup_id: Option<String>,

    /// Coarse-grained summary of the evidence being rolled up
    #[serde(default)]
    pub summary: Option<String>,

    /// Indirect references to evidence artifacts (URLs or resource IDs)
    #[serde(default)]
    pub artifact_refs: Option<Vec<String>>,

    /// Domain or system that originated this evidence rollup
    #[serde(default)]
    pub source_domain: Option<String>,

    /// Number of evidence artifacts included in this rollup
    #[serde(default)]
    pub evidence_count: Option<u32>,

    /// Reference to the delivery package this rollup belongs to
    #[serde(default)]
    pub delivery_id: Option<String>,
}
