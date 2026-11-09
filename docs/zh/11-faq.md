---
title: 常见问题与排错
slug: 11-faq
lang: zh
order: 11
summary: 构建、配置、导入、检索、认证、Wiki 与数据恢复的常见错误、降级码和当前限制。
---

# 常见问题与排错

先定位阶段：构建、配置校验、seed、导入、检索、摘要/会话、网关/配额/计费、认证、Wiki，还是恢复。

## 1. 构建与启动

### 链接失败，`link.exe` 指向奇怪的位置

Windows 上如果 `where link.exe` 找到的是 Git 自带 coreutils 的 `link`，说明没有可用的 MSVC 链接器。安装 MSVC Build Tools + Windows SDK，或使用 `cargo +stable-x86_64-pc-windows-gnu build --release` 并确保 MinGW gcc 在 `PATH`。

### 提示缺少 C 编译器

SQLite 是 bundled 构建，仍然需要 C 工具链；只装 Rust 不够。

### 启动时报绑定失败

`server.bind` 必须是 `host:port`，且端口未被占用。默认 `127.0.0.1:7331`。`auth.mode = "none"` 配非回环地址会被拒绝。

### CLI 退出码是 1 或 2

只有三种退出码：0 成功、1 命令错误（`anyhow`/`bail!`）、2 clap 用法错误或 `seed`/`import` 部分失败。旧设计文档里的 3–7 退出码没有实现。

## 2. 配置校验

| 报错含义 | 处理 |
| --- | --- |
| `profile solo requires storage.backend = "sqlite"` | solo 不能配 PostgreSQL |
| `profile team/enterprise requires storage.backend = "postgres"` | team/enterprise 不能配 SQLite |
| `deployment.mode ... is incompatible with profile ...` | 对齐 mode 与 profile |
| `deployment.commercial = true requires features.billing = true` / `solo deployment cannot enable billing` | 调整 billing 开关 |
| `enterprise_external requires signup, billing, multi_tenant and public_api` | 四项全部打开 |
| `enterprise deployments cannot disable audit` | 保持 `features.audit = true` |
| `refusing to start: auth.mode = "none" while ... is not loopback` | 免认证只允许回环 |
| `storage backend is postgres but environment variable ... is not set` | 设置 `LKB_DATABASE_URL` 或配置的变量 |
| `i18n.supported_locales must include en as fallback` | 加入 `en` |
| `payment.currency must be an ISO 4217 three-letter uppercase code` | 使用如 `USD` |

`config check` 会跑完整校验并打印脱敏后的生效配置；它不连接数据库。

## 3. Seed 与首次运行

### `seed` 报 `failed=1`，提示 `seed_import_failed: space ... does not exist`

只有 solo 会自动创建 `space=product`；team / enterprise 需要先创建空间，或用指向已存在空间的 `--space`。

### `seed` 显示 `imported: 0` 并有 warning

看报告字段：`unchanged` 表示内置内容与已导入内容一致；`user_modified` 表示该页有非种子修订，被有意跳过。内置 frontmatter 非法会整体中止并写 `seed_import_failed: invalid embedded documentation`。

### 没有自动 seed

自动 seed 需要 `seed.enabled`、`seed.auto`、`solo` profile 且数据库为空（无页面、无文档）。失败只记 `auto seed failed; serving without seed`。

## 4. 导入

### `imported: 0`，全部是 `skipped`

- 扩展名不在 `.md`、`.markdown`、`.txt`、`.html`、`.htm`、`.pdf` 内；
- 隐藏文件、符号链接，或位于 `target`、`node_modules`、`data`、`blobs`、`.lkb`；
- 同一路径同一内容已经导入过（幂等）；
- PDF 没有带 `pdf` feature 构建。

### `doc_id` 变了

只有规范化路径或内容哈希变化才会改变身份。移动/重命名文件会产生新 `doc_id`，旧记录写 `superseded_by`。

### JSONL 导入失败

每行必须是合法 JSON，含字符串 `path` 与 `content`；`acl` 必须是对象或可解析的 JSON 字符串；`valid_from`/`valid_to` 必须是 RFC3339。

### team 导入报空间不存在

首次向不存在的空间导入时必须提供 `--acl-json`；team/enterprise 不会自动创建 allow-all 空间。

## 5. 检索、回答与流式

### 搜索结果为空

确认导入报告 `imported > 0`；确认 `--lang`/`lang` 与内容语言一致（错误的语言过滤会改变候选集）；再看 `degraded[]` 是否有 `keyword_unavailable`。

### `keyword_exact_fallback`

全文查询没有命中，系统改用字面子串匹配。类别是 `quality`。

