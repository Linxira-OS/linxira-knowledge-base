---
title: 配置参考
slug: 07-configuration
lang: zh
order: 7
summary: lkb.toml 全字段默认值、环境变量覆盖、模型 provider 与密钥处理规则。
---

# 配置参考

配置只有一个入口：`lkb.toml`。文件不存在时全部使用默认值；环境变量在文件之后覆盖；最后做归一化与校验。校验失败会阻止启动。

## 1. 加载顺序

1. 读取 `--config` 指定的文件（默认 `lkb.toml`）。
2. 文件存在则按 TOML 解析，否则用内置默认值。
3. 应用 `LKB_*` 环境变量覆盖。
4. 归一化：按后端推断 `profile`；solo 强制固定治理 ID；非 solo 收紧限额；解析 `deployment.mode` 与功能默认值。
5. 校验；失败则退出。

`profile` 未设置时，SQLite 推断为 `solo`，PostgreSQL 推断为 `team`。solo 归一化会强制 `tenant_id = t_local`、`default_space = s_default`，并关闭自助建组/建空间。`deployment.mode` 未设置时：`solo → solo`、`team → team`、`enterprise → enterprise_internal`；`enterprise_external` 只能显式配置。

## 2. profile、server 与 auth

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `profile` | 未设置，按后端推断 | `solo` / `team` / `enterprise` |
| `server.bind` | `"127.0.0.1:7331"` | 必须是 `host:port` |
| `auth.mode` | `"none"` | `none` 或 `session`；`none` 只允许回环 |
| `auth.cookie_name` | `"lkb_session"` | 会话 cookie 名 |
| `auth.cookie_secure` | `false` | 非回环 Host 会自动加 Secure |
| `auth.session_ttl_hours` | `168` | 必须大于 0 |
| `auth.csrf` | `true` | 关闭后 Origin 校验仍然执行 |

### [sso]（可信 Header SSO）

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `sso.mode` | `"off"` | `off` 或 `trusted_header`；其他值启动失败 |
| `sso.user_header` | `"X-Auth-Request-User"` | 身份头；为空启动失败 |
| `sso.email_header` | `"X-Auth-Request-Email"` | 身份头；为空启动失败 |
| `sso.groups_header` | `"X-Auth-Request-Groups"` | 身份头；为空启动失败 |
| `sso.trusted_proxies` | `[]` | 允许注入头的反代 IP/CIDR；非法条目启动失败；空列表允许，但 `trusted_header` 模式下 `serve` 启动 WARN，SSO 请求全拒 |
| `sso.auto_provision` | `true` | 未知用户是否自动创建（`source=sso`） |
| `sso.default_role` | `"user"` | 自动创建时的默认角色；必须为 `owner`/`admin`/`user` |
| `sso.group_role_map` | `{}` | 组名（非空）→ `owner`/`admin`/`user`；非法键值启动失败；多组取最高 |
| `sso.allowed_email_domains` | `[]` | 精确域名白名单（大小写不敏感）；每项为空、含 `@`/`/`/空白 → 启动失败；空列表 = 不限制 |
| `sso.session_ttl_hours` | 无 | 设置时必须大于 0；缺省使用 `auth.session_ttl_hours` |

校验与生效规则：

- `config check` 会执行 `[sso]` 校验（即使 `mode = "off"`），非法配置直接退出；但它不打印 `[sso]` 段，只打印 `features.sso`。
- 具体校验：`mode` 只能是 `off`/`trusted_header`；三个 header 均非空；`trusted_proxies` 每个条目都能解析为 IP 或 CIDR；`default_role` 与 `group_role_map` 的值都能解析为 `owner`/`admin`/`user`；`group_role_map` 键非空；`allowed_email_domains` 每项非空且不含 `@`、`/` 或空白；`session_ttl_hours` 为 `Some(0)` 时拒绝。
- `trusted_proxies` 为空不会阻止启动；当 `mode = "trusted_header"` 时，`serve` 会记录 WARN `SSO trusted_header is enabled but trusted_proxies is empty; SSO will deny all peers`，且所有携带 SSO 头的请求返回 401。
- 没有 `LKB_SSO_*` 环境变量覆盖；该中间件只由 `[sso].mode` 控制，`features.sso` 不参与。行为细节见 [06 认证与安全](06-auth-security.md)。

