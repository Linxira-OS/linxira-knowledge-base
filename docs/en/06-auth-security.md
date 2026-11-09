---
title: Authentication and Security
slug: 06-auth-security
lang: en
order: 6
summary: Cookie sessions, CSRF, Bearer tokens, loopback restriction, TLS reverse proxying, and fail-closed behavior.
---

# Authentication and Security

There are two credential types for two callers: browsers use cookie sessions, agents and the CLI use Bearer tokens. JSON endpoints add a third pattern: a session cookie obtained through `/api/v1/auth/login` and sent with the `X-CSRF-Token` header. All paths resolve to the same `Principal`, which is then handed to `Policy` and the access layer.

## 1. Credentials at a Glance

| Credential | Used by | Stored as | Validation |
| --- | --- | --- | --- |
| Session cookie | Browser HTML pages and the SPA | Random session id; server stores only its SHA-256 | Hash lookup, expiry, tenant, user status |
| Bearer token | CLI, agents, HTTP API | Token SHA-256, scope, kind, expiry, revocation state | Hash lookup, scope, tenant, user status |
| JSON auth (`/api/v1/auth/*`) | SPA and scripts | Same session table as cookies | CSRF/Origin + username/password (Argon2) |

Bearer requests skip CSRF checks; cookie-session writes require the CSRF double-submit. Bearer is also accepted on page routes because the authentication entry point checks `Authorization` first.

## 2. The Boundary of solo No-Auth Mode

solo defaults to `auth.mode = "none"`. Every request resolves to the local `Principal` (`tenant_id = "t_local"`, space `s_default`, `allow_all = true`, role owner, scope admin).

No-auth mode has a hard boundary: if `auth.mode = "none"` and `server.bind` is not loopback, configuration validation fails and the process refuses to start. Loopback includes `127.0.0.1`, `::1`, and `localhost`. There is no legal configuration that exposes an unauthenticated service publicly.

## 3. JSON Auth Endpoints

| Endpoint | Behavior |
| --- | --- |
| `GET /api/v1/auth/csrf` | Origin check; reuses the `lkb_csrf` cookie if present, else issues one (`Path=/; SameSite=Lax; Max-Age=86400`, no HttpOnly) and returns `{"csrf_token":"..."}` |
| `POST /api/v1/auth/login` | Verifies Origin + CSRF (`csrf_token` in JSON or `X-CSRF-Token` header), then username/Argon2; success creates a 32-hex session, sets the session cookie, returns principal + features. `auth.mode = "none"` returns 403 "authentication is disabled" |
| `GET /api/v1/auth/me` | Origin check; returns `{principal, features}` for the current credential; 401 when missing/expired |
| `POST /api/v1/auth/logout` | CSRF required (`X-CSRF-Token`); deletes the session server-side and clears the cookie; returns `{"logged_out":true}` |

Login failures are rate-limited per "IP + username" (first `X-Forwarded-For`, else `X-Real-IP`, else `local`): 5 failures in 300 seconds → 429 `rate_limited`. Bad credentials return 401 `unauthorized` and write an `auth.login` audit record with result `denied`.

## 4. Cookie Sessions

- `cookie_name` defaults to `lkb_session`; cookies are always `Path=/` and `SameSite=Lax`.
- `HttpOnly` is set on the session cookie; the CSRF cookie is intentionally not HttpOnly.
- `Secure` is added when `auth.cookie_secure` is true or the request `Host` is not loopback.
- `session_ttl_hours` defaults to 168 hours and must be greater than zero; the server stores only the SHA-256 of the session id.
- Over plain HTTP to a non-loopback host, browsers will not send a Secure cookie: login appears to succeed but later requests return 401.
- Logout deletes the server session and sends an expired cookie.

## 5. CSRF and Origin

Two equivalent double-submit schemes:

- **HTML forms**: `lkb_csrf` cookie plus form field `csrf_token`, compared in constant time.
- **JSON API**: `lkb_csrf` cookie plus `X-CSRF-Token` header (or `csrf_token` in the login body).

Origin rules apply to every state-changing path: if an `Origin` header is present it must equal `Host` case-insensitively; `Origin: null` is rejected with 403. `auth.csrf = false` skips only the token comparison, never the Origin check.

CSRF is enforced only for cookie-session requests; Bearer requests are exempt. Most `/api/v1/admin/*` routes carry only admin-scope auth (no CSRF header), except `POST /api/v1/admin/quota-overrides`, which does enforce it.

## 6. Bearer Tokens

