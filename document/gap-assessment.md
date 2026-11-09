# 功能权限与能力缺口评估

> 评估日期：2026-11-08
> 范围：当前 Rust 实现的 HTTP/Wiki/权限能力与目标角色模型之间的差距。
> 证据约定：`路径:行号` 指向当前工作区文件；"缺失"类结论同时给出路由表或 schema 的证据范围。
> 结论：基础认证、Wiki 读写、检索/问答可用，但**授权模型尚未成立**。当前最严重的问题不是"少一个管理页"，而是会话主体固定为 `Admin`、空间范围等于租户全部空间、端点 scope 执行不完整且无审计。

## 1. 结论摘要

| 维度 | 结论 |
| --- | --- |
| 认证 | 登录、会话、Bearer、CSRF、登录限流已有实现；但认证成功后的授权语义不完整 |
| 角色 | 无 owner/admin/user/agent 角色存储；users/sessions 无 role；owner 概念不存在 |
| 空间 | 无 `space_members`；`allowed_spaces` 返回租户下全部空间 |
| scope | `TokenScope` 有 read/write/admin，但 `allows_read` / `allows_admin` 未被调用；只有 `page_edit` / `page_save` 调用 `require_write` |
| 管理面 | 无 `/admin`、无用户/角色/空间/ACL/模型/连接器/审计管理 API；只有 CLI 的用户/令牌操作 |
| AI 能力 | 无每页 AI 摘要端点与缓存；无 SSE 流式；模型配置只有 `lkb.toml` + `config check` |
| 审计 | 无 audit 表、无查询端点、无 actor 归因 |
| Zeta 接口 | 无 OpenAPI、无 MCP、无 webhook、无服务令牌/OAuth2 client credentials |
| Wiki 完整性 | 有读/写/历史；无删除/恢复、无 If-Match 409、无附件、无页面树 |
| 连接器 | 飞书/SharePoint/OneDrive/Confluence/MediaWiki 均未实现；产品文档也明确说明当前没有连接器进程 |
| 企业能力 | SSO/OIDC/SAML/SCIM、多租户 HTTP 路由、WORM、pgvector/ANN 均未实现，且不属于本期 P0 |

**最严重的 3 个安全缺口**：

1. **会话固定 Admin**：浏览器会话认证成功后 `scope: TokenScope::Admin`，且 `allowed_spaces` 返回租户全部空间；普通登录用户在授权层面等同管理员，并可写任意空间页面。证据：`src/web.rs:294-303`、`src/web.rs:307-313`。
2. **没有 RBAC 与空间成员**：users 表无 role，sessions/api_tokens 无角色，schema 无 `space_members`；所有登录用户共享同一空间集合，无法表达 owner/admin/user/agent 的最小权限。证据：`migrations/sqlite/0001_init.sql:123-155`、`src/store/mod.rs:321-350`、`src/web.rs:307-313`。
3. **scope 执行不完整且无审计**：`allows_read` / `allows_admin` 定义后未调用；`/api/search`、`/api/ask`、页面读、历史读均未检查 read scope；只有 `page_edit` / `page_save` 检查 write；无 audit 表，越权读写无法追溯。证据：`src/governance.rs:43-53`、`src/web.rs:483`、`src/web.rs:519`、`src/web.rs:692`、`src/web.rs:788`、`src/web.rs:739`、`src/web.rs:854`、`migrations/sqlite/0001_init.sql:1-158`。

## 2. 目标角色模型

### 2.1 角色定义

| 角色 | 定位 | 关键约束 |
| --- | --- | --- |
| owner | 实例 / 组织所有者 | 分配角色、修改租户设置、转移所有权；不能被 admin 删除或降级；至少保留一个 owner |
| admin | 实例 / 组织管理员 | 管理用户、角色、空间、ACL、连接器、模型、审计；不能删除/降级 owner |
| user | 普通成员 | 在所属空间内读写 Wiki、使用 AI 摘要/问答；不能管理其他用户或实例配置 |
| agent | Bearer 服务身份 | scope 收敛、按空间授权；无浏览器会话；不能管理用户/角色/实例设置 |

### 2.2 权限矩阵

符号：`✔` 允许；`△` 有条件/仅所属范围；`—` 禁止。

| 能力 | owner | admin | user | agent |
| --- | --- | --- | --- | --- |
| Wiki 读 | ✔ | ✔ | ✔（成员空间） | △（`read` scope + 空间） |
| Wiki 写 | ✔ | ✔ | ✔（成员空间） | △（`write` scope + 空间） |
| Wiki 删 / 恢复 | ✔ | ✔ | △（仅自己的页面，软删除） | — |
| Wiki 历史 | ✔ | ✔ | ✔（成员空间） | △（`read` scope） |
| 空间管理（创建/成员） | ✔ | ✔ | — | — |
| 用户与角色 | ✔ | ✔（不能动 owner） | — | — |
| 模型 / API 配置 | ✔ | ✔ | — | — |
| 连接器 | ✔ | ✔ | — | — |
| 审计 | ✔ | ✔ | — | — |
| 导出 | ✔ | ✔ | △（成员空间） | — |
| 租户设置 | ✔ | △（运行项，不含所有权） | — | — |
| 令牌管理 | ✔ | ✔（可管理他人令牌） | △（仅自己的令牌） | — |
| AI 摘要 / 问答 | ✔ | ✔ | ✔（成员空间） | △（`read` scope + 空间） |

