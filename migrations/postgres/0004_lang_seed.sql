ALTER TABLE documents ADD COLUMN lang TEXT NOT NULL DEFAULT 'zh';
ALTER TABLE chunks ADD COLUMN lang TEXT NOT NULL DEFAULT 'zh';
ALTER TABLE events ADD COLUMN lang TEXT NOT NULL DEFAULT 'zh';

ALTER TABLE revisions ADD COLUMN lang TEXT NOT NULL DEFAULT 'zh';
ALTER TABLE revisions ADD COLUMN source_url TEXT;
ALTER TABLE revisions ADD COLUMN source_revision TEXT;

ALTER TABLE pages ADD COLUMN lang TEXT NOT NULL DEFAULT 'zh';
ALTER TABLE pages ADD COLUMN summary TEXT NOT NULL DEFAULT '';
ALTER TABLE pages ADD COLUMN order_index BIGINT NOT NULL DEFAULT 0;
ALTER TABLE pages DROP CONSTRAINT IF EXISTS pages_pkey;
ALTER TABLE pages ADD CONSTRAINT pages_pkey PRIMARY KEY (tenant_id, space_id, lang, slug);

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
    page_count BIGINT NOT NULL,
    revision_count BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, space_id)
);
