# Retrieval and models / 检索与模型

> Scope: how much (if any) model infrastructure retrieval needs. The short
> answer: **none by default**; models are optional upgrades, and every missing
> capability degrades explicitly instead of blocking.
> 范围：检索需要多少模型基础设施。结论：**默认不需要**；模型是可选升级，缺失能力
> 显式降级，不阻塞使用。

## English

### Short answer

No model is required to run `lkb`. On SQLite, the default
`retrieval.mode = "auto"` resolves to `bm25` when neither an embedding provider
with stored vectors nor an extractor with events is available; a configured
embedding provider plus indexed vectors now enables `hybrid` automatically.
PostgreSQL resolves `auto` to `hybrid`. Answers fall back to extractive citations
when no LLM is configured. A solo install with zero model endpoints is a
supported, first-class configuration.

### Mode matrix

| Mode | Model dependency | Behavior | Fallback on missing capability |
| --- | --- | --- | --- |
| `bm25` | **None** | SQLite FTS5 `bm25()` / PostgreSQL `tsvector` + `ts_rank_cd` keyword retrieval | No fallback needed; this is the baseline |
| `bm25_judge` | Judge / rerank model only | BM25 recall, then judge reorders the recalled candidates (Top 30 rerank window, not a recall cap; the effective window follows `-k` / `top_k`) | Recall set unchanged; missing judge leaves BM25 order and writes `degraded[]` |
| `event` (alias `event_entity`) | Extractor model | Extracts events/entities at ingest, expands seeds by shared entities | Without extractor or events: `event_not_enabled` / `event_extraction_unavailable` / `event_no_seed`, falls back to keyword |
| `hybrid` | Embedding model (dense channel); extractor and judge optional | BM25 + dense + event fused with RRF (k=60, weights configurable), then an optional Top-30 judge rerank | Missing channels are pruned per capability and reported in `degraded[]`; keyword retrieval still returns results |
| `auto` | Resolved from configuration | SQLite: embedding + vectors → `hybrid`; else extractor + events → `hybrid`; else `bm25`. PostgreSQL: always `hybrid` | Same per-channel degradation rules; a configured embedding provider with no vectors writes `index_missing` |

**`auto` details.** On SQLite, `auto` probes the caller's visible tenant/spaces.
A configured embedding provider with at least one stored vector resolves to
`hybrid`; with no vectors it writes `index_missing` and falls through to the
extractor/events check, then `bm25`. A configured extractor with events also
resolves to `hybrid`. PostgreSQL always resolves to `hybrid`; without an
embedding provider the dense channel is reported as degraded and keyword
(BM25-equivalent) retrieval still works.

### What each model actually does

- **Embedding / vector model** is needed only for the dense channel of `hybrid`.
  It does not affect `bm25`, `bm25_judge`, or `event`. If it is not configured
  or the vector index is empty, the dense channel is skipped and the degraded
  code explains why; keyword results are still returned.
- **Judge / rerank model** only reorders candidates already recalled (Top 30
  rerank window, not a recall cap; the effective window follows `-k` /
  `top_k`). It never adds or removes candidates, so it cannot hide a document
  that BM25 found. It runs for `bm25_judge` and, when configured, after RRF in
  `hybrid` (`retrieval.judge_after_hybrid = true` by default). Turning it off
  restores plain BM25 or RRF ordering.
- **LLM** affects generative answers, summaries, and conversation query
  rewriting only. Without it, `lkb ask` / `/api/v1/answer` return the
  extractive answer with citations. Summaries that explicitly require an LLM
  fail or degrade visibly (`status=failed` plus extractive fallback); they are
  never silently marked ready.
- **Extractor** writes `events` / `entities` at ingest time. It is off by
  default and performs no network calls unless configured.

### Recommended setups

| Setup | Retrieval | Models |
| --- | --- | --- |
| Solo, offline / zero-cost | `auto` (resolves to `bm25`) | None |
| Solo with local embeddings | `auto` (switches to `hybrid` once vectors exist) or `hybrid` (explicit) | Local embedding model only |
| Team / enterprise | `hybrid` or `bm25_judge` | Evaluate judge and LLM per workload; measure before enabling |

### Operational notes

- Capability gaps are always reported in `degraded[]` with stable codes; a
  missing model never produces a silent success or a hard failure of retrieval.
- Identity / ACL failures are fail-closed and are **not** part of the degraded
  path.
- `lkb config check` prints which retrieval capabilities are active. See
  `docs/zh/05-search-ai.md` and `docs/en/05-search-ai.md` for end-user steps.

