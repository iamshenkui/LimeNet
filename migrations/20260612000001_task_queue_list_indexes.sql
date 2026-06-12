CREATE INDEX IF NOT EXISTS idx_tasks_queue_order
    ON tasks (topological_level, created_at);

CREATE INDEX IF NOT EXISTS idx_tasks_task_kind_order
    ON tasks ((metadata->>'task_kind'), topological_level, created_at);

CREATE INDEX IF NOT EXISTS idx_tasks_executor_role_order
    ON tasks ((metadata->>'executor_role'), topological_level, created_at);

CREATE INDEX IF NOT EXISTS idx_tasks_kind_role_order
    ON tasks ((metadata->>'task_kind'), (metadata->>'executor_role'), topological_level, created_at);
