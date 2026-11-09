# SAG / SAG-Wiki 评估

> 评估日期：2026-11-08
> 对照基线：[architecture.md](architecture.md)、[tier-matrix.md](tier-matrix.md)、[roadmap.md](roadmap.md)。
> 范围：公开官方文档、论文页与检索结果；未完整审计源码，未运行 SAG，未复现基准。
> 限制：GitHub 页面在本机 TLS 校验失败，star 数、提交频率、社区活跃度均不评价；安装包体积、建库耗时等来自搜索摘要的数字一律标注“未核实”。
> 结论：SAG 的 event-entity SQL-RAG 已被采纳为 Rust 内核的可选检索通道（clean-room 重实现，见 [ADR 0007](adr/0007-event-entity-retrieval.md)），不是当前内核的替代品。P1 默认路径不变；P2/P3 实现该通道并做 SQLite / PostgreSQL 调优；sidecar 不作为主路径。

## 1. 一页结论

| 问题 | 结论 |
| --- | --- |
| SAG 是什么 | Zleap-AI/SAG，MIT，Python 3.11+ 知识引擎；v1.2.2（文档更新 2026-07-22）。以 chunk 为原文证据边界，event 为完整语义单元，entity 为索引/扩展点；查询时用 SQL 沿共享实体生成局部动态超边，不维护全局知识图谱，也不做 GraphRAG 式离线实体合并/关系归一。 |
| 产品形态 | Web（Next.js 15 / React 19）、Electron 桌面、FastAPI API、REST / OpenAI 兼容 / MCP，以及可嵌入的 Python 包 `zleap-sag`；应用层通过 `apps/api/sag_api/sag/` 适配层访问引擎，引擎不依赖 Web、用户、会话或引用。 |
| 与我们的重合 | 都强调原文证据与可追溯引用；都用 SQLite 起步、支持 PostgreSQL/pgvector；接口方向都覆盖 REST/MCP；都不要求为多跳检索维护一张全局图；都关注增量索引与多跳召回。 |
| 与我们的差异 | SAG 的 event 由 LLM 抽取，不是人工 claim；官方文档未给出 tenant/space/ACL/时序/version 的等价契约；引擎不知道用户与会话；默认单用户、本地优先；运行时是 Python。我们的内核是 Rust 单二进制、治理字段 day-one、Answer 契约与 `degraded[]` 内建、三档 profile。 |
| 能否替代图数据库 | 不能整体替代。它能替代“为共享实体多跳检索维护的全局图/全局超边”这一职责；不能替代类型化关系遍历、深路径查询、社区摘要与全局综述。 |
| 建议 | 已拍板：采用 clean-room Rust event-entity 可选通道（[ADR 0007](adr/0007-event-entity-retrieval.md)）；P1 默认 BM25 不变；P2 实现 `bm25` / `bm25_judge` / `event_entity` / `hybrid` / `auto` 与 20+20 评测，P3 做 PostgreSQL 调优。sidecar 只作对比/应急，不作主路径。 |

### 1.1 已核实的公开基准

官方论文页给出的设置：相同 `BGE-Large-EN-v1.5` Embedding 与 `Qwen3.6-Flash` LLM；HotpotQA、2WikiMultiHopQA、MuSiQue 三个多跳数据集，9 项 Recall@1/2/5 中 8 项最佳。

| 数据集 | 指标 | SAG | HippoRAG 2 |
| --- | --- | --- | --- |
| HotpotQA | Recall@2 | 91.55% | 78.35% |
| 2WikiMultiHopQA | Recall@2 | 82.30% | 76.55% |
| MuSiQue | Recall@2 | 64.05% | 49.52% |
| 平均 | Recall@2 | 79.30% | 68.14% |
| 平均 | Recall@5 | 88.18% | 83.28% |

这些数字说明 event-entity + 查询时超边在多跳检索上有竞争力，但不证明它能在我们的语料、语言、权限与规模约束下复现。论文页也明确：模型、预处理、索引参数或评测实现变化都会影响数值。

## 2. 架构对照

