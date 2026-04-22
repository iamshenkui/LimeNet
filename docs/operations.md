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

```bash
export DATABASE_URL=postgres://<user>:<password>@localhost:5432/<db>
```

如果未设置，当前代码会回退到：

```text
postgres://chenhui@localhost:5432/postgres
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
0.0.0.0:3000
```

## 后台任务间隔

- `DependencyResolver`: 每 2 秒扫描一次
- `LeaseReaper`: 每 60 秒扫描一次
- `BackoffAwakener`: 每 30 秒扫描一次

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
- 配置项仍以代码内默认值为主，尚未完整环境变量化
- 目前没有 Web UI、鉴权、分布式部署或隔离沙箱

## 文档维护建议

如果后续版本继续演进，建议遵循以下约定：

- 新增或变更接口时，同时更新 `docs/api.md`
- 调度行为有变化时，同时更新 `docs/architecture.md`
- 运行方式、限制或默认值变化时，同时更新 `docs/operations.md`
- 每次发布时同步维护 `CHANGELOG.md`