- Format `lkb_` plus 64 hexadecimal characters, shown once at creation; the database stores only its SHA-256.
- Scopes: `read` < `write` < `admin` (read is implied by all). The runtime scopes are exactly these three; `kb.read/kb.write/kb.admin` appear only as OpenAPI/MCP labels.
- Kinds: `user`, `agent`, `service`; `token create` accepts `--kind`, `--scope`, `--label`, and `--expires-days`.
- Validation order: hash lookup → expiry/revocation → tenant → user exists/not disabled → scope.
- Transfer only via `Authorization: Bearer`; never put tokens in URLs or logs (the log stores a 16-hex SHA-256 prefix instead).

Agent config: `lkb auth login --token` writes the token into the local config file (`LKB_CONFIG_FILE`, `%APPDATA%\lkb\config.toml` on Windows, or `~/.config/lkb/config.toml` on Unix; Unix mode 0600, Windows best-effort `icacls`). `auth status` reads `LKB_TOKEN` → `LKB_API_TOKEN` → the config file; `auth logout` clears the file token. `--device` returns `not_implemented`.

## 7. Roles and the Access Model

| Layer | Roles | Scope |
| --- | --- | --- |
| Tenant | `owner` > `admin` > `user` | Whole tenant: users, groups, spaces, audit |
| Group | `group_admin` > `group_member` | One group: spaces it owns and its members |
| Space | `space_owner` > `space_admin` > `editor` > `viewer` | Pages, members, quota, sharing |

Effective space role precedence: **explicit `space_members` row > group-derived role > `owner_type`**. Multiple groups take the highest. Tenant admins act as `space_owner` through a break-glass path that is always audited (`page.read.break_glass` / `page.write.break_glass` / `content.read.break_glass`). Removing or demoting the last `space_owner` returns 409 `last_space_owner_protected`; the last `group_admin` is protected the same way.

## 8. ACL Enforcement

Before recall, `Policy` builds one visibility predicate: `tenant_id`, `space_id` in the principal's allowed set, the validity window, the language filter, and ACL principal matching (`|*|`, `|u:{id}|`, `|g:{group}|`). After SQL filtering, an in-process `can_read` check runs; any mismatch or ACL parse failure discards the candidate and writes `acl_second_check_dropped` (security category, sorted first).

Space membership and document ACL intersect: both must pass. Membership is recomputed from explicit members and group memberships; tenant admins see all non-deleted spaces. Authentication and ACL failures are never degradations — they return 401/403/409 and may write audit records (`acl.denied`, `acl.filtered`).

## 9. Limits, Quotas, and Backpressure

| Mechanism | Trigger | Response |
| --- | --- | --- |
| IP QPS (pre-auth middleware) | `qps_ip` (default 100/s; team 30) | 429 `rate_limited` |
| Per-key QPS | instance/user/token/tenant/space buckets | 429 `rate_limited` + `Retry-After` (default 1 s) |
| Daily requests | per token, reset by UTC day | 429 `rate_limited`, retry hint 86400 s |
| Concurrency | per instance/user/tenant/space | 503 `queue_full` + `Retry-After` |
| Login limiter | 5 failures / 300 s per IP+username | 429 `rate_limited` |
| Input validation | query > 8192 chars, top_k > 100 | 400 `invalid_query` / `invalid_top_k` |
| Token quota | hard policy exceeded | 429 `quota_exceeded` + `Retry-After: 60` |
| Storage quota | bytes / counts exceeded | 413 or 429 `space_quota_exceeded` / `tenant_quota_exceeded` |
| Balance gate | wallet/entitlement present and insufficient | 402 `payment_required` |

Limiter state is **per process**: counters reset on restart and are not shared across instances, so multi-instance deployments over-admit unless they add an external counter. `[limits].max_queue` is configured but never read; the 503 path is concurrency exhaustion. Denials are audited (`rate_limit`, result `denied`) and counted in `/metrics`.

## 10. Audit and Logs

`audit_events` stores `event_id`, tenant, space, actor, action, target, result, status code, request id, IP (raw), user agent (300 chars), detail JSON, and time. Actions follow dot-separated naming such as `space.created`, `member.role_changed`, `group.member_added`, `ownership.transferred`, `quota.updated`, `quota.override`, `billing.topup`, `models.route.update`, `auth.login`, `acl.filtered`. Conversation endpoints write no audit events.

`GET /api/v1/admin/audit` requires the `audit` feature and admin scope; filters are `actor`, `action`, `space`, `from`, `to`, and `limit` (default 100, max 500), ordered newest first. Audit stores the raw client IP; runtime access logs store `ip_hash = sha256(ip)` instead and never record query text (only `query_hash` and `query_length`), tokens, or credentials.

## 11. TLS and Reverse Proxies

The service does not terminate TLS. Production deployments should terminate TLS at a reverse proxy:

- The app listens on `127.0.0.1:7331`; the proxy listens on public 443.
- The proxy must preserve the original `Host`, or Origin validation and Secure-cookie detection fail.
- Set `auth.cookie_secure = true`; non-loopback Hosts get Secure automatically.
- Do not use `X-Forwarded-For` for identity; it only affects the login rate-limit key and log IP hashing.
- Proxy access control does not replace application ACLs.

