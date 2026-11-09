---
title: Core Concepts
slug: 02-concepts
lang: en
order: 2
summary: Defines space, page, revision, chunk, doc_id, claim, Answer, degraded, and validity, and how they relate.
---

# Core Concepts

This document defines the terms used throughout the product and their boundaries in this build. Implementation details are in [10 Architecture and Implementation](10-architecture.md).

## 1. Space and Members

A space is the isolation unit for permissions, quotas, and lifecycle, identified by `space_id`. Recall queries, wiki pages, documents, chunks, and events all carry it.

- solo always uses `s_default`, created automatically as an explicit allow-all space.
- team and enterprise use `storage.default_space` and may create other spaces.
- Visibility is `private` (default, members only) or `internal` (the tenant directory shows name/description/owner, but reading content still requires permission).
- Status is `active`, `archived` (read-only), or `deleted` (soft delete).
- A space owner is a `user` or `group` (`owner_type`/`owner_id`); `settings` can set the group-derived default role (`group_member_space_role`, default `viewer`, changeable to `editor`).

Memberships live in `space_members(space_id, principal_type, principal_id, role)` with principal types `user`, `group`, and `agent`. Space membership and document ACL are **two** filters: both must pass to read content. A wiki page's document ACL is currently `allow_all`, so its effective permission is the space role; imported documents may carry real ACLs.

## 2. Page and Revision

A page is the wiki's editing container; what gets retrieved is its revisions.

| Object | Key fields | Meaning |
| --- | --- | --- |
| Page | `tenant_id`, `space_id`, `lang`, `slug`, `title`, `current_revision_id`, `version`, `acl_json`, `status`/`valid_to` | `(tenant, space, lang, slug)` is unique; `version` starts at 1 |
| Revision | `revision_id`, `lang`, `markdown`, `content_hash`, `author`, `created_at`, `version`, `acl_json`, `source_url`, `source_revision` | Immutable snapshot written on save |

Saving: version increments, a new revision is written, the page points at it, and the same Markdown is compiled into Document/Chunk records and indexed. `revision_id` looks like `rev_` plus 32 hex characters and is exactly what the JSON API's `If-Match` header takes.

`author = system:seed` marks built-in seed pages; user edits use the real `principal_id`, and seed upgrades never overwrite them.

## 3. Document, Chunk, and doc_id

A document is the retrievable logical object, produced from a local file, a JSONL package, a seed page, or a wiki page. Chunks are line-oriented (at most 1000 characters by default) and record `line_start`/`line_end`, so citations return to source lines.

`doc_id` uses a fixed derivation:

```text
canonical_key = relative path under the source root, "/" separators, absolute paths/drive letters/".." rejected, ASCII lower-cased
content_hash  = SHA-256 of source content (Markdown bytes for wiki pages)
doc_id        = first 16 bytes of sha256(canonical_key + "\0" + content_hash), lowercase hex
```

- The same path with the same content, imported again or reindexed, keeps the same `doc_id` and counts as `skipped`.
- New content at the same path produces a new `doc_id`; the old document becomes `superseded` with `superseded_by` pointing to the new ID.
- A wiki page's logical path is `wiki/{space}/{lang}/{slug}.md`; seed documents use the same shape.
- Path canonicalization version `p1` deliberately does not apply Unicode NFC normalization.
- Chunk IDs start with `chk_` and page revision IDs with `rev_`; neither participates in `doc_id`.

## 4. Claim

A claim is a locatable assertion with author, review status, validity window, and evidence locations, separating human confirmation from raw text.

**Current build boundary**: there is no claim table and no write, query, or review API. The current `conflicts[]` check is "multiple distinct `doc_id` values under the same title", not a claim conflict. Treat claim as a data-model concept, not a delivered feature.

## 5. Answer

`lkb ask`, `POST /api/ask`, `POST /api/v1/answer`, and `/api/v1/query` return the same structure:

