---
title: External Sources and Connectors
slug: 09-connectors
lang: en
order: 9
summary: "Boundaries for Feishu, SharePoint, OneDrive, Confluence, and MediaWiki: references and deep links only, no drive storage, source ACLs authoritative."
---

# External Sources and Connectors

The connector framework is implemented: the source system remains authoritative, and this system stores only text snapshots, locators, deep links, and ACL mirrors. It does not download originals, take over external permissions, or write back. Real vendor adapters (Feishu, SharePoint, OneDrive, Confluence, MediaWiki) are configuration-validation stubs today; the working source kinds are `filesystem_export`, `jsonl`, and `http_json`.

## 1. Design Principles

1. **The source system is authoritative.** Content, permissions, and versions belong to the source; this system stores text, locators, hashes, and ACL snapshots.
2. **References and deep links only.** Do not download and retain drive attachments, synchronize bidirectionally, or write back.
3. **Source ACLs are authoritative.** Read item ACLs during sync and write them onto documents; when they cannot be read, deny.
4. **Do not embed third-party runtimes.** Connectors exchange data only over HTTP or files; Confluence, MediaWiki, and drive services do not run inside the process.
5. **Rebuildable and idempotent.** Re-syncing the same `external_id` updates the reference and document instead of duplicating them.

## 2. Connector Kinds and Status

Complete `kind` values (`SUPPORTED_KINDS`):

| kind | Status | Data source | Notes |
| --- | --- | --- | --- |
| `filesystem_export` | **Working** | `manifest.json` under `config.dir` | Reads an export manifest; `config.manifest` overrides the file name |
| `jsonl` | **Working** | `config.path` | One JSON object per line |
| `http_json` | **Working** | `config.base_url` | GET returning an array or `{"items":[...]}`; optional `config.api_key_env` |
| `feishu` | Config-validation stub | — | `test`/`sync` return `connector_not_configured` |
| `sharepoint` | Config-validation stub | — | Same |
| `onedrive` | Config-validation stub | — | Same |
| `confluence` | Config-validation stub | — | Same |
| `mediawiki` | Config-validation stub | — | Same |

**What stub means precisely**: records can be created, patched, and deleted, and `test`/`sync` can be called, but no vendor OAuth or API request is made. `test` returns HTTP 200 with `ok:false`, `status:"not_configured"`, and `code:"connector_not_configured"`; `sync` ends as `failed` with the same code. Real Feishu/SharePoint/OneDrive/Confluence/MediaWiki integration is not implemented.

## 3. Configuration Fields

| Field | Required | Meaning |
| --- | --- | --- |
| `space_id` | yes | Target space; must already exist |
| `kind` | yes | One of the values above |
| `name` | yes | Unique per tenant (`UNIQUE(tenant_id, name)`) |
| `config` / `config_json` | no | Connector configuration object; must be valid JSON |
| `secret_env` | no | Environment variable holding the secret; the API never stores the value. Built-in connectors do not read this field; `http_json` authentication uses `config.api_key_env` |
| `status` | no | Defaults to `active` |
| `connector_id` | no | Generated as `conn_` plus 10 hex characters |

`config` by kind:

- `filesystem_export`: `dir` required; `manifest` defaults to `manifest.json`. Create/patch reads the manifest for validation.
- `jsonl`: `path` required; parsed line by line, blank lines skipped.
- `http_json`: `base_url` required; optional `api_key_env` sends `Authorization: Bearer` when non-empty; 15-second timeout.

Item fields (manifest entries, JSONL lines, or HTTP items): `external_id` (also accepts `id`/`path`/`url`), `url` (deep link), `title`, optional `revision`, `acl`, `content`, `mime`, `lang`. Body text is indexed only when `content` is present and non-empty; otherwise only a reference is recorded.

Responses redact `config` keys containing `secret`/`token`/`api_key`/`password`/`authorization` and return `secret_configured` to show whether the `secret_env` variable is set. Secret values are neither returned nor stored. `secret_env` is currently storage/status metadata only; actual authentication follows each connector's `config`.

## 4. Sync, External References, and Deep Links

`POST /api/v1/connectors/{id}/sync` returns **202** with `{job_id, status:"queued"}` and runs in the background. Items are independent: one failure does not stop the rest.

