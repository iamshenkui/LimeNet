-- Make graph task identity graph-scoped:
-- Primary key becomes (graph_id, task_id) and ordering uniqueness is (graph_id, task_order).

-- Drop old single-column constraints
ALTER TABLE graph_tasks DROP CONSTRAINT IF EXISTS graph_tasks_pkey;
DROP INDEX IF EXISTS idx_graph_tasks_task_order;

-- Composite primary key on (graph_id, task_id)
ALTER TABLE graph_tasks ADD PRIMARY KEY (graph_id, task_id);

-- Composite unique index so task_order is unique per graph
CREATE UNIQUE INDEX idx_graph_tasks_graph_order ON graph_tasks (graph_id, task_order);
