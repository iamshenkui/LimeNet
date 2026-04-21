use crate::contracts::{Lease, RetryLogic, Task, TaskRow, TaskStatus};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub struct TaskRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> TaskRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
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
}
