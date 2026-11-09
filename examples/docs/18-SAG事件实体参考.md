# SAG 事件实体参考

## 它是什么

SAG 是一个 event-entity 的 SQL-RAG 参考实现：chunk 是原文证据边界，event 是完整语义单元，entity 是索引与扩展点。查询时用 SQL 沿共享实体连接局部 event，形成当前查询的动态超边，不维护全局知识图谱。

## 与本项目的关系

它是独立的外部参考实现，不持有本项目的 tenant、space、ACL、有效期与稳定 doc_id，也不输出 Answer 契约或 `degraded[]`。P1 只把它当参考，不引入依赖、不复制代码、不改变默认路径。

event 抽取依赖 LLM，会破坏 solo 的离线默认；ACL 过滤也必须由外层注入，不能依赖引擎。

## 后续可能

P2/P3 可在 Rust 内核做 event/entity/event_entity 表的 spike，作为可选多跳通道接入现有召回与重排，并在自有评测集上与 BM25、hybrid、图谱 L0 对比。未达到引用回跳与 ACL 不回归前，不进入默认。

具体版本与许可信息见 knowledge-freshness 台账。
