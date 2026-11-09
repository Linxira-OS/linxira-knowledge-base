# 种子内容（Seed Content）

> 状态：设计契约
> 适用范围：随软件分发的第一套产品文档数据库 / Wiki 内容。
> 对照基线：[architecture.md](architecture.md)、[tier-matrix.md](tier-matrix.md)、[roadmap.md](roadmap.md)。
> 边界：本文只定义 `docs/` 产品文档如何进入内置知识库；`document/` 下的架构、ADR、roadmap 仍为内部文档，不随产品分发。

## 1. 目标

首次启动零配置即可体验一个可检索、可阅读的内置知识库。内容来自仓库 `docs/zh` 与 `docs/en`，两种语言的页面 slug 一一对齐；导入到 `space = product`，用户无需先上传文件或配置模型。

| 目标 | 约束 |
| --- | --- |
| 零配置体验 | 首次启动即可在 `product` 空间看到产品文档，不要求 LLM、Embedding、外部数据库或网络 |
| 双语对齐 | `docs/zh` 与 `docs/en` 使用同一组 `slug`；`title` / `summary` 按语言本地化 |
| 可检索 | 文档按既有 Document / Chunk 管线编译，BM25 默认可查；语言过滤可用 |
| 可编辑 | 用户编辑走内置 Wiki 的 Page / Revision；种子不覆盖用户修改 |
| 可升级 | 新版本只追加新页面或新 Revision，不删除用户内容；`seed_version` 可追踪 |
| 不膨胀产品边界 | 种子只含 `docs/` 产品文档；内部架构与 ADR 不进入种子 |

## 2. 内容布局与 frontmatter

目录布局：

```text
docs/
├── zh/
│   ├── 00-index.md
│   ├── 10-architecture/
│   │   └── overview.md
│   └── 20-concepts/
│       └── retrieval.md
└── en/
    ├── 00-index.md
    ├── 10-architecture/
    │   └── overview.md
    └── 20-concepts/
        └── retrieval.md
```

- `zh/` 与 `en/` 是语言根；`lang` 必须与所在目录一致。
- 目录名与文件名只用于组织与排序提示；页面身份以 frontmatter 的 `slug` 为准。
- 两种语言中相同 `slug` 视为同一逻辑页面的两个语言版本；不要求内容逐句对应，但主题与结构应一致。
- 不允许同一语言内出现重复 `slug`。

frontmatter 字段：

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `title` | string | 是 | 该语言的页面标题 |
| `slug` | string | 是 | 跨语言对齐的稳定标识；只允许小写字母、数字、`-`、`/` |
| `lang` | enum | 是 | `zh` / `en`；必须与目录一致 |
| `order` | integer | 是 | 同层级导航排序；数值小的在前 |
| `summary` | string | 是 | 一句话摘要；用于列表、搜索摘要与 Agent 预览 |

正文使用 Markdown；frontmatter 之外的正文进入 Document / Chunk。`slug` 变更视为新页面身份，旧引用通过 `doc_alias` 或页面重定向处理；种子升级不得静默改 slug。

## 3. 种子机制契约

| 项 | 契约 |
| --- | --- |
| 命令 | `lkb seed` 或 `lkb init --seed` |
| 目标空间 | `space_id = product`；solo 使用 `allow_all` ACL；team / enterprise 使用 `product` 空间的默认 ACL |
| 内容来源 | 编译期嵌入（如 `include_dir`）或随包附带 `docs/` 目录；两种方式都必须离线可用 |
| 导入单位 | 每个 Markdown 文件编译为一个 Page / Revision，再派生 Document / Chunk；`slug` 来自 frontmatter |
| 来源标记 | Revision 的 `author = system:seed`，`source_url = seed://{lang}/{slug}`，`source_revision = seed_version` |
| 幂等 | 同一 `doc_id` 已存在则跳过；重复执行不产生重复 Document / Chunk |
| 失败处理 | 单页失败写 warning；整体失败写 `degraded[]` 的 `seed_import_failed`；不阻塞进程启动，不删除已导入内容 |
| 用户修改 | 页面已有非 `system:seed` 的 Revision 时，种子跳过该页，不覆盖、不合并、不删除用户内容 |
| 删除语义 | 种子导入不删除任何页面；新版本移除的页面保留原状并标记为历史，不物理删除 |
| 版本 | `seed_version` 写入 `seed_meta`；升级只追加新页面或新 Revision |
| 边界 | 只读取 `docs/`；不读取 `document/`、ADR、examples、tests 或运行时代码 |

`system:seed` 是内置服务主体，不出现在用户列表中；只用于标识种子 Revision 的作者与来源。用户编辑产生的 Revision 使用真实 `principal_id`。

种子导入流程：

1. 扫描 `docs/zh` / `docs/en`，校验 frontmatter、`lang` 与 `slug` 唯一性；不合法页面跳过并记录 warning。
2. 对每个页面计算目标 `page_id` / `slug`；若页面不存在，创建 Page 与首个 Revision。
3. 若页面已存在且只有 `system:seed` Revision：同一 `doc_id` 跳过；内容变化则追加新 Revision，旧 Revision 标记 `superseded`，`superseded_by` 指向新 ID。
4. 若页面已存在且有用户 Revision：跳过该页；记录 skip 计数与 warning，不覆盖用户内容。
5. 页面被用户删除（tombstone）时，种子不复活该页面；记录 skip。
6. 写入 `seed_meta.seed_version`、`source_manifest_hash`、`applied_at`；输出 imported / skipped / failed 计数。

