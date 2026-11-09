---
title: 架构与实现
slug: 10-architecture
lang: zh
order: 10
summary: 面向想理解实现的使用者，讲清 L1–L8 分层、Store/Provider/Policy 三组抽象、写入与查询数据流和代码模块边界。
---

# 架构与实现

这篇文档面向已经会用产品、想判断“它为什么这样工作”的读者。它不复述内部设计文档，只讲实际代码里存在的结构。

## 1. 总体分层

```text
入口（CLI / HTML / JSON API / SPA / MCP）
   │
   ├── 请求中间件：request id、Content-Language、IP 限流、结构化日志、指标
   ├── L1 接入与身份：cookie 会话 / Bearer / solo 主体、角色、功能门禁
   ├── L2 查询理解：检索模式、语言过滤、多轮改写
   ├── L3 多路检索：关键词 / dense / event
   ├── L4 融合与重排：RRF、judge、二次 ACL 校验
   └── L5 回答与呈现：抽取式或生成式、引用、SSE、降级
          │
L6 知识底座（导入、Wiki、seed、摘要、会话） → L7 存储与索引 → L3
L8 评测与运维（eval、health、metrics、audit）
X 横切（租户、ACL、时序、配额、日志、i18n）
```

## 2. L1–L8 的职责

| 层 | 当前实现 |
| --- | --- |
| L1 接入与身份 | axum 路由；HTML 会话、JSON 认证、Bearer、solo 主体；`access.rs` 求值租户/组/空间角色；功能门禁拦截 billing/audit/connectors |
| L2 查询理解 | 检索模式解析、语言过滤、`[query]` 多轮改写（经网关） |
| L3 检索 | 关键词 SQL、可选嵌入与余弦、可选 event/entity 扩展；每路独立预算 |
| L4 融合与重排 | hybrid 用 RRF（k=60）；`bm25_judge` 经 `retrieval.judge_alias` 重排前 30 条；`Policy::can_read` 二次校验 |
| L5 回答 | 前 5 条命中生成引用；生成式经网关 alias；SSE 流式；冲突、置信度与降级装配 |
| L6 知识底座 | 文件/JSONL 扫描、Wiki 页面、seed 页面、连接器同步、分块、摘要作业、会话 |
| L7 存储与索引 | SQLite 或 PostgreSQL；同一逻辑 schema（迁移 0001–0010）；网关/用量/配额/计费/连接器表 |
| L8 评测与运维 | Recall@k/MRR、`/health`（含 seed 元数据）、`/metrics`、审计查询、结构化日志 |
| X 横切 | 治理字段、可见性谓词、语言过滤、RFC3339 时间、配额、审计、i18n |

**PostgreSQL 运行路径未验证**（未设置 `LKB_TEST_PG_URL` 时测试 SKIP）；`usage_events` 绑定缺陷已修复，静态 SQL 占位符/bind 守卫通过，仍需实机 PG 验证。

## 3. 核心抽象

### Store、Provider、Policy

- `Store` 由 `SqliteStore` 与 `PostgresStore` 实现，覆盖空间/页面/修订、文档/切片、检索、事件、用户/会话/令牌，以及新增的网关、用量、配额、计费、摘要与会话表。
- `ProviderSet` 持有可选的 `llm`、`embedding`、`judge`、`extractor` 客户端；`[models.llm]` 会物化为网关里的 provider `legacy` / alias `default`。
- `Policy` 生成唯一可见性谓词（租户、空间、时序、语言、ACL），`can_read` 做进程内二次校验。

### 新增服务组件

| 组件 | 职责 |
| --- | --- |
| `gateway` | alias → route 解析、访问策略、优先级/权重/故障切换、OpenAI 兼容调用、usage 事件 |
| `token_quota` | 按 tenant/space/user/role 预留/提交/释放 tokens、requests、cost |
| `billing` | 双分录账本、套餐/订阅/entitlement、用量扣费、余额不足 402 |
| `payment` | `PaymentProvider` 实现（stripe/manual/mock）与 webhook 验签 |
| `access` | 租户/组/空间角色求值与 `allowed_space_ids` |
| `quota` | 存储配额检查/预留/释放/重算 |
| `audit` | best-effort 写入与查询 `audit_events` |
| `summary` | 静态摘要作业与后台 worker |
| `conversation` | 会话/消息持久化与查询改写 |
| `connectors` | 外部源读取、引用/深链、ACL 快照、可选正文索引与同步任务 |
| `logging` / `metrics` / `limits` | JSON/文本事件、Prometheus 计数、令牌桶与并发限制 |
| `i18n` | locale 解析、稳定消息键、格式化辅助 |
| `mcp` / `openapi` | stdio MCP 服务与 OpenAPI 3.1 文档 |

## 4. 写入数据流

```text
lkb seed / import / Wiki 保存
  → frontmatter 或源解析（lang）
  → 规范化路径 p1 → content_hash → doc_id
  → 分块（1000 字符、行号区间、lang）
  → 可选 embedding / 事件抽取
  → 配额预留 → upsert_document（+ 存储重算）→ 摘要作业入队
```

`upsert_document` 语义：同路径同内容 → `skipped`；同路径新内容 → 新 `doc_id`，旧文档 `superseded`；新路径 → 新身份。Wiki 保存走同一编译链路（`wiki/{space}/{lang}/{slug}.md`），并额外写 Page/Revision。种子写入 `author=system:seed` 的修订，绝不覆盖用户修订（`user_modified`）。

### 连接器同步

