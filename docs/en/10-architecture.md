---
title: Architecture and Implementation
slug: 10-architecture
lang: en
order: 10
summary: "For readers who want to understand the implementation: L1–L8 layers, Store/Provider/Policy abstractions, write and query data flows, and code module boundaries."
---

# Architecture and Implementation

This document is for people who already use the product and want to know why it behaves the way it does. It describes the structure that exists in the actual code.

## 1. Overall Layers

```text
Entry points (CLI / HTML / JSON API / SPA / MCP)
   │
   ├── request middleware: request id, Content-Language, IP rate limit, structured logging, metrics
   ├── L1 Access and identity: cookie session / Bearer / solo principal, roles, feature gates
   ├── L2 Query understanding: retrieval mode, language filter, multi-turn rewrite
   ├── L3 Multi-channel retrieval: keyword / dense / event
   ├── L4 Fusion and reranking: RRF, judge, secondary ACL check
   └── L5 Answer and presentation: extractive or generative, citations, SSE, degradation
          │
L6 Knowledge base (import, wiki, seed, summaries, conversations) → L7 Storage and index → L3
L8 Evaluation and operations (eval, health, metrics, audit)
X Cross-cutting (tenant, ACL, time, quota, logging, i18n)
```

## 2. Responsibilities of L1–L8

| Layer | Current implementation |
| --- | --- |
| L1 Access and identity | axum routes; HTML session, JSON auth, Bearer, solo principal; `access.rs` resolves tenant/group/space roles; feature gates for billing/audit/connectors |
| L2 Query understanding | Retrieval-mode resolution, language filter, `[query]` multi-turn rewrite through the gateway |
| L3 Retrieval | Keyword SQL, optional embeddings and cosine, optional event/entity expansion; per-channel budgets |
| L4 Fusion and reranking | RRF (k=60) for hybrid, judge for `bm25_judge` (top 30) through `retrieval.judge_alias`, `Policy::can_read` recheck |
| L5 Answer | First five hits become citations; generative answers via gateway alias; SSE streaming; conflicts, confidence, degradation |
| L6 Knowledge base | File/JSONL scanning, wiki pages, seed pages, connector sync, chunking, summary jobs, conversations |
| L7 Storage and index | SQLite or PostgreSQL; one logical schema (migrations 0001–0010); gateway/usage/quota/billing/connector tables |
| L8 Evaluation and operations | Recall@k/MRR, `/health` (seed metadata), `/metrics`, audit queries, structured logs |
| X Cross-cutting | Governance fields, visibility predicate, language filter, RFC3339 time, quotas, audit, i18n |

**PostgreSQL runtime is unverified** (tests SKIP without `LKB_TEST_PG_URL`); the `usage_events` bind defect is fixed and the static placeholder/bind guard passes, but a real PG run is still required.

## 3. Core Abstractions

### Store, Provider, Policy

- `Store` is implemented by `SqliteStore` and `PostgresStore`; it covers spaces/pages/revisions, documents/chunks, search, events, users/sessions/tokens, plus the newer gateway, usage, quota, billing, summary, and conversation tables.
- `ProviderSet` holds optional `llm`, `embedding`, `judge`, and `extractor` clients; `[models.llm]` is materialized into the gateway as provider `legacy` / alias `default`.
- `Policy` builds the single visibility predicate (tenant, space, validity, language, ACL) and `can_read` performs the in-process second check.

### Newer service components

| Component | Role |
| --- | --- |
| `gateway` | alias → route resolution, access policy, priority/weight/failover, OpenAI-compatible calls, usage events |
| `token_quota` | reserve/commit/release of tokens/requests/cost across tenant/space/user/role |
| `billing` | double-entry ledger, plans/subscriptions/entitlements, usage charging, 402 on insufficient balance |
| `payment` | `PaymentProvider` implementations (stripe/manual/mock) and webhook verification |
| `access` | tenant/group/space role resolution and `allowed_space_ids` |
| `quota` | storage quota check/reserve/release/recompute |
| `audit` | best-effort `audit_events` writer and query |
| `summary` | static summary jobs and the background worker |
| `conversation` | Conversation/message persistence and query rewriting |
| `connectors` | External source reads, references/deep links, ACL snapshots, optional content indexing, sync jobs |
| `logging` / `metrics` / `limits` | JSON/text events, Prometheus counters, token-bucket and concurrency limits |
| `i18n` | locale resolution, stable message keys, formatting helpers |
| `mcp` / `openapi` | stdio MCP server and the OpenAPI 3.1 document |

## 4. Write Data Flow

```text
lkb seed / import / wiki save
  → frontmatter or source parsing (lang)
  → canonical path p1 → content_hash → doc_id
  → chunker (1000 chars, line ranges, lang)
  → optional embedding / event extraction
  → quota reserve → upsert_document (+ storage recompute) → summary job enqueue
```

`upsert_document` semantics: same path and content → `skipped`; same path new content → new `doc_id` with the old document `superseded`; new path → new identity. Wiki saves go through the same compile step under `wiki/{space}/{lang}/{slug}.md` and additionally write Page/Revision. Seed writes `author=system:seed` revisions and never overwrites user revisions (`user_modified`).

### Connector Sync

