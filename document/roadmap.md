# 路线图（Roadmap）

阶段按依赖串行推进：后一阶段复用前一阶段的同一内核与契约，不重写。每个阶段给出**目标、交付物、可执行出口标准、明确不做**。出口标准一律写成可执行的验证动作，而不是"完成度良好"这类判断。

**当前状态**：P0 文档已定稿；P1（Rust 实现）进行中，尚未编译验证。

| 阶段 | 主题 | 依赖 | 主要风险 |
| --- | --- | --- | --- |
| P0 | 架构定稿 | — | 决策悬空，导致后续阶段反复返工 |
| P1 | solo 可运行闭环（Wiki / 认证 / 导入导出） | P0 | 零外部依赖约束被实现便利性侵蚀 |
| P2 | Agent 接口与评测回归 | P1 | 通道各自实现，契约分叉 |
| P3 | team（pgvector / OIDC / 共享空间 / 集中审计） | P2 | 权限过滤退化为结果后置剔除 |
| P4 | enterprise（多租户 / SSO / 审计 / 图谱与多模态） | P3 | 规模与治理同时引入，问题无法归因 |

## 跨阶段不变量

以下约束在任何阶段都不得被局部优化掉；违反即视为回归：

- 档位差异只出现在配置与适配器层，领域逻辑无档位分支（ADR 0001）。
- 治理字段在每一档都写入，过滤发生在召回阶段（ADR 0002）。
- doc_id 派生规则稳定，索引可从事实源重建（ADR 0003）。
- 可选能力缺失时返回 `degraded[]`，安全能力 fail-closed（ADR 0004）。
- 人类与 Agent 共用同一 Answer 契约，三通道共用同一内核（ADR 0005）。
- 实现为 Rust 单一二进制（axum + sqlx），无 Python 运行时；solo 用 SQLite，team / enterprise 用 PostgreSQL；不提供 SQL Server 适配器。

---

## P0 架构定稿

**目标**：冻结内核边界、三档 profile 契约、治理字段、Answer 契约与阶段出口，使后续实现不再需要重新讨论架构。

**交付物**

- `document/architecture.md`：内核分层、端口与适配器、摄入与检索数据流、Answer 契约字段。
- `document/tier-matrix.md`：三档逐项能力矩阵与差异来源。
- ADR 0001–0005（`document/adr/`）。
- `README.md` 与 `document/roadmap.md`。

**可执行出口标准**

- 已定决策逐条可在文档中定位：对"一个内核三档、治理字段、文件为事实源、显式降级、单一 Answer 契约、solo 零外部依赖、doc_id 稳定、保留 v1 取舍"八项，每项至少有一处明确归属，无一项只能靠口头解释。
- `tier-matrix.md` 的每个单元格都有取值，不存在"待定""视情况"之类的空项。
- 所有 ADR 状态为 Accepted，文档间链接无悬空目标。
- 文档中不出现无法执行的安装/运行命令，不出现未确定的版本号或接口路径。
- 对同一问题（例如"ACL 在什么阶段过滤"）只能得到一个答案，文档间无相互矛盾的表述。

**明确不做**

- P0 只产出文档，实现属于 P1；不产生提交，不配置仓库身份与远程。
- 文档不锁定依赖版本号，不承诺性能指标。
- 不做部署拓扑图之外的容量规划，不做基准测试。

---

## P1 solo 可运行闭环

**目标**：用 Rust 单一二进制在单机上跑通"摄入 Markdown / PDF → 检索（默认 BM25 主通道，hybrid 可升级）→ 带引用回答 → CLI / 本地 Web"，并具备最小 Wiki、认证与导入/导出；默认路径零外部服务依赖，外部模型仅在显式配置后启用。

**交付物**

