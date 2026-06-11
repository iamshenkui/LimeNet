# LimeNet API

## Base URL

默认本地地址：

```text
http://127.0.0.1:6987
```

## `GET /health`

暴露当前 LimeNet 实例的最小身份标识，供运维人员确认客户端连接的是预期实例。

### Response

- `200 OK`：返回实例身份 JSON

```json
{
  "status": "healthy",
  "instance_id": "local-task",
  "database_target": "localhost:5432/limenet_local",
  "bind_address": "127.0.0.1:6987"
}
```

字段说明：

- `status`：固定为 `"healthy"`，表示服务正在运行
- `instance_id`：实例标识符，通过环境变量 `LIMENET_INSTANCE_ID` 配置，默认值为 `"default"`
- `database_target`：已脱敏的数据库目标地址（不含密码），来自 `DATABASE_URL`
- `bind_address`：当前服务实际监听的地址和端口

### 用途

运维人员可以通过此端点区分本地任务后端和共享/全局 LimeNet 实例，避免将 meta-agent 指向错误的环境。

## Observe Status API

观察 API 是只读状态快照，默认监听 `http://127.0.0.1:6988`。它只观察 run-scoped Graph task 数据，不读取 native worker task 表。

### `GET /status.json`

返回全局轻量摘要，适合 AI agent 或脚本一次读取当前实例、run 列表、状态聚合与观察信号。该接口不返回完整 task payload。

### `GET /runs.json`

返回所有 run 的观察摘要。

### `GET /runs/{run_id}/snapshot.json`

返回单个 run 的完整观察快照，包括 ordered tasks、recent events、signals 与 raw graph task JSON。

### `GET /runs/{run_id}/tasks/{task_id}/snapshot.json`

返回单个 task 的观察详情，包括依赖状态、下游 dependents、是否为 next pending task，以及 raw graph task JSON。

状态会归一化为 LimeNet 原生状态名：

```text
PENDING
READY
IN_PROGRESS
EVALUATING
BACKOFF
COMPLETED
UNKNOWN
MISSING
```

## Meta-agent Graph Task Compatibility API

以下接口服务于 `meta-agent` 的 `TaskGraphBackend` / `LocalLimeNetStateBackend` 兼容层。它们保存的是 meta-agent 的完整 task graph payload，使用字符串 `task_id`，并与 LimeNet 原生 UUID worker task API 分开存储。

未带 `run_id` 的兼容接口会映射到固定 default run：`00000000-0000-0000-0000-000000000000`。新客户端应优先使用下面的 run-scoped API，避免多个任务图在同一个服务内互相覆盖。

## Run-scoped Graph Task API

### `POST /api/v1/runs`

创建一个 run namespace。未提供 `run_id` 时由 LimeNet 生成 UUID。
如果请求提供了已存在的 `run_id`，LimeNet 返回已有 run，HTTP status 为 `200 OK`；
新建成功时 HTTP status 为 `201 Created`。

```json
{
  "display_name": "PORT-24H-011",
  "source_kind": "task",
  "source_ref": "PORT-24H-011",
  "metadata": {}
}
```

Response:

```json
{
  "run_id": "11111111-1111-1111-1111-111111111111",
  "display_name": "PORT-24H-011",
  "source_kind": "task",
  "source_ref": "PORT-24H-011",
  "status": "active",
  "metadata": {},
  "created_at": "2026-05-29T00:00:00Z",
  "updated_at": "2026-05-29T00:00:00Z"
}
```

### `GET /api/v1/runs`

返回所有 run 的 dashboard 列表投影。

### `GET /api/v1/runs/{run_id}/summary`

返回单个 run 的 task 数量和状态聚合。默认不计算下一个可运行 task，避免 dashboard
轮询时全量加载任务图；需要时可传 `?include_next=true`。
`status_counts` 只统计 payload 中显式存在的 status；缺失或 JSON null 的 status 会统计到
`missing_status_count`，避免和真实的 `"unknown"` status 混淆。

Response:

```json
{
  "run_id": "11111111-1111-1111-1111-111111111111",
  "task_count": 3,
  "status_counts": {
    "pending": 1,
    "complete": 1
  },
  "missing_status_count": 1,
  "next_pending_task_id": null,
  "updated_at": "2026-05-29T00:00:00Z"
}
```

### `GET /api/v1/runs/{run_id}/timeline`

返回单个 run 的 task 最新状态快照，按 `updated_at` 倒序排列。当前不是完整事件流；
同一个 task 多次变更只会返回最新一条快照，事件 `kind` 为 `"task_snapshot"`。

### Scoped graph endpoints

以下 endpoint 与兼容 API 的行为一致，但所有读写都限制在指定 `run_id` 内：