补充规则：owner 不能被 admin 删除；最后一个 owner 不能退出；agent 的 scope 只能收敛不能放大；任何 scope 不足都返回 403 并写审计，不降级放行。

## 3. 当前实现 vs 目标

| 能力面 | 当前状态 | 证据 | 目标 |
| --- | --- | --- | --- |
| 登录与会话 | 部分实现：登录、会话、CSRF、登录限流存在；会话 scope 固定 Admin | `src/web.rs:271-304`、`src/web.rs:556-600` | 会话按用户角色派生 scope，不硬编码 |
| Bearer 令牌 | 实现：哈希查找、过期/吊销、租户/用户校验、scope 字段 | `src/web.rs:217-260`、`src/store/mod.rs:341-350` | 保持，并接入端点 scope 执行 |
| 用户存储 | 部分：有 user_id/username/password_hash/display_name/created_at/disabled，无 role | `migrations/sqlite/0001_init.sql:123-133`、`src/store/mod.rs:321-329` | 增加 role 与 owner 保护 |
| 角色 / owner | 缺失：无 role 表/字段，无 owner 语义 | `migrations/sqlite/0001_init.sql:1-158`、`src/store/mod.rs:321-350` | owner/admin/user/agent + 角色分配 |
| 空间成员 | 缺失：无 `space_members`；`allowed_spaces` 返回租户全部空间 | `migrations/sqlite/0001_init.sql:1-158`、`src/web.rs:307-313` | 成员表、邀请/移除、成员空间过滤 |
| 用户组 | 缺失：无 `groups/group_members`；无组角色 | `migrations/sqlite/0001_init.sql:1-158` | 组、组角色、组拥有空间 |
| 空间自助 | 缺失：spaces 只有 owner/visibility；无 `allow_user_creation`/max/默认配额 | `migrations/sqlite/0001_init.sql:6-33` | 自助策略、成员、转移/归档 |
| 存储配额 | 缺失：无 space_quotas/storage_usage/quota_overrides | `migrations/sqlite/0001_init.sql:1-158` | 四级配额、原子预留、413/429 |
| 共享/协作 | 部分：ACL 有，但无空间成员/组/共享链接 | `migrations/sqlite/0001_init.sql:1-158`、`src/web.rs:53-64` | private/internal、成员/组共享 |
| 会话 scope 派生 | 缺失：session 分支固定 `TokenScope::Admin` | `src/web.rs:294-303` | 由 role + space 派生 |
| read scope 执行 | 缺失：`allows_read` 未被调用；读路由未检查 | `src/governance.rs:43-45`、`src/web.rs:376`、`src/web.rs:483`、`src/web.rs:519`、`src/web.rs:692`、`src/web.rs:788` | 所有读端点 fail-closed |
| write scope 执行 | 部分：仅 `page_edit` / `page_save` 调用 `require_write`（不是只有 `page_save`） | `src/web.rs:325-330`、`src/web.rs:739`、`src/web.rs:854` | 所有写/删/管理端点统一执行 |
| admin scope / 端点 | 缺失：`allows_admin` 未被调用；无 admin 路由 | `src/governance.rs:51-53`、`src/web.rs:53-64` | admin API + UI + fail-closed |
| 管理前端 | 缺失：仅根页面列出页面，无用户/角色/空间/ACL/模型/连接器/审计页面 | `src/web.rs:375-419` | 用户端 Wiki + 管理后台 |
| 每页 AI 摘要 | 缺失：无摘要生成/读取端点；frontmatter 的 `summary` 是静态元数据，不是 AI 摘要 | `src/web.rs:53-64`、`src/frontmatter.rs:5-13` | 摘要端点 + 缓存键 + 失效 |
| SSE 流式 | 缺失：无 `text/event-stream` / 流式路由 | `src/web.rs:53-64` | 流式 answer/summary |
| 会话历史 / 统一入口 | 缺失：无 conversations/messages；搜索与问答无统一 query/conversation | `src/web.rs:53-64`、`migrations/sqlite/0001_init.sql:1-158` | `POST /api/v1/query` + conversations/messages |
| 审计 | 缺失：无 audit 表、无审计写入/查询 | `migrations/sqlite/0001_init.sql:1-158` | audit 表 + 查询 + actor |
| 模型配置管理 | 部分：`lkb.toml` + `config check`；无 HTTP API/UI | `src/config.rs:472-475`、`src/cli.rs:98-101` | 模型配置 API/UI + 连通性测试 |
| 模型网关 / 路由 | 缺失：无 alias/provider/routes/fallback；单 provider 配置 | `src/config.rs:472-475`、`src/web.rs:53-64` | alias + provider 池 + 路由/熔断 |
| 计量 / 配额 | 缺失：无 usage_events、quota_policies、预留/提交 | `migrations/sqlite/0001_init.sql:1-158` | usage 表 + 原子配额 |
| 计费 / 钱包 | 缺失：无 wallet/ledger；默认外接 OpenMeter/Lago/Stripe | `migrations/sqlite/0001_init.sql:1-158` | 外接计费引擎 + entitlement |
| Agent 集成（Zeta/PI） | 缺失：无 Zeta/PI 客户端、无 agent usage 回传 | `src/web.rs:53-64` | Zeta 客户端 + usage API/webhook |
| 导出 | 部分：CLI `export`；无 HTTP 导出、无权限细分 | `src/cli.rs:49-54` | 导出 API + scope |
| 令牌管理 | 部分：CLI create/revoke/list；无 HTTP/UI | `src/cli.rs:127-143` | 自助/管理员令牌管理 |
| 种子状态 | 部分：seed 命令与 `/health` 的 seed 元数据存在；无管理页 | `src/cli.rs:187-197`、`src/web.rs:422-442` | 管理后台种子状态/重跑 |
| Wiki 完整性 | 部分：读/写/历史存在；无删除/恢复、If-Match 409、附件、页面树 | `src/web.rs:59-61`、`src/web.rs:847-855`、`migrations/sqlite/0001_init.sql:17-33` | 完整 Wiki 写路径 |
| Zeta 接口 | 缺失：无 OpenAPI/MCP/webhook/服务令牌 | `src/web.rs:53-64` | 见第 9 节契约 |
| 连接器 | 缺失：产品文档明确当前无连接器进程 | `docs/zh/09-connectors.md:93-99` | 飞书/SharePoint/OneDrive/Confluence/MediaWiki |
| SSO / 多租户 / WORM | 缺失：无 OIDC/SAML/SCIM；单租户配置；无 WORM | `src/web.rs:53-64`、`src/config.rs:157`、`migrations/sqlite/0001_init.sql:1-158` | 本期不做 |
| pgvector / ANN | 缺失：检索用进程内余弦，无 pgvector/HNSW | `src/search.rs:431-453` | 本期不做，后续单独评估 |

