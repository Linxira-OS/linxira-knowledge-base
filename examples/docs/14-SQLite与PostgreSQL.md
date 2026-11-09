# SQLite 与 PostgreSQL

## 同一抽象

`Store` trait 由 `SqliteStore` 与 `PostgresStore` 同时实现，逻辑 schema 相同，迁移脚本分别放在 `migrations/sqlite/` 与 `migrations/postgres/`。检索流程、RRF、重排顺序与 Answer 契约不因后端改变。

## 关键词打分

SQLite 用 FTS5 的 `bm25()`，索引分词器为 trigram，分数越小越相关。PostgreSQL 用 `tsvector`、GIN 索引、`websearch_to_tsquery('simple', ...)` 与 `ts_rank_cd`，按分数降序；它不是 BM25。把两者当作同一种打分会导致阈值与排序预期错误。

## 共同回退

两边都在全文查询无行时切换到子串匹配：SQLite 用 `instr`，PostgreSQL 用 `strpos`，命中标记 `exact_fallback` 并写 `keyword_exact_fallback`。回退只保证可用性，不参与 BM25 排名。

## 过滤与运维

可见性条件由 `Policy::visibility` 按方言生成占位符，租户、空间、有效期与 ACL 条件完全一致。PostgreSQL 后端要求 `url_env` 指向的环境变量已设置，否则启动校验失败；集成测试读取 `LKB_TEST_PG_URL`，未设置时跳过并标注未验证。

关系库后端只有这两种，不提供 SQL Server 或其他第二适配器。
