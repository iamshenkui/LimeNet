CREATE TABLE IF NOT EXISTS runs (
    run_id UUID PRIMARY KEY,
    display_name TEXT,
    source_kind TEXT NOT NULL DEFAULT 'manual',
    source_ref TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO runs (run_id, display_name, source_kind, source_ref, status)
VALUES ('00000000-0000-0000-0000-000000000000', 'default', 'system', 'compatibility-default-run', 'active')
ON CONFLICT (run_id) DO NOTHING;

ALTER TABLE graph_tasks
    ADD COLUMN IF NOT EXISTS run_id UUID;

UPDATE graph_tasks
SET run_id = '00000000-0000-0000-0000-000000000000'
WHERE run_id IS NULL;

ALTER TABLE graph_tasks
    ALTER COLUMN run_id SET NOT NULL;

ALTER TABLE graph_tasks
    ADD CONSTRAINT fk_graph_tasks_run_id
    FOREIGN KEY (run_id) REFERENCES runs(run_id)
    ON DELETE CASCADE;

ALTER TABLE graph_tasks
    DROP CONSTRAINT IF EXISTS graph_tasks_pkey;

DROP INDEX IF EXISTS idx_graph_tasks_task_order;

ALTER TABLE graph_tasks
    ADD CONSTRAINT graph_tasks_pkey PRIMARY KEY (run_id, task_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_tasks_run_task_order
ON graph_tasks (run_id, task_order);

CREATE INDEX IF NOT EXISTS idx_graph_tasks_run_updated_at
ON graph_tasks (run_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_graph_tasks_run_status
ON graph_tasks (run_id, ((task_data->>'status')));

CREATE OR REPLACE FUNCTION update_runs_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_runs_updated_at ON runs;

CREATE TRIGGER trg_runs_updated_at
    BEFORE UPDATE ON runs
    FOR EACH ROW
    EXECUTE FUNCTION update_runs_updated_at_column();