**文档/实现不一致**：`docs/zh/06-auth-security.md:67` 声称"通过后按 scope 授权：read 只读，write 可读写，admin 全部"，但实现只在 `page_edit` / `page_save` 检查 write，read/admin 未执行；同一文档 `docs/zh/06-auth-security.md:99` 已经承认"没有空间成员表"，与 §5 的完整 scope 表述冲突。`docs/zh/04-wiki.md:45` 也把"浏览器会话用户等价于 admin"写成现状，但这不应是目标模型。

## 4. 差距矩阵

规模：S ≈ 0.5–1.5 人周；M ≈ 1.5–4 人周；L ≈ 4–8 人周。估算非承诺。

| 面 | 目标 | 优先级 | 规模 | 依赖 | 验收测试 |
| --- | --- | --- | --- | --- | --- |
| RBAC 与角色存储 | role 字段/表、owner/admin/user/agent、owner 保护 | P0 | M | schema migration、auth | owner 可分配角色；admin 删除/降级 owner 返回 403；session scope 由 role 派生 |
| 空间成员 | `space_members`、邀请/移除、成员空间过滤 | P0 | M | RBAC | 非成员访问成员空间返回 403；移除后下一次请求即失效 |
| 用户组 | groups/group_members、组拥有空间、组角色映射 | P0 | M | RBAC | group_admin 管理成员；最后 group_admin 保护；组角色映射正确 |
| 空间自助 | allow_user_creation、每用户空间数、默认配额、转移/归档 | P1 | M | 成员/配额 | 策略关闭时 403；超限 429；最后 owner 保护 |
| 存储配额 | 四级 quota、storage_usage、原子预留、override | P1 | M | 存储/事务 | hard 超限 413/429；失败回滚；SQLite/PG 同语义 |
| 共享/协作 | private/internal、成员/组共享、链接分享 P2 | P1/P2 | M | ACL/成员 | internal 只发现元数据；权限即时；跨空间双方可读 |
| 端点 scope 全量执行 | read/write/admin 在每个端点 fail-closed；scope 不足 403+审计 | P0 | S–M | RBAC、audit | 无令牌 401；read 令牌写 403；write 令牌读 200；admin 端点只接受 admin |
| 管理后台最小集 | 用户/角色/空间/成员/ACL/seed 状态；模型/连接器/审计后续 | P0 | M–L | RBAC、space members | `/admin` 对 user 403、admin 200；可完成用户/空间/成员操作 |
| 静态文章摘要 + 缓存 | 摘要 job/端点、键 `doc_id+revision+model+prompt+content_hash`；旧 revision 保留 | P0 | M | model provider、audit | 异步不阻塞摄入；同键命中；改 revision/model/prompt 失效；失败回退抽取式 |
| 会话历史 + 统一入口 | conversations/messages、`POST /api/v1/query`（search/answer/both）、search-only 也写历史 | P0 | M | RBAC、audit | 同一 query/conversation 共享引用；`?q=&conversation=` 可恢复；不存 CoT |
| 审计表与查询 | audit_event 写入、按 actor/action/target 查询、保留策略 | P0 | S–M | RBAC | 每次读/写/管理操作有记录；缺审计时 fail-closed 或告警 |
| Zeta 接口 | OpenAPI 3.1、MCP tools、服务令牌、幂等/限流/审计 actor | P0 | L | RBAC、audit、token | `GET /openapi.json` 3.1；MCP tools/list 与 REST 同内核；幂等重放 |
| 流式 + 多轮 + Zeta 历史映射 | SSE、最近 N 轮改写、`external_session_id`、摘要管理页 | P1 | M | Zeta、summary | stream 以 done 结束；Zeta 会话可读；不复制 CoT |
| 模型网关 / 路由 | P0：Rust 薄网关 alias + 单 provider；P1：LiteLLM/Bifrost sidecar + 池/fallback | P0/P1 | M | secret、gateway | 客户端只看到 alias；provider 故障切池；usage 不丢 |
| 计量 / 配额 | P0：usage_events + hard quota；P1：按用户配额 + 缓存统计 | P0/P1 | M | gateway、RBAC | 调用有 usage；超限 429；缓存命中计入统计 |
| 计费引擎外接 | OpenMeter/Lago/Stripe；不自建钱包/支付 | P2 | S–M | billing engine | entitlement 同步；发票/支付由外部负责 |
| Agent 集成（Zeta/PI） | Zeta 一等客户端；PI 可选；usage API/webhook 回传 | P1 | S–M | Zeta/PI、usage API | Zeta 经 MCP/OpenAPI 调 KB；agent usage 不双计 |
| Wiki 完整性 | 删除/恢复、If-Match 409、附件、页面树 | P1 | L | audit、storage | 旧 revision 可恢复；并发保存 409；附件可回跳；树可导航 |
| 生产 UI（用户端 Wiki） | 阅读、搜索、历史、编辑、语言切换、引用跳转 | P1 | M–L | 管理后台、Wiki 完整性 | 浏览器完成全流程；无越权入口 |
| 连接器（引用/深链） | 飞书/SharePoint/OneDrive/Confluence/MediaWiki | P1 | L | 导入管线、ACL 同步、audit | 只读引用与深链；ACL 以源为准；无源不伪造 |
| 模型配置 API/UI | 配置 CRUD、连通性测试、密钥引用 | P1 | S–M | admin | 保存后 `config check` 等价；密钥不回显 |
| 导出 API + 权限 | 空间导出、scope、审计 | P1 | S | RBAC、audit | 无权限 403；导出内容与 CLI 一致 |
| 令牌管理 API/UI | 自助令牌、管理员吊销、scope 展示 | P1 | S | RBAC、audit | 只显示一次明文；吊销即时生效 |
| webhook | 事件签名、重试、幂等 | P1 | S–M | Zeta、audit | 重放不重复；签名校验失败拒绝 |
| SSO / 多租户 / WORM / 多模态 | OIDC/SAML/SCIM、tenant 路由、WORM、视觉/音频 | 本期不做 | — | 独立项目 | 不进入 P0/P1 验收 |

