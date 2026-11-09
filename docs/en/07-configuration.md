---
title: Configuration
slug: 07-configuration
lang: en
order: 7
summary: Every lkb.toml field with defaults, environment overrides, model providers, and key handling rules.
---

# Configuration

There is one configuration entry point: `lkb.toml`. If the file is absent, defaults apply. Environment variables override the file, then normalization and validation run. A validation failure stops startup.

## 1. Load Order

1. Read the file passed to `--config` (default `lkb.toml`).
2. Parse TOML if it exists; otherwise use built-in defaults.
3. Apply `LKB_*` environment overrides.
4. Normalize: infer `profile` from the backend, force solo governance IDs, tighten limits for non-solo, resolve `deployment.mode` and feature defaults.
5. Validate; exit on failure.

When `profile` is unset, SQLite infers `solo` and PostgreSQL infers `team`. solo normalization forces `tenant_id = t_local`, `default_space = s_default`, and disables self-service groups/spaces. When `deployment.mode` is unset: `solo → solo`, `team → team`, `enterprise → enterprise_internal`; `enterprise_external` only via explicit configuration.

## 2. profile, server, auth

| Field | Default | Notes |
| --- | --- | --- |
| `profile` | unset; inferred | `solo` / `team` / `enterprise` |
| `server.bind` | `"127.0.0.1:7331"` | Must be `host:port` |
| `auth.mode` | `"none"` | `none` or `session`; `none` allows loopback only |
| `auth.cookie_name` | `"lkb_session"` | Session cookie name |
| `auth.cookie_secure` | `false` | Non-loopback Hosts add Secure automatically |
| `auth.session_ttl_hours` | `168` | Must be greater than zero |
| `auth.csrf` | `true` | Origin validation still runs when disabled |

### `[sso]` (trusted-header SSO)

| Field | Default | Notes |
| --- | --- | --- |
| `sso.mode` | `"off"` | `off` or `trusted_header`; any other value fails startup |
| `sso.user_header` | `"X-Auth-Request-User"` | Identity header; empty fails startup |
| `sso.email_header` | `"X-Auth-Request-Email"` | Identity header; empty fails startup |
| `sso.groups_header` | `"X-Auth-Request-Groups"` | Identity header; empty fails startup |
| `sso.trusted_proxies` | `[]` | Proxy IP/CIDR allowed to inject headers; an invalid entry fails startup; an empty list is allowed, but in `trusted_header` mode `serve` logs a WARN and SSO requests are denied |
| `sso.auto_provision` | `true` | Auto-create unknown users (`source=sso`) |
| `sso.default_role` | `"user"` | Role for auto-created users; must be `owner`/`admin`/`user` |
| `sso.group_role_map` | `{}` | Non-empty group → `owner`/`admin`/`user`; invalid key/value fails startup; highest match wins |
| `sso.allowed_email_domains` | `[]` | Exact domain allowlist (case-insensitive); an empty item or one containing `@`/`/`/whitespace fails startup; an empty list means no restriction |
| `sso.session_ttl_hours` | none | Must be greater than zero when set; falls back to `auth.session_ttl_hours` |

Validation and effective rules:

- `config check` runs `[sso]` validation even when `mode = "off"`; an invalid configuration exits immediately. It still does not print the `[sso]` section, only `features.sso`.
- Exact checks: `mode` must be `off`/`trusted_header`; all three header names must be non-empty; every `trusted_proxies` entry must parse as an IP or CIDR; `default_role` and every `group_role_map` value must parse as `owner`/`admin`/`user`; `group_role_map` keys must be non-empty; every `allowed_email_domains` item must be non-empty and contain no `@`, `/`, or whitespace; `session_ttl_hours = Some(0)` is rejected.
- An empty `trusted_proxies` does not block startup; when `mode = "trusted_header"`, `serve` logs the WARN `SSO trusted_header is enabled but trusted_proxies is empty; SSO will deny all peers`, and every request carrying SSO headers returns 401.
- There are no `LKB_SSO_*` environment overrides; the middleware is controlled only by `[sso].mode`, and `features.sso` is not consulted. See [06 Authentication and Security](06-auth-security.md) for behavior.

## 3. storage

