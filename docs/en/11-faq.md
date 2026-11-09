---
title: FAQ and Troubleshooting
slug: 11-faq
lang: en
order: 11
summary: Common errors, degraded codes, and current limitations across build, configuration, import, retrieval, authentication, wiki, and data recovery.
---

# FAQ and Troubleshooting

Identify the stage first: build, config validation, seed, import, retrieval, summary/conversation, gateway/quota/billing, authentication, wiki, or recovery.

## 1. Build and Startup

### Linking fails and `link.exe` points somewhere odd

On Windows, if `where link.exe` finds the coreutils `link` shipped with Git, no real MSVC linker is available. Install MSVC Build Tools + Windows SDK, or use `cargo +stable-x86_64-pc-windows-gnu build --release` with MinGW gcc on `PATH`.

### Missing C compiler

SQLite is built bundled and still needs a C toolchain; Rust alone is not enough.

### Binding fails at startup

`server.bind` must be `host:port`, and the port must be free. The default is `127.0.0.1:7331`. Binding a non-loopback address with `auth.mode = "none"` is rejected.

### CLI exits with 1 or 2

Only three exit codes exist: 0 success, 1 any command error (`anyhow`/`bail!`), 2 clap usage errors or `seed`/`import` partial failures. Exit codes 3–7 discussed in older design notes are not implemented.

## 2. Configuration Validation

| Error meaning | Resolution |
| --- | --- |
| `profile solo requires storage.backend = "sqlite"` | solo cannot use PostgreSQL |
| `profile team/enterprise requires storage.backend = "postgres"` | team/enterprise cannot use SQLite |
| `deployment.mode ... is incompatible with profile ...` | Align mode and profile |
| `deployment.commercial = true requires features.billing = true` / `solo deployment cannot enable billing` | Fix the billing switch |
| `enterprise_external requires signup, billing, multi_tenant and public_api` | Enable all four |
| `enterprise deployments cannot disable audit` | Keep `features.audit = true` |
| `refusing to start: auth.mode = "none" while ... is not loopback` | No-auth is loopback-only |
| `storage backend is postgres but environment variable ... is not set` | Set `LKB_DATABASE_URL` or the configured variable |
| `i18n.supported_locales must include en as fallback` | Add `en` |
| `payment.currency must be an ISO 4217 three-letter uppercase code` | Use e.g. `USD` |

`config check` runs full validation and prints the effective configuration with masking; it does not connect to the database.

## 3. Seed and First Run

### `seed` reports `failed=1` with `seed_import_failed: space ... does not exist`

Only solo auto-creates `space=product`; team/enterprise must create the space first, or pass `--space` pointing at an existing one.

### `seed` reports `imported: 0` and warnings

Check the report: `unchanged` means the embedded content is identical; `user_modified` means the page has a non-seed revision and was deliberately skipped. Invalid embedded frontmatter aborts the whole run with `seed_import_failed: invalid embedded documentation`.

### Auto-seed did not run

Auto-seed requires `seed.enabled`, `seed.auto`, profile `solo`, and an empty database (no pages and no documents). Failures only log `auto seed failed; serving without seed`.

## 4. Import

### `imported: 0` and everything is `skipped`

- Extensions outside `.md`, `.markdown`, `.txt`, `.html`, `.htm`, `.pdf`.
- Hidden files, symlinks, or files under `target`, `node_modules`, `data`, `blobs`, `.lkb`.
- The same path and content were already imported (idempotence).
- PDF found but built without the `pdf` feature.

### `doc_id` changed

Only the canonical path or the content hash can change it. Moving or renaming a file produces a new `doc_id` with `superseded_by` on the old record.

### JSONL import fails

Each line needs string `path` and `content`; `acl` must be an object or parseable JSON string; `valid_from`/`valid_to` must be RFC3339.

### team import reports a missing space

Pass `--acl-json` when importing into a space that does not exist yet; team/enterprise never auto-create allow-all spaces.

## 5. Search, Answer, Streaming

### No results

Check that the import report has `imported > 0`, that `--lang`/`lang` matches the content language (a wrong or absent language filter changes the candidate set), and whether `degraded[]` contains `keyword_unavailable`.

### `keyword_exact_fallback`

Full-text search produced no rows, so literal substring matching ran. Category `quality`.

### `decision_unavailable`

