---
title: Deployment
slug: 08-deployment
lang: en
order: 8
summary: SQLite and PostgreSQL deployment, automatic migrations, backup and restore, single-binary distribution, resource targets, and upgrade boundaries.
---

# Deployment

Commands here use GNU/Bash style (Linux, macOS, Git Bash). In Windows PowerShell, replace `./target/release/lkb` with `.\target\release\lkb.exe` and `export NAME=value` with `$env:NAME = "value"`.

## 1. Shapes and Modes

| Shape | Composition | Fit |
| --- | --- | --- |
| solo | One process plus one SQLite file | Personal, offline, single machine |
| team / enterprise | One process plus one PostgreSQL database | Shared team use |
| Tenant isolation | One process and database (or schema) per tenant | The only tenant isolation available today |

`deployment.mode` must match `profile` (`solo` → SQLite; `team`/`enterprise_internal`/`enterprise_external` → PostgreSQL). The mode supplies `[features]` defaults and startup validation; `billing`, `audit`, and `connectors` actually gate behavior, the other features (`signup`, `sso`, `multi_tenant`, `public_api`, `mcp`, `public_share`) are config/display only, and `rate_limit` always applies. `enterprise_external` additionally requires `signup`, `billing`, `multi_tenant`, and `public_api`.

There is no dependency on container orchestration, a message queue, a cache service, or a vector service. `lkb` is the only runtime artifact.

## 2. SQLite on One Machine

The default database is `data/lkb.sqlite`, overridable with `LKB_SQLITE_PATH` or `storage.sqlite_path`. At startup:

1. Create the parent directory.
2. Open SQLite.
3. Run embedded migrations.
4. In solo, ensure the allow-all space `s_default` exists.
5. If `seed.enabled` and `seed.auto` are set and the database is empty, run the built-in seed (failures only log warnings).

Operational notes:

- One database file belongs to one tenant; do not attach it to multiple writing processes.
- Stop `serve` and any running import before copying the file.
- Keep the database out of imported source directories; the scanner already skips `data`, `blobs`, `.lkb`, `target`, `node_modules`, and hidden directories.
- Filesystem permissions are data permissions: the solo boundary is the operating-system user.

## 3. PostgreSQL (Unverified)

1. Prepare the database and account.
2. Set `storage.backend = "postgres"`.
3. Put the connection string in the variable named by `storage.url_env` (default `LKB_DATABASE_URL`).
4. Run any `lkb` subcommand; migrations run when the connection opens.

```bash
export LKB_DATABASE_URL='postgres://user:pass@localhost/lkb'
./target/release/lkb config check
./target/release/lkb serve
```

**Known limitations:** the integration test prints `SKIP postgres integration test: LKB_TEST_PG_URL is not set; refusing to pretend it passed` unless you set `LKB_TEST_PG_URL`; the PostgreSQL runtime path has not been exercised on this machine. The old `usage_events` 22-placeholder / 21-bind defect is fixed (22/22), and a static guard test audits SQL literals against `.bind()` chains in `src/store/postgres.rs` (140 queries pass). Real migrations and the full path still need a PostgreSQL run before production.

## 4. Migrations

- SQLite migrations live in `migrations/sqlite/` (0001–0009); PostgreSQL in `migrations/postgres/` (0001–0009); the logical schema is the same.
- Migrations are embedded in the binary and run automatically when the store opens; there is no separate command.
- Migrations are forward-only, with no down migration. Back up before upgrading; roll back by restoring the backup and running the old binary.
- Applied migration files must not be edited; add a new numbered file instead.

## 5. Backup and Restore

### SQLite

```bash
# stop all writers first
cp data/lkb.sqlite backup/lkb-$(date +%Y%m%d).sqlite
```

Restore by putting the file back with correct permissions. For a logical migration instead:

```bash
./target/release/lkb export --out backup/lkb.jsonl
./target/release/lkb import backup/lkb.jsonl --space s_default
```

Exported records include `doc_id`, path, content, ACL, validity, version, and `superseded_by`. If path and content are unchanged, `doc_id` is preserved and citations still resolve. The export does **not** include the Page/Revision tables, but documents compiled from wiki pages appear under `wiki/...` paths; revision history itself requires a database-level backup.

### PostgreSQL

Use `pg_dump`/`pg_restore` through the normal PostgreSQL process. Given the unverified runtime path above, verify the export/import cycle and the `usage_events` insert before relying on it.

### Restore Checks

- Counts: `imported + skipped + failed` matches the source entries.
- Identity: sample and compare `doc_id` sets.
- Retrieval: run `lkb eval` on a fixed question set and compare Recall@k and MRR with the baseline.
- Permissions: query as an unauthorized principal and confirm zero recall.

## 6. Single Binary and Seed

The release artifact is one file: `target\release\lkb.exe` on Windows, `target/release/lkb` elsewhere. The verified Windows release binary is about 27 MB; `target/` is about 12 GB of cache and must not be shipped. A deployment needs the binary, an optional `lkb.toml`, environment variables for the database URL and model keys, and a writable database directory or reachable PostgreSQL.

