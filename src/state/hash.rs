use serde_json::Value;
use sha2::{Digest, Sha256};

/// Normalize a JSON value for stable canonical serialization.
///
/// Object keys are sorted lexicographically so that semantically
/// identical payloads always serialize to the same byte sequence.
fn normalize_json_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            for key in keys {
                sorted.insert(key.clone(), normalize_json_value(&map[key]));
            }
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(normalize_json_value).collect()),
        other => other.clone(),
    }
}

/// Build the canonical JSON representation of a graph task state.
///
/// The digest input covers `graph_id`, `task_id`, `task_order`, and the
/// normalized `task_data`. Volatile audit fields such as `updated_at` are
/// excluded by design because they are not part of the logical task state.
pub fn canonical_serialize_graph_task(
    graph_id: &str,
    task_id: &str,
    task_order: i32,
    task_data: &Value,
) -> String {
    let normalized_data = normalize_json_value(task_data);
    let mut canonical = serde_json::Map::new();
    canonical.insert("graph_id".to_string(), Value::String(graph_id.to_string()));
    canonical.insert("task_id".to_string(), Value::String(task_id.to_string()));
    canonical.insert(
        "task_order".to_string(),
        Value::Number(task_order.into()),
    );
    canonical.insert("task_data".to_string(), normalized_data);
    serde_json::to_string(&canonical).expect("canonical serialization must succeed")
}

/// Compute the SHA-256 integrity hash for a graph task state.
///
/// Returns `(hash_algorithm, state_hash)` where `hash_algorithm` is always
/// `"sha-256"` and `state_hash` is the lowercase hex-encoded digest.
pub fn compute_graph_task_hash(
    graph_id: &str,
    task_id: &str,
    task_order: i32,
    task_data: &Value,
) -> (String, String) {
    let canonical = canonical_serialize_graph_task(graph_id, task_id, task_order, task_data);
    let digest = Sha256::digest(canonical);
    let hex = digest.iter().map(|b| format!("{:02x}", b)).collect();
    ("sha-256".to_string(), hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_sorts_object_keys() {
        let input: Value = serde_json::from_str(r#"{"z":1,"a":2}"#).unwrap();
        let normalized = normalize_json_value(&input);
        let json = serde_json::to_string(&normalized).unwrap();
        assert_eq!(json, r#"{"a":2,"z":1}"#);
    }

    #[test]
    fn test_normalize_sorts_nested_keys() {
        let input: Value =
            serde_json::from_str(r#"{"outer":{"z":1,"a":2}}"#).unwrap();
        let normalized = normalize_json_value(&input);
        let json = serde_json::to_string(&normalized).unwrap();
        assert_eq!(json, r#"{"outer":{"a":2,"z":1}}"#);
    }

    #[test]
    fn test_canonical_serialization_excludes_updated_at() {
        let task_data = serde_json::json!({"status": "pending"});
        let canonical = canonical_serialize_graph_task("g1", "t1", 0, &task_data);
        assert!(canonical.contains("graph_id"));
        assert!(canonical.contains("task_id"));
        assert!(canonical.contains("task_order"));
        assert!(canonical.contains("task_data"));
        assert!(!canonical.contains("updated_at"));
    }

    #[test]
    fn test_hash_is_deterministic() {
        let task_data = serde_json::json!({"status": "pending", "priority": 100});
        let (algo1, hash1) = compute_graph_task_hash("g1", "t1", 0, &task_data);
        let (algo2, hash2) = compute_graph_task_hash("g1", "t1", 0, &task_data);
        assert_eq!(algo1, "sha-256");
        assert_eq!(algo1, algo2);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn test_hash_changes_with_different_payload() {
        let task_data1 = serde_json::json!({"status": "pending"});
        let task_data2 = serde_json::json!({"status": "complete"});
        let (_, hash1) = compute_graph_task_hash("g1", "t1", 0, &task_data1);
        let (_, hash2) = compute_graph_task_hash("g1", "t1", 0, &task_data2);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_changes_with_different_order() {
        let task_data = serde_json::json!({"status": "pending"});
        let (_, hash1) = compute_graph_task_hash("g1", "t1", 0, &task_data);
        let (_, hash2) = compute_graph_task_hash("g1", "t1", 1, &task_data);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_ignores_json_formatting() {
        let raw1 = r#"{"b":2,"a":1}"#;
        let raw2 = r#"{"a":1,"b":2}"#;
        let v1: Value = serde_json::from_str(raw1).unwrap();
        let v2: Value = serde_json::from_str(raw2).unwrap();
        let (_, hash1) = compute_graph_task_hash("g1", "t1", 0, &v1);
        let (_, hash2) = compute_graph_task_hash("g1", "t1", 0, &v2);
        assert_eq!(hash1, hash2);
    }
}
