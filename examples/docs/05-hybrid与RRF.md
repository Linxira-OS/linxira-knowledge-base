# hybrid 与 RRF

## 启用条件

`--mode auto` 只在 embedding provider 可用时选择 hybrid，否则用 bm25。provider 必须显式配置 `provider`、`base_url`、`model`；`api_key_env` 指向的变量缺失时构造失败，能力进入降级，不会静默联网。

## 向量召回

hybrid 先对查询调用 `/embeddings`，再在治理过滤后的分块中取候选，候选上限为 `limit * 20` 且不少于 100。相似度是进程内余弦，按分数降序取前 limit 条。当前实现把向量存在分块行内，不依赖 sqlite-vec 或 pgvector；这是与架构目标的一处已知实现边界。

## RRF 融合

两路结果按排名融合，`RRF = Σ weight / (k + rank)`：两路权重均为 1.0，`k = 60`。融合只比较名次，不比较 BM25 与余弦的原始分数。相同分数按 `chunk_id` 升序稳定排序，保证重复查询结果一致。

## 降级码

显式选择 hybrid 但向量未启用时报 `dense_not_enabled`；已配置但调用失败报 `embedding_unavailable`；库里没有向量报 `index_missing`。三者都回退关键词，且不改变已有候选。