## 5. 工作量估算

估算口径：1 人周 = 1 名熟悉 Rust/axum/sqlx 的工程师 5 个工作日；不含需求反复、外部连接器账号申请与法务；**估算，非承诺**；分项与合计独立取整，可能有 1 人周差异。

| 工作项 | 人周 |
| --- | --- |
| RBAC + owner 保护 + session scope 派生 | 2–3 |
| `space_members` + 成员过滤 | 1.5–2.5 |
| 端点 scope 全量执行 | 1–2 |
| 管理后台最小集（用户/角色/空间/成员/seed） | 3–5 |
| 静态文章摘要 + 缓存 | 2–3 |
| conversations/messages + 统一 query 入口 | 3–5 |
| 审计表 + 查询 | 1.5–2.5 |
| Zeta OpenAPI + MCP + 服务令牌 | 3–5 |
| 模型网关/计量/配额最小集（Rust 薄网关 + tokenizer + usage + hard quota + usage 页） | 6–10 |
| groups/space_members/三层角色/基础 storage quota | 6–10 |
| P0 安全回归与联调 | 1–2 |
| **P0 合计** | **49–81** |
| Wiki 删除/恢复/If-Match/附件/页面树 | 3–5 |
| 生产 UI（用户端 Wiki） | 3–5 |
| 连接器（5 类，只做引用/深链） | 8–12 |
| 模型配置 API/UI | 1.5–2.5 |
| 导出 API + 权限 | 1–1.5 |
| 令牌管理 API/UI | 1–1.5 |
| webhook | 1.5–2.5 |
| 审计 UI | 1–1.5 |
| LiteLLM/Bifrost sidecar + provider 池/fallback + 缓存统计 + 按用户配额 + 仪表盘 + Zeta/PI 客户端 | 10–16 |
| SSE + 多轮改写 + Zeta 历史映射 + 摘要管理页 | 3–6 |
| 自助配额/用量页/共享/归档/转移所有权 | 6–10 |
| P1 回归与打磨 | 1–2 |
| **P1 合计** | **59–97** |

| 排法 | 总工期（P0） | 总工期（P0+P1） |
| --- | --- | --- |
| 串行（1 人） | 49–81 周 | 108–178 周 |
| 并行（3–4 人做 P0；5–6 人做全量） | 约 12–16 周 | 约 15–20 周 |

并行估算假设接口先冻结、P0 各轨可独立联调、外部连接器凭据不阻塞开发；实际以排期为准。

## 6. 完成度记分卡

记分是能力面粗估，不是代码行数比例，也不是测试覆盖率。

