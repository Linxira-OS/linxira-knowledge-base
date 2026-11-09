# ADR 0007：可选 event-entity SQL-RAG 检索通道

- **状态**：Accepted
- **日期**：2026-11-08

## 背景

当前 solo 默认只有 SQLite FTS5 BM25，可选 judge 只对已召回候选重排；多跳、关系型召回弱，且不能用重排弥补召回漏项。SAG 的公开工作说明：把 chunk 组织成语义完整的 event，以 entity 作为扩展键，在查询时用 SQL 沿共享实体生成局部超边，可以在不维护全局知识图谱的前提下改善多跳召回。

本 ADR 采用 clean-room 重实现：只借鉴 event/entity/查询时局部超边的架构思想，不复制 SAG 代码；SAG 仓库的 MIT 许可仅作为参考与合规依据。实现语言为 Rust，默认路径不变（solo 仍为 BM25），不引入 Python、SAG 应用或 sidecar。

## 决策

**在 Rust 内核增加可选的 event-entity 检索通道，提供 `bm25` / `bm25_judge` / `event_entity` / `hybrid` / `auto` 五种检索模式；抽取默认关闭，失败只降级该通道，不阻塞 BM25。**

- `bm25`：现有默认模式，始终可用。
- `bm25_judge`：BM25 召回后接可选 judge 重排；judge 不可用时回 BM25，写 `decision_unavailable`。
- `event_entity`：event/entity 专用通道；抽取未启用时写 `event_entity_not_enabled`，抽取失败时写 `event_extraction_unavailable`，并回退 BM25。
- `hybrid`：BM25 + dense（若启用）+ event_entity（若抽取启用）多路 RRF；event 通道只提供候选，不改变最终 Answer 契约。
- `auto`：按 backend 与能力选择。solo SQLite 默认解析为 `bm25`，只有配置了抽取才考虑 `event_entity`；team / enterprise PostgreSQL 默认解析为 `hybrid`（未启用 dense/event 时退化为当前可用通道）。

部署默认值：solo 为 `bm25`；team / enterprise 可配置为 `auto`（解析为 `hybrid`）。显式请求某个模式时以请求为准；任何模式都不得静默伪装成另一模式，降级必须进入 `degraded[]`。

## 数据模型

三张表使用同一套逻辑 schema，SQLite 与 PostgreSQL 只在物理类型、索引与查询计划上不同。

### events

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `event_id` | string | 是 | 稳定事件 ID；主键 |
| `tenant_id` | string | 是 | 租户过滤 |
| `space_id` | string | 是 | 空间过滤 |
| `doc_id` | string | 是 | 来源文档；引用仍回到 chunk |
| `chunk_id` | string | 是 | 事件对应证据切片 |
| `seq` | integer | 是 | 同一 chunk 内的事件序号 |
| `text` | string | 是 | 事件摘要 / 完整语义单元 |
| `content_hash` | string | 是 | 事件文本哈希，用于幂等与失效 |
| `extractor_version` | string | 是 | 抽取器版本；模型或提示词变化必须递增 |
| `extraction_model` | string | 是 | 实际使用的模型标识；规则抽取写规则版本 |
| `acl_json` | string | 是 | ACL 策略序列化；与文档 / 切片一致 |
| `acl_principals` | string | 是 | 可读取主体集合的索引表示 |
| `valid_from` | timestamp | 是 | 生效时间 |
| `valid_to` | timestamp | 是（可为空） | 失效时间；`null` 表示未知 |
| `version` | integer | 是 | 事件版本；重抽取递增 |
| `superseded_by` | string | 是（可为空） | 被新 event 取代时指向新 `event_id` |

### entities

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `entity_id` | string | 是 | 稳定实体 ID；主键 |
| `tenant_id` | string | 是 | 租户过滤 |
| `space_id` | string | 是 | 空间过滤 |
| `name` | string | 是 | 展示名 |
| `normalized_name` | string | 是 | 归一化名称；用于精确匹配与种子选取 |
| `type` | string | 是 | 实体类型；受控词表或抽取 schema 约束 |
| `created_at` | timestamp | 是 | 创建时间 |

唯一键：`(tenant_id, space_id, normalized_name, type)`。`entities` 是被引用的字典表，不直接返回给调用方；所有实体查询必须带 `tenant_id` / `space_id`。

### event_entities

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `event_id` | string | 是 | 事件 ID；外键到 `events` |
| `entity_id` | string | 是 | 实体 ID；外键到 `entities` |
| `weight` | real | 是 | 共享实体权重；默认 `1.0`，按抽取置信度或规则调整 |
| `tenant_id` | string | 是 | 租户过滤 |
| `space_id` | string | 是 | 空间过滤 |

主键：`(event_id, entity_id)`。`event_entities` 是扩展索引，不直接返回；查询必须 join `events` 并通过 `Policy::visibility_from` 过滤后才可扩展或返回。

### 治理与可见性