### `decision_unavailable`

`bm25_judge` 使用网关 alias `retrieval.judge_alias`（默认 `default`）；alias 不存在、网关调用失败或分数数量不匹配都会写这个码。

### event 通道的降级码

`event_not_enabled`（未配置抽取器）、`event_extraction_unavailable`（无事件/索引不可用）、`event_no_seed`（无种子实体）、`event_channel_timeout`（超过 `event_timeout_ms`）。都回退其他通道。

### 流式端点返回 HTTP 200 但收到 `error` 事件

SSE 端点在流内校验认证/CSRF/限流；失败以 `event: error` + `{code, message}` 返回。cookie 会话写请求缺 `X-CSRF-Token` 时会在流内得到 `forbidden`。

### `stream_fallback`

上游不支持流式，回答被缓冲后一次返回。

### `conversation_rewrite_failed`

多轮改写失败（或改写结果为空）；系统使用原查询。这是查询响应 `degraded` 数组里的纯字符串码。

## 6. 摘要与会话

### 页面摘要返回 `status:"pending"`

还没有摘要记录就绪；响应同时带 `summary_generation_failed`，`detail:"pending"`。用 `lkb summary run` 推进队列，或 `lkb summary list --status queued` 查看任务。

### 摘要一直是 `failed`

生成在 `summary.max_attempts` 次后仍失败；系统仍会返回抽取式回退（`model="extractive"`）。检查网关 alias `summary.model_alias` 与 provider 密钥。

### 会话接口返回 403

`GET /api/v1/conversations/{id}` 对已删除会话和非 owner 返回 403（admin 可恢复）。`POST /api/v1/query` 还要求会话处于 active 且空间与请求一致。

## 7. 网关、配额、计费与支付

### `model_unavailable` / `model_forbidden` / `model_invalid_request`

alias 不存在或无健康路由；访问策略拒绝了 tenant/space/role；或上游返回确定性 4xx。provider 拓扑与 base_url 不会暴露。

### `model_rate_limited` / `model_timeout`

上游 429 或 5xx/超时；网关打开 30 秒熔断并尝试下一路由，全部失败时返回 429/503 与 `Retry-After: 30`。

### `quota_exceeded`（429，Retry-After 60）

硬性 token 配额（tenant/space/user/role）超限。软配额只写审计 `quota.soft_exceeded`。

### `payment_required`（402）

存在钱包或 entitlement，但余额/包含 token 不足，或 entitlement 处于 `suspended`/`cancelled`。充值或激活套餐。

### `payment_not_configured` / `payment_provider_unavailable`

微信/支付宝返回 `payment_not_configured`；未知 provider（包括 PayPal）返回 `payment_provider_unavailable`。Stripe 需要 `STRIPE_SECRET_KEY`（或配置的变量）。Stripe 的退款与订阅同步**未实现**；只有 webhook 退款路径（写反向分录）。

### `lkb billing checkout` 总是失败

CLI 命令固定以 `payment_provider_unavailable: payment adapter is not implemented in this release` 结束。需要在配置好 provider 后使用 HTTP 的 `/api/v1/billing/checkout`。

### `feature_disabled` / `billing_not_enabled`

计费路由要求 `features.billing`；billing webhook 还要求 `billing.enabled`；审计查询要求 `features.audit`；连接器接口要求 `features.connectors`。

## 8. 认证、角色与空间

### 登录成功但请求仍 401

非回环地址通过 HTTP 访问时不会发送 Secure cookie。使用 HTTPS 或回环访问。

### 403 CSRF / Origin

cookie 会话写请求需要 `csrf_token`（表单）或 `X-CSRF-Token`（JSON）以及 `lkb_csrf` cookie；带 `Origin` 时必须与 Host 一致。Bearer 请求豁免。

### SSO 401：peer / user / 自动创建 / disabled

- `untrusted peer`：请求的 TCP peer 不在 `sso.trusted_proxies` → 401。确认代理地址（IP/CIDR）已加入列表，且应用前面没有另一层代理改写 peer。
- `missing user`：配置的 user header 缺失或为空 → 401。反代必须注入该头；header 名可配置且大小写不敏感。
- `auto_provision = false` 且用户不存在 → 401：预创建用户（email、`source=sso` 的 external_id、username 任一可匹配），或打开自动创建。
- 用户被禁用 → 401：由管理员恢复用户。
- `[sso]` 配置非法（mode、空 header、代理 IP/CIDR、default_role、group_role_map、email 域、TTL=0）会在启动或 `config check` 时直接失败；`trusted_proxies` 为空只 WARN，但所有 SSO 请求返回 401。
- 三个 SSO 头都不存在时不会走 SSO，普通未登录请求自然返回 401。

