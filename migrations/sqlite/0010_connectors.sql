CREATE TABLE IF NOT EXISTS connectors (
    connector_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    config_json TEXT NOT NULL DEFAULT '{}',
    secret_env TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    last_sync_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, name)
);
CREATE INDEX IF NOT EXISTS idx_connectors_tenant ON connectors (tenant_id, space_id);

CREATE TABLE IF NOT EXISTS external_refs (
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    doc_id TEXT NOT NULL DEFAULT '',
    provider TEXT NOT NULL,
    external_id TEXT NOT NULL,
    url TEXT NOT NULL,
    revision TEXT,
    acl_json TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    status TEXT NOT NULL,
    PRIMARY KEY (tenant_id, provider, external_id)
);
CREATE INDEX IF NOT EXISTS idx_external_refs_doc ON external_refs (tenant_id, doc_id);
CREATE INDEX IF NOT EXISTS idx_external_refs_space ON external_refs (tenant_id, space_id);

CREATE TABLE IF NOT EXISTS connector_jobs (
    job_id TEXT PRIMARY KEY,
    connector_id TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    stats_json TEXT NOT NULL DEFAULT '{}',
    error TEXT
);
CREATE INDEX IF NOT EXISTS idx_connector_jobs_connector ON connector_jobs (connector_id, started_at);
