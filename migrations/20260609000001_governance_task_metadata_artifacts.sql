ALTER TABLE tasks
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

CREATE INDEX IF NOT EXISTS idx_tasks_ready_executor_role
ON tasks ((metadata->>'executor_role'), status)
WHERE status = 'READY';

CREATE INDEX IF NOT EXISTS idx_tasks_ready_task_kind
ON tasks ((metadata->>'task_kind'), status)
WHERE status = 'READY';

CREATE TABLE IF NOT EXISTS governance_artifacts (
    artifact_id TEXT PRIMARY KEY,
    artifact_kind TEXT NOT NULL CHECK (
        artifact_kind IN (
            'plan_artifact',
            'cartographer_plan_confirmation',
            'cartographer_checkpoint_review',
            'architecture_review_surface',
            'quartermaster_checkpoint_review',
            'quartermaster_review',
            'shared_surface_guard',
            'break_glass_authorization',
            'dockmaster_merge_decision'
        )
    ),
    task_id UUID NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    repo TEXT NOT NULL,
    base_sha TEXT NOT NULL,
    head_sha TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    producer_role TEXT NOT NULL CHECK (
        producer_role IN ('meta-agent', 'cartographer', 'quartermaster', 'dockmaster')
    ),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_governance_artifacts_task_id
ON governance_artifacts (task_id);

CREATE INDEX IF NOT EXISTS idx_governance_artifacts_repo_head
ON governance_artifacts (repo, head_sha);
