use crate::contracts::{
    ArtifactKind, ExecutorRole, GovernanceArtifact, Lease, Payload, RetryLogic, Task, TaskKind,
    TaskMetadata, TaskRow, TaskStatus,
};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Notify;
use uuid::Uuid;

pub const DEFAULT_RUN_ID: Uuid = Uuid::nil();

pub struct TaskRepository<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, Clone, Default)]
pub struct TaskListFilter {
    pub status: Option<String>,
    pub task_kind: Option<String>,
    pub executor_role: Option<String>,
    pub limit: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRunInput {
    #[serde(default)]
    pub run_id: Option<Uuid>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub source_kind: Option<String>,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RunRecord {
    pub run_id: Uuid,
    pub display_name: Option<String>,
    pub source_kind: String,
    pub source_ref: Option<String>,
    pub status: String,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateRunOutcome {
    pub run: RunRecord,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RunListItem {
    pub run_id: Uuid,
    pub display_name: Option<String>,
    pub source_kind: String,
    pub source_ref: Option<String>,
    pub status: String,
    pub task_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub run_id: Uuid,
    pub task_count: i64,
    pub status_counts: HashMap<String, i64>,
    pub missing_status_count: i64,
    pub next_pending_task_id: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunTimelineEvent {
    pub kind: String,
    pub task_id: String,
    pub status: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    pub run_id: Uuid,
    pub task_id: String,
    pub hash_algorithm: String,
    pub task_hash: String,
    pub task: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskProgressInput {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskResultInput {
    pub result_summary: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug)]
pub enum TaskViewError {
    NotFound,
    HashMismatch,
    InvalidTaskPayload,
    SqlxError(sqlx::Error),
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchTaskInput {
    pub task_id: Uuid,
    pub parent_ids: Vec<Uuid>,
    pub child_ids: Vec<Uuid>,
    pub payload: Payload,
    #[serde(default)]
    pub metadata: TaskMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchTaskResult {
    pub created_task_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitRequest {
    pub agent_id: String,
    pub result_summary: String,
    pub files_changed: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetryRequest {
    pub agent_id: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubmitResult {
    pub task_id: Uuid,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct GovernanceArtifactRow {
    artifact_id: String,
    artifact_kind: String,
    task_id: Uuid,
    repo: String,
    base_sha: String,
    head_sha: String,
    created_at: DateTime<Utc>,
    producer_role: String,
    payload: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ClaimFilter {
    pub task_id: Option<Uuid>,
    pub task_kind: Option<TaskKind>,
    pub executor_role: Option<ExecutorRole>,
}

#[derive(Debug)]
pub enum BatchError {
    CycleDetected(String),
    SqlxError(sqlx::Error),
}

#[derive(Debug)]
pub enum HeartbeatError {
    TaskNotFound,
    AgentMismatch,
    SqlxError(sqlx::Error),
}

#[derive(Debug)]
pub enum SubmitError {
    TaskNotFound,
    StatusMismatch,
    AgentMismatch,
    SqlxError(sqlx::Error),
}

impl From<sqlx::Error> for SubmitError {
    fn from(err: sqlx::Error) -> Self {
        SubmitError::SqlxError(err)
    }
}

impl From<sqlx::Error> for HeartbeatError {
    fn from(err: sqlx::Error) -> Self {
        HeartbeatError::SqlxError(err)
    }
}

impl From<sqlx::Error> for BatchError {
    fn from(err: sqlx::Error) -> Self {
        BatchError::SqlxError(err)
    }
}

impl TryFrom<GovernanceArtifactRow> for GovernanceArtifact {
    type Error = sqlx::Error;

    fn try_from(row: GovernanceArtifactRow) -> Result<Self, Self::Error> {
        let artifact_kind =
            ArtifactKind::try_from(row.artifact_kind.as_str()).map_err(sqlx::Error::Protocol)?;
        let producer_role =
            ExecutorRole::try_from(row.producer_role.as_str()).map_err(sqlx::Error::Protocol)?;
        Ok(Self {
            artifact_id: row.artifact_id,
            artifact_kind,
            task_id: row.task_id,
            repo: row.repo,
            base_sha: row.base_sha,
            head_sha: row.head_sha,
            created_at: row.created_at,
            producer_role,
            payload: row.payload,
        })
    }
}

impl From<sqlx::Error> for TaskViewError {
    fn from(err: sqlx::Error) -> Self {
        TaskViewError::SqlxError(err)
    }
}

pub fn graph_task_hash(run_id: Uuid, task_id: &str, task_data: &Value) -> String {
    let canonical = json!({
        "run_id": run_id,
        "task_id": task_id,
        "task": task_data,
    });
    let encoded = serde_json::to_vec(&canonical)
        .expect("canonical graph task hash payload must be JSON-serializable");
    let digest = Sha256::digest(encoded);
    format!("{digest:x}")
}

fn task_view(run_id: Uuid, task_id: String, task: Value) -> TaskView {
    let task_hash = graph_task_hash(run_id, &task_id, &task);
    TaskView {
        run_id,
        task_id,
        hash_algorithm: "sha256".to_string(),
        task_hash,
        task,
    }
}

fn ensure_object(value: &mut Value) -> Result<&mut Map<String, Value>, TaskViewError> {
    value
        .as_object_mut()
        .ok_or(TaskViewError::InvalidTaskPayload)
}

fn ensure_metadata(task: &mut Value) -> Result<&mut Map<String, Value>, TaskViewError> {
    let object = ensure_object(task)?;
    let metadata = object
        .entry("metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    metadata
        .as_object_mut()
        .ok_or(TaskViewError::InvalidTaskPayload)
}

impl<'a> TaskRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn ensure_run(&self, run_id: Uuid) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO runs (run_id, display_name, source_kind, source_ref, status)
            VALUES ($1, $2, 'system', 'auto-created', 'active')
            ON CONFLICT (run_id) DO NOTHING
            "#,
        )
        .bind(run_id)
        .bind(if run_id == DEFAULT_RUN_ID {
            Some("default")
        } else {
            None
        })
        .execute(self.pool)
        .await
        .map(|_| ())
    }

    pub async fn create_run(&self, input: CreateRunInput) -> sqlx::Result<CreateRunOutcome> {
        let run_id = input.run_id.unwrap_or_else(Uuid::new_v4);
        let source_kind = input.source_kind.unwrap_or_else(|| "manual".to_string());
        let metadata = input.metadata.unwrap_or_else(|| serde_json::json!({}));

        let inserted: Option<RunRecord> = sqlx::query_as(
            r#"
            INSERT INTO runs (run_id, display_name, source_kind, source_ref, metadata)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (run_id) DO NOTHING
            RETURNING run_id, display_name, source_kind, source_ref, status, metadata, created_at, updated_at
            "#,
        )
        .bind(run_id)
        .bind(input.display_name)
        .bind(source_kind)
        .bind(input.source_ref)
        .bind(metadata)
        .fetch_optional(self.pool)
        .await?;

        if let Some(run) = inserted {
            return Ok(CreateRunOutcome { run, created: true });
        }

        let run = self
            .get_run(run_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        Ok(CreateRunOutcome {
            run,
            created: false,
        })
    }

    pub async fn get_run(&self, run_id: Uuid) -> sqlx::Result<Option<RunRecord>> {
        sqlx::query_as(
            r#"
            SELECT run_id, display_name, source_kind, source_ref, status, metadata, created_at, updated_at
            FROM runs
            WHERE run_id = $1
            "#,
        )
        .bind(run_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn run_exists(&self, run_id: Uuid) -> sqlx::Result<bool> {
        let row: Option<(Uuid,)> = sqlx::query_as("SELECT run_id FROM runs WHERE run_id = $1")
            .bind(run_id)
            .fetch_optional(self.pool)
            .await?;
        Ok(row.is_some())
    }

    pub async fn list_runs(&self) -> sqlx::Result<Vec<RunListItem>> {
        sqlx::query_as(
            r#"
            SELECT
                r.run_id,
                r.display_name,
                r.source_kind,
                r.source_ref,
                r.status,
                COUNT(g.task_id)::bigint AS task_count,
                r.created_at,
                GREATEST(r.updated_at, COALESCE(MAX(g.updated_at), r.updated_at)) AS updated_at
            FROM runs r
            LEFT JOIN graph_tasks g ON g.run_id = r.run_id
            GROUP BY r.run_id, r.display_name, r.source_kind, r.source_ref, r.status, r.created_at, r.updated_at
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(self.pool)
        .await
    }

    pub async fn insert_batch(
        &self,
        tasks: Vec<BatchTaskInput>,
    ) -> Result<BatchTaskResult, BatchError> {
        if tasks.is_empty() {
            return Ok(BatchTaskResult {
                created_task_ids: vec![],
            });
        }

        // Build adjacency list and check for cycles using DFS
        let task_ids: HashSet<Uuid> = tasks.iter().map(|t| t.task_id).collect();
        let mut adjacency: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        let mut in_degree: HashMap<Uuid, usize> = HashMap::new();

        for task in &tasks {
            in_degree.insert(task.task_id, task.parent_ids.len());
            adjacency.entry(task.task_id).or_default();
            for &parent_id in &task.parent_ids {
                if !task_ids.contains(&parent_id) {
                    adjacency.entry(parent_id).or_default();
                }
                adjacency.entry(parent_id).or_default().push(task.task_id);
            }
        }

        // DFS cycle detection
        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut rec_stack: HashSet<Uuid> = HashSet::new();

        fn dfs(
            node: Uuid,
            adjacency: &HashMap<Uuid, Vec<Uuid>>,
            visited: &mut HashSet<Uuid>,
            rec_stack: &mut HashSet<Uuid>,
        ) -> Option<String> {
            visited.insert(node);
            rec_stack.insert(node);

            if let Some(neighbors) = adjacency.get(&node) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        if let Some(msg) = dfs(neighbor, adjacency, visited, rec_stack) {
                            return Some(msg);
                        }
                    } else if rec_stack.contains(&neighbor) {
                        return Some(format!(
                            "Circular dependency detected: {} -> {}",
                            node, neighbor
                        ));
                    }
                }
            }

            rec_stack.remove(&node);
            None
        }

        for &task_id in &task_ids {
            if !visited.contains(&task_id)
                && let Some(msg) = dfs(task_id, &adjacency, &mut visited, &mut rec_stack)
            {
                return Err(BatchError::CycleDetected(msg));
            }
        }

        // Compute topological levels using Kahn's algorithm (BFS)
        let mut levels: HashMap<Uuid, i32> = HashMap::new();
        let mut queue: Vec<Uuid> = in_degree
            .iter()
            .filter_map(|(id, degree)| if *degree == 0 { Some(*id) } else { None })
            .collect();

        while let Some(node) = queue.pop() {
            let level = if let Some(&parent_level) = levels.get(&node) {
                parent_level + 1
            } else {
                0
            };

            levels.entry(node).or_insert(level);

            if let Some(neighbors) = adjacency.get(&node) {
                for &neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(&neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push(neighbor);
                        }
                    }
                    let new_level = level + 1;
                    let current = levels.get(&neighbor).copied().unwrap_or(0);
                    if new_level > current {
                        levels.insert(neighbor, new_level);
                    }
                }
            }
        }

        // Insert all tasks in a single transaction
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();
        let mut created_ids = Vec::new();

        for task in &tasks {
            let status = if task.parent_ids.is_empty() {
                TaskStatus::Ready
            } else {
                TaskStatus::Pending
            };

            let topological_level = *levels.get(&task.task_id).unwrap_or(&0);

            sqlx::query(
                r#"
                INSERT INTO tasks (
                    task_id, status, parent_ids, child_ids, payload, metadata,
                    lease, retry_logic, topological_level, created_at, updated_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "#,
            )
            .bind(task.task_id)
            .bind(status)
            .bind(&task.parent_ids)
            .bind(&task.child_ids)
            .bind(sqlx::types::Json(&task.payload))
            .bind(sqlx::types::Json(&task.metadata))
            .bind(Option::<sqlx::types::Json<Lease>>::None)
            .bind(Option::<sqlx::types::Json<RetryLogic>>::None)
            .bind(topological_level)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            created_ids.push(task.task_id);
        }

        tx.commit().await?;

        Ok(BatchTaskResult {
            created_task_ids: created_ids,
        })
    }

    pub async fn insert(&self, task: &Task) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO tasks (
                task_id, status, parent_ids, child_ids, payload, metadata,
                lease, retry_logic, topological_level, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(task.task_id)
        .bind(task.status)
        .bind(&task.parent_ids)
        .bind(&task.child_ids)
        .bind(sqlx::types::Json(&task.payload))
        .bind(sqlx::types::Json(&task.metadata))
        .bind(task.lease.as_ref().map(sqlx::types::Json))
        .bind(task.retry_logic.as_ref().map(sqlx::types::Json))
        .bind(task.topological_level)
        .bind(task.created_at)
        .bind(task.updated_at)
        .execute(self.pool)
        .await
        .map(|_| ())
    }

    pub async fn get(&self, task_id: Uuid) -> sqlx::Result<Option<Task>> {
        let row: Option<TaskRow> = sqlx::query_as("SELECT * FROM tasks WHERE task_id = $1")
            .bind(task_id)
            .fetch_optional(self.pool)
            .await?;

        Ok(row.map(Into::into))
    }

    pub async fn list_tasks(&self, filter: TaskListFilter) -> sqlx::Result<Vec<Task>> {
        let limit = if filter.limit <= 0 { 100 } else { filter.limit.min(500) };
        let rows: Vec<TaskRow> = match (
            filter.status.as_deref(),
            filter.task_kind.as_deref(),
            filter.executor_role.as_deref(),
        ) {
            (Some(status), Some(kind), Some(role)) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE status = $1
                      AND metadata->>'task_kind' = $2
                      AND metadata->>'executor_role' = $3
                    ORDER BY topological_level ASC, created_at ASC
                    LIMIT $4
                    "#,
                )
                .bind(status)
                .bind(kind)
                .bind(role)
                .bind(limit)
                .fetch_all(self.pool)
                .await?
            }
            (Some(status), Some(kind), None) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE status = $1
                      AND metadata->>'task_kind' = $2
                    ORDER BY topological_level ASC, created_at ASC
                    LIMIT $3
                    "#,
                )
                .bind(status)
                .bind(kind)
                .bind(limit)
                .fetch_all(self.pool)
                .await?
            }
            (Some(status), None, Some(role)) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE status = $1
                      AND metadata->>'executor_role' = $2
                    ORDER BY topological_level ASC, created_at ASC
                    LIMIT $3
                    "#,
                )
                .bind(status)
                .bind(role)
                .bind(limit)
                .fetch_all(self.pool)
                .await?
            }
            (Some(status), None, None) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE status = $1
                    ORDER BY topological_level ASC, created_at ASC
                    LIMIT $2
                    "#,
                )
                .bind(status)
                .bind(limit)
                .fetch_all(self.pool)
                .await?
            }
            (None, Some(kind), Some(role)) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE metadata->>'task_kind' = $1
                      AND metadata->>'executor_role' = $2
                    ORDER BY topological_level ASC, created_at ASC
                    LIMIT $3
                    "#,
                )
                .bind(kind)
                .bind(role)
                .bind(limit)
                .fetch_all(self.pool)
                .await?
            }
            (None, Some(kind), None) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE metadata->>'task_kind' = $1
                    ORDER BY topological_level ASC, created_at ASC
                    LIMIT $2
                    "#,
                )
                .bind(kind)
                .bind(limit)
                .fetch_all(self.pool)
                .await?
            }
            (None, None, Some(role)) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE metadata->>'executor_role' = $1
                    ORDER BY topological_level ASC, created_at ASC
                    LIMIT $2
                    "#,
                )
                .bind(role)
                .bind(limit)
                .fetch_all(self.pool)
                .await?
            }
            (None, None, None) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    ORDER BY topological_level ASC, created_at ASC
                    LIMIT $1
                    "#,
                )
                .bind(limit)
                .fetch_all(self.pool)
                .await?
            }
        };

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn upsert_governance_artifact(
        &self,
        artifact: &GovernanceArtifact,
    ) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO governance_artifacts (
                artifact_id, artifact_kind, task_id, repo, base_sha, head_sha,
                created_at, producer_role, payload
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (artifact_id) DO UPDATE SET
                artifact_kind = EXCLUDED.artifact_kind,
                task_id = EXCLUDED.task_id,
                repo = EXCLUDED.repo,
                base_sha = EXCLUDED.base_sha,
                head_sha = EXCLUDED.head_sha,
                producer_role = EXCLUDED.producer_role,
                payload = EXCLUDED.payload
            "#,
        )
        .bind(&artifact.artifact_id)
        .bind(artifact.artifact_kind.as_str())
        .bind(artifact.task_id)
        .bind(&artifact.repo)
        .bind(&artifact.base_sha)
        .bind(&artifact.head_sha)
        .bind(artifact.created_at)
        .bind(artifact.producer_role.as_str())
        .bind(&artifact.payload)
        .execute(self.pool)
        .await
        .map(|_| ())
    }

    pub async fn get_governance_artifact(
        &self,
        artifact_id: &str,
    ) -> sqlx::Result<Option<GovernanceArtifact>> {
        let row: Option<GovernanceArtifactRow> = sqlx::query_as(
            r#"
            SELECT artifact_id, artifact_kind, task_id, repo, base_sha, head_sha,
                   created_at, producer_role, payload
            FROM governance_artifacts
            WHERE artifact_id = $1
            "#,
        )
        .bind(artifact_id)
        .fetch_optional(self.pool)
        .await?;

        row.map(TryInto::try_into).transpose()
    }

    pub async fn list_graph_tasks(&self) -> sqlx::Result<Vec<Value>> {
        self.list_graph_tasks_for_run(DEFAULT_RUN_ID).await
    }

    pub async fn list_graph_tasks_for_run(&self, run_id: Uuid) -> sqlx::Result<Vec<Value>> {
        let rows: Vec<(Value,)> = sqlx::query_as(
            "SELECT task_data FROM graph_tasks WHERE run_id = $1 ORDER BY task_order ASC",
        )
        .bind(run_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(|(task_data,)| task_data).collect())
    }

    pub async fn get_graph_task(&self, task_id: &str) -> sqlx::Result<Option<Value>> {
        self.get_graph_task_for_run(DEFAULT_RUN_ID, task_id).await
    }

    pub async fn get_graph_task_for_run(
        &self,
        run_id: Uuid,
        task_id: &str,
    ) -> sqlx::Result<Option<Value>> {
        let row: Option<(Value,)> =
            sqlx::query_as("SELECT task_data FROM graph_tasks WHERE run_id = $1 AND task_id = $2")
                .bind(run_id)
                .bind(task_id)
                .fetch_optional(self.pool)
                .await?;
        Ok(row.map(|(task_data,)| task_data))
    }

    pub async fn get_task_view_by_hash(
        &self,
        run_id: Uuid,
        task_hash: &str,
    ) -> Result<Option<TaskView>, TaskViewError> {
        let rows: Vec<(String, Value)> = sqlx::query_as(
            "SELECT task_id, task_data FROM graph_tasks WHERE run_id = $1 ORDER BY task_order ASC",
        )
        .bind(run_id)
        .fetch_all(self.pool)
        .await?;

        for (task_id, task_data) in rows {
            if graph_task_hash(run_id, &task_id, &task_data) == task_hash {
                return Ok(Some(task_view(run_id, task_id, task_data)));
            }
        }
        Ok(None)
    }

    pub async fn append_task_progress_by_hash(
        &self,
        run_id: Uuid,
        task_hash: &str,
        input: TaskProgressInput,
    ) -> Result<TaskView, TaskViewError> {
        let mut tx = self.pool.begin().await?;
        let rows: Vec<(String, Value)> = sqlx::query_as(
            "SELECT task_id, task_data FROM graph_tasks WHERE run_id = $1 ORDER BY task_order ASC FOR UPDATE",
        )
        .bind(run_id)
        .fetch_all(&mut *tx)
        .await?;

        for (task_id, mut task_data) in rows {
            if graph_task_hash(run_id, &task_id, &task_data) != task_hash {
                continue;
            }
            let metadata = ensure_metadata(&mut task_data)?;
            let progress = metadata
                .entry("worker_progress")
                .or_insert_with(|| Value::Array(Vec::new()));
            if !progress.is_array() {
                *progress = Value::Array(Vec::new());
            }
            progress
                .as_array_mut()
                .ok_or(TaskViewError::InvalidTaskPayload)?
                .push(json!({
                    "summary": input.summary,
                    "details": input.details,
                    "recorded_at": Utc::now(),
                }));

            sqlx::query("UPDATE graph_tasks SET task_data = $1 WHERE run_id = $2 AND task_id = $3")
                .bind(&task_data)
                .bind(run_id)
                .bind(&task_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(task_view(run_id, task_id, task_data));
        }

        tx.commit().await?;
        Err(TaskViewError::HashMismatch)
    }

    pub async fn submit_task_result_by_hash(
        &self,
        run_id: Uuid,
        task_hash: &str,
        input: TaskResultInput,
    ) -> Result<TaskView, TaskViewError> {
        let mut tx = self.pool.begin().await?;
        let rows: Vec<(String, Value)> = sqlx::query_as(
            "SELECT task_id, task_data FROM graph_tasks WHERE run_id = $1 ORDER BY task_order ASC FOR UPDATE",
        )
        .bind(run_id)
        .fetch_all(&mut *tx)
        .await?;

        for (task_id, mut task_data) in rows {
            if graph_task_hash(run_id, &task_id, &task_data) != task_hash {
                continue;
            }
            let object = ensure_object(&mut task_data)?;
            let status = input.status.unwrap_or_else(|| "evaluating".to_string());
            object.insert("status".to_string(), Value::String(status));
            let metadata = ensure_metadata(&mut task_data)?;
            metadata.insert(
                "worker_result".to_string(),
                json!({
                    "result_summary": input.result_summary,
                    "evidence_refs": input.evidence_refs,
                    "recorded_at": Utc::now(),
                }),
            );

            sqlx::query("UPDATE graph_tasks SET task_data = $1 WHERE run_id = $2 AND task_id = $3")
                .bind(&task_data)
                .bind(run_id)
                .bind(&task_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(task_view(run_id, task_id, task_data));
        }

        tx.commit().await?;
        Err(TaskViewError::HashMismatch)
    }

    pub async fn replace_graph_tasks(&self, tasks: &[Value]) -> sqlx::Result<()> {
        self.replace_graph_tasks_for_run(DEFAULT_RUN_ID, tasks)
            .await
    }

    pub async fn replace_graph_tasks_for_run(
        &self,
        run_id: Uuid,
        tasks: &[Value],
    ) -> sqlx::Result<()> {
        self.ensure_run(run_id).await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM graph_tasks WHERE run_id = $1")
            .bind(run_id)
            .execute(&mut *tx)
            .await?;

        for (index, task) in tasks.iter().enumerate() {
            let task_id = graph_task_id(task).ok_or_else(|| {
                sqlx::Error::Protocol("graph task payload missing non-empty task_id".into())
            })?;
            sqlx::query(
                r#"
                INSERT INTO graph_tasks (run_id, task_id, task_order, task_data)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(run_id)
            .bind(&task_id)
            .bind(index as i32)
            .bind(task)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_graph_task(&self, task_id: &str, task: &Value) -> sqlx::Result<()> {
        self.upsert_graph_task_for_run(DEFAULT_RUN_ID, task_id, task)
            .await
    }

    pub async fn upsert_graph_task_for_run(
        &self,
        run_id: Uuid,
        task_id: &str,
        task: &Value,
    ) -> sqlx::Result<()> {
        self.ensure_run(run_id).await?;
        let existing_order: Option<(i32,)> =
            sqlx::query_as("SELECT task_order FROM graph_tasks WHERE run_id = $1 AND task_id = $2")
                .bind(run_id)
                .bind(task_id)
                .fetch_optional(self.pool)
                .await?;

        let task_order = match existing_order {
            Some((order,)) => order,
            None => {
                let next_order: (Option<i32>,) =
                    sqlx::query_as("SELECT MAX(task_order) FROM graph_tasks WHERE run_id = $1")
                        .bind(run_id)
                        .fetch_one(self.pool)
                        .await?;
                next_order.0.unwrap_or(-1) + 1
            }
        };

        sqlx::query(
            r#"
            INSERT INTO graph_tasks (run_id, task_id, task_order, task_data)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (run_id, task_id)
            DO UPDATE SET task_data = EXCLUDED.task_data
            "#,
        )
        .bind(run_id)
        .bind(task_id)
        .bind(task_order)
        .bind(task)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_graph_tasks_after(
        &self,
        anchor_task_id: &str,
        tasks: &[Value],
    ) -> Result<(), GraphTaskInsertError> {
        self.insert_graph_tasks_after_for_run(DEFAULT_RUN_ID, anchor_task_id, tasks)
            .await
    }

    pub async fn insert_graph_tasks_after_for_run(
        &self,
        run_id: Uuid,
        anchor_task_id: &str,
        tasks: &[Value],
    ) -> Result<(), GraphTaskInsertError> {
        self.ensure_run(run_id).await?;
        let mut tx = self.pool.begin().await?;

        let anchor_row: Option<(i32,)> =
            sqlx::query_as("SELECT task_order FROM graph_tasks WHERE run_id = $1 AND task_id = $2")
                .bind(run_id)
                .bind(anchor_task_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((anchor_order,)) = anchor_row else {
            tx.commit().await?;
            return Err(GraphTaskInsertError::UnknownAnchor(
                anchor_task_id.to_string(),
            ));
        };

        for task in tasks {
            let Some(task_id) = graph_task_id(task) else {
                tx.commit().await?;
                return Err(GraphTaskInsertError::InvalidTaskPayload);
            };

            let exists: Option<(String,)> = sqlx::query_as(
                "SELECT task_id FROM graph_tasks WHERE run_id = $1 AND task_id = $2",
            )
            .bind(run_id)
            .bind(&task_id)
            .fetch_optional(&mut *tx)
            .await?;
            if exists.is_some() {
                tx.commit().await?;
                return Err(GraphTaskInsertError::DuplicateTask(task_id));
            }
        }

        sqlx::query(
            "UPDATE graph_tasks SET task_order = task_order + $1 WHERE run_id = $2 AND task_order > $3"
        )
        .bind(tasks.len() as i32)
        .bind(run_id)
        .bind(anchor_order)
        .execute(&mut *tx)
        .await?;

        for (index, task) in tasks.iter().enumerate() {
            let task_id = graph_task_id(task).ok_or(GraphTaskInsertError::InvalidTaskPayload)?;
            sqlx::query(
                r#"
                INSERT INTO graph_tasks (run_id, task_id, task_order, task_data)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(run_id)
            .bind(&task_id)
            .bind(anchor_order + 1 + index as i32)
            .bind(task)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn next_pending_graph_task(
        &self,
        exclude_task_ids: &[String],
    ) -> sqlx::Result<Option<Value>> {
        self.next_pending_graph_task_for_run(DEFAULT_RUN_ID, exclude_task_ids)
            .await
    }

    pub async fn next_pending_graph_task_for_run(
        &self,
        run_id: Uuid,
        exclude_task_ids: &[String],
    ) -> sqlx::Result<Option<Value>> {
        let tasks = self.list_graph_tasks_for_run(run_id).await?;
        let excluded: HashSet<&str> = exclude_task_ids.iter().map(String::as_str).collect();
        let completed_ids: HashSet<String> = tasks
            .iter()
            .filter(|task| graph_task_status(task).as_deref() == Some("complete"))
            .filter_map(graph_task_id)
            .collect();

        let mut candidates: Vec<(usize, i64, Value)> = tasks
            .into_iter()
            .enumerate()
            .filter_map(|(index, task)| {
                let task_id = graph_task_id(&task)?;
                if graph_task_status(&task).as_deref() != Some("pending") {
                    return None;
                }
                if excluded.contains(task_id.as_str()) {
                    return None;
                }
                let dependencies = graph_task_dependencies(&task);
                if !dependencies.iter().all(|dep| completed_ids.contains(dep)) {
                    return None;
                }
                Some((index, graph_task_priority(&task), task))
            })
            .collect();

        candidates.sort_by(|(index_a, priority_a, _), (index_b, priority_b, _)| {
            priority_b.cmp(priority_a).then(index_a.cmp(index_b))
        });

        Ok(candidates.into_iter().next().map(|(_, _, task)| task))
    }

    pub async fn recover_in_progress_graph_tasks(&self) -> sqlx::Result<i64> {
        self.recover_in_progress_graph_tasks_for_run(DEFAULT_RUN_ID)
            .await
    }

    pub async fn recover_in_progress_graph_tasks_for_run(&self, run_id: Uuid) -> sqlx::Result<i64> {
        let tasks = self.list_graph_tasks_for_run(run_id).await?;
        let mut recovered = 0_i64;
        let mut tx = self.pool.begin().await?;

        for mut task in tasks {
            if graph_task_status(&task).as_deref() != Some("in_progress") {
                continue;
            }
            if let Some(status) = task.get_mut("status") {
                *status = Value::String("pending".to_string());
            }
            let task_id = graph_task_id(&task).ok_or_else(|| {
                sqlx::Error::Protocol("graph task payload missing non-empty task_id".into())
            })?;
            sqlx::query("UPDATE graph_tasks SET task_data = $1 WHERE run_id = $2 AND task_id = $3")
                .bind(&task)
                .bind(run_id)
                .bind(task_id)
                .execute(&mut *tx)
                .await?;
            recovered += 1;
        }

        tx.commit().await?;
        Ok(recovered)
    }

    pub async fn run_summary(&self, run_id: Uuid, include_next: bool) -> sqlx::Result<RunSummary> {
        let rows: Vec<(Option<String>, i64)> = sqlx::query_as(
            r#"
            SELECT task_data->>'status' AS status, COUNT(*)::bigint
            FROM graph_tasks
            WHERE run_id = $1
            GROUP BY task_data->>'status'
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool)
        .await?;

        let mut status_counts = HashMap::new();
        let mut task_count = 0_i64;
        let mut missing_status_count = 0_i64;
        for (status, count) in rows {
            task_count += count;
            if let Some(status) = status {
                status_counts.insert(status, count);
            } else {
                missing_status_count += count;
            }
        }

        let next_pending_task_id = if include_next {
            self.next_pending_graph_task_for_run(run_id, &[])
                .await?
                .and_then(|task| graph_task_id(&task))
        } else {
            None
        };

        let updated_at: (Option<DateTime<Utc>>,) =
            sqlx::query_as("SELECT MAX(updated_at) FROM graph_tasks WHERE run_id = $1")
                .bind(run_id)
                .fetch_one(self.pool)
                .await?;

        Ok(RunSummary {
            run_id,
            task_count,
            status_counts,
            missing_status_count,
            next_pending_task_id,
            updated_at: updated_at.0,
        })
    }

    pub async fn run_timeline(
        &self,
        run_id: Uuid,
        limit: i64,
    ) -> sqlx::Result<Vec<RunTimelineEvent>> {
        let rows: Vec<(String, Option<String>, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT task_id, task_data->>'status' AS status, updated_at
            FROM graph_tasks
            WHERE run_id = $1
            ORDER BY updated_at DESC, task_order ASC
            LIMIT $2
            "#,
        )
        .bind(run_id)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(task_id, status, updated_at)| RunTimelineEvent {
                kind: "task_snapshot".to_string(),
                task_id,
                status,
                updated_at,
            })
            .collect())
    }

    pub async fn update(&self, task: &Task) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            UPDATE tasks
            SET status = $1, parent_ids = $2, child_ids = $3, payload = $4,
                metadata = $5, lease = $6, retry_logic = $7, topological_level = $8,
                updated_at = $9
            WHERE task_id = $10
            "#,
        )
        .bind(task.status)
        .bind(&task.parent_ids)
        .bind(&task.child_ids)
        .bind(sqlx::types::Json(&task.payload))
        .bind(sqlx::types::Json(&task.metadata))
        .bind(task.lease.as_ref().map(sqlx::types::Json))
        .bind(task.retry_logic.as_ref().map(sqlx::types::Json))
        .bind(task.topological_level)
        .bind(task.updated_at)
        .bind(task.task_id)
        .execute(self.pool)
        .await
        .map(|_| ())
    }

    pub async fn update_status(&self, task_id: Uuid, status: TaskStatus) -> sqlx::Result<()> {
        sqlx::query("UPDATE tasks SET status = $1 WHERE task_id = $2")
            .bind(status)
            .bind(task_id)
            .execute(self.pool)
            .await
            .map(|_| ())
    }

    pub async fn claim_ready(
        &self,
        agent_id: &str,
        expires_at: DateTime<Utc>,
        filter: ClaimFilter,
    ) -> sqlx::Result<Option<Task>> {
        let mut tx = self.pool.begin().await?;
        let task_id_filter = filter.task_id;
        let task_kind = filter.task_kind.map(|kind| kind.as_str().to_string());
        let executor_role = filter.executor_role.map(|role| role.as_str().to_string());

        let row: Option<TaskRow> = if let Some(task_id) = task_id_filter {
            sqlx::query_as(
                r#"
                SELECT * FROM tasks
                WHERE status = 'READY'
                  AND task_id = $1
                FOR UPDATE SKIP LOCKED
                LIMIT 1
                "#,
            )
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await?
        } else {
            match (task_kind.as_deref(), executor_role.as_deref()) {
            (Some(kind), Some(role)) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE status = 'READY'
                      AND metadata->>'task_kind' = $1
                      AND metadata->>'executor_role' = $2
                    ORDER BY topological_level ASC, created_at ASC
                    FOR UPDATE SKIP LOCKED
                    LIMIT 1
                    "#,
                )
                .bind(kind)
                .bind(role)
                .fetch_optional(&mut *tx)
                .await?
            }
            (Some(kind), None) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE status = 'READY'
                      AND metadata->>'task_kind' = $1
                    ORDER BY topological_level ASC, created_at ASC
                    FOR UPDATE SKIP LOCKED
                    LIMIT 1
                    "#,
                )
                .bind(kind)
                .fetch_optional(&mut *tx)
                .await?
            }
            (None, Some(role)) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE status = 'READY'
                      AND metadata->>'executor_role' = $1
                    ORDER BY topological_level ASC, created_at ASC
                    FOR UPDATE SKIP LOCKED
                    LIMIT 1
                    "#,
                )
                .bind(role)
                .fetch_optional(&mut *tx)
                .await?
            }
            (None, None) => {
                sqlx::query_as(
                    r#"
                    SELECT * FROM tasks
                    WHERE status = 'READY'
                    ORDER BY topological_level ASC, created_at ASC
                    FOR UPDATE SKIP LOCKED
                    LIMIT 1
                    "#,
                )
                .fetch_optional(&mut *tx)
                .await?
            }
            }
        };

        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };

        let task_id = row.task_id;

        let lease = Lease {
            agent_id: agent_id.to_string(),
            expires_at,
        };

        sqlx::query(
            r#"
            UPDATE tasks
            SET status = 'IN_PROGRESS', lease = $1
            WHERE task_id = $2
            "#,
        )
        .bind(sqlx::types::Json(&lease))
        .bind(task_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let mut task: Task = row.into();
        task.status = TaskStatus::InProgress;
        task.lease = Some(lease);

        Ok(Some(task))
    }

    pub async fn release_lease(&self, task_id: Uuid, new_status: TaskStatus) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            UPDATE tasks
            SET status = $1, lease = NULL
            WHERE task_id = $2
            "#,
        )
        .bind(new_status)
        .bind(task_id)
        .execute(self.pool)
        .await
        .map(|_| ())
    }

    pub async fn list_by_status(&self, status: TaskStatus) -> sqlx::Result<Vec<Task>> {
        let rows: Vec<TaskRow> =
            sqlx::query_as("SELECT * FROM tasks WHERE status = $1 ORDER BY created_at ASC")
                .bind(status)
                .fetch_all(self.pool)
                .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn pending_children(&self, parent_id: Uuid) -> sqlx::Result<Vec<Task>> {
        let rows: Vec<TaskRow> = sqlx::query_as(
            r#"
            SELECT * FROM tasks
            WHERE $1 = ANY(parent_ids) AND status = 'PENDING'
            "#,
        )
        .bind(parent_id)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn delete(&self, task_id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM tasks WHERE task_id = $1")
            .bind(task_id)
            .execute(self.pool)
            .await
            .map(|_| ())
    }

    pub async fn update_retry_logic(
        &self,
        task_id: Uuid,
        retry_logic: &RetryLogic,
    ) -> sqlx::Result<()> {
        sqlx::query("UPDATE tasks SET retry_logic = $1 WHERE task_id = $2")
            .bind(sqlx::types::Json(retry_logic))
            .bind(task_id)
            .execute(self.pool)
            .await
            .map(|_| ())
    }

    pub async fn renew_lease(&self, task_id: Uuid, agent_id: &str) -> Result<(), HeartbeatError> {
        let task = self
            .get(task_id)
            .await?
            .ok_or(HeartbeatError::TaskNotFound)?;

        if task.status != TaskStatus::InProgress {
            return Err(HeartbeatError::TaskNotFound);
        }

        let lease = task.lease.as_ref().ok_or(HeartbeatError::TaskNotFound)?;

        if lease.agent_id != agent_id {
            return Err(HeartbeatError::AgentMismatch);
        }

        let new_expires_at = Utc::now() + chrono::Duration::minutes(15);

        sqlx::query(
            r#"
            UPDATE tasks
            SET lease = $1, updated_at = NOW()
            WHERE task_id = $2
            "#,
        )
        .bind(sqlx::types::Json(&Lease {
            agent_id: agent_id.to_string(),
            expires_at: new_expires_at,
        }))
        .bind(task_id)
        .execute(self.pool)
        .await?;

        Ok(())
    }

    pub async fn submit(
        &self,
        task_id: Uuid,
        agent_id: &str,
        _result_summary: &str,
        _files_changed: Vec<String>,
    ) -> Result<SubmitResult, SubmitError> {
        let mut tx = self.pool.begin().await?;

        let row: Option<TaskRow> = sqlx::query_as(
            r#"
            SELECT * FROM tasks
            WHERE task_id = $1
            FOR UPDATE
            "#,
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.commit().await?;
            return Err(SubmitError::TaskNotFound);
        };

        if row.status != TaskStatus::InProgress {
            tx.commit().await?;
            return Err(SubmitError::StatusMismatch);
        }

        let lease = row.lease.as_ref().ok_or(SubmitError::StatusMismatch)?;

        if lease.agent_id != agent_id {
            tx.commit().await?;
            return Err(SubmitError::AgentMismatch);
        }

        sqlx::query(
            r#"
            UPDATE tasks
            SET status = 'COMPLETED', lease = NULL, updated_at = NOW()
            WHERE task_id = $1
            "#,
        )
        .bind(task_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        self.resolve_dependencies(task_id).await?;

        Ok(SubmitResult { task_id })
    }

    pub async fn retry_task(
        &self,
        task_id: Uuid,
        agent_id: &str,
        _reason: &str,
    ) -> Result<SubmitResult, SubmitError> {
        let row: Option<TaskRow> = sqlx::query_as(
            r#"
            SELECT * FROM tasks
            WHERE task_id = $1
            "#,
        )
        .bind(task_id)
        .fetch_optional(self.pool)
        .await?;

        let Some(row) = row else {
            return Err(SubmitError::TaskNotFound);
        };

        if row.status != TaskStatus::InProgress {
            return Err(SubmitError::StatusMismatch);
        }

        let lease = row.lease.as_ref().ok_or(SubmitError::StatusMismatch)?;

        if lease.agent_id != agent_id {
            return Err(SubmitError::AgentMismatch);
        }

        self.backoff_task(task_id).await?;

        Ok(SubmitResult { task_id })
    }

    pub async fn complete_task(&self, task_id: Uuid) -> sqlx::Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE tasks
            SET status = 'COMPLETED', lease = NULL, updated_at = NOW()
            WHERE task_id = $1
            "#,
        )
        .bind(task_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        self.resolve_dependencies(task_id).await
    }

    async fn resolve_dependencies(&self, parent_id: Uuid) -> sqlx::Result<()> {
        let children = self.pending_children(parent_id).await?;

        for child in children {
            let parent_ids = &child.parent_ids;
            let mut all_parents_completed = true;

            for pid in parent_ids {
                let parent: Option<Task> = self.get(*pid).await?;
                match parent {
                    Some(p) if p.status == TaskStatus::Completed => {}
                    _ => {
                        all_parents_completed = false;
                        break;
                    }
                }
            }

            if all_parents_completed {
                sqlx::query(
                    r#"
                    UPDATE tasks
                    SET status = 'READY', updated_at = NOW()
                    WHERE task_id = $1
                    "#,
                )
                .bind(child.task_id)
                .execute(self.pool)
                .await?;
            }
        }

        Ok(())
    }

    pub async fn backoff_task(&self, task_id: Uuid) -> sqlx::Result<()> {
        let task = self.get(task_id).await?.ok_or(sqlx::Error::RowNotFound)?;

        let current_attempt = task
            .retry_logic
            .as_ref()
            .map(|r| r.attempt_count)
            .unwrap_or(0);

        let new_attempt_count = current_attempt + 1;
        let backoff_minutes = (2_i64).pow(new_attempt_count as u32) * 2;
        let backoff_minutes = backoff_minutes.min(30);
        let backoff_until = Utc::now() + chrono::Duration::minutes(backoff_minutes);

        let retry_logic = RetryLogic {
            attempt_count: new_attempt_count,
            backoff_until: Some(backoff_until),
        };

        sqlx::query(
            r#"
            UPDATE tasks
            SET status = 'BACKOFF', lease = NULL, retry_logic = $1, updated_at = NOW()
            WHERE task_id = $2
            "#,
        )
        .bind(sqlx::types::Json(&retry_logic))
        .bind(task_id)
        .execute(self.pool)
        .await?;

        Ok(())
    }

    pub async fn reap_expired_leases(&self) -> sqlx::Result<Vec<(Uuid, String)>> {
        let expired_tasks: Vec<(Uuid, String)> = sqlx::query_as(
            r#"
            SELECT task_id, lease->>'agent_id' as agent_id
            FROM tasks
            WHERE status = 'IN_PROGRESS'
            AND lease IS NOT NULL
            AND (lease->>'expires_at')::timestamp with time zone < NOW()
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        for (task_id, _previous_agent_id) in &expired_tasks {
            sqlx::query(
                r#"
                UPDATE tasks
                SET status = 'READY', lease = NULL,
                    retry_logic = jsonb_set(
                        COALESCE(retry_logic, '{}'),
                        '{attempt_count}',
                        (COALESCE((retry_logic->>'attempt_count')::int, 0) + 1)::text::jsonb
                    ),
                    updated_at = NOW()
                WHERE task_id = $1
                "#,
            )
            .bind(task_id)
            .execute(self.pool)
            .await?;
        }

        Ok(expired_tasks)
    }

    pub async fn wake_backoff_tasks(&self) -> sqlx::Result<Vec<(Uuid, i32)>> {
        let backoff_tasks: Vec<(Uuid, i32)> = sqlx::query_as(
            r#"
            SELECT task_id, COALESCE((retry_logic->>'attempt_count')::int, 0) as attempt_count
            FROM tasks
            WHERE status = 'BACKOFF'
            AND retry_logic IS NOT NULL
            AND (retry_logic->>'backoff_until')::timestamp with time zone < NOW()
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        for (task_id, _) in &backoff_tasks {
            sqlx::query(
                r#"
                UPDATE tasks
                SET status = 'READY', updated_at = NOW()
                WHERE task_id = $1
                "#,
            )
            .bind(task_id)
            .execute(self.pool)
            .await?;
        }

        Ok(backoff_tasks)
    }

    pub fn run_validation_and_complete(
        pool: Arc<PgPool>,
        task_id: Uuid,
        validation_script: String,
    ) {
        let pool_clone = Arc::clone(&pool);
        tokio::spawn(async move {
            let output = Command::new("sh")
                .arg("-c")
                .arg(&validation_script)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await;

            let repo = TaskRepository::new(&pool_clone);

            match output {
                Ok(output) if output.status.success() => {
                    if let Err(e) = repo.complete_task(task_id).await {
                        eprintln!("Failed to complete task {}: {}", task_id, e);
                    }
                }
                Ok(_) => {
                    if let Err(e) = repo.backoff_task(task_id).await {
                        eprintln!("Failed to backoff task {}: {}", task_id, e);
                    }
                }
                Err(e) => {
                    eprintln!("Validation script failed for task {}: {}", task_id, e);
                    if let Err(e) = repo.backoff_task(task_id).await {
                        eprintln!("Failed to backoff task {}: {}", task_id, e);
                    }
                }
            }
        });
    }
}

#[derive(Debug)]
pub enum GraphTaskInsertError {
    UnknownAnchor(String),
    DuplicateTask(String),
    InvalidTaskPayload,
    SqlxError(sqlx::Error),
}

impl From<sqlx::Error> for GraphTaskInsertError {
    fn from(err: sqlx::Error) -> Self {
        GraphTaskInsertError::SqlxError(err)
    }
}

fn graph_task_id(task: &Value) -> Option<String> {
    task.get("task_id")?
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn graph_task_status(task: &Value) -> Option<String> {
    task.get("status")?.as_str().map(str::to_string)
}

fn graph_task_priority(task: &Value) -> i64 {
    task.get("priority").and_then(Value::as_i64).unwrap_or(100)
}

fn graph_task_dependencies(task: &Value) -> Vec<String> {
    task.get("dependencies")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub struct DependencyResolver {
    pool: PgPool,
    notify: Arc<Notify>,
    poll_interval: Duration,
}

impl DependencyResolver {
    pub fn new(pool: &PgPool, notify: Arc<Notify>) -> Self {
        Self {
            pool: pool.clone(),
            notify,
            poll_interval: Duration::from_secs(2),
        }
    }

    pub async fn run(self) {
        let mut last_check = Utc::now() - TimeDelta::seconds(5);
        let mut interval = tokio::time::interval(self.poll_interval);

        loop {
            tokio::select! {
                _ = self.notify.notified() => {
                    if let Err(e) = self.resolve_all(&mut last_check).await {
                        eprintln!("Dependency resolver error: {}", e);
                    }
                }
                _ = interval.tick() => {
                    if let Err(e) = self.resolve_all(&mut last_check).await {
                        eprintln!("Dependency resolver error: {}", e);
                    }
                }
            }
        }
    }

    async fn resolve_all(&self, last_check: &mut DateTime<Utc>) -> sqlx::Result<()> {
        let now = Utc::now();
        let completed_tasks = self.get_recently_completed_tasks(*last_check).await?;
        for task in completed_tasks {
            self.resolve_dependencies_for_task(task.task_id).await?;
        }
        *last_check = now;
        Ok(())
    }

    async fn get_recently_completed_tasks(&self, since: DateTime<Utc>) -> sqlx::Result<Vec<Task>> {
        let rows: Vec<TaskRow> = sqlx::query_as(
            r#"
            SELECT * FROM tasks
            WHERE status = 'COMPLETED' AND updated_at > $1
            ORDER BY updated_at ASC
            "#,
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn resolve_dependencies_for_task(&self, parent_id: Uuid) -> sqlx::Result<()> {
        let children_rows: Vec<TaskRow> = sqlx::query_as(
            r#"
            SELECT * FROM tasks
            WHERE $1 = ANY(parent_ids) AND status = 'PENDING'
            "#,
        )
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;

        for child_row in children_rows {
            let child: Task = child_row.into();
            let parent_ids = &child.parent_ids;
            let mut all_parents_completed = true;

            for pid in parent_ids {
                let parent_row: Option<TaskRow> =
                    sqlx::query_as("SELECT * FROM tasks WHERE task_id = $1")
                        .bind(pid)
                        .fetch_optional(&self.pool)
                        .await?;

                match parent_row {
                    Some(row) if row.status == TaskStatus::Completed => {}
                    _ => {
                        all_parents_completed = false;
                        break;
                    }
                }
            }

            if all_parents_completed {
                sqlx::query(
                    r#"
                    UPDATE tasks
                    SET status = 'READY', updated_at = NOW()
                    WHERE task_id = $1 AND status = 'PENDING'
                    "#,
                )
                .bind(child.task_id)
                .execute(&self.pool)
                .await?;
            }
        }

        Ok(())
    }
}

pub struct LeaseReaper {
    pool: PgPool,
    poll_interval: Duration,
}

impl LeaseReaper {
    pub fn new(pool: &PgPool) -> Self {
        Self {
            pool: pool.clone(),
            poll_interval: Duration::from_secs(60),
        }
    }

    pub async fn run(self) {
        let mut interval = tokio::time::interval(self.poll_interval);

        loop {
            interval.tick().await;
            if let Err(e) = self.reap_expired_leases().await {
                eprintln!("Lease reaper error: {}", e);
            }
        }
    }

    async fn reap_expired_leases(&self) -> sqlx::Result<()> {
        let repo = TaskRepository::new(&self.pool);
        let reclaimed = repo.reap_expired_leases().await?;

        for (task_id, previous_agent_id) in &reclaimed {
            println!(
                "Reclaimed task {} from agent {}",
                task_id, previous_agent_id
            );
        }

        Ok(())
    }
}

pub struct BackoffAwakener {
    pool: PgPool,
    poll_interval: Duration,
}

impl BackoffAwakener {
    pub fn new(pool: &PgPool) -> Self {
        Self {
            pool: pool.clone(),
            poll_interval: Duration::from_secs(30),
        }
    }

    pub async fn run(self) {
        let mut interval = tokio::time::interval(self.poll_interval);

        loop {
            interval.tick().await;
            if let Err(e) = self.wake_backoff_tasks().await {
                eprintln!("Backoff awakener error: {}", e);
            }
        }
    }

    async fn wake_backoff_tasks(&self) -> sqlx::Result<()> {
        let repo = TaskRepository::new(&self.pool);
        let awakened = repo.wake_backoff_tasks().await?;

        for (task_id, attempt_count) in &awakened {
            println!(
                "Awakened task {} (attempt_count={})",
                task_id, attempt_count
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Payload, TaskStatus};

    async fn test_pool() -> PgPool {
        let database_url =
            crate::config::resolve_database_url().expect("DATABASE_URL must be set for tests");
        PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to database")
    }

    fn test_task(instruction: &str) -> Task {
        Task {
            task_id: Uuid::new_v4(),
            status: TaskStatus::Pending,
            parent_ids: vec![],
            child_ids: vec![],
            payload: Payload {
                instruction: instruction.to_string(),
                context_paths: vec![],
                validation_script: None,
            },
            metadata: TaskMetadata::default(),
            lease: None,
            retry_logic: None,
            topological_level: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn graph_task(
        task_id: &str,
        status: Option<&str>,
        priority: i64,
        dependencies: &[&str],
    ) -> Value {
        let mut task = serde_json::json!({
            "task_id": task_id,
            "priority": priority,
            "dependencies": dependencies,
        });
        if let Some(status) = status {
            task["status"] = Value::String(status.to_string());
        }
        task
    }

    async fn cleanup_run(pool: &PgPool, run_id: Uuid) {
        let _ = sqlx::query("DELETE FROM runs WHERE run_id = $1")
            .bind(run_id)
            .execute(pool)
            .await;
    }

    #[tokio::test]
    async fn test_run_scoped_graph_tasks_allow_same_task_id_and_order() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let run_a = Uuid::new_v4();
        let run_b = Uuid::new_v4();

        repo.ensure_run(run_a).await.unwrap();
        repo.ensure_run(run_b).await.unwrap();
        repo.replace_graph_tasks_for_run(run_a, &[graph_task("same", Some("pending"), 10, &[])])
            .await
            .unwrap();
        repo.replace_graph_tasks_for_run(run_b, &[graph_task("same", Some("pending"), 10, &[])])
            .await
            .unwrap();

        let tasks_a = repo.list_graph_tasks_for_run(run_a).await.unwrap();
        let tasks_b = repo.list_graph_tasks_for_run(run_b).await.unwrap();
        assert_eq!(tasks_a.len(), 1);
        assert_eq!(tasks_b.len(), 1);
        assert_eq!(graph_task_id(&tasks_a[0]).as_deref(), Some("same"));
        assert_eq!(graph_task_id(&tasks_b[0]).as_deref(), Some("same"));

        cleanup_run(&pool, run_a).await;
        cleanup_run(&pool, run_b).await;
    }

    #[tokio::test]
    async fn test_task_hash_is_run_scoped() {
        let run_a = Uuid::new_v4();
        let run_b = Uuid::new_v4();
        let task = graph_task("same", Some("pending"), 10, &[]);

        let hash_a = graph_task_hash(run_a, "same", &task);
        let hash_b = graph_task_hash(run_b, "same", &task);

        assert_ne!(hash_a, hash_b);
    }

    #[tokio::test]
    async fn test_task_view_by_hash_returns_only_matching_task() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let run_id = Uuid::new_v4();

        repo.ensure_run(run_id).await.unwrap();
        let task_a = graph_task("a", Some("pending"), 10, &[]);
        let task_b = graph_task("b", Some("pending"), 9, &[]);
        repo.replace_graph_tasks_for_run(run_id, &[task_a.clone(), task_b])
            .await
            .unwrap();

        let task_hash = graph_task_hash(run_id, "a", &task_a);
        let view = repo
            .get_task_view_by_hash(run_id, &task_hash)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(view.task_id, "a");
        assert_eq!(view.task_hash, task_hash);
        assert_eq!(graph_task_id(&view.task).as_deref(), Some("a"));

        cleanup_run(&pool, run_id).await;
    }

    #[tokio::test]
    async fn test_task_progress_hash_must_match_current_task_snapshot() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let run_id = Uuid::new_v4();

        repo.ensure_run(run_id).await.unwrap();
        let task = graph_task("a", Some("pending"), 10, &[]);
        repo.replace_graph_tasks_for_run(run_id, &[task.clone()])
            .await
            .unwrap();

        let old_hash = graph_task_hash(run_id, "a", &task);
        let updated_view = repo
            .append_task_progress_by_hash(
                run_id,
                &old_hash,
                TaskProgressInput {
                    summary: "started".to_string(),
                    details: vec!["sandbox ready".to_string()],
                },
            )
            .await
            .unwrap();
        assert_ne!(updated_view.task_hash, old_hash);
        assert!(matches!(
            repo.append_task_progress_by_hash(
                run_id,
                &old_hash,
                TaskProgressInput {
                    summary: "stale".to_string(),
                    details: vec![],
                },
            )
            .await,
            Err(TaskViewError::HashMismatch)
        ));

        cleanup_run(&pool, run_id).await;
    }

    #[tokio::test]
    async fn test_replace_graph_tasks_for_run_only_deletes_target_run() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let run_a = Uuid::new_v4();
        let run_b = Uuid::new_v4();

        repo.ensure_run(run_a).await.unwrap();
        repo.ensure_run(run_b).await.unwrap();
        repo.replace_graph_tasks_for_run(run_a, &[graph_task("a1", Some("pending"), 10, &[])])
            .await
            .unwrap();
        repo.replace_graph_tasks_for_run(run_b, &[graph_task("b1", Some("pending"), 10, &[])])
            .await
            .unwrap();
        repo.replace_graph_tasks_for_run(run_a, &[graph_task("a2", Some("pending"), 10, &[])])
            .await
            .unwrap();

        let tasks_a = repo.list_graph_tasks_for_run(run_a).await.unwrap();
        let tasks_b = repo.list_graph_tasks_for_run(run_b).await.unwrap();
        assert_eq!(tasks_a.len(), 1);
        assert_eq!(tasks_b.len(), 1);
        assert_eq!(graph_task_id(&tasks_a[0]).as_deref(), Some("a2"));
        assert_eq!(graph_task_id(&tasks_b[0]).as_deref(), Some("b1"));

        cleanup_run(&pool, run_a).await;
        cleanup_run(&pool, run_b).await;
    }

    #[tokio::test]
    async fn test_next_pending_graph_task_for_run_is_scoped() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let run_a = Uuid::new_v4();
        let run_b = Uuid::new_v4();

        repo.ensure_run(run_a).await.unwrap();
        repo.ensure_run(run_b).await.unwrap();
        repo.replace_graph_tasks_for_run(
            run_a,
            &[
                graph_task("dep", Some("complete"), 0, &[]),
                graph_task("target", Some("pending"), 10, &["dep"]),
            ],
        )
        .await
        .unwrap();
        repo.replace_graph_tasks_for_run(
            run_b,
            &[graph_task("target", Some("pending"), 10, &["dep"])],
        )
        .await
        .unwrap();

        let next_a = repo
            .next_pending_graph_task_for_run(run_a, &[])
            .await
            .unwrap();
        let next_b = repo
            .next_pending_graph_task_for_run(run_b, &[])
            .await
            .unwrap();
        assert_eq!(
            next_a.and_then(|task| graph_task_id(&task)).as_deref(),
            Some("target")
        );
        assert!(next_b.is_none());

        cleanup_run(&pool, run_a).await;
        cleanup_run(&pool, run_b).await;
    }

    #[tokio::test]
    async fn test_run_summary_is_scoped_and_separates_missing_status() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let run_a = Uuid::new_v4();
        let run_b = Uuid::new_v4();

        repo.ensure_run(run_a).await.unwrap();
        repo.ensure_run(run_b).await.unwrap();
        repo.replace_graph_tasks_for_run(
            run_a,
            &[
                graph_task("a1", Some("pending"), 10, &[]),
                graph_task("a2", Some("complete"), 0, &[]),
                graph_task("a3", None, 0, &[]),
            ],
        )
        .await
        .unwrap();
        repo.replace_graph_tasks_for_run(run_b, &[graph_task("b1", Some("pending"), 10, &[])])
            .await
            .unwrap();

        let summary_a = repo.run_summary(run_a, false).await.unwrap();
        let summary_b = repo.run_summary(run_b, false).await.unwrap();
        assert_eq!(summary_a.task_count, 3);
        assert_eq!(summary_a.status_counts.get("pending"), Some(&1));
        assert_eq!(summary_a.status_counts.get("complete"), Some(&1));
        assert_eq!(summary_a.missing_status_count, 1);
        assert_eq!(summary_a.next_pending_task_id, None);
        assert_eq!(summary_b.task_count, 1);
        assert_eq!(summary_b.status_counts.get("pending"), Some(&1));
        assert_eq!(summary_b.missing_status_count, 0);

        let summary_a_with_next = repo.run_summary(run_a, true).await.unwrap();
        assert_eq!(
            summary_a_with_next.next_pending_task_id.as_deref(),
            Some("a1")
        );

        cleanup_run(&pool, run_a).await;
        cleanup_run(&pool, run_b).await;
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let task = test_task("insert_and_get");

        repo.insert(&task).await.expect("insert failed");
        let fetched = repo.get(task.task_id).await.expect("get failed");
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.task_id, task.task_id);
        assert_eq!(fetched.status, TaskStatus::Pending);
        assert_eq!(fetched.payload.instruction, "insert_and_get");

        repo.delete(task.task_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_update_status() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let task = test_task("update_status");
        repo.insert(&task).await.unwrap();

        repo.update_status(task.task_id, TaskStatus::Ready)
            .await
            .unwrap();

        let fetched = repo.get(task.task_id).await.unwrap().unwrap();
        assert_eq!(fetched.status, TaskStatus::Ready);

        repo.delete(task.task_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_claim_ready() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);

        // Clean up any stale test tasks to avoid interference.
        let _ = sqlx::query("DELETE FROM tasks WHERE payload->>'instruction' = 'claim_ready'")
            .execute(&pool)
            .await;

        let mut task = test_task("claim_ready");
        task.status = TaskStatus::Ready;
        repo.insert(&task).await.unwrap();

        let claimed = repo
            .claim_ready(
                "agent-1",
                Utc::now() + chrono::Duration::minutes(5),
                ClaimFilter::default(),
            )
            .await
            .expect("claim failed");

        assert!(claimed.is_some());
        let claimed = claimed.unwrap();
        assert_eq!(claimed.status, TaskStatus::InProgress);
        assert_eq!(claimed.lease.as_ref().unwrap().agent_id, "agent-1");

        // Remove the claimed task; if it happened to be a different row
        // also try to delete the one we inserted.
        repo.delete(claimed.task_id).await.unwrap();
        let _ = repo.delete(task.task_id).await;
    }

    #[tokio::test]
    async fn test_task_metadata_round_trip_and_filtered_claim() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);

        let mut meta_task = test_task("claim_meta_agent_implementation");
        meta_task.status = TaskStatus::Ready;
        meta_task.metadata.task_kind = Some(TaskKind::Implementation);
        meta_task.metadata.executor_role = Some(ExecutorRole::MetaAgent);
        repo.insert(&meta_task).await.unwrap();

        let mut review_task = test_task("claim_quartermaster_code_review");
        review_task.status = TaskStatus::Ready;
        review_task.metadata.task_kind = Some(TaskKind::CodeReview);
        review_task.metadata.executor_role = Some(ExecutorRole::Quartermaster);
        repo.insert(&review_task).await.unwrap();

        let wrong_role = repo
            .claim_ready(
                "meta-agent-1",
                Utc::now() + chrono::Duration::minutes(5),
                ClaimFilter {
                    task_id: None,
                    task_kind: Some(TaskKind::CodeReview),
                    executor_role: Some(ExecutorRole::MetaAgent),
                },
            )
            .await
            .unwrap();
        assert!(wrong_role.is_none());

        let claimed = repo
            .claim_ready(
                "quartermaster-1",
                Utc::now() + chrono::Duration::minutes(5),
                ClaimFilter {
                    task_id: None,
                    task_kind: Some(TaskKind::CodeReview),
                    executor_role: Some(ExecutorRole::Quartermaster),
                },
            )
            .await
            .unwrap()
            .expect("quartermaster task should be claimed");
        assert_eq!(claimed.task_id, review_task.task_id);
        assert_eq!(claimed.metadata.task_kind, Some(TaskKind::CodeReview));
        assert_eq!(
            claimed.metadata.executor_role,
            Some(ExecutorRole::Quartermaster)
        );

        let fetched = repo.get(meta_task.task_id).await.unwrap().unwrap();
        assert_eq!(fetched.metadata.task_kind, Some(TaskKind::Implementation));
        assert_eq!(
            fetched.metadata.executor_role,
            Some(ExecutorRole::MetaAgent)
        );

        repo.delete(claimed.task_id).await.unwrap();
        repo.delete(meta_task.task_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_governance_artifact_upsert_round_trip() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);

        let mut task = test_task("governance_artifact_round_trip");
        task.status = TaskStatus::Ready;
        repo.insert(&task).await.unwrap();

        let artifact = GovernanceArtifact {
            artifact_id: format!("artifact-{}", task.task_id),
            artifact_kind: ArtifactKind::CartographerPlanConfirmation,
            task_id: task.task_id,
            repo: "owner/repo".to_string(),
            base_sha: "base".to_string(),
            head_sha: "head".to_string(),
            created_at: Utc::now(),
            producer_role: ExecutorRole::Cartographer,
            payload: json!({
                "technical_plan_review": {
                    "decision": "approve_plan"
                }
            }),
        };

        repo.upsert_governance_artifact(&artifact).await.unwrap();
        let fetched = repo
            .get_governance_artifact(&artifact.artifact_id)
            .await
            .unwrap()
            .expect("artifact should exist");

        assert_eq!(
            fetched.artifact_kind,
            ArtifactKind::CartographerPlanConfirmation
        );
        assert_eq!(fetched.producer_role, ExecutorRole::Cartographer);
        assert_eq!(fetched.payload, artifact.payload);

        repo.delete(task.task_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_release_lease() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let mut task = test_task("release_lease");
        task.status = TaskStatus::InProgress;
        task.lease = Some(Lease {
            agent_id: "agent-1".to_string(),
            expires_at: Utc::now(),
        });
        repo.insert(&task).await.unwrap();

        repo.release_lease(task.task_id, TaskStatus::Backoff)
            .await
            .unwrap();

        let fetched = repo.get(task.task_id).await.unwrap().unwrap();
        assert_eq!(fetched.status, TaskStatus::Backoff);
        assert!(fetched.lease.is_none());

        repo.delete(task.task_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_list_by_status() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let mut task = test_task("list_by_status");
        task.status = TaskStatus::Ready;
        repo.insert(&task).await.unwrap();

        let list = repo.list_by_status(TaskStatus::Ready).await.unwrap();
        assert!(list.iter().any(|t| t.task_id == task.task_id));

        repo.delete(task.task_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_pending_children() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);
        let parent_id = Uuid::new_v4();
        let mut child = test_task("pending_children");
        child.parent_ids = vec![parent_id];
        repo.insert(&child).await.unwrap();

        let children = repo.pending_children(parent_id).await.unwrap();
        assert!(children.iter().any(|t| t.task_id == child.task_id));

        repo.delete(child.task_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_batch_no_tasks() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);

        let result = repo.insert_batch(vec![]).await.unwrap();
        assert!(result.created_task_ids.is_empty());
    }

    #[tokio::test]
    async fn test_insert_batch_root_task() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);

        let task_id = Uuid::new_v4();
        let tasks = vec![BatchTaskInput {
            task_id,
            parent_ids: vec![],
            child_ids: vec![],
            payload: Payload {
                instruction: "batch_root".to_string(),
                context_paths: vec![],
                validation_script: None,
            },
            metadata: TaskMetadata::default(),
        }];

        let result = repo.insert_batch(tasks).await.unwrap();
        assert_eq!(result.created_task_ids, vec![task_id]);

        let fetched = repo.get(task_id).await.unwrap().unwrap();
        assert_eq!(fetched.status, TaskStatus::Ready);
        assert_eq!(fetched.topological_level, 0);

        repo.delete(task_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_batch_with_dependencies() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);

        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        let tasks = vec![
            BatchTaskInput {
                task_id: parent_id,
                parent_ids: vec![],
                child_ids: vec![child_id],
                payload: Payload {
                    instruction: "batch_parent".to_string(),
                    context_paths: vec![],
                    validation_script: None,
                },
                metadata: TaskMetadata::default(),
            },
            BatchTaskInput {
                task_id: child_id,
                parent_ids: vec![parent_id],
                child_ids: vec![],
                payload: Payload {
                    instruction: "batch_child".to_string(),
                    context_paths: vec![],
                    validation_script: None,
                },
                metadata: TaskMetadata::default(),
            },
        ];

        let result = repo.insert_batch(tasks).await.unwrap();
        assert_eq!(result.created_task_ids, vec![parent_id, child_id]);

        let parent = repo.get(parent_id).await.unwrap().unwrap();
        assert_eq!(parent.status, TaskStatus::Ready);
        assert_eq!(parent.topological_level, 0);

        let child = repo.get(child_id).await.unwrap().unwrap();
        assert_eq!(child.status, TaskStatus::Pending);
        assert_eq!(child.topological_level, 1);

        repo.delete(parent_id).await.unwrap();
        repo.delete(child_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_batch_circular_dependency() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);

        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();

        let tasks = vec![
            BatchTaskInput {
                task_id: task_a,
                parent_ids: vec![task_b],
                child_ids: vec![task_b],
                payload: Payload {
                    instruction: "batch_a".to_string(),
                    context_paths: vec![],
                    validation_script: None,
                },
                metadata: TaskMetadata::default(),
            },
            BatchTaskInput {
                task_id: task_b,
                parent_ids: vec![task_a],
                child_ids: vec![task_a],
                payload: Payload {
                    instruction: "batch_b".to_string(),
                    context_paths: vec![],
                    validation_script: None,
                },
                metadata: TaskMetadata::default(),
            },
        ];

        let result = repo.insert_batch(tasks).await;
        assert!(result.is_err());
        match result {
            Err(BatchError::CycleDetected(msg)) => {
                assert!(msg.contains("Circular dependency detected"));
            }
            _ => panic!("Expected CycleDetected error"),
        }
    }

    #[tokio::test]
    async fn test_renew_lease_success() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);

        let mut task = test_task("renew_lease_success");
        task.status = TaskStatus::InProgress;
        let expires_at = Utc::now() + chrono::Duration::minutes(5);
        task.lease = Some(Lease {
            agent_id: "agent-1".to_string(),
            expires_at,
        });
        repo.insert(&task).await.unwrap();

        let result = repo.renew_lease(task.task_id, "agent-1").await;
        assert!(result.is_ok());

        let fetched = repo.get(task.task_id).await.unwrap().unwrap();
        assert!(fetched.lease.is_some());
        let new_expires = fetched.lease.unwrap().expires_at;
        assert!(new_expires > expires_at);

        repo.delete(task.task_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_renew_lease_task_not_found() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);

        let result = repo.renew_lease(Uuid::new_v4(), "agent-1").await;
        assert!(matches!(result, Err(HeartbeatError::TaskNotFound)));
    }

    #[tokio::test]
    async fn test_renew_lease_agent_mismatch() {
        let pool = test_pool().await;
        let repo = TaskRepository::new(&pool);

        let mut task = test_task("renew_lease_agent_mismatch");
        task.status = TaskStatus::InProgress;
        task.lease = Some(Lease {
            agent_id: "agent-1".to_string(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
        });
        repo.insert(&task).await.unwrap();

        let result = repo.renew_lease(task.task_id, "agent-2").await;
        assert!(matches!(result, Err(HeartbeatError::AgentMismatch)));

        repo.delete(task.task_id).await.unwrap();
    }
}
