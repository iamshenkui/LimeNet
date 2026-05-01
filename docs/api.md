# LimeNet API

## Base URL

默认本地地址：

```text
http://127.0.0.1:3000
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

### Response

- `201 Created`: 返回创建的任务 ID 列表
- `400 Bad Request`: 任务图存在循环依赖

## `POST /api/v1/tasks/claim`

原子申领一个 `READY` 任务。

### Request

```json
{
  "agent_id": "hermes-local-01",
  "capabilities": ["coding", "rust"]
}
```

### Behavior

- 仅从 `READY` 任务池挑选
- 按 `topological_level ASC, created_at ASC` 排序
- 使用 `FOR UPDATE SKIP LOCKED`
- 成功后写入 15 分钟租约并转为 `IN_PROGRESS`

### Response

- `200 OK`: 返回完整任务对象
- `204 No Content`: 当前无可用任务

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