---

## 简体中文

### 结论

运行 `lkb` 不需要任何模型。SQLite 上，既没有带向量的 embedding provider、也没有
抽取器与 events 时，默认 `retrieval.mode = "auto"` 解析为 `bm25`；配置 embedding
且已有索引向量后，`auto` 会自动进入 `hybrid`。PostgreSQL 的 `auto` 解析为
`hybrid`。未配置 LLM 时回答自动使用抽取式引用。零模型端点的 solo 安装是受支持的
一等配置。

### 模式矩阵

| 模式 | 模型依赖 | 行为 | 能力缺失时的降级 |
| --- | --- | --- | --- |
| `bm25` | **无** | SQLite FTS5 `bm25()` / PostgreSQL `tsvector` + `ts_rank_cd` 关键词检索 | 无需降级，这是基线 |
| `bm25_judge` | 仅决策/重排模型 | BM25 召回后，judge 对已召回候选重排（Top 30 是重排窗口，不是召回上限；实际窗口由 `-k` / `top_k` 决定） | 候选集不变；缺 judge 时保留 BM25 顺序并写 `degraded[]` |
| `event`（别名 `event_entity`） | 抽取器模型 | 摄入时抽取事件/实体，查询时沿共享实体扩展种子 | 无抽取器或无 events 时写 `event_not_enabled` / `event_extraction_unavailable` / `event_no_seed`，回退关键词 |
| `hybrid` | embedding 模型（dense 通道）；extractor 与 judge 可选 | BM25 + dense + event 三路 RRF（k=60，权重可配），可选再做 Top 30 judge 重排 | 缺失通道按能力裁剪并写入 `degraded[]`；关键词通道仍返回结果 |
| `auto` | 按配置能力解析 | SQLite：embedding + 已有向量 → `hybrid`；否则 extractor + events → `hybrid`；否则 `bm25`。PostgreSQL：固定 `hybrid` | 同样的按通道降级规则；配置了 embedding 但无向量写 `index_missing` |

**`auto` 的细节。** SQLite 上的 `auto` 只探测调用方可见的租户/空间。配置了
embedding provider 且至少有一条已存向量时解析为 `hybrid`；没有向量时写
`index_missing`，继续检查 extractor/events，最后落到 `bm25`。配置了抽取器且已有
events 也会解析为 `hybrid`。PostgreSQL 固定解析为 `hybrid`；缺 embedding 时 dense
通道记为降级，关键词（BM25 等价）检索仍可用。

### 各模型的真实作用

- **Embedding / 向量模型**只服务 `hybrid` 的 dense 通道，不影响 `bm25`、
  `bm25_judge` 或 `event`。未配置或向量索引为空时跳过 dense 通道，并由 degraded
  code 说明原因；关键词结果照常返回。
- **决策 / 重排模型（judge）**只对已召回候选重排（Top 30 是重排窗口，不是召回
  上限；实际窗口由 `-k` / `top_k` 决定），不增加也不删除候选，因此不会隐藏 BM25
  已找到的文档。`bm25_judge` 会使用；`hybrid` 在配置了 judge 时也默认于 RRF 后
  重排（`retrieval.judge_after_hybrid = true`）。关闭后恢复普通 BM25 或 RRF 顺序。
- **LLM** 只影响生成式回答、摘要与多轮查询改写。没有 LLM 时，`lkb ask` /
  `/api/v1/answer` 返回带引用的抽取式回答。显式依赖 LLM 的摘要会失败或可见降级
  （`status=failed` + 抽取式 fallback），不会静默标记为 ready。
- **抽取器（extractor）**在摄入时写入 `events` / `entities`；默认关闭，未配置时
  不发起任何网络请求。

### 推荐配置

| 场景 | 检索 | 模型 |
| --- | --- | --- |
| solo，离线 / 零成本 | `auto`（解析为 `bm25`） | 无 |
| solo + 本地 embedding | `auto`（有向量后自动切换 `hybrid`）或显式 `hybrid` | 仅本地 embedding 模型 |
| team / enterprise | `hybrid` 或 `bm25_judge` | 按负载评估 judge 与 LLM，先测量再开启 |

### 运维要点

- 能力缺口始终以稳定 code 写入 `degraded[]`；模型缺失既不会静默成功，也不会让
  检索整体硬失败。
- 身份 / ACL 失败 fail-closed，**不属于**降级放行路径。
- `lkb config check` 会打印当前生效的检索能力。用户操作步骤见
  `docs/zh/05-search-ai.md` 与 `docs/en/05-search-ai.md`。
