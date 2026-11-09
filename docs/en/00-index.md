---
title: Documentation Index
slug: 00-index
lang: en
order: 0
summary: English entry point and reading order for the Layered Knowledge Base product documentation.
---

# Layered Knowledge Base Documentation

This documentation ships with the software and is also the compile-time seed source for `lkb seed`: after first start you can read and search it in `space=product`. It describes what the current build actually does; capabilities that are not implemented or not tested are stated as boundaries, not as plans.

## 0. How to Use This Documentation

- First run: follow [03 Quickstart](03-quickstart.md) to build, run the first seed, import, search, and start the server.
- Evaluation: start with [01 Overview](01-overview.md), then read [08 Deployment](08-deployment.md).
- Agent integration: read [05 Search and AI Answers](05-search-ai.md) (unified query and SSE), [06 Authentication and Security](06-auth-security.md) (tokens and roles), and [10 Architecture and Implementation](10-architecture.md) (MCP, OpenAPI, gateway).
- Troubleshooting: go straight to [11 FAQ and Troubleshooting](11-faq.md).

## 1. Documentation Map

| No. | Document | Content |
| --- | --- | --- |
| 01 | [Overview](01-overview.md) | Positioning, three profiles, four deployment modes, what it does and does not do |
| 02 | [Core Concepts](02-concepts.md) | space, page, revision, chunk, doc_id, claim, Answer, degraded, validity, plus roles, quotas, aliases, ledger, summaries, and conversations |
| 03 | [Quickstart](03-quickstart.md) | Build, seed, import, search, ask, evaluate, serve, build the frontend, MCP |
| 04 | [Built-in Wiki](04-wiki.md) | Page creation, editing, history and restore, If-Match concurrency, permissions, language |
| 05 | [Search and AI Answers](05-search-ai.md) | Five retrieval modes, unified query, SSE, static summaries, citations, `knowledge_as_of` |
| 06 | [Authentication and Security](06-auth-security.md) | JSON login, cookie/CSRF/Bearer, roles, rate limits, audit, TLS proxy, fail-closed |
| 07 | [Configuration](07-configuration.md) | Every `lkb.toml` field, environment variables, model gateway, limits, i18n, payment and billing switches |
| 08 | [Deployment](08-deployment.md) | SQLite/PostgreSQL, migrations, backup, single binary, static frontend, observability |
| 09 | [External Sources and Connectors](09-connectors.md) | Feishu, SharePoint, OneDrive, Confluence, MediaWiki boundaries |
| 10 | [Architecture and Implementation](10-architecture.md) | L1–L8, Store/Provider/Policy, gateway/quota/billing, summaries/conversations, data flows and modules |
| 11 | [FAQ and Troubleshooting](11-faq.md) | Common errors, degraded codes, error-code reference, known limitations |

## 2. Languages and Version

- Chinese lives in `docs/zh/`; English lives in `docs/en/`. Slugs match one-to-one, and commands, fields, and section numbers stay aligned.
- `lkb seed` embeds exactly these 24 files (12 per language) at compile time, writes them as Page/Revision into `space=product`, and derives searchable Document/Chunk records.
- The English edition is an adaptation for English readers, not a word-for-word translation. Technical terms are introduced in English on first use.
- Both editions describe the current implementation in this repository (`lkb` 0.2.0). For commands, trust [03 Quickstart](03-quickstart.md) and `lkb --help`.

## 3. Reading Order

The shortest path is 01 → 03 → 05. Read 06, 07, and 08 before handing the system to a team; read 09 when connecting external systems; read 10 when changing code or judging implementation boundaries; go to 11 for errors.

## 4. What Is New in This Release

| Capability | Entry point | Status |
| --- | --- | --- |
| Built-in seed knowledge base | `lkb seed`; auto-seed on first `serve` of an empty solo database | Implemented, `space=product` |
| Roles, groups, space members, self-service spaces, storage quotas | `lkb group/space ...`, `/api/v1/spaces|groups` | Implemented (verified on SQLite) |
| Model gateway (public alias, routing, failover) | `/api/v1/gateway/chat`, `lkb models ...` | Implemented |
| Token quotas and usage metering | `lkb usage/quota`, `/api/v1/admin/*` | Implemented |
| Wallet/plans/subscription/payment adapters | `/api/v1/billing/*`, admin Billing tab | Local ledger implemented; payment only stripe/manual/mock |
| Static summaries | `lkb summary ...`, page summary endpoint | Implemented, extractive fallback |
| Unified query / conversation history / SSE | `POST /api/v1/query`, `/query/stream` | Implemented |
| Contract REST and JSON login | `/api/v1/auth/*`, `/api/v1/pages` (If-Match) | Implemented |
| MCP and OpenAPI | `lkb mcp serve`, `/openapi.json` | Implemented (MCP is a minimal read + page-write set) |
| Deployment modes, feature gates, i18n | `[deployment]`, `[features]`, `[i18n]` | Modes and validation implemented; `billing`/`audit`/`connectors` actually gate behavior, other features (signup/sso/multi_tenant/public_api/mcp/public_share) are config/display only |
| Structured logs, `/metrics`, rate limits | `LKB_LOG_FORMAT`, `GET /metrics` | Implemented (per-process counters) |
| Web frontend (React SPA) | `web/`, builds to `web/dist` | Implemented; no new frontend tests |

## 5. Related Documents

- [01 Overview](01-overview.md)
- [03 Quickstart](03-quickstart.md)
- [11 FAQ and Troubleshooting](11-faq.md)
