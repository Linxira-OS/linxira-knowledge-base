-- P0d: static summaries, summary jobs, conversations and messages.
CREATE TABLE IF NOT EXISTS document_summaries (
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    doc_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    lang TEXT NOT NULL,
    model TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    status TEXT NOT NULL,
    summary_json TEXT NOT NULL DEFAULT '{}',
    usage_event_id TEXT,
    generated_at TEXT,
    error TEXT,
    PRIMARY KEY (tenant_id, space_id, doc_id, revision_id, model, prompt_version)
);

CREATE TABLE IF NOT EXISTS summary_jobs (
    job_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    doc_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    attempts INTEGER NOT NULL DEFAULT 0,
    scheduled_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    error TEXT,
    UNIQUE (tenant_id, space_id, doc_id, revision_id)
);

CREATE INDEX IF NOT EXISTS idx_summary_jobs_status
    ON summary_jobs (status, scheduled_at);

CREATE TABLE IF NOT EXISTS conversations (
    conversation_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    title TEXT NOT NULL,
    external_session_id TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_owner
    ON conversations (tenant_id, owner_id, updated_at);

CREATE TABLE IF NOT EXISTS messages (
    message_id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    citations_json TEXT NOT NULL DEFAULT '[]',
    model TEXT,
    usage_json TEXT,
    degraded_json TEXT,
    request_id TEXT NOT NULL,
    mode TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation
    ON messages (conversation_id, created_at);
