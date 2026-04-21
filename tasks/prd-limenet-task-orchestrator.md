# PRD: Limenet Task Orchestrator MVP

## Introduction

Limenet is a high-concurrency private task orchestration hub for AI Agents (e.g., Hermes, Claw). Its core responsibility is managing collections of code tasks with complex prerequisite/post-requisite dependency relationships, and safely and efficiently distributing tasks among multiple concurrently executing Agents.

This PRD defines the Minimum Viable Product (MVP) for the Limenet backend API and core scheduling engine. It implements the full task state machine, DAG-based dependency resolution, atomic task claiming, lease management, and local shell-based validation — all without a frontend UI.

## Goals

- Implement the complete task state machine lifecycle (PENDING → READY → IN_PROGRESS → EVALUATING → COMPLETED / BACKOFF)
- Support batch ingestion of task graphs with cyclic dependency detection
- Implement atomic task claiming to prevent duplicate assignments under concurrent load
- Implement topological-level-based priority scheduling (upstream tasks execute first)
- Implement lease heartbeats, lease reaping, and automatic retry with exponential backoff
- Implement local shell execution of validation scripts and automatic state transition based on exit code
- Ensure zero terminal failure states — the system always remains active or self-healing

## User Stories

### US-001: Project Skeleton and Database Schema
**Description:** As a developer, I need a Rust project skeleton and PostgreSQL schema so that all task data can be persisted and queried atomically.

**Acceptance Criteria:**
- [ ] Initialize Rust project with Axum, sqlx, tokio, serde, uuid, chrono dependencies
- [ ] Create `tasks` table schema with all required fields:
  - `task_id` (UUID, PRIMARY KEY)
  - `status` (VARCHAR: PENDING, READY, IN_PROGRESS, EVALUATING, BACKOFF, COMPLETED)
  - `parent_ids` (UUID[])
  - `child_ids` (UUID[])
  - `payload` (JSONB: instruction, context_paths, validation_script)
  - `lease` (JSONB: agent_id, expires_at)
  - `retry_logic` (JSONB: attempt_count, backoff_until)
  - `topological_level` (INT, default 0)
  - `created_at`, `updated_at` (TIMESTAMPTZ)
- [ ] Create sqlx migration files (e.g., `migrations/001_initial_schema.sql`)
- [ ] Run `sqlx migrate run` successfully against PostgreSQL
- [ ] `cargo check` passes with zero errors

### US-002: Ingest Task Graph API with DAG Validation
**Description:** As an Agent operator, I want to submit a batch of interdependent tasks so that Limenet can manage their execution order and detect invalid cycles.

**Acceptance Criteria:**
- [ ] `POST /api/v1/tasks/batch` accepts a JSON array of tasks, each with `task_id`, `parent_ids`, `child_ids`, and `payload`
- [ ] Endpoint detects circular dependencies using DFS or Kahn's algorithm; returns `400 Bad Request` with a clear error message if a cycle is found
- [ ] Computes `topological_level` for each task: `0` for root nodes (no parents), `max(parent_levels) + 1` for all others
- [ ] Tasks with no `parent_ids` are inserted with status `READY`; all others with status `PENDING`
- [ ] Returns `201 Created` with a list of created `task_id`s
- [ ] All database operations are wrapped in a single SQL transaction
- [ ] `cargo check` passes

### US-003: Atomic Task Claim API
**Description:** As an AI Agent, I want to atomically claim the highest-priority available task so that multiple agents never receive the same task.

**Acceptance Criteria:**
- [ ] `POST /api/v1/tasks/claim` accepts JSON body `{"agent_id": "...", "capabilities": ["..."]}` (capabilities parsed but not filtered in MVP)
- [ ] Query uses `SELECT ... FROM tasks WHERE status = 'READY' ORDER BY topological_level ASC, created_at ASC FOR UPDATE SKIP LOCKED LIMIT 1` for atomic, contention-free locking
- [ ] On successful claim, updates `status` to `IN_PROGRESS`, sets `lease.agent_id`, and sets `lease.expires_at` to `NOW() + 15 minutes`
- [ ] Returns `200 OK` with the full task payload (including `task_id`, `payload`, `parent_ids`, etc.)
- [ ] Returns `204 No Content` if no `READY` tasks exist
- [ ] `cargo check` passes

### US-004: Heartbeat Lease Renewal API
**Description:** As an AI Agent, I want to periodically renew my task lease so that long-running tasks are not reclaimed mid-execution.

