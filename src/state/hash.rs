use serde_json::Value;
use sha2::{Digest, Sha256};

/// Well-known volatile audit field names that are excluded from the canonical
/// hash input so that repeated reads of the same logical payload return the
/// same digest across writes and reloads.
const VOLATILE_AUDIT_FIELDS: &[&str] = &[
    "updated_at",
    "created_at",
    "modified_at",
    "timestamp",
    "hash_algorithm",
    "state_hash",
];

/// Compute a stable SHA-256 digest for a graph task state object.
///
/// The canonical input covers:
/// - `graph_id`
/// - `task_id`
/// - `task_order` (graph-scoped ordering)
/// - the normalized `task_data` payload with volatile audit fields stripped
///
/// Returns `(hash_algorithm, state_hash)` where `hash_algorithm` is always
/// `"sha256"` and `state_hash` is a lowercase hex string.
pub fn compute_graph_task_state_hash(
    graph_id: &str,
    task_id: &str,
    task_order: i32,
    task_data: &Value,
) -> (String, String) {
    let normalized_payload = strip_volatile_fields(normalize_value(task_data));

    let canonical = serde_json::json!({
        "graph_id": graph_id,
        "task_id": task_id,
        "task_order": task_order,
        "task_data": normalized_payload,
    });

    let canonical_bytes = serde_json::to_vec(&canonical).expect("canonical JSON must serialize");
    let hash = Sha256::digest(&canonical_bytes);
    let hash_hex = hash.iter().map(|b| format!("{:02x}", b)).collect::<String>();

    ("sha256".to_string(), hash_hex)
}

/// Inject `hash_algorithm` and `state_hash` into a `task_data` JSON value.
///
/// If `task_data` is a JSON object, the fields are added to it.
/// Otherwise the value is returned unchanged.
pub fn enrich_task_with_hash(
    graph_id: &str,
    task_id: &str,
    task_order: i32,
    mut task_data: Value,
) -> Value {
    if let Some(obj) = task_data.as_object_mut() {
        let (algorithm, hash) = compute_graph_task_state_hash(graph_id, task_id, task_order, &Value::Object(obj.clone()));
        obj.insert("hash_algorithm".to_string(), Value::String(algorithm));
        obj.insert("state_hash".to_string(), Value::String(hash));
    }
    task_data
}

/// Recursively strip well-known volatile audit fields from a JSON value.
fn strip_volatile_fields(value: Value) -> Value {
    match value {
        Value::Object(mut map) => {
            for field in VOLATILE_AUDIT_FIELDS {
                map.remove(*field);
            }
            let cleaned: serde_json::Map<String, Value> = map
                .into_iter()
                .map(|(k, v)| (k, strip_volatile_fields(v)))
                .collect();
            Value::Object(cleaned)
        }
        Value::Array(arr) => {
            Value::Array(arr.into_iter().map(strip_volatile_fields).collect())
        }
        other => other,
    }
}

