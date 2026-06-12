CREATE INDEX IF NOT EXISTS idx_tasks_ready_metadata_tags
    ON tasks USING GIN ((metadata->'tags'))
    WHERE status = 'READY';

