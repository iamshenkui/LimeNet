# LimeNet Architecture

## 系统定位

LimeNet 是一个面向多 Agent 并发执行场景的任务编排后端。系统以任务 DAG 为核心数据结构，围绕任务生命周期提供：

- 批量写入任务图
- DAG 环检测
- 原子申领任务
- 租约续期与超时回收
- 异步校验与失败退避
- 下游依赖解锁

## 技术栈

- Rust `edition = "2024"`
- Axum `0.8`
- Tokio
- SQLx
- PostgreSQL

## 代码结构

```text
LimeNet/
├── src/main.rs                 # HTTP 服务入口与后台守护任务启动
├── src/contracts/task.rs       # Task/Lease/RetryLogic/请求体定义
├── src/state/repository.rs     # 仓储、状态流转、后台循环
├── migrations/                 # PostgreSQL schema
├── prd.MD
└── tasks/prd-limenet-task-orchestrator.md
```

## 核心模型

### Task

每个任务包含以下关键字段：

- `task_id`: UUID
- `status`: `PENDING | READY | IN_PROGRESS | EVALUATING | BACKOFF | COMPLETED`
- `parent_ids`: 所有父任务都完成后才可解锁
- `child_ids`: 当前任务完成后可触发检查的下游任务
- `payload.instruction`: 给 Agent 的执行指令
- `payload.context_paths`: 相关上下文文件路径
- `payload.validation_script`: 校验命令，提交后异步执行
- `lease.agent_id / lease.expires_at`: 当前租约
- `retry_logic.attempt_count / retry_logic.backoff_until`: 重试信息
- `topological_level`: 调度优先级层级，越小越优先

### 状态机

```text
PENDING
  -> READY
  -> IN_PROGRESS
  -> EVALUATING
  -> COMPLETED
  -> BACKOFF
  -> READY
```

状态含义：

- `PENDING`: 仍有依赖未完成
- `READY`: 可被 Agent 申领
- `IN_PROGRESS`: 已被某个 Agent 持有
- `EVALUATING`: 已提交产出，等待校验脚本结果
- `COMPLETED`: 校验成功
- `BACKOFF`: 校验失败或执行异常，等待退避到期

## 调度模型

### Batch Ingest

批量接收任务图后，系统会：

- 使用 DFS 检测循环依赖
- 计算 `topological_level`
- 把无父节点任务置为 `READY`
- 把有父节点任务置为 `PENDING`
- 在单个事务中持久化所有任务

### Claim

任务申领阶段采用数据库原子锁实现并发安全：

- 仅从 `READY` 集合中挑选任务
- 按 `topological_level ASC, created_at ASC` 排序
- 使用 `FOR UPDATE SKIP LOCKED`
- 申领成功后写入 `lease` 并转为 `IN_PROGRESS`

### Submit & Evaluate

Agent 提交结果后：

- 系统校验任务状态与租约归属
- 任务先转为 `EVALUATING`
- 异步执行 `payload.validation_script`
- 成功则转为 `COMPLETED`
- 失败则转为 `BACKOFF`，并更新重试信息

## 后台守护逻辑

### DependencyResolver

- 周期性扫描最近完成的任务
- 找出其所有仍处于 `PENDING` 的子任务
- 当子任务的所有父任务都已完成时，将其转为 `READY`

### LeaseReaper

- 周期性扫描 `IN_PROGRESS` 且租约过期的任务
- 清空 `lease`
- 将任务重新置为 `READY`
- 增加 `attempt_count`

### BackoffAwakener

- 周期性扫描 `BACKOFF` 且 `backoff_until` 已经过期的任务
- 将任务重新置为 `READY`

## 当前实现说明

当前实现与设计方向基本一致，但仍有几个需要注意的现实细节：

- `capabilities` 已进入请求模型，但暂未参与调度过滤
- `Notify` 已初始化，但依赖解锁当前主要仍依赖轮询扫描
- 若任务没有配置 `validation_script`，提交后会停留在 `EVALUATING`
