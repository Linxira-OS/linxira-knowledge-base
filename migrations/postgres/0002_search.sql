-- PostgreSQL keyword index. This is tsvector + GIN + websearch_to_tsquery +
-- ts_rank_cd, not BM25. The 'simple' configuration has no Chinese word
-- segmentation; the store reports keyword_exact_fallback when it must use the
-- substring fallback for such queries.
CREATE INDEX IF NOT EXISTS idx_chunks_tsv ON chunks USING GIN (tsv);