| Field | Default | Notes |
| --- | --- | --- |
| `storage.backend` | `"sqlite"` | `sqlite` or `postgres` (`postgresql`/`pg` accepted) |
| `storage.sqlite_path` | `"data/lkb.sqlite"` | Parent directories are created |
| `storage.url_env` | `"LKB_DATABASE_URL"` | Environment variable holding the PostgreSQL URL |
| `storage.tenant_id` | `"t1"` (solo forces `t_local`) | Non-empty; one process serves one tenant |
| `storage.default_space` | `"s_default"` | Non-empty, single path segment |

## 4. spaces and groups

| Field | Default | Notes |
| --- | --- | --- |
| `spaces.allow_user_creation` | `true` (solo forces `false`) | Self-service space creation |
| `spaces.max_per_user` | `10` | Per-owner space count |
| `spaces.default_storage_bytes` | none | Default per-space quota; `None` means unlimited |
| `spaces.default_documents` | none | Default per-space document quota |
| `spaces.default_chunks` | none | Default per-space chunk quota |
| `spaces.default_members` | none | Default per-space member quota |
| `groups.allow_user_creation` | `true` (solo forces `false`) | Self-service group creation |

## 5. deployment and features

| Field | Default | Notes |
| --- | --- | --- |
| `deployment.mode` | inferred from profile | `solo` / `team` / `enterprise_internal` / `enterprise_external` |
| `deployment.commercial` | `false` | `true` requires `features.billing = true` |
| `features.*` | per-mode defaults | Explicit booleans override the default |

Feature defaults per mode:

| Feature | solo | team | enterprise_internal | enterprise_external |
| --- | --- | --- | --- | --- |
| `signup` | false | false | false | true |
| `billing` | false | false | false | true |
| `connectors` | false | true | true | true |
| `sso` | false | false | true | true |
| `multi_tenant` | false | false | true | true |
| `public_api` | false | false | true | true |
| `mcp` | false | false | true | true |
| `audit` | true | true | true | true |
| `rate_limit` | true | true | true | true |
| `public_share` | false | false | false | true |

**`billing`, `audit`, and `connectors` are enforced at runtime.** The remaining switches (`signup`, `sso`, `multi_tenant`, `public_api`, `mcp`, `public_share`) are surfaced by `GET /api/v1/features` but do not gate behavior; `rate_limit` always applies. Validation requires `deployment.mode` to match `profile`, forbids billing under solo, requires `signup + billing + multi_tenant + public_api` for `enterprise_external`, and forbids disabling audit on enterprise.

## 6. i18n

| Field | Default | Notes |
| --- | --- | --- |
| `i18n.default_locale` | `"zh-CN"` | Must be present in `supported_locales` |
| `i18n.supported_locales` | `["zh-CN","en"]` | Must include `en`; at most 28 entries |
| `i18n.tenant_locale` | none | Optional tenant default |

Runtime locale recognition currently covers `zh-CN` and `en` only; other tags fall back through the resolution chain (`Accept-Language` → user → tenant → default → `en`). API errors return a stable `code` plus a localized `message`, and every response carries `Content-Language`.

## 7. limits

| Field | Default | team/enterprise |
| --- | --- | --- |
| `qps_instance` | `1000` | `500` |
| `qps_ip` | `100` | `30` |
| `qps_user` | `500` | `100` |
| `qps_token` | `200` | `60` |
| `qps_tenant` | `1000` | `300` |
| `qps_space` | `500` | `120` |
| `max_concurrency_instance` | `256` | `128` |
| `max_concurrency_user` | `64` | unchanged |
| `max_concurrency_tenant` | `256` | unchanged |
| `max_concurrency_space` | `64` | unchanged |
| `max_queue` | `512` | `128` (never read at runtime) |
| `daily_requests_token` | `1000000` | `200000` |
| `retry_after_seconds` | `1` | unchanged |
| `max_query_chars` | `8192` | unchanged |
| `max_top_k` | `100` | unchanged |

Team/enterprise tightening only overwrites values still equal to the defaults. Limiter state is per process.

## 8. summary, query, seed, retrieval

