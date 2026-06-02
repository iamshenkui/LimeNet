pub mod config;
pub mod contracts;
pub mod state;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use limenet::contracts::{
    ClaimRequest, DelegationContract, DeliveryPackage, DeliveryStatus, EvidenceRollup,
    HeartbeatRequest, Ownership,
};
use limenet::observe::{
    ObserveConfig, ObserveInstance, ObserveRepository, http_origin, resolve_observe_bind_address,
};
use limenet::state::{
    BackoffAwakener, BatchError, BatchTaskInput, CreateRunInput, DEFAULT_RUN_ID,
    DependencyResolver, GraphTaskInsertError, HeartbeatError, LeaseReaper, SubmitError,
    SubmitRequest, TaskProgressInput, TaskRepository, TaskResultInput, TaskViewError,
};

#[derive(Clone)]
struct AppState {
    pool: sqlx::PgPool,
    instance_id: String,
    database_target: String,
    bind_address: String,
}

#[derive(Debug, Deserialize)]
struct GraphTasksPayload {
    tasks: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct GraphInsertPayload {
    anchor_task_id: String,
    tasks: Vec<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct GraphNextPendingRequest {
    #[serde(default)]
    exclude_task_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RunSummaryQuery {
    #[serde(default)]
    include_next: bool,
}

async fn list_graph_tasks(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.list_graph_tasks().await {
        Ok(tasks) => (StatusCode::OK, Json(json!({ "tasks": tasks }))).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn create_run(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateRunInput>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.create_run(payload).await {
        Ok(outcome) => {
            let status = if outcome.created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (status, Json(outcome.run)).into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn list_runs(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.list_runs().await {
        Ok(runs) => (StatusCode::OK, Json(json!({ "runs": runs }))).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<uuid::Uuid>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.get_run(run_id).await {
        Ok(Some(run)) => (StatusCode::OK, Json(run)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn run_summary(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<uuid::Uuid>,
    Query(query): Query<RunSummaryQuery>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.run_exists(run_id).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    }

    match repo.run_summary(run_id, query.include_next).await {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn run_timeline(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<uuid::Uuid>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.run_exists(run_id).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    }

    match repo.run_timeline(run_id, 100).await {
        Ok(events) => (
            StatusCode::OK,
            Json(json!({ "run_id": run_id, "events": events })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn ensure_scoped_run(repo: &TaskRepository<'_>, run_id: uuid::Uuid) -> Result<(), Response> {
    match repo.run_exists(run_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(StatusCode::NOT_FOUND.into_response()),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response()),
    }
}

async fn list_graph_tasks_for_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<uuid::Uuid>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    if let Err(response) = ensure_scoped_run(&repo, run_id).await {
        return response;
    }

    match repo.list_graph_tasks_for_run(run_id).await {
        Ok(tasks) => (
            StatusCode::OK,
            Json(json!({ "run_id": run_id, "tasks": tasks })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn get_graph_task_for_run(
    State(state): State<Arc<AppState>>,
    Path((run_id, task_id)): Path<(uuid::Uuid, String)>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    if let Err(response) = ensure_scoped_run(&repo, run_id).await {
        return response;
    }

    match repo.get_graph_task_for_run(run_id, &task_id).await {
        Ok(Some(task)) => (StatusCode::OK, Json(task)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

fn task_view_error_response(err: TaskViewError) -> Response {
    match err {
        TaskViewError::NotFound => StatusCode::NOT_FOUND.into_response(),
        TaskViewError::HashMismatch => (
            StatusCode::CONFLICT,
            Json(json!({
                "error": "task_hash_mismatch",
                "message": "No task in this run matches the supplied task_hash."
            })),
        )
            .into_response(),
        TaskViewError::InvalidTaskPayload => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_task_payload" })),
        )
            .into_response(),
        TaskViewError::SqlxError(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn get_task_view_by_hash(
    State(state): State<Arc<AppState>>,
    Path((run_id, task_hash)): Path<(uuid::Uuid, String)>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    if let Err(response) = ensure_scoped_run(&repo, run_id).await {
        return response;
    }

    match repo.get_task_view_by_hash(run_id, &task_hash).await {
        Ok(Some(view)) => (StatusCode::OK, Json(view)).into_response(),
        Ok(None) => task_view_error_response(TaskViewError::NotFound),
        Err(err) => task_view_error_response(err),
    }
}

async fn append_task_progress_by_hash(
    State(state): State<Arc<AppState>>,
    Path((run_id, task_hash)): Path<(uuid::Uuid, String)>,
    Json(payload): Json<TaskProgressInput>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    if let Err(response) = ensure_scoped_run(&repo, run_id).await {
        return response;
    }

    match repo
        .append_task_progress_by_hash(run_id, &task_hash, payload)
        .await
    {
        Ok(view) => (StatusCode::OK, Json(view)).into_response(),
        Err(err) => task_view_error_response(err),
    }
}

async fn submit_task_result_by_hash(
    State(state): State<Arc<AppState>>,
    Path((run_id, task_hash)): Path<(uuid::Uuid, String)>,
    Json(payload): Json<TaskResultInput>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    if let Err(response) = ensure_scoped_run(&repo, run_id).await {
        return response;
    }

    match repo
        .submit_task_result_by_hash(run_id, &task_hash, payload)
        .await
    {
        Ok(view) => (StatusCode::OK, Json(view)).into_response(),
        Err(err) => task_view_error_response(err),
    }
}

async fn replace_graph_tasks_for_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<uuid::Uuid>,
    Json(payload): Json<GraphTasksPayload>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    if let Err(response) = ensure_scoped_run(&repo, run_id).await {
        return response;
    }

    match repo
        .replace_graph_tasks_for_run(run_id, &payload.tasks)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({}))).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn get_graph_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.get_graph_task(&task_id).await {
        Ok(Some(task)) => (StatusCode::OK, Json(task)).into_response(),
        Ok(None) => (StatusCode::OK, Json(json!({ "status": "not_found" }))).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn upsert_graph_task_for_run(
    State(state): State<Arc<AppState>>,
    Path((run_id, task_id)): Path<(uuid::Uuid, String)>,
    Json(task): Json<Value>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    if let Err(response) = ensure_scoped_run(&repo, run_id).await {
        return response;
    }

    match repo
        .upsert_graph_task_for_run(run_id, &task_id, &task)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({}))).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn insert_graph_tasks_after_for_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<uuid::Uuid>,
    Json(payload): Json<GraphInsertPayload>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    if let Err(response) = ensure_scoped_run(&repo, run_id).await {
        return response;
    }

    match repo
        .insert_graph_tasks_after_for_run(run_id, &payload.anchor_task_id, &payload.tasks)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({}))).into_response(),
        Err(GraphTaskInsertError::UnknownAnchor(task_id)) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Unknown task: {task_id}") })),
        )
            .into_response(),
        Err(GraphTaskInsertError::DuplicateTask(task_id)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": format!("Task already exists: {task_id}") })),
        )
            .into_response(),
        Err(GraphTaskInsertError::InvalidTaskPayload) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "graph task payload missing non-empty task_id" })),
        )
            .into_response(),
        Err(GraphTaskInsertError::SqlxError(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn next_pending_graph_task_for_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<uuid::Uuid>,
    request: Option<Json<GraphNextPendingRequest>>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    if let Err(response) = ensure_scoped_run(&repo, run_id).await {
        return response;
    }

    let exclude_task_ids = request
        .map(|Json(body)| body.exclude_task_ids)
        .unwrap_or_default();
    match repo
        .next_pending_graph_task_for_run(run_id, &exclude_task_ids)
        .await
    {
        Ok(Some(task)) => (
            StatusCode::OK,
            Json(json!({ "run_id": run_id, "task": task })),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::OK,
            Json(json!({ "run_id": run_id, "task": null })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn recover_graph_tasks_for_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<uuid::Uuid>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    if let Err(response) = ensure_scoped_run(&repo, run_id).await {
        return response;
    }

    match repo.recover_in_progress_graph_tasks_for_run(run_id).await {
        Ok(recovered_count) => (
            StatusCode::OK,
            Json(json!({ "run_id": run_id, "recovered_count": recovered_count })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn replace_graph_tasks(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GraphTasksPayload>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.replace_graph_tasks(&payload.tasks).await {
        Ok(()) => (StatusCode::OK, Json(json!({}))).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn upsert_graph_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(task): Json<Value>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.upsert_graph_task(&task_id, &task).await {
        Ok(()) => (StatusCode::OK, Json(json!({}))).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn insert_graph_tasks_after(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GraphInsertPayload>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo
        .insert_graph_tasks_after(&payload.anchor_task_id, &payload.tasks)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(json!({}))).into_response(),
        Err(GraphTaskInsertError::UnknownAnchor(task_id)) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Unknown task: {task_id}") })),
        )
            .into_response(),
        Err(GraphTaskInsertError::DuplicateTask(task_id)) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": format!("Task already exists: {task_id}") })),
        )
            .into_response(),
        Err(GraphTaskInsertError::InvalidTaskPayload) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "graph task payload missing non-empty task_id" })),
        )
            .into_response(),
        Err(GraphTaskInsertError::SqlxError(err)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn next_pending_graph_task(
    State(state): State<Arc<AppState>>,
    request: Option<Json<GraphNextPendingRequest>>,
) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    let exclude_task_ids = request
        .map(|Json(body)| body.exclude_task_ids)
        .unwrap_or_default();
    match repo.next_pending_graph_task(&exclude_task_ids).await {
        Ok(Some(task)) => (StatusCode::OK, Json(json!({ "task": task }))).into_response(),
        Ok(None) => (StatusCode::OK, Json(json!({}))).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn recover_graph_tasks(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let repo = TaskRepository::new(&state.pool);
    match repo.recover_in_progress_graph_tasks().await {
        Ok(recovered_count) => (
            StatusCode::OK,
            Json(json!({ "recovered_count": recovered_count })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
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
    Path(task_id): Path<uuid::Uuid>,
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
    Path(task_id): Path<uuid::Uuid>,
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
///
/// On validation failure, returns a structured JSON error object with
/// `error`, `reason`, `field`, and `anchor` fields for stable
/// cross-repo comparison and explicit identity-anchor surfacing.
pub fn delegation_ingest_logic(contract: DelegationContract) -> impl IntoResponse {
    match contract.validate_structured() {
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
        Err(err) => (StatusCode::BAD_REQUEST, Json(serde_json::json!(err))).into_response(),
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
///
/// On validation failure, returns a structured JSON error object with
/// `error`, `reason`, `field`, `value`, and `anchor` fields for stable
/// cross-repo comparison and explicit missing-anchor surfacing.
pub fn delivery_package_ingest_logic(pkg: DeliveryPackage) -> impl IntoResponse {
    match pkg.validate_structured() {
        Ok(()) => {
            let response = serde_json::json!({
                "status": "accepted",
                "package_id": pkg.package_id,
                "delivery_contract_id": pkg.delivery_contract_id,
                "result_summary": pkg.result_summary,
                "evidence_refs": pkg.evidence_refs,
                "review_surface_refs": pkg.review_surface_refs,
                "open_risks": pkg.open_risks,
                "unresolved_items": pkg.unresolved_items,
                "recommended_next_action": pkg.recommended_next_action,
                "delivery_status": pkg.delivery_status,
                "trace_context": pkg.trace_context,
            });
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(err) => (StatusCode::BAD_REQUEST, Json(serde_json::json!(err))).into_response(),
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
                "task_id": rollup.task_id,
                "summary": rollup.summary,
                "evidence_refs": rollup.evidence_refs,
                "artifact_refs": rollup.artifact_refs,
                "conclusion": rollup.conclusion,
                "trace_context": rollup.trace_context,
                "source_domain": rollup.source_domain,
                "evidence_count": rollup.evidence_count,
                "delivery_id": rollup.delivery_id,
            });
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(err) => (StatusCode::BAD_REQUEST, Json(serde_json::json!(err))).into_response(),
    }
}

/// Pure delivery-status ingest logic, free of AppState / database dependencies.
///
/// Parses the incoming string via [`DeliveryStatus::validate_status`] so that unknown
/// values produce a structured `{"error": "...", "reason": "unsupported_status", "value": "..."}`
/// response rather than a generic deserialisation error at the HTTP boundary.
pub fn delivery_status_ingest_logic(status_str: &str) -> impl IntoResponse + use<> {
    match DeliveryStatus::validate_status(status_str) {
        Ok(status) => {
            let response = serde_json::json!({
                "status": "accepted",
                "delivery_status": status.as_str(),
            });
            (StatusCode::ACCEPTED, Json(response)).into_response()
        }
        Err(err) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!(err)),
        )
            .into_response(),
    }
}

/// Pure ownership ingest logic, free of AppState / database dependencies.
///
/// Returns all received ownership fields in the response so that consumers
/// can verify correct receipt without field loss or semantic drift.
///
/// On validation failure, returns a structured JSON error object with
/// `error`, `reason`, `field`, and `ownership_mode` fields for stable
/// cross-repo comparison.
pub fn ownership_ingest_logic(ownership: Ownership) -> impl IntoResponse {
    match ownership.validate_structured() {
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
        Err(err) => (StatusCode::BAD_REQUEST, Json(serde_json::json!(err))).into_response(),
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

/// Resolve the bind address from an explicit value or the default `127.0.0.1:6987`.
///
/// The caller should pass `std::env::var("LIMENET_BIND").ok().as_deref()` when
/// resolving from the environment so the function stays pure and testable.
fn resolve_bind_address(env_value: Option<&str>) -> String {
    env_value
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "127.0.0.1:6987".to_string())
}

/// Resolve the instance identity from an explicit value or the default `default`.
///
/// The caller should pass `std::env::var("LIMENET_INSTANCE_ID").ok().as_deref()` when
/// resolving from the environment so the function stays pure and testable.
/// Whitespace is trimmed and empty values fall back to the default so that
/// accidental blank exports do not produce an unhelpful identity.
fn resolve_instance_id(env_value: Option<&str>) -> String {
    env_value
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "default".to_string())
}

/// Pure health-response logic, free of AppState / database dependencies.
///
/// Returns a minimal JSON payload that lets operators verify which LimeNet
/// instance they are talking to without leaking sensitive configuration.
pub fn health_response(
    instance_id: &str,
    database_target: &str,
    bind_address: &str,
) -> impl IntoResponse + use<> {
    let response = serde_json::json!({
        "status": "healthy",
        "instance_id": instance_id,
        "database_target": database_target,
        "bind_address": bind_address,
    });
    (StatusCode::OK, Json(response))
}

async fn health_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    health_response(
        &state.instance_id,
        &state.database_target,
        &state.bind_address,
    )
}

async fn observe_status(State(repo): State<Arc<ObserveRepository>>) -> impl IntoResponse {
    match repo.global_snapshot().await {
        Ok(snapshot) => (StatusCode::OK, Json(snapshot)).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn observe_runs(State(repo): State<Arc<ObserveRepository>>) -> impl IntoResponse {
    match repo.run_summaries(chrono::Utc::now()).await {
        Ok(runs) => (StatusCode::OK, Json(json!({ "runs": runs }))).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn observe_run(
    State(repo): State<Arc<ObserveRepository>>,
    Path(run_id): Path<uuid::Uuid>,
) -> impl IntoResponse {
    match repo.run_snapshot(run_id).await {
        Ok(Some(snapshot)) => (StatusCode::OK, Json(snapshot)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn observe_task(
    State(repo): State<Arc<ObserveRepository>>,
    Path((run_id, task_id)): Path<(uuid::Uuid, String)>,
) -> impl IntoResponse {
    match repo.task_snapshot(run_id, &task_id).await {
        Ok(Some(snapshot)) => (StatusCode::OK, Json(snapshot)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn dashboard_index() -> impl IntoResponse {
    Html(include_str!("../static/observe-dashboard.html"))
}

fn observe_json_router(repo: Arc<ObserveRepository>) -> Router {
    Router::new()
        .route("/status.json", get(observe_status))
        .route("/runs.json", get(observe_runs))
        .route("/runs/{run_id}.json", get(observe_run))
        .route("/runs/{run_id}/tasks/{task_id}.json", get(observe_task))
        .with_state(repo)
}

fn dashboard_router(repo: Arc<ObserveRepository>) -> Router {
    observe_json_router(repo).route("/", get(dashboard_index))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = limenet::config::resolve_database_url()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    let instance_id = resolve_instance_id(std::env::var("LIMENET_INSTANCE_ID").ok().as_deref());
    let database_target = limenet::config::display_database_target(&database_url);
    println!("LimeNet connecting to database {database_target}...");

    let pool = sqlx::PgPool::connect(&database_url).await?;
    TaskRepository::new(&pool)
        .ensure_run(DEFAULT_RUN_ID)
        .await?;

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

    let bind_addr = resolve_bind_address(std::env::var("LIMENET_BIND").ok().as_deref());
    let status_bind_addr = resolve_observe_bind_address(
        std::env::var("LIMENET_STATUS_BIND").ok().as_deref(),
        &bind_addr,
    );
    let dashboard_bind_addr = resolve_observe_bind_address(
        std::env::var("LIMENET_DASHBOARD_BIND").ok().as_deref(),
        &status_bind_addr,
    );

    let state = Arc::new(AppState {
        pool: pool.clone(),
        instance_id,
        database_target: database_target.clone(),
        bind_address: bind_addr.clone(),
    });

    let observe_repo = Arc::new(ObserveRepository::new(
        pool,
        ObserveConfig {
            instance: ObserveInstance {
                instance_id: state.instance_id.clone(),
                database_target,
                task_api_address: http_origin(&bind_addr),
                status_api_address: http_origin(&status_bind_addr),
                dashboard_address: http_origin(&dashboard_bind_addr),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        },
    ));

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/api/v1/runs", get(list_runs).post(create_run))
        .route("/api/v1/runs/{run_id}", get(get_run))
        .route("/api/v1/runs/{run_id}/summary", get(run_summary))
        .route("/api/v1/runs/{run_id}/timeline", get(run_timeline))
        .route(
            "/api/v1/runs/{run_id}/graph/tasks",
            get(list_graph_tasks_for_run).post(replace_graph_tasks_for_run),
        )
        .route(
            "/api/v1/runs/{run_id}/graph/tasks/insert",
            post(insert_graph_tasks_after_for_run),
        )
        .route(
            "/api/v1/runs/{run_id}/graph/tasks/next_pending",
            post(next_pending_graph_task_for_run),
        )
        .route(
            "/api/v1/runs/{run_id}/graph/tasks/recover",
            post(recover_graph_tasks_for_run),
        )
        .route(
            "/api/v1/runs/{run_id}/graph/tasks/{task_id}",
            get(get_graph_task_for_run).put(upsert_graph_task_for_run),
        )
        .route(
            "/api/v1/runs/{run_id}/task-views/{task_hash}",
            get(get_task_view_by_hash),
        )
        .route(
            "/api/v1/runs/{run_id}/task-views/{task_hash}/progress",
            post(append_task_progress_by_hash),
        )
        .route(
            "/api/v1/runs/{run_id}/task-views/{task_hash}/result",
            post(submit_task_result_by_hash),
        )
        .route(
            "/api/v1/graph/tasks",
            get(list_graph_tasks).post(replace_graph_tasks),
        )
        .route("/api/v1/graph/tasks/insert", post(insert_graph_tasks_after))
        .route(
            "/api/v1/graph/tasks/next_pending",
            post(next_pending_graph_task),
        )
        .route("/api/v1/graph/tasks/recover", post(recover_graph_tasks))
        .route(
            "/api/v1/graph/tasks/{task_id}",
            get(get_graph_task).put(upsert_graph_task),
        )
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

    let status_listener = TcpListener::bind(&status_bind_addr).await?;
    let status_app = observe_json_router(Arc::clone(&observe_repo));
    tokio::spawn(async move {
        if let Err(err) = axum::serve(status_listener, status_app).await {
            eprintln!("LimeNet status API stopped: {err}");
        }
    });
    println!("LimeNet status API starting on {status_bind_addr}...");

    let dashboard_listener = TcpListener::bind(&dashboard_bind_addr).await?;
    let dashboard_app = dashboard_router(observe_repo);
    tokio::spawn(async move {
        if let Err(err) = axum::serve(dashboard_listener, dashboard_app).await {
            eprintln!("LimeNet dashboard stopped: {err}");
        }
    });
    println!("LimeNet dashboard starting on {dashboard_bind_addr}...");

    let listener = TcpListener::bind(&bind_addr).await?;
    println!("LimeNet task orchestrator starting on {bind_addr}...");

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
    use limenet::contracts::{
        BackendKind, DeliveryStatus, EvidenceRollupPolicy, OwnershipMode, ReviewSurface,
        StatusMappingPolicy, VisibilityPolicy,
    };

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
            upstream_domain_id: Some("limenet".into()),
            downstream_domain_id: Some("local-meta-agent".into()),
            delivery_contract_id: Some("dc-001".into()),
            visibility_policy: Some(VisibilityPolicy::Shared),
            evidence_rollup_policy: Some(EvidenceRollupPolicy::Summary),
            status_mapping_policy: Some(StatusMappingPolicy::Strict),
            trace_context: None,
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
            upstream_domain_id: None,
            downstream_domain_id: None,
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
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
            upstream_domain_id: None,
            downstream_domain_id: Some("local-meta-agent".into()),
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
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
            upstream_domain_id: None,
            downstream_domain_id: None,
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
            upstream_work_request_id: Some("wr-001".into()),
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "graph".into(),
            downstream_graph_id: None,
        };
        let response = delegation_ingest_logic(contract).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert_eq!(
            body["error"], "validation_failed",
            "error: {:?}",
            body["error"]
        );
        assert_eq!(
            body["reason"], "missing_field",
            "reason: {:?}",
            body["reason"]
        );
        assert_eq!(
            body["field"], "upstream_backend_id",
            "field: {:?}",
            body["field"]
        );
        assert_eq!(body["anchor"], "upstream", "anchor: {:?}", body["anchor"]);
    }

    #[tokio::test]
    async fn test_missing_upstream_work_request_id_returns_bad_request() {
        let contract = DelegationContract {
            delegation_id: Some("del-003".into()),
            upstream_domain_id: None,
            downstream_domain_id: None,
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
            upstream_work_request_id: None,
            upstream_task_id: Some("task-001".into()),
            upstream_backend_id: Some("backend-alpha".into()),
            downstream_domain_kind: "graph".into(),
            downstream_graph_id: None,
        };
        let response = delegation_ingest_logic(contract).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert_eq!(
            body["error"], "validation_failed",
            "error: {:?}",
            body["error"]
        );
        assert_eq!(
            body["reason"], "missing_field",
            "reason: {:?}",
            body["reason"]
        );
        assert_eq!(
            body["field"], "upstream_work_request_id",
            "field: {:?}",
            body["field"]
        );
        assert_eq!(body["anchor"], "upstream", "anchor: {:?}", body["anchor"]);
    }

    #[tokio::test]
    async fn test_empty_domain_kind_with_graph_id_returns_bad_request() {
        let contract = DelegationContract {
            delegation_id: None,
            upstream_domain_id: None,
            downstream_domain_id: None,
            delivery_contract_id: None,
            visibility_policy: None,
            evidence_rollup_policy: None,
            status_mapping_policy: None,
            trace_context: None,
            upstream_work_request_id: None,
            upstream_task_id: None,
            upstream_backend_id: None,
            downstream_domain_kind: "".into(),
            downstream_graph_id: Some("g-001".into()),
        };
        let response = delegation_ingest_logic(contract).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert_eq!(
            body["error"], "validation_failed",
            "error: {:?}",
            body["error"]
        );
        assert_eq!(
            body["reason"], "empty_field",
            "reason: {:?}",
            body["reason"]
        );
        assert_eq!(
            body["field"], "downstream_domain_kind",
            "field: {:?}",
            body["field"]
        );
        assert_eq!(body["anchor"], "downstream", "anchor: {:?}", body["anchor"]);
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
                body["downstream_domain_kind"], record.downstream_domain_kind,
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
            package_id: Some("dp-001".to_string()),
            delivery_contract_id: Some("dc-001".to_string()),
            result_summary: None,
            evidence_refs: None,
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: Some(DeliveryStatus::Proposed),
            trace_context: None,
        };
        let response = delivery_package_ingest_logic(pkg).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["package_id"], "dp-001");
    }

    #[tokio::test]
    async fn test_full_delivery_package_returns_accepted_with_id() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".into()),
            delivery_contract_id: Some("dc-001".into()),
            result_summary: Some("All checks passed".into()),
            evidence_refs: Some(vec![]),
            review_surface_refs: Some(vec!["review/approved-001.md".into()]),
            open_risks: Some(vec!["Edge-case risk".into()]),
            unresolved_items: Some(vec![]),
            recommended_next_action: Some("proceed".into()),
            delivery_status: Some(DeliveryStatus::Accepted),
            trace_context: None,
        };
        let response = delivery_package_ingest_logic(pkg).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["package_id"], "dp-001");
    }

    #[tokio::test]
    async fn test_delivery_package_missing_contract_returns_bad_request() {
        let pkg = DeliveryPackage {
            package_id: Some("dp-001".to_string()),
            delivery_contract_id: None,
            result_summary: None,
            evidence_refs: None,
            review_surface_refs: None,
            open_risks: None,
            unresolved_items: None,
            recommended_next_action: None,
            delivery_status: None,
            trace_context: None,
        };
        let response = delivery_package_ingest_logic(pkg).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert_eq!(
            body["error"], "validation_failed",
            "error: {:?}",
            body["error"]
        );
        assert_eq!(
            body["reason"], "missing_anchor",
            "reason: {:?}",
            body["reason"]
        );
        assert_eq!(
            body["field"], "delivery_contract_id",
            "field: {:?}",
            body["field"]
        );
    }

    #[tokio::test]
    async fn test_delivery_package_no_local_subtask_details_required() {
        // Only package_id and delivery contract anchor are required;
        // no local subtask queue details from either the source or
        // target domain are needed.
        for status in &[
            DeliveryStatus::Proposed,
            DeliveryStatus::Accepted,
            DeliveryStatus::NeedsRevision,
        ] {
            let pkg = DeliveryPackage {
                package_id: Some("dp-001".to_string()),
                delivery_contract_id: Some("dc-001".to_string()),
                result_summary: None,
                evidence_refs: None,
                review_surface_refs: None,
                open_risks: None,
                unresolved_items: None,
                recommended_next_action: None,
                delivery_status: Some(*status),
                trace_context: None,
            };
            let response = delivery_package_ingest_logic(pkg).into_response();
            assert_eq!(
                response.status(),
                StatusCode::ACCEPTED,
                "expected ACCEPTED for delivery_status={status:?}",
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
        assert_eq!(
            body["error"], "validation_failed",
            "error: {:?}",
            body["error"]
        );
        assert_eq!(
            body["reason"], "unsupported_status",
            "reason: {:?}",
            body["reason"]
        );
        assert_eq!(body["value"], "unknown", "value: {:?}", body["value"]);
    }

    #[tokio::test]
    async fn test_delivery_status_empty_string_returns_unprocessable() {
        let response = delivery_status_ingest_logic("").into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_to_json(response).await;
        assert_eq!(
            body["error"], "validation_failed",
            "error: {:?}",
            body["error"]
        );
        assert_eq!(
            body["reason"], "unsupported_status",
            "reason: {:?}",
            body["reason"]
        );
        assert_eq!(body["value"], "", "value: {:?}", body["value"]);
    }

    #[tokio::test]
    async fn test_delivery_status_case_variation_returns_unprocessable() {
        let response = delivery_status_ingest_logic("Proposed").into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_to_json(response).await;
        assert_eq!(
            body["error"], "validation_failed",
            "error: {:?}",
            body["error"]
        );
        assert_eq!(
            body["reason"], "unsupported_status",
            "reason: {:?}",
            body["reason"]
        );
        assert_eq!(body["value"], "Proposed", "value: {:?}", body["value"]);
    }

    #[tokio::test]
    async fn test_delivery_status_serde_error_consistent_with_try_from() {
        // The custom Deserialize impl should produce an error whose
        // Display representation contains the unrecognised value,
        // matching the TryFrom error format.
        let serde_err = serde_json::from_str::<DeliveryStatus>(r#""bogus_value""#).unwrap_err();
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
        let serde_err =
            serde_json::from_str::<ReviewSurface>(r#"{"status":"invalid_status"}"#).unwrap_err();
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
            backend_kind: Some(BackendKind::Json),
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
    async fn test_local_canonical_ownership_ingest_returns_accepted() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::LocalCanonical),
            backend_kind: Some(BackendKind::Json),
            created_from: None,
            promoted_from: None,
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["ownership_mode"], "local_canonical");
    }

    #[tokio::test]
    async fn test_remote_canonical_ownership_ingest_returns_accepted() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::RemoteCanonical),
            backend_kind: Some(BackendKind::RemoteLimenet),
            created_from: None,
            promoted_from: None,
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["ownership_mode"], "remote_canonical");
    }

    #[tokio::test]
    async fn test_promotion_ownership_ingest_returns_accepted() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Promotion),
            backend_kind: Some(BackendKind::Json),
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
            backend_kind: Some(BackendKind::Json),
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
    async fn test_local_canonical_ownership_ingest_excludes_lineage() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::LocalCanonical),
            backend_kind: Some(BackendKind::Json),
            created_from: None,
            promoted_from: None,
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["ownership_mode"], "local_canonical");
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
        assert_eq!(
            body["error"], "validation_failed",
            "error: {:?}",
            body["error"]
        );
        assert_eq!(
            body["reason"], "missing_field",
            "reason: {:?}",
            body["reason"]
        );
        assert_eq!(body["field"], "backend_kind", "field: {:?}", body["field"]);
        assert_eq!(
            body["ownership_mode"], "mirror",
            "mode: {:?}",
            body["ownership_mode"]
        );
    }

    #[tokio::test]
    async fn test_mirror_with_promoted_from_returns_bad_request() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::Mirror),
            backend_kind: Some(BackendKind::Json),
            created_from: None,
            promoted_from: Some("task-abc".into()),
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert_eq!(
            body["error"], "validation_failed",
            "error: {:?}",
            body["error"]
        );
        assert_eq!(
            body["reason"], "invalid_transition",
            "reason: {:?}",
            body["reason"]
        );
        assert_eq!(body["field"], "promoted_from", "field: {:?}", body["field"]);
        assert_eq!(
            body["ownership_mode"], "mirror",
            "mode: {:?}",
            body["ownership_mode"]
        );
    }

    #[tokio::test]
    async fn test_remote_canonical_without_backend_kind_returns_bad_request() {
        let ownership = Ownership {
            ownership_mode: Some(OwnershipMode::RemoteCanonical),
            backend_kind: None,
            created_from: None,
            promoted_from: None,
        };
        let response = ownership_ingest_logic(ownership).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_to_json(response).await;
        assert_eq!(
            body["error"], "validation_failed",
            "error: {:?}",
            body["error"]
        );
        assert_eq!(
            body["reason"], "missing_field",
            "reason: {:?}",
            body["reason"]
        );
        assert_eq!(body["field"], "backend_kind", "field: {:?}", body["field"]);
        assert_eq!(
            body["ownership_mode"], "remote_canonical",
            "mode: {:?}",
            body["ownership_mode"]
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
            assert_eq!(rt.package_id, record.package_id);
            assert_eq!(rt.delivery_contract_id, record.delivery_contract_id);
            assert_eq!(rt.result_summary, record.result_summary);
            assert_eq!(rt.evidence_refs, record.evidence_refs);
            assert_eq!(rt.review_surface_refs, record.review_surface_refs);
            assert_eq!(rt.open_risks, record.open_risks);
            assert_eq!(rt.unresolved_items, record.unresolved_items);
            assert_eq!(rt.recommended_next_action, record.recommended_next_action);
            assert_eq!(rt.delivery_status, record.delivery_status);
            assert_eq!(rt.trace_context, record.trace_context);
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
                body["package_id"],
                serde_json::to_value(&record.package_id).unwrap(),
                "package_id mismatch for {case_name}"
            );

            assert_eq!(
                body["delivery_contract_id"],
                serde_json::to_value(&record.delivery_contract_id).unwrap(),
                "delivery_contract_id mismatch for {case_name}"
            );

            assert_eq!(
                body["result_summary"],
                serde_json::to_value(&record.result_summary).unwrap(),
                "result_summary mismatch for {case_name}"
            );

            assert_eq!(
                body["evidence_refs"],
                serde_json::to_value(&record.evidence_refs).unwrap(),
                "evidence_refs mismatch for {case_name}"
            );

            assert_eq!(
                body["review_surface_refs"],
                serde_json::to_value(&record.review_surface_refs).unwrap(),
                "review_surface_refs mismatch for {case_name}"
            );

            assert_eq!(
                body["open_risks"],
                serde_json::to_value(&record.open_risks).unwrap(),
                "open_risks mismatch for {case_name}"
            );

            assert_eq!(
                body["unresolved_items"],
                serde_json::to_value(&record.unresolved_items).unwrap(),
                "unresolved_items mismatch for {case_name}"
            );

            assert_eq!(
                body["recommended_next_action"],
                serde_json::to_value(&record.recommended_next_action).unwrap(),
                "recommended_next_action mismatch for {case_name}"
            );

            assert_eq!(
                body["delivery_status"],
                serde_json::to_value(&record.delivery_status).unwrap(),
                "delivery_status mismatch for {case_name}"
            );

            assert_eq!(
                body["trace_context"],
                serde_json::to_value(&record.trace_context).unwrap(),
                "trace_context mismatch for {case_name}"
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
            let rt: EvidenceRollup = serde_json::from_str(&json).expect("fixture must deserialize");
            assert_eq!(rt.evidence_rollup_id, record.evidence_rollup_id);
            assert_eq!(rt.task_id, record.task_id);
            assert_eq!(rt.summary, record.summary);
            assert_eq!(rt.evidence_refs, record.evidence_refs);
            assert_eq!(rt.artifact_refs, record.artifact_refs);
            assert_eq!(rt.conclusion, record.conclusion);
            assert_eq!(rt.trace_context, record.trace_context);
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
                body["task_id"],
                serde_json::to_value(&record.task_id).unwrap(),
                "task_id mismatch for {case_name}"
            );

            assert_eq!(
                body["summary"],
                serde_json::to_value(&record.summary).unwrap(),
                "summary mismatch for {case_name}"
            );

            assert_eq!(
                body["evidence_refs"],
                serde_json::to_value(&record.evidence_refs).unwrap(),
                "evidence_refs mismatch for {case_name}"
            );

            assert_eq!(
                body["artifact_refs"],
                serde_json::to_value(&record.artifact_refs).unwrap(),
                "artifact_refs mismatch for {case_name}"
            );

            assert_eq!(
                body["conclusion"],
                serde_json::to_value(&record.conclusion).unwrap(),
                "conclusion mismatch for {case_name}"
            );

            assert_eq!(
                body["trace_context"],
                serde_json::to_value(&record.trace_context).unwrap(),
                "trace_context mismatch for {case_name}"
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

    // -------------------------------------------------------------------
    // Bind-address resolution tests
    // -------------------------------------------------------------------

    #[test]
    fn test_resolve_bind_address_defaults_to_local_6987() {
        assert_eq!(resolve_bind_address(None), "127.0.0.1:6987");
    }

    #[test]
    fn test_resolve_bind_address_uses_custom_port() {
        assert_eq!(
            resolve_bind_address(Some("127.0.0.1:8080")),
            "127.0.0.1:8080"
        );
    }

    #[test]
    fn test_resolve_bind_address_uses_custom_host_and_port() {
        assert_eq!(
            resolve_bind_address(Some("192.168.1.10:9090")),
            "192.168.1.10:9090"
        );
    }

    #[test]
    fn test_resolve_bind_address_empty_string_falls_back_to_default() {
        assert_eq!(resolve_bind_address(Some("")), "127.0.0.1:6987");
    }

    #[test]
    fn test_resolve_bind_address_whitespace_only_falls_back_to_default() {
        assert_eq!(resolve_bind_address(Some("   ")), "127.0.0.1:6987");
    }

    // -------------------------------------------------------------------
    // Instance-identity resolution tests
    // -------------------------------------------------------------------

    #[test]
    fn test_resolve_instance_id_defaults_to_default() {
        assert_eq!(resolve_instance_id(None), "default");
    }

    #[test]
    fn test_resolve_instance_id_uses_custom_value() {
        assert_eq!(resolve_instance_id(Some("local-task")), "local-task");
    }

    #[test]
    fn test_resolve_instance_id_trims_whitespace() {
        assert_eq!(
            resolve_instance_id(Some("  shared-staging  ")),
            "shared-staging"
        );
    }

    #[test]
    fn test_resolve_instance_id_empty_string_falls_back() {
        assert_eq!(resolve_instance_id(Some("")), "default");
    }

    #[test]
    fn test_resolve_instance_id_whitespace_only_falls_back() {
        assert_eq!(resolve_instance_id(Some("   ")), "default");
    }

    // -------------------------------------------------------------------
    // Health-response tests
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_health_response_returns_ok() {
        let response = health_response(
            "local-dev",
            "localhost:5432/limenet_local",
            "127.0.0.1:3000",
        )
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_health_response_includes_identity_fields() {
        let response = health_response(
            "shared-staging",
            "db.example.com:5432/limenet_shared",
            "0.0.0.0:3001",
        )
        .into_response();
        let body = body_to_json(response).await;
        assert_eq!(body["status"], "healthy");
        assert_eq!(body["instance_id"], "shared-staging");
        assert_eq!(
            body["database_target"],
            "db.example.com:5432/limenet_shared"
        );
        assert_eq!(body["bind_address"], "0.0.0.0:3001");
    }

    #[tokio::test]
    async fn test_health_response_does_not_leak_credentials() {
        // database_target is already credential-stripped by config::display_database_target.
        // The health endpoint merely echoes what it is given, so we verify it
        // does not add any extra parsing that could expose secrets.
        let response = health_response("default", "host:5432/db", "0.0.0.0:3000").into_response();
        let body = body_to_json(response).await;
        let json_str = serde_json::to_string(&body).unwrap();
        assert!(
            !json_str.contains("password"),
            "health response must not contain raw credentials"
        );
        assert!(
            !json_str.contains("secret"),
            "health response must not contain raw credentials"
        );
        assert_eq!(body["database_target"], "host:5432/db");
    }
}
