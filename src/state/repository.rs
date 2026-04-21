use crate::contracts::{Lease, Payload, RetryLogic, Task, TaskRow, TaskStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
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

#[derive(Debug)]
pub enum BatchError {
    CycleDetected(String),
    SqlxError(sqlx::Error),
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
            if !visited.contains(&task_id) {
                if let Some(msg) = dfs(task_id, &adjacency, &mut visited, &mut rec_stack) {
                    return Err(BatchError::CycleDetected(msg));
                }
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

            if levels.get(&node).is_none() || level > *levels.get(&node).unwrap_or(&0) {
                levels.insert(node, level);
            }

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
}