| Field | Default | Notes |
| --- | --- | --- |
| `summary.enabled` | `true` | Master switch |
| `summary.auto` | `true` | Enqueue jobs on import/wiki/seed |
| `summary.model_alias` | `"default"` | Gateway alias |
| `summary.prompt_version` | `"summary-v1"` | Bumping it regenerates summaries |
| `summary.max_chars` | `1200` | Input truncation |
| `summary.batch` | `8` | Worker batch size |
| `summary.timeout_ms` | `15000` | Validated but unused at runtime |
| `summary.max_attempts` | `3` | Retries before `status=failed` |
| `summary.retry_backoff_ms` | `1000` | Retry delay |
| `query.rewrite_enabled` | `true` | Multi-turn query rewriting |
| `query.rewrite_alias` | `"default"` | Gateway alias for rewriting |
| `query.context_messages` | `6` | Recent messages used as context |
| `seed.enabled` | `true` | Seed subsystem switch |
| `seed.auto` | `true` | Auto-seed an empty solo database on `serve` |
| `seed.space` | `"product"` | Single path segment |
| `retrieval.mode` | `"auto"` | `bm25`, `keyword`, `bm25_judge`, `event`, `event_entity`, `hybrid`, `auto` |
| `retrieval.judge_alias` | `"default"` | Gateway alias for the judge |
| `retrieval.event_depth` | `1` | 1 or 2 |
| `retrieval.event_max_seeds` | `10` | 1–50 |
| `retrieval.event_max_events` | `50` | 1–500 |
| `retrieval.event_timeout_ms` | `800` | 1–10000 |
| `retrieval.event_text_weight` | `0.25` | Finite, non-negative |
| `retrieval.rrf_bm25_weight` | `1.0` | Finite, non-negative |
| `retrieval.rrf_dense_weight` | `1.0` | Finite, non-negative |
| `retrieval.rrf_event_weight` | `1.0` | Finite, non-negative |

## 9. payment and billing

| Field | Default | Notes |
| --- | --- | --- |
| `payment.provider` | `"none"` | `stripe`, `manual`, `mock` implemented; `wechat`/`alipay` return `payment_not_configured` |
| `payment.currency` | `"USD"` | Must be a 3-letter uppercase ISO 4217 code |
| `payment.mode` | `"test"` | `test` or `live`; validated, not read at runtime |
| `payment.secret_env` | none | Must be a variable name, not a secret value |
| `billing.enabled` | `false` | Feature gate for billing routes |
| `billing.engine` | `"local"` | `local` writes the ledger; any other value emits events only |
| `billing.currency` | `"USD"` | Wallet/plan currency |
| `billing.trial_days` | `14` | Trial length on subscribe |
| `billing.grace_days` | `7` | Defined but unused |
| `billing.stripe_secret_env` | `"STRIPE_SECRET_KEY"` | Stripe API key variable |
| `billing.stripe_webhook_secret_env` | `"STRIPE_WEBHOOK_SECRET"` | Webhook signature secret |
| `billing.webhook_tolerance_seconds` | `300` | Timestamp tolerance |

## 10. models

`models.llm`, `models.embedding`, `models.judge`, and `models.extractor` share one structure:

| Field | Default | Notes |
| --- | --- | --- |
| `provider` | `"none"` | Anything else enables it; every name uses the OpenAI-compatible protocol |
| `base_url` | none | Required when enabled; only `provider = "openai"` has a default (`https://api.openai.com/v1`) |
| `model` | none | Required when enabled |
| `api_key_env` | none | Environment variable name; missing value makes the capability unavailable |
| `timeout_ms` | `15000` | Must be greater than zero |
| `max_retries` | `2` | Retries 5xx and transport errors with 200 ms × attempt backoff |

`[models.llm]` is materialized into the gateway as provider `legacy`, alias `default`, and route `route_legacy_default`. `provider = "jev"` has no special implementation; a Jev endpoint must be exposed through an OpenAI-compatible gateway.

## 11. Environment Variables

