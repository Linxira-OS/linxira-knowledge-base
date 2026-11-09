-- SPDX-License-Identifier: AGPL-3.0-only
CREATE TABLE IF NOT EXISTS schema_migrations (
    version TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS spaces (
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    name TEXT NOT NULL,
    visibility TEXT NOT NULL,
    default_acl_json TEXT NOT NULL,
    owner_principal_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, space_id)
);

CREATE TABLE IF NOT EXISTS pages (
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    current_revision_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    acl_json TEXT NOT NULL,
    acl_principals TEXT NOT NULL,
    valid_from TEXT NOT NULL,
    valid_to TEXT,
    version INTEGER NOT NULL,
    superseded_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, space_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_pages_visibility
    ON pages (tenant_id, space_id, valid_from, valid_to);

CREATE TABLE IF NOT EXISTS revisions (
    tenant_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    markdown TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    author TEXT NOT NULL,
    created_at TEXT NOT NULL,
    version INTEGER NOT NULL,
    acl_json TEXT NOT NULL,
    acl_principals TEXT NOT NULL,
    valid_from TEXT NOT NULL,
    valid_to TEXT,
    superseded_by TEXT,
    PRIMARY KEY (tenant_id, revision_id)
);

CREATE INDEX IF NOT EXISTS idx_revisions_page
    ON revisions (tenant_id, space_id, slug, version DESC);

CREATE TABLE IF NOT EXISTS documents (
    tenant_id TEXT NOT NULL,
    doc_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    logical_path TEXT NOT NULL,
    source_uri TEXT NOT NULL,
    title TEXT NOT NULL,
    mime TEXT NOT NULL,
    raw_hash TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    content TEXT NOT NULL,
    acl_json TEXT NOT NULL,
    acl_principals TEXT NOT NULL,
    valid_from TEXT NOT NULL,
    valid_to TEXT,
    version INTEGER NOT NULL,
    superseded_by TEXT,
    status TEXT NOT NULL,
    source_role TEXT NOT NULL,
    canonicalization_version TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    chunker_version TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, doc_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_documents_active_path
    ON documents (tenant_id, space_id, logical_path)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_documents_visibility
    ON documents (tenant_id, space_id, valid_from, valid_to, superseded_by);

CREATE INDEX IF NOT EXISTS idx_documents_path
    ON documents (tenant_id, logical_path);

CREATE TABLE IF NOT EXISTS chunks (
    tenant_id TEXT NOT NULL,
    chunk_id TEXT NOT NULL,
    doc_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    body TEXT NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    acl_json TEXT NOT NULL,
    acl_principals TEXT NOT NULL,
    valid_from TEXT NOT NULL,
    valid_to TEXT,
    version INTEGER NOT NULL,
    superseded_by TEXT,
    embedding_json TEXT,
    PRIMARY KEY (tenant_id, chunk_id)
);

CREATE INDEX IF NOT EXISTS idx_chunks_doc
    ON chunks (tenant_id, doc_id, seq);

CREATE INDEX IF NOT EXISTS idx_chunks_visibility
    ON chunks (tenant_id, space_id, valid_from, valid_to, superseded_by);

CREATE TABLE IF NOT EXISTS users (
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    display_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    disabled INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, user_id),
    UNIQUE (tenant_id, username)
);

CREATE TABLE IF NOT EXISTS sessions (
    session_hash TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_expiry ON sessions (expires_at);

CREATE TABLE IF NOT EXISTS api_tokens (
    token_hash TEXT PRIMARY KEY,
    token_id TEXT NOT NULL UNIQUE,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    label TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_user
    ON api_tokens (tenant_id, user_id, created_at);