- `events` 是可召回表，携带 `acl_json` / `acl_principals` / `valid_from` / `valid_to` / `version` / `superseded_by`，并接入 `Policy::visibility_from(alias, dialect, start_index)`。
- `event_entities` 不独立授权；只有 join 到通过 `Policy::visibility_from` 的 `events` 行时才可用于扩展。`entities` 是字典表，不直接返回，按 `tenant_id` / `space_id` 作用域查询。
- 种子实体查询与扩展查询都必须 join `events` 并应用 `Policy::visibility_from`，先过滤再扩展；禁止先扩展、后过滤。扩展结果最终映射到 `chunk_id`，Answer 的引用仍由 `doc_id + revision_id + offsets` 构成。
- 所有查询都带 `tenant_id`；缺失租户直接拒绝。`superseded_by` 非空的 event 默认不参与召回，除非显式历史查询。

## 抽取管线

抽取是可选的后台步骤，不属于默认检索路径。

| 项 | 决策 |
| --- | --- |
| 输入 | 一个 chunk（`text`、`doc_id`、`chunk_id`、治理字段） |
| 输出 | 一个 event 摘要 + 多个 `entities(name, type)` |
| 实现 | 可选 LLM 抽取，或规则抽取（无 LLM 的降级实现）；两者共用同一输出契约 |
| 批量 | 按批提交，批大小可配置；避免逐 chunk 单请求 |
| 超时 / 重试 | 单批超时、有限重试；超时或重试耗尽不阻塞其他批 |
| 失败 | 写 `degraded[]` 的 `event_extraction_unavailable`，保留已有 event；BM25 不受影响 |
| 默认 | 关闭；solo 默认不启动抽取，规则抽取可离线运行，LLM 抽取才需要外部端点 |
| 版本 | 记录 `extractor_version` 与 `extraction_model`；模型、提示词或规则变化必须重抽取 |
| 幂等 | 同一 `chunk_id + content_hash + extractor_version` 不重复写入；重抽取生成新 event，旧 event 由 `superseded_by` 指向新 ID |
| 部分失败 | 允许部分 chunk 有 event、部分没有；event 通道只覆盖已抽取部分，`degraded[]` 说明缺口 |

抽取失败不得回滚 BM25 索引，也不得把空 event 结果当成“没有相关文档”。只有显式请求 `event_entity` 或 `auto` 解析到 event 时，才把不可用写入 `degraded[]`；普通 `bm25` 请求不因 event 未启用而降级。

## 查询

### event_entity 通道

1. **查询实体归一**：复用术语归一与受控词表，得到查询实体名与类型候选。
2. **种子实体**：先按 `entities.normalized_name` 精确匹配；不足时用 trigram / FTS 近似匹配；同时 join `events` 应用 `Policy::visibility_from`，避免通过实体存在性泄漏无权文档。
3. **扩展**：沿 `event_entities` 从种子实体扩展 1–2 跳，join `events` 再次应用可见性与时序过滤；限制种子数、扩展事件数、深度与超时。
4. **聚合**：按 `event_id` 聚合共享实体权重与事件文本相关性，得到事件分数；再按 `chunk_id` / `doc_id` 聚合为候选；若指定 `lang`，聚合时 join Document / Chunk 应用语言过滤。
5. **映射回 chunk**：返回 `CandidateSet`，每项含 `doc_id`、`chunk_id`、`event_id`、分数与来源通道；最终 Answer 仍走 L4/L5，不新增契约字段。

评分采用可配置的线性组合：`shared_entity_weight` 与 `event_text_relevance` 两项加权；权重与截断参数由评测确定，不在本 ADR 中承诺固定数值。候选爆炸通过种子数、扩展事件数、深度、超时与 `LIMIT` 共同约束。

### 模式语义

| 模式 | 行为 | 默认 / 回退 |
| --- | --- | --- |
| `bm25` | 仅 BM25 / FTS | 始终可用；solo 默认 |
| `bm25_judge` | BM25 + 可选 judge 重排 | judge 不可用 → BM25 + `decision_unavailable` |
| `event_entity` | 仅 event-entity 通道 | 未启用 → `event_entity_not_enabled` + BM25；抽取失败 → `event_extraction_unavailable` + BM25 |
| `hybrid` | BM25 + dense（若启用）+ event_entity（若启用）多路 RRF | 缺失通道按能力裁剪并声明；event 只提供候选 |
| `auto` | 按 backend 与能力解析 | solo SQLite → `bm25`；team / enterprise PG → `hybrid` |

`hybrid` 不要求 event 一定存在：未启用抽取时是 BM25 + dense；未启用 dense 时是 BM25 + event；两者都未启用时退化为 BM25，并按需在 `degraded[]` 标注。`auto` 不得把未启用的可选通道伪装成已启用。

## 双后端性能调优

同一逻辑查询由同一查询构造器生成，SQLite 与 PostgreSQL 允许不同物理计划；**不允许按 profile 分支查询语义**。

