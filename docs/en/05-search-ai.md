---
title: Search and AI Answers
slug: 05-search-ai
lang: en
order: 5
summary: Explains bm25, bm25_judge, event_entity, hybrid, and auto, plus citations, AI summaries, and knowledge_as_of.
---

# Search and AI Answers

Retrieval is the center of the product. Four facts explain a response: the requested mode, the mode that ran, the capability that was missing, and whether the answer is a static summary or a dynamic answer. They map to `mode_requested`, `mode_used`, `degraded[]`, and `summary_kind`.

## 1. The Five Retrieval Modes

| Mode | Requires | Behavior | When unavailable |
| --- | --- | --- | --- |
| `bm25` | Keyword index | The always-available base recall; SQLite uses FTS5 with `bm25()`, PostgreSQL uses `tsvector` with `ts_rank_cd` | If the keyword index is unavailable, `keyword_unavailable` is recorded and evidence is empty |
| `bm25_judge` | Keyword plus judge alias | Recalls first, then lets the judge rerank the top 30 (a rerank window, not a recall cap); candidate membership is unchanged | Records `decision_unavailable` and keeps the original order |
| `event_entity` (alias `event`) | Extractor plus indexed events | Picks seed entities, expands 1–2 hops over shared entities, aggregates events, and maps back to chunks | Disabled writes `event_not_enabled`; unavailable extraction writes `event_extraction_unavailable`; no seeds writes `event_no_seed`; timeout writes `event_channel_timeout`; all fall back |
| `hybrid` | Keyword plus optional dense, optional event, optional judge | Fuses candidate lists with RRF (`k = 60`, configurable weights); a configured judge reranks Top 30 after fusion by default | Missing channels are pruned and reported; with none left only keyword remains; absent judge is not degraded |
| `auto` | Backend and configured capabilities | Resolves to one of the modes above | See the next section |

The CLI and HTTP accept these values. `--mode event` equals `event_entity`; `--mode keyword` equals `bm25`. Language filtering uses `--lang` / `lang` and applies inside the recall SQL.

## 2. How Modes Resolve

`auto` resolution is deterministic:

1. PostgreSQL backend → `hybrid`.
2. SQLite with a configured embedding provider and at least one visible stored vector → `hybrid`.
3. Otherwise, SQLite with a configured extractor and events in the database → `hybrid`.
4. Anything else → `bm25`.

solo defaults to `retrieval.mode = "auto"`, so without models it runs `bm25`; a configured embedding provider with indexed vectors is enough to enable `hybrid`. An explicitly requested mode is honored.

Note: on SQLite, `auto` probes only the caller's visible tenant/spaces. If an embedding provider is configured but no stored vector is visible, the mode falls through to the extractor/events check (or `bm25`) and writes `index_missing`, so the empty dense channel is never silent. On PostgreSQL, `auto` always resolves to `hybrid`; without an embedding provider the dense channel degrades and keyword retrieval still works.

## 3. The Keyword Channel

- SQLite builds an FTS5 index and orders by `bm25()`; lower scores rank higher.
- PostgreSQL builds `tsvector` and a GIN index and orders by `ts_rank_cd`; higher scores rank higher. Scores from the two backends are not comparable.
- Queries first fetch `limit × 3` candidates, then apply space, ACL, validity, and language filters and truncate.
- When a full-text query cannot cover short or Chinese fragments, both backends may fall back to literal substring matching and record `keyword_exact_fallback`. Substring hits are never disguised as BM25 scores.

## 4. Judge Reranking (through the gateway)

`bm25_judge` changes only order, never candidate membership. The judge is called through `retrieval.judge_alias` (default `default`) with the query and truncated snippets of up to 30 candidates; it must return the same number of scores. The top 30 is the judge rerank window, not a recall cap. Failure or a count mismatch writes `decision_unavailable` and keeps the original order. Judge usage is written to `usage_events` with `source=judge`.

A judge cannot recover documents missed during recall. If the correct answer is absent from the candidate set, reranking cannot invent it.

## 5. The event-entity Channel (Optional)

event-entity requires both:

1. A configured extractor (`[models.extractor].provider` is not `none`; if `api_key_env` is set, that variable must exist).
2. Successful extraction at import time, with non-superseded events in the database.

At query time, normalized terms select seed entities, `event_entities` expands one hop (`event_depth = 2` allows two), scores aggregate by shared-entity weight and event text relevance, and results map back to `chunk_id`. Seeds and expansions apply tenant, space, ACL, validity, and language filters inside SQL first.

Events only provide candidates; the Answer contract and citation anchors are unchanged. The channel is off by default; requesting `event` while disabled writes `event_not_enabled` and falls back; extraction failure writes `event_extraction_unavailable` without touching the keyword index.

## 6. Hybrid Fusion

`hybrid` fuses available channels by rank using Reciprocal Rank Fusion:

- The keyword channel always participates when it has results.
- The dense channel participates only with a configured embedding provider: query embedding plus in-process cosine over stored chunk vectors, with a cap of `max(limit × 20, 100)`.
- The event channel participates only when an extractor is configured and events exist.
- Each channel contributes `weight / (60 + rank)`; the same `chunk_id` accumulates score across channels.

