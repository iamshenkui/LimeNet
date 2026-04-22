# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project currently tracks an internal `0.x` release line.

## [0.1.0] - 2026-04-22

### Added

- 初始化 LimeNet Rust 项目骨架，包含 Axum、Tokio、SQLx、Serde、UUID 和 Chrono 依赖
- 新增 `tasks` 表 migration，包含任务状态、依赖关系、租约、重试信息与索引
- 新增任务核心模型，包括 `Task`、`Payload`、`Lease`、`RetryLogic` 与状态枚举
- 新增批量任务图写入能力，支持 DAG 环检测与 `topological_level` 计算
- 新增原子任务申领接口，基于 `FOR UPDATE SKIP LOCKED`
- 新增任务心跳续租接口
- 新增任务提交与异步校验执行流程
- 新增后台守护逻辑：`DependencyResolver`、`LeaseReaper`、`BackoffAwakener`
- 新增基础仓储测试与任务表读写验证测试
- 新增 `docs/` 文档目录，并将项目 wiki 拆分为专题文档

### Known Limitations

- `capabilities` 字段已进入接口但尚未参与任务筛选
- 未配置 `validation_script` 的任务提交后会停留在 `EVALUATING`
- 依赖解锁目前主要依赖轮询扫描
- 配置项尚未完整环境变量化
