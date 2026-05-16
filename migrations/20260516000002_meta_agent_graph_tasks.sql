CREATE TABLE IF NOT EXISTS graph_tasks (
    task_id TEXT PRIMARY KEY,
    task_order INT NOT NULL,
    task_data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_tasks_task_order ON graph_tasks (task_order);

CREATE OR REPLACE FUNCTION update_graph_tasks_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_graph_tasks_updated_at ON graph_tasks;

CREATE TRIGGER trg_graph_tasks_updated_at
    BEFORE UPDATE ON graph_tasks
    FOR EACH ROW
    EXECUTE FUNCTION update_graph_tasks_updated_at_column();