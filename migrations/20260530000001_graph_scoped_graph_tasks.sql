-- LimeNet: Make graph task identity graph-scoped
-- Changes primary key from (run_id, task_id) to (graph_id, task_id)
-- and scopes ordering/uniqueness by graph_id.

ALTER TABLE graph_tasks
    ADD COLUMN IF NOT EXISTS graph_id TEXT NOT NULL DEFAULT 'default';

-- Migrate existing rows: derive graph_id from run_id so each run's tasks
-- live under a distinct graph.  The nil/default run maps to 'default'.
UPDATE graph_tasks
SET graph_id = CASE
    WHEN run_id = '00000000-0000-0000-0000-000000000000' THEN 'default'
    ELSE run_id::text
END
WHERE graph_id = 'default';

-- Drop run-scoped constraints and indexes
ALTER TABLE graph_tasks
    DROP CONSTRAINT IF EXISTS graph_tasks_pkey;

DROP INDEX IF EXISTS idx_graph_tasks_run_task_order;
DROP INDEX IF EXISTS idx_graph_tasks_run_updated_at;
DROP INDEX IF EXISTS idx_graph_tasks_run_status;

-- Add graph-scoped constraints and indexes
ALTER TABLE graph_tasks
    ADD CONSTRAINT graph_tasks_pkey PRIMARY KEY (graph_id, task_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_tasks_graph_task_order
ON graph_tasks (graph_id, task_order);

CREATE INDEX IF NOT EXISTS idx_graph_tasks_graph_updated_at
ON graph_tasks (graph_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_graph_tasks_graph_status
ON graph_tasks (graph_id, ((task_data->>'status')));

-- Keep run_id for backward-compatible run-level aggregation but allow
-- graph-scoped tasks that are not tied to a specific run.
ALTER TABLE graph_tasks
    ALTER COLUMN run_id DROP NOT NULL;