## 3. storage

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `storage.backend` | `"sqlite"` | `sqlite` 或 `postgres`（接受 `postgresql`/`pg`） |
| `storage.sqlite_path` | `"data/lkb.sqlite"` | 父目录自动创建 |
| `storage.url_env` | `"LKB_DATABASE_URL"` | 存放 PostgreSQL URL 的环境变量名 |
| `storage.tenant_id` | `"t1"`（solo 强制 `t_local`） | 非空；一个进程只服务一个租户 |
| `storage.default_space` | `"s_default"` | 非空且必须是单个路径段 |

## 4. spaces 与 groups

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `spaces.allow_user_creation` | `true`（solo 强制 `false`） | 自助建空间 |
| `spaces.max_per_user` | `10` | 每个 owner 的空间数上限 |
| `spaces.default_storage_bytes` | 无 | 新空间默认存储配额；`None` 表示不限 |
| `spaces.default_documents` | 无 | 新空间默认文档数配额 |
| `spaces.default_chunks` | 无 | 新空间默认切片数配额 |
| `spaces.default_members` | 无 | 新空间默认成员数配额 |
| `groups.allow_user_creation` | `true`（solo 强制 `false`） | 自助建组 |

## 5. deployment 与 features

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `deployment.mode` | 按 profile 推断 | `solo` / `team` / `enterprise_internal` / `enterprise_external` |
| `deployment.commercial` | `false` | 为真要求 `features.billing = true` |
| `features.*` | 按模式给默认值 | 显式布尔值覆盖默认 |

各模式的功能默认值：

| 功能 | solo | team | enterprise_internal | enterprise_external |
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

**`billing`、`audit` 与 `connectors` 已在运行时实际门禁。** 其余开关（`signup`、`sso`、`multi_tenant`、`public_api`、`mcp`、`public_share`）仅出现在 `GET /api/v1/features`，不改变行为；`rate_limit` 始终按 `[limits]` 生效。校验要求 `deployment.mode` 与 `profile` 匹配、solo 禁止 billing、`enterprise_external` 必须 `signup + billing + multi_tenant + public_api` 全开，enterprise 不允许关闭 audit。

## 6. i18n

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `i18n.default_locale` | `"zh-CN"` | 必须在 `supported_locales` 中 |
| `i18n.supported_locales` | `["zh-CN","en"]` | 必须包含 `en`；上限 28 个 |
| `i18n.tenant_locale` | 无 | 可选的租户默认 |

运行时实际识别的 locale 只有 `zh-CN` 与 `en`；其他标签按回退链处理（`Accept-Language` → 用户偏好 → 租户默认 → 配置默认 → `en`）。API 错误返回稳定 `code` + 本地化 `message`，响应带 `Content-Language`。

## 7. limits

| 字段 | 默认值 | team/enterprise |
| --- | --- | --- |
| `qps_instance` | `1000` | `500` |
| `qps_ip` | `100` | `30` |
| `qps_user` | `500` | `100` |
| `qps_token` | `200` | `60` |
| `qps_tenant` | `1000` | `300` |
| `qps_space` | `500` | `120` |
| `max_concurrency_instance` | `256` | `128` |
| `max_concurrency_user` | `64` | 不变 |
| `max_concurrency_tenant` | `256` | 不变 |
| `max_concurrency_space` | `64` | 不变 |
| `max_queue` | `512` | `128`（运行时未读取） |
| `daily_requests_token` | `1000000` | `200000` |
| `retry_after_seconds` | `1` | 不变 |
| `max_query_chars` | `8192` | 不变 |
| `max_top_k` | `100` | 不变 |

非 solo 的收紧只覆盖仍等于默认值的项。限流状态是进程内的。

