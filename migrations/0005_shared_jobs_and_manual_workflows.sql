-- Job 配置从用户私有改为组件实例内共享。先合并历史上不同用户保存的同一个 Job，
-- 并同步任务版本、执行节点中的引用，再移除 user_id。
CREATE TEMP TABLE gd_job_config_merge ON COMMIT DROP AS
SELECT id AS old_id,
       first_value(id) OVER (
           PARTITION BY component_instance_id, job_full_name
           ORDER BY updated_at DESC, id
       ) AS keep_id
FROM gd_job_configs;

UPDATE gd_node_executions n
SET job_config_id = m.keep_id
FROM gd_job_config_merge m
WHERE n.job_config_id = m.old_id
  AND m.old_id <> m.keep_id;

UPDATE gd_task_versions tv
SET definition = jsonb_set(
    tv.definition,
    '{nodes}',
    COALESCE((
        SELECT jsonb_agg(
            CASE
                WHEN m.keep_id IS NULL THEN node
                ELSE jsonb_set(node, '{job_config_id}', to_jsonb(m.keep_id::text))
            END
            ORDER BY ordinal
        )
        FROM jsonb_array_elements(tv.definition->'nodes') WITH ORDINALITY AS item(node, ordinal)
        LEFT JOIN gd_job_config_merge m
          ON m.old_id::text = node->>'job_config_id'
    ), '[]'::jsonb)
)
WHERE jsonb_typeof(tv.definition->'nodes') = 'array';

UPDATE gd_executions ex
SET snapshot = jsonb_set(
    ex.snapshot,
    '{nodes}',
    COALESCE((
        SELECT jsonb_agg(
            CASE
                WHEN m.keep_id IS NULL THEN node
                ELSE jsonb_set(node, '{job_config_id}', to_jsonb(m.keep_id::text))
            END
            ORDER BY ordinal
        )
        FROM jsonb_array_elements(ex.snapshot->'nodes') WITH ORDINALITY AS item(node, ordinal)
        LEFT JOIN gd_job_config_merge m
          ON m.old_id::text = node->>'job_config_id'
    ), '[]'::jsonb)
)
WHERE jsonb_typeof(ex.snapshot->'nodes') = 'array';

DELETE FROM gd_job_config_versions v
USING gd_job_config_merge m
WHERE v.job_config_id = m.old_id
  AND m.old_id <> m.keep_id;

DELETE FROM gd_job_configs j
USING gd_job_config_merge m
WHERE j.id = m.old_id
  AND m.old_id <> m.keep_id;

ALTER TABLE gd_job_configs
    DROP CONSTRAINT IF EXISTS gd_job_configs_user_id_component_instance_id_display_name_key,
    DROP CONSTRAINT IF EXISTS gd_job_configs_user_id_component_instance_id_job_full_name_key;
DROP INDEX IF EXISTS job_configs_user_idx;
ALTER TABLE gd_job_configs DROP COLUMN user_id;
ALTER TABLE gd_job_configs
    ADD CONSTRAINT gd_job_configs_instance_job_unique
    UNIQUE (component_instance_id, job_full_name);

-- 与版本化定义分开保存最后一次实际执行所用参数，避免篡改历史配置版本。
ALTER TABLE gd_job_configs ADD COLUMN latest_parameter_presets jsonb;

-- 手工添加但尚未点亮保存的 WorkflowJob 也需要跨弹窗回显。
CREATE TABLE gd_manual_workflow_jobs
(
    id                    uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    component_instance_id uuid        NOT NULL,
    name                  varchar(160) NOT NULL,
    full_name             text        NOT NULL,
    url                   text        NOT NULL,
    class_name            text        NOT NULL,
    created_by            uuid        NOT NULL,
    created_at            timestamptz NOT NULL DEFAULT now(),
    UNIQUE (component_instance_id, full_name),
    UNIQUE (component_instance_id, url)
);