```text
POST /api/v1/connectors/{id}/sync
  → 202 + job_id (connector_jobs: queued)
  → background run_sync: list_items paginated (100/page, max 50 pages)
  → per item:
       acl_snapshot(item.acl) (missing/invalid → empty allow list = deny)
       upsert external_refs(provider, external_id, url/deep link, revision, acl, fetched_at)
       content present → connectors/{kind}/{safe_id}.md document + chunks + optional embedding → summary job
       no content → refs_only
  → aggregate stats {items,indexed,refs_only,failed,errors[]}
  → job status completed / partial / failed
```

Connectors never download binary originals or write back to the source; content comes only from the item's `content` text, and the deep link is stored in `external_refs.url`.

## 5. Query Data Flow

```text
CLI / HTML / JSON API / MCP
  → Config + Store + ProviderSet
  → authenticate (session / Bearer / solo) → feature gates → limits
  → Policy(principal, as_of, lang)
  → optional multi-turn rewrite via gateway (conversation_id present)
  → recall: keyword(limit×3) + dense + event
  → RRF / judge (top 30) → can_read second check → truncate
  → extractive or gateway-generated answer + citations
  → persist user/assistant messages; write usage_events; SSE events
```

CLI and HTTP call the same library functions; the interfaces hold no index handles and never filter on their own. SSE endpoints check auth/CSRF inside the stream and emit failures as `event: error` frames.

## 6. Model Gateway Flow

1. Resolve the alias; missing/inactive → 503 `model_unavailable`.
2. Apply the access policy (`allow_all` or tenant/space/role lists); denied → 403 `model_forbidden`.
3. Build candidates from active providers/models, skipping open circuits; sort by priority and weighted pick.
4. Call `/chat/completions` through the provider; 429/5xx/504 open a 30-second circuit and move to the next route; deterministic 4xx are returned as 400/403.
5. On success write `usage_events`, optionally charge the local ledger, commit token quota, and map the response back to the alias.

If wallets or entitlements exist, a pre-call balance gate can return 402 `payment_required`; token quotas return 429 `quota_exceeded` with `Retry-After: 60`. Both run before the provider call.

## 7. Summary Jobs and Conversations

- Summary: import/wiki/seed enqueue `summary_jobs`; the `serve` worker (5-second loop, `summary.batch`) generates through alias `summary.model_alias`, retries up to `summary.max_attempts`, then stores an extractive fallback with `status=failed`.
- Conversations: `POST /api/v1/query` (and its stream twin) loads recent messages, rewrites the query when enabled, persists user and assistant messages, and returns/streams `results` plus `answer`. `external_session_id` maps an external agent session.

## 8. Module Map

| Module | Responsibility |
| --- | --- |
| `config` | Defaults, environment, normalization, validation, masked description |
| `governance` | Principals, roles, ACL, policy predicate, language filter |
| `store` | `Store` trait and both SQL backends |
| `ingest` / `frontmatter` / `seed` | Import, export, frontmatter parsing, seed embed and idempotency |
| `providers` / `gateway` | Direct providers and the alias-based gateway |
| `search` / `answer` / `contract` | Recall, fusion, answers, output contract |
| `access` / `quota` / `audit` | Roles, storage quota, audit |
| `token_quota` / `billing` / `payment` | Model quota, ledger, payment adapters |
| `summary` / `conversation` | Static summaries, conversations and rewriting |
| `connectors` | External source sync, `external_refs`, ACL snapshots and deep links |
| `web` / `cli` / `mcp` / `openapi` | HTTP, CLI, MCP, OpenAPI |
| `logging` / `metrics` / `limits` / `i18n` / `error` | Observability, protection, localization, error mapping |

## 9. Invariants

- **No profile/mode branches** in domain code; differences live in config, gates, and adapters.
- **Governance fields** on documents, chunks, events, pages, and revisions; `lang` is part of the schema and the recall predicate.
- **One visibility predicate** for every recall path; `can_read` is only a second check.
- **Stable doc_id** from path and content hash; changed content produces a new ID and a `superseded_by` link.
- **Explicit degradation** for optional capabilities; auth/ACL failures fail closed.
- **One Answer contract** across CLI, HTML, JSON API, and MCP.
- **Usage and quota accounting** happen before/after model calls, never inside the provider client.
- **Connectors store references and text only**: sync writes `external_refs`/deep links/ACL snapshots and builds documents only from item `content`; no binary downloads, no write-back, and missing ACLs deny.

## 10. Backend Differences and Known Gaps

| Dimension | SQLite | PostgreSQL |
| --- | --- | --- |
| Full-text search | FTS5, `bm25()`, lower first | `tsvector` + GIN, `ts_rank_cd`, higher first |
| Placeholders | `?` | `$1`, `$2`, … |
| Event text index | FTS5 trigram | Generated `tsv` column + GIN |
| Migration parity | 0001–0010 | 0001–0010 |
| Runtime status | Verified by the test suite | **Unverified**; static placeholder/bind guard passes, real PG still required |

Locking and quota serialization are per process (in-memory mutexes and counters), so multi-instance deployments need external coordination for exact limits.

## 11. Related Documents

- [02 Core Concepts](02-concepts.md)
- [05 Search and AI Answers](05-search-ai.md)
- [07 Configuration](07-configuration.md)
- [09 External Sources and Connectors](09-connectors.md)
