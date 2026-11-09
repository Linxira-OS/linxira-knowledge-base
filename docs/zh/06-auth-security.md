---
title: 认证与安全
slug: 06-auth-security
lang: zh
order: 6
summary: cookie 会话、CSRF、Bearer 令牌、回环限制、TLS 反向代理与 fail-closed 的完整行为说明。
---

# 认证与安全

系统服务两类调用者：浏览器使用 cookie 会话，Agent/CLI 使用 Bearer 令牌。JSON 端点在此之外提供第三种形态：通过 `/api/v1/auth/login` 拿到会话 cookie，之后写请求带 `X-CSRF-Token` 头。所有路径最终解析成同一个 `Principal`，交给同一套 `Policy` 与访问层。

## 1. 凭据一览

| 凭据 | 谁使用 | 存放 | 校验 |
| --- | --- | --- | --- |
| 会话 cookie | 浏览器 HTML 页面与 SPA | cookie 中是随机 session id；服务端只存 SHA-256 | 哈希查找、过期、租户、用户状态 |
| Bearer 令牌 | CLI、Agent、HTTP API | token 的 SHA-256、scope、kind、过期与吊销状态 | 哈希查找、scope、租户、用户状态 |
| JSON 认证（`/api/v1/auth/*`） | SPA 与脚本 | 复用同一张 session 表 | CSRF/Origin + Argon2 密码校验 |

Bearer 请求跳过 CSRF 校验；cookie 会话的写请求需要 CSRF 双重提交。Bearer 在页面路由上也被接受，因为认证入口先检查 `Authorization`。

## 2. solo 免认证的边界

solo 默认 `auth.mode = "none"`，所有请求解析为本地 `Principal`（`tenant_id = "t_local"`、空间 `s_default`、`allow_all = true`、租户角色 owner、scope admin）。

免认证有硬性边界：`auth.mode = "none"` 且 `server.bind` 不是回环地址时，配置校验失败，进程拒绝启动。回环包括 `127.0.0.1`、`::1` 与 `localhost`。不存在合法的“把免认证服务暴露到公网”配置。

## 3. JSON 认证端点

| 端点 | 行为 |
| --- | --- |
| `GET /api/v1/auth/csrf` | Origin 校验；若已有 `lkb_csrf` cookie 则复用，否则下发一个（`Path=/; SameSite=Lax; Max-Age=86400`，非 HttpOnly），返回 `{"csrf_token":"..."}` |
| `POST /api/v1/auth/login` | 校验 Origin + CSRF（JSON 里的 `csrf_token` 或 `X-CSRF-Token` 头），再校验用户名/Argon2；成功创建 32 位十六进制 session，下发会话 cookie，返回 principal 与 features。`auth.mode = "none"` 时返回 403 "authentication is disabled" |
| `GET /api/v1/auth/me` | Origin 校验；返回 `{principal, features}`；凭证缺失/过期返回 401 |
| `POST /api/v1/auth/logout` | 需要 CSRF（`X-CSRF-Token`）；删除服务端会话并清理 cookie，返回 `{"logged_out":true}` |

登录失败按“IP + 用户名”限流（取 `X-Forwarded-For` 首段，否则 `X-Real-IP`，否则 `local`）：300 秒内 5 次失败返回 429 `rate_limited`。密码错误返回 401 `unauthorized`，并写 `auth.login` 审计（结果 `denied`）。

## 4. 浏览器会话

- `cookie_name` 默认 `lkb_session`；cookie 固定 `Path=/`、`SameSite=Lax`。
- 会话 cookie 带 `HttpOnly`；CSRF cookie 有意不带 HttpOnly，供前端 JS 读取。
- `auth.cookie_secure` 为真，或请求 `Host` 不是回环地址时加 `Secure`。
- `session_ttl_hours` 默认 168 小时，必须大于 0；服务端只保存 session id 的 SHA-256。
- 通过 HTTP 访问非回环主机时浏览器不会回传 Secure cookie：表现为登录成功但后续请求 401。
- 登出会删除服务端会话并下发过期 cookie。