```text
GET  /api/v1/runs/{run_id}/graph/tasks
POST /api/v1/runs/{run_id}/graph/tasks
GET  /api/v1/runs/{run_id}/graph/tasks/{task_id}
PUT  /api/v1/runs/{run_id}/graph/tasks/{task_id}
POST /api/v1/runs/{run_id}/graph/tasks/insert
POST /api/v1/runs/{run_id}/graph/tasks/next_pending
POST /api/v1/runs/{run_id}/graph/tasks/recover
```

Scoped task get 未找到 task 时返回 `404 Not Found`。

Scoped `next_pending` 有可运行任务时返回：

```json
{
  "run_id": "11111111-1111-1111-1111-111111111111",
  "task": {
    "task_id": "US-002",
    "title": "Next task",
    "status": "pending"
  }
}
```

无可运行任务时仍保留 `task` 字段，值为 `null`：

```json
{
  "run_id": "11111111-1111-1111-1111-111111111111",
  "task": null
}
```

### `GET /api/v1/graph/tasks`

按 `task_order ASC` 返回当前 meta-agent task graph。

Response:

```json
{
  "tasks": [
    {
      "task_id": "US-001",
      "title": "Implement task backend",
      "status": "pending",
      "dependencies": []
    }
  ]
}
```

### `POST /api/v1/graph/tasks`

整体替换当前 meta-agent task graph。请求体形状与 `GET /api/v1/graph/tasks` 一致：

```json
{
  "tasks": [
    {
      "task_id": "US-001",
      "title": "Implement task backend",
      "status": "pending",
      "dependencies": []
    }
  ]
}
```

### `GET /api/v1/graph/tasks/{task_id}`

返回单个 meta-agent task payload。未找到时返回：

```json
{ "status": "not_found" }
```

### `PUT /api/v1/graph/tasks/{task_id}`

插入或更新单个 meta-agent task payload。新任务会追加到当前 graph 末尾；已有任务保持原 `task_order`。

### `POST /api/v1/graph/tasks/insert`

在指定 task 后插入一组新任务。

```json
{
  "anchor_task_id": "US-001",
  "tasks": [
    {
      "task_id": "US-001-a",
      "title": "Part A",
      "status": "pending",
      "dependencies": ["US-001"]
    }
  ]
}
```

### `POST /api/v1/graph/tasks/next_pending`

返回下一个可运行的 `pending` task。依赖项必须全部为 `complete`，排序规则为 priority 降序、原 graph 顺序升序。

```json
{
  "exclude_task_ids": ["US-001"]
}
```

Response:

```json
{
  "task": {
    "task_id": "US-002",
    "title": "Next task",
    "status": "pending"
  }
}
```

无可运行任务时返回空对象：

```json
{}
```

### `POST /api/v1/graph/tasks/recover`

将所有 `in_progress` task 恢复为 `pending`。

```json
{ "recovered_count": 1 }
```

## `POST /api/v1/tasks/batch`

批量写入任务图。

### Request

```json
[
  {
    "task_id": "11111111-1111-1111-1111-111111111111",
    "parent_ids": [],
    "child_ids": ["22222222-2222-2222-2222-222222222222"],
    "payload": {
      "instruction": "实现数据库连接池",
      "context_paths": ["src/db.rs"],
      "validation_script": "cargo test"
    },
    "metadata": {
      "task_kind": "implementation",
      "executor_role": "meta-agent",
      "target_ref": {
        "repo": "owner/repo",
        "base_branch": "main",
        "head_branch": "codex/task-branch"
      },
      "artifacts": {
        "inputs": ["plan-1"],
        "outputs": []
      },
      "goal": "preserved generic metadata"
    }
  },
  {
    "task_id": "22222222-2222-2222-2222-222222222222",
    "parent_ids": ["11111111-1111-1111-1111-111111111111"],
    "child_ids": [],
    "payload": {
      "instruction": "补充 API 层集成",
      "context_paths": ["src/main.rs"],
      "validation_script": "cargo test"
    }
  }
]
```

### Behavior

- 校验任务图是否存在环
- 计算每个任务的 `topological_level`
- 无父节点任务初始化为 `READY`
- 其他任务初始化为 `PENDING`
- `metadata` 是 JSONB；`task_kind`、`executor_role`、`target_ref`、`artifacts` 是可选治理字段，其他通用 metadata key 会原样保留

### Response

- `201 Created`: 返回创建的任务 ID 列表
- `400 Bad Request`: 任务图存在循环依赖

## `GET /api/v1/tasks`

只读列出 native worker task queue 中的任务，用于 worker/resident 做 queue snapshot
或无副作用地 peek 下一个可执行任务。

### Query

- `status`：可选，按 native task status 过滤，例如 `READY`。
- `task_kind`：可选，按 `metadata.task_kind` 过滤。
- `executor_role`：可选，按 `metadata.executor_role` 过滤。
- `limit`：可选，默认 `100`，最大 `500`。

### Response

