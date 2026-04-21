Limenet MVP: 多任务编排与验证引擎核心规范

1. 系统定位与核心原则

Limenet 是一个面向 AI Agent（如 Hermes, Claw）的高并发私有任务编排中枢。其核心职责是管理具有复杂前置/后置依赖关系的代码任务集合，并在多个并发执行的 Agent 之间安全、高效地分配任务。

设计准则：流转优于状态 (Flow over State)

无终态失败原则：系统内不存在 FAILED 或 BLOCKED 等死局状态。一切异常（网络断开、代码验证未通过、API 限流）均通过引入重试机制（回滚或退避）来解决，保证任务图始终处于活跃或自我修复状态。

事件驱动解锁：下游任务的激活不依赖于全局轮询，而是由上游任务进入特定状态（如 COMPLETED）时的事件广播来触发。

原子操作：在多并发环境下，任务的获取与状态转移必须是原子的，防止重复派单。

2. 核心数据架构：任务有向无环图 (DAG)

系统通过关系型数据库（如 PostgreSQL）或支持图计算的数据结构存储任务。

Task Schema (核心数据结构)

{
  "task_id": "string (UUID)",
  "status": "enum (PENDING | READY | IN_PROGRESS | EVALUATING | BACKOFF | COMPLETED)",
  
  // 核心依赖图结构
  "parent_ids": ["uuid_1", "uuid_2"], // 必须全部 COMPLETED，此任务才能 READY
  "child_ids": ["uuid_3"],            // 本任务 COMPLETED 时，尝试解锁这些任务
  
  "payload": {
    "instruction": "string (具体给 Agent 的指令，如 '重构 network.js 增加重试')",
    "context_paths": ["src/network.js", "tests/network.test.js"],
    "validation_script": "string (验证指令，如 'npm run test network')"
  },
  
  "lease": {
    "agent_id": "string (持有该任务的 Agent 标识)",
    "expires_at": "timestamp (租约到期时间)"
  },
  
  "retry_logic": {
    "attempt_count": "integer",
    "backoff_until": "timestamp (在此之前任务保持 BACKOFF 态不可见)"
  },
  
  "created_at": "timestamp",
  "updated_at": "timestamp"
}


3. 状态机与任务流转生命周期

状态机是系统调度和多任务协作的信号灯系统。

状态

核心意义

流转条件 (Event Triggers)

PENDING

蛰伏：存在未完成的依赖任务

监听父节点 COMPLETED 事件。当 parent_ids 全空或全部完成 $\rightarrow$ READY

READY

释放：进入系统的可调度资源池

被 Agent 通过 API 原子申领 $\rightarrow$ IN_PROGRESS

IN_PROGRESS

锁定：资源正在被消耗执行

1. Agent 提交产出 $\rightarrow$ EVALUATING



2. 租约超时/心跳停止 $\rightarrow$ 强制重置为 READY

EVALUATING

关口：触发底层沙盒验证引擎

1. 验证脚本 Pass $\rightarrow$ COMPLETED



2. 验证失败 $\rightarrow$ BACKOFF

BACKOFF

冷却：因报错产生的惩罚性退避

指数退避时间 (backoff_until) 到期 $\rightarrow$ READY

COMPLETED

激活：本节点生命周期结束

触发事件，检查自身的 child_ids，尝试解锁下游节点

4. 核心 API 协议 (Client <-> Limenet)

这些 API 用于本地/云端部署的 Hermes 或 Claw 与 Limenet 进行交互。

4.1. 压入任务图 (Ingest Graph)

Endpoint: POST /api/v1/tasks/batch

行为: 允许一次性注入一组互相关联的 Task。系统需校验是否存在循环依赖（检测 DAG 的合法性）。

4.2. 原子申领任务 (Claim Task) - [最关键接口]

Endpoint: POST /api/v1/tasks/claim

Request: {"agent_id": "hermes-local-01", "capabilities": ["coding", "python"]}

行为:

引擎从数据库查询最高优先级的、状态为 READY 的任务。

必须使用原子锁（例如 PostgreSQL 的 SELECT ... FOR UPDATE SKIP LOCKED）。

将该任务赋予当前 agent_id，更新 status 为 IN_PROGRESS，并设定 lease.expires_at (例如当前时间 + 15分钟)。

Response: 返回完整的 Task Payload。若无可用任务，返回 204 No Content。

4.3. 提交产出并触发验证 (Submit & Evaluate)

Endpoint: POST /api/v1/tasks/{task_id}/submit

Request: {"agent_id": "hermes-local-01", "result_summary": "...", "files_changed": [...]}

行为:

验证 agent_id 与 Lease 是否匹配。

将任务状态从 IN_PROGRESS 转移至 EVALUATING。

异步触发: Limenet 后台启动 validation_script 进行代码逻辑校验。

4.4. 维持心跳 (Heartbeat)

Endpoint: POST /api/v1/tasks/{task_id}/heartbeat

行为: Agent 定期（如每 3 分钟）调用。Limenet 刷新该任务的 lease.expires_at，防止执行耗时任务时被系统错误回收。

5. 后台调度引擎核心职责 (The Engine Daemons)

Limenet 服务端除了暴露 API，还需在后台持续运行以下逻辑循环：

依赖解锁器 (Dependency Resolver):

监听任何状态变为 COMPLETED 的任务。

获取其 child_ids，遍历检查这些子任务的所有 parent_ids。若全通，将子任务状态从 PENDING 置为 READY。

死锁回收器 (Lease Reaper):

定时扫描 IN_PROGRESS 列表，找出当前时间超过 lease.expires_at 的任务。

剥夺原 Agent 的控制权，将任务打回 READY。

退避轮询器 (Backoff Awakener):

检查 BACKOFF 队列中冷却时间已到的任务，重新放回 READY 池。