## 4. 语言

| 项 | 契约 |
| --- | --- |
| 元数据 | Document 与 Chunk 都带 `lang`；从 frontmatter 写入，Chunk 从 Document 继承 |
| 检索过滤 | `QueryPlan.filters.lang` 可选；指定时在召回阶段过滤，不在结果后置剔除 |
| `auto` | 跟随查询语言检测或用户设置；无法判断时不过滤，并在 trace 记录 `lang_auto` |
| UI 切换 | 只切换展示语言与默认 `lang` 过滤；不复制索引、不重建文档、不改变 `doc_id` |
| 跨语言回退 | 指定 `lang` 时严格过滤，不自动混入其他语言；需要跨语言时由调用方显式去掉 `lang` 过滤 |
| slug 对齐 | 同一 `slug` 的 zh / en 是两个 Document / Page，不是同一文档的翻译副本；引用按语言定位 |

语言过滤只影响候选集，不改变 Answer 契约；引用仍指向具体语言的 `doc_id + revision_id + offsets`。

## 5. 版本与升级

| 项 | 契约 |
| --- | --- |
| `seed_version` | 构建时生成的不透明字符串（例如日期 + 构建号）；写入 `seed_meta`，不写入用户可见正文 |
| `source_manifest_hash` | `docs/` 清单与 frontmatter 的哈希；用于判断是否需要执行升级 |
| 升级行为 | 只追加新页面或新 Revision；内容变化的 seed Revision 通过 `superseded_by` 指向新 ID；不删除用户内容 |
| 用户内容 | 用户 Revision 优先；种子不覆盖、不合并、不物理删除 |
| `doc_id` | 规则不变：`sha256(canonical_path + "\0" + content_hash)` 前 16 字节 hex；同一内容同一路径稳定，内容变化生成新 ID（ADR 0006） |
| 回滚 | 回滚只切换应用版本与 `seed_version` 指针，不回滚用户 Revision；旧 seed Revision 仍可按 ID 读取 |

`seed_meta` 逻辑字段：

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `tenant_id` | string | 是 | 租户 |
| `space_id` | string | 是 | 固定 `product` |
| `seed_version` | string | 是 | 当前已应用的种子版本 |
| `source_manifest_hash` | string | 是 | 种子清单哈希 |
| `applied_at` | timestamp | 是 | 最近一次成功应用时间 |
| `page_count` / `revision_count` | integer | 是 | 已导入页面与 Revision 计数 |

唯一键：`(tenant_id, space_id)`。

## 6. 验收

| 编号 | 验收项 | 验证方法 | 通过标准 |
| --- | --- | --- | --- |
| SC1 | 双语首启 | 干净数据目录首次启动，执行 `lkb seed` 或 `lkb init --seed` | `product` space 中 `zh` / `en` 各 ≥ 10 篇；页面 slug 对齐 |
| SC2 | 中文检索 | 用中文问题检索 | 命中 `lang=zh` 的对应页面；引用可回跳 |
| SC3 | 英文检索 | 用英文问题检索 | 命中 `lang=en` 的对应页面；引用可回跳 |
| SC4 | 幂等 | 连续执行两次 seed | 第二次 imported=0、skipped=N、failed=0；Document / Chunk 数量不变 |
| SC5 | 不覆盖用户修改 | 编辑某 seed 页面生成用户 Revision，再执行 seed | 用户 Revision 保留；该页 skipped；无覆盖、无删除 |
| SC6 | 失败不阻塞 | 注入单页解析失败 / 整体导入失败 | 进程正常启动；warning 或 `seed_import_failed` 可见；BM25 仍可用 |
| SC7 | 离线 | 断网、无外部模型、无外部数据库 | seed 成功；不发起外部请求 |
| SC8 | 边界 | 检查种子清单与导入日志 | 只包含 `docs/`；不包含 `document/`、ADR、examples、tests |
| SC9 | 版本升级 | 用新 `seed_version` 执行 seed | 只追加新页面 / 新 Revision；用户内容不被删除；`seed_meta` 更新 |
| SC10 | 身份稳定 | 同一内容重复 seed，比较 `doc_id` 集合 | 集合不变；内容变化时新 ID 生成，旧 ID 保留 |

## 7. 与 document/ 内部文档的边界

- 种子只含 `docs/` 产品文档；`document/architecture.md`、ADR、tier-matrix、roadmap、build-policy 不随产品分发。
- 若产品文档需要解释代码组件逻辑，由产品文档作者在 `docs/10-architecture/` 写面向用户的摘要；不直接复制内部 ADR。
- `seed_version` 与 `docs/` 清单属于产品构建产物；内部文档版本不参与种子版本计算。
- 种子导入失败不得回退到内部文档，也不得把 `document/` 作为隐式 fallback。

## 8. 关联文档

- [document/architecture.md](architecture.md) — L6 种子导入、Document/Chunk `lang`、SeedMeta
- [document/tier-matrix.md](tier-matrix.md) — 语言过滤与种子内容的三档差异
- [document/roadmap.md](roadmap.md) — P1/P2 交付与出口标准
- [document/adr/0006-doc-id-identity.md](adr/0006-doc-id-identity.md) — 内容敏感 `doc_id` 与历史引用