## 12. Fail-Closed Behavior and Status Codes

| Status | Scenario |
| --- | --- |
| 400 | Invalid parameters, If-Match missing, invalid slug/mode/markdown/title, `payment_invalid` |
| 401 | Unauthenticated, expired session, expired/revoked token, tenant mismatch, disabled user |
| 402 | Insufficient balance or suspended/cancelled entitlement (`payment_required`) |
| 403 | CSRF/Origin failure, insufficient scope, insufficient space role, `feature_disabled`, `billing_not_enabled`, `model_forbidden` |
| 404 | Page/revision/conversation/space/group/member/plan not found |
| 409 | `conflict`, `revision_conflict`, `space_not_active`, `last_space_owner_protected`, `last_group_admin_protected` |
| 410 | `client_disconnected` (gateway stream cancel) |
| 413 | Storage byte quota exceeded |
| 429 | `rate_limited`, `quota_exceeded`, count quotas |
| 500 | `internal`, `billing_unavailable`, `quota_check_failed` |
| 503 | `queue_full`, store `timeout`, `storage_unavailable` (`/health`), `model_unavailable`, `payment_not_configured`, `payment_provider_unavailable` |

When identity or ACL cannot be determined, access is denied. `degraded[]` describes missing capability; it is never an authorization escape hatch.

## 13. Trusted-Header SSO

This is a "the reverse proxy already authenticated the user" trust model, not OIDC/SAML/SCIM: there is no login redirect, IdP metadata, SCIM directory provisioning, or multi-tenant Host routing. The only enablement path today is `[sso].mode = "trusted_header"`.

### Trust Boundary

- Accept headers only when the TCP peer IP matches `trusted_proxies` (IP or CIDR, IPv4/IPv6); `serve` obtains the peer through ConnectInfo.
- An empty `trusted_proxies` denies every request that carries SSO headers (401); being local does not grant trust.
- A client-supplied `x-lkb-sso-authenticated` header is always stripped first; spoofing it grants nothing.
- When all three configured headers are absent or empty, the middleware does not trigger and normal cookie/Bearer auth applies. Any non-empty SSO header starts SSO validation and requires a trusted peer.
- Never expose the application directly to clients that can forge these headers; see [08 Deployment](08-deployment.md).

### Headers and Identity Binding

| Config | Default header | Meaning |
| --- | --- | --- |
| `sso.user_header` | `X-Auth-Request-User` | Required; missing/empty → 401 |
| `sso.email_header` | `X-Auth-Request-Email` | Optional; identity binding and domain check |
| `sso.groups_header` | `X-Auth-Request-Groups` | Optional; comma-separated |

Header names are case-insensitive. Processing order:

1. Empty `user_header` → 401 "sso user header is missing".
2. When `allowed_email_domains` is non-empty, email must exist and its domain must match exactly (case-insensitive, no subdomain wildcard) → otherwise 403.
3. Resolve the user by email → `(source=sso, external_id=username)` → username; if an existing user's email differs from the incoming value → 403.
4. No user found: `auto_provision = false` → 401; otherwise create a `source=sso` user with the highest matching `group_role_map` role, else `default_role` (startup validation restricts it to `owner`/`admin`/`user`).
5. `disabled` user → 401; an existing user gets a mapped role update when a mapping matches, plus email/external_id/last_sso_at refresh.
6. On success write `sso.login`, rotate the old session, and issue an HttpOnly session cookie (TTL from `sso.session_ttl_hours`, else `auth.session_ttl_hours`). Later writes still use normal session CSRF.

Identity effect: the tenant role comes from the user record; `owner`/`admin` get Admin scope, everyone else gets Write scope; space scope is still resolved by `access.rs` membership.

### Failures and Audit

| Status | reason | Scenario |
| --- | --- | --- |
| 401 | `untrusted_peer` | Peer is not in `trusted_proxies` |
| 401 | `missing_user` | SSO headers present but the user header is missing/empty |
| 401 | `auto_provision_disabled` | User does not exist and provisioning is off |
| 401 | `user_disabled` | User is disabled |
| 403 | `email_domain_not_allowed` | Email missing or domain not allowlisted |
| 403 | `email_conflict` | An existing user has a different email |

`GET /api/v1/auth/sso/status` publicly returns `{enabled, mode}` and leaks no users or config. Audit actions: `sso.login` (success), `sso.provision` (auto-created user), `sso.denied` (rejection, with `reason` in detail). `features.sso` remains a config/display switch and does not control this middleware; only `[sso].mode` does.

## 14. Related Documents

- [04 Built-in Wiki](04-wiki.md)
- [07 Configuration](07-configuration.md)
- [11 FAQ and Troubleshooting](11-faq.md)