After fusion, a configured judge reranks the Top 30 when `retrieval.judge_after_hybrid = true` (default); membership and the tail are unchanged. Hybrid does not require a judge, so an absent judge writes no degradation; a configured judge that fails writes `decision_unavailable`. Set `judge_after_hybrid = false` to keep pure RRF order.

There is no separate vector index. No embedding provider writes `dense_not_enabled`; a failed query embedding writes `embedding_unavailable`; no stored vectors write `index_missing`. None of these block keyword results.

## 7. Citations and Answers

An answer runs retrieval once, then cites up to the first `min(5)` hits. Without an LLM:

- `mode = extractive`; the body is assembled from hit snippets as `[n] path:start-end`.
- Every citation carries `doc_id`, `chunk_id`, path, line range, and source snippet; `source_url` is not populated today.

With `[models.llm]` materialized as the gateway alias `default`, answers are generated through the gateway and must return strict JSON: `{"answer": "...", "citations": [1,2]}`. Validation: citation indices must be in range with at least one; duplicates are removed; invalid JSON, no citations, or out-of-range indices write `citation_alignment_failed` and fall back to extractive; call failure writes `llm_unavailable`. On success, Answer adds `model` (the public alias) and `usage`; `cache_hit` is always `false` (no answer cache exists).

## 8. Unified Query and SSE

`POST /api/v1/query` is the unified entry point. Its body fields are `query`, `space`, `lang`, `conversation_id`, and `mode` (`search` | `answer` | `both`, default `both`). The response includes `conversation_id`, `message_id`, `user_message_id`, `mode`, `rewritten_query`, `conversation_context_hash`, `degraded`, `results`, and `answer`.

Streaming uses `POST /api/v1/query/stream` and `GET /api/v1/answer/stream`. SSE event order:

1. `meta`: `{conversation_id, message_id, mode, rewritten_query}`
2. `results`: search results (`null` when `mode=answer`)
3. `token` (0..n): `{"content": "<delta>"}`
4. `citation` (0..n): one event per citation
5. `usage`: tokens/cost (may be `null`)
6. `degraded`: degradation array
7. `done`: `{conversation_id, message_id}`

Once streaming starts the HTTP status is always 200; authentication, CSRF, and rate-limit failures arrive as `event: error` frames with `{code, message}`. On client disconnect the gateway releases quota reservations and writes a `status=cancelled` usage event; when upstream does not support streaming, the answer is buffered and `stream_fallback` is recorded.

## 9. Multi-Turn Rewriting

`[query]` controls rewriting: `rewrite_enabled` (default on), `rewrite_alias` (default `default`), and `context_messages` (default 6). The system joins the most recent N messages into context and rewrites the query through the gateway; on success the rewritten text is stored as `rewritten_query` on the usage event and message. On failure it falls back to the original query and adds the string code `conversation_rewrite_failed` to `degraded` (never blocking the answer).

## 10. Static Summaries

Static summaries are separate from dynamic answers:

- Trigger: after import/extraction, wiki revision save, or seed, a `summary_jobs` row is enqueued; `lkb serve` starts a background worker (every 5 seconds, batch size `summary.batch`).
- Generation: through gateway alias `summary.model_alias` (default `default`) with `usage_events.source=summary`; failures retry up to `summary.max_attempts`, then keep `status=failed` while storing an extractive summary (`tldr`/`bullets`/`keywords`, `fallback: true`).
- Read: `GET /api/v1/pages/{space}/{slug}/summary?lang=`. When ready it returns the summary record; when pending it returns `status:"pending"`, `summary:null`, and `summary_generation_failed` in `degraded`.
- CLI: `lkb summary run --limit 16` advances the queue; `lkb summary list --status queued` lists jobs.

A static summary must never be presented as a live answer; a dynamic answer may use it as a hint, but citations still point at source chunks.

## 11. knowledge_as_of

`knowledge_as_of` is the minimum `valid_from` among cited documents, formatted as RFC3339 UTC. It answers "when did this evidence first become effective", not "how fresh is the index" or "how trustworthy is the answer". `null` means not applicable (usually no citations).

## 12. Reading Degradations

Read `mode_used` first, then `degraded[]`:

```text
mode: auto (used bm25)
degraded:
  - dense_not_enabled (evidence): semantic candidates are not available
```

Common cases:

| Symptom | Meaning |
| --- | --- |
| `keyword_exact_fallback` | Full-text search returned nothing; literal substring matching was used |
| `decision_unavailable` | The judge is missing or the gateway call failed; order is unchanged |
| `event_not_enabled` / `event_extraction_unavailable` / `event_no_seed` / `event_channel_timeout` | Event channel disabled/unavailable/no seed/over budget; other channels continue |
| `dense_not_enabled` / `embedding_unavailable` / `index_missing` | Dense disabled/embedding failed/no vectors |
| `stream_fallback` | Upstream does not support streaming; the answer was buffered |
| `summary_generation_failed` | Static summary pending or failed; an extractive fallback exists |
| `conversation_rewrite_failed` | Multi-turn rewriting failed; the original query was used (string code) |
| `acl_second_check_dropped` | Secondary check discarded candidates; security category, sorted first |

## 13. Related Documents

- [02 Core Concepts](02-concepts.md)
- [04 Built-in Wiki](04-wiki.md)
- [07 Configuration](07-configuration.md)
- [11 FAQ and Troubleshooting](11-faq.md)
