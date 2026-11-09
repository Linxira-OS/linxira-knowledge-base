---
title: 核心概念
slug: 02-concepts
lang: zh
order: 2
summary: 解释 space、page、revision、chunk、doc_id、claim、Answer、degraded 与 validity 的含义及相互关系。
---

# 核心概念

这篇文档只定义产品中反复出现的名词，以及它们在本版中的实现边界。实现细节见 [10 架构与实现](10-architecture.md)。

## 1. space（空间）与成员

空间是权限、配额与生命周期的隔离单元，字段名为 `space_id`。召回查询、Wiki 页面、文档、切片与事件都带空间字段。

- solo 固定使用 `s_default`，由启动流程自动创建为显式 allow-all 空间。
- team / enterprise 使用配置的 `storage.default_space`，也可以创建其他空间。
- 空间可见性：`private`（默认，仅成员可见）或 `internal`（租户目录可见名称/描述/owner，读正文仍需权限）。
- 空间状态：`active`、`archived`（只读）、`deleted`（软删除）。
- 空间 owner 可以是 `user` 或 `group`（`owner_type`/`owner_id`）；`settings` 里可配置组派生默认角色（`group_member_space_role`，默认 `viewer`，可改为 `editor`）。

成员关系存在 `space_members(space_id, principal_type, principal_id, role)`，主体类型为 `user` / `group` / `agent`。注意：空间成员资格与文档 ACL 是**两层**过滤，两者都通过才能读到内容。Wiki 页面的文档 ACL 当前统一为 `allow_all`，实际权限由空间角色决定；导入文档可以携带真实 ACL。

## 2. page（页面）与 revision（修订）

页面是内置 Wiki 的编辑容器；检索的是它下面的修订。

| 对象 | 关键字段 | 说明 |
| --- | --- | --- |
| Page | `tenant_id`、`space_id`、`lang`、`slug`、`title`、`current_revision_id`、`version`、`acl_json`、`status`/`valid_to` | `(tenant, space, lang, slug)` 唯一；`version` 从 1 递增 |
| Revision | `revision_id`、`lang`、`markdown`、`content_hash`、`author`、`created_at`、`version`、`acl_json`、`source_url`、`source_revision` | 保存时的不可变快照 |

保存行为：版本号加一、写入新 Revision、页面指向新 Revision、同一 Markdown 编译为 Document/Chunk 并同步索引。`revision_id` 形如 `rev_` 加 32 位十六进制，JSON API 的 `If-Match` 用的就是它。

`author` 为 `system:seed` 表示内置种子页面；用户编辑后的修订使用真实 `principal_id`，种子升级不会覆盖。

## 3. document、chunk 与 doc_id

文档是可检索的逻辑对象，来自本地文件、JSONL 导入包、种子页面或 Wiki 页面；切片按行切分（默认每块最多 1000 字符），记录 `line_start` / `line_end`，引用可以回到源文件行号。

`doc_id` 派生规则固定：

```text
canonical_key = 源根目录下的相对路径，统一用 /，拒绝绝对路径、盘符与 ..，ASCII 小写
content_hash  = 源内容的 SHA-256（Wiki 页面取 Markdown 字节）
doc_id        = sha256(canonical_key + "\0" + content_hash) 的前 16 字节，小写 hex
```

- 同一路径同一内容重复导入或重建索引，`doc_id` 不变，导入计为 `skipped`。
- 同一路径内容更新，生成新 `doc_id`；旧文档标记 `superseded`，`superseded_by` 指向新 ID。
- Wiki 页面的逻辑路径为 `wiki/{space}/{lang}/{slug}.md`；种子文档同样如此。
- 路径规范化版本 `p1` 不做 Unicode NFC 归一化，这是已知边界。
- 切片 ID 以 `chk_` 开头，页面修订 ID 以 `rev_` 开头，都不参与 `doc_id` 计算。

## 4. claim（结构化断言）

claim 是“某条可定位的断言，带提出人、复核状态、有效期与证据位置”的模型，用于把人工确认从原始文本中分离。

**当前构建边界**：数据库中没有 claim 表，也没有写入、查询或复核接口。`conflicts[]` 的检测口径是“同一标题下出现多个不同 `doc_id`”，不是 claim 冲突。评估 claim 能力时，请把它视为数据模型概念，而不是已交付功能。

## 5. Answer（回答契约）

