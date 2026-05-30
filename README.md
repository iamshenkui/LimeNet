# LimeNet

LimeNet 是一个用 Rust、Axum 和 PostgreSQL 实现的多 Agent 任务编排后端，负责管理 DAG 任务依赖、原子派单、租约续期、异步校验与失败退避。

## 文档

- `docs/README.md`: 文档导航
- `docs/architecture.md`: 架构、状态机与调度模型
- `docs/api.md`: HTTP API 与示例请求
- `docs/operations.md`: 数据库、运行方式与当前限制
- `docs/observe-dashboard-prd.md`: 观察 dashboard 数据范围与端口模型
- `CHANGELOG.md`: 版本变更记录

## 快速开始

```bash
export DATABASE_URL=postgres://<user>:<password>@localhost:5432/<db>
sqlx migrate run
cargo run
```

`DATABASE_URL` **必须显式设置**，没有静默回退默认值。启动时会打印解析后的数据库目标（不含密码），例如：

```text
LimeNet connecting to database localhost:5432/limenet...
LimeNet status API starting on 127.0.0.1:6988...
LimeNet dashboard starting on 127.0.0.1:6989...
LimeNet task orchestrator starting on 127.0.0.1:6987...
```

通过设置不同的 `DATABASE_URL` 和 `LIMENET_BIND` 可以在同一台机器上运行多个完全隔离的实例。详见 `docs/operations.md`。

默认本地端口：

- 任务 API: `http://127.0.0.1:6987`
- 状态 JSON: `http://127.0.0.1:6988/status.json`
- 观察 dashboard: `http://127.0.0.1:6989`

## 参考

- `prd.MD`
- `tasks/prd-limenet-task-orchestrator.md`
