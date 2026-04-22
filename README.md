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

服务默认监听 `0.0.0.0:3000`。

## 参考

- `prd.MD`
- `tasks/prd-limenet-task-orchestrator.md`