### 2.1 SAG 的数据模型

| 对象 | 定义 | 作用 | 证据/派生关系 |
| --- | --- | --- | --- |
| Chunk | 原文证据的最小返回边界 | 检索结果最终映射回 chunk；引用回到原文 | 事实源侧证据 |
| Event | 一个 chunk 的完整事件或陈述 | 保留完整语义，不拆成失去上下文的三元组 | 由 chunk 抽取/生成，属派生语义单元 |
| Entity | 轻量索引节点 | 定位与扩展相关 event；作为共享实体连接键 | 由 chunk/event 抽取，非全局实体主数据 |
| Dynamic hyperedge | 查询时由 SQL 沿共享实体连接局部 event | 生成当前查询的局部候选空间，不预建全局超边 | 查询时临时实例化，不持久化为全局图 |
| SQL 检索 | 以共享实体为 join key 的局部扩展 | 多跳候选扩展；最终仍返回原始 chunk | 在线查询路径 |
| Document / Source | 应用层文档与信源管理 | 解析、任务、状态、上传文件 | 应用层对象，引擎只接收规范化内容 |

### 2.2 与我们数据模型的映射

| SAG | 我们的对应物 | 可复用点 | 冲突/缺口 | 判断 |
| --- | --- | --- | --- | --- |
| Document / Source（应用层） | Document：`doc_id`、`revision_id`、`source_role`、`pair_id`、治理字段 | 规范化 Markdown、文档解析、信源管理 | SAG 引擎不持有我们的稳定 `doc_id`、版本、有效期、ACL；需要适配层映射 | 可借鉴解析与任务分层，不能直接共用身份 |
| Chunk（证据边界） | Chunk：派生缓存，可重建；引用锚点是 `doc_id + revision_id + offsets` | 以 chunk 作为最终证据边界；引用回到原文 | SAG chunk 是返回/引用边界；我们的 `chunk_id` 会随分块器变化，不能作为外部引用锚点 | 可复用“证据边界”思想，不能复用 chunk ID 语义 |
| Event（完整语义单元） | 没有直接等价物；最接近 Claim，但 Claim 是人工断言 | 把 chunk 组织成可扩展的语义单元，利于多跳 | Event 由 LLM 抽取，不是人工核验的 claim；需要模型版本、重抽取与成本控制 | 可作为新的派生层，不能替代 Claim |
| Entity（索引/扩展点） | 图谱通道中的实体/节点；我们未把 Entity 作为第一等表 | 共享实体作为 join key，扩展局部候选 | 无全局实体合并/关系归一；不等于类型化图模型；需要租户/空间/ACL 过滤 | 可作为多跳索引键，不足以承载完整图谱 |
| Dynamic hyperedge | 图谱通道的查询时扩展；我们当前依赖图库或邻接表 | 只为当前查询实例化局部超边，避免全局图维护 | 仅共享实体语义，不表达“负责/依赖/属于”等类型化关系；SQL 扩展必须注入权限与时间过滤 | 可作为图谱 L1/L2 的替代实现，不是图数据库全替身 |
| SQL-RAG 检索 | 结构化通道 + 关键词/向量召回 + 融合 | SQLite/PostgreSQL 都可执行；局部 join 成本可控 | SAG 还支持 MySQL/OceanBase/ES；我们只保留 SQLite/PostgreSQL；SQL 方言与查询计划需验证 | 可复用查询思路，需按我们的存储约束重写 |
| 应用层引用组装 | Answer 契约：`citations`、`conflicts`、`degraded`、`sources` | 引用目标来自引擎返回的 chunk；生成与证据分离 | SAG 引擎不知道引用、用户、会话；没有 `degraded[]`、冲突与租户字段 | 必须经适配层转换成我们的 Answer |
| 应用层用户/JWT | Principal、tenant、space、ACL、审计 | 认证与应用编排分层 | 引擎不执行 ACL；默认单用户；未提供多租户/行级权限契约 | 不能把 SAG 的用户模型当作我们的治理层 |

### 2.3 系统边界对照