## 5. CSRF 与 Origin

两套等价的双重提交校验：

- **HTML 表单**：`lkb_csrf` cookie + 表单字段 `csrf_token`，常量时间比较。
- **JSON API**：`lkb_csrf` cookie + `X-CSRF-Token` 头（登录也可放在 JSON 的 `csrf_token`）。

Origin 规则适用于所有有副作用的路径：请求带 `Origin` 时，其 authority 必须与 `Host` 大小写不敏感地相等；`Origin: null` 直接 403。`auth.csrf = false` 只跳过 token 比对，Origin 校验仍然执行。

CSRF 只对 cookie 会话请求生效，Bearer 请求豁免。`/api/v1/admin/*` 大多只做 admin scope 校验（不带 CSRF 头），例外是 `POST /api/v1/admin/quota-overrides`，它会校验 CSRF。

## 6. Bearer 令牌

- 格式为 `lkb_` 加 64 位十六进制；只在创建时显示一次，数据库只存 SHA-256。
- scope：`read` < `write` < `admin`（read 隐含在全部中）。运行时 scope 就是这三个；`kb.read/kb.write/kb.admin` 只出现在 OpenAPI/MCP 的标签里。
- kind：`user`、`agent`、`service`；`token create` 支持 `--kind`、`--scope`、`--label`、`--expires-days`。
- 校验顺序：哈希查找 → 过期/吊销 → 租户 → 用户存在且未禁用 → scope。
- 只能放在 `Authorization: Bearer` 中传输，禁止放进 URL 或日志（日志里只有 16 位十六进制 SHA-256 前缀）。

Agent 本地配置：`lkb auth login --token` 把 token 写入本机配置文件（`LKB_CONFIG_FILE`；Windows `%APPDATA%\lkb\config.toml`，Unix `~/.config/lkb/config.toml`，Unix 0600，Windows 尽力 `icacls` 收紧）。`auth status` 依次读 `LKB_TOKEN` → `LKB_API_TOKEN` → 配置文件；`auth logout` 清除文件里的 token。`--device` 返回 `not_implemented`。

## 7. 角色与访问模型

| 层 | 角色 | 作用域 |
| --- | --- | --- |
| 租户 | `owner` > `admin` > `user` | 整个 tenant：用户、组、空间、审计 |
| 组 | `group_admin` > `group_member` | 一个 group：组拥有的空间与组成员 |
| 空间 | `space_owner` > `space_admin` > `editor` > `viewer` | 页面、成员、配额、共享 |

有效空间角色的优先级：**显式 `space_members` 行 > 组派生角色 > `owner_type`**；多组取最高。租户 admin 通过 break-glass 以 `space_owner` 身份访问，且必然写审计（`page.read.break_glass` / `page.write.break_glass` / `content.read.break_glass`）。移除或降级最后一个 `space_owner` 返回 409 `last_space_owner_protected`；最后一个 `group_admin` 同样受保护。

## 8. ACL 如何执行

召回前由 `Policy` 生成唯一可见性条件：`tenant_id`、`space_id` 在主体允许集合内、时序窗口、语言过滤，以及 ACL 主体匹配（`|*|`、`|u:{id}|`、`|g:{group}|`）。SQL 过滤之后还有一次进程内 `can_read` 校验；任何字段不符或 ACL 解析失败都丢弃该条并写 `acl_second_check_dropped`（安全类，排序最靠前）。

空间成员资格与文档 ACL 取交集：两者都通过才返回。成员关系由显式成员与组员关系实时求值；租户 admin 可见所有未删除空间。认证失败与 ACL 拒绝本身不进入 `degraded[]`，而是 401/403/409，并可写审计（`acl.denied`、`acl.filtered`）。

## 9. 限流、配额与背压