`lkb ask`、`POST /api/ask`、`POST /api/v1/answer` 与 `/api/v1/query` 返回同一结构：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| `answer` | string 或 null | 正文；没有证据时为 `null` |
| `citations` | array | `doc_id`、`chunk_id`、`path`、`line_start`、`line_end`、`quote`，可选 `source_url` |
| `confidence` | object | `score`、`band`（`high`/`medium`/`low`）、`basis[]` |
| `conflicts` | array | 冲突列表；当前基于相同标题 |
| `degraded` | array | 降级列表；正常时是空数组 |
| `knowledge_as_of` | string 或 null | 被引用文档 `valid_from` 的最小值；`null` 表示不适用 |
| `mode` | string | `extractive` 或 `generative`（`hybrid` 在契约中定义但运行时不产生） |
| `model` | string，可选 | 生成成功时为公开 alias |
| `usage` | object，可选 | `input_tokens`/`output_tokens`/`cached_tokens`/`token_source`/`cost_micros`/`currency` |
| `cache_hit` | bool | 当前恒为 `false`；答案缓存未实现 |
| `conversation_id`/`message_id` | string，可选 | 绑定会话时写入 |
| `summary_kind`/`summary_source` | string，可选 | `dynamic`；来源为 `llm` 或 `extractive` |

回答最多引用前 5 条命中；生成式回答至少要有 1 个合法引用，否则回退抽取式。`confidence` 是启发式分数，不是校准概率（`high ≥ 0.70`、`medium ≥ 0.40`）。

## 6. degraded（显式降级）

可选能力未启用、不可用或超时时，会往 `degraded[]` 追加记录：

| 字段 | 含义 |
| --- | --- |
| `code` | 稳定降级码，例如 `decision_unavailable` |
| `category` | `security`、`evidence`、`quality`、`performance` |
| `capability` | 受影响能力，例如 `judge`、`event`、`dense`、`summary`、`stream` |
| `expected_level` / `actual_level` | 期望与实际能力等级 |
| `reason` | `not_enabled`、`unavailable`、`timeout`、`model_missing`、`budget_exceeded`、`stale`、`error` |
| `impact` / `fallback` | 影响与实际替代路径 |

排序优先级固定为 `security` > `evidence` > `quality` > `performance`。身份认证与 ACL 失败不属于降级：它们返回 401/403/409。除 `Degraded` 结构外，`/api/v1/query` 的 `degraded` 数组还可能出现纯字符串码 `conversation_rewrite_failed`。

## 7. validity（时序有效性）

文档、切片与事件都带 `valid_from` 与 `valid_to`：

- 时间戳统一为 RFC3339 UTC、秒精度、以 `Z` 结尾。
- 召回条件为 `valid_from <= as_of` 且（`valid_to` 为空或 `valid_to > as_of`）。
- `as_of` 默认当前时间；CLI 用 `--as-of`，HTTP 由查询参数或服务端默认。
- `valid_to` 为 `null` 表示“未知失效时间”，不等于永久有效。
- 归档页面把 `valid_to` 作为归档标记；软删除空间在物理清理前仍计入存储。

## 8. 角色、组与空间成员

角色分三层，互不自动叠加：

| 层 | 角色 | 作用域 |
| --- | --- | --- |
| 租户 | `owner` > `admin` > `user`（agent 由 token kind 表达） | 整个 tenant：建组、建空间、看审计、管理用户 |
| 组 | `group_admin` > `group_member` | 一个 group：组空间与组成员 |
| 空间 | `space_owner` > `space_admin` > `editor` > `viewer` | 一个 space：页面、成员、配额、共享 |

有效空间角色按固定优先级求值：**显式 `space_members` 行 > 组派生 > `owner_type`**；多组取最高，租户 admin 走 break-glass（记为 `space_owner`，并写审计）。

## 9. 配额：存储配额与 token 配额

两套配额完全独立：

| 系统 | 管控对象 | 维度/scope | 超限行为 |
| --- | --- | --- | --- |
| 存储配额 `space_quotas`/`storage_usage` | 空间内容容量 | tenant/group/space/user × storage_bytes/documents/chunks/members/spaces | 字节维度 413 `space_quota_exceeded`/`tenant_quota_exceeded`；计数维度 429 |
| token 配额 `token_quotas`/`quota_counters` | 模型调用 | tenant/space/user/role × tokens/requests/cost，`hard`/`soft` | hard 429 `quota_exceeded` + `Retry-After: 60` |

存储配额写入前预留、成功重算、失败释放，默认不配置（不限）；管理员可用 `quota_overrides`（带 reason/expires_at，写审计）临时覆盖。token 配额调用前估算预留、成功后提交实际用量、失败释放。