### SSO 403：email 域 / 身份冲突

- `allowed_email_domains` 非空时，email 必须存在且域名精确匹配（大小写不敏感，不支持子域通配）；email 缺失或 `evil.example` 都返回 403。
- 已有用户的 email 与传入值不同 → 403 `email_conflict`；先修正身份或邮箱绑定。

### 伪造 Header 与直接暴露

- `x-lkb-sso-authenticated` 始终被剥离；伪造它不会获得身份。
- 直连应用端口并自带 SSO 头时，peer 不在信任列表 → 401；不要把应用端口暴露给客户端。
- `GET /api/v1/auth/sso/status` 公开返回 `{enabled, mode}`，可用于确认模式；`features.sso` 不决定启用（由 `[sso].mode` 决定）。OIDC/SAML/SCIM 未实现。

### SSO 会话与写请求

成功后会签发 HttpOnly 会话 cookie（并轮换旧会话）；写请求仍按普通会话要求 CSRF。TTL 取 `sso.session_ttl_hours`，缺省用 `auth.session_ttl_hours`。

### `last_space_owner_protected` / `last_group_admin_protected`（409）

不能移除或降级最后一个空间 owner / 组 admin；先指定继任者。

### `space_not_active`（409）

空间已归档或软删除；写入前先恢复。

### `space_quota_exceeded` / `tenant_quota_exceeded`（413 或 429）

字节维度返回 413，计数维度（members/spaces）返回 429。查看 `space usage` 与 `quota_overrides`。存储的 soft 限制当前不阻断。

## 9. Wiki

### JSON 保存返回 400 或 409

`If-Match` 是必需的；缺头返回 400（"If-Match header is required"），`current_revision_id` 过期返回 409 `revision_conflict`。重新读取页面后再提交。HTML 表单路径没有前置检查，是最后写入生效。

### 页面列表看起来是空的

`GET /api/v1/pages?lang=` 是在取回页面行（最多 500 行）之后按语言过滤的（不是 SQL 过滤）；过期或错误的 `lang` 参数会隐藏页面。同时确认空间成员角色至少是 `viewer`。

## 10. 数据与备份

### `export` 包含 Wiki 页面吗

它导出文档记录，不导出 Page/Revision 表；Wiki 内容以 `wiki/...` 文档出现。版本历史需要数据库级备份。

### 恢复后 `doc_id` 是否一致

路径与内容不变时一致；导出包带 `doc_id`，导入时会比对。

### 可以删除 SQLite 文件重建索引吗

不可以。那会同时删除页面、修订、用户、令牌、会话、用量与计费数据。重建应从源或导出包重新导入。

## 11. 降级码速查

| code | 类别 | 何时出现 |
| --- | --- | --- |
| `keyword_unavailable` | evidence | 关键词索引不可用 |
| `keyword_exact_fallback` | quality | 全文无命中，改字面匹配 |
| `dense_not_enabled` | evidence | 未配置 embedding provider |
| `embedding_unavailable` | evidence | 查询嵌入失败 |
| `index_missing` | evidence | 没有切片向量 |
| `decision_unavailable` | quality | judge 未配置或失败 |
| `llm_unavailable` | quality | 生成失败 |
| `citation_alignment_failed` | quality | 生成答案的引用无法对齐 |
| `acl_second_check_dropped` | security | 二次 ACL 校验丢弃候选 |
| `event_not_enabled` | evidence | event 抽取未启用 |
| `event_extraction_unavailable` | evidence | 抽取或索引不可用 |
| `event_no_seed` | evidence | 没有种子实体 |
| `event_channel_timeout` | performance | event 通道超时 |
| `stream_fallback` | performance | 上游不支持流式 |
| `summary_generation_failed` | quality | 静态摘要未就绪或生成失败 |
| `conversation_rewrite_failed` | quality | 多轮改写失败（字符串码） |

排序优先级是 security > evidence > quality > performance。`malformed_hit` 在契约中已定义，但当前没有调用点。

## 12. 错误码速查

| 状态 | 错误码 |
| --- | --- |
| 400 | `bad_request`、`invalid_query`、`invalid_top_k`、`model_invalid_request`、`invalid_billing_request`、`payment_invalid` |
| 401 | `unauthorized` |
| 402 | `payment_required` |
| 403 | `forbidden`、`feature_disabled`、`billing_not_enabled`、`model_forbidden` |
| 404 | `not_found` |
| 409 | `conflict`、`revision_conflict`、`space_not_active`、`last_space_owner_protected`、`last_group_admin_protected` |
| 410 | `client_disconnected` |
| 413 | `space_quota_exceeded`、`tenant_quota_exceeded`（字节） |
| 429 | `rate_limited`、`quota_exceeded`、计数类配额、`model_rate_limited` |
| 500 | `internal`、`billing_unavailable`、`quota_check_failed` |
| 503 | `queue_full`、`timeout`、`model_unavailable`、`model_timeout`、`payment_not_configured`、`payment_provider_unavailable`、`storage_unavailable`（`/health`） |