| 维度 | SAG | 我们 | 影响 |
| --- | --- | --- | --- |
| 语言/运行时 | Python 3.11+ 引擎；FastAPI；Next.js 15 / React 19；Electron；PyInstaller sidecar | Rust 单二进制（axum + sqlx）；无 Python 运行时 | C 方案直接冲突；B 方案引入第二运行时 |
| 入口 | Web/Desktop、REST、OpenAI 兼容、MCP、Python 包 | CLI、本地 Web、REST、MCP | 入口形态重合，但认证与 Answer 契约不同 |
| 身份 | 应用层用户 + JWT；默认本地单用户 | Principal + OIDC/SSO + tenant/space + 服务账号 | SAG 不是多租户治理系统 |
| 网络边界 | 默认 Compose 只监听 `127.0.0.1`；生产要求 HTTPS、VPN/IP 白名单或反代认证；JWT 不替代公网访问控制 | 默认 loopback；企业由网关/SSO/租户策略控制 | SAG 不能直接暴露公网，也不能替代我们的边界 |
| 存储 | 默认 SQLite + LanceDB；可迁移 PostgreSQL/pgvector、Elasticsearch、OceanBase；官方 Python 页另列 MySQL | SQLite 单文件（solo）/ PostgreSQL（team、enterprise）；不提供 SQL Server | 可在 PG/pgvector 上重合；多后端会增加适配与测试面 |
| ACL | 应用层安全策略；引擎不知道用户与会话；官方文档未给出行级 ACL 契约 | 召回阶段 ACL 过滤 + L4 复核；租户/空间隔离 | SAG 原样接入无法满足我们的权限底线 |
| 时序/版本 | 官方数据模型未给出 `valid_from/valid_to`、`version`、`superseded_by` 等价物 | day-one 必填并参与召回过滤 | 需要外层补齐，不能依赖引擎 |
| 审计 | 官方文档强调应用层负责引用、任务与安全策略；未见 WORM/租户审计契约 | 本地/集中/WORM 审计，按档位分级 | 企业档不能直接采用 |
| 检索 | event-entity + 查询时动态超边；语义召回 + SQL 扩展 | BM25 / hybrid / 图谱多路；RRF；判别/CE 重排 | 可作为多跳通道候选，不是全量替代 |
| 生成/引用 | 应用层检索证据 → LLM → SSE + 引用 | 抽取式或 LLM + 统一 Answer 契约 | 需要把 SAG 输出适配为我们的字段 |
| 评测 | 论文基准（英文多跳、固定模型配置） | 自有评测集、Recall@K/NDCG/引用回跳 | SAG 数字只能作先验，不能作验收 |

## 3. 与我们三档的映射

### 3.1 档位映射

| 档位 | 我们当前基线 | SAG 的可用形态 | 判断 | 触发条件 |
| --- | --- | --- | --- | --- |
| solo | SQLite 单文件；FTS5 BM25 Top 30；可选 Jev 判别重排；抽取式回答；无 Embedding、无向量库、无 GPU | Python 3.11+ 引擎；event/entity 抽取需要 LLM；默认 SQLite + LanceDB；FastAPI 应用 | 不适合作为默认路径。引入即违反零外部依赖、无 Python 运行时、单二进制；即使只嵌引擎，LLM 抽取也破坏离线默认 | 无 P1 触发；只做离线研究/评测，不进入产品 |
| team | PostgreSQL + pgvector hybrid；OIDC；空间级 ACL；Rust 服务 | 支持 PostgreSQL/pgvector；可独立 sidecar；引擎无 ACL，应用层 JWT 单用户 | 不采用 sidecar；clean-room event-entity 已进入 Rust 内核（ADR 0007），SAG 仅作参考 | 无；仅在需要第三方对照时使用 |
| enterprise | 多租户、SSO、源 ACL、WORM 审计；图谱/多模态可选 | 单用户、本地优先；无 tenant/ACL/时序/审计契约；可按租户部署多实例 | 不采用 SAG 运行时；clean-room 通道按租户/空间配额启用，权限仍由我们的 Policy 执行 | 按 ADR 0007 的租户启用与配额策略，不依赖 SAG 部署 |
| 多跳/图谱 | 图谱通道可选；图库承担类型化关系、路径与社区摘要；L0 邻接表可降级 | event-entity + 查询时超边，擅长共享实体多跳；无全局图；无类型化关系/社区摘要 | 已采用为可选 event-entity 通道；不替代整个图数据库层 | 多跳评测优于现有 BM25/hybrid/图谱 L0，且引用与延迟不回归 |