/// Produce a normalized JSON value with stable ordering.
///
/// - Object keys are sorted lexicographically.
/// - Arrays are preserved in order.
/// - Scalar values are left as-is.
fn normalize_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let ordered: serde_json::Map<String, Value> = keys
                .into_iter()
                .map(|k| (k.clone(), normalize_value(&map[k])))
                .collect();
            Value::Object(ordered)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(normalize_value).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_payload_same_hash() {
        let payload = serde_json::json!({
            "task_id": "t1",
            "status": "pending",
            "priority": 10,
        });

        let (algo1, hash1) = compute_graph_task_state_hash("g1", "t1", 0, &payload);
        let (algo2, hash2) = compute_graph_task_state_hash("g1", "t1", 0, &payload);

        assert_eq!(algo1, "sha256");
        assert_eq!(algo1, algo2);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_graph_id_different_hash() {
        let payload = serde_json::json!({ "status": "pending" });

        let (_, hash1) = compute_graph_task_state_hash("g1", "t1", 0, &payload);
        let (_, hash2) = compute_graph_task_state_hash("g2", "t1", 0, &payload);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_different_task_id_different_hash() {
        let payload = serde_json::json!({ "status": "pending" });

        let (_, hash1) = compute_graph_task_state_hash("g1", "t1", 0, &payload);
        let (_, hash2) = compute_graph_task_state_hash("g1", "t2", 0, &payload);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_different_task_order_different_hash() {
        let payload = serde_json::json!({ "status": "pending" });

        let (_, hash1) = compute_graph_task_state_hash("g1", "t1", 0, &payload);
        let (_, hash2) = compute_graph_task_state_hash("g1", "t1", 1, &payload);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_different_payload_different_hash() {
        let payload1 = serde_json::json!({ "status": "pending" });
        let payload2 = serde_json::json!({ "status": "complete" });

        let (_, hash1) = compute_graph_task_state_hash("g1", "t1", 0, &payload1);
        let (_, hash2) = compute_graph_task_state_hash("g1", "t1", 0, &payload2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_volatile_fields_excluded_from_hash() {
        let payload_with = serde_json::json!({
            "status": "pending",
            "updated_at": "2024-01-01T00:00:00Z",
        });
        let payload_without = serde_json::json!({
            "status": "pending",
        });

        let (_, hash_with) = compute_graph_task_state_hash("g1", "t1", 0, &payload_with);
        let (_, hash_without) = compute_graph_task_state_hash("g1", "t1", 0, &payload_without);

        assert_eq!(hash_with, hash_without);
    }

    #[test]
    fn test_key_order_does_not_affect_hash() {
        let payload_a = serde_json::json!({ "a": 1, "b": 2 });
        let payload_b = serde_json::json!({ "b": 2, "a": 1 });

        let (_, hash_a) = compute_graph_task_state_hash("g1", "t1", 0, &payload_a);
        let (_, hash_b) = compute_graph_task_state_hash("g1", "t1", 0, &payload_b);

        assert_eq!(hash_a, hash_b);
    }

    #[test]
    fn test_enrich_injects_fields() {
        let payload = serde_json::json!({ "status": "pending" });
        let enriched = enrich_task_with_hash("g1", "t1", 0, payload);

        assert_eq!(enriched["hash_algorithm"], "sha256");
        assert!(enriched["state_hash"].as_str().unwrap().len() == 64);
    }

    #[test]
    fn test_enrich_non_object_passthrough() {
        let payload = Value::String("plain".to_string());
        let enriched = enrich_task_with_hash("g1", "t1", 0, payload.clone());

        assert_eq!(enriched, payload);
    }

    #[test]
    fn test_nested_volatile_fields_stripped() {
        let payload = serde_json::json!({
            "status": "pending",
            "meta": {
                "updated_at": "2024-01-01T00:00:00Z",
                "label": "important",
            },
        });
        let payload_clean = serde_json::json!({
            "status": "pending",
            "meta": {
                "label": "important",
            },
        });

        let (_, hash1) = compute_graph_task_state_hash("g1", "t1", 0, &payload);
        let (_, hash2) = compute_graph_task_state_hash("g1", "t1", 0, &payload_clean);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_fields_stripped_from_input() {
        let payload = serde_json::json!({
            "status": "pending",
            "hash_algorithm": "sha256",
            "state_hash": "abc123",
        });
        let payload_clean = serde_json::json!({
            "status": "pending",
        });

        let (_, hash1) = compute_graph_task_state_hash("g1", "t1", 0, &payload);
        let (_, hash2) = compute_graph_task_state_hash("g1", "t1", 0, &payload_clean);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_enrich_roundtrip_stable() {
        // Simulate: client reads enriched task, writes it back, reads again.
        let payload = serde_json::json!({ "status": "pending" });
        let enriched = enrich_task_with_hash("g1", "t1", 0, payload.clone());

        // Re-hash the enriched value as if the client wrote it back
        let (_, hash_roundtrip) =
            compute_graph_task_state_hash("g1", "t1", 0, &enriched);
        let (_, hash_original) =
            compute_graph_task_state_hash("g1", "t1", 0, &payload);

        assert_eq!(hash_roundtrip, hash_original);
    }
}