**Acceptance Criteria:**
- [ ] `POST /api/v1/tasks/{task_id}/heartbeat` accepts JSON body `{"agent_id": "..."}`
- [ ] Verifies that the task exists, `status = IN_PROGRESS`, and `lease.agent_id` matches the provided `agent_id`
- [ ] Updates `lease.expires_at` to `NOW() + 15 minutes`
- [ ] Returns `200 OK` on success
- [ ] Returns `409 Conflict` if `agent_id` does not match the current lease
- [ ] Returns `404 Not Found` if the task does not exist
- [ ] `cargo check` passes

### US-005: Task Submission, Validation Execution, and State Transition
**Description:** As an AI Agent, I want to submit task results and trigger validation so that Limenet can determine if the task is complete or needs to retry.

**Acceptance Criteria:**
- [ ] `POST /api/v1/tasks/{task_id}/submit` accepts JSON body `{"agent_id": "...", "result_summary": "...", "files_changed": ["..."]}`
- [ ] Verifies that `agent_id` matches the current `lease.agent_id` and `status = IN_PROGRESS`
- [ ] Atomically transitions `status` from `IN_PROGRESS` to `EVALUATING`
- [ ] Asynchronously executes `payload.validation_script` via `tokio::process::Command` in the local shell (non-blocking to the HTTP response)
- [ ] On validation exit code `0`: transitions `status` to `COMPLETED`, clears `lease`, and triggers downstream dependency resolution
- [ ] On validation non-zero exit: transitions `status` to `BACKOFF`, increments `retry_logic.attempt_count`, and sets `retry_logic.backoff_until` to `NOW() + min(2^attempt_count * 2 minutes, 30 minutes)`
- [ ] Returns `202 Accepted` immediately (validation runs asynchronously after response)
- [ ] `cargo check` passes

### US-006: Dependency Resolver Background Daemon
**Description:** As the system, I want to automatically unlock downstream tasks when a parent completes so that the DAG flows forward without global polling.

**Acceptance Criteria:**
- [ ] A background tokio task runs continuously alongside the HTTP server
- [ ] When a task transitions to `COMPLETED`, the resolver inspects that task's `child_ids`
- [ ] For each child task, it queries whether **all** tasks in the child's `parent_ids` have `status = COMPLETED`
- [ ] If the condition is met and the child's current status is `PENDING`, it atomically updates the child's status to `READY`
- [ ] Uses a lightweight in-application channel or direct DB trigger logic to react to completions within 5 seconds
- [ ] `cargo check` passes

### US-007: Lease Reaper Background Daemon
**Description:** As the system, I want to reclaim tasks from unresponsive agents so that work does not stall indefinitely.

**Acceptance Criteria:**
- [ ] A background tokio task wakes up every `60 seconds`
- [ ] Queries for tasks where `status = 'IN_PROGRESS'` and `lease->>'expires_at'` is in the past
- [ ] Atomically resets `status` to `READY`, clears `lease.agent_id`, and clears `lease.expires_at`
- [ ] Increments `retry_logic.attempt_count` for each reclaimed task
- [ ] Logs each reclamation event with `task_id` and the previous `agent_id`
- [ ] `cargo check` passes

### US-008: Backoff Awakener Background Daemon
**Description:** As the system, I want to re-queue cooled-off tasks so that transient failures (e.g., API rate limits, flaky tests) are retried automatically.

**Acceptance Criteria:**
- [ ] A background tokio task wakes up every `30 seconds`
- [ ] Queries for tasks where `status = 'BACKOFF'` and `retry_logic->>'backoff_until'` is in the past
- [ ] Atomically updates `status` to `READY`
- [ ] Logs each awakening event with `task_id` and current `attempt_count`
- [ ] `cargo check` passes

## Functional Requirements

- **FR-1:** `POST /api/v1/tasks/batch` ingests a task graph, validates DAG acyclicity, computes `topological_level` for each node, and persists tasks in a single transaction. Root nodes become `READY`; all others become `PENDING`.
- **FR-2:** `POST /api/v1/tasks/claim` atomically assigns a `READY` task to an Agent using `SELECT ... FOR UPDATE SKIP LOCKED`. Priority ordering is: lowest `topological_level` first, then oldest `created_at`.
- **FR-3:** `POST /api/v1/tasks/{task_id}/heartbeat` verifies Agent lease ownership and extends `lease.expires_at` by `15 minutes` from the current time.
- **FR-4:** `POST /api/v1/tasks/{task_id}/submit` verifies Agent lease ownership and `IN_PROGRESS` status, transitions the task to `EVALUATING`, and spawns an asynchronous subprocess to execute `payload.validation_script`.
- **FR-5:** On validation subprocess exit code `0`, the system transitions the task to `COMPLETED`, clears lease data, and triggers downstream dependency resolution.
- **FR-6:** On validation subprocess non-zero exit, the system transitions the task to `BACKOFF`, increments `retry_logic.attempt_count`, and sets `retry_logic.backoff_until` using exponential backoff: `min(2^attempt_count * 2 minutes, 30 minutes)`.
- **FR-7:** The Lease Reaper daemon scans every `60 seconds` for expired leases (`IN_PROGRESS` with `lease.expires_at < NOW()`) and resets them to `READY`, clearing lease data and incrementing the attempt count.
- **FR-8:** The Backoff Awakener daemon scans every `30 seconds` for tasks in `BACKOFF` whose `backoff_until` has passed, and transitions them to `READY`.
- **FR-9:** The Dependency Resolver ensures a `PENDING` task transitions to `READY` **only when all** tasks in its `parent_ids` are `COMPLETED`.
- **FR-10:** No authentication, authorization, or TLS termination is required on any endpoint; the system assumes a trusted intranet environment.

