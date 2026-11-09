-- SPDX-License-Identifier: AGPL-3.0-only
-- SQLite keyword index. The trigram tokenizer makes substring matching work
-- for Chinese text without a separate dictionary, and bm25() still ranks rows.
-- If this SQLite build lacks FTS5 (or trigram), the store records the index as
-- unavailable and recall reports keyword_unavailable instead of silently
-- falling back to a different ranking.
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    chunk_id UNINDEXED,
    doc_id UNINDEXED,
    tenant_id UNINDEXED,
    body,
    tokenize = 'trigram'
);
