# LimeNet

LimeNet 是一个用 Rust、Axum 和 PostgreSQL 实现的多 Agent 任务编排后端，负责管理 DAG 任务依赖、原子派单、租约续期、异步校验与失败退避。

## 文档

- `docs/README.md`: 文档导航
- `docs/architecture.md`: 架构、状态机与调度模型
- `docs/api.md`: HTTP API 与示例请求
- `docs/operations.md`: 数据库、运行方式与当前限制
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
LimeNet task orchestrator starting on 0.0.0.0:3000...
```

通过设置不同的 `DATABASE_URL` 和 `LIMENET_BIND` 可以在同一台机器上运行多个完全隔离的实例。详见 `docs/operations.md`。

## 参考

- `prd.MD`
- `tasks/prd-limenet-task-orchestrator.md`
