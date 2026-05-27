-- Add graph_id column to graph_tasks for stable task state integrity hashes
ALTER TABLE graph_tasks ADD COLUMN IF NOT EXISTS graph_id TEXT NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_graph_tasks_graph_id ON graph_tasks (graph_id);
