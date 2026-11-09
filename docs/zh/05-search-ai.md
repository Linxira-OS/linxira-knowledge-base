---
title: 检索与 AI 回答
slug: 05-search-ai
lang: zh
order: 5
summary: 解释 bm25、bm25_judge、event_entity、hybrid、auto 五种模式、引用契约、AI 摘要与 knowledge_as_of。
---

# 检索与 AI 回答

检索是产品的中心。理解四件事就够了：请求了什么模式、实际用了什么模式、缺了什么能力、回答是静态摘要还是动态回答。它们分别对应 `mode_requested`、`mode_used`、`degraded[]` 与 `summary_kind`。

## 1. 五种检索模式

| 模式 | 依赖 | 行为 | 不可用时 |
| --- | --- | --- | --- |
| `bm25` | 关键词索引 | 始终可用的基础召回；SQLite 用 FTS5 + `bm25()`，PostgreSQL 用 `tsvector` + `ts_rank_cd` | 关键词索引不可用则写 `keyword_unavailable`，返回空证据 |
| `bm25_judge` | 关键词 + judge alias | 先召回，再让 judge 对前 30 条重排（重排窗口，不是召回上限）；候选集成员不变 | 写 `decision_unavailable`，保留原顺序 |
| `event_entity`（别名 `event`） | 抽取器 + 已建事件 | 先选种子实体，沿共享实体扩展 1–2 跳，聚合事件后映射回切片 | 未启用写 `event_not_enabled`；抽取不可用写 `event_extraction_unavailable`；无种子写 `event_no_seed`；超时写 `event_channel_timeout`；均回退关键词 |
| `hybrid` | 关键词 + 可选 dense + 可选 event + 可选 judge | 多路候选做 RRF 融合（`k = 60`，权重可配）；已配置 judge 时默认在融合后重排 Top 30 | 缺失通道按能力裁剪并写降级；两路都缺时只剩关键词；未配置 judge 不算降级 |
| `auto` | 后端与已配置能力 | 按规则解析成上面某一种 | 解析结果见下一节 |

CLI 与 HTTP 都接受这些值；`--mode event` 和 `--mode event_entity` 等价，`--mode keyword` 等价于 `bm25`。语言过滤用 `--lang` / `lang`，在召回 SQL 的 WHERE 中生效。

## 2. 模式如何解析

`auto` 的解析规则是确定的：

1. PostgreSQL 后端 → `hybrid`。
2. SQLite 且已配置 embedding provider，且当前可见范围内已有切片向量 → `hybrid`。
3. 否则，SQLite 且已配置抽取器、数据库中已有事件 → `hybrid`。
4. 其他情况 → `bm25`。

solo 默认配置是 `retrieval.mode = "auto"`，因此不配模型时走 `bm25`；配置 embedding provider 且已有索引向量即可进入 `hybrid`。显式请求某个模式时以请求为准。

注意：SQLite 的 `auto` 只探测调用方可见的租户/空间。若配置了 embedding provider 但没有可见的切片向量，则继续检查抽取器/events（或落到 `bm25`），并写入 `index_missing`，空的 dense 通道不会被静默忽略。PostgreSQL 的 `auto` 固定解析为 `hybrid`，缺 embedding 时 dense 通道降级、关键词检索仍可用。

## 3. 关键词通道

- SQLite 建 FTS5 索引，按 `bm25()` 排序，分数越小越靠前。
- PostgreSQL 建 `tsvector` 与 GIN 索引，按 `ts_rank_cd` 排序，分数越大越靠前；两个后端的分数量纲不可直接比较。
- 查询先取 `limit × 3` 条候选，再按空间、ACL、时序、语言过滤与截断。
- 当全文查询无法覆盖短查询或中文片段时，两端都可能退化为字面子串匹配，并写入 `keyword_exact_fallback`。这是显式降级，不会把字面命中伪装成 BM25 分数。

## 4. judge 重排（经模型网关）

`bm25_judge` 只改变顺序，不改变候选集成员。judge 通过 `retrieval.judge_alias`（默认 `default`）调用模型网关，输入查询与前 30 条候选的截断片段，返回同数量的分数；前 30 条是 judge 的重排窗口，不是召回上限。调用失败或数量不匹配写 `decision_unavailable`，保留原顺序。judge 的用量同样写入 `usage_events`，`source=judge`。

判别模型不能找回召回阶段漏掉的文档。如果候选集本身没有正确答案，重排不会创造答案。

## 5. event-entity 通道（可选）

event-entity 是可选通道，需要两件事同时成立：

1. 配置了抽取器（`[models.extractor]` 的 `provider` 不是 `none`；若设置了 `api_key_env`，该环境变量必须存在）。
2. 导入时抽取成功，数据库中存在未失效的 event 记录。

运行时行为：查询实体归一后选取种子实体，沿 `event_entities` 扩展 1 跳（`event_depth = 2` 时两跳），按共享实体权重与事件文本相关性聚合，最后映射回 `chunk_id`。种子与扩展都在 SQL 内先做租户、空间、ACL、时序与语言过滤。

事件只提供候选；Answer 契约与引用锚点不变。默认关闭，未启用时请求 `event` 写 `event_not_enabled` 并回退关键词；抽取失败写 `event_extraction_unavailable`，不影响关键词索引。

## 6. hybrid 融合

`hybrid` 把可用的多路候选按排名融合，使用 Reciprocal Rank Fusion（倒数排名融合）：

- 关键词一路始终参与（有结果时）。
- dense 一路只在显式配置 embedding provider 时参与：查询嵌入 + 已存切片向量的进程内余弦，候选上限 `max(limit × 20, 100)`。
- event 一路只在抽取器已配置且存在事件时参与。
- 每路贡献 `weight / (60 + rank)`；同一 `chunk_id` 多路命中会累加分数。

