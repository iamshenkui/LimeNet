pub mod contracts;
pub mod state;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use limenet::state::{BackoffAwakener, BatchError, BatchTaskInput, DependencyResolver, HeartbeatError, LeaseReaper, SubmitError, SubmitRequest, TaskRepository};
use limenet::contracts::{ClaimRequest, DelegationContract, DeliveryPackage, DeliveryStatus, EvidenceRollup, HeartbeatRequest};

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
        Err(BatchError::CycleDetected(msg)) => {
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg }))).into_response()
        }
        Err(BatchError::SqlxError(e)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
                .into_response()
        }
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
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
                .into_response()
        }
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
        Err(HeartbeatError::SqlxError(e)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
                .into_response()
        }
    }
}

async fn submit_task(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(task_id): axum::extract::Path<uuid::Uuid>,
    Json(request): Json<SubmitRequest>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.submit(task_id, &request.agent_id, &request.result_summary, request.files_changed).await {
        Ok(result) => (StatusCode::ACCEPTED, Json(result)).into_response(),
        Err(SubmitError::TaskNotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(SubmitError::StatusMismatch) => StatusCode::CONFLICT.into_response(),
        Err(SubmitError::AgentMismatch) => StatusCode::FORBIDDEN.into_response(),
        Err(SubmitError::SqlxError(e)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
                .into_response()
        }
    }
}

/// Pure delegation ingest logic, free of AppState / database dependencies.
pub fn delegation_ingest_logic(contract: DelegationContract) -> impl IntoResponse {
    match contract.validate() {
        Ok(()) => {
            let response = serde_json::json!({
                "status": "accepted",
                "delegation_id": contract.delegation_id,
            });
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(msg) => {
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg }))).into_response()
        }
    }
}

async fn delegation_ingest(
    State(_state): State<Arc<AppState>>,
    Json(contract): Json<DelegationContract>,
) -> impl IntoResponse {
    delegation_ingest_logic(contract)
}

/// Pure delivery-package ingest logic, free of AppState / database dependencies.
pub fn delivery_package_ingest_logic(pkg: DeliveryPackage) -> impl IntoResponse {
    match pkg.validate() {
        Ok(()) => {
            let response = serde_json::json!({
                "status": "accepted",
                "delivery_id": pkg.delivery_id,
            });
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(msg) => {
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg }))).into_response()
        }
    }
}

/// Pure evidence-rollup ingest logic, free of AppState / database dependencies.
pub fn evidence_rollup_ingest_logic(rollup: EvidenceRollup) -> impl IntoResponse {
    match rollup.validate() {
        Ok(()) => {
            let response = serde_json::json!({
                "status": "accepted",
                "evidence_rollup_id": rollup.evidence_rollup_id,
            });
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(msg) => {
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg }))).into_response()
        }
    }
}

/// Pure delivery-status ingest logic, free of AppState / database dependencies.
pub fn delivery_status_ingest_logic(status: DeliveryStatus) -> impl IntoResponse {
    let status_str = serde_json::to_string(&status).unwrap_or_else(|_| "unknown".into());
    let response = serde_json::json!({
        "status": "accepted",
        "delivery_status": status_str.trim_matches('"'),
    });
    (StatusCode::ACCEPTED, Json(response)).into_response()
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
    Json(status): Json<DeliveryStatus>,
) -> impl IntoResponse {
    delivery_status_ingest_logic(status)
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
    use axum::http::Response;
    use axum::body::Body;
    use http_body_util::BodyExt;

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
            body["error"].as_str().unwrap().contains("upstream_backend_id"),
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
            body["error"].as_str().unwrap().contains("upstream_work_request_id"),
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
            body["error"].as_str().unwrap().contains("downstream_domain_kind"),
            "expected error about empty downstream_domain_kind, got: {:?}",
            body["error"]
        );
    }
}