## 8. summary、query、seed 与 retrieval

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `summary.enabled` | `true` | 总开关 |
| `summary.auto` | `true` | 导入/Wiki/seed 后入队 |
| `summary.model_alias` | `"default"` | 网关 alias |
| `summary.prompt_version` | `"summary-v1"` | 变更会触发重新生成 |
| `summary.max_chars` | `1200` | 输入截断 |
| `summary.batch` | `8` | worker 批大小 |
| `summary.timeout_ms` | `15000` | 已校验但运行时未使用 |
| `summary.max_attempts` | `3` | 失败重试上限 |
| `summary.retry_backoff_ms` | `1000` | 重试间隔 |
| `query.rewrite_enabled` | `true` | 多轮改写 |
| `query.rewrite_alias` | `"default"` | 改写用的网关 alias |
| `query.context_messages` | `6` | 参与上下文的最近消息数 |
| `seed.enabled` | `true` | 种子子系统开关 |
| `seed.auto` | `true` | solo 空库 `serve` 时自动 seed |
| `seed.space` | `"product"` | 必须是单个路径段 |
| `retrieval.mode` | `"auto"` | `bm25`、`keyword`、`bm25_judge`、`event`、`event_entity`、`hybrid`、`auto` |
| `retrieval.judge_alias` | `"default"` | judge 使用的网关 alias |
| `retrieval.event_depth` | `1` | 1 或 2 |
| `retrieval.event_max_seeds` | `10` | 1–50 |
| `retrieval.event_max_events` | `50` | 1–500 |
| `retrieval.event_timeout_ms` | `800` | 1–10000 |
| `retrieval.event_text_weight` | `0.25` | 有限非负数 |
| `retrieval.rrf_bm25_weight` | `1.0` | 有限非负数 |
| `retrieval.rrf_dense_weight` | `1.0` | 有限非负数 |
| `retrieval.rrf_event_weight` | `1.0` | 有限非负数 |

## 9. payment 与 billing

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `payment.provider` | `"none"` | 已实现 `stripe`、`manual`、`mock`；`wechat`/`alipay` 返回 `payment_not_configured` |
| `payment.currency` | `"USD"` | 必须是 3 位大写 ISO 4217 代码 |
| `payment.mode` | `"test"` | `test` 或 `live`；已校验但运行时未读取 |
| `payment.secret_env` | 无 | 必须是变量名，不能是密钥值 |
| `billing.enabled` | `false` | 计费路由的功能开关 |
| `billing.engine` | `"local"` | `local` 写本地账本；其他值只发事件 |
| `billing.currency` | `"USD"` | 钱包/套餐币种 |
| `billing.trial_days` | `14` | 订阅试用天数 |
| `billing.grace_days` | `7` | 已定义但未使用 |
| `billing.stripe_secret_env` | `"STRIPE_SECRET_KEY"` | Stripe API key 变量 |
| `billing.stripe_webhook_secret_env` | `"STRIPE_WEBHOOK_SECRET"` | webhook 验签密钥变量 |
| `billing.webhook_tolerance_seconds` | `300` | 时间戳容差 |

## 10. models

`models.llm`、`models.embedding`、`models.judge`、`models.extractor` 结构相同：

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `provider` | `"none"` | 非 `none` 即启用；所有名称都走 OpenAI 兼容协议 |
| `base_url` | 无 | 启用时必填；只有 `provider = "openai"` 有默认值（`https://api.openai.com/v1`） |
| `model` | 无 | 启用时必填 |
| `api_key_env` | 无 | 存放密钥的环境变量名；变量缺失则该能力不可用 |
| `timeout_ms` | `15000` | 必须大于 0 |
| `max_retries` | `2` | 对 5xx 与传输错误按 200ms × 次数退避重试 |

`[models.llm]` 会在网关中物化为 provider `legacy`、alias `default`、route `route_legacy_default`。`provider = "jev"` 没有特殊实现；Jev 端点必须由 OpenAI 兼容网关暴露。

## 11. 环境变量