```json
{
  "tasks": [
    {
      "task_id": "11111111-1111-1111-1111-111111111111",
      "status": "READY",
      "parent_ids": [],
      "child_ids": [],
      "payload": {
        "instruction": "实现数据库连接池",
        "context_paths": ["src/db.rs"],
        "validation_script": "cargo test"
      },
      "metadata": {
        "task_kind": "implementation",
        "executor_role": "meta-agent"
      }
    }
  ]
}
```

## `POST /api/v1/tasks/claim`

原子申领一个 `READY` 任务。

### Request

```json
{
  "agent_id": "hermes-local-01",
  "task_id": "11111111-1111-1111-1111-111111111111",
  "capabilities": ["coding", "rust"],
  "task_kind": "code_review",
  "executor_role": "quartermaster"
}
```

### Behavior

- 仅从 `READY` 任务池挑选
- 如果传入 `task_id`，仅申领该任务；不传时按筛选条件选择下一条 READY task
- 如果传入 `task_kind`，仅申领 `metadata.task_kind` 匹配的任务
- 如果传入 `executor_role`，仅申领 `metadata.executor_role` 匹配的任务
- 按 `topological_level ASC, created_at ASC` 排序
- 使用 `FOR UPDATE SKIP LOCKED`
- 成功后写入 15 分钟租约并转为 `IN_PROGRESS`

### Response

- `200 OK`: 返回完整任务对象
- `204 No Content`: 当前无可用任务

## `POST /api/v1/governance/artifacts`

写入或覆盖一个治理 artifact。

### Request

```json
{
  "artifact_id": "quartermaster-review-1",
  "artifact_kind": "quartermaster_checkpoint_review",
  "task_id": "11111111-1111-1111-1111-111111111111",
  "repo": "owner/repo",
  "base_sha": "base",
  "head_sha": "head",
  "producer_role": "quartermaster",
  "created_at": "2026-06-09T00:00:00Z",
  "payload": {
    "decision": "continue"
  }
}
```

### Response

- `200 OK`: 返回写入后的 artifact
- `400 Bad Request`: artifact kind、producer role 或 task id 无效

## `GET /api/v1/governance/artifacts/{artifact_id}`

读取治理 artifact。

### Response

- `200 OK`: 返回完整 artifact
- `404 Not Found`: artifact 不存在

## `POST /api/v1/tasks/{task_id}/heartbeat`

续租一个正在执行中的任务。

### Request

```json
{
  "agent_id": "hermes-local-01"
}
```

### Behavior

- 校验任务存在
- 校验任务处于 `IN_PROGRESS`
- 校验租约持有者匹配
- 将租约延长到当前时间之后 15 分钟

### Response

- `200 OK`: 续租成功
- `404 Not Found`: 任务不存在，或当前不在 `IN_PROGRESS`
- `409 Conflict`: `agent_id` 与当前租约不匹配

## `POST /api/v1/tasks/{task_id}/submit`

提交任务结果并触发异步校验。

### Request

```json
{
  "agent_id": "hermes-local-01",
  "result_summary": "implemented retry logic",
  "files_changed": ["src/network.rs"]
}
```

### Behavior

- 锁定目标任务
- 校验状态必须为 `IN_PROGRESS`
- 校验租约归属
- 先将任务转为 `EVALUATING`
- 异步执行 `payload.validation_script`

### Validation Result

- 校验成功：任务转为 `COMPLETED`
- 校验失败：任务转为 `BACKOFF`，并增加重试次数

### Response

- `202 Accepted`: 已进入异步校验阶段
- `404 Not Found`: 任务不存在
- `409 Conflict`: 任务状态不允许提交
- `403 Forbidden`: 当前 Agent 不是租约持有者

## Hermes Integration Notes

对于 Hermes 这类外部 worker / control-plane 客户端，当前推荐的最小交互顺序是：

1. 使用 `POST /api/v1/tasks/batch` 写入一个任务图
2. worker 使用 `POST /api/v1/tasks/claim` 申领 `READY` 任务
3. 长任务周期性调用 `POST /api/v1/tasks/{task_id}/heartbeat` 续租
4. 完成实现后调用 `POST /api/v1/tasks/{task_id}/submit` 进入 `EVALUATING`
5. 在 `EVALUATING` 阶段调用 Quartermaster，根据 review verdict 决定完成、重试或拆分

补充说明：

- 文档中的 `{task_id}` 是路径参数占位符，实际请求时需要替换成具体 UUID
- `capabilities` 字段已经接收，但当前实现尚未参与筛选逻辑
- LimeNet 负责任务状态与租约语义，Quartermaster 只负责返回 review verdict

## 响应说明

当前实现里，错误响应主要返回 HTTP 状态码，部分路径会附带：

```json
{
  "error": "descriptive message"
}
```

如果要对外提供更稳定的客户端契约，建议后续统一错误结构。