| 能力面 | 完成度 | 依据 |
| --- | --- | --- |
| 登录 / 会话 / CSRF | 60% | 登录、会话、CSRF、限流有实现；无角色派生 |
| RBAC / 角色 | 5% | 无 role/owner；仅 CLI 创建用户 |
| 空间成员 | 10% | 有 spaces，无 space_members；所有空间对登录用户开放 |
| 用户组 | 0% | 无 groups/group_members；无组角色 |
| 空间自助 | 0% | 无创建策略/默认配额/转移/归档 |
| 存储配额 | 0% | 无 space_quotas/storage_usage/预留 |
| 共享协作 | 5% | 有文档 ACL，无空间成员/组/链接分享 |
| 端点 scope 执行 | 20% | 仅 write 在两个页面端点执行；read/admin 未执行 |
| 管理 API / UI | 5% | 无 admin 路由与页面 |
| Wiki 读 / 写 / 历史 | 55% | 基本可用；无删除/恢复、If-Match、附件、页面树 |
| Wiki 删除 / 恢复 / 附件 / 树 | 10% | 无删除/恢复路由、无附件存储、无页面树；pages 表无 status/deleted |
| 检索 / 问答 | 65% | 功能可用；read scope 未执行 |
| 静态摘要 / 动态总结 / SSE | 0% | 无摘要 job/端点/缓存；无 SSE |
| 会话历史 | 0% | 无 conversations/messages；无 Zeta 映射 |
| 统一入口 | 0% | 搜索/问答未共用 query/conversation |
| 审计 | 0% | 无表、无写入、无查询 |
| Zeta OpenAPI / MCP / Webhook | 0% | 均无 |
| 连接器 | 0% | 产品文档明确未实现 |
| 导出 | 30% | 仅 CLI；无 HTTP/权限 |
| 模型配置 | 25% | 仅 toml + config check |
| 模型网关 / 路由 | 0% | 无 alias/provider/routes；仅单 provider 配置 |
| 计量 / 配额 | 0% | 无 usage_events、quota_policies、预留/提交 |
| 计费 / 钱包 | 0% | 无 wallet/ledger；默认应外接计费引擎 |
| Agent 集成（Zeta/PI） | 0% | 无 Zeta/PI 客户端；无 usage 回传接口 |
| 种子 / 健康 | 70% | seed 命令与 health 元数据；无管理页 |
| 生产 UI | 30% | 最小 Wiki 页面；无管理后台 |
| Agent 鉴权/服务令牌 | 15% | 有 Bearer/scope 字段；无 scope 全量执行/设备码/轮换 |
| Agent CLI | 35% | 有 search/ask/page/token CLI；无 auth/mcp/--json 统一 |
| 限流/背压 | 10% | 仅登录限流；无查询级/并发/队列 |
| 结构化日志/指标 | 20% | 有 tracing/本地日志；无 JSON 契约//metrics |
| 运营模式/功能门禁 | 0% | 无 deployment.mode/features |
| 支付/计费 | 0% | 无 PaymentProvider/entitlement；默认外接 |
| i18n | 10% | 文档双语；无消息目录/Intl/API 本地化 |
| 桌面 APP | 0% | 无 Tauri 壳/首启/签名更新 |
| solo+PostgreSQL | 0% | 当前 solo 仅 SQLite 校验；未验证 solo+PG |
| 静态 Web 托管 | 20% | 有最小 HTML 页面；无 web/dist SPA fallback |
| Markdown 笔记 | 0% | 无 vault/文件监听/反向链接/笔记入口 |
| 企业能力（SSO/多租户/WORM） | 0% | 本期不做 |

**综合粗估：约 12%**（简单平均，仅作导航；不按代码行数计算；此前未纳入模型网关/计量/计费/Agent 集成时约 23%）。P0 完成后预计约 50–60%，P1 完成后约 78%；剩余为企业能力与连接器深度。

## 7. 建议的下一冲刺（P0）

目标：先让"角色 → scope → 端点 → 审计"闭环成立，再叠加摘要与 Zeta 接口。所有端点均为目标接口，当前尚未实现；验收命令标注"实现后"。

| P0 项 | 验收命令 / 场景 |
| --- | --- |
| RBAC + owner 保护 | 实现后：`lkb user add alice --role owner`、`lkb user role set bob admin`；admin 删除/降级 owner 返回 403；最后一个 owner 不能退出 |
| 空间成员 | 实现后：`POST /api/v1/admin/spaces/{space}/members` 添加 bob；bob 可读该空间；移除后下一次 `GET /api/v1/search?space=...` 返回 403 或空且不泄露标题 |
| 空间+组+角色+基础配额 | 实现后：user 组创建空间；group_admin 加成员；显式用户角色覆盖组；非成员 private 403；最后 space_owner/group_admin 保护；hard storage 超限 413/429 |
| 端点 scope | 现有路径先用 `curl` 验证：无 Bearer 的 `GET /api/search` 401；read 令牌 `POST /w/{space}/{slug}` 403；write 令牌读 200；实现后 admin 端点只接受 `kb.admin`；scope 不足写审计 |
| 管理后台最小集 | 实现后：user 访问 `/admin` 403；admin 访问 200；能列出用户/角色/空间/成员/seed 状态；不能删除 owner |
| 静态摘要 + 会话/统一入口 | 实现后：导入/编辑后 summary job 异步 ready；`GET /api/v1/pages/{space}/{slug}/summary` 带 revision/cache_key；`POST /api/v1/query` 的 `mode=both` 返回 results+answer；search-only 写 messages；同 query/conversation 引用一致 |
| 审计 | 实现后：`GET /api/v1/admin/audit?actor=alice&action=page.write` 返回记录；每条含 actor/tenant/space/action/target/request_id/result/time；无审计写入时严格模式拒绝写操作 |
| Zeta OpenAPI/MCP | 实现后：`GET /openapi.json` 为 OpenAPI 3.1；MCP `tools/list` 含 `kb_search`/`kb_answer`/`kb_get_page`/`kb_get_page_summary`；服务令牌只返回一次明文；`Idempotency-Key` 重放返回同一结果 |
| 模型网关/计量/配额最小集 | 实现后：客户端只提交 alias；provider 故障不暴露内部名；每次调用写 `usage_events`；hard quota 超限 429 + `Retry-After`；管理员 `/api/v1/admin/usage` 可按 user/space/tenant 聚合 |