## Non-Goals (Out of Scope)

- **No Web UI / Dashboard:** This MVP is a pure backend API service. There is no frontend, admin panel, or visual DAG viewer.
- **No Agent Capability Matching:** The `capabilities` field in the claim request is accepted but ignored. All agents compete for the same task pool.
- **No Distributed Deployment:** The MVP targets a single server running the Rust binary and a local PostgreSQL instance. No clustering, no load balancer logic.
- **No Containerized Sandbox:** Validation scripts execute directly on the host shell via `tokio::process::Command`. There is no Docker, Firecracker, or other isolation.
- **No Manual Task Controls:** No endpoints for admin cancellation, pausing, force-completing, or manually editing a task graph after ingestion.
- **No Dynamic Graph Mutation:** Tasks cannot be added, removed, or re-wired after the initial batch ingest.
- **No Persistent Artifact Storage:** Task outputs, logs, and `files_changed` are stored only in the task row (or memory) and are not uploaded to S3, Git, or similar.
- **No Notification System:** No Webhooks, Slack messages, or email alerts on task completion or failure.

## Design Considerations

- **API Style:** RESTful JSON. All error responses use a consistent envelope: `{"error": "descriptive message"}`.
- **IDs:** UUID v4 for all `task_id` values, generated by the client at ingest time.
- **Timestamps:** All timestamps stored and compared in UTC.
- **Lease Duration:** Default `15 minutes`, overridable via the `LEASE_DURATION_MINUTES` environment variable.
- **Backoff Formula:** Exponential with ceiling: `backoff_seconds = min(2^attempt_count * 120, 1800)`.
- **State Machine Enforcement:** Status transitions are strict. Illegal transitions (e.g., `PENDING` → `COMPLETED`) must be rejected with `409 Conflict`.

## Technical Considerations

- **Language:** Rust (Edition 2021)
- **Web Framework:** Axum (modular, Tower-native, excellent async support)
- **Database:** PostgreSQL 14+ (required for `SKIP LOCKED`, JSONB operators, and array types)
- **SQL Toolkit:** sqlx with compile-time checked queries and built-in migration management
- **Async Runtime:** Tokio with the multi-thread scheduler (`tokio::main`)
- **Process Execution:** `tokio::process::Command` for non-blocking shell invocation of validation scripts
- **Background Scheduling:** `tokio::time::interval` for the three daemon loops (Dependency Resolver, Lease Reaper, Backoff Awakener)
- **Configuration:** Environment-variable based (e.g., `DATABASE_URL`, `LISTEN_ADDR`, `LEASE_DURATION_MINUTES`, `REAPER_INTERVAL_SECS`)
- **No External Message Broker:** PostgreSQL `SKIP LOCKED` serves as the distributed-safe work queue. No Redis or RabbitMQ is required for the MVP.

## Success Metrics

- **Ingest Performance:** Batch ingestion of `1000` tasks completes in under `5 seconds` on standard hardware.
- **Atomic Claim Safety:** Under a test with `10` concurrent agents repeatedly claiming tasks, zero instances of double-assignment occur.
- **Lease Recovery:** The Lease Reaper correctly recovers `100%` of timed-out tasks within `60 seconds` of expiration.
- **Validation Turnaround:** The state transition (`EVALUATING` → `COMPLETED` or `BACKOFF`) is reflected in the database within `5 seconds` of the validation script exiting.
- **Zero Terminal States:** At no point in normal operation does a task enter a `FAILED` or `BLOCKED` state. Only the defined six states are used.

## Open Questions

- Should validation script execution have a hard timeout (e.g., `10 minutes`) to prevent runaway or hanging processes from permanently occupying the `EVALUATING` state?
- Should there be a maximum `attempt_count` ceiling, or does the "no terminal failure" principle imply infinite retries?
- How should the system handle validation scripts that produce large volumes of `stdout`/`stderr`? Should output be captured, truncated, streamed to logs, or discarded?
