ALTER TABLE gd_tasks ADD COLUMN pinned_at timestamptz;
CREATE INDEX gd_tasks_pin_order_idx
    ON gd_tasks (pinned_at ASC, created_at DESC)
    WHERE NOT is_deleted;