- 运行时与存储：Rust 单一二进制（axum + sqlx）；solo 使用 SQLite 单文件；无 Python 运行时。
- Store 抽象 + SQLite 与 PostgreSQL 两个后端、两套迁移脚本；PG 集成测试用 LKB_TEST_PG_URL，未设置则跳过并标注未验证。
- 摄入管线：Markdown / PDF 解析、doc_id 派生、治理字段以 solo 取值写入（ACL allow-all）。
- 索引：SQLite FTS5 BM25 为默认主通道；hybrid 为可选升级（本地 ONNX 嵌入 + sqlite-vec + RRF）。索引可从源文件全量重建。
- 回答：无 LLM 时为抽取式回答，带可回跳引用；有 LLM 时为生成式，引用集合仍指向源文件。
- `degraded[]` 上报：未启用（not_enabled）与不可用（unavailable / timeout）分别声明，不混淆两者。
- 外置模型配置：LLM / embedding / judge 三类端点，字段为 base_url、model、api_key_env、timeout、retry；密钥只经环境变量读取，不写入配置文件。
- 接口：CLI 与本地 Web，两者都调用同一内核。
- 最小 Wiki：spaces / pages / revisions；编辑页面即产生新 revision，旧 revision 保留。
- 种子内容：`docs/zh` + `docs/en` 双语产品文档，`lkb seed` 或 `lkb init --seed` 导入 `space=product`（solo allow-all）；Page/Revision 标记 `author=system:seed`；幂等、离线、不覆盖用户 Revision；`seed_version` 写入 `seed_meta`（见 [seed-content.md](seed-content.md)）。
- 语言元数据：Document / Chunk 带 `lang`；检索支持 `lang` 过滤；UI 语言切换只改展示与默认过滤，不复制索引。
- 静态摘要：转写/导入/Wiki revision 保存后写 `summary_jobs`，异步生成 `document_summaries`（tldr/bullets/keywords/entities/lang/source_revision）；不阻塞摄入；失败回退抽取式。
- 会话与统一入口：`conversations` / `messages` 存用户可见历史；`POST /api/v1/query` 支持 `mode=search|answer|both`，Wiki 顶栏与右侧面板共用 query/conversation；search-only 也写历史（见 [summaries-and-conversations.md](summaries-and-conversations.md)）。
- 认证：浏览器会话使用 HttpOnly cookie + CSRF（跨站请求伪造）校验；Agent 使用 Bearer token；solo 默认只监听 loopback 且免认证，绑定非 loopback 地址且未启用认证时拒绝启动。
- 导入 / 导出：目录与 JSONL 两种格式，doc_id 跨导入导出稳定；导入产出计数报告（imported / skipped / failed）。
- 本地日志：查询与操作的追加日志（默认 30 天，无防篡改）。
- 评测素材：20 篇文档 + 20 条问题，每条标注正确文档与位置（Markdown 行号、PDF 页码）。

**可执行出口标准**

