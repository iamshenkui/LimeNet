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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Implementation,
    Plan,
    ArchitectureReview,
    CodeReview,
    MergeCandidate,
    MergeRepair,
}

impl TaskKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Implementation => "implementation",
            TaskKind::Plan => "plan",
            TaskKind::ArchitectureReview => "architecture_review",
            TaskKind::CodeReview => "code_review",
            TaskKind::MergeCandidate => "merge_candidate",
            TaskKind::MergeRepair => "merge_repair",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutorRole {
    MetaAgent,
    Cartographer,
    Quartermaster,
    Dockmaster,
}

impl ExecutorRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutorRole::MetaAgent => "meta-agent",
            ExecutorRole::Cartographer => "cartographer",
            ExecutorRole::Quartermaster => "quartermaster",
            ExecutorRole::Dockmaster => "dockmaster",
        }
    }
}

impl TryFrom<&str> for ExecutorRole {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "meta-agent" => Ok(ExecutorRole::MetaAgent),
            "cartographer" => Ok(ExecutorRole::Cartographer),
            "quartermaster" => Ok(ExecutorRole::Quartermaster),
            "dockmaster" => Ok(ExecutorRole::Dockmaster),
            other => Err(format!("unknown executor_role: {other}")),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetRef {
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub base_branch: Option<String>,
    #[serde(default)]
    pub base_sha: Option<String>,
    #[serde(default)]
    pub head_branch: Option<String>,
    #[serde(default)]
    pub head_sha: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRefs {
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskMetadata {
    #[serde(default)]
    pub task_kind: Option<TaskKind>,
    #[serde(default)]
    pub executor_role: Option<ExecutorRole>,
    #[serde(default)]
    pub target_ref: Option<TargetRef>,
    #[serde(default)]
    pub artifacts: ArtifactRefs,
    #[serde(default, flatten)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    PlanArtifact,
    CartographerPlanConfirmation,
    CartographerCheckpointReview,
    ArchitectureReviewSurface,
    QuartermasterCheckpointReview,
    QuartermasterReview,
    SharedSurfaceGuard,
    BreakGlassAuthorization,
    DockmasterMergeDecision,
}

impl ArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactKind::PlanArtifact => "plan_artifact",
            ArtifactKind::CartographerPlanConfirmation => "cartographer_plan_confirmation",
            ArtifactKind::CartographerCheckpointReview => "cartographer_checkpoint_review",
            ArtifactKind::ArchitectureReviewSurface => "architecture_review_surface",
            ArtifactKind::QuartermasterCheckpointReview => "quartermaster_checkpoint_review",
            ArtifactKind::QuartermasterReview => "quartermaster_review",
            ArtifactKind::SharedSurfaceGuard => "shared_surface_guard",
            ArtifactKind::BreakGlassAuthorization => "break_glass_authorization",
            ArtifactKind::DockmasterMergeDecision => "dockmaster_merge_decision",
        }
    }
}

impl TryFrom<&str> for ArtifactKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "plan_artifact" => Ok(ArtifactKind::PlanArtifact),
            "cartographer_plan_confirmation" => Ok(ArtifactKind::CartographerPlanConfirmation),
            "cartographer_checkpoint_review" => Ok(ArtifactKind::CartographerCheckpointReview),
            "architecture_review_surface" => Ok(ArtifactKind::ArchitectureReviewSurface),
            "quartermaster_checkpoint_review" => Ok(ArtifactKind::QuartermasterCheckpointReview),
            "quartermaster_review" => Ok(ArtifactKind::QuartermasterReview),
            "shared_surface_guard" => Ok(ArtifactKind::SharedSurfaceGuard),
            "break_glass_authorization" => Ok(ArtifactKind::BreakGlassAuthorization),
            "dockmaster_merge_decision" => Ok(ArtifactKind::DockmasterMergeDecision),
            other => Err(format!("unknown artifact_kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceArtifact {
    pub artifact_id: String,
    pub artifact_kind: ArtifactKind,
    pub task_id: Uuid,
    pub repo: String,
    pub base_sha: String,
    pub head_sha: String,
    pub created_at: DateTime<Utc>,
    pub producer_role: ExecutorRole,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaimRequest {
    pub agent_id: String,
    #[serde(default)]
    pub task_id: Option<Uuid>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub task_kind: Option<TaskKind>,
    #[serde(default)]
    pub executor_role: Option<ExecutorRole>,
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
    #[serde(default)]
    pub metadata: TaskMetadata,
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
    pub metadata: Json<TaskMetadata>,
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
            metadata: row.metadata.0,
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
            metadata: Json(task.metadata),
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
        let database_url =
            crate::config::resolve_database_url().expect("DATABASE_URL must be set for tests");
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
            metadata: TaskMetadata::default(),
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

        let row: (String,) = sqlx::query_as("SELECT status::text FROM tasks WHERE task_id = $1")
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
        let database_url =
            crate::config::resolve_database_url().expect("DATABASE_URL must be set for tests");
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
