use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ownership {
    /// Where this task's ownership originated (e.g., "canonical", "mirror", "promotion")
    #[serde(default)]
    pub created_from: Option<String>,

    /// Reference to the source this task was promoted from
    #[serde(default)]
    pub promoted_from: Option<String>,
}