## 8. 明确不做

| 不做 | 原因 |
| --- | --- |
| SSO / OIDC / SAML / SCIM | 本期 P0/P1 不引入；先用本地用户 + 角色 + 服务令牌闭环 |
| 多租户 HTTP 路由 / 租户隔离 | 当前是单租户配置；多租户需要独立数据模型、路由、审计与配额，不与 P0 混做 |
| WORM / 不可篡改审计 | P0 只做可查询审计表；WORM 需要存储层与合规流程，另行立项 |
| 多模态（图/音/视） | 本期不扩模态；不进入 P0 |
| pgvector / ANN | 当前是进程内余弦；向量索引是独立性能项目，不进入 P0 |
| 连接器双向同步 | 本期只做引用与深链；不做源系统写回、增量同步、OAuth 授权流程 |
| 评论 / 审批流 / CRDT | Wiki 不是通用 CMS；这些能力不进入 P0 |

## 9. 与 Zeta 的接口契约草案

### 9.1 REST

基础路径 `/api/v1`；JSON；`X-Request-Id` 可选；写操作要求 `Idempotency-Key`；页面更新要求 `If-Match: <current_revision_id>`。

| 方法与路径 | 用途 | 作用域 |
| --- | --- | --- |
| `GET /api/v1/search` | 检索 | `kb.read` |
| `POST /api/v1/answer` | 非流式回答 | `kb.read` |
| `GET /api/v1/answer/stream` | SSE 流式回答 | `kb.read` |
| `GET /api/v1/pages/{space}/{slug}` | 页面正文 | `kb.read` |
| `GET /api/v1/pages/{space}/{slug}/history` | 修订历史 | `kb.read` |
| `GET /api/v1/pages/{space}/{slug}/summary` | 页面摘要（含缓存键） | `kb.read` |
| `POST /api/v1/pages/{space}/{slug}` | 保存页面 | `kb.write` |
| `DELETE /api/v1/pages/{space}/{slug}` | 软删除 | `kb.write` + 空间管理权限 |
| `GET /api/v1/spaces` | 可访问空间 | `kb.read` |
| `GET/POST /api/v1/admin/users` | 用户查询/创建 | `kb.admin` |
| `GET/POST /api/v1/admin/roles` | 角色查询/分配 | `kb.admin` |
| `GET/POST /api/v1/admin/spaces` | 空间管理 | `kb.admin` |
| `GET /api/v1/admin/audit` | 审计查询 | `kb.admin` |
| `GET/PUT /api/v1/admin/models` | 模型配置 | `kb.admin` |
| `GET/POST /api/v1/admin/connectors` | 连接器配置 | `kb.admin` |
| `GET /api/v1/admin/seed/status` | 种子状态 | `kb.admin` |
| `POST /api/v1/tokens` | 创建令牌 | 自助或 `kb.admin` |
| `POST /api/v1/webhooks` | 创建 webhook | `kb.admin` |

### 9.2 MCP tools

MCP 与 REST 共用同一内核与 Answer 契约，不另写检索逻辑。

| Tool | 作用 | 作用域 |
| --- | --- | --- |
| `kb_search` | 检索候选 | `kb.read` |
| `kb_answer` | 返回 Answer JSON | `kb.read` |
| `kb_get_page` | 读取页面 | `kb.read` |
| `kb_get_page_summary` | 读取页面摘要 | `kb.read` |
| `kb_get_page_history` | 读取历史 | `kb.read` |
| `kb_list_spaces` | 列出可访问空间 | `kb.read` |
| `kb_write_page` | 保存页面 | `kb.write` |

不向默认 MCP 暴露 admin 工具；管理操作走 REST + 独立管理员令牌。

### 9.3 认证与授权

| 项 | 契约 |
| --- | --- |
| 服务身份 | 服务令牌 `Authorization: Bearer lkb_svc_...`；数据库只存 SHA-256、scope、tenant/space 绑定、过期/吊销 |
| OAuth2 | 目标支持 `client_credentials`，`POST /oauth/token` 返回短时 access token，audience 为 `lkb` |
| Scope | `kb.read` / `kb.write` / `kb.admin`；admin 隐含 read/write；不默认授予 agent admin |
| 浏览器 | 仍用会话 cookie + CSRF；agent 不使用 cookie |
| 失败 | 无令牌 401；scope 不足 403；scope 解析失败 fail-closed；不把 deny 写成 `degraded[]` |
| 限流 | 按 token/actor/租户限流；429 带 `Retry-After` |

