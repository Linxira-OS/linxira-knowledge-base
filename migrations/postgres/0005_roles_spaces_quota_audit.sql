-- SPDX-License-Identifier: AGPL-3.0-only
-- P0a/P0b: tenant roles, groups, space members, quotas and audit.
ALTER TABLE api_tokens ADD COLUMN kind TEXT NOT NULL DEFAULT 'user';
ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'user';

ALTER TABLE spaces ADD COLUMN owner_type TEXT NOT NULL DEFAULT 'user';
ALTER TABLE spaces ADD COLUMN owner_id TEXT NOT NULL DEFAULT '';
ALTER TABLE spaces ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
ALTER TABLE spaces ADD COLUMN settings TEXT NOT NULL DEFAULT '{}';
ALTER TABLE spaces ADD COLUMN archived_at TEXT;
ALTER TABLE spaces ADD COLUMN deleted_at TEXT;
UPDATE spaces SET owner_id = owner_principal_id WHERE owner_id = '';

CREATE TABLE IF NOT EXISTS groups (
    group_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_groups_tenant ON groups (tenant_id, status);

CREATE TABLE IF NOT EXISTS group_members (
    group_id TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    role TEXT NOT NULL,
    added_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (group_id, principal_id)
);

CREATE INDEX IF NOT EXISTS idx_group_members_principal
    ON group_members (principal_id);

CREATE TABLE IF NOT EXISTS space_members (
    space_id TEXT NOT NULL,
    principal_type TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    role TEXT NOT NULL,
    added_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (space_id, principal_type, principal_id)
);

CREATE INDEX IF NOT EXISTS idx_space_members_principal
    ON space_members (principal_type, principal_id);

CREATE TABLE IF NOT EXISTS space_quotas (
    tenant_id TEXT NOT NULL,
    scope_type TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    storage_bytes BIGINT,
    documents BIGINT,
    chunks BIGINT,
    members BIGINT,
    spaces BIGINT,
    enforcement TEXT NOT NULL DEFAULT 'hard',
    updated_by TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, scope_type, scope_id)
);

CREATE TABLE IF NOT EXISTS storage_usage (
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    source_bytes BIGINT NOT NULL DEFAULT 0,
    index_bytes BIGINT NOT NULL DEFAULT 0,
    history_bytes BIGINT NOT NULL DEFAULT 0,
    total_bytes BIGINT NOT NULL DEFAULT 0,
    reserved_bytes BIGINT NOT NULL DEFAULT 0,
    documents BIGINT NOT NULL DEFAULT 0,
    chunks BIGINT NOT NULL DEFAULT 0,
    members BIGINT NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, space_id)
);

CREATE TABLE IF NOT EXISTS quota_overrides (
    override_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    scope_type TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    dimension TEXT NOT NULL,
    limit_value BIGINT NOT NULL,
    reason TEXT NOT NULL,
    actor TEXT NOT NULL,
    expires_at TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_quota_overrides_scope
    ON quota_overrides (tenant_id, scope_type, scope_id, dimension, expires_at);

CREATE TABLE IF NOT EXISTS audit_events (
    event_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    space_id TEXT,
    actor_id TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    result TEXT NOT NULL,
    status_code BIGINT NOT NULL,
    request_id TEXT,
    ip TEXT,
    user_agent TEXT,
    detail_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_tenant_time
    ON audit_events (tenant_id, created_at);
CREATE INDEX IF NOT EXISTS idx_audit_actor
    ON audit_events (tenant_id, actor_id, created_at);
CREATE INDEX IF NOT EXISTS idx_audit_action
    ON audit_events (tenant_id, action, created_at);
CREATE INDEX IF NOT EXISTS idx_audit_space
    ON audit_events (tenant_id, space_id, created_at);
