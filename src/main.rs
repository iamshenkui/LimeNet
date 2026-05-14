pub mod contracts;
pub mod state;

use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use limenet::contracts::{
    ClaimRequest, DelegationContract, DeliveryPackage, DeliveryStatus, EvidenceRollup,
    HeartbeatRequest, Ownership,
};
use limenet::state::{
    BackoffAwakener, BatchError, BatchTaskInput, DependencyResolver, HeartbeatError, LeaseReaper,
    SubmitError, SubmitRequest, TaskRepository,
};

#[derive(Clone)]
struct AppState {
    pool: sqlx::PgPool,
}

async fn create_tasks_batch(
    State(state): State<Arc<AppState>>,
    Json(tasks): Json<Vec<BatchTaskInput>>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.insert_batch(tasks).await {
        Ok(result) => (StatusCode::CREATED, Json(result)).into_response(),
        Err(BatchError::CycleDetected(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response(),
        Err(BatchError::SqlxError(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn claim_task(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ClaimRequest>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(15);
    match repo.claim_ready(&request.agent_id, expires_at).await {
        Ok(Some(task)) => (StatusCode::OK, Json(task)).into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn heartbeat_task(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<uuid::Uuid>,
    Json(request): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.renew_lease(task_id, &request.agent_id).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(HeartbeatError::TaskNotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(HeartbeatError::AgentMismatch) => StatusCode::CONFLICT.into_response(),
        Err(HeartbeatError::SqlxError(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn submit_task(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<uuid::Uuid>,
    Json(request): Json<SubmitRequest>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo
        .submit(
            task_id,
            &request.agent_id,
            &request.result_summary,
            request.files_changed,
        )
        .await
    {
        Ok(result) => (StatusCode::ACCEPTED, Json(result)).into_response(),
        Err(SubmitError::TaskNotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(SubmitError::StatusMismatch) => StatusCode::CONFLICT.into_response(),
        Err(SubmitError::AgentMismatch) => StatusCode::FORBIDDEN.into_response(),
        Err(SubmitError::SqlxError(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Pure delegation ingest logic, free of AppState / database dependencies.
///
/// Returns all received delegation fields in the response so that consumers
/// can verify correct receipt without field loss or semantic drift.
pub fn delegation_ingest_logic(contract: DelegationContract) -> impl IntoResponse {
    match contract.validate() {
        Ok(()) => {
            let response = serde_json::json!({
                "status": "accepted",
                "delegation_id": contract.delegation_id,
                "upstream_work_request_id": contract.upstream_work_request_id,
                "upstream_task_id": contract.upstream_task_id,
                "upstream_backend_id": contract.upstream_backend_id,
                "downstream_domain_kind": contract.downstream_domain_kind,
                "downstream_graph_id": contract.downstream_graph_id,
            });
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(msg) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response(),
    }
}

async fn delegation_ingest(
    State(_state): State<Arc<AppState>>,
    Json(contract): Json<DelegationContract>,
) -> impl IntoResponse {
    delegation_ingest_logic(contract)
}

/// Pure delivery-package ingest logic, free of AppState / database dependencies.
///
/// Returns all received delivery-package fields in the response so that consumers
/// can verify correct receipt without field loss or semantic drift.
pub fn delivery_package_ingest_logic(pkg: DeliveryPackage) -> impl IntoResponse {
    match pkg.validate() {
        Ok(()) => {
            let response = serde_json::json!({
                "status": "accepted",
                "delivery_id": pkg.delivery_id,
                "source_domain": pkg.source_domain,
                "target_domain": pkg.target_domain,
                "package_type": pkg.package_type,
                "delegation_contract_id": pkg.delegation_contract_id,
                "ownership_ref": pkg.ownership_ref,
                "payload_summary": pkg.payload_summary,
                "artifact_count": pkg.artifact_count,
            });
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(msg) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response(),
    }
}

/// Pure evidence-rollup ingest logic, free of AppState / database dependencies.
///
/// Returns all received evidence-rollup fields in the response so that consumers
/// can verify correct receipt without field loss or semantic drift.
pub fn evidence_rollup_ingest_logic(rollup: EvidenceRollup) -> impl IntoResponse {
    match rollup.validate() {
        Ok(()) => {
            let response = serde_json::json!({
                "status": "accepted",
                "evidence_rollup_id": rollup.evidence_rollup_id,
                "summary": rollup.summary,
                "artifact_refs": rollup.artifact_refs,
                "source_domain": rollup.source_domain,
                "evidence_count": rollup.evidence_count,
                "delivery_id": rollup.delivery_id,
            });
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(msg) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response(),
    }
}

/// Pure delivery-status ingest logic, free of AppState / database dependencies.
///
/// Parses the incoming string via [`DeliveryStatus::try_from`] so that unknown
/// values produce a structured `{"error": "..."}` response rather than a
/// generic deserialisation error at the HTTP boundary.
pub fn delivery_status_ingest_logic(status_str: &str) -> impl IntoResponse + use<> {
    match DeliveryStatus::try_from(status_str) {
        Ok(status) => {
            let response = serde_json::json!({
                "status": "accepted",
                "delivery_status": status.as_str(),
            });
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(msg) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response(),
    }
}

/// Pure ownership ingest logic, free of AppState / database dependencies.
///
/// Returns all received ownership fields in the response so that consumers
/// can verify correct receipt without field loss or semantic drift.
pub fn ownership_ingest_logic(ownership: Ownership) -> impl IntoResponse {
    match ownership.validate() {
        Ok(()) => {
            let mut response = serde_json::json!({
                "status": "accepted",
                "ownership_mode": ownership.ownership_mode,
                "backend_kind": ownership.backend_kind,
            });
            if let Some(ref v) = ownership.created_from {
                response["created_from"] = serde_json::json!(v);
            }
            if let Some(ref v) = ownership.promoted_from {
                response["promoted_from"] = serde_json::json!(v);
            }
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(msg) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response(),
    }
}

async fn delivery_package_ingest(
    State(_state): State<Arc<AppState>>,
    Json(pkg): Json<DeliveryPackage>,
) -> impl IntoResponse {
    delivery_package_ingest_logic(pkg)
}

async fn evidence_rollup_ingest(
    State(_state): State<Arc<AppState>>,
    Json(rollup): Json<EvidenceRollup>,
) -> impl IntoResponse {
    evidence_rollup_ingest_logic(rollup)
}

async fn delivery_status_ingest(
    State(_state): State<Arc<AppState>>,
    Json(status_str): Json<String>,
) -> impl IntoResponse {
    delivery_status_ingest_logic(&status_str)
}

async fn ownership_ingest(
    State(_state): State<Arc<AppState>>,
    Json(ownership): Json<Ownership>,
) -> impl IntoResponse {
    ownership_ingest_logic(ownership)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://chenhui@localhost:5432/postgres".to_string());

    let pool = sqlx::PgPool::connect(&database_url).await?;

    let notify = Arc::new(Notify::new());
    let resolver = DependencyResolver::new(&pool, Arc::clone(&notify));
    tokio::spawn(async move {
        resolver.run().await;
    });

    let reaper = LeaseReaper::new(&pool);
    tokio::spawn(async move {
        reaper.run().await;
    });

    let awakener = BackoffAwakener::new(&pool);
    tokio::spawn(async move {
        awakener.run().await;
    });

    let state = Arc::new(AppState { pool });

    let app = Router::new()
        .route("/api/v1/tasks/batch", post(create_tasks_batch))
        .route("/api/v1/tasks/claim", post(claim_task))
        .route("/api/v1/tasks/{task_id}/heartbeat", post(heartbeat_task))
        .route("/api/v1/tasks/{task_id}/submit", post(submit_task))
        .route("/api/v1/delegations/ingest", post(delegation_ingest))
        .route("/api/v1/deliveries/package", post(delivery_package_ingest))
        .route("/api/v1/deliveries/evidence", post(evidence_rollup_ingest))
        .route("/api/v1/deliveries/status", post(delivery_status_ingest))
        .route("/api/v1/ownership/ingest", post(ownership_ingest))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    println!("LimeNet task orchestrator starting on 0.0.0.0:3000...");

    axum::serve(listener, app).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Focused unit tests for delegation_ingest_logic
// ---------------------------------------------------------------------------
// These tests exercise the pure ingest function directly, without any
// database / AppState dependency, so they never imply remote canonical
// takeover and can run without a PostgreSQL server.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Response;
    use http_body_util::BodyExt;
    use limenet::contracts::{BackendKind, OwnershipMode, PackageType, ReviewSurface};

    /// Convert a `Response<Body>` into a JSON value for assertion convenience.
    async fn body_to_json(response: Response<Body>) -> serde_json::Value {
        let collected = response
            .into_body()
            .collect()
            .await
            .expect("body collection should succeed");
        serde_json::from_slice(&collected.to_bytes()).unwrap()
    }

    // -- successful ingest ---------------------------------------------------

    #[tokio::test]
    async fn test_full_delegation_returns_accepted() {
        let contract = DelegationContract {
            delegation_id: Some("del-001".into()),
            upstream_work_request_id: Some("wr-001".into()),
            upstream_task_id: Some("task-001".into()),
            upstream_backend_id: Some("backend-alpha".into()),
            downstream_domain_kind: "graph".into(),
            downstream_graph_id: Some("g-001".into()),
        };
        let response = delegation_ingest_logic(contract).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["delegation_id"], "del-001");
    }

    #[tokio::test]
    async fn test_minimal_delegation_returns_accepted() {
        let contract = DelegationContract {
            delegation_id: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "mesh".into(),
            downstream_graph_id: None,
        };
        let response = delegation_ingest_logic(contract).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn test_delegation_with_only_downstream_graph_returns_accepted() {
        let contract = DelegationContract {
            delegation_id: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "graph".into(),
            downstream_graph_id: Some("g-002".into()),
        };
        let response = delegation_ingest_logic(contract).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    // -- failed ingest (validation errors) ---------------------------------

    #[tokio::test]
    async fn test_missing_upstream_backend_id_returns_bad_request() {
        let contract = DelegationContract {
            delegation_id: Some("del-002".into()),
            upstream_work_request_id: Some("wr-001".into()),
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "graph".into(),
            downstream_graph_id: None,
        };
        let response = delegation_ingest_logic(contract).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("upstream_backend_id"),
            "expected error about missing upstream_backend_id, got: {:?}",
            body["error"]
        );
    }

    #[tokio::test]
    async fn test_missing_upstream_work_request_id_returns_bad_request() {
        let contract = DelegationContract {
            delegation_id: Some("del-003".into()),
            upstream_work_request_id: None,
            upstream_task_id: Some("task-001".into()),
            upstream_backend_id: Some("backend-alpha".into()),
            downstream_domain_kind: "graph".into(),
            downstream_graph_id: None,
        };
        let response = delegation_ingest_logic(contract).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("upstream_work_request_id"),
            "expected error about missing upstream_work_request_id, got: {:?}",
            body["error"]
        );
    }

    #[tokio::test]
    async fn test_empty_domain_kind_with_graph_id_returns_bad_request() {
        let contract = DelegationContract {
            delegation_id: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "".into(),
            downstream_graph_id: Some("g-001".into()),
        };
        let response = delegation_ingest_logic(contract).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("downstream_domain_kind"),
            "expected error about empty downstream_domain_kind, got: {:?}",
            body["error"]
        );
    }

    // -------------------------------------------------------------------
    // Fixture-baseline roundtrip tests — delegation
    // -------------------------------------------------------------------

    /// Serialize → deserialize roundtrip preserves all fields for every baseline record.
    #[test]
    fn test_all_baseline_delegation_records_serde_roundtrip() {
        for record in limenet::fixtures::DelegationFixtures::all_baseline_records() {
            let json = serde_json::to_string(&record).expect("fixture must serialize");
            let rt: DelegationContract =
                serde_json::from_str(&json).expect("fixture must deserialize");
            assert_eq!(rt.delegation_id, record.delegation_id);
            assert_eq!(rt.upstream_work_request_id, record.upstream_work_request_id);
            assert_eq!(rt.upstream_task_id, record.upstream_task_id);
            assert_eq!(rt.upstream_backend_id, record.upstream_backend_id);
            assert_eq!(rt.downstream_domain_kind, record.downstream_domain_kind);
            assert_eq!(rt.downstream_graph_id, record.downstream_graph_id);
        }
    }

    /// Every baseline delegation record is accepted by the ingest handler.
    #[tokio::test]
    async fn test_all_baseline_delegation_records_accepted_by_ingest() {
        for record in limenet::fixtures::DelegationFixtures::all_baseline_records() {
            let response = delegation_ingest_logic(record).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "all baseline delegation fixtures must be accepted"
            );
        }
    }

    /// Ingest response echoes back all fields for every baseline record.
    #[tokio::test]
    async fn test_baseline_delegation_ingest_response_echoes_all_fields() {
        use limenet::fixtures::DelegationFixtures;

        for (case_name, record) in DelegationFixtures::records_by_case() {
            let response = delegation_ingest_logic(record.clone()).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "case {case_name} must be accepted"
            );
            let body = body_to_json(response).await;
            assert_eq!(body["status"], "accepted", "case {case_name}");

            assert_eq!(
                body["delegation_id"],
                serde_json::to_value(&record.delegation_id).unwrap(),
                "delegation_id mismatch for {case_name}"
            );

            assert_eq!(
                body["upstream_work_request_id"],
                serde_json::to_value(&record.upstream_work_request_id).unwrap(),
                "upstream_work_request_id mismatch for {case_name}"
            );

            assert_eq!(
                body["upstream_task_id"],
                serde_json::to_value(&record.upstream_task_id).unwrap(),
                "upstream_task_id mismatch for {case_name}"
            );

            assert_eq!(
                body["upstream_backend_id"],
                serde_json::to_value(&record.upstream_backend_id).unwrap(),
                "upstream_backend_id mismatch for {case_name}"
            );

            assert_eq!(
                body["downstream_domain_kind"],
                record.downstream_domain_kind,
                "downstream_domain_kind mismatch for {case_name}"
            );

            assert_eq!(
                body["downstream_graph_id"],
                serde_json::to_value(&record.downstream_graph_id).unwrap(),
                "downstream_graph_id mismatch for {case_name}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Focused unit tests for delivery_package_ingest_logic
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_minimal_delivery_package_returns_accepted() {
        let pkg = DeliveryPackage {
            delivery_id: None,
            source_domain: None,
            target_domain: None,
            package_type: PackageType::Standard,
            delegation_contract_id: None,
            ownership_ref: None,
            payload_summary: None,
            artifact_count: None,
        };
        let response = delivery_package_ingest_logic(pkg).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["delivery_id"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn test_full_delivery_package_returns_accepted_with_id() {
        let pkg = DeliveryPackage {
            delivery_id: Some("del-001".into()),
            source_domain: Some("task-graph".into()),
            target_domain: Some("human-review".into()),
            package_type: PackageType::Expedited,
            delegation_contract_id: Some("dc-001".into()),
            ownership_ref: Some("own-001".into()),
            payload_summary: Some("Review batch for sprint-42".into()),
            artifact_count: Some(3),
        };
        let response = delivery_package_ingest_logic(pkg).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["delivery_id"], "del-001");
    }

    #[tokio::test]
    async fn test_delivery_package_zero_artifact_count_returns_bad_request() {
        let pkg = DeliveryPackage {
            delivery_id: None,
            source_domain: None,
            target_domain: None,
            package_type: PackageType::Batch,
            delegation_contract_id: None,
            ownership_ref: None,
            payload_summary: None,
            artifact_count: Some(0),
        };
        let response = delivery_package_ingest_logic(pkg).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert!(
            body["error"].as_str().unwrap().contains("artifact_count"),
            "expected error about artifact_count, got: {:?}",
            body["error"]
        );
    }

    #[tokio::test]
    async fn test_delivery_package_no_local_subtask_details_required() {
        // Only package_type is required; no local subtask queue details
        // from either the source or target domain are needed.
        for ptype in &[
            PackageType::Standard,
            PackageType::Expedited,
            PackageType::Batch,
        ] {
            let pkg = DeliveryPackage {
                delivery_id: None,
                source_domain: None,
                target_domain: None,
                package_type: *ptype,
                delegation_contract_id: None,
                ownership_ref: None,
                payload_summary: None,
                artifact_count: None,
            };
            let response = delivery_package_ingest_logic(pkg).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "expected ACCEPTED for package_type={ptype:?}",
            );
        }
    }

    // -----------------------------------------------------------------------
    // Focused unit tests for delivery_status_ingest_logic
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_delivery_status_accepted() {
        let response = delivery_status_ingest_logic("accepted").into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["delivery_status"], "accepted");
    }

    #[tokio::test]
    async fn test_delivery_status_needs_revision() {
        let response = delivery_status_ingest_logic("needs_revision").into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["delivery_status"], "needs_revision");
    }

    #[tokio::test]
    async fn test_delivery_status_all_variants_return_accepted() {
        for (input, expected) in &[
            ("proposed", "proposed"),
            ("accepted", "accepted"),
            ("needs_revision", "needs_revision"),
            ("rejected", "rejected"),
            ("superseded", "superseded"),
        ] {
            let response = delivery_status_ingest_logic(input).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "expected ACCEPTED for {expected}",
            );
            let body = body_to_json(response).await;
            assert_eq!(body["status"], "accepted");
            assert_eq!(body["delivery_status"], *expected);
        }
    }

    #[tokio::test]
    async fn test_delivery_status_unknown_value_returns_unprocessable() {
        let response = delivery_status_ingest_logic("unknown").into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_to_json(response).await;
        assert!(
            body["error"].as_str().unwrap().contains("unknown"),
            "expected error about unknown status, got: {:?}",
            body["error"]
        );
    }

    #[tokio::test]
    async fn test_delivery_status_empty_string_returns_unprocessable() {
        let response = delivery_status_ingest_logic("").into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_to_json(response).await;
        assert!(
            body["error"].as_str().unwrap().contains("unknown delivery status"),
            "expected error about unknown status, got: {:?}",
            body["error"]
        );
    }

    #[tokio::test]
    async fn test_delivery_status_case_variation_returns_unprocessable() {
        let response = delivery_status_ingest_logic("Proposed").into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_to_json(response).await;
        assert!(
            body["error"].as_str().unwrap().contains("unknown delivery status"),
            "expected error about unknown status, got: {:?}",
            body["error"]
        );
    }

    #[tokio::test]
    async fn test_delivery_status_serde_error_consistent_with_try_from() {
        // The custom Deserialize impl should produce an error whose
        // Display representation contains the unrecognised value,
        // matching the TryFrom error format.
        let serde_err =
            serde_json::from_str::<DeliveryStatus>(r#""bogus_value""#).unwrap_err();
        let err_msg = serde_err.to_string();
        assert!(
            err_msg.contains("bogus_value"),
            "custom Deserialize error should include the unrecognised value: {err_msg}",
        );
    }

    #[tokio::test]
    async fn test_delivery_status_serde_error_inside_review_surface() {
        // Unknown status inside ReviewSurface must also fail with a
        // consistent error that includes the unrecognised value.
        let serde_err = serde_json::from_str::<ReviewSurface>(
            r#"{"status":"invalid_status"}"#,
        )
        .unwrap_err();
        let err_msg = serde_err.to_string();
        assert!(
            err_msg.contains("invalid_status"),
            "custom Deserialize error inside ReviewSurface should include \
             the unrecognised value: {err_msg}",
        );
    }

    // -------------------------------------------------------------------
    // Focused unit tests for ownership_ingest_logic
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_mirror_ownership_ingest_returns_accepted() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Workflow),
            created_from: None,
            promoted_from: None,
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["ownership_mode"], "mirror");
    }

    #[tokio::test]
    async fn test_canonical_ownership_ingest_returns_accepted() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: None,
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["ownership_mode"], "canonical");
    }

    #[tokio::test]
    async fn test_promotion_ownership_ingest_returns_accepted() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: Some("task-001".into()),
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["ownership_mode"], "promotion");
        assert_eq!(body["promoted_from"], "task-001");
    }

    #[tokio::test]
    async fn test_promotion_ownership_ingest_includes_lineage() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: Some("graph-node-42".into()),
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["promoted_from"], "graph-node-42");
        // Canonical/mirror should NOT include promoted_from
    }

    #[tokio::test]
    async fn test_canonical_ownership_ingest_excludes_lineage() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Canonical),
            backend_kind: Some(BackendKind::Task),
            created_from: None,
            promoted_from: None,
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["ownership_mode"], "canonical");
        assert!(body.get("promoted_from").is_none());
    }

    #[tokio::test]
    async fn test_mirror_without_backend_kind_returns_bad_request() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("backend_kind is required when ownership_mode is mirror"),
            "error: {:?}",
            body["error"],
        );
    }

    #[tokio::test]
    async fn test_mirror_with_promoted_from_returns_bad_request() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Workflow),
            created_from: None,
            promoted_from: Some("task-abc".into()),
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .contains("invalid mirror-mode transition"),
            "error: {:?}",
            body["error"],
        );
    }

    #[tokio::test]
    async fn test_empty_ownership_ingest_returns_accepted() {
        let ownership = Ownership {
            ownership_mode: None,
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["ownership_mode"], serde_json::Value::Null);
    }

    // -------------------------------------------------------------------
    // Fixture-baseline roundtrip tests — verify that every baseline
    // ownership record survives serialization, deserialization, and
    // ingest without field loss or semantic drift.
    // -------------------------------------------------------------------

    /// Serialize → deserialize roundtrip preserves all fields for every baseline record.
    #[test]
    fn test_all_baseline_records_serde_roundtrip() {
        for record in limenet::fixtures::OwnershipFixtures::all_baseline_records() {
            let json = serde_json::to_string(&record).expect("fixture must serialize");
            let rt: Ownership = serde_json::from_str(&json).expect("fixture must deserialize");
            assert_eq!(rt.ownership_mode, record.ownership_mode);
            assert_eq!(rt.backend_kind, record.backend_kind);
            assert_eq!(rt.created_from, record.created_from);
            assert_eq!(rt.promoted_from, record.promoted_from);
        }
    }

    /// Every baseline record is accepted by the ingest handler.
    #[tokio::test]
    async fn test_all_baseline_records_accepted_by_ingest() {
        for record in limenet::fixtures::OwnershipFixtures::all_baseline_records() {
            let response = ownership_ingest_logic(record).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "all baseline fixtures must be accepted"
            );
        }
    }

    /// Ingest response echoes back all non-None fields for every baseline record.
    #[tokio::test]
    async fn test_baseline_ingest_response_echoes_all_fields() {
        use limenet::fixtures::OwnershipFixtures;

        for (case_name, record) in OwnershipFixtures::records_by_lineage_case() {
            let response = ownership_ingest_logic(record.clone()).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "case {case_name} must be accepted"
            );
            let body = body_to_json(response).await;
            assert_eq!(body["status"], "accepted", "case {case_name}");

            // ownership_mode must match the serialized form
            let expected_mode = serde_json::to_value(&record.ownership_mode).unwrap();
            assert_eq!(
                body["ownership_mode"], expected_mode,
                "ownership_mode mismatch for {case_name}"
            );

            // backend_kind must match
            let expected_backend = serde_json::to_value(&record.backend_kind).unwrap();
            assert_eq!(
                body["backend_kind"], expected_backend,
                "backend_kind mismatch for {case_name}"
            );

            // created_from echoed when present
            match &record.created_from {
                Some(v) => assert_eq!(
                    body["created_from"].as_str(),
                    Some(v.as_str()),
                    "created_from mismatch for {case_name}"
                ),
                None => assert!(
                    body.get("created_from").is_none(),
                    "created_from must be absent for {case_name}"
                ),
            }

            // promoted_from echoed when present (regardless of mode)
            match &record.promoted_from {
                Some(v) => assert_eq!(
                    body["promoted_from"].as_str(),
                    Some(v.as_str()),
                    "promoted_from mismatch for {case_name}"
                ),
                None => assert!(
                    body.get("promoted_from").is_none(),
                    "promoted_from must be absent for {case_name}"
                ),
            }
        }
    }

    // -------------------------------------------------------------------
    // Fixture-baseline roundtrip tests — delivery packages
    // -------------------------------------------------------------------

    /// Serialize → deserialize roundtrip preserves all fields for every baseline package.
    #[test]
    fn test_all_baseline_delivery_package_records_serde_roundtrip() {
        for record in limenet::fixtures::DeliveryFixtures::all_baseline_packages() {
            let json = serde_json::to_string(&record).expect("fixture must serialize");
            let rt: DeliveryPackage =
                serde_json::from_str(&json).expect("fixture must deserialize");
            assert_eq!(rt.delivery_id, record.delivery_id);
            assert_eq!(rt.source_domain, record.source_domain);
            assert_eq!(rt.target_domain, record.target_domain);
            assert_eq!(rt.package_type, record.package_type);
            assert_eq!(rt.delegation_contract_id, record.delegation_contract_id);
            assert_eq!(rt.ownership_ref, record.ownership_ref);
            assert_eq!(rt.payload_summary, record.payload_summary);
            assert_eq!(rt.artifact_count, record.artifact_count);
        }
    }

    /// Every baseline delivery package is accepted by the ingest handler.
    #[tokio::test]
    async fn test_all_baseline_delivery_package_records_accepted_by_ingest() {
        for record in limenet::fixtures::DeliveryFixtures::all_baseline_packages() {
            let response = delivery_package_ingest_logic(record).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "all baseline delivery package fixtures must be accepted"
            );
        }
    }

    /// Ingest response echoes back all fields for every baseline package.
    #[tokio::test]
    async fn test_baseline_delivery_package_ingest_response_echoes_all_fields() {
        use limenet::fixtures::DeliveryFixtures;

        for (case_name, record) in DeliveryFixtures::packages_by_case() {
            let response = delivery_package_ingest_logic(record.clone()).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "case {case_name} must be accepted"
            );
            let body = body_to_json(response).await;
            assert_eq!(body["status"], "accepted", "case {case_name}");

            assert_eq!(
                body["delivery_id"],
                serde_json::to_value(&record.delivery_id).unwrap(),
                "delivery_id mismatch for {case_name}"
            );

            assert_eq!(
                body["source_domain"],
                serde_json::to_value(&record.source_domain).unwrap(),
                "source_domain mismatch for {case_name}"
            );

            assert_eq!(
                body["target_domain"],
                serde_json::to_value(&record.target_domain).unwrap(),
                "target_domain mismatch for {case_name}"
            );

            assert_eq!(
                body["package_type"],
                serde_json::to_value(record.package_type).unwrap(),
                "package_type mismatch for {case_name}"
            );

            assert_eq!(
                body["delegation_contract_id"],
                serde_json::to_value(&record.delegation_contract_id).unwrap(),
                "delegation_contract_id mismatch for {case_name}"
            );

            assert_eq!(
                body["ownership_ref"],
                serde_json::to_value(&record.ownership_ref).unwrap(),
                "ownership_ref mismatch for {case_name}"
            );

            assert_eq!(
                body["payload_summary"],
                serde_json::to_value(&record.payload_summary).unwrap(),
                "payload_summary mismatch for {case_name}"
            );

            assert_eq!(
                body["artifact_count"],
                serde_json::to_value(record.artifact_count).unwrap(),
                "artifact_count mismatch for {case_name}"
            );
        }
    }

    // -------------------------------------------------------------------
    // Fixture-baseline roundtrip tests — evidence rollups
    // -------------------------------------------------------------------

    /// Serialize → deserialize roundtrip preserves all fields for every baseline rollup.
    #[test]
    fn test_all_baseline_evidence_rollup_records_serde_roundtrip() {
        for record in limenet::fixtures::DeliveryFixtures::all_baseline_rollups() {
            let json = serde_json::to_string(&record).expect("fixture must serialize");
            let rt: EvidenceRollup =
                serde_json::from_str(&json).expect("fixture must deserialize");
            assert_eq!(rt.evidence_rollup_id, record.evidence_rollup_id);
            assert_eq!(rt.summary, record.summary);
            assert_eq!(rt.artifact_refs, record.artifact_refs);
            assert_eq!(rt.source_domain, record.source_domain);
            assert_eq!(rt.evidence_count, record.evidence_count);
            assert_eq!(rt.delivery_id, record.delivery_id);
        }
    }

    /// Every baseline evidence rollup is accepted by the ingest handler.
    #[tokio::test]
    async fn test_all_baseline_evidence_rollup_records_accepted_by_ingest() {
        for record in limenet::fixtures::DeliveryFixtures::all_baseline_rollups() {
            let response = evidence_rollup_ingest_logic(record).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "all baseline evidence rollup fixtures must be accepted"
            );
        }
    }

    /// Ingest response echoes back all fields for every baseline rollup.
    #[tokio::test]
    async fn test_baseline_evidence_rollup_ingest_response_echoes_all_fields() {
        use limenet::fixtures::DeliveryFixtures;

        for (case_name, record) in DeliveryFixtures::rollups_by_case() {
            let response = evidence_rollup_ingest_logic(record.clone()).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "case {case_name} must be accepted"
            );
            let body = body_to_json(response).await;
            assert_eq!(body["status"], "accepted", "case {case_name}");

            assert_eq!(
                body["evidence_rollup_id"],
                serde_json::to_value(&record.evidence_rollup_id).unwrap(),
                "evidence_rollup_id mismatch for {case_name}"
            );

            assert_eq!(
                body["summary"],
                serde_json::to_value(&record.summary).unwrap(),
                "summary mismatch for {case_name}"
            );

            assert_eq!(
                body["artifact_refs"],
                serde_json::to_value(&record.artifact_refs).unwrap(),
                "artifact_refs mismatch for {case_name}"
            );

            assert_eq!(
                body["source_domain"],
                serde_json::to_value(&record.source_domain).unwrap(),
                "source_domain mismatch for {case_name}"
            );

            assert_eq!(
                body["evidence_count"],
                serde_json::to_value(record.evidence_count).unwrap(),
                "evidence_count mismatch for {case_name}"
            );

            assert_eq!(
                body["delivery_id"],
                serde_json::to_value(&record.delivery_id).unwrap(),
                "delivery_id mismatch for {case_name}"
            );
        }
    }
}