### 3.2 SAG 能否替代“图数据库”这一层

| 能力 | SAG event-entity | 图数据库 | 结论 |
| --- | --- | --- | --- |
| 共享实体多跳 | 强：SQL 沿共享实体连接局部 event | 强 | SAG 可竞争 |
| 类型化关系（负责/依赖/属于） | 弱：实体是索引键，未提供类型化边与关系归一 | 强 | 不能替代 |
| 深路径/任意跳遍历 | 受限：查询时局部扩展，不是通用路径查询引擎 | 强 | 不能替代 |
| 社区摘要/全局综述 | 不支持：不维护全局图 | 支持（配合社区检测/摘要） | 不能替代 |
| 增量更新 | 较好：新 chunk 追加自己的 event/entity，不重建全局图 | 可能需要图维护 | SAG 有优势 |
| 运维 | 无图库，但增加 LLM 抽取与 event/entity 表 | 图库运维 | 取决于抽取成本 |
| 权限/租户过滤 | 未内建；需外层注入或物理隔离 | 可在查询层实现 | SAG 需补齐 |

**判断**：如果图数据库的唯一用途是“共享实体多跳召回”，SAG 式 event-entity + 查询时超边可以替代这张全局图；如果还要承担类型化关系、路径查询、社区摘要与全局分析，则不能替代，最多作为图谱通道的一个 L1/L2 实现，与图库并存。

## 4. 集成选项决策矩阵

### 4.1 语言、构建与迁移

| 选项 | 形态 | 语言/运行时冲突 | 单二进制 | 许可 | 迁移成本 | 检索收益 |
| --- | --- | --- | --- | --- | --- | --- |
| A 仅参考架构 | 读论文、文档与评测设计，不引入依赖 | 无 | 保持 | MIT；仅参考 | 低：文档与研究成本 | 间接：影响设计与评测 |
| B 外部 sidecar | 独立 Python 服务，经 REST/MCP 返回候选；主内核仍负责 Answer | 有：Python/FastAPI、第二套存储与 LLM | 主二进制保持，部署不再是单二进制 | MIT；依赖与模型许可另审 | 中：重建索引、doc_id 映射、Answer 适配 | 中高：多跳候选；待复现 |
| C 嵌入 Python 引擎 | Rust 进程内/子进程调用 `zleap.sag` | 高：Python 3.11+；一个进程一份 `EngineConfig`；一个 `DataEngine` 代表一个逻辑信源；连接是进程级全局资源 | 破坏 | MIT；依赖与模型许可另审 | 高：桥接、数据模型、ACL 包装 | 高：直接使用引擎；但治理缺口大 |
| D Rust 重实现 | 在我们的内核实现 event/entity/event_entity 表与查询时 SQL 展开 | 无：Rust + SQLx；SQLite/PostgreSQL | 保持 | MIT 参考；若移植代码需保留版权许可，优先 clean-room | 高：研发、迁移、抽取管线、评测 | 中高：待我们语料验证 |

### 4.2 运维与 P1 门禁

| 选项 | 运维复杂度 | 与 P1 构建门禁的关系 | 结论 |
| --- | --- | --- | --- |
| A 仅参考架构 | 无 | 不冲突；P1 仍是 Rust 单二进制、SQLite、BM25、无 Python | 保留为参考（不单独采用） |
| B 外部 sidecar | 高：第二套服务、双索引、LLM 抽取、备份与监控 | 不进入 P1；若打包进产品则违反“单二进制/无 Python 运行时”；仅作对照/应急 | 不采用（仅对照） |
| C 嵌入 Python 引擎 | 高：Python 运行时、进程级全局配置、多租户需多进程 | 直接违反 P1“无 Python 运行时”和单二进制启动 | 不采用 |
| D Rust 重实现 | 中：新增表/索引与抽取任务；无图库 | P1 默认不变；P2/P3 作为可选多跳通道，不改变 P1 门禁 | 已采用（clean-room，ADR 0007） |

