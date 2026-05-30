# LimeNet Operations

## 数据库

初始化 migration 位于 `migrations/20260422000001_initial_schema.sql`，主要包含：

- `tasks` 表
- `status` CHECK 约束
- `READY` 热路径索引
- 过期租约索引
- 退避唤醒索引
- `parent_ids` 的 GIN 索引
- 自动维护 `updated_at` 的 trigger

## 本地运行

### 1. 准备数据库

确保本地有 PostgreSQL，并创建可访问的数据库。

`DATABASE_URL` **必须显式设置**，不存在静默回退默认值。这样可以在同一台机器或同一集群中安全运行多个 LimeNet 实例，每个实例指向独立的数据库，避免误操作共享数据。

```bash
export DATABASE_URL=postgres://<user>:<password>@localhost:5432/<db>
```

如果未设置，启动会立即失败并给出明确提示：

```text
DATABASE_URL is not set. Set it explicitly, e.g. DATABASE_URL=postgres://user@localhost:5432/limenet
```

启动日志会打印解析后的数据库目标（已脱敏，不含密码）：

```text
LimeNet connecting to database localhost:5432/limenet...
LimeNet status API starting on 127.0.0.1:6988...
LimeNet dashboard starting on 127.0.0.1:6989...
LimeNet task orchestrator starting on 127.0.0.1:6987...
```

### 2. 执行迁移

```bash
sqlx migrate run
```

### 3. 启动服务

```bash
cargo run
```

默认监听：

```text
127.0.0.1:6987
```

默认还会启动两个只读观察端口：

```text
Status JSON: 127.0.0.1:6988
Dashboard:   127.0.0.1:6989
```

### 4. 修改监听地址或端口

通过环境变量 `LIMENET_BIND` 可以自定义监听地址和端口：

```bash
export LIMENET_BIND=127.0.0.1:8080
cargo run
```

观察端口可以显式覆盖：

```bash
export LIMENET_STATUS_BIND=127.0.0.1:8081
export LIMENET_DASHBOARD_BIND=127.0.0.1:8082
cargo run
```

如果未设置，状态端口会从任务端口 + 1 推导，dashboard 端口会从状态端口 + 1 推导。

启动日志会打印实际解析后的地址，例如：

```text
LimeNet task orchestrator starting on 127.0.0.1:8080...
```

### 5. 在同一台机器上运行多个 run

推荐的新路径是：一个 LimeNet 服务端口承载多个 run，每个 run 用独立 `run_id` 隔离任务图。

```bash
curl -fsS -X POST http://127.0.0.1:6987/api/v1/runs \
  -H 'Content-Type: application/json' \
  --data '{"display_name":"PORT-24H-011","source_kind":"task","source_ref":"PORT-24H-011"}'
```

随后使用 run-scoped graph API：

```bash
curl -fsS http://127.0.0.1:6987/api/v1/runs/<run_id>/summary
```

这种方式不需要为每个并行任务分配新端口或新数据库，适合后续 dashboard 从同一个 base URL 汇总所有 run。

### 6. 兼容：在同一台机器上运行多个实例

每个 LimeNet 实例需要 **独立的 `DATABASE_URL` 数据库** 和独立的 `LIMENET_BIND` 端口，避免数据混用。

这是历史兼容路径。只有在需要进程级或数据库级硬隔离时才建议继续使用；一般并行任务优先使用 run-scoped API。

推荐做法是为不同用途创建独立的数据库：

```bash
# 1) 在 PostgreSQL 中创建两个数据库
psql -c "CREATE DATABASE limenet_local;"
psql -c "CREATE DATABASE limenet_shared;"
```

```bash
# 本地任务实例（本地开发、CI 测试）
export DATABASE_URL=postgres://chenhui@localhost:5432/limenet_local
export LIMENET_BIND=127.0.0.1:6987
cargo run
```

```bash
# 共享任务实例（团队共享的 staging / prod）
export DATABASE_URL=postgres://chenhui@localhost:5432/limenet_shared
export LIMENET_BIND=0.0.0.0:3001
cargo run
```

显式设置 `DATABASE_URL` 能确保不会出现以下误操作：
- 本地开发时不小心写入共享数据库
- CI 测试用例污染生产数据
- 多个实例竞争同一套任务表

### 7. 配置实例身份标识

通过环境变量 `LIMENET_INSTANCE_ID` 可以为每个 LimeNet 实例设置一个可读的身份标识，便于运维区分本地任务后端和共享实例：

```bash
# 本地开发实例
export LIMENET_INSTANCE_ID=local-task
export DATABASE_URL=postgres://chenhui@localhost:5432/limenet_local
export LIMENET_BIND=127.0.0.1:6987
cargo run
```

```bash
# 共享 staging 实例
export LIMENET_INSTANCE_ID=shared-staging
export DATABASE_URL=postgres://chenhui@localhost:5432/limenet_shared
export LIMENET_BIND=0.0.0.0:3001
cargo run
```

如果不设置 `LIMENET_INSTANCE_ID`，默认值为 `"default"`。空白值会被自动回退到默认值，避免意外导出空字符串导致身份丢失。

### 8. 验证当前连接的实例身份

启动后访问 `/health` 端点即可确认当前实例的身份：

```bash
curl http://127.0.0.1:6987/health
```

示例输出：

```json
{
  "status": "healthy",
  "instance_id": "local-task",
  "database_target": "localhost:5432/limenet_local",
  "bind_address": "127.0.0.1:6987"
}
```

运维人员应将 `/health` 作为连接后的第一个检查步骤，确保 `instance_id` 和 `database_target` 与预期一致，防止 meta-agent 或其他客户端指向错误的 LimeNet 实例。

如果需要进一步隔离，也可以在同一个数据库中使用不同的 schema：

```bash
export DATABASE_URL=postgres://chenhui@localhost:5432/limenet?options=-csearch_path%3Dlocal_tasks
```

## 后台任务间隔

- `DependencyResolver`: 每 2 秒扫描一次
- `LeaseReaper`: 每 60 秒扫描一次
- `BackoffAwakener`: 每 30 秒扫描一次

## 观察 Dashboard

启动后可以打开 dashboard：

```text
http://127.0.0.1:6989
```

AI agent 或脚本应优先读取状态 JSON：

```bash
curl http://127.0.0.1:6988/status.json
curl http://127.0.0.1:6988/runs/<run_id>.json
```

`status.json` 是全局轻量摘要，不包含完整 task payload。需要某个 run 或 task 的细节时使用 scoped URL。

## 示例任务图

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

## 已知限制

- `capabilities` 已接收但暂未参与任务筛选
- 若任务没有配置 `validation_script`，当前实现会停留在 `EVALUATING`
- 依赖解锁当前主要依赖轮询，不是纯事件驱动
- 配置项已环境变量化（`DATABASE_URL`、`LIMENET_BIND`、`LIMENET_STATUS_BIND`、`LIMENET_DASHBOARD_BIND`、`LIMENET_INSTANCE_ID`），其他参数仍以代码内默认值为主
- 观察 dashboard 是只读 UI，没有鉴权、分布式部署或隔离沙箱

## 文档维护建议

如果后续版本继续演进，建议遵循以下约定：

- 新增或变更接口时，同时更新 `docs/api.md`
- 调度行为有变化时，同时更新 `docs/architecture.md`
- 运行方式、限制或默认值变化时，同时更新 `docs/operations.md`
- 每次发布时同步维护 `CHANGELOG.md`
