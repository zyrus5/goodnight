ALTER TABLE gd_tasks
    ADD COLUMN is_deleted boolean NOT NULL DEFAULT false;

CREATE INDEX gd_tasks_active_updated_idx
    ON gd_tasks (updated_at DESC)
    WHERE NOT is_deleted;
