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
use limenet::contracts::{ClaimRequest, HeartbeatRequest};

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
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    println!("LimeNet task orchestrator starting on 0.0.0.0:3000...");

    axum::serve(listener, app).await?;

    Ok(())
}
