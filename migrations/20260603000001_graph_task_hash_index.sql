-- LimeNet: Persist and index graph task state hashes for O(1) lookup.
--
-- Adds a state_hash column to graph_tasks and indexes it by graph_id so
-- that task-view/progress/result endpoints can resolve a hash directly
-- instead of scanning the entire graph.

ALTER TABLE graph_tasks
    ADD COLUMN IF NOT EXISTS state_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_graph_tasks_graph_hash
ON graph_tasks (graph_id, state_hash);