```text
POST /api/v1/connectors/{id}/sync
  → 202 + job_id（connector_jobs: queued）
  → 后台 run_sync：list_items 分页（100/页，最多 50 页）
  → 每个条目：
       acl_snapshot(item.acl)（缺失/非法 → 空 allow list = deny）
       upsert external_refs(provider, external_id, url/deep link, revision, acl, fetched_at)
       有 content → connectors/{kind}/{safe_id}.md 文档 + 分块 + 可选 embedding → 摘要入队
       无 content → refs_only
  → 汇总 stats {items,indexed,refs_only,failed,errors[]}
  → 任务状态 completed / partial / failed
```

连接器不下载二进制原件、不写回源系统；内容只取条目里的 `content` 文本，深链保存在 `external_refs.url`。

## 5. 查询数据流

```text
CLI / HTML / JSON API / MCP
  → Config + Store + ProviderSet
  → 认证（会话 / Bearer / solo）→ 功能门禁 → 限流
  → Policy(principal, as_of, lang)
  → 有 conversation_id 时经网关做多轮改写
  → 召回：关键词（limit×3）+ dense + event
  → RRF / judge（前 30）→ can_read 二次校验 → 截断
  → 抽取式或网关生成回答 + 引用
  → 持久化 user/assistant 消息；写 usage_events；SSE 事件
```

CLI 与 HTTP 调用同一组库函数；接口层不持有索引句柄，也不自行过滤。SSE 端点在流内校验认证/CSRF，失败以 `event: error` 帧返回。

## 6. 模型网关流程

1. 解析 alias；不存在或 disabled → 503 `model_unavailable`。
2. 应用访问策略（`allow_all` 或 tenant/space/role 列表）；拒绝 → 403 `model_forbidden`。
3. 构造候选路由（provider/model 均 active，跳过熔断），按优先级排序并按权重随机。
4. 经 provider 调 `/chat/completions`；429/5xx/504 打开 30 秒熔断并切下一路由；确定性 4xx 映射为 400/403。
5. 成功后写 `usage_events`，必要时扣本地账本、提交 token 配额，并把响应映射回 alias。

若租户存在钱包或 entitlement，调用前的余额闸门可返回 402 `payment_required`；token 配额返回 429 `quota_exceeded` + `Retry-After: 60`。两者都在调用 provider 之前执行。

## 7. 摘要作业与会话

- 摘要：导入/Wiki/seed 入队 `summary_jobs`；`serve` 的 worker（每 5 秒，批 `summary.batch`）经 alias `summary.model_alias` 生成，失败按 `summary.max_attempts` 重试，仍失败则写抽取式摘要并保留 `status=failed`。
- 会话：`POST /api/v1/query`（及流式版本）读取最近消息，按需改写查询，持久化 user/assistant 消息，并返回/流式输出 `results` 与 `answer`。`external_session_id` 映射外部 Agent 会话。

## 8. 模块地图

| 模块 | 职责 |
| --- | --- |
| `config` | 默认值、环境变量、归一化、校验、脱敏描述 |
| `governance` | 主体、角色、ACL、可见性谓词、语言过滤 |
| `store` | `Store` trait 与两个 SQL 后端 |
| `ingest` / `frontmatter` / `seed` | 导入导出、frontmatter 解析、种子嵌入与幂等 |
| `providers` / `gateway` | 直连 provider 与 alias 网关 |
| `search` / `answer` / `contract` | 召回、融合、回答与输出契约 |
| `access` / `quota` / `audit` | 角色、存储配额、审计 |
| `token_quota` / `billing` / `payment` | 模型配额、账本、支付适配器 |
| `summary` / `conversation` | 静态摘要、会话与改写 |
| `connectors` | 外部源同步、`external_refs`、ACL 快照与深链 |
| `web` / `cli` / `mcp` / `openapi` | HTTP、CLI、MCP、OpenAPI |
| `logging` / `metrics` / `limits` / `i18n` / `error` | 可观测、保护、本地化、错误映射 |

## 9. 不变量

- **无 profile/mode 分支**：差异只在配置、门禁与适配器。
- **治理字段齐全**：文档、切片、事件、页面、修订都带治理字段；`lang` 进入 schema 与召回谓词。
- **单一可见性谓词**：所有召回都走 `Policy`；`can_read` 只做二次校验。
- **doc_id 稳定**：只使用路径与内容哈希；内容变化生成新 ID 并写 `superseded_by`。
- **显式降级**：可选能力缺失写 `degraded[]`；认证与 ACL 失败 fail-closed。
- **单一 Answer 契约**：CLI、HTML、JSON API、MCP 共享同一字段集合。
- **用量与配额**在模型调用前后完成，不写进 provider 客户端内部。
- **连接器只存引用与文本**：同步写 `external_refs`/深链/ACL 快照，只有条目自带 `content` 才建文档；不下载原件、不写回源系统；缺失 ACL 按拒绝处理。

## 10. 双后端差异与已知缺口

| 维度 | SQLite | PostgreSQL |
| --- | --- | --- |
| 全文检索 | FTS5，`bm25()`，分数越小越靠前 | `tsvector` + GIN，`ts_rank_cd`，分数越大越靠前 |
| 占位符 | `?` | `$1`、`$2` … |
| 事件文本索引 | FTS5 trigram | 生成列 `tsv` + GIN |
| 迁移对等 | 0001–0010 | 0001–0010 |
| 运行状态 | 测试集验证过 | **未验证**；静态占位符/bind 守卫通过，仍需实机 PG |

锁与配额串行化都是进程内的（内存互斥锁与计数），多实例部署需要外部分布式协调才能得到精确限额。

## 11. 相关文档

- [02 核心概念](02-concepts.md)
- [05 检索与 AI 回答](05-search-ai.md)
- [07 配置参考](07-configuration.md)
- [09 外部知识源与连接器](09-connectors.md)