### 4.3 P1 门禁逐项对照

| P1 门禁 | A | B | C | D |
| --- | --- | --- | --- | --- |
| Rust 单一二进制（axum + sqlx） | 保持 | 主二进制保持，部署多进程 | 破坏 | 保持 |
| 无 Python 运行时 | 满足 | 不满足（sidecar） | 不满足 | 满足 |
| solo SQLite 单文件、默认 BM25 | 不改变 | 不改变主路径 | 可能改变 | 可保持默认不变 |
| 不引入 pgvector/HNSW | 不改变 | 若 sidecar 自带存储则引入 | 若引擎用 PG 则引入 | 可只做 SQL/表，不强制向量 |
| 不做图谱/多模态 | 不改变 | 多跳 sidecar 需单独评估 | 同左 | event-entity 可选通道；不引入图库 |
| 不做 OIDC/多租户 | 不改变 | 外层仍需我们的身份 | 不满足 | 不改变 |

## 5. 推荐

### 5.1 短期（P1）

| 决策 | 内容 | 理由 |
| --- | --- | --- |
| 不引入 SAG 运行时 | 不新增 Python 依赖、不接 sidecar、不复制代码 | P1 门禁要求 Rust 单二进制、无 Python、SQLite/BM25 默认；SAG 的 event 抽取需要 LLM，破坏离线默认 |
| 采用 clean-room 设计 | event/entity/查询时超边写入 [ADR 0007](adr/0007-event-entity-retrieval.md)，P2 起在 Rust 内核实现 | 只借鉴架构思想，MIT 仅作参考；不引入 SAG 代码或运行时 |
| 保持现有默认 | solo 仍是 BM25 Top 30 + 可选 Jev + 抽取式；hybrid 可选 | 与当前架构、tier-matrix、roadmap 一致；event 抽取默认关闭 |

### 5.2 中期（P2/P3）

| 阶段 | 动作 | 出口标准 |
| --- | --- | --- |
| P2 | 在 Rust 内核实现 event/entity/event_entity 与五种模式：`bm25` / `bm25_judge` / `event_entity` / `hybrid` / `auto`；抽取可选、默认关闭；查询时按共享实体做局部 SQL 展开，接入现有 L3/L4，不新增 Answer 路径 | 20+20 上分别跑 Recall@5、MRR、P95；BM25 基线不回归；抽取失败写 `event_extraction_unavailable` 且不阻塞 BM25；抽取 token/耗时入库 |
| P3 | 在 team 档 PostgreSQL 实现同一通道并做计划调优：`events.tsv` GIN、`entities` btree/trgm、`event_entities` 复合索引；所有扩展注入 tenant/space/ACL/时间过滤 | `EXPLAIN (ANALYZE, BUFFERS)` 达标；ACL 泄漏测试为 0；引用与 `degraded[]` 契约不变；成本与延迟在预算内 |
| P4 | 视评测结果决定 event-entity 是否进入 enterprise 多跳默认；图数据库仅保留给类型化关系、路径与社区摘要 | 需要新 ADR 或修订 ADR 0007；不得直接替换现有图谱默认；保留回滚开关 |

### 5.3 sidecar 的边界（不作主路径）

主路径是 clean-room Rust event-entity 通道；sidecar 只用于第三方对照或 D 受阻时的临时方案，不进入产品默认。若仍要评估 sidecar，必须同时满足下表条件。

