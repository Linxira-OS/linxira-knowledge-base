# FTS5 与中文 trigram

## 索引形态

SQLite 迁移脚本创建 `chunks_fts` 虚拟表，分词器为 `tokenize = 'trigram'`。trigram 让中文这类无空格文本可以按三字符子串匹配，不需要额外分词词典。

## 查询构造

查询先按字符类别切段：拉丁字母、数字与 `_ - . / @ +` 保留为整词；CJK 连续串长度超过三时展开为重叠三元组，例如“混合检索”得到“混合检”“合检索”。所有词项加引号并用 OR 连接，单个词缺失不会丢弃整个查询。

## 打分与回退

FTS5 的 `bm25()` 越小越相关，召回 SQL 按 `score ASC` 排序。若 MATCH 没有返回任何行，内核改用 `instr(c.body, ?) > 0` 的子串回退，命中记 `exact_fallback = true`，分数为 0，并在响应中写 `keyword_exact_fallback` 降级。

短于三个字符的中文查询可能无法被 trigram 命中，这正是回退路径存在的原因。FTS5 在构建中不可用时，检索写入 `keyword_unavailable`，关键词证据为空。
