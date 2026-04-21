use crate::contracts::{Lease, Payload, RetryLogic, Task, TaskRow, TaskStatus};
use chrono::{DateTime, Utc, TimeDelta};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Notify;
use uuid::Uuid;

pub struct TaskRepository<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchTaskInput {
    pub task_id: Uuid,
    pub parent_ids: Vec<Uuid>,
    pub child_ids: Vec<Uuid>,
    pub payload: Payload,
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

#[derive(Debug, Clone, Serialize)]
pub struct SubmitResult {
    pub task_id: Uuid,
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

impl<'a> TaskRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
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
                    task_id, status, parent_ids, child_ids, payload,
                    lease, retry_logic, topological_level, created_at, updated_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                "#,
            )
            .bind(task.task_id)
            .bind(status)
            .bind(&task.parent_ids)
            .bind(&task.child_ids)
            .bind(sqlx::types::Json(&task.payload))
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
                task_id, status, parent_ids, child_ids, payload,
                lease, retry_logic, topological_level, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(task.task_id)
        .bind(task.status)
        .bind(&task.parent_ids)
        .bind(&task.child_ids)
        .bind(sqlx::types::Json(&task.payload))
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

    pub async fn update(&self, task: &Task) -> sqlx::Result<()> {
        sqlx::query(
            r#"
            UPDATE tasks
            SET status = $1, parent_ids = $2, child_ids = $3, payload = $4,
                lease = $5, retry_logic = $6, topological_level = $7, updated_at = $8
            WHERE task_id = $9
            "#,
        )
        .bind(task.status)
        .bind(&task.parent_ids)
        .bind(&task.child_ids)
        .bind(sqlx::types::Json(&task.payload))
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
    ) -> sqlx::Result<Option<Task>> {
        let mut tx = self.pool.begin().await?;

        let row: Option<TaskRow> = sqlx::query_as(
            r#"
            SELECT * FROM tasks
            WHERE status = 'READY'
            ORDER BY topological_level ASC, created_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT 1
            "#,
        )
        .fetch_optional(&mut *tx)
        .await?;

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

    pub async fn release_lease(
        &self,
        task_id: Uuid,
        new_status: TaskStatus,
    ) -> sqlx::Result<()> {
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
        let rows: Vec<TaskRow> = sqlx::query_as(
            "SELECT * FROM tasks WHERE status = $1 ORDER BY created_at ASC",
        )
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

    pub async fn renew_lease(
        &self,
        task_id: Uuid,
        agent_id: &str,
    ) -> Result<(), HeartbeatError> {
        let task = self.get(task_id).await?.ok_or(HeartbeatError::TaskNotFound)?;

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
            SET status = 'EVALUATING', updated_at = NOW()
            WHERE task_id = $1
            "#,
        )
        .bind(task_id)
        .execute(&mut *tx)
        .await?;

        let validation_script = row.payload.0.validation_script.clone();

        tx.commit().await?;

        if let Some(script) = validation_script {
            let pool = Arc::new(self.pool.clone());
            tokio::spawn(async move {
                Self::run_validation_and_complete(pool, task_id, script);
            });
        }

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

        let current_attempt = task.retry_logic.as_ref()
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
                let parent_row: Option<TaskRow> = sqlx::query_as(
                    "SELECT * FROM tasks WHERE task_id = $1"
                )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Payload, TaskStatus};

    async fn test_pool() -> PgPool {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://chenhui@localhost:5432/postgres".to_string());
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
            lease: None,
            retry_logic: None,
            topological_level: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
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
            .claim_ready("agent-1", Utc::now() + chrono::Duration::minutes(5))
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