## 13. 连接器

### `connector_not_configured`

`test` 返回 HTTP 200，但正文是 `{"ok":false,"status":"not_configured",...}`；`sync` 任务以 `failed` 结束。`status` 字段固定为 `not_configured`，真正的区分看 `code`：厂商 stub 返回 `connector_not_configured`；配置/文件/HTTP 问题分别返回 `connector_invalid_config`、`connector_io_error`、`connector_http_error`。当前可用的三类源是 `filesystem_export`、`jsonl`、`http_json`。

### `connector_invalid_config` / `connector_io_error` / `connector_http_error`

- `connector_invalid_config`：`config` 不是合法 JSON，或缺少 `dir`/`path`/`base_url`，或清单/JSONL 行不是约定结构；创建/修改通常返回 400 `bad_request`。
- `connector_io_error`：`filesystem_export` 的清单或 `jsonl` 的文件读不到。
- `connector_http_error`：`http_json` 的 GET 失败、非 2xx 或超过 15 秒。

### 同步任务 `failed` / `partial`

`POST /api/v1/connectors/{id}/sync` 返回 202，任务在后台执行。用 `GET /api/v1/connectors/{id}/jobs?limit=50` 查看 `status` 与 `stats_json`：`items`、`indexed`、`refs_only`、`failed`、`errors[]`。条目级错误码包括 `connector_invalid_path`、`connector_index_failed`、`connector_ref_failed`。`partial` 表示部分条目成功；没有 `content` 的条目只会成为 `refs_only`，这是预期行为。

### ACL 拒绝或文档搜不到

条目缺少 `acl` 或 ACL 非法时，连接器使用空 allow list（拒绝），不会回退 allow-all；`tests/connectors.rs` 明确覆盖该行为。即使 ACL 允许，还需要空间成员角色至少 `viewer`（空间角色 ∩ 文档 ACL）。`external_refs` 仍会记录引用，但文档可能不可见或未被索引。

### 403 `feature_disabled` / admin scope

连接器接口要求 admin scope，并受 `features.connectors` 门禁；solo 默认关闭，team/enterprise 默认开启。可在 `[features]` 显式打开，否则返回 403 `feature_disabled`。

### 深链与原件

系统不保存网盘/附件原件，只保存条目的 `content` 文本、`url` 深链、`revision`、ACL 快照与哈希。`external_refs.url` 可用 `GET /api/v1/external-refs?doc_id=&provider=&limit=` 查询；`citations[].source_url` 当前不填充。

## 14. 已知限制

- PostgreSQL 运行路径未验证（SKIP）；`usage_events` 绑定缺陷已修复，静态 SQL 占位符/bind 守卫通过，仍需实机 PG 验证。
- 连接器框架已实现（`filesystem_export`/`jsonl`/`http_json`）；飞书/SharePoint/OneDrive/Confluence/MediaWiki 适配器仍是 stub；SSO 目前仅 `trusted_header`（OIDC/SAML/SCIM 与 HTTP 层多租户未实现）；功能开关只有 `billing`、`audit`、`connectors` 真正拦截。
- 微信/支付宝 `not_configured`；Stripe 退款/订阅同步与 PayPal 不可用；没有税务、发票 PDF 或催收。
- `lkb auth login --device` 返回 `not_implemented`；CLI 是本机优先的。
- Rust 服务不托管 `web/dist`，SPA 需要独立静态部署；前端没有单测/E2E。
- 没有答案缓存（`cache_hit` 恒为 `false`），也没有语义缓存。
- `summary.timeout_ms`、`[limits].max_queue`、`payment.mode`、`payment.secret_env`、`billing.grace_days` 已配置但未使用；存储 soft 限制不阻断；`lkb_slow_queries_total` 没有调用者。
- 限流与存储配额预留是进程内的；多实例会在边界上超额放行。
- 会话不写审计；没有 claim 存储与接口；`citations[].source_url` 不填充。

## 15. 相关文档

- [03 快速开始](03-quickstart.md)
- [05 检索与 AI 回答](05-search-ai.md)
- [06 认证与安全](06-auth-security.md)
- [08 部署与运维](08-deployment.md)