| Field | Type | Meaning |
| --- | --- | --- |
| `answer` | string or null | Body; `null` when there is no evidence |
| `citations` | array | `doc_id`, `chunk_id`, `path`, `line_start`, `line_end`, `quote`, optional `source_url` |
| `confidence` | object | `score`, `band` (`high`/`medium`/`low`), `basis[]` |
| `conflicts` | array | Conflicts; currently title-based |
| `degraded` | array | Degradations; an empty array when healthy |
| `knowledge_as_of` | string or null | Minimum `valid_from` among cited documents; `null` means not applicable |
| `mode` | string | `extractive` or `generative` (`hybrid` is defined but not produced at runtime) |
| `model` | optional string | The public alias on a successful generative answer |
| `usage` | optional object | `input_tokens`/`output_tokens`/`cached_tokens`/`token_source`/`cost_micros`/`currency` |
| `cache_hit` | bool | Always `false` today; no answer cache exists |
| `conversation_id`/`message_id` | optional strings | Set when bound to a conversation |
| `summary_kind`/`summary_source` | optional strings | `dynamic`; source is `llm` or `extractive` |

An answer cites at most five hits; a generative answer needs at least one valid citation or it falls back to extractive. `confidence` is a heuristic score, not a calibrated probability (`high ≥ 0.70`, `medium ≥ 0.40`).

## 6. Degraded

When an optional capability is disabled, unavailable, or times out, a record is appended to `degraded[]`:

| Field | Meaning |
| --- | --- |
| `code` | Stable code, such as `decision_unavailable` |
| `category` | `security`, `evidence`, `quality`, `performance` |
| `capability` | Affected capability: `judge`, `event`, `dense`, `summary`, `stream` |
| `expected_level` / `actual_level` | Expected and actual capability levels |
| `reason` | `not_enabled`, `unavailable`, `timeout`, `model_missing`, `budget_exceeded`, `stale`, `error` |
| `impact` / `fallback` | Effect and the actual fallback path |

Sort order is fixed: `security` > `evidence` > `quality` > `performance`. Authentication and ACL failures are not degradations; they return 401/403/409. Besides `Degraded` objects, the `/api/v1/query` `degraded` array can also contain the plain string code `conversation_rewrite_failed`.

## 7. Validity

Documents, chunks, and events carry `valid_from` and `valid_to`:

- Timestamps are RFC3339 UTC, second precision, ending in `Z`.
- Recall requires `valid_from <= as_of` and (`valid_to` is null or `valid_to > as_of`).
- `as_of` defaults to now; the CLI takes `--as-of`.
- A null `valid_to` means "unknown end", not "valid forever".
- Archiving a page uses `valid_to` as the tombstone; a soft-deleted space still counts toward storage until purged.

## 8. Roles, Groups, and Space Members

Roles form three independent layers:

| Layer | Roles | Scope |
| --- | --- | --- |
| Tenant | `owner` > `admin` > `user` (agents are expressed by token kind) | The whole tenant: create groups/spaces, read audit, manage users |
| Group | `group_admin` > `group_member` | One group: group spaces and members |
| Space | `space_owner` > `space_admin` > `editor` > `viewer` | One space: pages, members, quota, sharing |

Effective space role precedence is fixed: **explicit `space_members` row > group-derived > `owner_type`**; with multiple groups the highest wins; tenant admins get break-glass as `space_owner` with an audit entry.

## 9. Quotas: Storage and Tokens

The two quota systems are completely separate:

| System | Governs | Dimensions/scopes | On exceed |
| --- | --- | --- | --- |
| Storage `space_quotas`/`storage_usage` | Space content size | tenant/group/space/user × storage_bytes/documents/chunks/members/spaces | Byte dimensions: 413 `space_quota_exceeded`/`tenant_quota_exceeded`; count dimensions: 429 |
| Token `token_quotas`/`quota_counters` | Model calls | tenant/space/user/role × tokens/requests/cost, `hard`/`soft` | Hard: 429 `quota_exceeded` with `Retry-After: 60` |

Storage quotas reserve before a write, recompute on success, and release on failure; the default is unlimited. Admins can add `quota_overrides` (with reason/expires_at, audited). Token quotas reserve an estimate before the call, commit actual usage, and release on failure.

## 10. Model Aliases, Providers, and Routes

The gateway separates the public model name from internal providers:

