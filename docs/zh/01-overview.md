---
title: 产品概览
slug: 01-overview
lang: zh
order: 1
summary: lkb 是同一检索内核驱动的分层 AI 知识库，用 solo、team、enterprise 三档 profile 覆盖个人单机到企业多租户。
---

# 产品概览

## 1. 一句话定位

`lkb`（Layered Knowledge Base，分层知识库）是同一套检索内核驱动的 AI 知识库：同一套数据模型、治理字段、检索流程与回答契约，通过 `solo`、`team`、`enterprise` 三档 profile（配置档位）和 `solo`、`team`、`enterprise_internal`、`enterprise_external` 四种部署模式（deployment mode）提供从个人单机到对外运营的能力。

它不是三个产品，也不是企业版的精简分支。档位差异只出现在配置默认值与适配器装配处；领域逻辑不按档位或模式分支，只读功能门禁（feature gate）。

## 2. 解决什么问题

- 企业架构对个人过重：问一个本地目录的问题，不需要先安装外部数据库、向量服务、缓存与模型服务。
- 个人方案撑不起治理：租户、空间、成员、角色、ACL、有效期、版本与配额如果等到企业阶段再补，需要重建索引并回填无法推断的历史权限。
- 缺组件时行为不明：可选能力缺失必须写进 `degraded[]`，让人类与 Agent 都知道答案缺了什么。
- 卖不出服务：对外运营需要注册、套餐、用量、配额与支付边界；这些能力必须与内核分离，不能写进领域逻辑。

因此：治理字段从第一天写入；文件是事实源、索引可重建；可选能力缺失时显式降级；人类与 Agent 共用同一个 Answer（回答）契约；运行入口与审计、配额、i18n 在装配层。

## 3. 三档 profile 与四种部署模式

| 维度 | solo | team | enterprise |
| --- | --- | --- | --- |
| 存储 | SQLite 单文件 | PostgreSQL | PostgreSQL |
| 部署 | 单机单进程 | 服务 + 数据库 | 服务 + 数据库 |
| 身份 | 默认本机免认证；可启用会话与 Bearer | 本地会话 + Bearer，角色/组/空间成员 | 同 team；多租户靠独立部署 |
| 治理 | 固定租户 `t_local`、固定空间 `s_default`、显式 allow-all | 配置的 `tenant_id`；空间成员与角色 | 配置的 `tenant_id`；多租户靠独立配置或数据库 |
| 自助空间/组 | 启动校验强制关闭 | 默认开启，可关 | 默认开启 |

`deployment.mode` 与 profile 解耦但必须匹配：`solo` 只配 SQLite；`team`/`enterprise_internal`/`enterprise_external` 必须配 PostgreSQL。模式决定 `[features]` 的默认值，并参与启动校验：

| 模式 | signup | billing | connectors | sso | multi_tenant | public_api | mcp | audit | rate_limit | public_share |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `solo` | — | — | — | — | — | — | — | 开 | 开 | — |
| `team` | — | — | 开 | — | — | — | — | 开 | 开 | — |
| `enterprise_internal` | — | — | 开 | 开 | 开 | 开 | 开 | 开 | 开 | — |
| `enterprise_external` | 开 | 开 | 开 | 开 | 开 | 开 | 开 | 开 | 开 | 开 |

必须说清的实现边界：

1. **`billing`、`audit` 与 `connectors` 三个开关当前真正拦截行为。** `signup`、`sso`、`multi_tenant`、`public_api`、`mcp`、`rate_limit`、`public_share` 已进入配置与 `GET /api/v1/features`，但没有路由或领域逻辑读取它们；`rate_limit` 实际由 `[limits]` 无条件执行。连接器 API 读取 `connectors`，关闭时返回 403 `feature_disabled`。
2. **enterprise 当前不是独立实现。** `enterprise` 与 `team` 行为相同；真正的 SSO、多租户路由、配额策略引擎与 WORM 审计未实现。
3. **规模列是设计目标量级，不是实测结论。** 已验证规模是仓库示例语料与 24 篇种子文档。

## 4. 能做什么

