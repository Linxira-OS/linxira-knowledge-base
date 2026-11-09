---
title: Overview
slug: 01-overview
lang: en
order: 1
summary: lkb is a layered AI knowledge base driven by one retrieval core, with solo, team, and enterprise profiles from a single laptop to multi-tenant deployment.
---

# Overview

## 1. One-Sentence Positioning

`lkb` (Layered Knowledge Base) is an AI knowledge base driven by one retrieval core: one data model, one set of governance fields, one retrieval pipeline, and one answer contract. The `solo`, `team`, and `enterprise` profiles (configuration tiers) plus the `solo`, `team`, `enterprise_internal`, and `enterprise_external` deployment modes cover everything from a single laptop to a commercially operated service.

It is not three products and not a stripped-down enterprise branch. Profile differences appear only in configuration defaults and adapter wiring; domain logic never branches on profile or mode, it only reads feature gates.

## 2. What Problem It Solves

- Enterprise architectures are too heavy for individuals: answering a question about a local folder should not require an external database, a vector service, a cache, and a model server.
- Personal tools cannot carry governance: tenants, spaces, members, roles, ACLs, validity windows, versions, and quotas cannot be added later without rebuilding indexes and backfilling permissions.
- Missing components have undefined behavior: optional capabilities must be reported in `degraded[]` so humans and agents know what is missing.
- Services are hard to sell: signup, plans, usage, quotas, and payment boundaries must exist, and they must not leak into domain logic.

The result: governance fields are written from day one; files are the source of truth and indexes are rebuildable; missing optional capabilities degrade explicitly; humans and agents share one Answer contract; runtime concerns such as audit, quotas, and i18n sit in the assembly layer.

## 3. Three Profiles and Four Deployment Modes

| Dimension | solo | team | enterprise |
| --- | --- | --- | --- |
| Storage | One SQLite file | PostgreSQL | PostgreSQL |
| Deployment | Single process on one machine | Service plus database | Service plus database |
| Identity | Local-only by default; optional session and Bearer | Local session + Bearer, roles/groups/space members | Same as team; multi-tenancy via separate deployments |
| Governance | Fixed tenant `t_local`, fixed space `s_default`, explicit allow-all | Configured `tenant_id`; space members and roles | Configured `tenant_id`; isolation via separate config or database |
| Self-service spaces/groups | Forced off at startup | On by default, can be disabled | On by default |

`deployment.mode` is decoupled from `profile` but must match it: `solo` requires SQLite; `team`/`enterprise_internal`/`enterprise_external` require PostgreSQL. The mode provides `[features]` defaults and participates in startup validation:

| Mode | signup | billing | connectors | sso | multi_tenant | public_api | mcp | audit | rate_limit | public_share |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `solo` | — | — | — | — | — | — | — | on | on | — |
| `team` | — | — | on | — | — | — | — | on | on | — |
| `enterprise_internal` | — | — | on | on | on | on | on | on | on | — |
| `enterprise_external` | on | on | on | on | on | on | on | on | on | on |

Implementation boundaries that must be stated:

1. **`billing`, `audit`, and `connectors` currently gate behavior.** `signup`, `sso`, `multi_tenant`, `public_api`, `mcp`, `rate_limit`, and `public_share` exist in configuration and in `GET /api/v1/features`, but no route or domain path reads them; rate limiting always runs through `[limits]`. Connector APIs read `connectors` and return 403 `feature_disabled` when it is off.
2. **Enterprise is not a separate implementation.** `enterprise` behaves like `team`; real SSO, HTTP-level multi-tenancy, a policy engine, and WORM audit are not implemented.
3. **Scale figures are design targets, not measurements.** The validated workload is the repository corpus plus the 24 seed documents.

## 4. What It Does