### 9.4 Webhook、版本与幂等

| 项 | 契约 |
| --- | --- |
| 事件 | `page.created`、`page.updated`、`page.deleted`、`summary.completed` |
| 签名 | `X-LKB-Signature: sha256=<hmac>`；`X-LKB-Event`、`X-LKB-Delivery` |
| 投递 | 至少一次；按 `X-LKB-Delivery` 去重；失败指数退避重试 |
| 版本 | REST `/api/v1`；`GET /openapi.json` 为 OpenAPI 3.1；Answer 带 `schema_version`；webhook 带 `event_version` |
| 幂等 | 写请求 `Idempotency-Key`；同键同 payload 返回首次结果；同键不同 payload 409；保留 24 小时（目标） |
| 审计字段 | `event_id`、`tenant_id`、`space_id`、`actor_id`、`actor_type`、`action`、`target_type`、`target_id`、`request_id`、`idempotency_key`、`result`、`status_code`、`ip`、`user_agent`、`at` |

## 10. 证据索引

| 证据 | 位置 |
| --- | --- |
| 路由表仅 8 条，无 admin/summary/MCP/OpenAPI/webhook | `src/web.rs:53-64` |
| 会话 scope 固定 `Admin`，`allowed_spaces` 全租户 | `src/web.rs:294-303`、`src/web.rs:307-313` |
| `TokenScope` read/write/admin；allows_read/admin 未被调用 | `src/governance.rs:43-53` |
| 仅 `page_edit` / `page_save` 调用 `require_write` | `src/web.rs:739`、`src/web.rs:854`、`src/web.rs:325-330` |
| `/api/search`、`/api/ask` 只认证不检查 read | `src/web.rs:483`、`src/web.rs:519` |
| users/sessions/api_tokens 无 role；无 space_members/audit | `migrations/sqlite/0001_init.sql:123-155`、`migrations/sqlite/0001_init.sql:1-158` |
| 用户/会话/令牌结构体无 role | `src/store/mod.rs:321-350` |
| 文档声称按 scope 授权 | `docs/zh/06-auth-security.md:67` |
| 文档承认没有空间成员表 | `docs/zh/06-auth-security.md:99` |
| 连接器未实现 | `docs/zh/09-connectors.md:93-99` |
| 检索为进程内余弦，无 pgvector/ANN | `src/search.rs:431-453` |
| 模型配置只有 toml + config check | `src/config.rs:472-475`、`src/cli.rs:98-101` |

## 11. 模型网关 / 计量 / 配额 / 计费 / Agent 集成

完整设计见 [model-gateway-and-billing.md](model-gateway-and-billing.md)，方向决策见 [ADR 0008](adr/0008-model-gateway-and-billing.md)。

### 11.1 当前状态

| 能力面 | 现状 | 证据 |
| --- | --- | --- |
| 模型配置 | 单 provider 的 `lkb.toml` + `config check`；无 alias/provider 池/路由/fallback | `src/config.rs:472-475`、`src/cli.rs:98-101` |
| 计量 | 无 `usage_events`；无按 user/space/tenant 的 token/cost 聚合 | `migrations/sqlite/0001_init.sql:1-158` |
| 配额 | 无 `quota_policies`、无原子预留/提交/释放、无 429 | `migrations/sqlite/0001_init.sql:1-158` |
| 计费 | 无 wallet/ledger/topup/invoice；默认应外接 OpenMeter/Lago/Stripe | `migrations/sqlite/0001_init.sql:1-158` |
| Agent 集成 | 无 Zeta/PI 客户端、无 agent usage 回传 | `src/web.rs:53-64` |

### 11.2 目标架构

| 平面 | 决策 |
| --- | --- |
| Agent | Zeta 一等客户端（OpenAPI 3.1 / MCP）；PI 可选第二客户端；不嵌入 Rust 内核 |
| 网关 | team/enterprise 用 LiteLLM Proxy（首选）/ Bifrost（备选）sidecar；solo 用 Rust 薄网关 + tokenizer，保持单二进制 |
| 计量/配额 | KB `usage_events` + hard quota 是事实源；网关 virtual key/budget 只作 provider 级防线 |
| 计费 | 外接 OpenMeter/Lago/Stripe Billing；KB 只发 usage 事件与 entitlement 快照，不手写支付/发票 |
| 归属 | agent 编排/工具用 Zeta/PI；provider/tokenizer 用网关；支付/发票用计费引擎；我们写计量、配额、审计、权限 |

### 11.3 增量阶段与估算

| 阶段 | 范围 | 人周 |
| --- | --- | --- |
| P0 | Rust 薄网关 alias + 单 provider + tokenizer + usage_events + hard quota + 管理 usage 页 | 6–10 |
| P1 | LiteLLM/Bifrost sidecar + provider 池/fallback + Zeta/PI 客户端 + 缓存统计 + 按用户配额 + 仪表盘 | 10–16 |
| P2 | 语义缓存 + OpenMeter/Lago/Stripe Billing 接入 + entitlement 同步 | 6–10 |
| **合计** |  | **22–36** |