1. **Recall@5 达标**：对 20 条问题逐一执行检索，正确答案所在片段出现在前 5 条召回结果中的比例 ≥ 0.8（即至少 16 条）。评分按标注位置比对，不靠人工印象。
2. **引用可回跳**：20 条回答中的每一条引用都能解析为"源文件 + 行号/页码"，且该位置的文本包含标注答案片段；通过率 20/20。
3. **断网可运行**：默认配置（未配置外部模型端点）下，断网完成摄入、检索、回答全流程，且无外部请求（本地服务访问除外）。
4. **可重建**：删除索引目录后全量重建，重建前后 doc_id 集合完全一致，Recall@5 下降不超过 0.05。
5. **能力声明可见**：solo 默认配置（仅 FTS5、无 LLM、无向量）下，标准 1–4 全部通过，能力状态如实标注未启用项（不得假装语义检索可用）；启用 hybrid 后禁用向量通道，仍返回结果且 `degraded[]` 标明该通道不可用。任何配置下不得出现"无声明即切换实现"。
6. **摄入幂等**：对同一目录重复摄入，索引条目数与 doc_id 集合不变。
7. **未认证被拒**：启用认证后，不带 cookie 或 Bearer 的检索请求返回 401；携带有效 Bearer 的 Agent 请求成功；过期或伪造的 Bearer 返回 401。
8. **CSRF 缺失被拒**：携带会话 cookie 的写请求缺少 CSRF token 时返回 403；补上正确 token 的同类请求成功；两个响应码由同一测试脚本断言。
9. **loopback 保护**：默认配置只绑定 127.0.0.1 且免认证可用；把绑定地址改为非 loopback 地址且未启用认证时，进程拒绝启动（非零退出并输出明确错误）。
10. **模型配置生效**：把 base_url 指向本地 stub 服务后，调用使用配置中的 model，stub 收到请求；删除 api_key_env 指向的环境变量后，该能力进入 `degraded[]` 且进程不崩溃；把 timeout 设为 200 ms、stub 延迟 1 s 时，调用按 timeout 失败，stub 记录的尝试次数等于 1 + retry。
11. **Wiki 修订**：编辑同一页面一次后，revisions 数量 +1，旧 revision 内容不变并仍可按原 doc_id 读取；新 revision 的 version 递增且可被检索。
12. **导入导出计数**：把 20 篇文档导出为目录与 JSONL 后导入空库，报告 imported=20、skipped=0、failed=0，doc_id 集合与源一致；对同一目录重复导入，报告 imported=0、skipped=20、failed=0，doc_id 集合不变。
13. **单二进制启动**：在干净目录中只放可执行文件与配置即可启动并完成一次检索；启动与检索期间不调用 Python / Node 等外部解释器（以进程树核对）。
14. **PG 后端可验证**：设置 LKB_TEST_PG_URL 时，PostgreSQL 集成测试通过且与 SQLite 跑同一组用例；未设置时测试跳过，并在报告中明确标注"未验证"，不得记作通过。
15. **种子首启**：干净数据目录首次启动并执行 `lkb seed` / `lkb init --seed` 后，`product` space 中 `zh` / `en` 各不少于 10 篇，slug 对齐；全程离线。
16. **语言检索**：中文查询命中 `lang=zh` 的对应页面，英文查询命中 `lang=en` 的对应页面；指定 `lang` 时不混入其他语言。
17. **种子幂等**：连续执行两次 seed，第二次 imported=0、skipped=N、failed=0；Document / Chunk 计数与 `doc_id` 集合不变。
18. **不覆盖用户修改**：编辑种子页面生成用户 Revision 后再次 seed，该页 skipped，用户 Revision 保留，无覆盖或删除。
19. **种子失败隔离**：注入单页解析失败或整体导入失败，进程仍启动，warning / `seed_import_failed` 可见，BM25 检索可用。
20. **静态摘要异步**：导入/编辑 revision 后摄入立即完成；`summary_jobs` 最终为 ready；修改 revision/model/prompt_version 触发新摘要；失败回退抽取式并标 `summary_generation_failed`。
21. **统一入口**：`POST /api/v1/query` 的 `mode=both` 同时返回 results 与 answer；同一 query/conversation 的引用一致；`?q=&conversation=` 可恢复。
22. **会话历史**：search-only/answer 轮次都写入 `messages`，按 tenant/space/owner 隔离；不存 chain-of-thought；删除写 tombstone。

**明确不做**

- 不做 pgvector / HNSW、不做 OIDC / SSO / 多租户、不做集中审计；认证仅限本地会话（cookie + CSRF）与 Bearer token。
- P1 的 spaces 仅作为本地 Wiki 命名空间，不承担跨用户共享与权限协作；审计仅本地追加日志，不做不可篡改审计。
- 不做图谱与多模态。
- 不提供 SQL Server 或其他第二关系库适配器（关系库后端只有 SQLite 与 PostgreSQL）；不引入 Python 运行时或 Python 扩展。
- 不做 Markdown / PDF / Wiki 页面之外的格式摄入。
- 不做服务端并发容量承诺与性能基准，不承诺冷启动时间。

---

## P2 Agent 接口（REST + MCP）与评测集回归

**目标**：把内核暴露给 Agent，并让评测集回归成为每次变更的固定检查，而不是上线前的一次性动作。

**交付物**

