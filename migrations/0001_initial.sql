CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE gd_users
(
    id            uuid PRIMARY KEY      DEFAULT gen_random_uuid(),
    username      varchar(80)  NOT NULL UNIQUE,
    display_name  varchar(120) NOT NULL,
    password_hash text         NOT NULL,
    is_admin      boolean      NOT NULL DEFAULT false,
    is_active     boolean      NOT NULL DEFAULT true,
    version       integer      NOT NULL DEFAULT 1,
    created_at    timestamptz  NOT NULL DEFAULT now(),
    updated_at    timestamptz  NOT NULL DEFAULT now()
);

CREATE TABLE gd_sessions
(
    id           uuid PRIMARY KEY     DEFAULT gen_random_uuid(),
    user_id      uuid        NOT NULL,
    token_hash   bytea       NOT NULL UNIQUE,
    expires_at   timestamptz NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sessions_expiry_idx ON gd_sessions (expires_at);

CREATE TABLE gd_customers
(
    id         uuid PRIMARY KEY      DEFAULT gen_random_uuid(),
    code       varchar(64)  NOT NULL UNIQUE,
    name       varchar(160) NOT NULL,
    is_active  boolean      NOT NULL DEFAULT true,
    version    integer      NOT NULL DEFAULT 1,
    created_at timestamptz  NOT NULL DEFAULT now(),
    updated_at timestamptz  NOT NULL DEFAULT now()
);

CREATE TABLE gd_components
(
    id         uuid PRIMARY KEY      DEFAULT gen_random_uuid(),
    code       varchar(64)  NOT NULL UNIQUE,
    name       varchar(160) NOT NULL,
    is_active  boolean      NOT NULL DEFAULT true,
    version    integer      NOT NULL DEFAULT 1,
    created_at timestamptz  NOT NULL DEFAULT now(),
    updated_at timestamptz  NOT NULL DEFAULT now()
);

CREATE TABLE gd_component_members
(
    component_id uuid        NOT NULL,
    user_id      uuid        NOT NULL,
    role         varchar(20) NOT NULL CHECK (role IN ('MAINTAINER', 'TESTER')),
    PRIMARY KEY (component_id, user_id, role)
);

CREATE TABLE gd_environments
(
    id                      uuid PRIMARY KEY      DEFAULT gen_random_uuid(),
    customer_id             uuid         NOT NULL,
    deployment_domain       varchar(160) NOT NULL DEFAULT '',
    code                    varchar(64)  NOT NULL,
    name                    varchar(160) NOT NULL,
    jenkins_url             text         NOT NULL,
    request_timeout_seconds integer      NOT NULL DEFAULT 10 CHECK (request_timeout_seconds BETWEEN 1 AND 300),
    allow_invalid_certs     boolean      NOT NULL DEFAULT false,
    notes                   text         NOT NULL DEFAULT '',
    is_active               boolean      NOT NULL DEFAULT false,
    connection_status       varchar(20)  NOT NULL DEFAULT 'UNTESTED' CHECK (connection_status IN ('UNTESTED', 'CONNECTED', 'ERROR')),
    last_checked_at         timestamptz,
    last_success_at         timestamptz,
    last_error              text,
    version                 integer      NOT NULL DEFAULT 1,
    created_at              timestamptz  NOT NULL DEFAULT now(),
    updated_at              timestamptz  NOT NULL DEFAULT now(),
    UNIQUE (customer_id, deployment_domain, code)
);

CREATE TABLE gd_component_instances
(
    id               uuid PRIMARY KEY      DEFAULT gen_random_uuid(),
    name             varchar(160) NOT NULL,
    component_id     uuid         NOT NULL,
    environment_id   uuid         NOT NULL,
    folder_full_name text         NOT NULL,
    folder_path      text         NOT NULL,
    folder_url       text,
    status           varchar(20)  NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'INACTIVE', 'ERROR')),
    notes            text         NOT NULL DEFAULT '',
    custom_fields    jsonb        NOT NULL DEFAULT '[]'::jsonb,
    last_synced_at   timestamptz,
    version          integer      NOT NULL DEFAULT 1,
    created_at       timestamptz  NOT NULL DEFAULT now(),
    updated_at       timestamptz  NOT NULL DEFAULT now(),
    UNIQUE (environment_id, folder_full_name),
    UNIQUE (environment_id, component_id, name)
);

CREATE TABLE gd_job_configs
(
    id                    uuid PRIMARY KEY      DEFAULT gen_random_uuid(),
    component_instance_id uuid         NOT NULL,
    display_name          varchar(160) NOT NULL,
    description           text         NOT NULL DEFAULT '',
    job_full_name         text         NOT NULL,
    job_url               text,
    status                varchar(20)  NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'INACTIVE', 'ERROR', 'STALE')),
    current_version       integer      NOT NULL DEFAULT 1,
    version               integer      NOT NULL DEFAULT 1,
    created_at            timestamptz  NOT NULL DEFAULT now(),
    updated_at            timestamptz  NOT NULL DEFAULT now(),
    UNIQUE (component_instance_id, display_name),
    UNIQUE (component_instance_id, job_full_name)
);