| 条件 | 必须验证 | 不满足时 |
| --- | --- | --- |
| 现有 BM25/hybrid/event_entity 在真实多跳问题上存在可量化缺口 | 用同一评测集对比，缺口可复现；不是主观感受 | 不采用 sidecar；继续优化 D |
| 部署方接受 Python sidecar | team/enterprise 有独立进程、容器或主机；P1 不引入 | 不采用 B；走 D |
| ACL/租户可隔离 | SAG 是否支持元数据过滤需验证；若不支持，按租户/空间物理隔离部署；wrapper 做召回期过滤 | 不采用 B；post-filter 不可接受 |
| 抽取成本可测 | 统计每 chunk 的 LLM 调用次数、token、重试、延迟；纳入 ingest 预算 | 不采用 B；先优化 D 或缩小范围 |
| 评测可复现 | 在我们的语料上复现多跳收益，且引用回跳不回归 | 不采用 B；论文数字只作先验 |
| 安全与许可通过 | MIT 与依赖/模型许可审查；不暴露公网；密钥与数据隔离 | 不采用 B |

### 5.4 明确不做

| 不做 | 原因 |
| --- | --- |
| 不在 P1 嵌 `zleap-sag`（C） | 违反 Rust 单二进制、无 Python 运行时；进程级全局配置不利于多租户 |
| 不把 SAG 应用当作我们的产品外壳 | 其用户/JWT/单用户模型与我们的 Principal/tenant/space/ACL/审计不兼容 |
| 不用 event-entity 替代整个图数据库层 | 仅覆盖共享实体多跳；类型化关系、深路径、社区摘要仍需图能力 |
| 不根据论文数字直接上线 | 英文多跳基准、固定 BGE/Qwen 配置；我们的语料、语言、权限与规模不同 |
| 不把未核实数字写入架构或容量规划 | 14MB、0.41s、star、活跃度均未核实 |

## 6. 风险与待验证

| 风险 | 已知事实 | 影响 | 验证方法 | 状态 |
| --- | --- | --- | --- | --- |
| event/entity 抽取成本 | 官方 Python 引擎配置需要 LLM；文档未给出每 chunk 调用次数、token 与重试成本 | ingest 成本、延迟、外部依赖；solo 零依赖默认不可用 | 在固定语料上测 tokens/chunk、调用数、P95；对比抽取前后召回 | 待实测 |
| 基准可复现性 | 官方数字使用 BGE-Large-EN-v1.5 + Qwen3.6-Flash；英文多跳数据集；官方页 9 项 8 最佳，arXiv 摘要称每个 benchmark 最佳，2Wiki R@5 为 88.00% 低于 HippoRAG 2 的 90.35% | 误读为普遍优势 | 用 SAG-Benchmark 或自建脚本复现；固定模型/预处理/索引参数；按单指标与数据集整体分别报告 | 官方数字已核实，独立复现未做 |
| SQL 方言差异 | SAG 支持 SQLite/PostgreSQL/MySQL/OceanBase/ES；我们只保留 SQLite/PostgreSQL | 动态 join SQL 在方言、索引、查询计划上表现不同 | 在 SQLite 与 PostgreSQL 上跑同一组多跳查询；检查执行计划与超时 | 待验证 |
| 权限与多租户缺口 | 引擎不知道用户、会话、引用；应用层用户 + JWT；默认本地单用户；官方文档未给出 tenant/space/ACL/时序/version 契约 | 直接接入会绕过我们的召回期 ACL；企业档不可用 | 设计租户/空间隔离或元数据过滤；做跨租户/跨空间探针；fail-closed | 缺口明确，方案未验证 |
| Python 依赖 | `zleap-sag` 要求 Python 3.11+；应用要求 `zleap-sag>=0.7.1`；一个进程一份 `EngineConfig`，连接为进程级全局资源 | 与 Rust 单二进制、多租户进程模型冲突 | 仅在 sidecar 方案中评估；做进程/资源/隔离测试 | 已知冲突 |
| 数据模型与引用 | SAG event 是 LLM 派生语义单元；引用在应用层组装；引擎返回 chunk | 与我们的 Claim、doc_id/revision、引用锚点不一致 | 建立映射表；验证引用回跳、版本变更、重抽取后的引用稳定性 | 待设计 |
| 许可与依赖 | 仓库与 `zleap-sag` 为 MIT；模型、解析器、前端依赖的许可未逐一审查；SAG-Benchmark 复现仓库未核验 | 分发与商用风险 | 做依赖清单与许可证扫描；区分代码、模型、数据许可 | 部分核实 |
| 运维与备份 | 生产文档要求同时备份 `pgdata` 与 `sagdata`；单用户、本地优先；不承诺多租户/HA | 企业运维成本与 RTO 风险 | 在 team 测试环境做备份恢复；评估双索引与双写 | 待验证 |
| 未核实数字 | 安装包 14MB、建库 0.41s、star 数、社区活跃度来自搜索摘要/文章；GitHub 页面 TLS 校验失败 | 误判成熟度与性能 | 不纳入决策；需要时从可访问的 Release/仓库页面重新核实 | 未核实 |

