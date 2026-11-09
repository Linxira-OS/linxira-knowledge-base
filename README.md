# 分层 AI 知识库（Layered Knowledge Base）

[![CI](https://github.com/Linxira-OS/linxira-knowledge-base/actions/workflows/ci.yml/badge.svg)](https://github.com/Linxira-OS/linxira-knowledge-base/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust stable](https://img.shields.io/badge/rust-stable-blue.svg)](https://www.rust-lang.org/)

**一句话定位**：同一检索内核，通过 solo / team / enterprise 三档 profile（配置档位）提供从个人单机到企业多租户的 AI 知识库能力。

> 仓库：`Linxira-OS/linxira-knowledge-base`；包 / CLI / 二进制名仍为 `lkb`。
> Topics（建议）：`rag` · `knowledge-base` · `wiki` · `rust` · `self-hosted` · `mcp`

## 为什么分层

问题陈述是两头都不合适：

- **企业架构对个人过重。** v1 按企业场景设计：外部数据库、独立向量服务、SSO、多租户隔离、审计链路。个人用户要的只是一个目录、一次启动和本地问答，上述组件把部署与运维成本推高到超过收益本身。
- **个人方案撑不起企业治理。** 纯本地文件检索 + 单机向量库的形态里，tenant / space / ACL / 有效期 / 版本这些治理字段根本不存在。等企业客户出现再补，代价是 schema 迁移、历史数据回填、召回路径分叉，以及无法回答"这条结果为什么被返回"这类审计问题。

因此采用 **一个内核 + 三档 profile**：档位差异只体现为配置与适配器选择，不 fork 代码（[ADR 0001](document/adr/0001-one-core-three-profiles.md)）。配套的四个跨档约束是：治理字段从第一天存在（[ADR 0002](document/adr/0002-governance-fields-day-one.md)）、文件是事实源且索引可重建（[ADR 0003](document/adr/0003-files-are-source-of-truth.md)）、可选能力不可用时显式降级（[ADR 0004](document/adr/0004-explicit-degradation.md)）、人类与 Agent 共用同一 Answer（回答）契约（[ADR 0005](document/adr/0005-single-answer-contract.md)）。

## 三档能力阶梯

| 维度 | solo（个人单机） | team（团队共享） | enterprise（企业多租户） |
| --- | --- | --- | --- |
| 部署形态 | 单机单进程，SQLite 单文件 + 本地目录，无外部服务 | Rust 服务 + PostgreSQL（pgvector），容器化部署 | 多租户集群 + PostgreSQL，按租户隔离与配额，支持数据驻留 |
| 身份 | 本机用户；默认仅 loopback 免认证，可启用本地会话（cookie + CSRF）；allow-all 策略 | OIDC（组织账号），空间成员与角色 | SSO（SAML / OIDC）+ 目录供给 + 服务账号 |
| 检索 | SQLite FTS5 BM25 Top 30 为默认主通道，可选判别模型对 Top 30 重排；hybrid 为可选升级（本地 ONNX 嵌入 + sqlite-vec + RRF）；无 LLM 时走抽取式回答 | PostgreSQL 全文检索 + pgvector hybrid，RRF 融合后重排（Cross-Encoder / 判别模型可选） | 分布式多路并行（关键词 / 向量 / 图谱 / 多模态）→ RRF → 判别模型 + Cross-Encoder + 可选 LLM 择优 |
| 治理 | 治理字段完整存在；ACL 取 allow-all，space 归属、有效期与版本按记录写入；召回过滤照常执行；本地追加日志 | space_id 隔离 + 空间级 ACL，召回阶段过滤；集中日志 | tenant_id 强隔离 + 召回阶段 ACL；WORM 审计与 SIEM 导出 |
| 典型规模 | 10³ 量级文档，单人使用 | 10⁵ 量级文档，10–10² 人 | 10⁷ 量级文档，10³ 人以上 |

规模列为设计目标量级（order of magnitude），不是实测结论。逐项差异与来源见 [document/tier-matrix.md](document/tier-matrix.md)。

## 技术栈

- **Rust 单一二进制**：HTTP 层用 axum，数据访问用 sqlx；不依赖 Python 运行时，也不保留 Python 实现。
- **存储**：solo 使用 SQLite 单文件（FTS5，hybrid 时挂 sqlite-vec）；team / enterprise 使用 PostgreSQL（pgvector）。两者共用同一 Store 抽象，各自带迁移脚本。
- **不提供 SQL Server 或其他第二关系库适配器**；关系库后端只有 SQLite 与 PostgreSQL。

## 文档索引

仓库文档分两类：`document/` 存放内部文档（架构、ADR、路线图，面向维护者与 Agent），`docs/` 存放产品使用文档（`docs/zh` 与 `docs/en` 各 12 篇，并作为 `lkb seed` 的编译期内置种子源）。

- [document/architecture.md](document/architecture.md) — 内核分层、组件边界、数据流
- [document/tier-matrix.md](document/tier-matrix.md) — 三档逐项能力矩阵与差异来源
- [document/knowledge-freshness.md](document/knowledge-freshness.md) — 知识时效、模型台账与外部查找
- [document/knowledge/model-ledger.yaml](document/knowledge/model-ledger.yaml) — 机器可读模型台账
- [document/oss-landscape.md](document/oss-landscape.md) — 外部 Wiki/知识库连接器优先级与许可证红线
- [document/sag-assessment.md](document/sag-assessment.md) — SAG / SAG-Wiki 评估与集成选项
- [document/seed-content.md](document/seed-content.md) — 随包产品文档种子、双语布局与升级契约
- [document/model-gateway-and-billing.md](document/model-gateway-and-billing.md) — 模型网关、计量、配额、计费与 Agent 集成
- [document/summaries-and-conversations.md](document/summaries-and-conversations.md) — 静态摘要、统一搜索/问答入口与会话历史
- [document/spaces-and-permissions.md](document/spaces-and-permissions.md) — 空间自助、用户组、三层角色与存储配额
- [document/agent-access-and-observability.md](document/agent-access-and-observability.md) — Agent 鉴权、结构化日志、并发限流与 CLI
- [document/deployment-modes-i18n-payments.md](document/deployment-modes-i18n-payments.md) — 运营模式、功能门禁、限速、i18n 与支付边界
- [document/desktop-app-and-notes.md](document/desktop-app-and-notes.md) — 桌面 APP、首启、solo+PG、Markdown 笔记与发布包
- [document/product-spec.md](document/product-spec.md) — 统一规范 v1：命名、路径、版本 0.2.z、UI、打包与配置
- [document/roadmap.md](document/roadmap.md) — 阶段划分、出口标准与明确不做的事
- [document/build-policy.md](document/build-policy.md) — 构建门禁与当前实现边界
- 架构决策记录（ADR）：
  - [ADR 0001 一个内核、三档 profile](document/adr/0001-one-core-three-profiles.md)
  - [ADR 0002 治理字段第一天就在 schema 中](document/adr/0002-governance-fields-day-one.md)
  - [ADR 0003 文件是事实源，索引是派生](document/adr/0003-files-are-source-of-truth.md)
  - [ADR 0004 可选能力缺失时显式降级](document/adr/0004-explicit-degradation.md)
  - [ADR 0005 人类与 Agent 共用同一 Answer 契约](document/adr/0005-single-answer-contract.md)
  - [ADR 0006 doc_id 采用内容敏感口径](document/adr/0006-doc-id-identity.md)
  - [ADR 0007 可选 event-entity SQL-RAG 检索通道](document/adr/0007-event-entity-retrieval.md)
  - [ADR 0008 公开模型别名、供应商池与计量/计费方向](document/adr/0008-model-gateway-and-billing.md)
  - [ADR 0009 静态摘要、统一入口与 KB 侧会话历史](document/adr/0009-summaries-and-conversations.md)
  - [ADR 0010 空间自助、用户组与分层权限](document/adr/0010-self-service-spaces.md)
  - [ADR 0011 Agent 访问、结构化日志与并发背压](document/adr/0011-agent-access-and-observability.md)
  - [ADR 0012 部署/运营模式、限速、i18n 与支付边界](document/adr/0012-deployment-modes-i18n-payments.md)
  - [ADR 0013 桌面 APP 与 Markdown 笔记模式](document/adr/0013-desktop-app-notes.md)
  - [ADR 0014 产品统一规范与 0.2.z 版本冻结](document/adr/0014-product-conventions.md)

其余已定取舍（solo 零外部依赖、doc_id 由内容指纹与逻辑路径派生并遵循 [ADR 0006](document/adr/0006-doc-id-identity.md)、保留 v1 的有效取舍如召回期 ACL 与多通道 RRF 融合）记录在架构正文与上述 ADR 中。

## 当前状态

**源码、迁移与测试已落地，并完成 GNU 工具链验证。** 验证记录：默认 `stable-x86_64-pc-windows-msvc` 因 `link.exe` 解析到 Git 的 coreutils `link`（`cl.exe` 缺失）无法链接；改用已安装的 `stable-x86_64-pc-windows-gnu` + MSYS2 gcc 16.1.0 执行 `cargo check --all-targets`、`cargo build --release` 与 `cargo test`，全部通过（115 tests passed；PostgreSQL 集成已通过 WSL PostgreSQL 18.6 实测、不再 SKIP；无 PG 环境时仍 SKIP，详见 `document/wsl-postgres.md`）。发布二进制 `target/release/lkb.exe` 约 28 MB，`target/` 约 13 GB。MSVC 链接器/VS Build Tools 仍缺失，安装需单独确认。

实现要点：

- **单一内核**：`src/lib.rs` 暴露 `config`、`governance`、`store`、`ingest`、`search`、`answer`、`providers`、`web`、`cli`；`src/main.rs` 只调用库入口。
- **Store 抽象**：`Store` trait 同时由 `SqliteStore` 与 `PostgresStore` 实现；逻辑 schema 相同，迁移分别在 `migrations/sqlite/` 与 `migrations/postgres/`。
- **治理字段 day-one**：`tenant_id` / `space_id` / `acl_json` / `valid_from` / `valid_to` / `version` / `superseded_by` 在所有文档与切片上写入；召回 SQL 统一经 `Policy::visibility(principal)` 生成过滤条件，solo 也走该路径。
- **可信反代 Header SSO（P2）**：`[sso] mode=trusted_header` 时仅当 TCP peer IP 命中 `trusted_proxies`（IP/CIDR）才接受 `user_header`/`email_header`/`groups_header`；否则 401 并审计，客户端直传内部身份头一律剥离。可信请求按 email/external_id/username 绑定或自动创建 `source=sso` 用户（unique email / external_id，migration 0011），`group_role_map` 取最高角色、否则 provision 用 `default_role`；disabled 用户拒绝，`allowed_email_domains` 不匹配 403；成功后签发与普通登录相同的 HttpOnly 会话 cookie（轮换、CSRF 仍适用），审计 `sso.login`/`sso.provision`/`sso.denied`；`GET /api/v1/auth/sso/status` 只返回 enabled/mode。启动校验覆盖 mode/header/trusted_proxies CIDR/default_role/group_role_map/allowed_email_domains/session_ttl_hours；trusted_proxies 为空合法但启动打印 `SSO will deny all peers` 警告。OIDC/SAML/SCIM 未实现。
- **外部连接器（P2 收尾）**：migration 0010 新增 `connectors`/`external_refs`/`connector_jobs`；Connector trait（test/list_items/fetch_item/deep_link/acl_snapshot）提供 `filesystem_export`（manifest.json）、`jsonl`、`http_json`（真实 HTTP + Bearer api_key_env）；`feishu/sharepoint/onedrive/confluence/mediawiki` 仅做配置结构与 `connector_not_configured` 占位。Sync 幂等 upsert 引用；manifest 带 content 时按导入管线生成 Document/Chunk/FTS（source_uri=外部 URL、source_role=external、ACL 以源为准，读不到 ACL 按拒绝），不保存原件/二进制。Admin API `GET/POST /api/v1/connectors`、PATCH/DELETE `/{id}`、`POST /{id}/test`、`POST /{id}/sync`（后台 job）、`GET /{id}/jobs`、`GET /api/v1/external-refs`；feature gate connectors + admin scope + 审计；secret_env 只回显名称与是否配置。
- **管理 API 补齐（P2）**：users 创建（argon2、唯一 409）/角色/禁用/软删/重置密码，users 列表含 `last_login_at`（migration 0012，密码与 SSO 登录更新）；admin billing plans CRUD（删除前校验活跃订阅 409 `plan_in_use`）与 subscriptions 分页/状态过滤；provider-models PATCH/DELETE、provider 最小连通性 test（密钥不回显、失败返回 `provider_not_configured`/`provider_unreachable`）；overview（users/spaces/documents/chunks + 24h requests/errors/cost/cache hit rate + recent audit，SQL 聚合）；audit 详情与 action/actor/space/from/to/limit/cursor 过滤；全部 admin scope + 写审计。
- **PostgreSQL 静态 SQL 守卫**：修复 `usage_events` 插入 22 占位符/21 绑定缺陷（补 `rewritten_query`）；`tests/pg_sql_guard.rs` 静态扫描 `src/store/postgres.rs` 的 140 条字面量 SQL 与 `.bind()` 链，要求 `$1..$n` 与绑定数/顺序一致，并有自检用例防止守卫空转。PG 运行时已通过 WSL PostgreSQL 18.6 实测（115 tests 全绿，PG 集成不再 SKIP；本机凭据在 `.lkb/pg.env`，已 gitignore；详见 `document/wsl-postgres.md`）。
- **钱包/账本/套餐/支付（P2a）**：migration 0009 新增 plans/subscriptions/wallets/ledger_entries（双分录、idempotency_key 唯一、只追加）/topups/invoices/payment_events（provider_event_id 幂等）/entitlements；本地钱包支持手工/模拟充值、usage 扣费、退款冲正、余额不足 402 `payment_required`、并发不超扣；套餐 trial→active→past_due→suspended→cancelled 状态机与 entitlement 快照（plan included tokens 优先于钱包）；`BillingEngine` 适配器默认 local，lago/openmeter/stripe_billing 只发事件/读快照不写财务表；PaymentProvider 新增 stripe（Checkout Session + HMAC-SHA256 webhook 验签、时间戳容差）与 manual/mock（微信/支付宝 not_configured）；API `GET/POST /api/v1/billing/{plans,wallet,ledger,invoices,topup,checkout,webhook/{provider},subscribe,subscription,entitlement}`（订阅 GET/PATCH 状态机、tenant admin/owner、审计）；charge.refunded 反向冲正 topup（钱包可转负）；网关在存在钱包/entitlement 的租户上先鉴权后扣费（402/429）；judge 搜索路径也按 billing.engine 设置 local_ledger，外部引擎不本地扣费。
- **契约 REST + 真流式（P1a2）**：`GET /api/v1/search`、`POST /api/v1/answer`、`GET /api/v1/answer/stream`（真 SSE）、页面列表 `GET /api/v1/pages?space&lang&limit&cursor&q`（cursor 分页）、单 revision `GET /api/v1/pages/{space}/{slug}/revisions/{revision_id}`（history 支持 `include_markdown=true`）、JSON 认证 `GET /api/v1/auth/csrf` + `POST /api/v1/auth/login`（Origin+CSRF，HttpOnly 会话 cookie、返回 principal/features）+ `GET /api/v1/auth/me` + `POST /api/v1/auth/logout`、会话恢复 `POST /api/v1/conversations/{id}/restore`；页面 JSON `GET/PUT/POST/DELETE /api/v1/pages/{space}/{slug}`（`If-Match` 必需，冲突 409 `revision_conflict`）、`GET .../history`、`POST .../restore`（space owner）；保存会按 web 表单同一路径重建 Document/Chunk/FTS 索引并占配额（失败释放/500），删除归档同步隐藏索引、恢复重新可见；`/api/v1/query|answer/stream` 通过网关 `stream:true` 解析上游 SSE delta，axum `Body::from_stream` 逐 token 推送；客户端断开释放配额预留并写 `status=cancelled` usage；上游不支持流时回退非流式并写 `stream_fallback` degraded。管理写操作（users/groups/spaces/quotas/models providers/aliases/routes）写 `audit_events`，记录真实 target_id 且创建操作只有一条审计；MCP 另有子进程 stdio smoke，`kb_write_page` 取空间默认 ACL 而非硬编码 allow_all。
- **OpenAPI/MCP/管理面（P1a）**：`GET /openapi.json` 无认证输出 OpenAPI 3.1（覆盖当前全部真实 public/admin 路由与 Answer/Citation/Degraded/Space/User/Quota/Usage/Conversation/Summary schemas；未实装的 search/answer 与页面 JSON CRUD 不写入，避免文档超出路由）；`lkb mcp serve` 提供 stdio JSON-RPC 2.0（initialize/tools/list/tools/call），tools 为 kb_search/kb_answer/kb_get_page/kb_get_page_summary/kb_get_page_history/kb_list_spaces/kb_write_page，复用 REST 同一内核与 token scope，不暴露 admin 工具；admin 补齐 `GET /api/v1/admin/users|roles|groups|spaces|usage/aggregate`（含 group_by=user|space|alias|period|source 与分页）及 users 角色/禁用写入；`/api/v1/query/stream` 逐段推送多个 `token` 事件（meta/results/token/citation/usage/degraded/done）。
- **默认检索**：SQLite 使用 FTS5 + `bm25()`；PostgreSQL 使用 `tsvector` + GIN + `websearch_to_tsquery` + `ts_rank_cd`（不是 BM25）。可选 judge 只对已召回候选重排，不改变候选集成员。
- **检索模式**：`bm25`（默认）、`bm25_judge`、`event`（别名 `event_entity`）、`hybrid`、`auto`。`auto` 在 SQLite 且未配置 extractor 时解析为 `bm25`；SQLite 已配置 extractor 且已有 events 时解析为 `hybrid`；PostgreSQL 解析为 `hybrid`（缺失通道按能力裁剪并写 `degraded[]`）。
- **event-entity 通道**：可选抽取器把 chunk 写成 `events` + `entities` + `event_entities`；查询先选种子实体，再沿共享实体做 1 跳（`retrieval.event_depth = 2` 可开启 2 跳），聚合事件后映射回 chunk。默认关闭，未启用写 `event_not_enabled`，抽取/索引不可用写 `event_extraction_unavailable`，无种子写 `event_no_seed`；event 只提供候选，不改变 Answer 契约。
- **hybrid 的当前实现**：BM25 + dense（仅当显式配置 embedding provider）+ event（仅当 extractor 已配置且有 events）三路 RRF（k=60，权重可配）。默认不引入 sqlite-vec / pgvector 或任何向量服务；未配置或未建向量时以 `degraded[]` 说明并回退关键词。
- **外部模型**：`provider = "jev"` 与其他非 `none` 名称一样按 OpenAI 兼容 `/chat/completions`、`/embeddings` 调用；Jev 端点必须由 OpenAI 兼容网关暴露。`[models.extractor]` 同一套 OpenAI 兼容客户端，默认 `none`；未配置 `api_key_env` 或变量缺失时，能力进入 `degraded[]`，不会静默联网。
- **语言元数据**：`documents` / `chunks` / `pages` / `revisions` / `events` 都带 `lang`；import 从 frontmatter `lang` 或 `--lang` 读取，Wiki 页面按页面 `lang`；`--lang` / `?lang=` 在召回 SQL 的 WHERE 中过滤，不做结果后置筛选。
- **内置种子知识库**：编译期嵌入 `docs/zh` / `docs/en` 各 12 篇；`lkb seed` 写入 `space=product` 的 Page/Revision 并派生 Document/Chunk/FTS。默认 `[seed] enabled/auto=true`，solo 空库首次 `serve` 自动 seed；同一内容幂等跳过，用户编辑过的页面不覆盖，`seed_meta` 记录版本与清单哈希。
- **P0a/P0b 角色与自助空间**：租户角色 owner/admin/user（Agent 由 token kind 表达）；`groups` / `group_members` / `space_members` 三层角色；空间有效角色 = 显式用户成员 > 组派生（group_admin→space_admin、group_member→viewer，settings 可改默认）> owner_type；多组取 max。`/api/v1/spaces|groups|admin/audit` 提供空间/组 CRUD、成员、归档/恢复/转移、usage 与审计查询；写请求要求 scope + 空间角色，session 写请求还需 CSRF header。
- **存储配额**：`space_quotas`（tenant→group→space→user）+ `storage_usage`（source/index/history/reserved/docs/chunks/members）；import/wiki 写入前检查并预留、成功后重算、失败释放；超限返回 413 `space_quota_exceeded`/`tenant_quota_exceeded`（成员/空间数 429）；`quota_overrides` 支持管理员带 reason/expires_at 的临时覆盖。
- **审计**：`audit_events` 记录空间/成员/组/配额/转移/认证/ACL 失败（含 search 二次 ACL 过滤），含 actor/tenant/space/action/result/request_id；`GET /api/v1/admin/audit` 支持 actor/action/space/from/to 过滤，detail 不含正文与密钥。
- **结构化日志**：`LKB_LOG_FORMAT=json|text`、`LKB_LOG_LEVEL`；HTTP 中间件生成/透传 `X-Request-Id`，JSON 字段含 request_id/trace_id/actor/token_id（哈希前缀）/tenant/space/route/method/status/latency_ms/db_ms/queue_ms/degraded_codes/error_code/concurrency/ip_hash/user_agent；query 只记 SHA-256 与长度，正文/密钥/Authorization 不落日志；日志写 stderr，与 audit 分离。
- **限流/背压**：`[limits]` 配置 instance/IP/user/token/tenant/space 的 QPS、并发、每日请求上限；令牌桶 + 进程内并发计数，超限 429 `rate_limited` + Retry-After，并发/队列满 503 `queue_full`；`/metrics` 输出 Prometheus 文本（低基数）；P0 为单进程计数，多实例共享计数留待后续。
- **P0 缺口修复**：配额检查/预留/写入/重算用进程写锁串行化，预留字节原子累加；`members` 用量展开直接成员 + 组（含 group-owned 空间）去重；search 二次 ACL 过滤写 `acl.filtered` 审计。
- **部署模式/功能门禁**：`[deployment] mode = solo|team|enterprise_internal|enterprise_external` 与 profile 解耦并校验（solo→sqlite、team/enterprise→postgres；enterprise_external→profile enterprise 且允许 signup/billing）；`[features]` 十个开关按 mode 给默认值、可显式覆盖；禁用功能返回 403 `feature_disabled`，billing 路由在未配置支付时返回 503 `payment_not_configured`。领域代码只读 feature gate，不按 mode 分支。
- **i18n**：`src/i18n.rs` 实现 Accept-Language → 用户/租户默认 → 配置默认 → en 的回退；zh-CN/en 消息键稳定；API 错误返回稳定 code + 本地化 message，`Content-Language` 透传；日志保持英文；提供数字/日期/货币/复数格式化函数；配置 `[i18n] default_locale/supported_locales/tenant_locale`，上限预留 28 locale。
- **模型网关/usage/quota**：Answer 生成式 LLM 调用统一走网关 alias `default`（不再直接调用 `ProviderSet.llm`），Answer 返回 model/usage；调用前 token 配额预留、成功后按 provider usage 提交实际 used、失败释放；`providers` / `provider_models` / `model_aliases` / `routes` 控制面；对外只暴露 alias，provider/base_url/upstream 仅 admin 可见；按 alias→访问策略→priority/weight/health/circuit 路由，429/5xx/超时切下一路由，400/401/403 不盲目重试；`usage_events` 记录 provider/estimated token、成本、延迟、状态；`token_quotas`/`quota_counters`/`quota_reservations` 调用前估算预留、hard 超限 429 `quota_exceeded`+Retry-After、失败释放。`[models.llm]` 自动物化为 `default` alias/provider/route，保留旧回答路径。
- **静态摘要/会话/统一入口**：`document_summaries` + `summary_jobs` 异步摘要（导入/Wiki/seed revision 入队，网关 alias，provider usage 写 `source=summary`；失败按 `summary.max_attempts`/`retry_backoff_ms` 有限重试，全部失败写 `status=failed` 同时保留抽取式 fallback，不静默 ready；同 doc/revision/model/prompt 幂等）；`lkb summary run/list` 可视化处理与查看队列；`GET /api/v1/pages/{space}/{slug}/summary`；`conversations`/`messages` 与 `POST /api/v1/query`（search/answer/both，共享 conversation、写 user+assistant 消息，`conversation_id` 时用最近 N 条消息经网关 alias 改写 query，失败回退原 query 并写 `degraded[]`，消息记录 `rewritten_query`/`conversation_context_hash`）；`POST /api/v1/query/stream` 输出 SSE（meta/results/token/citation/usage/done）；会话 cursor 分页与 `POST /api/v1/conversations/{id}/external-session` 绑定/查询；judge 经网关 alias（`retrieval.judge_alias`）并写 `source=judge` usage；会话 owner/space 隔离、软删除；Answer 增加 conversation_id/message_id/summary_kind/summary_source。
- **支付边界**：`PaymentProvider` trait（create_checkout/verify_webhook/refund/subscription_sync）+ `PaymentConfig(provider/currency/test|live/secret_env)`；本期无任何网关实现，未配置统一 `payment_not_configured`，配置了但无适配器为 `payment_provider_unavailable`；不存卡号、不写支付状态机。
- **Agent CLI**：`lkb auth login --token` / `auth status` / `auth logout`（`--device` 返回 not_implemented）；本地凭据在 `<config_dir>/config.toml`（即 `%APPDATA%\lkb\config.toml` / `$XDG_CONFIG_HOME/lkb/config.toml`，Unix 0600，Windows icacls 尽力收紧），`LKB_CONFIG_DIR` 可覆盖；`LKB_TOKEN`、`LKB_API_TOKEN`；`LKB_CONFIG_FILE` 选择主配置 `lkb.toml`；`--json`。
- **降级**：可选能力缺失或失败写入 `degraded[]`；身份/ACL 失败 fail-closed，不进入降级放行。
- **Answer 契约**：`answer`、`citations[{doc_id,chunk_id,path,line_start,line_end,quote,source_url?}]`、`confidence`、`conflicts[]`、`degraded[]`、`knowledge_as_of`、`mode`；生成式答案额外带 `model`（公开 alias）、`usage`（input/output/cached tokens、cost、currency、token_source）和 `cache_hit`。
- **认证**：浏览器使用 HttpOnly + SameSite=Lax（非 loopback 自动 Secure）会话 cookie，服务端只存 session id 的 sha256；Agent/CLI 使用 `Authorization: Bearer`，库中只存 token 的 sha256 + scope + 吊销状态。
- **doc_id 口径**：`sha256(规范化相对路径 + "\0" + 内容哈希)` 的前 16 字节 hex（32 个小写 hex 字符，无前缀）；同一路径同一内容在重建、导出/导入与跨档迁移中稳定；内容变化生成新 doc_id，旧记录 `superseded_by` 指向新 id，`as_of` 仍可解析旧 id。该口径由 [ADR 0006](document/adr/0006-doc-id-identity.md) 冻结，architecture.md §5.8 已同步。

## 快速开始（已构建验证）

> 构建与测试已在 GNU 工具链上执行；默认 MSVC 工具链仍缺链接器。构建前仍应遵守 [document/build-policy.md](document/build-policy.md)。

前提：Rust stable（>= 1.82）与可用的 Windows C 编译器/链接器（MSVC Build Tools + Windows SDK，或 MinGW）。bundled SQLite 仍需要 C 工具链。

Windows 下二进制为 `target\\release\\lkb.exe`；PowerShell 设置环境变量用 `$env:LKB_SQLITE_PATH = "..."`，与下例的 `export` 写法等价。本机默认 MSVC 工具链缺少可用链接器，已验证的构建命令是 `cargo +stable-x86_64-pc-windows-gnu build --release`；安装 MSVC Build Tools 后可直接使用默认 toolchain。

```bash
# 本机已验证的 GNU 链接器路径；MSVC Build Tools 可用时可改用 cargo build --release
cargo +stable-x86_64-pc-windows-gnu build --release

# solo：默认 SQLite、auth=none、只绑定 127.0.0.1
target/release/lkb config check
target/release/lkb import examples
target/release/lkb seed                    # 24 篇产品文档写入 space=product
target/release/lkb search "产品概览" --lang zh
target/release/lkb search "Overview" --lang en
target/release/lkb search "doc_id 如何派生"
target/release/lkb search "doc_id 如何派生" --mode bm25_judge
target/release/lkb search "doc_id 如何派生" --mode event   # 未配置 extractor 时写 event_not_enabled 并回退 BM25
target/release/lkb ask "没有 LLM 时回答如何生成，引用是什么"
target/release/lkb eval examples/questions.jsonl --k 5
target/release/lkb eval examples/questions.jsonl --k 5 --mode event_entity
target/release/lkb serve          # http://127.0.0.1:7331/；空库首次启动自动 seed
```

默认路径按 OS 目录规范解析（`src/paths.rs`；任何路径不含品牌名）：

| 平台 | 配置 | 数据 | 日志 |
| --- | --- | --- | --- |
| Windows | `%APPDATA%\lkb\lkb.toml` | `%LOCALAPPDATA%\lkb\data`（SQLite `…\data\lkb.sqlite`） | `%LOCALAPPDATA%\lkb\logs` |
| Windows portable | `.\lkb-data\lkb.toml` | `.\lkb-data\data`（SQLite `.\lkb-data\data\lkb.sqlite`） | `.\lkb-data\logs` |
| Linux | `$XDG_CONFIG_HOME/lkb/lkb.toml`（fallback `~/.config/lkb/lkb.toml`） | `$XDG_DATA_HOME/lkb`（fallback `~/.local/share/lkb`） | `$XDG_STATE_HOME/lkb/logs`（fallback `~/.local/state/lkb/logs`） |
| macOS | `~/Library/Application Support/lkb/lkb.toml` | `~/Library/Application Support/lkb` | `~/Library/Logs/lkb` |

- 目录覆盖：`LKB_CONFIG_DIR` / `LKB_DATA_DIR` / `LKB_LOG_DIR` > portable（exe 旁存在 `portable.flag` 时整目录 `.\lkb-data`）> OS 默认。
- 配置文件：显式 `--config` > `LKB_CONFIG_FILE` > 当前目录 `lkb.toml`（存在时，便于仓库内开发）> OS 默认 `<config_dir>/lkb.toml`；SQLite 路径：`[storage] sqlite_path` / `LKB_SQLITE_PATH` > `<data_dir>/lkb.sqlite`。
- `lkb config check` 打印解析后的实际路径（`config.file`、`paths.*`）与生效配置并对 DSN/密钥打码；`lkb config init` 在解析到的位置写入最小 solo 配置（已存在时不覆盖）；`lkb --help` 尾部同样列出默认路径。

最小 `lkb.toml`（solo 默认值，可以省略）：

```toml
profile = "solo"

[server]
bind = "127.0.0.1:7331"

[auth]
mode = "none"
cookie_name = "lkb_session"
cookie_secure = false
session_ttl_hours = 168
csrf = true

[storage]
backend = "sqlite"
# sqlite_path = "<data_dir>/lkb.sqlite"   # 省略时使用 OS 默认数据目录；LKB_SQLITE_PATH 可覆盖
url_env = "LKB_DATABASE_URL"
tenant_id = "t_local"
default_space = "s_default"

[spaces]
allow_user_creation = true   # solo 启动校验会强制 false
max_per_user = 10

[groups]
allow_user_creation = true   # solo 启动校验会强制 false

[deployment]
mode = "solo"          # solo | team | enterprise_internal | enterprise_external
commercial = false

[features]
# signup = false
# billing = false
# connectors = false
# sso = false
# multi_tenant = false
# public_api = false
# mcp = false
# audit = true
# rate_limit = true

[i18n]
default_locale = "zh-CN"
supported_locales = ["zh-CN", "en"]
# tenant_locale = "zh-CN"

[payment]
provider = "none"
currency = "USD"
mode = "test"
# secret_env = "LKB_PAYMENT_SECRET"

[limits]
qps_instance = 1000
qps_ip = 100
qps_user = 500
qps_token = 200
qps_tenant = 1000
qps_space = 500
max_concurrency_instance = 256
max_queue = 512
daily_requests_token = 1000000
retry_after_seconds = 1
max_query_chars = 8192
max_top_k = 100

[summary]
enabled = true
auto = true            # 导入/Wiki revision 自动入队
model_alias = "default"
max_attempts = 3       # 失败重试上限，超过写 status=failed + 抽取式 fallback
retry_backoff_ms = 1000

[query]
rewrite_enabled = true
rewrite_alias = "default"
context_messages = 6   # 多轮改写使用的最近消息数

[seed]
enabled = true
auto = true            # solo 空库首次 serve 自动 seed；非空库不追加
space = "product"

[retrieval]
mode = "auto"          # bm25 | bm25_judge | event | event_entity | hybrid | auto
judge_alias = "default" # judge 经网关该 alias 调用，usage source=judge
event_depth = 1        # 1 or 2
event_max_seeds = 10
event_max_events = 50
event_timeout_ms = 800
event_text_weight = 0.25
rrf_bm25_weight = 1.0
rrf_dense_weight = 1.0
rrf_event_weight = 1.0

# 可选：开启后 import 才会抽取 events/entities；默认关闭、零网络。
[models.extractor]
provider = "none"      # OpenAI-compatible; Jev must be exposed via gateway
# base_url = "https://api.example/v1"
# model = "extractor-model"
# api_key_env = "LKB_EXTRACTOR_API_KEY"
timeout_ms = 15000
max_retries = 2
```

team / enterprise 示例：

```toml
profile = "team"

[server]
bind = "0.0.0.0:7331"

[auth]
mode = "session"
cookie_name = "lkb_session"
cookie_secure = true
session_ttl_hours = 168
csrf = true

[storage]
backend = "postgres"
url_env = "LKB_DATABASE_URL"
tenant_id = "t1"
default_space = "s_default"

[retrieval]
mode = "auto"
event_depth = 1
```

```bash
export LKB_DATABASE_URL='postgres://user:pass@localhost/lkb'
LKB_USER_PASSWORD='change-me' target/release/lkb user add alice --role user
target/release/lkb user role set alice admin
target/release/lkb auth login --token "$LKB_TOKEN" --json
target/release/lkb auth status --json
target/release/lkb auth logout --json
target/release/lkb features --json
target/release/lkb models list
target/release/lkb usage --json
target/release/lkb quota --json
target/release/lkb billing checkout --plan pro --amount-minor 1000
target/release/lkb group create eng --description engineering
target/release/lkb group list
target/release/lkb space create --name "Team Space"
target/release/lkb space list
target/release/lkb space usage TEAM_SPACE_ID
target/release/lkb import examples --space s_default --acl-json '{"mode":"allow_all","users":[],"groups":[],"version":1}'
target/release/lkb serve
```

`/health` 在正常字段之外返回 `seed`（`seed_version`、`applied_at`、`page_count`、`revision_count`）；种子失败只写 warning，不阻塞启动。

安全要点：

- solo 的 `auth.mode = "none"` 只允许回环地址；非 loopback + `none` 会在启动时被拒绝。
- 密钥只从环境变量读取（`models.*.api_key_env` 指定变量名），不会写入配置或日志；`config check` 只显示变量是否已设置。
- team / enterprise 的 CLI 查询需要 `--token` 或 `LKB_API_TOKEN`；token 只在创建时显示一次，数据库中只存 sha256。
- PostgreSQL 集成测试读取 `LKB_TEST_PG_URL`（本机取值见 `.lkb/pg.env`，已 gitignore）；本机已通过 WSL PostgreSQL 18.6 实测（115 tests）；未设置时仍打印 SKIP 原因并跳过，不会伪装通过。

## 发布

发布由 tag 驱动：推送与三处版本号一致的 `v<version>` tag 后，[Release 工作流](.github/workflows/release.yml) 会构建并发布两套形态——**个人用户版（solo）**原生归档（Linux x86_64 / Windows x64 / macOS x86_64+arm64，均为同一个二进制）与 **企业部署版（enterprise）**镜像 `ghcr.io/linxira-os/linxira-knowledge-base` 及 compose/nginx bundle，并附 `SHA256SUMS`、SPDX SBOM 与构建来源证明（attestation）。

- Release 页面（首个正式 release 发布后在此查看）：https://github.com/Linxira-OS/linxira-knowledge-base/releases
- 版本规则、资产清单、dry-run、校验与回滚：[document/release-process.md](document/release-process.md)
- 部署资产（Dockerfile / compose / 示例配置）：[deploy/README.md](deploy/README.md)
- 问题反馈：https://github.com/Linxira-OS/linxira-knowledge-base/issues ；安全漏洞请走私密通道：https://github.com/Linxira-OS/linxira-knowledge-base/security/advisories/new

同一二进制同时支持 SQLite 与 PostgreSQL（配置切换），因此没有单独的 SQLite / PostgreSQL 包；桌面安装包（Tauri）尚未提供（P1）。

## 贡献约定

1. **文档先于代码。** 任何实现变更，先落到 architecture.md、tier-matrix.md 或 ADR，再改代码；文档与实现不一致视为缺陷。
2. **决策必须落 ADR。** 影响跨档行为、数据模型或接口契约的决定，以新增 ADR 记录。状态为 Accepted 的 ADR 不静默改写；被取代时标注 Superseded 并链接新 ADR。
3. **档位差异只允许出现在配置与适配器层。** 领域逻辑中出现按档位分支的代码，视为设计缺陷（ADR 0001）。
4. **跨档不变量不可省略。** 治理字段、degraded[] 汇报、可回跳引用在每一档都必须存在（ADR 0002 / 0004 / 0003）。
5. **ADR 之间不重复论述。** 用链接表达依赖，避免同一理由在多处维护。
6. **构建门禁。** 任何 `cargo build`、`cargo test` 或清理缓存前，必须先获得用户确认；大改动优先清理 `target/`，清理前报告其体积。