- REST 接口与 MCP server，两者映射到同一检索入口与同一 Answer 契约。
- REST 与 MCP 复用 P1 的 Bearer 认证与令牌校验；未认证请求被拒绝。
- Answer 契约的显式版本标识与兼容策略（新增字段可选，删除或改语义需升版）。
- 评测集：在 P1 的 20 条基础上扩充，覆盖精确事实、语义改写、跨文档多跳、无答案四类问题，每条带标注位置或"应无答案"标记。
- 可选 event-entity 通道：`events` / `entities` / `event_entities` 三表与 `bm25` / `bm25_judge` / `event_entity` / `hybrid` / `auto` 五种模式；抽取默认关闭，BM25 默认不变；SQLite 与 PostgreSQL 共用逻辑 schema（见 [ADR 0007](adr/0007-event-entity-retrieval.md)）。
- event 评测：在 20 篇文档 + 20 条问题上分模式跑 Recall@5、MRR、P95；记录 event 开启前后与抽取成本，不用 SAG 公开数字代替本项目结果。
- 双语评测：中文 / 英文各一组查询，验证 `lang` 过滤命中对应语言；UI 语言切换只改展示与默认过滤，不复制索引；REST / MCP 接受同一 `lang` 参数。
- 摘要/会话 Agent 面：SSE 流式回答、最近 N 轮改写、Zeta session 映射（`external_session_id`）、摘要管理页；`POST /api/v1/query` 的 answer 模式与 `kb_query` 共用会话。
- 回归报告：指标、引用可回跳率、`degraded[]` 分布、与上一基线的差异。

**可执行出口标准**

1. **通道一致**：同一问题分别经 CLI、REST、MCP 调用，返回的 Answer 字段集合一致（正文、正文类型标识、引用、`degraded[]`、治理上下文、请求回显均存在）。
2. **指标一致**：三通道在同一评测集上的 Recall@5 差异为 0；引用可回跳率均为 100%。
3. **无旁路**：接口层不持有索引句柄、不实现召回或过滤（依赖检查 + 代码审查确认），检索调用全部经过内核入口。
4. **回归可重复**：同一数据集、同一配置连续运行两次，检索指标完全一致；含生成式回答时，质量指标的差异不超过 1 条问题。
5. **无答案可识别**：标注为"应无答案"的问题不得返回编造引用；回答为"证据不足"且引用为空。
6. **回归报告存档**：每次变更附带报告，基线指标可追溯到具体数据集与配置。
7. **认证复用**：无 Bearer 的 REST / MCP 请求返回 401；有效 Bearer 成功；过期或伪造令牌返回 401（与 P1 判定一致）。
8. **默认模式不变**：未配置抽取时，`bm25` / `auto` 不触发 event 通道，不出现 event 相关降级；显式 `event_entity` 返回 `event_entity_not_enabled`，BM25 仍可返回结果。
9. **抽取失败隔离**：注入抽取超时/失败后，BM25 仍满足 P1 的 Recall@5；`degraded[]` 含 `event_extraction_unavailable`，不得阻塞其他通道。
10. **模式契约一致**：`bm25`、`bm25_judge`、`event_entity`、`hybrid`、`auto` 返回同一 Answer 字段集合；event 只影响候选，不得改变引用锚点或 Answer 契约。
11. **语言过滤**：中文查询命中 `lang=zh`，英文查询命中 `lang=en`；指定 `lang` 时其他语言候选为 0。
12. **切换不复制索引**：切换 UI 语言前后，Document / Chunk 计数与 `doc_id` 集合不变；只改变展示与默认 `lang` 过滤。
13. **SSE 流式**：`GET /api/v1/answer/stream` 返回 `text/event-stream`，按顺序输出 token/event，最终 `done`；中断可重连。
14. **Zeta 会话映射**：Zeta `zeta_session_id` 写入 `conversations.external_session_id`；`kb_get_conversation`/`kb_list_conversations` 可读；不存 CoT。
15. **摘要管理**：`GET /api/v1/pages/{space}/{slug}/summary` 返回 status/ready/failed；失败可重跑；`summary_jobs` 有计数与错误。

**明确不做**

- 不做 OIDC、共享空间、多租户；P2 仍为 solo 配置。
- 不做 SDK 语言绑定与第三方集成。
- 不做图数据库与多模态；event-entity SQL-RAG 是可选通道，不维护全局图。
- P2 只使用 P1 的 Store 后端，不引入 pgvector / HNSW。

---

## P3 team：pgvector hybrid、OIDC、共享空间、集中审计