- **首次启动即有内容**：`lkb seed` 把内置 24 篇产品文档写入 `space=product`；solo 空库首次 `serve` 会自动 seed，失败只写 warning，不阻塞启动。
- **导入本地知识**：Markdown、TXT、HTML、JSONL；PDF 需要带 `pdf` feature 构建。目录导入跳过隐藏文件、符号链接与 `target`、`node_modules`、`data`、`blobs`、`.lkb`；`--lang` 指定语言。
- **连接器框架**：`filesystem_export`、`jsonl`、`http_json` 三类源可用；同步写入 `external_refs`、深链与 ACL 快照，有 `content` 时索引为文档，不下载原件；飞书/SharePoint/OneDrive/Confluence/MediaWiki 仍是配置校验 stub，返回 `connector_not_configured`。
- **关键词与增强检索**：SQLite FTS5 + `bm25()`；PostgreSQL `tsvector` + `ts_rank_cd`；可选 judge 重排、event-entity 通道与 RRF 融合。统一 `POST /api/v1/query` 支持 `search`、`answer`、`both` 三种模式。
- **带引用的回答与流式输出**：抽取式或 OpenAI 兼容生成式；`/api/v1/answer/stream` 与 `/api/v1/query/stream` 为 SSE；生成失败或引用无法对齐时回退抽取式并写降级。
- **静态摘要**：导入、Wiki 保存与 seed 后异步生成 `document_summaries`；失败有限重试后回退抽取式，并保留 `status=failed` 记录。
- **内置 Wiki**：space / page / revision（按语言分列）、Markdown 编辑、历史与旧版正文、归档/恢复、`If-Match` 并发控制（JSON API）。
- **角色与协作**：租户角色 owner/admin/user、组角色 group_admin/group_member、空间角色 space_owner/space_admin/editor/viewer；组可拥有空间；显式成员优先于组派生。
- **存储配额**：tenant → group → space → user 四级，维度为 storage_bytes/documents/chunks/members/spaces；写入前预留、失败释放；超限 413/429。
- **模型网关与计量**：对外只暴露 alias；provider/provider_model/route 控制面、优先级与故障切换、`usage_events`、按 tenant/space/user/role 的 token 配额（429）。
- **计费（默认关闭）**：本地双分录钱包/账本/套餐/订阅/entitlement；余额不足 402；外部引擎模式只发事件不写本地财务表。
- **会话历史**：`conversations`/`messages`、外部会话绑定（Zeta `external_session_id`）、多轮 query 改写、软删除与恢复、cursor 分页。
- **契约 REST 与 JSON 登录**：`/api/v1/auth/{csrf,login,me,logout}`、页面 JSON CRUD（If-Match/409）、`/api/v1/search|answer|query`。
- **Agent 接入**：`lkb mcp serve` 提供 stdio MCP（kb_search/kb_answer/kb_get_page/kb_get_page_summary/kb_get_page_history/kb_list_spaces/kb_write_page）；`/openapi.json` 为 OpenAPI 3.1（覆盖真实 public/admin 路由的子集）。
- **本地 Web**：`serve` 提供 HTML 页面与 JSON API；React SPA 位于 `web/`，构建产物 `web/dist` 由独立静态服务器或反向代理托管。
- **可观测与保护**：`/health`（含 seed 元数据）、`/metrics`、结构化日志（JSON/text、请求 ID、脱敏）、六维限流（429/503）与审计事件。

最小命令示例（完整平台说明见 [03 快速开始](03-quickstart.md)）：

```bash
target/release/lkb config check
target/release/lkb seed                     # 24 篇文档写入 space=product
target/release/lkb search "产品概览" --lang zh
target/release/lkb ask "没有 LLM 时回答如何生成，引用是什么"
target/release/lkb serve                    # http://127.0.0.1:7331/；solo 空库自动 seed
```

## 5. 不能做什么

当前构建明确不提供或未验证：

- **真实厂商连接器**：框架已实现，`filesystem_export`/`jsonl`/`http_json` 可用；飞书、SharePoint、OneDrive、Confluence、MediaWiki 仍是配置校验 stub，`test`/`sync` 返回 `connector_not_configured`；没有厂商 OAuth、API 抓取、附件下载或 ACL 自动同步。参见 [09 外部知识源与连接器](09-connectors.md)。
- **SSO / 多租户**：没有 OIDC、SAML、SCIM、工作负载身份，也没有 HTTP 层多租户路由；`sso`、`multi_tenant` 开关不改变行为。
- **支付渠道**：微信/支付宝返回 `payment_not_configured`，PayPal 返回 `payment_provider_unavailable`；Stripe 只有 Checkout + HMAC webhook 验签，退款与订阅同步未实现；没有税务计算、发票 PDF 或催收（dunning）。
- **PostgreSQL 运行路径未验证**：集成测试在未设置 `LKB_TEST_PG_URL` 时打印 SKIP。此前 `usage_events` 插入的 22 占位符 / 21 绑定缺陷已修复，并有静态守卫测试扫描 `src/store/postgres.rs` 的字面量 SQL 与 `.bind()` 链（140 条通过）；但本机仍无 PG 运行时，投入生产前仍需实机跑通迁移与全链路。
- **设备码登录**：`lkb auth login --device` 返回 `not_implemented`；`auth` 只读写本地配置文件中的 token。
- **CLI 远端模式**：CLI 是本机优先的（直接打开本地库），`auth` 配置文件里的 `base_url` 仅供显示，不发起 HTTP。
- **多模态**：没有 OCR、ASR、视频关键帧或图片向量。
- **向量索引**：没有 sqlite-vec/pgvector 加速；`hybrid` 的 dense 是进程内余弦。
- **富文本协同**：没有评论、审批、CRDT、链接分享或插件市场。
- **答案缓存**：Answer 的 `cache_hit` 恒为 `false`，语义缓存未实现。
- **Rust 托管 SPA**：`serve` 不提供 `web/dist` 静态文件服务；SPA 需要单独托管并反代 `/api`。
- **前端自动化测试**：`web/` 无单测/E2E，只有 4 个 Node 检查脚本。

## 6. 一个内核的边界

“同一套内核”是可检查的约束：

- 领域代码不出现 `if profile == solo` 或 `if mode == ...`；差异只在配置归一化、校验与适配器装配处。
- `Store`、`Provider`、`Policy` 三组抽象服务两个数据库后端与所有检索模式；新增的 gateway、quota、billing、access、audit 同样以适配器/服务形式接入。
- 治理字段在文档、切片、事件、页面、修订上都存在；召回过滤统一由 `Policy` 生成，语言过滤也是如此。
- Answer 字段集合恒定；降级写入 `degraded[]`，身份与 ACL 失败直接拒绝。

## 7. 相关文档

- [02 核心概念](02-concepts.md)
- [03 快速开始](03-quickstart.md)
- [08 部署与运维](08-deployment.md)
- [10 架构与实现](10-architecture.md)
