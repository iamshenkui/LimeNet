use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Pending,
    Ready,
    InProgress,
    Evaluating,
    Backoff,
    Completed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "PENDING",
            TaskStatus::Ready => "READY",
            TaskStatus::InProgress => "IN_PROGRESS",
            TaskStatus::Evaluating => "EVALUATING",
            TaskStatus::Backoff => "BACKOFF",
            TaskStatus::Completed => "COMPLETED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub instruction: String,
    #[serde(default)]
    pub context_paths: Vec<String>,
    pub validation_script: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub agent_id: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryLogic {
    pub attempt_count: i32,
    pub backoff_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaimRequest {
    pub agent_id: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatRequest {
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: Uuid,
    pub status: TaskStatus,
    pub parent_ids: Vec<Uuid>,
    pub child_ids: Vec<Uuid>,
    pub payload: Payload,
    pub lease: Option<Lease>,
    pub retry_logic: Option<RetryLogic>,
    pub topological_level: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskRow {
    pub task_id: Uuid,
    pub status: TaskStatus,
    pub parent_ids: Vec<Uuid>,
    pub child_ids: Vec<Uuid>,
    pub payload: Json<Payload>,
    pub lease: Option<Json<Lease>>,
    pub retry_logic: Option<Json<RetryLogic>>,
    pub topological_level: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<TaskRow> for Task {
    fn from(row: TaskRow) -> Self {
        Self {
            task_id: row.task_id,
            status: row.status,
            parent_ids: row.parent_ids,
            child_ids: row.child_ids,
            payload: row.payload.0,
            lease: row.lease.map(|j| j.0),
            retry_logic: row.retry_logic.map(|j| j.0),
            topological_level: row.topological_level,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<Task> for TaskRow {
    fn from(task: Task) -> Self {
        Self {
            task_id: task.task_id,
            status: task.status,
            parent_ids: task.parent_ids,
            child_ids: task.child_ids,
            payload: Json(task.payload),
            lease: task.lease.map(Json),
            retry_logic: task.retry_logic.map(Json),
            topological_level: task.topological_level,
            created_at: task.created_at,
            updated_at: task.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tasks_table_accepts_full_task() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://chenhui@localhost:5432/postgres".to_string());
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to database");

        let task_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let task = Task {
            task_id,
            status: TaskStatus::Pending,
            parent_ids: vec![parent_id],
            child_ids: vec![child_id],
            payload: Payload {
                instruction: "Test instruction".to_string(),
                context_paths: vec!["src/test.rs".to_string()],
                validation_script: Some("cargo test".to_string()),
            },
            lease: Some(Lease {
                agent_id: "test-agent".to_string(),
                expires_at: Utc::now(),
            }),
            retry_logic: Some(RetryLogic {
                attempt_count: 0,
                backoff_until: None,
            }),
            topological_level: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        sqlx::query(
            r#"
            INSERT INTO tasks (
                task_id, status, parent_ids, child_ids, payload,
                lease, retry_logic, topological_level, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(task.task_id)
        .bind(task.status.as_str())
        .bind(&task.parent_ids)
        .bind(&task.child_ids)
        .bind(sqlx::types::Json(&task.payload))
        .bind(task.lease.as_ref().map(sqlx::types::Json))
        .bind(task.retry_logic.as_ref().map(sqlx::types::Json))
        .bind(task.topological_level)
        .bind(task.created_at)
        .bind(task.updated_at)
        .execute(&pool)
        .await
        .expect("Failed to insert task");

        let row: (String,) =
            sqlx::query_as("SELECT status::text FROM tasks WHERE task_id = $1")
                .bind(task_id)
                .fetch_one(&pool)
                .await
                .expect("Failed to fetch task");

        assert_eq!(row.0, "PENDING");

        sqlx::query("DELETE FROM tasks WHERE task_id = $1")
            .bind(task_id)
            .execute(&pool)
            .await
            .expect("Failed to clean up test task");
    }
}

#[cfg(test)]
mod verify_taskrow {
    use super::*;

    #[tokio::test]
    async fn test_fetch_taskrow() {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://chenhui@localhost:5432/postgres".to_string());
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to database");

        let task_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO tasks (task_id, status, payload)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(task_id)
        .bind("PENDING")
        .bind(sqlx::types::Json(&Payload {
            instruction: "test".to_string(),
            context_paths: vec![],
            validation_script: None,
        }))
        .execute(&pool)
        .await
        .expect("Failed to insert");

        let row: TaskRow = sqlx::query_as("SELECT * FROM tasks WHERE task_id = $1")
            .bind(task_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch TaskRow");

        assert_eq!(row.status, TaskStatus::Pending);

        sqlx::query("DELETE FROM tasks WHERE task_id = $1")
            .bind(task_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