- **Content on first start**: `lkb seed` writes 24 built-in product documents into `space=product`; an empty solo database auto-seeds on the first `serve`. Failures only log warnings and never block startup.
- **Imports local knowledge**: Markdown, TXT, HTML, JSONL; PDF requires a build with the `pdf` feature. Directory imports skip hidden files, symlinks, and `target`, `node_modules`, `data`, `blobs`, `.lkb`; `--lang` sets the language.
- **Connector framework**: `filesystem_export`, `jsonl`, and `http_json` work today; sync writes `external_refs`, deep links, and ACL snapshots, indexes `content` as documents when present, and never downloads originals. Feishu/SharePoint/OneDrive/Confluence/MediaWiki remain config-validation stubs that return `connector_not_configured`.
- **Keyword and enhanced retrieval**: SQLite FTS5 with `bm25()`; PostgreSQL `tsvector` with `ts_rank_cd`; optional judge reranking, event-entity channel, and RRF fusion. The unified `POST /api/v1/query` supports `search`, `answer`, and `both`.
- **Cited answers and streaming**: extractive or OpenAI-compatible generative; `/api/v1/answer/stream` and `/api/v1/query/stream` are SSE. Failed generation or citation alignment falls back to extractive with a degraded record.
- **Static summaries**: generated asynchronously after import, wiki save, and seed into `document_summaries`; limited retries, then an extractive fallback that keeps the `status=failed` record.
- **Built-in wiki**: space / page / revision (split by language), Markdown editing, history and old revision bodies, archive/restore, and `If-Match` concurrency control on the JSON API.
- **Roles and collaboration**: tenant roles owner/admin/user; group roles group_admin/group_member; space roles space_owner/space_admin/editor/viewer; groups can own spaces; explicit members beat group-derived roles.
- **Storage quotas**: tenant → group → space → user, with storage_bytes/documents/chunks/members/spaces; reserve before write, release on failure; 413/429 when exceeded.
- **Model gateway and metering**: only aliases are public; provider/provider_model/route control plane, priority and failover, `usage_events`, and token quotas by tenant/space/user/role (429).
- **Billing (off by default)**: local double-entry wallet/ledger/plans/subscriptions/entitlements; insufficient balance returns 402; external engine modes emit events and do not write local financial tables.
- **Conversation history**: `conversations`/`messages`, external session binding (Zeta `external_session_id`), multi-turn query rewriting, soft delete and restore, cursor pagination.
- **Contract REST and JSON login**: `/api/v1/auth/{csrf,login,me,logout}`, page JSON CRUD (If-Match/409), `/api/v1/search|answer|query`.
- **Agent access**: `lkb mcp serve` exposes stdio MCP (kb_search/kb_answer/kb_get_page/kb_get_page_summary/kb_get_page_history/kb_list_spaces/kb_write_page); `/openapi.json` is OpenAPI 3.1 covering a subset of real public/admin routes.
- **Local web**: `serve` provides HTML pages and the JSON API; the React SPA in `web/` builds to `web/dist`, hosted by a separate static server or reverse proxy.
- **Observability and protection**: `/health` (including seed metadata), `/metrics`, structured logs (JSON/text, request IDs, redaction), six-dimension rate limiting (429/503), and audit events.

Minimal commands (platform details in [03 Quickstart](03-quickstart.md)):

```bash
target/release/lkb config check
target/release/lkb seed                     # 24 documents into space=product
target/release/lkb search "doc_id 如何派生"
target/release/lkb ask "没有 LLM 时回答如何生成，引用是什么"
target/release/lkb serve                    # http://127.0.0.1:7331/; empty solo DB auto-seeds
```

## 5. What It Does Not Do

The current build explicitly does not provide or has not verified:

- **Real vendor connectors**: the framework is implemented and `filesystem_export`/`jsonl`/`http_json` work; Feishu, SharePoint, OneDrive, Confluence, and MediaWiki remain config-validation stubs whose `test`/`sync` return `connector_not_configured`. There is no vendor OAuth, API crawl, attachment download, or automatic ACL sync. See [09 External Sources and Connectors](09-connectors.md).
- **SSO / multi-tenancy**: no OIDC, SAML, SCIM, or workload identity, and no HTTP-level tenant routing; the `sso` and `multi_tenant` switches change nothing.
- **Payment channels**: WeChat/Alipay return `payment_not_configured`; PayPal returns `payment_provider_unavailable`; Stripe only has Checkout plus HMAC webhook verification, with refund and subscription sync unimplemented; no tax computation, invoice PDF, or dunning.
- **PostgreSQL runtime is unverified**: the integration test prints SKIP unless `LKB_TEST_PG_URL` is set. The old `usage_events` bind-count mismatch (22 placeholders / 21 binds) is fixed, and a static guard test audits literal SQL against `.bind()` chains in `src/store/postgres.rs` (140 queries pass). There is still no PostgreSQL runtime on this machine, so migrations and the full path need a real PG run before production.
- **Device-code login**: `lkb auth login --device` returns `not_implemented`; `auth` only reads and writes a local config file token.
- **Remote CLI mode**: the CLI is local-first (it opens the local store); the `base_url` in the auth config file is display-only and performs no HTTP calls.
- **Multimodality**: no OCR, ASR, video keyframes, or image vectors.
- **Vector index**: no sqlite-vec/pgvector acceleration; hybrid dense search is in-process cosine.
- **Rich-text collaboration**: no comments, approvals, CRDT, link sharing, or plugin marketplace.
- **Answer cache**: `cache_hit` is always `false`; semantic caching is not implemented.
- **Rust-hosted SPA**: `serve` does not serve `web/dist`; the SPA must be hosted separately with `/api` reverse proxied.
- **Frontend automated tests**: `web/` has no unit or E2E tests, only four Node check scripts.

## 6. The Boundary of "One Core"

"One core" is a checkable constraint:

- Domain code contains no `if profile == solo` or `if mode == ...`; differences live in config normalization, validation, and adapter wiring.
- The `Store`, `Provider`, and `Policy` abstractions serve both database backends and every retrieval mode; the newer gateway, quota, billing, access, and audit components plug in the same way.
- Governance fields exist on documents, chunks, events, pages, and revisions; recall filters, including the language filter, are generated by `Policy`.
- The Answer field set is constant; degradations go into `degraded[]`, while identity and ACL failures are rejected outright.

## 7. Related Documents

- [02 Core Concepts](02-concepts.md)
- [03 Quickstart](03-quickstart.md)
- [08 Deployment](08-deployment.md)
- [10 Architecture and Implementation](10-architecture.md)