| 后端 | 索引与查询 | 验收 |
| --- | --- | --- |
| SQLite | `events` 文本用 FTS5 trigram；`entities(normalized_name)`；`event_entities(entity_id, event_id)`；预编译语句；避免 N+1；用 `LIMIT` 与 CTE 控制扩展 | `EXPLAIN QUERY PLAN` 显示使用 FTS / 索引；不出现对 `events` / `event_entities` 的全表扫描；种子与扩展查询均带 `tenant_id` / `space_id` / 可见性条件 |
| PostgreSQL | `events.tsv` 生成列 + GIN；`entities` 的 `lower(name)` btree，或可选 `pg_trgm`；`event_entities` 复合索引；CTE + `statement_timeout` | `EXPLAIN (ANALYZE, BUFFERS)` 显示 GIN / btree / 复合索引；无大表 Seq Scan；超时由 `statement_timeout` 截断并写 `degraded[]` |

调优目标是最小化种子查询与 1–2 跳扩展的扫描行数，而不是让两端使用相同 SQL 文本。SQLite 的 trigram 与 PostgreSQL 的 GIN/trgm 是同一逻辑能力的不同物理实现；查询构造器只负责语义与参数，索引选择由各后端计划器完成。

## 评测

- 在 P1 的 20 篇文档 + 20 条问题上，分别运行 `bm25`、`bm25_judge`、`event_entity`、`hybrid`、`auto`。
- 指标：Recall@5、MRR、P95 延迟；event 通道开启前后对比；记录抽取 token / 调用次数 / 失败率。
- 不引用 SAG 的公开数字作为本项目结论；SAG 数字只作为设计先验，最终以本项目评测集为准。
- 多跳问题必须同时看候选集覆盖率与引用回跳；event 通道不得改变 Answer 契约或引用锚点。

## 风险与待验证

| 风险 | 影响 | 缓解 / 验证 |
| --- | --- | --- |
| 抽取成本与延迟 | 离线 ingest 成本上升；LLM 费用与重试放大 | 批量、超时、有限重试；按 chunk 统计 token；默认关闭；与 BM25 解耦 |
| 实体歧义（同名异义） | 错误扩展，多跳召回变差 | `type` 约束、受控词表、归一化规则；评测集加入歧义样例 |
| 中文实体归一 | 分词、别名、简繁与英文缩写不一致 | 复用术语归一；trigram 兜底；记录归一失败样本 |
| 权限过滤位置 | 先扩展后过滤会泄漏存在性 | 种子与扩展都 join `events` 并调用 `Policy::visibility_from`；跨租户/空间探针为 0 |
| 候选爆炸 | 扩展事件过多，延迟与上下文成本上升 | 限制种子数、事件数、深度、超时；`LIMIT` 与 CTE；P95 回归 |
| 模型版本漂移 | event 分布变化，结果不可复现 | 记录 `extractor_version` / `extraction_model`；模型/提示词变化必须重抽取并跑评测 |

## 后果

**正面**

- 多跳 / 关系型召回多一个可选通道，不引入全局图维护。
- 默认 BM25 不变；event 抽取失败或未启用不影响基础检索。
- 同一逻辑 schema 在 SQLite / PostgreSQL 上复用，迁移与评测口径一致。

**负面**

- 多一套派生表、抽取任务与索引维护；event 质量依赖抽取质量。
- event 内容变化会产生新的 event / doc 版本，历史引用通过 `superseded_by` 与 `as_of` 解析。
- 双后端需要分别验证查询计划，不能只测一端。

## 验收标准

1. 默认 solo 配置（未启用抽取）下，`bm25` 路径与 P1 基线一致，不出现 event 相关降级。
2. 启用抽取后，`event_entity` 返回候选；抽取失败时 BM25 仍返回结果，`degraded[]` 含 `event_extraction_unavailable`。
3. 五种模式返回同一 Answer 字段集合；event 通道只影响候选，不改变引用锚点。
4. 种子与扩展查询均带 `tenant_id` / `space_id` / `Policy::visibility_from`；跨租户/空间探针命中为 0。
5. SQLite `EXPLAIN QUERY PLAN` 与 PostgreSQL `EXPLAIN (ANALYZE, BUFFERS)` 满足上表的索引验收。
6. 20+20 评测报告包含各模式 Recall@5、MRR、P95，以及 event 开启前后对比；不得用 SAG 数字代替本项目结果。

## 关联文档

- [document/architecture.md](../architecture.md) — L2/L3/L4、L6 抽取、数据模型
- [document/tier-matrix.md](../tier-matrix.md) — 通道矩阵、模式与降级码
- [document/roadmap.md](../roadmap.md) — P2/P3 交付与出口标准
- [document/sag-assessment.md](../sag-assessment.md) — SAG 评估与 clean-room 边界
- [ADR 0006](0006-doc-id-identity.md) — 内容敏感 doc_id 与历史引用
