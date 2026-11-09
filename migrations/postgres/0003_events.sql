CREATE TABLE IF NOT EXISTS events (
    event_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    doc_id TEXT NOT NULL,
    chunk_id TEXT NOT NULL,
    seq BIGINT NOT NULL,
    text TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    extraction_model TEXT NOT NULL,
    acl_json TEXT NOT NULL,
    acl_principals TEXT NOT NULL,
    valid_from TEXT NOT NULL,
    valid_to TEXT,
    version BIGINT NOT NULL,
    superseded_by TEXT,
    created_at TEXT NOT NULL,
    tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple', text)) STORED
);

CREATE INDEX IF NOT EXISTS idx_events_scope_doc
    ON events (tenant_id, space_id, doc_id);

CREATE INDEX IF NOT EXISTS idx_events_scope_chunk
    ON events (tenant_id, chunk_id);

CREATE INDEX IF NOT EXISTS idx_events_visibility
    ON events (tenant_id, space_id, valid_from, valid_to, superseded_by);

CREATE INDEX IF NOT EXISTS idx_events_tsv ON events USING GIN (tsv);

CREATE TABLE IF NOT EXISTS entities (
    entity_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (tenant_id, space_id, normalized_name, type)
);

CREATE INDEX IF NOT EXISTS idx_entities_lookup
    ON entities (tenant_id, space_id, normalized_name);

CREATE TABLE IF NOT EXISTS event_entities (
    event_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    weight DOUBLE PRECISION NOT NULL,
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    PRIMARY KEY (event_id, entity_id),
    FOREIGN KEY (event_id) REFERENCES events(event_id),
    FOREIGN KEY (entity_id) REFERENCES entities(entity_id)
);

CREATE INDEX IF NOT EXISTS idx_event_entities_expand
    ON event_entities (tenant_id, space_id, entity_id, event_id);
