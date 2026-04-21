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

use limenet::state::{BatchError, BatchTaskInput, TaskRepository};

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://chenhui@localhost:5432/postgres".to_string());

    let pool = sqlx::PgPool::connect(&database_url).await?;

    let state = Arc::new(AppState { pool });

    let app = Router::new()
        .route("/api/v1/tasks/batch", post(create_tasks_batch))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    println!("LimeNet task orchestrator starting on 0.0.0.0:3000...");

    axum::serve(listener, app).await?;

    Ok(())
}
