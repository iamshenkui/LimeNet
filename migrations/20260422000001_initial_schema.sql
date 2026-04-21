-- LimeNet: Initial schema for task orchestration DAG
-- Creates the `tasks` table with full task lifecycle support.

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE tasks (
    task_id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    status           VARCHAR(20) NOT NULL DEFAULT 'PENDING'
                     CHECK (status IN ('PENDING', 'READY', 'IN_PROGRESS', 'EVALUATING', 'BACKOFF', 'COMPLETED')),
    parent_ids       UUID[] NOT NULL DEFAULT '{}',
    child_ids        UUID[] NOT NULL DEFAULT '{}',
    payload          JSONB NOT NULL
                     CHECK (jsonb_typeof(payload->'instruction') = 'string'),
    lease            JSONB,
    retry_logic      JSONB,
    topological_level INT NOT NULL DEFAULT 0,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for efficient READY-task claiming (the hot path for agents)
CREATE INDEX idx_tasks_status_ready ON tasks (status) WHERE status = 'READY';

-- Index for lease reaper: find expired IN_PROGRESS tasks
CREATE INDEX idx_tasks_lease_expired ON tasks ((lease->>'expires_at'))
    WHERE status = 'IN_PROGRESS';

-- Index for backoff awakener: find tasks whose backoff has elapsed
CREATE INDEX idx_tasks_backoff_until ON tasks ((retry_logic->>'backoff_until'))
    WHERE status = 'BACKOFF';

-- Index for dependency resolution: find PENDING children of a completed task
CREATE INDEX idx_tasks_parent_ids ON tasks USING GIN (parent_ids);

-- Auto-update updated_at on row modification
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_tasks_updated_at
    BEFORE UPDATE ON tasks
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
