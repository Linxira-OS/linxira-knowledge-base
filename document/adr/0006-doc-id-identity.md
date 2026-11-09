# ADR 0006：doc_id 采用内容敏感口径

- **状态**：Accepted
- **日期**：2026-11-08

## 背景

`architecture.md` §5.8 早期写法是 `genesis_hash + base32(26 字符)`，并声明内容更新不回写 `genesis_hash`、`doc_id` 不随内容变化。仓库 README 与实际 Rust 实现采用另一套口径：`sha256(canonical_path + "\0" + content_hash)` 的前 16 字节 hex。两套规则不能同时成立：

- 早期规则需要持久化“首个版本”的 `genesis_hash`；仅从源文件与当前内容无法重建该状态，跨档迁移或索引重建在缺少注册表时会漂移。
- 实现规则是内容敏感的：同一路径与同一内容得到同一 `doc_id`；内容变化生成新 `doc_id`。它与“文件是事实源、索引可重建”一致，也不需要额外的隐藏状态。

本 ADR 以仓库实现与验收口径为准，冻结内容敏感规则。`architecture.md` §5.8 及重复该规则的文档同步修改。

## 决策

**`doc_id` 由规范化逻辑路径与内容哈希共同派生，内容变化生成新 ID；旧 ID 保留为历史身份，不重写引用。**

派生规则（规范化版本 `p1`）：

- `canonical_path`：源根目录下的相对路径；`/` 分隔、无前导 `./`、丢弃 `.` 与空组件、拒绝 `..`、拒绝绝对路径与 Windows 盘符；ASCII 小写。`p1` **不做 Unicode NFC 归一化**，这是已知边界，同一路径的不同 Unicode 组合可能得到不同 `doc_id`。
- `content_hash`：源字节的 SHA-256 小写 hex；Wiki 页面取规范化 Markdown 字节的 SHA-256。
- `doc_id = lowercase_hex(sha256(canonical_path + "\0" + content_hash)[0..16])`，即 32 个小写 hex 字符，无前缀。
- `revision_id` 与 `chunk_id` 是各自独立的派生 ID，不参与 `doc_id` 计算；Wiki 页面 revision 使用 `rev_` 前缀，切片使用 `chk_` 前缀，均为 16 字节 hex。

身份语义：

- 同一 `canonical_path` + 同一 `content_hash`：在任何重建、导出/导入、跨档迁移之后得到同一 `doc_id`。
- 内容变化：生成新 `doc_id`；旧 Document 保留，`status = superseded`，`superseded_by` 指向新 `doc_id`；旧 revision 仍可按旧 `doc_id` 读取。
- `as_of` 历史查询：返回当时有效的旧 `doc_id`，不得把历史结果重写成新 `doc_id`。
- 文件移动 / 重命名：`canonical_path` 变化 → 新 `doc_id`；通过 `doc_alias` 记录旧 ID 到新 ID，引用解析跟随别名。
- 同一内容出现在不同路径：按路径区分 `doc_id`，不做内容去重合并。
- Wiki 页面：`canonical_path = wiki/{space_id}/{page_slug}.md`；同一内容重复导入保持同一 ID；内容不同的新 Revision 生成新 `doc_id`，通过 `page_id` / `page_revision_id` 与 `superseded_by` 保持逻辑连续。
- 跨档迁移：只有在源字节与 `canonical_path` 均未变化时，迁移前后 `doc_id` 集合才应完全一致；若迁移改写了内容（重新抽取、格式转换），必须报告 ID 差异并建立 `superseded_by` 链，不得声称“ID 不变”。

## 备选方案与为何不选

| 方案 | 否决理由 |
| --- | --- |
| `genesis_hash + base32(26)`（早期架构写法） | 需要持久化首个版本状态；仅凭源文件无法重建；迁移/重建在缺少注册表时会漂移；与实现不一致 |
| 自增主键或随机 UUID | 不可从源内容重现；重建与迁移后引用、评测标注、ACL 绑定全部失效 |
| 只用逻辑路径 | 内容替换后仍沿用旧 ID，引用指向错误内容，历史不可追溯 |
| 只用内容哈希 | 相同内容出现在多个路径时无法区分，治理归属会互相覆盖 |

## 后果

**正面**

- 身份可从源内容独立重现，不依赖隐藏的 genesis 状态；索引重建与跨档迁移可验证。
- 内容变化被显式表达为新 ID，旧 ID 保留，历史引用仍可解析。
- 与 `superseded_by`、`as_of`、`doc_alias` 组合后，既能表达“同一逻辑文档的演进”，也能表达“移动/重命名”。

**负面**

- 每次内容修改都会新增 `doc_id`；以“最新 ID”为书签的客户端需要通过 `page_id` / `current_revision_id` 或 `superseded_by` 跟随。
- 文档 ID 数量随 revision 增长；注册表与别名表需要保留历史映射。
- `p1` 不做 Unicode NFC；同一路径的不同 Unicode 组合可能得到不同 ID，这是已记录的边界，修复需要新规范化版本与迁移。

**需要监控的信号**

- 同一 `canonical_path` 下 `doc_id` 的变更频率；异常高频说明写入或规范化不稳定。
- 重建 / 导出导入后 `doc_id` 集合差异；在内容未变时应为 0。
- `superseded_by` 断链数量；目标为 0。
- `as_of` 查询解析失败的旧 `doc_id` 数量；目标为 0。
- `doc_alias` 覆盖率：移动 / 重命名但缺少别名的数量；目标为 0。

## 验收标准

1. 同一路径、同一内容在两次全量重建后得到相同 `doc_id` 集合。
2. 同一数据集导出后导入空库，未改写内容时 `doc_id` 集合完全一致。
3. 修改文件内容后重新摄入：生成新 `doc_id`；旧 Document 的 `superseded_by` 指向新 ID；旧 ID 仍可按 `as_of` 或显式 ID 读取。
4. Wiki 页面保存新 Revision 后：新 `doc_id` 生成，旧 Revision 的 `doc_id` 与引用仍可解析。
5. 除本 ADR 的历史背景外，全仓库不再出现 `genesis_hash`、`base32(26)` 或 `doc_` 前缀作为 `doc_id` 规则的表述。

## 关联文档

- [document/architecture.md](../architecture.md) — §5.8 doc_id 派生与稳定性
- [document/oss-landscape.md](../oss-landscape.md) — Wiki 导入的 doc_id 口径
- [README.md](../../README.md) — 实现口径与验收说明
- [ADR 0003](0003-files-are-source-of-truth.md) — 文件是事实源、索引可重建
- [ADR 0002](0002-governance-fields-day-one.md) — `version` / `superseded_by` 的治理语义
