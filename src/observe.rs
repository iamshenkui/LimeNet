use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct ObserveInstance {
    pub instance_id: String,
    pub database_target: String,
    pub task_api_address: String,
    pub status_api_address: String,
    pub dashboard_address: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct ObserveConfig {
    pub instance: ObserveInstance,
}

#[derive(Debug, Clone)]
pub struct ObserveRepository {
    pool: PgPool,
    config: ObserveConfig,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObservedStatus {
    Pending,
    Ready,
    InProgress,
    Evaluating,
    Backoff,
    Completed,
    Unknown,
    Missing,
}

impl ObservedStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Ready => "READY",
            Self::InProgress => "IN_PROGRESS",
            Self::Evaluating => "EVALUATING",
            Self::Backoff => "BACKOFF",
            Self::Completed => "COMPLETED",
            Self::Unknown => "UNKNOWN",
            Self::Missing => "MISSING",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusObservation {
    pub normalized: String,
    pub raw: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Signal {
    pub level: String,
    pub code: String,
    pub message: String,
    pub run_id: Option<Uuid>,
    pub task_id: Option<String>,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSnapshot {
    pub schema_version: String,
    pub observed_at: DateTime<Utc>,
    pub scope: SnapshotScope,
    pub instance: ObserveInstance,
    pub runs: Vec<RunObservationSummary>,
    pub signals: Vec<Signal>,
    pub links: SnapshotLinks,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotScope {
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotLinks {
    pub task_api: String,
    pub status_api: String,
    pub dashboard: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunObservationSummary {
    pub run_id: Uuid,
    pub display_name: Option<String>,
    pub source_kind: String,
    pub source_ref: Option<String>,
    pub status: String,
    pub task_count: i64,
    pub status_counts: BTreeMap<String, i64>,
    pub missing_status_count: i64,
    pub unknown_status_count: i64,
    pub next_pending_task_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub signals: Vec<Signal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSnapshot {
    pub schema_version: String,
    pub observed_at: DateTime<Utc>,
    pub instance: ObserveInstance,
    pub run: RunObservationSummary,
    pub tasks: Vec<TaskObservation>,
    pub recent_events: Vec<TaskEventObservation>,
    pub signals: Vec<Signal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskSnapshot {
    pub schema_version: String,
    pub observed_at: DateTime<Utc>,
    pub instance: ObserveInstance,
    pub run: RunObservationSummary,
    pub task: TaskObservation,
    pub recent_events: Vec<TaskEventObservation>,
    pub signals: Vec<Signal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskObservation {
    pub run_id: Uuid,
    pub task_id: String,
    pub task_order: i32,
    pub title: Option<String>,
    pub status: StatusObservation,
    pub updated_at: DateTime<Utc>,
    pub dependencies: Vec<String>,
    pub dependency_statuses: Vec<DependencyObservation>,
    pub dependencies_complete: bool,
    pub is_next_pending: bool,
    pub raw_task: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyObservation {
    pub task_id: String,
    pub status: Option<String>,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskEventObservation {
    pub kind: String,
    pub task_id: String,
    pub status: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RunRow {
    run_id: Uuid,
    display_name: Option<String>,
    source_kind: String,
    source_ref: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct GraphTaskRow {
    task_id: String,
    task_order: i32,
    task_data: Value,
    updated_at: DateTime<Utc>,
}

impl ObserveRepository {
    pub fn new(pool: PgPool, config: ObserveConfig) -> Self {
        Self { pool, config }
    }

    pub async fn global_snapshot(&self) -> sqlx::Result<GlobalSnapshot> {
        let observed_at = Utc::now();
        let runs = self.run_summaries(observed_at).await?;
        let signals = runs
            .iter()
            .flat_map(|run| run.signals.iter().cloned())
            .collect();

        Ok(GlobalSnapshot {
            schema_version: "0.1".to_string(),
            observed_at,
            scope: SnapshotScope {
                mode: "global_summary".to_string(),
            },
            instance: self.config.instance.clone(),
            runs,
            signals,
            links: SnapshotLinks {
                task_api: self.config.instance.task_api_address.clone(),
                status_api: self.config.instance.status_api_address.clone(),
                dashboard: self.config.instance.dashboard_address.clone(),
            },
        })
    }

    pub async fn run_summaries(
        &self,
        observed_at: DateTime<Utc>,
    ) -> sqlx::Result<Vec<RunObservationSummary>> {
        let rows: Vec<RunRow> = sqlx::query_as(
            r#"
            SELECT run_id, display_name, source_kind, source_ref, status, created_at, updated_at
            FROM runs
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut summaries = Vec::with_capacity(rows.len());
        for run in rows {
            let tasks = self.graph_task_rows(run.run_id).await?;
            summaries.push(summarize_run(run, &tasks, observed_at));
        }
        summaries.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));
        Ok(summaries)
    }

    pub async fn run_snapshot(&self, run_id: Uuid) -> sqlx::Result<Option<RunSnapshot>> {
        let observed_at = Utc::now();
        let Some(run_row) = self.run_row(run_id).await? else {
            return Ok(None);
        };
        let task_rows = self.graph_task_rows(run_id).await?;
        let run = summarize_run(run_row, &task_rows, observed_at);
        let tasks = observe_tasks(run_id, &task_rows, run.next_pending_task_id.as_deref());
        let recent_events = tasks
            .iter()
            .rev()
            .take(100)
            .map(task_event)
            .collect::<Vec<_>>();
        let signals = run.signals.clone();

        Ok(Some(RunSnapshot {
            schema_version: "0.1".to_string(),
            observed_at,
            instance: self.config.instance.clone(),
            run,
            tasks,
            recent_events,
            signals,
        }))
    }

    pub async fn task_snapshot(
        &self,
        run_id: Uuid,
        task_id: &str,
    ) -> sqlx::Result<Option<TaskSnapshot>> {
        let Some(run_snapshot) = self.run_snapshot(run_id).await? else {
            return Ok(None);
        };
        let Some(task) = run_snapshot
            .tasks
            .iter()
            .find(|task| task.task_id == task_id)
            .cloned()
        else {
            return Ok(None);
        };
        let recent_events = run_snapshot
            .recent_events
            .iter()
            .filter(|event| event.task_id == task_id)
            .cloned()
            .collect::<Vec<_>>();
        let signals = run_snapshot
            .signals
            .iter()
            .filter(|signal| signal.task_id.as_deref() == Some(task_id))
            .cloned()
            .collect::<Vec<_>>();

        Ok(Some(TaskSnapshot {
            schema_version: "0.1".to_string(),
            observed_at: run_snapshot.observed_at,
            instance: run_snapshot.instance,
            run: run_snapshot.run,
            task,
            recent_events,
            signals,
        }))
    }

    async fn run_row(&self, run_id: Uuid) -> sqlx::Result<Option<RunRow>> {
        sqlx::query_as(
            r#"
            SELECT run_id, display_name, source_kind, source_ref, status, created_at, updated_at
            FROM runs
            WHERE run_id = $1
            "#,
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn graph_task_rows(&self, run_id: Uuid) -> sqlx::Result<Vec<GraphTaskRow>> {
        sqlx::query_as(
            r#"
            SELECT task_id, task_order, task_data, updated_at
            FROM graph_tasks
            WHERE run_id = $1
            ORDER BY task_order ASC
            "#,
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
    }
}

fn summarize_run(
    run: RunRow,
    tasks: &[GraphTaskRow],
    observed_at: DateTime<Utc>,
) -> RunObservationSummary {
    let task_count = tasks.len() as i64;
    let mut status_counts = BTreeMap::new();
    let mut missing_status_count = 0_i64;
    let mut unknown_status_count = 0_i64;

    for task in tasks {
        let (status, _) = normalize_status(raw_status(&task.task_data));
        match status {
            ObservedStatus::Missing => missing_status_count += 1,
            ObservedStatus::Unknown => {
                unknown_status_count += 1;
                *status_counts
                    .entry(ObservedStatus::Unknown.as_str().to_string())
                    .or_insert(0) += 1;
            }
            _ => {
                *status_counts
                    .entry(status.as_str().to_string())
                    .or_insert(0) += 1;
            }
        }
    }

    let task_observations = observe_tasks(run.run_id, tasks, None);
    let next_pending_task_id = find_next_pending_task_id(&task_observations);
    let last_activity_at = tasks
        .iter()
        .map(|task| task.updated_at)
        .max()
        .unwrap_or(run.updated_at);
    let signals = run_signals(
        run.run_id,
        task_count,
        missing_status_count,
        unknown_status_count,
        next_pending_task_id.as_deref(),
        &task_observations,
        observed_at,
    );

    RunObservationSummary {
        run_id: run.run_id,
        display_name: run.display_name,
        source_kind: run.source_kind,
        source_ref: run.source_ref,
        status: run.status,
        task_count,
        status_counts,
        missing_status_count,
        unknown_status_count,
        next_pending_task_id,
        created_at: run.created_at,
        updated_at: run.updated_at,
        last_activity_at,
        signals,
    }
}

fn observe_tasks(
    run_id: Uuid,
    tasks: &[GraphTaskRow],
    next_pending_task_id: Option<&str>,
) -> Vec<TaskObservation> {
    let status_by_id = tasks
        .iter()
        .map(|task| {
            let (status, _) = normalize_status(raw_status(&task.task_data));
            (task.task_id.clone(), status.as_str().to_string())
        })
        .collect::<HashMap<_, _>>();
    let known_ids = tasks
        .iter()
        .map(|task| task.task_id.clone())
        .collect::<HashSet<_>>();

    let mut observations = Vec::with_capacity(tasks.len());
    for task in tasks {
        let raw = raw_status(&task.task_data);
        let (normalized, raw_value) = normalize_status(raw);
        let dependencies = dependencies(&task.task_data);
        let dependency_statuses = dependencies
            .iter()
            .map(|task_id| DependencyObservation {
                task_id: task_id.clone(),
                status: status_by_id.get(task_id).cloned(),
                exists: known_ids.contains(task_id),
            })
            .collect::<Vec<_>>();
        let dependencies_complete = dependency_statuses.iter().all(|dependency| {
            dependency.exists && dependency.status.as_deref() == Some("COMPLETED")
        });

        observations.push(TaskObservation {
            run_id,
            task_id: task.task_id.clone(),
            task_order: task.task_order,
            title: title(&task.task_data),
            status: StatusObservation {
                normalized: normalized.as_str().to_string(),
                raw: raw_value,
            },
            updated_at: task.updated_at,
            dependencies,
            dependency_statuses,
            dependencies_complete,
            is_next_pending: next_pending_task_id == Some(task.task_id.as_str()),
            raw_task: task.task_data.clone(),
        });
    }

    let next = next_pending_task_id
        .map(str::to_string)
        .or_else(|| find_next_pending_task_id(&observations));
    if let Some(next) = next {
        for task in &mut observations {
            task.is_next_pending = task.task_id == next;
        }
    }

    observations
}

fn normalize_status(raw: Option<&str>) -> (ObservedStatus, Option<String>) {
    let Some(raw) = raw else {
        return (ObservedStatus::Missing, None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (ObservedStatus::Missing, Some(raw.to_string()));
    }

    let status = match trimmed.to_ascii_lowercase().as_str() {
        "pending" => ObservedStatus::Pending,
        "ready" => ObservedStatus::Ready,
        "in_progress" | "in-progress" | "in progress" => ObservedStatus::InProgress,
        "evaluating" => ObservedStatus::Evaluating,
        "backoff" | "back_off" | "back-off" => ObservedStatus::Backoff,
        "complete" | "completed" => ObservedStatus::Completed,
        _ => ObservedStatus::Unknown,
    };
    (status, Some(raw.to_string()))
}

fn find_next_pending_task_id(tasks: &[TaskObservation]) -> Option<String> {
    tasks
        .iter()
        .find(|task| {
            matches!(task.status.normalized.as_str(), "PENDING" | "READY")
                && task.dependencies_complete
        })
        .map(|task| task.task_id.clone())
}

fn run_signals(
    run_id: Uuid,
    task_count: i64,
    missing_status_count: i64,
    unknown_status_count: i64,
    next_pending_task_id: Option<&str>,
    tasks: &[TaskObservation],
    observed_at: DateTime<Utc>,
) -> Vec<Signal> {
    let mut signals = Vec::new();

    if task_count == 0 {
        signals.push(signal(
            "info",
            "NO_TASKS",
            "Run has no graph tasks.",
            Some(run_id),
            None,
            observed_at,
        ));
    }

    if missing_status_count > 0 {
        signals.push(signal(
            "warning",
            "MISSING_STATUS",
            format!("{missing_status_count} task(s) have no status."),
            Some(run_id),
            None,
            observed_at,
        ));
    }

    if unknown_status_count > 0 {
        signals.push(signal(
            "warning",
            "UNKNOWN_STATUS",
            format!("{unknown_status_count} task(s) have unrecognized status values."),
            Some(run_id),
            None,
            observed_at,
        ));
    }

    let incomplete_count = tasks
        .iter()
        .filter(|task| task.status.normalized != "COMPLETED")
        .count();
    if incomplete_count > 0 && next_pending_task_id.is_none() {
        signals.push(signal(
            "watch",
            "INCOMPLETE_WITHOUT_NEXT_TASK",
            "Run has incomplete tasks but no dependency-complete PENDING or READY task.",
            Some(run_id),
            None,
            observed_at,
        ));
    }

    signals
}

fn signal(
    level: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
    run_id: Option<Uuid>,
    task_id: Option<String>,
    detected_at: DateTime<Utc>,
) -> Signal {
    Signal {
        level: level.into(),
        code: code.into(),
        message: message.into(),
        run_id,
        task_id,
        detected_at,
    }
}

fn task_event(task: &TaskObservation) -> TaskEventObservation {
    TaskEventObservation {
        kind: "task_snapshot".to_string(),
        task_id: task.task_id.clone(),
        status: task.status.normalized.clone(),
        updated_at: task.updated_at,
    }
}

fn raw_status(task: &Value) -> Option<&str> {
    task.get("status").and_then(Value::as_str)
}

fn title(task: &Value) -> Option<String> {
    task.get("title")
        .or_else(|| task.get("name"))
        .or_else(|| task.pointer("/payload/instruction"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn dependencies(task: &Value) -> Vec<String> {
    task.get("dependencies")
        .or_else(|| task.get("parent_ids"))
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

pub fn derive_next_bind_address(base: &str) -> Option<String> {
    let (host, port) = split_host_port(base)?;
    let next_port = port.checked_add(1)?;
    Some(format!("{host}:{next_port}"))
}

fn split_host_port(address: &str) -> Option<(&str, u16)> {
    let trimmed = address.trim();
    let (host, port) = trimmed.rsplit_once(':')?;
    if host.is_empty() {
        return None;
    }
    Some((host, port.parse().ok()?))
}

pub fn resolve_observe_bind_address(env_value: Option<&str>, fallback_base: &str) -> String {
    env_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| derive_next_bind_address(fallback_base))
        .unwrap_or_else(|| "127.0.0.1:6988".to_string())
}

pub fn http_origin(bind_address: &str) -> String {
    let mut address = bind_address.to_string();
    if let Some(port) = address.strip_prefix("0.0.0.0:") {
        address = format!("127.0.0.1:{port}");
    }
    format!("http://{address}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_limenet_status_names() {
        assert_eq!(normalize_status(Some("pending")).0, ObservedStatus::Pending);
        assert_eq!(normalize_status(Some("READY")).0, ObservedStatus::Ready);
        assert_eq!(
            normalize_status(Some("in_progress")).0,
            ObservedStatus::InProgress
        );
        assert_eq!(
            normalize_status(Some("complete")).0,
            ObservedStatus::Completed
        );
        assert_eq!(
            normalize_status(Some("completed")).0,
            ObservedStatus::Completed
        );
    }

    #[test]
    fn separates_missing_and_unknown_status() {
        assert_eq!(normalize_status(None).0, ObservedStatus::Missing);
        assert_eq!(normalize_status(Some("")).0, ObservedStatus::Missing);
        assert_eq!(normalize_status(Some("paused")).0, ObservedStatus::Unknown);
    }

    #[test]
    fn derives_next_bind_address_from_port() {
        assert_eq!(
            derive_next_bind_address("127.0.0.1:6987").as_deref(),
            Some("127.0.0.1:6988")
        );
    }

    #[test]
    fn resolve_observe_bind_prefers_explicit_value() {
        assert_eq!(
            resolve_observe_bind_address(Some("127.0.0.1:9000"), "127.0.0.1:6987"),
            "127.0.0.1:9000"
        );
    }
}