CREATE TABLE gd_job_config_versions
(
    id                    uuid PRIMARY KEY     DEFAULT gen_random_uuid(),
    job_config_id         uuid        NOT NULL,
    version               integer     NOT NULL,
    parameter_definitions jsonb       NOT NULL DEFAULT '[]'::jsonb,
    parameter_presets     jsonb       NOT NULL DEFAULT '{}'::jsonb,
    definition_hash       varchar(64) NOT NULL,
    created_by            uuid        NOT NULL,
    created_at            timestamptz NOT NULL DEFAULT now(),
    UNIQUE (job_config_id, version)
);

CREATE TABLE gd_tasks
(
    id              uuid PRIMARY KEY      DEFAULT gen_random_uuid(),
    name            varchar(200) NOT NULL,
    description     text         NOT NULL DEFAULT '',
    creator_id      uuid         NOT NULL,
    trigger_type    varchar(20)  NOT NULL CHECK (trigger_type IN ('IMMEDIATE', 'ONCE', 'CRON')),
    scheduled_at    timestamptz,
    cron_expression varchar(100),
    timezone        varchar(80)  NOT NULL DEFAULT 'Asia/Shanghai',
    is_enabled      boolean      NOT NULL DEFAULT true,
    current_version integer      NOT NULL DEFAULT 1,
    version         integer      NOT NULL DEFAULT 1,
    next_run_at     timestamptz,
    created_at      timestamptz  NOT NULL DEFAULT now(),
    updated_at      timestamptz  NOT NULL DEFAULT now()
);

CREATE TABLE gd_task_versions
(
    id         uuid PRIMARY KEY     DEFAULT gen_random_uuid(),
    task_id    uuid        NOT NULL,
    version    integer     NOT NULL,
    definition jsonb       NOT NULL,
    created_by uuid        NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (task_id, version)
);

CREATE TABLE gd_executions
(
    id           uuid PRIMARY KEY      DEFAULT gen_random_uuid(),
    task_id      uuid         NOT NULL,
    task_version integer      NOT NULL,
    trigger_key  varchar(180) NOT NULL UNIQUE,
    trigger_type varchar(20)  NOT NULL,
    status       varchar(24)  NOT NULL CHECK (status IN
                                              ('SCHEDULED', 'RUNNING', 'SUCCESS', 'FAILED', 'CANCELING', 'CANCELED')),
    snapshot     jsonb        NOT NULL,
    scheduled_at timestamptz,
    started_at   timestamptz,
    finished_at  timestamptz,
    created_by   uuid,
    created_at   timestamptz  NOT NULL DEFAULT now()
);
CREATE INDEX executions_task_idx ON gd_executions (task_id, created_at DESC);

CREATE TABLE gd_node_executions
(
    id               uuid PRIMARY KEY      DEFAULT gen_random_uuid(),
    execution_id     uuid         NOT NULL,
    node_key         varchar(100) NOT NULL,
    node_name        varchar(160) NOT NULL,
    environment_id   uuid         NOT NULL,
    job_config_id    uuid         NOT NULL,
    status           varchar(28)  NOT NULL CHECK (status IN
                                                  ('PENDING', 'WAITING_DEPENDENCY', 'QUEUED', 'RUNNING', 'SUCCESS',
                                                   'FAILED', 'CANCELED', 'TIMED_OUT', 'UNKNOWN', 'SKIPPED')),
    dependencies     jsonb        NOT NULL DEFAULT '[]'::jsonb,
    parameters       jsonb        NOT NULL DEFAULT '{}'::jsonb,
    timeout_seconds  integer      NOT NULL DEFAULT 3600,
    queue_id         bigint,
    queue_url        text,
    build_number     bigint,
    build_url        text,
    blocking_reason  text,
    error_summary    text,
    log_offset       bigint       NOT NULL DEFAULT 0,
    claimed_by       uuid,
    claim_expires_at timestamptz,
    submitted_at     timestamptz,
    started_at       timestamptz,
    finished_at      timestamptz,
    updated_at       timestamptz  NOT NULL DEFAULT now(),
    UNIQUE (execution_id, node_key)
);
CREATE INDEX node_work_idx ON gd_node_executions (status, claim_expires_at);

CREATE TABLE gd_schedule_events
(
    id           uuid PRIMARY KEY     DEFAULT gen_random_uuid(),
    task_id      uuid        NOT NULL,
    event_type   varchar(40) NOT NULL,
    scheduled_at timestamptz,
    reason       text,
    created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE gd_audit_logs
(
    id          uuid PRIMARY KEY     DEFAULT gen_random_uuid(),
    actor_id    uuid,
    action      varchar(80) NOT NULL,
    object_type varchar(80) NOT NULL,
    object_id   uuid,
    request_id  varchar(80),
    summary     jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_lookup_idx ON gd_audit_logs (created_at DESC, object_type, actor_id);
