# LimeNet Observe Dashboard PRD

## Purpose

LimeNet needs a read-only observation surface for humans and AI agents. The first version focuses on the task progress that meta-agent already stores through the run-scoped graph task API.

The dashboard observes task progress. It does not mutate tasks, recover tasks, claim work, or inspect the native worker task table.

## Ports

Default ports use a less common local range:

| Surface | Default bind | Purpose |
| --- | --- | --- |
| Task API | `127.0.0.1:6987` | Existing LimeNet API used by meta-agent and clients |
| Status API | `127.0.0.1:6988` | Read-only JSON snapshots for AI agents and scripts |
| Dashboard UI | `127.0.0.1:6989` | Human-readable browser dashboard |

Overrides:

- `LIMENET_BIND`
- `LIMENET_STATUS_BIND`
- `LIMENET_DASHBOARD_BIND`

When the status or dashboard bind address is not set, it is derived from the task API port. Status uses task port + 1. Dashboard uses status port + 1.

## Data Source

Version 1 uses only Graph task data:

- `runs`
- `graph_tasks`
- existing run-scoped graph semantics

The native worker task endpoints are explicitly out of scope for v1:

- `POST /api/v1/tasks/batch`
- `POST /api/v1/tasks/claim`
- `POST /api/v1/tasks/{task_id}/heartbeat`
- `POST /api/v1/tasks/{task_id}/submit`

## Status Model

Observation normalizes task status to LimeNet native status names:

- `PENDING`
- `READY`
- `IN_PROGRESS`
- `EVALUATING`
- `BACKOFF`
- `COMPLETED`
- `UNKNOWN`
- `MISSING`

Compatibility mappings:

- `pending` -> `PENDING`
- `ready` -> `READY`
- `in_progress` -> `IN_PROGRESS`
- `evaluating` -> `EVALUATING`
- `backoff` -> `BACKOFF`
- `complete` / `completed` -> `COMPLETED`

Missing status is reported as `MISSING`. Unrecognized status is reported as `UNKNOWN` and preserves the raw value.

## JSON Surfaces

`GET /status.json`

Returns a global lightweight snapshot. It includes run summaries, normalized status counts, and signals. It does not expose full task payloads.

`GET /runs.json`

Returns all observed run summaries.

`GET /runs/{run_id}/snapshot.json`

Returns the complete observation snapshot for one run, including ordered tasks and raw graph task JSON.

`GET /runs/{run_id}/tasks/{task_id}/snapshot.json`

Returns one task observation with run context, inferred dependency state, and raw graph task JSON.

## Dashboard UI

The dashboard shows all runs. It does not require a selected run as the primary page state.

Layout:

1. Instance and port identity bar.
2. Runs sorted by latest activity.
3. Each run contains summary counts, signals, ordered tasks, and recent timeline.
4. Clicking a task opens a detail drawer for that task.

Task detail includes:

- `run_id`
- `task_id`
- title
- normalized status
- raw status
- `updated_at`
- dependencies
- whether dependencies are complete
- whether the task is the next pending task
- raw graph task JSON

## Signals

Signals are conservative in v1. There is no hard stale risk based only on elapsed time.

Signals:

- `MISSING_STATUS`
- `UNKNOWN_STATUS`
- `INCOMPLETE_WITHOUT_NEXT_TASK`
- `NO_TASKS`

Elapsed idle time may be displayed as context, but it is not a high-confidence risk by itself.