## 7. 来源清单

所有访问日期均为 2026-11-08。

### 7.1 已核实

| ID | 来源 | URL | 访问日期 | 用途 | 状态 |
| --- | --- | --- | --- | --- | --- |
| S1 | SAG 官方首页 | https://sag.wiki/ | 2026-11-08 | 产品定位、MIT、Python 3.11+、v1.2.2、公开基准摘要 | 已核实 |
| S2 | 认识 SAG | https://sag.wiki/guide/introduction | 2026-11-08 | chunk/event/entity/动态超边、产品边界、存储与引擎入口 | 已核实 |
| S3 | 系统架构 | https://sag.wiki/developer/architecture | 2026-11-08 | 仓库结构、FastAPI、Next.js 15/React 19、Electron、PyInstaller、引擎边界 | 已核实 |
| S4 | 论文与基准 | https://sag.wiki/project/paper | 2026-11-08 | 论文题名、arXiv 编号、9 项指标与对照表 | 已核实 |
| S5 | 安装与启动 | https://sag.wiki/guide/installation | 2026-11-08 | Desktop/Docker、127.0.0.1、Python/Node 要求 | 已核实 |
| S6 | zleap-sag Python 引擎 | https://sag.wiki/developer/python-engine | 2026-11-08 | Python 3.11+、MIT、DataEngine API、SQLite+LanceDB、PG/pgvector、ES/OceanBase、进程级配置 | 已核实 |
| S7 | 生产部署 | https://sag.wiki/operations/production | 2026-11-08 | PostgreSQL/pgvector、JWT、单用户定位、HTTPS/外围访问控制、备份要求 | 已核实 |
| S8 | arXiv 论文 | https://arxiv.org/abs/2606.15971 | 2026-11-08 | 摘要、作者、发布时间、方法概述 | 已核实 |

### 7.2 未核实或未访问

| ID | 来源 | URL | 访问日期 | 用途 | 状态 |
| --- | --- | --- | --- | --- | --- |
| S9 | GitHub 仓库页 | https://github.com/Zleap-AI/SAG | 2026-11-08（尝试） | star、提交频率、Release 节奏、社区活跃度 | 未核实：本机 TLS 校验失败；仅从官方文档与搜索结果确认仓库存在 |
| S10 | SAG-Benchmark 复现仓库 | https://github.com/Zleap-AI/SAG-Benchmark | 2026-11-08（未访问） | 独立复现基准 | 未访问/未运行；官方论文页提供链接 |
| S11 | 搜索结果中的 GitHub README / arXiv HTML | 搜索结果摘要 | 2026-11-08 | 辅助确认项目存在与 README 表述 | 二手来源，仅作线索 |
| S12 | 安装包体积 14MB、建库 0.41s、star 数、社区活跃度 | 搜索摘要/文章 | 2026-11-08 | 成熟度与性能参考 | 未核实，不作为事实或决策依据 |

> 说明：SAG 应用 v1.2.2 与 Python 引擎 `zleap-sag>=0.7.1` 是不同版本口径；本文不把应用版本当作引擎版本。SAG 官方文档页更新时间为 2026-07-22。