**目标**：在 P1 的 PostgreSQL 后端上启用 pgvector hybrid 作为默认，加入 OIDC 身份、共享空间与集中审计，在不改内核语义的前提下支持多人协作。

**交付物**

- pgvector hybrid 默认启用（HNSW），全文与向量检索共用同一融合与打分路径；复用 P1 的 PostgreSQL 后端与迁移体系。
- event-entity PG 调优：`events.tsv` 生成列 + GIN，`entities` 的 `lower(name)` btree 或可选 `pg_trgm`，`event_entities` 复合索引；CTE + `statement_timeout`；验收用 `EXPLAIN (ANALYZE, BUFFERS)`（见 [ADR 0007](adr/0007-event-entity-retrieval.md)）。
- 抽取任务按空间配置；`hybrid` 在抽取启用时把 event 通道纳入 RRF，未启用时保持 BM25 + dense，不伪装成已启用。
- OIDC 集成：登录、token 校验、登出失效。
- 空间（space）成员与角色模型；`space_id` 隔离；groups/group_members、space_members、租户/组/空间三层角色；自助创建策略与基础 storage quota（见 [spaces-and-permissions.md](spaces-and-permissions.md)）。
- 召回期 ACL 过滤（不是结果后置剔除）。
- 集中审计：检索与读取的查询审计（默认 90 天，可脱敏）。
- 迁移工具：复用 P1 的目录 / JSONL 导入导出把 solo 数据集导入 team，doc_id 保持不变。

**可执行出口标准**

1. **空间隔离**：构造两个用户、两个空间，B 空间用户对 A 空间文档发起 20 条探针查询，命中数为 0。
2. **过滤先于截断**：准备"高相关但无权限"的文档与至少 5 篇有权限且相关的文档，TopN = 5 时结果数为 5 且不含无权限文档；查询计划显示过滤条件在截断之前生效。
3. **迁移稳定**：同一数据集从 solo 迁移到 team 后，doc_id 集合完全一致，P1 的 20 条问题 Recall@5 不低于 solo 基线。
4. **身份有效**：token 过期或登出后的请求被拒绝；无 token 请求不返回任何文档内容（fail-closed）。
5. **ACL 变更即时**：撤销某用户对某文档的权限后，该用户的下一次检索不再返回该文档。
11. **空间成员即时**：移除成员/组后下一次请求不再返回该空间内容；非成员访问 private 空间 403；`internal` 只返回元数据。
12. **最后 owner 保护**：移除/降级最后一个 space_owner 返回 `last_space_owner_protected`；group 至少保留一个 group_admin。
13. **存储配额**：hard 超限返回 413/429 + `space_quota_exceeded`；删除/归档在 purge 前仍计入；token quota 不受影响。
6. **契约不变**：三通道 Answer 字段集合与 P2 一致，`degraded[]` 语义不变。
7. **hybrid 默认生效**：默认配置下 pgvector 通道参与召回（查询日志或查询计划可见），Answer 不出现 `dense_not_enabled`；强制禁用向量通道时返回 `degraded[]`，不得静默只用关键词。
8. **event 计划验收**：`EXPLAIN (ANALYZE, BUFFERS)` 显示 `events.tsv` GIN、`entities` 的 btree/trgm、`event_entities` 复合索引；无大表 Seq Scan；`statement_timeout` 生效。
9. **ACL 先于扩展**：种子实体与 1–2 跳扩展的 SQL 均带 `tenant_id` / `space_id` / `Policy::visibility_from`；跨空间/跨租户探针命中为 0。
10. **hybrid 通道裁剪**：抽取启用时 event 参与 RRF；未启用时 BM25 + dense 正常返回，不出现虚假的 event 降级；显式 `event_entity` 仍按 `event_entity_not_enabled` 声明。

**明确不做**

- 不做多租户（`tenant_id` 仍为单值），不做 SSO / SAML、目录供给。
- 不做 SQL Server / MySQL 适配；PostgreSQL 是 team / enterprise 唯一关系库。
- 不做图数据库与多模态；event-entity 是可选 SQL 通道，不维护全局图。
- 不做跨区域复制与读写分离。
- 不做计费、配额与租户级运营后台。