`bm25_judge` uses gateway alias `retrieval.judge_alias` (default `default`); a missing alias, failed gateway call, or score-count mismatch produces this code.

### Event channel codes

`event_not_enabled` (no extractor), `event_extraction_unavailable` (no events/index missing), `event_no_seed` (no seed entity), `event_channel_timeout` (over `event_timeout_ms`). All fall back to other channels.

### Streaming returns HTTP 200 but an `error` event

SSE endpoints check auth/CSRF/limits inside the stream; failures arrive as `event: error` with `{code, message}`. Without `X-CSRF-Token`, a cookie-session write gets `forbidden` in-band.

### `stream_fallback`

Upstream does not support streaming; the answer was buffered and returned non-streaming.

### `conversation_rewrite_failed`

Multi-turn rewriting failed (or `rewrite_enabled` produced an empty result); the original query was used. This is a plain string code in the query response's `degraded` array.

## 6. Summaries and Conversations

### Page summary returns `status:"pending"`

No summary row is ready yet; the response includes `summary_generation_failed` with `detail:"pending"`. Run `lkb summary run` to advance the queue, or check `lkb summary list --status queued`.

### Summary stays `failed`

Generation failed after `summary.max_attempts`; the extractive fallback is still served, and `model="extractive"`. Check the gateway alias `summary.model_alias` and provider keys.

### Conversation returns 403

`GET /api/v1/conversations/{id}` returns 403 for deleted conversations and for anyone who is not the owner (admins can restore). `POST /api/v1/query` additionally requires an active conversation whose space matches the request.

## 7. Gateway, Quota, Billing, Payment

### `model_unavailable` / `model_forbidden` / `model_invalid_request`

The alias is missing or has no healthy route; the access policy denied tenant/space/role; or the upstream returned a deterministic 4xx. Provider topology and base URLs are never exposed.

### `model_rate_limited` / `model_timeout`

Upstream 429 or 5xx/timeout; the gateway opens a 30-second circuit and tries the next route, returning 429/503 with `Retry-After: 30` when all fail.

### `quota_exceeded` (429, Retry-After 60)

A hard token quota (tenant/space/user/role) was exceeded. Soft quotas only audit `quota.soft_exceeded`.

### `payment_required` (402)

A wallet or entitlement exists and balance/included tokens are insufficient, or the entitlement is `suspended`/`cancelled`. Top up or activate a plan.

### `payment_not_configured` / `payment_provider_unavailable`

WeChat/Alipay return `payment_not_configured`; unknown providers (including PayPal) return `payment_provider_unavailable`. Stripe requires `STRIPE_SECRET_KEY` (or the configured variable). Stripe refund and subscription sync are **not implemented**; only the webhook refund path (which writes a reversing ledger entry) exists.

### `lkb billing checkout` always fails

The CLI command intentionally ends with `payment_provider_unavailable: payment adapter is not implemented in this release`. Use the HTTP `/api/v1/billing/checkout` route with a configured provider instead.

### `feature_disabled` / `billing_not_enabled`

Billing routes require `features.billing`; the billing webhook additionally requires `billing.enabled`. Audit queries require `features.audit`, and connector endpoints require `features.connectors`.

## 8. Authentication, Roles, Spaces

### Login succeeds but requests still return 401

Over plain HTTP to a non-loopback host, Secure cookies are not sent. Use HTTPS or loopback.

### 403 CSRF / Origin

Cookie-session writes need `csrf_token` (form) or `X-CSRF-Token` (JSON) plus the `lkb_csrf` cookie; Origin must equal Host when present. Bearer requests are exempt.

### SSO 401: peer / user / auto-provision / disabled

- `untrusted peer`: the TCP peer is not in `sso.trusted_proxies` → 401. Confirm the proxy address (IP/CIDR) is listed and that no second proxy rewrites the peer.
- `missing user`: the configured user header is missing or empty → 401. The proxy must inject it; header names are configurable and case-insensitive.
- `auto_provision = false` and the user does not exist → 401: pre-create the user (match by email, `source=sso` external_id, or username), or enable provisioning.
- Disabled user → 401: have an administrator re-enable the account.
- Invalid `[sso]` configuration (mode, empty header, proxy IP/CIDR, default_role, group_role_map, email domain, TTL=0) fails startup or `config check` immediately; an empty `trusted_proxies` only logs a WARN, but every SSO request returns 401.
- When all three SSO headers are absent, SSO does not run; a normal unauthenticated request returns 401.