| Variable | Overrides |
| --- | --- |
| `LKB_PROFILE` | `profile` |
| `LKB_BIND` | `server.bind` |
| `LKB_AUTH_MODE`, `LKB_COOKIE_NAME`, `LKB_COOKIE_SECURE`, `LKB_SESSION_TTL_HOURS`, `LKB_CSRF` | `auth.*` |
| `LKB_STORAGE_BACKEND`, `LKB_SQLITE_PATH`, `LKB_URL_ENV`, `LKB_STORAGE_TENANT_ID`, `LKB_DEFAULT_SPACE` | `storage.*` |
| `LKB_SEED_ENABLED`, `LKB_SEED_AUTO`, `LKB_SEED_SPACE` | `seed.*` |
| `LKB_SPACES_ALLOW_USER_CREATION`, `LKB_SPACES_MAX_PER_USER` | `spaces.*` |
| `LKB_GROUPS_ALLOW_USER_CREATION` | `groups.*` |
| `LKB_DEPLOYMENT_MODE`, `LKB_COMMERCIAL` | `deployment.*` |
| `LKB_I18N_DEFAULT_LOCALE` | `i18n.default_locale` |
| `LKB_PAYMENT_PROVIDER` | `payment.provider` |
| `LKB_RETRIEVAL_MODE` | `retrieval.mode` |
| `LKB_DATABASE_URL` (name from `storage.url_env`) | PostgreSQL URL |
| `LKB_LOG_FORMAT` (`json`), `LKB_LOG_LEVEL`, `LKB_SERVICE` | Logging |
| `LKB_TOKEN`, `LKB_API_TOKEN`, `LKB_CONFIG_FILE` | CLI credentials |
| `LKB_USER_PASSWORD` | `lkb user add` password |

No environment overrides exist for `models.*`, `limits.*`, `summary.*`, `query.*`, `billing.*`, or the remaining `retrieval.*` fields. solo normalization runs after environment application, so `LKB_STORAGE_TENANT_ID` is still overwritten with `t_local` in solo.

## 12. Validation Rules

Startup is rejected when any of these fail:

- `solo` with `postgres`, or `team`/`enterprise` with `sqlite`.
- `deployment.mode` incompatible with `profile`.
- `commercial = true` without `features.billing`; billing enabled under solo; `enterprise_external` missing `signup`/`billing`/`multi_tenant`/`public_api`; enterprise with audit disabled.
- i18n: empty `default_locale`, empty `supported_locales`, more than 28 locales, default missing from supported, or no `en` fallback.
- `summary.model_alias`/`prompt_version` empty; `max_chars`/`batch`/`timeout_ms`/`max_attempts` zero; `query.rewrite_alias` empty or `context_messages` zero.
- `payment.currency` not a 3-letter uppercase code; `payment.mode` not `test`/`live`; `payment.secret_env` not a variable name.
- `server.bind` not `host:port`; `auth.mode = "none"` on a non-loopback bind; `session_ttl_hours = 0`.
- Empty `tenant_id`/`default_space`; multi-segment `default_space`; invalid `seed.space`.
- PostgreSQL backend without a database URL in the configured environment variable.
- Invalid `retrieval.mode`, out-of-range event parameters, negative or non-finite weights.
- Any enabled model section without an http(s) `base_url`, without a model, or with `timeout_ms = 0`.

## 13. Secrets and Redaction

- Model keys come only from `api_key_env`; payment secrets from `billing.stripe_*_env`. They are never written to configuration, databases, or logs.
- `config check` prints environment variable names and a state (`set`/`missing`/`not configured`) but never values; database URLs and base URLs are masked.
- `lkb user add` reads `LKB_USER_PASSWORD`; without it the password prompt echoes input.
- The CLI writes only the token into its local config file; secret values never enter `lkb.toml`.

## 14. Examples

solo (everything may be omitted):

```toml
profile = "solo"

[server]
bind = "127.0.0.1:7331"

[auth]
mode = "none"
csrf = true

[storage]
backend = "sqlite"
sqlite_path = "data/lkb.sqlite"

[seed]
enabled = true
auto = true
space = "product"

[retrieval]
mode = "auto"
judge_alias = "default"
```

team / enterprise_external:

```toml
profile = "team"

[server]
bind = "0.0.0.0:7331"

[auth]
mode = "session"
cookie_secure = true

[storage]
backend = "postgres"
url_env = "LKB_DATABASE_URL"
tenant_id = "t1"
default_space = "s_default"

[deployment]
mode = "team"          # enterprise_internal | enterprise_external
commercial = false

[features]
billing = false        # enterprise_external forces this on
audit = true

[i18n]
default_locale = "zh-CN"
supported_locales = ["zh-CN", "en"]

[retrieval]
mode = "auto"
event_depth = 1
```

## 15. Related Documents

- [03 Quickstart](03-quickstart.md)
- [06 Authentication and Security](06-auth-security.md)
- [08 Deployment](08-deployment.md)