## 10. 模型别名、供应商与路由

模型网关把“对外模型名”和“内部供应商”分开：

| 概念 | 说明 |
| --- | --- |
| Alias（公开模型） | 客户端使用的稳定 `public_id`（如 `default`）；Answer 的 `model` 字段就是它 |
| Provider | 供应方或独立端点：base_url、api_key_env、优先级、权重、超时 |
| Provider Model | provider 上的具体上游模型，含上下文与内部成本 |
| Route | alias 到 provider_model 的路径：优先级、权重、health、circuit |

请求按 alias → 访问策略 → 优先级/权重选路；429、5xx、超时切下一路由并打开 30 秒熔断；400/401/403 等确定性错误不盲目重试。客户端永远看不到 base_url、upstream_model 或密钥。

## 11. usage、钱包、账本、套餐与订阅

- `usage_events` 记录每次模型调用：tenant/space/user、alias、provider_model_id、input/output/cached tokens、`token_source`（`provider` 或 `estimated`）、成本、延迟、状态、`source`（`kb`/`judge`/`summary`）。
- `wallets` + `ledger_entries` 是双分录账本：每组借贷平衡，`idempotency_key` 唯一，只追加、不更新，退款走反向分录。
- `plans` 定义价格与包含 token；默认内置 `free`、`starter`、`pro` 三档。
- `subscriptions` 状态机：`trial → active → past_due → suspended → cancelled`，非法迁移返回 400。
- `entitlements` 是套餐快照：计费时先扣套餐包含 token，再扣钱包余额；余额不足返回 402 `payment_required`。
- `billing.engine` 为 `local` 时写本地账本；其他值只发事件、读本地 entitlement 快照，不写本地财务表。

## 12. 摘要、会话与消息

- **静态摘要**：`document_summaries` 按 `(tenant, space, doc_id, revision_id, model, prompt_version)` 唯一；`summary_jobs` 驱动异步生成，状态 `queued → running → ready/failed`，失败有限重试后回退抽取式并保留 `status=failed`。
- **动态回答**：每次查询经多轮改写、检索、证据融合、生成与引用对齐；`summary_kind=dynamic`。
- **会话**：`conversations`（tenant/space/owner/title/`external_session_id`/status）与 `messages`（role/content/citations/model/usage/degraded/mode/`rewritten_query`/`conversation_context_hash`）。软删除写 `status=deleted`，可恢复。
- **外部会话**：`external_session_id` 映射 Zeta 等 Agent 的 session；KB 只存用户可见消息，不存 chain-of-thought。

## 13. 部署模式、功能门禁与 locale

- `deployment.mode`（`solo`/`team`/`enterprise_internal`/`enterprise_external`）决定 `[features]` 默认值与启动校验；领域逻辑只调用 `feature_enabled`。
- `billing`、`audit` 与 `connectors` 已实际门禁；其余 feature（`signup`、`sso`、`multi_tenant`、`public_api`、`mcp`、`public_share`）仅出现在配置与 `GET /api/v1/features`，不改变行为；`rate_limit` 始终按 `[limits]` 生效。
- **UI locale 与内容 `lang` 是两件事**：locale（`zh-CN`/`en`）决定界面语言与 API 错误消息；`lang`（`zh`/`en`）是内容元数据，决定召回 SQL 的 `lang` 过滤。

## 14. 概念之间的关系

```text
tenant
  ├── user / group（space_members, group_members）
  ├── space（owner_type/owner_id, status, settings）
  │     ├── page（tenant, space, lang, slug）── revision（lang, content_hash, If-Match）
  │     │        └── document（wiki/{space}/{lang}/{slug}.md）── chunk（lang, 行号区间）
  │     ├── document（本地文件 / JSONL / seed）
  │     └── event / entity（可选抽取，派生索引）
  ├── model_aliases → routes → provider_models → providers
  ├── usage_events → token_quotas / space_quotas
  ├── wallets / ledger_entries / plans / subscriptions / entitlements
  └── conversations ── messages（引用 Answer 与 citations）

一次检索：Policy(tenant, space, ACL, as_of, lang) → 候选 → Answer(citations, degraded, knowledge_as_of)
```

## 15. 相关文档

- [04 内置 Wiki](04-wiki.md)
- [05 检索与 AI 回答](05-search-ai.md)
- [06 认证与安全](06-auth-security.md)
- [10 架构与实现](10-architecture.md)