| Concept | Meaning |
| --- | --- |
| Alias | The stable `public_id` clients use (for example `default`); this is what Answer's `model` field reports |
| Provider | A vendor or endpoint: base_url, api_key_env, priority, weight, timeouts |
| Provider Model | The concrete upstream model with context window and internal cost |
| Route | A path from alias to provider_model with priority, weight, health, and circuit state |

Requests resolve alias → access policy → priority/weight; 429, 5xx, and timeouts move to the next route and open a 30-second circuit; deterministic 400/401/403 errors are not retried. Clients never see base_url, upstream_model, or keys.

## 11. Usage, Wallet, Ledger, Plans, and Subscriptions

- `usage_events` records every model call: tenant/space/user, alias, provider_model_id, input/output/cached tokens, `token_source` (`provider` or `estimated`), cost, latency, status, and `source` (`kb`/`judge`/`summary`).
- `wallets` plus `ledger_entries` form a double-entry ledger: every group balances, `idempotency_key` is unique, entries are append-only, and refunds are reversing entries.
- `plans` define price and included tokens; `free`, `starter`, and `pro` ship as defaults.
- `subscriptions` follow `trial → active → past_due → suspended → cancelled`; illegal transitions return 400.
- `entitlements` are the plan snapshot: charges draw from plan-included tokens first, then the wallet; insufficient balance returns 402 `payment_required`.
- `billing.engine = local` writes the local ledger; any other value only emits events and reads the local entitlement snapshot without writing financial tables.

## 12. Summaries, Conversations, and Messages

- **Static summary**: `document_summaries` is unique by `(tenant, space, doc_id, revision_id, model, prompt_version)`; `summary_jobs` drives async generation through `queued → running → ready/failed`; failures retry a limited number of times, then fall back to extractive and keep `status=failed`.
- **Dynamic answer**: every query goes through multi-turn rewriting, retrieval, evidence fusion, generation, and citation alignment; `summary_kind=dynamic`.
- **Conversations**: `conversations` (tenant/space/owner/title/`external_session_id`/status) and `messages` (role/content/citations/model/usage/degraded/mode/`rewritten_query`/`conversation_context_hash`). Soft delete sets `status=deleted` and can be restored.
- **External session**: `external_session_id` maps a Zeta (or similar) agent session; the KB stores user-visible messages only, never chain-of-thought.

## 13. Deployment Modes, Feature Gates, and Locale

- `deployment.mode` (`solo`/`team`/`enterprise_internal`/`enterprise_external`) provides `[features]` defaults and startup validation; domain logic only calls `feature_enabled`.
- `billing`, `audit`, and `connectors` actually gate behavior; the remaining features (`signup`, `sso`, `multi_tenant`, `public_api`, `mcp`, `public_share`) appear only in configuration and `GET /api/v1/features` and change nothing; `rate_limit` always follows `[limits]`.
- **UI locale and content `lang` are different things**: locale (`zh-CN`/`en`) controls interface language and API error messages; `lang` (`zh`/`en`) is content metadata that drives the `lang` filter in recall SQL.

## 14. How the Concepts Relate

```text
tenant
  ├── user / group (space_members, group_members)
  ├── space (owner_type/owner_id, status, settings)
  │     ├── page (tenant, space, lang, slug) ── revision (lang, content_hash, If-Match)
  │     │        └── document (wiki/{space}/{lang}/{slug}.md) ── chunk (lang, line range)
  │     ├── document (local file / JSONL / seed)
  │     └── event / entity (optional extraction, derived index)
  ├── model_aliases → routes → provider_models → providers
  ├── usage_events → token_quotas / space_quotas
  ├── wallets / ledger_entries / plans / subscriptions / entitlements
  └── conversations ── messages (citing Answer and citations)

One query: Policy(tenant, space, ACL, as_of, lang) → candidates → Answer(citations, degraded, knowledge_as_of)
```

## 15. Related Documents

- [04 Built-in Wiki](04-wiki.md)
- [05 Search and AI Answers](05-search-ai.md)
- [06 Authentication and Security](06-auth-security.md)
- [10 Architecture and Implementation](10-architecture.md)