| 机制 | 触发 | 响应 |
| --- | --- | --- |
| IP QPS（认证前中间件） | `qps_ip`（默认 100/s；team 30） | 429 `rate_limited` |
| 各维度 QPS | instance/user/token/tenant/space 令牌桶 | 429 `rate_limited` + `Retry-After`（默认 1 秒） |
| 每日请求数 | 按 token，UTC 日切 | 429 `rate_limited`，重试提示 86400 秒 |
| 并发上限 | instance/user/tenant/space | 503 `queue_full` + `Retry-After` |
| 登录限流 | 每 IP+用户名 300 秒 5 次失败 | 429 `rate_limited` |
| 输入校验 | query > 8192 字符、top_k > 100 | 400 `invalid_query` / `invalid_top_k` |
| token 配额 | hard 策略超限 | 429 `quota_exceeded` + `Retry-After: 60` |
| 存储配额 | 字节/计数超限 | 413 或 429 `space_quota_exceeded` / `tenant_quota_exceeded` |
| 余额闸门 | 存在钱包/entitlement 且余额不足 | 402 `payment_required` |

限流状态是**进程内**的：重启即清零，多实例不共享，因此多实例部署会在边界上超额放行，需要外部分布式计数。`[limits].max_queue` 已定义但运行时未读取；503 来自并发耗尽。拒绝会写审计（`rate_limit`，结果 `denied`）并计入 `/metrics`。

## 10. 审计与日志

`audit_events` 存 `event_id`、tenant、space、actor、action、target、result、状态码、request id、原始 IP、user agent（300 字符）、detail JSON 与时间。动作采用点分命名，如 `space.created`、`member.role_changed`、`group.member_added`、`ownership.transferred`、`quota.updated`、`quota.override`、`billing.topup`、`models.route.update`、`auth.login`、`acl.filtered`。会话相关端点不写审计。

`GET /api/v1/admin/audit` 需要 `audit` 功能开关与 admin scope；过滤参数为 `actor`、`action`、`space`、`from`、`to`、`limit`（默认 100，上限 500），按时间倒序。审计保存原始客户端 IP；运行日志只存 `ip_hash = sha256(ip)`，查询文本只记 `query_hash` 与 `query_length`，不落 token 与凭据。

## 11. TLS 与反向代理

服务本身不终止 TLS。生产部署应由反向代理终止 TLS：

- 应用只监听 `127.0.0.1:7331`，代理监听公网 443。
- 代理必须保留原始 `Host`，否则 Origin 校验与 Secure cookie 判断会失败。
- 设置 `auth.cookie_secure = true`；非回环 Host 会自动加 Secure。
- 不要依赖 `X-Forwarded-For` 做身份判断；它只影响登录限流键与日志 IP 哈希。
- 代理层的访问控制不能替代应用内 ACL。

## 12. fail-closed 与错误码

| 状态 | 场景 |
| --- | --- |
| 400 | 参数非法、缺少 If-Match、slug/mode/markdown/title 非法、`payment_invalid` |
| 401 | 未认证、会话过期、令牌过期/吊销、租户不符、用户禁用 |
| 402 | 余额不足或 entitlement 被暂停/取消（`payment_required`） |
| 403 | CSRF/Origin 失败、scope 不足、空间角色不足、`feature_disabled`、`billing_not_enabled`、`model_forbidden` |
| 404 | 页面/修订/会话/空间/组/成员/套餐不存在 |
| 409 | `conflict`、`revision_conflict`、`space_not_active`、`last_space_owner_protected`、`last_group_admin_protected` |
| 410 | `client_disconnected`（网关流式取消） |
| 413 | 存储字节配额超限 |
| 429 | `rate_limited`、`quota_exceeded`、计数类配额 |
| 500 | `internal`、`billing_unavailable`、`quota_check_failed` |
| 503 | `queue_full`、存储 `timeout`、`storage_unavailable`（`/health`）、`model_unavailable`、`payment_not_configured`、`payment_provider_unavailable` |