| Outcome | Behavior |
| --- | --- |
| `content` present | Upsert a document at `connectors/{kind}/{safe_external_id}.md` with `source_role=external` and `lang` defaulting to `zh`; chunk, optionally embed, and enqueue a summary |
| No `content` | Write only `external_refs`, counted as `refs_only`; no searchable document |
| Every item | Upsert `external_refs`: `provider=kind`, `external_id`, `url` (deep link), `revision`, `acl_json`, `fetched_at`, `status`, `doc_id` (empty for refs-only) |
| Pagination | 100 items per page, at most 50 pages (5,000 items) |
| Job status | `completed` (no failures), `partial` (some failures with successes), `failed` (all failed or no successful item) |
| Stats | `items`, `indexed`, `refs_only`, `failed`, `errors[]` |

Sync is idempotent: the same `external_id` updates the content-addressed document and the `(tenant, provider, external_id)` reference, so repeated runs do not duplicate documents (`tests/connectors.rs` covers this).

The deep link is the item `url`, stored in `external_refs.url`; answer citations still point at `doc_id` and line ranges and do not require the external system to be online.

## 5. Source ACLs Are Authoritative

- When an item carries `acl`, it is parsed with `Acl::from_json` into an ACL snapshot written onto the document and reference.
- When `acl` is missing or invalid, an **empty allow list** (deny) is used; there is no fallback to allow-all. `tests/connectors.rs` asserts "missing source ACL must deny".
- Final access is still two layers: space member role (at least `viewer`) ∩ document ACL. Add users/groups to the target space after sync.
- There is no ACL synchronization daemon: re-sync after source changes, and never write back to the source.

## 6. HTTP API and Feature Gate

Every connector endpoint requires **admin scope** and is gated by `features.connectors`; when disabled it returns 403 `feature_disabled`.

| Method and path | Purpose |
| --- | --- |
| `GET /api/v1/connectors` | List connectors (config redacted) |
| `POST /api/v1/connectors` | Create; validates kind, space, JSON, and constructor |
| `PATCH /api/v1/connectors/{id}` | Update name/space/status/secret_env/config |
| `DELETE /api/v1/connectors/{id}` | Delete the record |
| `POST /api/v1/connectors/{id}/test` | Connectivity test; stubs return 200 + `connector_not_configured` |
| `POST /api/v1/connectors/{id}/sync` | Queue and run in the background; 202 |
| `GET /api/v1/connectors/{id}/jobs?limit=50` | Job history |
| `GET /api/v1/external-refs?doc_id&provider&limit=200` | External references and deep links |

`features.connectors` defaults: off for solo, on for team/enterprise; override explicitly in `[features]`. It is a gate for the connector API, not evidence that vendor adapters are ready.

## 7. Current Boundaries

- Only `filesystem_export`, `jsonl`, and `http_json` read data; the five vendor kinds are stubs and there is **no real Feishu/SharePoint/OneDrive/Confluence/MediaWiki integration**.
- `http_json` performs a single GET: no pagination, OAuth refresh, or attachment download; an external export script or gateway must shape the JSON.
- No binary originals, incremental webhooks, write-back, or automatic ACL sync.
- Connector output is `Document/Chunk` (`source_role=external`); it creates no Page/Revision and never enters built-in wiki history.
- `citations[].source_url` is not populated; deep-link data lives in `external_refs.url` and the document `source_uri`.

## 8. Migration Checklist

- [ ] Every source space has an owner, item count, and ACL source.
- [ ] The export package keeps original formats and SHA-256 and can be re-verified.
- [ ] Items carry a stable `external_id` and `url`; items without ACL are treated as deny.
- [ ] After sync, count `items/indexed/refs_only/failed`; failures carry `external_id` and an error code.
- [ ] Add users/groups to the target space (space role ∩ document ACL is the real visibility).
- [ ] Run `lkb eval` on a fixed question set and record Recall@k and MRR baselines.
- [ ] Re-sync does not duplicate documents and keeps `external_refs` counts stable.

## 9. Related Documents

- [02 Core Concepts](02-concepts.md)
- [06 Authentication and Security](06-auth-security.md)
- [08 Deployment](08-deployment.md)
- [11 FAQ and Troubleshooting](11-faq.md)