### SSO 403: email domain / identity conflict

- With `allowed_email_domains` non-empty, email must exist and its domain must match exactly (case-insensitive, no subdomain wildcard); a missing email or `evil.example` returns 403.
- An existing user with a different email → 403 `email_conflict`; fix the identity or email binding first.

### Forged headers and direct exposure

- `x-lkb-sso-authenticated` is always stripped; spoofing it grants nothing.
- Calling the app port directly with SSO headers from an untrusted peer → 401. Never expose the app port to clients.
- `GET /api/v1/auth/sso/status` publicly returns `{enabled, mode}` for verification; `features.sso` does not enable SSO (`[sso].mode` does). OIDC/SAML/SCIM are not implemented.

### SSO sessions and writes

A successful login issues an HttpOnly session cookie (and rotates the old session); writes still require normal session CSRF. TTL comes from `sso.session_ttl_hours`, falling back to `auth.session_ttl_hours`.

### `last_space_owner_protected` / `last_group_admin_protected` (409)

You cannot remove or demote the last space owner or group admin. Promote a replacement first.

### `space_not_active` (409)

The space is archived or soft-deleted; restore it before writing.

### `space_quota_exceeded` / `tenant_quota_exceeded` (413 or 429)

Storage byte dimensions return 413; count dimensions (members/spaces) return 429. Inspect `space usage` and `quota_overrides`. Soft storage enforcement currently does not block.

## 9. Wiki

### Saving over JSON returns 400 or 409

`If-Match` is required; a missing header returns 400 ("If-Match header is required") and a stale `current_revision_id` returns 409 `revision_conflict`. Re-read the page and retry. The legacy HTML form path has no precondition and is last-write-wins.

### Page list looks empty

`GET /api/v1/pages?lang=` filters languages **after** fetching the page rows (up to 500; it is not a SQL filter); a stale `lang` parameter can hide pages. Also check space membership (`viewer` at minimum).

## 10. Data and Backup

### Does `export` include wiki pages?

It exports document records, not the Page/Revision tables; wiki content appears as `wiki/...` documents. Revision history needs a database backup.

### Is `doc_id` preserved after restore?

Yes when path and content are unchanged; exports carry `doc_id` and imports compare it.

### Can I delete the SQLite file to rebuild the index?

No. That also deletes pages, revisions, users, tokens, sessions, usage, and billing rows. Rebuild by re-importing sources or exports.

## 11. Degraded Code Reference

| Code | Category | When it appears |
| --- | --- | --- |
| `keyword_unavailable` | evidence | Keyword index unavailable |
| `keyword_exact_fallback` | quality | No full-text rows; literal matching used |
| `dense_not_enabled` | evidence | No embedding provider |
| `embedding_unavailable` | evidence | Query embedding failed |
| `index_missing` | evidence | No chunk vectors |
| `decision_unavailable` | quality | Judge missing or failed |
| `llm_unavailable` | quality | Generation failed |
| `citation_alignment_failed` | quality | Generated citations could not be aligned |
| `acl_second_check_dropped` | security | Secondary ACL check discarded candidates |
| `event_not_enabled` | evidence | Event extraction not enabled |
| `event_extraction_unavailable` | evidence | Extraction or index unavailable |
| `event_no_seed` | evidence | No seed entity matched |
| `event_channel_timeout` | performance | Event channel exceeded its budget |
| `stream_fallback` | performance | Upstream does not support streaming |
| `summary_generation_failed` | quality | Static summary pending or failed |
| `conversation_rewrite_failed` | quality | Multi-turn rewrite failed (string code) |

Sort priority is security > evidence > quality > performance. `malformed_hit` exists in the contract but currently has no call site.

## 12. Error Code Quick Reference

| Status | Codes |
| --- | --- |
| 400 | `bad_request`, `invalid_query`, `invalid_top_k`, `model_invalid_request`, `invalid_billing_request`, `payment_invalid` |
| 401 | `unauthorized` |
| 402 | `payment_required` |
| 403 | `forbidden`, `feature_disabled`, `billing_not_enabled`, `model_forbidden` |
| 404 | `not_found` |
| 409 | `conflict`, `revision_conflict`, `space_not_active`, `last_space_owner_protected`, `last_group_admin_protected` |
| 410 | `client_disconnected` |
| 413 | `space_quota_exceeded`, `tenant_quota_exceeded` (bytes) |
| 429 | `rate_limited`, `quota_exceeded`, count quotas, `model_rate_limited` |
| 500 | `internal`, `billing_unavailable`, `quota_check_failed` |
| 503 | `queue_full`, `timeout`, `model_unavailable`, `model_timeout`, `payment_not_configured`, `payment_provider_unavailable`, `storage_unavailable` (`/health`) |