| 变量 | 覆盖 |
| --- | --- |
| `LKB_PROFILE` | `profile` |
| `LKB_BIND` | `server.bind` |
| `LKB_AUTH_MODE`、`LKB_COOKIE_NAME`、`LKB_COOKIE_SECURE`、`LKB_SESSION_TTL_HOURS`、`LKB_CSRF` | `auth.*` |
| `LKB_STORAGE_BACKEND`、`LKB_SQLITE_PATH`、`LKB_URL_ENV`、`LKB_STORAGE_TENANT_ID`、`LKB_DEFAULT_SPACE` | `storage.*` |
| `LKB_SEED_ENABLED`、`LKB_SEED_AUTO`、`LKB_SEED_SPACE` | `seed.*` |
| `LKB_SPACES_ALLOW_USER_CREATION`、`LKB_SPACES_MAX_PER_USER` | `spaces.*` |
| `LKB_GROUPS_ALLOW_USER_CREATION` | `groups.*` |
| `LKB_DEPLOYMENT_MODE`、`LKB_COMMERCIAL` | `deployment.*` |
| `LKB_I18N_DEFAULT_LOCALE` | `i18n.default_locale` |
| `LKB_PAYMENT_PROVIDER` | `payment.provider` |
| `LKB_RETRIEVAL_MODE` | `retrieval.mode` |
| `LKB_DATABASE_URL`（变量名由 `storage.url_env` 决定） | PostgreSQL URL |
| `LKB_LOG_FORMAT`（`json`）、`LKB_LOG_LEVEL`、`LKB_SERVICE` | 日志 |
| `LKB_TOKEN`、`LKB_API_TOKEN`、`LKB_CONFIG_FILE` | CLI 凭据 |
| `LKB_USER_PASSWORD` | `lkb user add` 密码 |

`models.*`、`limits.*`、`summary.*`、`query.*`、`billing.*` 以及其余 `retrieval.*` 字段没有环境变量覆盖。solo 归一化发生在环境变量之后，因此 `LKB_STORAGE_TENANT_ID` 在 solo 下仍会被覆盖为 `t_local`。

## 12. 校验规则

以下任一条件失败即拒绝启动：

- `solo` + `postgres`，或 `team`/`enterprise` + `sqlite`。
- `deployment.mode` 与 `profile` 不兼容。
- `commercial = true` 但未开 `features.billing`；solo 打开 billing；`enterprise_external` 缺 `signup`/`billing`/`multi_tenant`/`public_api`；enterprise 关闭 audit。
- i18n：`default_locale` 为空、`supported_locales` 为空、超过 28 个、默认值不在支持列表、缺少 `en` 回退。
- `summary.model_alias`/`prompt_version` 为空；`max_chars`/`batch`/`timeout_ms`/`max_attempts` 为 0；`query.rewrite_alias` 为空或 `context_messages` 为 0。
- `payment.currency` 不是 3 位大写代码；`payment.mode` 不是 `test`/`live`；`payment.secret_env` 不是变量名。
- `server.bind` 不是 `host:port`；`auth.mode = "none"` 且非回环；`session_ttl_hours = 0`。
- `tenant_id`/`default_space` 为空；`default_space` 含多段；`seed.space` 非法。
- PostgreSQL 后端但配置的环境变量中没有 URL。
- `retrieval.mode` 非法、事件参数越界、权重为负或非有限数。
- 任一模型节启用但 `base_url` 不是 http(s)、`model` 为空或 `timeout_ms = 0`。

## 13. 密钥与脱敏

- 模型密钥只从 `api_key_env` 读取；支付密钥从 `billing.stripe_*_env` 读取。两者都不会写入配置、数据库或日志。
- `config check` 只打印变量名与状态（`set`/`missing`/`not configured`），并打码数据库 URL 与 base_url。
- `lkb user add` 读取 `LKB_USER_PASSWORD`；未设置时交互输入会回显。
- CLI 只把 token 写入本机配置文件；密钥值永远不进入 `lkb.toml`。

## 14. 示例

solo（可全部省略）：

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

team / enterprise_external：

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
billing = false        # enterprise_external 会强制打开
audit = true

[i18n]
default_locale = "zh-CN"
supported_locales = ["zh-CN", "en"]

[retrieval]
mode = "auto"
event_depth = 1
```

## 15. 相关文档

- [03 快速开始](03-quickstart.md)
- [06 认证与安全](06-auth-security.md)
- [08 部署与运维](08-deployment.md)