### 11.4 对记分卡与总工期的影响

- 新增能力面：模型网关/路由、计量/配额、计费/钱包、Agent 集成、静态摘要/会话/统一入口，当前均为 0%；综合完成度从约 23% 降到约 17%。
- P0 从 15–26 调整为 30–51 人周；P1 从 20–34 调整为 39–66 人周；P0+P1 为 69–117 人周；P2 另计 6–10 人周（网关）+ 3–6 人周（摘要）+ 4–8 人周（链接分享/评论/审批）。
- 并行排法：P0 约 8–10 周（3–4 人）；P0+P1 约 11–15 周（5–6 人）。估算非承诺。

### 11.5 明确不手写

| 不做 | 用谁 |
| --- | --- |
| Agent 编排/会话/工具 | Zeta（一等）/ PI（可选） |
| provider 适配/路由/tokenizer | LiteLLM/Bifrost（team/enterprise）；Rust 薄网关（solo） |
| 钱包/发票/支付/催收/退款 | OpenMeter/Lago/Stripe Billing |
| 计量/配额/审计/权限 | 我们（KB 内核） |

## 12. 摘要、会话与统一入口

设计见 [summaries-and-conversations.md](summaries-and-conversations.md)，决策见 [ADR 0009](adr/0009-summaries-and-conversations.md)。

| 能力面 | 当前状态 | 目标 |
| --- | --- | --- |
| 静态文章摘要 | 0%：无 `summary_jobs` / `document_summaries`；frontmatter `summary` 不是 AI 摘要 | revision 后异步生成 tldr/bullets/keywords/entities；失败回退抽取式 |
| 动态总结 / SSE | 0%：无 SSE；现有 `/api/ask` 非流式 | 多轮改写 + Answer/citations + `text/event-stream` |
| 会话历史 | 0%：无 conversations/messages；无 Zeta 映射 | KB canonical 可见历史；`external_session_id` 映射 Zeta |
| 统一入口 | 0%：搜索/问答未共用 query/conversation | `POST /api/v1/query` mode=search/answer/both；搜索-only 写历史 |

增量估算：P0 5–8 人周（静态摘要 + 会话存储 + 统一 query）；P1 4–7 人周（SSE/多轮/Zeta 映射/摘要管理）；P2 3–6 人周（语义缓存/翻译）。纳入后综合完成度约 14%；P0 总量 30–51 人周，P1 39–66 人周，P0+P1 69–117 人周。

## 13. 空间自助、用户组与存储配额

设计见 [spaces-and-permissions.md](spaces-and-permissions.md)，决策见 [ADR 0010](adr/0010-self-service-spaces.md)。

| 能力面 | 当前状态 | 目标 |
| --- | --- | --- |
| 用户组 | 0%：无 groups/group_members | tenant 级组、组角色、组拥有空间 |
| 空间自助 | 0%：无 allow_user_creation/默认配额/转移/归档 | 策略控制、成员、转移、软删除/恢复 |
| 存储配额 | 0%：无 space_quotas/storage_usage | tenant→group→space→user；原子预留；413/429 |
| 共享协作 | 5%：只有文档 ACL | private/internal、成员/组共享、链接分享 P2 |

阶段：P0 6–10 人周（组/成员/三层角色/基础配额）；P1 6–10 人周（自助配额/用量/共享/归档/转移）；P2 4–8 人周（链接分享/评论/审批）。已并入上述 P0/P1/P2 总量。


## 14. Agent 访问、限速与可观测

设计见 [agent-access-and-observability.md](agent-access-and-observability.md)，决策见 [ADR 0011](adr/0011-agent-access-and-observability.md)。当前：Bearer/scope 字段存在但执行不全；无设备码、统一 CLI/MCP、查询级限流、JSON 日志契约、/metrics。P0 8–12 人周（日志/指标/服务令牌/CLI auth/基础限流/滥用测试）；P1 6–9 人周（设备码/轮换/mcp/多实例限流/仪表盘）；P2 4–6 人周（mTLS/OAuth2/异常检测）。

## 15. 运营模式、限速、支付与 i18n

设计见 [deployment-modes-i18n-payments.md](deployment-modes-i18n-payments.md)，决策见 [ADR 0012](adr/0012-deployment-modes-i18n-payments.md)。当前：无 deployment.mode/features；限流仅登录；无 PaymentProvider/entitlement；i18n 仅文档双语。P0 5–8 人周（模式/门禁/限速/i18n zh-en）；P1 8–12 人周（支付适配/套餐/entitlement/发票）；P2 6–10 人周（多语言/税务/RTL/dunning）。


## 16. 桌面 APP 与笔记

设计见 [desktop-app-and-notes.md](desktop-app-and-notes.md)，决策见 [ADR 0013](adr/0013-desktop-app-notes.md)。当前：无桌面壳/首启向导/静态 web dist/笔记 vault；solo 仅允许 SQLite。P0 6–10 人周（Tauri 壳/首启 SQLite-PG/凭据/静态托管）；P1 6–10 人周（笔记入口/vault/编辑器/反向链接/索引）；P2 4–8 人周（安装包/更新/签名/团队发布）。已并入上述 P0/P1/P2 总量。