## 13. Connectors

### `connector_not_configured`

`test` returns HTTP 200 with `{"ok":false,"status":"not_configured",...}`; `sync` ends as `failed`. The `status` field is always `not_configured` on failure; the `code` distinguishes the cause: vendor stubs return `connector_not_configured`, while config/file/HTTP problems return `connector_invalid_config`, `connector_io_error`, or `connector_http_error`. The working kinds are `filesystem_export`, `jsonl`, and `http_json`.

### `connector_invalid_config` / `connector_io_error` / `connector_http_error`

- `connector_invalid_config`: `config` is not valid JSON, missing `dir`/`path`/`base_url`, or the manifest/JSONL item shape is wrong; create/patch usually returns 400 `bad_request`.
- `connector_io_error`: the `filesystem_export` manifest or `jsonl` file cannot be read.
- `connector_http_error`: the `http_json` GET failed, returned non-2xx, or exceeded 15 seconds.

### Sync job `failed` / `partial`

`POST /api/v1/connectors/{id}/sync` returns 202 and runs in the background. Inspect `GET /api/v1/connectors/{id}/jobs?limit=50` for `status` and `stats_json`: `items`, `indexed`, `refs_only`, `failed`, `errors[]`. Item-level codes include `connector_invalid_path`, `connector_index_failed`, and `connector_ref_failed`. `partial` means some items succeeded; items without `content` become `refs_only`, which is expected.

### ACL denies or documents are not searchable

When an item lacks `acl` or its ACL is invalid, the connector uses an empty allow list (deny) and never falls back to allow-all; `tests/connectors.rs` asserts this. Even an ACL-permitted item also needs a space role of at least `viewer` (space role ∩ document ACL). `external_refs` still records the reference, but the document may be invisible or unindexed.

### 403 `feature_disabled` / admin scope

Connector endpoints require admin scope and are gated by `features.connectors`; solo defaults it off, team/enterprise default it on. Enable it explicitly in `[features]`, otherwise the API returns 403 `feature_disabled`.

### Deep links and originals

No drive or attachment original is stored; only the item `content` text, the `url` deep link, `revision`, ACL snapshot, and hashes. Query `GET /api/v1/external-refs?doc_id=&provider=&limit=` for `external_refs.url`; `citations[].source_url` is not populated today.

## 14. Known Limitations

- PostgreSQL runtime is unverified (SKIP); the `usage_events` bind defect is fixed (22/22) and the static SQL placeholder/bind guard passes, but a real PostgreSQL run is still required.
- The connector framework is implemented (`filesystem_export`/`jsonl`/`http_json`); Feishu/SharePoint/OneDrive/Confluence/MediaWiki adapters are stubs; SSO is trusted-header only (OIDC/SAML/SCIM and HTTP-level multi-tenancy are not implemented); only `billing`, `audit`, and `connectors` feature flags actually gate behavior.
- WeChat/Alipay are `not_configured`; Stripe refund/subscription sync and PayPal are unavailable; no tax, invoice PDF, or dunning.
- `lkb auth login --device` returns `not_implemented`; the CLI is local-first.
- The Rust server does not serve `web/dist`; the SPA is a separate static deployment. The frontend has no unit/E2E tests.
- Answer cache absent (`cache_hit` always false); semantic caching absent.
- `summary.timeout_ms`, `[limits].max_queue`, `payment.mode`, `payment.secret_env`, and `billing.grace_days` are configured but unused; storage soft enforcement does not block; `lkb_slow_queries_total` has no caller.
- Rate limits and storage quota reservations are per process; multi-instance deployments over-admit.
- Conversations are not audited; claim storage and claim APIs do not exist; `citations[].source_url` is not populated.

## 15. Related Documents

- [03 Quickstart](03-quickstart.md)
- [05 Search and AI Answers](05-search-ai.md)
- [06 Authentication and Security](06-auth-security.md)
- [08 Deployment](08-deployment.md)