身份或 ACL 无法确定时一律拒绝；`degraded[]` 只描述能力缺失，不是授权失败的出口。

## 13. 可信 Header SSO

这是“反向代理已完成认证”的信任模式，不是 OIDC/SAML/SCIM：没有登录重定向、IdP 元数据、SCIM 目录供给，也没有多租户 Host 路由。当前只有 `[sso].mode = "trusted_header"` 一种启用方式。

### 信任边界

- 只接受 TCP peer IP 命中 `trusted_proxies`（IP 或 CIDR，支持 IPv4/IPv6）的请求；`serve` 通过 ConnectInfo 取得 peer 地址。
- `trusted_proxies` 为空 = 所有携带 SSO 头的请求全部拒绝（401）；不会因为请求来自本机而放行。
- 客户端直传的内部身份头 `x-lkb-sso-authenticated` 一律先被剥离；伪造它不会获得身份。
- 配置的 user/email/groups 三个头都为空或缺失时，中间件不触发，回退普通 cookie/Bearer 认证；只要出现任意一个非空 SSO 头，就进入 SSO 校验并要求可信 peer。
- 应用不能直接暴露给可伪造这些头的客户端；见 [08 部署与运维](08-deployment.md)。

### 头与身份绑定

| 配置 | 默认 header | 说明 |
| --- | --- | --- |
| `sso.user_header` | `X-Auth-Request-User` | 必需；缺失或为空 → 401 |
| `sso.email_header` | `X-Auth-Request-Email` | 可选；用于身份绑定与域校验 |
| `sso.groups_header` | `X-Auth-Request-Groups` | 可选；逗号分隔 |

Header 名大小写不敏感。处理顺序：

1. `user_header` 为空 → 401 `sso user header is missing`。
2. `allowed_email_domains` 非空时，email 必须存在且域名精确匹配（大小写不敏感，不做子域通配）→ 否则 403。
3. 依次按 email → `(source=sso, external_id=username)` → username 查找用户；已有用户 email 与传入值不一致 → 403。
4. 找不到用户：`auto_provision = false` → 401；为 true 则创建 `source=sso` 用户，角色取 `group_role_map` 中最高匹配，否则用 `default_role`（启动校验已限定为 `owner`/`admin`/`user`）。
5. 用户 `disabled` → 401；已有用户在有组映射时更新角色，并更新 email/external_id/last_sso_at。
6. 成功写 `sso.login` 审计，轮换旧会话并签发 HttpOnly 会话 cookie（TTL 取 `sso.session_ttl_hours`，缺省用 `auth.session_ttl_hours`）。后续写请求仍按普通会话走 CSRF。

身份效果：租户角色来自用户记录；`owner`/`admin` 获得 Admin scope，其余为 Write scope；空间范围仍由 `access.rs` 的成员关系求值。

### 失败与审计

| 状态 | reason | 场景 |
| --- | --- | --- |
| 401 | `untrusted_peer` | peer 不在 `trusted_proxies` |
| 401 | `missing_user` | 有 SSO 头但 user 头缺失/为空 |
| 401 | `auto_provision_disabled` | 用户不存在且未开自动创建 |
| 401 | `user_disabled` | 用户已禁用 |
| 403 | `email_domain_not_allowed` | email 缺失或域名不在白名单 |
| 403 | `email_conflict` | 已有用户 email 与传入值不同 |

`GET /api/v1/auth/sso/status` 公开返回 `{enabled, mode}`，不泄露用户或配置。审计动作：`sso.login`（成功）、`sso.provision`（自动建用户）、`sso.denied`（拒绝，detail 含 reason）。`features.sso` 仍只是配置/展示开关，不控制该中间件；启用与否只看 `[sso].mode`。

## 14. 相关文档

- [04 内置 Wiki](04-wiki.md)
- [07 配置参考](07-configuration.md)
- [11 常见问题与排错](11-faq.md)