`lkb seed` embeds the 24 shipped product documents and writes them into `space=product` as Page/Revision plus Document/Chunk. It is idempotent, never overwrites user-edited pages, and records `seed_meta` (`seed_version`, `source_manifest_hash`, `applied_at`, counts). `GET /health` reports the same seed metadata. A failed seed never blocks startup.

## 7. Web Frontend Deployment

The Rust server does not serve `web/dist`; the SPA is a separate artifact.

```bash
cd web
npm install
npm run build        # tsc --noEmit && vite build → web/dist
npm run preview      # 127.0.0.1:4173 with the /api proxy
```

Deployment rules:

- Host `web/dist` on any static server and reverse proxy `/api` to the backend **on the same origin**, so cookie sessions and CSRF keep working.
- In development, `npm run dev` serves on `127.0.0.1:5173` and proxies `/api` to `LKB_API_TARGET` (default `http://127.0.0.1:7331`).
- `web/tools/preview-proxy.mjs`, `check-deployment.mjs`, `check-session-flow.mjs`, and `screenshot.mjs` are local verification tools; screenshots are written to `web/screenshots/` (14 files covering search, answer, article, editor, history, login, and admin pages in both themes).
- The frontend has no unit/E2E tests; treat the Node check scripts plus the backend test suite as the current coverage.

### Reverse Proxy and SSO Headers

With `[sso].mode = "trusted_header"`, the deployment must satisfy:

- The reverse proxy authenticates first and **overwrites** (never appends or passes through) the user/email/groups headers; client-supplied copies must be dropped.
- The app trusts only the TCP peer IP; add the proxy address (IP/CIDR) to `sso.trusted_proxies`. An invalid entry fails startup; an empty list is allowed to boot, but `serve` logs a WARN and every request carrying SSO headers returns 401.
- Do not expose the application port directly to clients; bind loopback or a private network and let the firewall admit only the proxy. The app strips a forged `x-lkb-sso-authenticated`, but network isolation is still required.
- `X-Forwarded-For` is not used for SSO trust (only for log IP hashing and login rate limiting); do not substitute it for the peer check.
- The proxy terminates TLS and preserves `Host`; failure codes and debugging are in [06 Authentication and Security](06-auth-security.md) and [11 FAQ and Troubleshooting](11-faq.md).

## 8. Observability

- `GET /health` is public and returns `{status, profile, backend, fts, seed}`; storage failure returns 503 `{"status":"unavailable","error":"storage_unavailable"}`.
- `GET /metrics` is public, `text/plain; version=0.0.4`, with no labels: `lkb_http_requests_total`, `lkb_http_5xx_total`, `lkb_rate_limited_total`, `lkb_queue_full_total`, `lkb_auth_failures_total`, `lkb_slow_queries_total`, `lkb_inflight_requests`. `lkb_slow_queries_total` currently has no caller and stays 0.
- Logs go to **stderr** as text or JSON (`LKB_LOG_FORMAT=json`). JSON fields include `request_id`, `trace_id`, `actor`, `token_id` (hash prefix), tenant/space, route/method/status, latency/db/queue, `degraded_codes`, `error_code`, `concurrency`, `ip_hash`, `user_agent`. Query text is never logged; only `query_hash` and `query_length`.
- Audit and runtime logs are separate; audit rows keep the raw IP while logs keep `ip_hash`.

## 9. Limits, Quotas, and Multi-Instance Caveats

Production deployments should know:

- Rate limiting is per process (in-memory token buckets and counters); restarting clears it and multiple instances do not share counts.
- Storage quota reservations are serialized by a process-wide mutex; the same caveat applies to multiple writers.
- Token quotas and the billing ledger are stored in the database and are consistent across restarts, but the PostgreSQL path is unverified.
- `[limits].max_queue` is defined but unused; 503 comes from concurrency exhaustion.

## 10. Resources and Scale

Design targets (not measured): solo ≤ 2,000 documents / 30,000 chunks; team ≤ 100,000 / 2,000,000; enterprise ≤ 10,000,000 / 200,000,000. The validated workload is the repository corpus plus 24 seed documents, with 115 tests on SQLite. Above roughly 50,000 solo chunks or sustained latency growth, evaluate moving to team rather than adding more index to solo.

## 11. Upgrades and Rollback

1. Back up the database and configuration.
2. Stop the old process.
3. Replace the binary.
4. Start and watch migration logs plus `/health` (including the `seed` block).
5. Run a fixed evaluation set.

Rollback requires restoring the backup and using the old binary, because migrations have no downgrade script. Confirm `lkb.toml` still passes `config check` first; stricter validation fails startup rather than running with an invalid configuration.

## 12. Related Documents

- [06 Authentication and Security](06-auth-security.md)
- [07 Configuration](07-configuration.md)
- [11 FAQ and Troubleshooting](11-faq.md)
