-- SPDX-License-Identifier: AGPL-3.0-only
ALTER TABLE documents ADD COLUMN lang TEXT NOT NULL DEFAULT 'zh';
ALTER TABLE chunks ADD COLUMN lang TEXT NOT NULL DEFAULT 'zh';
ALTER TABLE events ADD COLUMN lang TEXT NOT NULL DEFAULT 'zh';

ALTER TABLE revisions ADD COLUMN lang TEXT NOT NULL DEFAULT 'zh';
ALTER TABLE revisions ADD COLUMN source_url TEXT;
ALTER TABLE revisions ADD COLUMN source_revision TEXT;

DROP INDEX IF EXISTS idx_pages_visibility;
ALTER TABLE pages RENAME TO pages_old;

CREATE TABLE pages (
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    slug TEXT NOT NULL,
    lang TEXT NOT NULL DEFAULT 'zh',
    title TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    order_index INTEGER NOT NULL DEFAULT 0,
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
    PRIMARY KEY (tenant_id, space_id, lang, slug)
);

INSERT INTO pages (
    tenant_id, space_id, slug, lang, title, summary, order_index,
    current_revision_id, content_hash, acl_json, acl_principals, valid_from,
    valid_to, version, superseded_by, created_at, updated_at
)
SELECT tenant_id, space_id, slug, 'zh', title, '', 0, current_revision_id, content_hash,
       acl_json, acl_principals, valid_from, valid_to, version, superseded_by,
       created_at, updated_at
FROM pages_old;

DROP TABLE pages_old;

CREATE INDEX IF NOT EXISTS idx_pages_visibility
    ON pages (tenant_id, space_id, valid_from, valid_to);
CREATE INDEX IF NOT EXISTS idx_pages_lang
    ON pages (tenant_id, space_id, lang);
CREATE INDEX IF NOT EXISTS idx_documents_lang
    ON documents (tenant_id, space_id, lang);
CREATE INDEX IF NOT EXISTS idx_chunks_lang
    ON chunks (tenant_id, space_id, lang);
CREATE INDEX IF NOT EXISTS idx_events_lang
    ON events (tenant_id, space_id, lang);

CREATE TABLE IF NOT EXISTS seed_meta (
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    seed_version TEXT NOT NULL,
    source_manifest_hash TEXT NOT NULL,
    applied_at TEXT NOT NULL,
    page_count INTEGER NOT NULL,
    revision_count INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, space_id)
);