---

## P4 enterprise：SSO、召回期 ACL、多租户、审计、图谱与多模态

**目标**：在 team 的基础上达到企业级隔离、可审计与可扩展检索，同时保持低档行为不变。

**交付物**

- SSO（SAML / OIDC）与目录供给、服务账号。
- `tenant_id` 强隔离：索引与查询都必须携带租户条件。
- 审计：检索、读取、ACL 变更各产生可导出事件（主体、租户、空间、查询、返回的 doc_id 集合、时间），WORM 不可篡改存储与 SIEM 导出。
- ACL 变更的生效路径与缓存失效策略（按权限版本）。
- 图谱索引与多模态解析，作为可选能力接入（可降级）。
- 数据驻留与留存策略。
- 租户级空间配额、共享/归档策略与 space.* / quota.* 审计；组与空间角色按租户策略生效。

**可执行出口标准**

1. **跨租户隔离**：以租户 A 身份对租户 B 文档发起 20 条探针查询，命中数为 0；索引层查询必须携带 `tenant_id` 条件（查询计划核对）。
2. **召回期 ACL 仍成立**：P3 的"过滤先于截断"验证在多租户条件下重跑通过；高相关但无权限的文档不占 TopN 名额。
3. **审计覆盖**：抽查 20 次检索与 20 次读取，每次操作各产生一条可导出记录，字段完整且与请求一致；缺失数为 0。严格模式下审计写入失败即拒绝回答，不返回内容。
4. **ACL 变更时效**：撤权后下一次检索即不返回该文档；缓存窗口必须可观测，不能存在"未知何时生效"的状态。
5. **降级演练**：分别禁用图谱与多模态，基础检索仍满足 P1 出口标准，`degraded[]` 非空并标明能力；禁用身份服务时请求被拒绝（fail-closed），不返回任何内容。
6. **规模抽样**：在可复现的抽样子集上，Recall@5 与 P1 基线差异不超过 0.05，并记录查询延迟分布作为基线。

**明确不做**

- 不做 10⁷ 全量压测与 SLA 承诺（需单独的容量规划与压测环境）。
- 不做跨租户的数据共享/联合检索。
- 不做非结构化格式的无限扩展（新增模态需单独 ADR 与出口标准）。
- 不改变 doc_id 派生规则；若确需改变，须新增 ADR 并提供迁移路径（ADR 0003）。


## 跨阶段补充：运营模式、限速、i18n、支付与 Agent 访问

| 阶段 | 交付 | 出口 |
| --- | --- | --- |
| P1 | JSON 日志契约/脱敏、/health、基础 /metrics、服务令牌 scope、CLI auth/search/ask --json、基础并发/QPS | 日志字段完整不泄漏；429/503 可复现；CLI --json 与 REST 一致 |
| P2 | 设备码登录、token 轮换、lkb mcp、慢查询/死锁重试、Redis/PG 多实例限流 | 吊销立即 401；单 token 超 QPS 429；SQLite/PG 并发无死锁 |
| P3 | deployment.mode/features、启动校验、限速维度、i18n zh/en、错误 code | 非法模式拒绝启动；限流分维度生效；UI/API 无硬编码文案 |
| P4 | PaymentProvider 适配、套餐/试用/降级、entitlement、webhook 幂等、发票/税务 | 沙箱支付闭环；欠费停服/降级可回滚；对账可追溯 |


## 跨阶段补充：桌面 APP 与笔记

| 阶段 | 交付 | 出口 |
| --- | --- | --- |
| P0 | Tauri 2 壳 + sidecar 启停/单实例/托盘；首启 SQLite/PG + 测试连接 + 凭据存储；静态 web dist 托管 | 双击 exe 可用；portable 可用；solo+PG 跑同一组用例；team+SQLite 被拒 |
| P1 | 笔记入口/personal 空间/vault/文件树/编辑器/反向链接/索引 | 外部编辑可索引；原子写/冲突不丢；笔记与 Wiki 在 UI 统一 |
| P2 | 安装包/portable/自动更新/签名/团队发布共享/诊断 | 签名更新可回滚；未签名提示；发布到团队空间权限正确 |