融合后，若 `retrieval.judge_after_hybrid = true`（默认）且已配置 judge，则对 Top 30 重排；候选集成员与尾部顺序不变。hybrid 不要求 judge，未配置时**不写降级**；已配置但调用失败写 `decision_unavailable`。设为 `false` 可保持纯 RRF 顺序。

dense 没有独立向量索引。未配置 embedding 写 `dense_not_enabled`；查询嵌入失败写 `embedding_unavailable`；没有切片向量写 `index_missing`。这些都不阻塞关键词结果。

## 7. 引用与回答

回答先执行一次检索，再取前 `min(5)` 条命中生成引用。没有配置 LLM 时：

- `mode = extractive`，正文由命中片段按 `[序号] path:起-止` 拼接。
- 每条引用带 `doc_id`、`chunk_id`、路径、行号区间与原文片段；`source_url` 当前不填充。

配置 `[models.llm]` 在网关中物化为 alias `default` 后，系统通过网关生成回答，要求返回严格 JSON：`{"answer": "...", "citations": [1,2]}`。校验规则：引用下标必须落在候选范围内且至少一个；重复去重；JSON 非法、无引用或越界写 `citation_alignment_failed` 并回退抽取式；调用失败写 `llm_unavailable`。生成成功时 Answer 增加 `model`（公开 alias）与 `usage`；`cache_hit` 恒为 `false`（答案缓存未实现）。

## 8. 统一入口与 SSE

`POST /api/v1/query` 是搜索与问答的统一入口，请求体字段为 `query`、`space`、`lang`、`conversation_id`、`mode`（`search` | `answer` | `both`，默认 `both`）。响应包含 `conversation_id`、`message_id`、`user_message_id`、`mode`、`rewritten_query`、`conversation_context_hash`、`degraded`、`results` 与 `answer`。

流式入口是 `POST /api/v1/query/stream` 与 `GET /api/v1/answer/stream`。SSE 事件顺序：

1. `meta`：`{conversation_id, message_id, mode, rewritten_query}`
2. `results`：检索结果（`mode=answer` 时为 `null`）
3. `token`（0..n）：`{"content": "<delta>"}`
4. `citation`（0..n）：每条引用一个事件
5. `usage`：token/成本（可能为 `null`）
6. `degraded`：降级数组
7. `done`：`{conversation_id, message_id}`

流式端点一旦开始输出就固定返回 HTTP 200；认证、CSRF、限流失败以 `event: error` 帧返回 `{code, message}`。客户端断开时网关释放配额预留并写 `status=cancelled` 的 usage；上游不支持流时回退非流式并写 `stream_fallback`。

## 9. 多轮改写

`[query]` 控制改写：`rewrite_enabled`（默认开）、`rewrite_alias`（默认 `default`）、`context_messages`（默认 6）。系统取会话最近 N 条消息拼成上下文，经网关改写查询；成功时把改写结果写入 usage 事件与消息的 `rewritten_query`。改写失败时回退原查询，并在 `degraded` 中写字符串码 `conversation_rewrite_failed`（不阻断回答）。

## 10. 静态摘要

静态摘要与动态回答分开：

- 触发：导入/抽取完成、Wiki revision 保存、seed 后写入 `summary_jobs`；`lkb serve` 启动后台 worker（每 5 秒一轮，批大小 `summary.batch`）。
- 生成：通过网关 alias `summary.model_alias`（默认 `default`），`usage_events.source=summary`；失败按 `summary.max_attempts` 重试，仍失败则写 `status=failed` 并保留抽取式摘要（`tldr`/`bullets`/`keywords`，`fallback: true`）。
- 查询：`GET /api/v1/pages/{space}/{slug}/summary?lang=`。就绪时返回摘要记录；未就绪时返回 `status:"pending"`、`summary:null`，并在 `degraded` 中给 `summary_generation_failed`。
- CLI：`lkb summary run --limit 16` 手动推进队列，`lkb summary list --status queued` 查看任务。

静态摘要不得当作实时回答；动态回答可以把它作为辅助线索，但引用仍指向原文 chunk。

## 11. knowledge_as_of

`knowledge_as_of` 取所有被引用文档 `valid_from` 的最小值，格式为 RFC3339 UTC。它回答“这组证据最早从什么时候开始生效”，不是“索引有多新”，也不是“答案有多可信”。`null` 表示不适用（通常是没有引用）。

## 12. 降级如何阅读

先看 `mode_used`，再看 `degraded[]`：

```text
mode: auto (used bm25)
degraded:
  - dense_not_enabled (evidence): semantic candidates are not available
```

常见组合：

| 现象 | 含义 |
| --- | --- |
| `keyword_exact_fallback` | 全文索引没命中，改用了字面子串匹配 |
| `decision_unavailable` | judge 未配置或经网关调用失败，排序是原始顺序 |
| `event_not_enabled` / `event_extraction_unavailable` / `event_no_seed` / `event_channel_timeout` | event 通道未启用/不可用/无种子/超预算，均回退其他通道 |
| `dense_not_enabled` / `embedding_unavailable` / `index_missing` | dense 通道未启用/嵌入失败/无向量 |
| `stream_fallback` | 上游不支持流式，回答被缓冲后一次返回 |
| `summary_generation_failed` | 静态摘要未就绪或生成失败，已回退抽取式 |
| `conversation_rewrite_failed` | 多轮改写失败，使用原查询（字符串码） |
| `acl_second_check_dropped` | 二次校验丢弃了候选；安全类，优先级最高 |

## 13. 相关文档

- [02 核心概念](02-concepts.md)
- [04 内置 Wiki](04-wiki.md)
- [07 配置参考](07-configuration.md)
- [11 常见问题与排错](11-faq.md)
