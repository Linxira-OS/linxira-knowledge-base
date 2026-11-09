# 档位 × 能力矩阵

> 配套文档：[architecture.md](architecture.md)
> 适用范围：solo / team / enterprise 三档的默认能力、降级形态、基础设施替换、治理行为、资源目标与验收标准。
> 本文件只用表格与必要说明，不重复架构图。

## 0. 阅读约定

| 记号 | 含义 |
| --- | --- |
| 默认状态 | 档位启动后的初始配置；可被部署方覆盖，但覆盖必须通过启动校验 |
| L0–L3 | 单个能力域的实现等级；L0 是最小可用降级实现，不是“关闭”或空壳 |
| 关闭 | 该能力在该档位默认不启用；请求依赖它时返回 `*_not_enabled` |
| — | 该档位不提供该能力，且不属于默认路径 |
| 目标 / 假设 | 未实测的工程目标或容量假设，必须由实现阶段的基准测试校准 |
| `degraded[]` | Answer 契约中的降级数组；正常时为空数组，不是缺失字段 |

降级条目必须带 `category`：`evidence`、`quality`、`security`、`performance`。优先级为 `security` > `evidence` > `quality` > `performance`。缓存等只影响性能的组件也要上报，但归入 `performance`，人类界面可折叠。

## 1. 能力等级定义

| 能力域 | L0 | L1 | L2 | L3 |
| --- | --- | --- | --- | --- |
| 检索通道 | FTS5 BM25 Top 30 + SQL 点查；向量默认关闭；无图谱、无多模态 | L0 + 本地 ONNX 嵌入 + 精确向量（hybrid，RRF）；仍单机 | 服务化向量 + 关键词 hybrid；单租户；图谱 / 多模态可选 | 分布式多路并行；图谱 / 多模态全量；按租户分片 |
| 重排 | BM25 / RRF 顺序 + 规则加权（时效、来源角色、精确匹配）；无模型 | 判别模型（decision model）证据重排；或本地轻量 Cross-Encoder | Cross-Encoder 服务 | 判别模型 + Cross-Encoder + 预算门控的 LLM 择优 |
| 治理 | 单租户 allow-all 策略对象；治理字段齐全；召回过滤照常执行 | 单租户 + 空间级 ACL + 本地多主体 | OIDC + 组 ACL + 集中审计 | 多租户 + 源 ACL 同步 + 策略引擎 + 合规审计 |
| 多模态 | 文本层、alt text、文件元数据；图片 / 音视频不入索引 | 本地 OCR + 图片图注 | OCR + ASR + 图片向量 | OCR + ASR + VLM + 视频关键帧 + 跨模态检索 |
| 规模 | 单进程、单文件；≤ 5 万切片 | 单机多进程；≤ 20 万切片 | 单租户集群；≤ 200 万切片 | 分布式多租户；> 200 万切片，平台可到 2 亿 |
| 生成 | 抽取式选句 + 模板 | 本地小模型生成（可选） | LLM 服务 | LLM + 忠实度校验 + 多语言 / 多模型路由 |
| 评测 / 审计 | 本地固定集 + 本地追加日志 | 本地指标面板 + 结构化日志 | CI 回归 + 集中日志 | 持续评测 + 租户分层 + WORM 审计 + SIEM |

等级是“实现深度”，不是“功能开关数量”。同一等级在不同档位可以有不同实现，但必须满足同一行为契约：过滤在召回阶段、降级必须上报、Answer 字段完整。

## 2. 能力矩阵

### 2.1 检索通道

| 通道 | solo 默认 | solo 降级形态 | team 默认 | team 降级形态 | enterprise 默认 | enterprise 降级形态 |
| --- | --- | --- | --- | --- | --- | --- |
| 稠密向量 | 关闭；hybrid 可选 sqlite-vec 精确余弦，≤ 3 万切片 | 默认关闭，查询要求语义时 → `dense_not_enabled`；hybrid 启用后模型缺失 → `embedding_model_missing`；索引缺失 → `index_missing` | L2：pgvector HNSW，hybrid 默认开启 | 索引不可用 → 精确扫描 + `vector_degraded_exact`；超时 → `dense_timeout` | L3：分布式 HNSW / IVF-PQ，按租户分片 | 单分片失败 → `vector_partial`；全不可用 → 仅关键词 + `dense_unavailable` |
| 稀疏关键词 | L0：SQLite FTS5 BM25 Top 30，默认主通道 | FTS5 不可用 → 纯 solo 无结果 + `keyword_unavailable`；hybrid 时仅稠密 + `keyword_unavailable` | L2：PostgreSQL 全文检索，hybrid 默认开启 | 搜索引擎不可用 → 稠密 + `keyword_unavailable` | L3：ES / OpenSearch，按租户隔离 | 集群不可用 → 稠密 + `keyword_unavailable`；降级不改变 ACL 过滤 |
| event-entity | 默认关闭；配置抽取后才可用；默认 `bm25`，显式切 `event_entity` 才进入 | 未启用 → `event_entity_not_enabled`；抽取失败 → `event_extraction_unavailable`；均回 BM25 | 可选；抽取启用的空间作为 hybrid 第三路；PG 索引与计划见 ADR 0007 | 未启用不参与 hybrid，不报错；显式请求 → `event_entity_not_enabled`；失败 → `event_extraction_unavailable` | 可选；按租户/空间配额启用；只提供候选 | 租户禁用 → `event_entity_not_enabled`；抽取失败不阻塞其他通道 |
| 结构化 | L0：SQLite SQL，仅显式结构化表 | 未配置 → `structured_not_configured`；查询失败 → `structured_unavailable` | L2：PostgreSQL，只读账号 | 连接失败 → `structured_unavailable`，不猜数值 | L3：联邦查询 / 数仓只读视图 | 源不可用 → 最近快照 + `structured_unavailable`，标注时效 |
| 原文深链 | L0：本地文件路径 + 行号 / 页码 | 文件缺失 → `source_original_missing` | L2：对象存储签名地址 + 偏移 | 对象缺失 → `source_original_missing` | L3：源系统深链 + ACL 校验 | 源不可达 → `source_original_missing`；无权 → 不返回链接并记 `acl_unknown` |
| 多模态 | L0：文本层、alt text、文件元数据；默认关闭视觉通道 | 请求依赖图片 / 音频 → `multimodal_not_enabled` | L2：OCR + 图片图注 + 图片向量，可选 | 模型不可用 → 文本层 + `multimodal_unavailable` | L3：OCR + VLM + ASR + 关键帧，默认开启 | 单模态失败 → `multimodal_partial`；全失败 → `multimodal_unavailable` |

### 2.2 重排与生成

| 能力 | solo 默认 | solo 降级形态 | team 默认 | team 降级形态 | enterprise 默认 | enterprise 降级形态 |
| --- | --- | --- | --- | --- | --- | --- |
| RRF 融合 | 默认不启用（单通道 BM25）；hybrid 时启用，权重偏关键词 | 单通道无需融合；hybrid 融合异常属内核错误，不静默改走单通道 | 启用；标准权重 | 同左 | 启用；按通道可靠性调权 | 同左 |
| 判别模型证据重排 | 关闭；可选 L1 Jev，对 BM25 Top 30 重排 | 未配置 → `decision_not_enabled`（仅在策略要求时）；不可用 / 超时 → `decision_unavailable`，回 BM25 顺序 | 可选 L1 / L2；用于证据判断 | 不可用 → `decision_unavailable`，回 Cross-Encoder 或 RRF 顺序 | L3：默认用于证据判断，再交 CE / LLM | 不可用 → `decision_unavailable`，回 Cross-Encoder 顺序 |
| Cross-Encoder 精排 | 关闭；可选 L1 本地 ONNX | 未配置 → `rerank_not_enabled`（仅在查询要求精排时）；加载失败 → `rerank_unavailable` | L2：ONNX 服务，Top 50 | 服务不可用 → RRF 顺序 + `rerank_unavailable`，置信度上限降为 medium | L3：GPU / CPU 池，Top 50 | 池不可用 → RRF 顺序 + `rerank_unavailable`，记录容量事件 |
| LLM 列表择优 | 关闭 | 请求要求时 → `llm_rerank_unavailable` | L2：可选，按空间预算门控 | 预算超限 → `llm_rerank_unavailable`，保留 CE 结果 | L3：高价值意图默认，按租户预算 | 超预算 / 不可用 → `llm_rerank_unavailable`，保留 CE 结果 |
| LLM 生成 | 关闭；抽取式 + 模板 | 请求要求生成时 → `llm_unavailable`，仍返回抽取式答案 | L2：可选，按空间配置 | 超时 / 配额 → `llm_unavailable`，回抽取式 | L3：默认开启 | 不可用 → `llm_unavailable`，回抽取式并降低置信度 |
| 忠实度校验 | 不适用；抽取式天然引用原文 | — | L2：可选，抽样或全量 | 不可用 → `verification_unavailable`，置信度上限 medium | L3：默认开启 | 不可用 → `verification_unavailable`，生成模式降级或回抽取式 |

#### 2.2.1 决策/判别模型事实与选型证据

| 项 | 事实与口径 |
| --- | --- |
| 代表实现 | TypeSafe AI 的 Jev；作为参考组件，不排除其他同契约实现 |
| 输入 / 输出 | 输入 state + 类型化问题；输出 `choice`、`score`、`noul` 三种原语，以及校准置信度（RLCD 训练） |
| 生成能力 | 不做自回归生成；不能替代生成模型 |
| 多模态与上下文 | Jev 1.14 版本实验性支持图片输入、Box 输出与 262k 长上下文 |
| 价格 | 官网口径：$0.084 / 百万输入 token，输出免费（2026-11-08，可能变化） |
| 延迟 | 公开口径 70–500 ms；以部署与网络实测为准 |
| 定位 | 对已召回候选做证据判断与重排；放在融合之后、生成之前 |
| 能力边界 | 只改善“召回到了但没排上去”；不能找回召回阶段漏掉的文档；不能替代召回通道或生成模型 |

社区自测证据（非官方 MTEB；来源 `aifabrice/jev-rag`，MIT，独立社区项目）：

| 配置 | BEIR NFCorpus nDCG@10 | 相对 BM25 Top 30 |
| --- | --- | --- |
| BM25 Top 30（基线） | 0.305654 | — |
| BM25 Top 30 + Jev | 0.353235 | +0.047581 |
| 无向量 Agentic + Jev | 0.430969 | +0.125315 |
| Hybrid + Jev | 0.444327 | +0.138673 |

这些数字只用于说明“判别模型能提升排序、但仍以召回结构为基础”。它们不是官方 MTEB 结果，也不构成对其他数据集的性能承诺。上线前必须用本项目评测集复测。

### 2.3 多模态

| 能力 | solo 默认 | solo 降级形态 | team 默认 | team 降级形态 | enterprise 默认 | enterprise 降级形态 |
| --- | --- | --- | --- | --- | --- | --- |
| 图片 OCR / 图注 | L0：仅文本层与 alt text；本地 OCR 可选 | 无 OCR / 无文本层 → `multimodal_not_enabled` | L1 / L2：OCR + 图注，可选 | OCR 服务失败 → 文本层 + `multimodal_unavailable` | L3：OCR + VLM 图注，默认开启 | 单模型失败 → `multimodal_partial`；图注与 OCR 都不足 → `multimodal_unavailable` |
| 音频 ASR | 关闭 | 请求依赖音频 → `multimodal_not_enabled` | L2：可选 ASR | ASR 不可用 → `multimodal_unavailable`，保留音频元数据 | L3：默认 ASR，带时间戳 | ASR 超时 → `multimodal_partial`，返回已转写片段并标注 |
| 视频关键帧 | 关闭 | 请求依赖视频 → `multimodal_not_enabled` | 关闭；仅提取音轨转写可选 | 未配置 → `multimodal_not_enabled` | L3：关键帧 + 音轨 ASR + 时间戳 | 关键帧失败 → `multimodal_partial`；整段不可处理 → `multimodal_unavailable` |
| 跨模态检索 | 不支持 | `multimodal_not_enabled` | L2：以文搜图可选 | 向量服务不可用 → `multimodal_unavailable` | L3：以文搜图 / 搜音频 / 搜视频 | 单模态索引缺失 → `multimodal_partial`，不伪造命中 |

### 2.4 图谱

| 能力 | solo 默认 | solo 降级形态 | team 默认 | team 降级形态 | enterprise 默认 | enterprise 降级形态 |
| --- | --- | --- | --- | --- | --- | --- |
| 实体关系遍历 | L0：SQLite 邻接表，1 跳，默认关闭 | 多跳查询 → `graph_not_enabled`；可用元数据过滤替代，但必须声明 | L1：PostgreSQL 递归查询，1–2 跳，默认关闭 | 同左，`graph_not_enabled` | L3：图数据库，默认开启 | 图库不可用 → 1 跳元数据扩展 + `graph_unavailable`，不伪造多跳 |
| 社区摘要 | 关闭 | `graph_not_enabled` | 关闭 | `graph_not_enabled` | L3：按主题簇预计算摘要 | 摘要缺失 → `graph_unavailable`；回退到逐条检索并声明覆盖不足 |

### 2.5 缓存、评测与审计

| 能力 | solo 默认 | solo 降级形态 | team 默认 | team 降级形态 | enterprise 默认 | enterprise 降级形态 |
| --- | --- | --- | --- | --- | --- | --- |
| 语义缓存 | 默认关闭；hybrid 后可用 SQLite 查询向量精确余弦 + TTL；默认只做归一化查询哈希的结果缓存 | 缓存不可用 → 绕过 + `cache_unavailable`（performance）；默认模式不冒充语义缓存 | L2：Redis 语义缓存 | Redis 不可用 → 绕过 + `cache_unavailable`，不阻塞请求 | L3：分层缓存，按租户隔离 | 单层失效 → 回源 + `cache_unavailable`；不得跨租户复用 |
| 结果缓存 | 启用；按 ACL 指纹与索引代次失效 | 同左 | 启用 | 同左 | 启用，按租户分区 | 同左；缓存故障不改证据集 |
| 评测回归 | L0：本地固定集，手动或定时 | 无门禁；结果记录在本地 | L2：CI 回归，发布门禁 | 评测服务不可用 → 阻断发布，不放行 | L3：按租户持续评测 + 灰度 + 自动回滚 | 评测不可用 → 冻结发布，保留旧代次 |
| 在线反馈 | 本地记录点赞 / 纠错 | 反馈丢失不影响回答，但记录失败事件 | 集中收集，进入候选评测集 | 同左 | 按租户收集，审计可追溯 | 同左；不得直接用于自动调权 |
| 查询审计 | L0：本地追加日志，默认 30 天 | 磁盘满 → `audit_degraded`，回答继续 | L2：集中日志，默认 90 天 | 日志后端不可用 → 本地缓冲 + `audit_degraded` | L3：WORM + SIEM，1–3 年按策略 | 严格模式 → 审计写入失败即拒绝回答；非严格模式 → `audit_degraded` |
| 内容审计 | 记录 Page / Revision、文档变更与 claim 状态 | 本地文件损坏 → 从源重建并告警 | 集中记录 Page / Revision、版本与 claim 变更 | 同左 | Page / Revision 不可篡改版本链 + 双源指纹对账 | 对账失败 → `source_original_stale` 并进入复核队列 |

### 2.6 检索模式

| 模式 | solo | team / enterprise | 说明 |
| --- | --- | --- | --- |
| `bm25` | 默认；始终可用 | 可用 | 不依赖抽取或向量；安全回退 |
| `bm25_judge` | 可选；BM25 后接 judge | 可选 | judge 不可用 → `decision_unavailable`，回 BM25 |
| `event_entity` | 仅配置抽取后可用 | 仅抽取启用时可用 | 未启用 → `event_entity_not_enabled`；失败 → `event_extraction_unavailable`；均回 BM25 |
| `hybrid` | 可选；BM25 + dense 和/或 event | PG 默认；按已启用通道融合 | 缺失通道按能力裁剪并声明；event 只提供候选 |
| `auto` | embedding 有可见向量或配置抽取且已有 events 时解析为 `hybrid`，否则 `bm25` | 解析为 `hybrid` | 按 backend 与能力选择，不静默伪装 |

部署默认：solo 为 `bm25`；team / enterprise 为 `auto`（解析为 `hybrid`）。显式请求优先于默认值；任何降级都进入 `degraded[]`。模式语义、抽取与双后端调优见 [ADR 0007](adr/0007-event-entity-retrieval.md)。

### 2.7 语言与种子内容

| 能力 | solo | team | enterprise | 约束 / 降级 |
| --- | --- | --- | --- | --- |
| `lang` 元数据与过滤 | Document / Chunk 均带 `lang`；按请求语言过滤；UI 切换只改展示与默认过滤 | 同左；空间可设默认语言 | 同左；按租户/空间策略 | 召回阶段过滤，不复制索引；指定语言默认严格过滤，回退必须显式声明 |
| 种子内容 | 首次启动导入 `docs/zh` + `docs/en` 到 `product`；allow-all ACL；离线可用 | 同左；`product` 空间使用默认 ACL；可关闭自动 seed | 同左；按租户/空间策略；可关闭自动 seed | 幂等；不覆盖用户 Revision；失败写 warning / `seed_import_failed`，不阻塞启动 |
| 种子版本 | `seed_meta.seed_version`；升级只追加新页面 / 新 Revision | 同左 | 同左；可按租户记录 | 不删除用户内容；`doc_id` 规则不变 |

种子布局、frontmatter 与验收细节见 [seed-content.md](seed-content.md)。

### 2.8 摘要、统一入口与会话

| 能力 | solo | team | enterprise | 约束 / 降级 |
| --- | --- | --- | --- | --- |
| 静态文章摘要 | 异步 job；无 LLM 时抽取式；按 revision 缓存 | 异步 LLM + 抽取式回退；批量/限流 | 同左；按租户配额与保留策略 | 不阻塞摄入；失败 `summary_generation_failed`；旧 revision 摘要保留 |
| 动态检索/问答总结 | 抽取式或可选 LLM；支持多轮；SSE 可选 | LLM + SSE；多轮改写 | 同左；按租户模型策略 | 缓存键含 `conversation_context_hash`；引用/citations 不变 |
| 会话历史 | 本地 owner；search-only 也写入 | owner/space 隔离；可归档/删除 | 同左 + admin 审计元数据 | 不存 CoT；删除 tombstone；不跨租户 |
| 统一入口 | Wiki 顶栏 + 右侧面板共用 query/conversation | 同左 | 同左 + 深链 `?q=&conversation=` | 一个入口，不维护第二套检索/回答状态 |

静态摘要异步生成，不纳入查询 P95；动态回答沿用生成首 token/SLO。

### 2.9 空间自助、用户组与存储配额

| 能力 | solo | team | enterprise | 约束 / 降级 |
| --- | --- | --- | --- | --- |
| 用户组 | 不启用；单本地 Principal | tenant 级 groups/group_members；组可拥有空间 | 同左 + 按租户策略/外部组同步后续 | 至少一个 group_admin；跨 tenant 拒绝 |
| 空间自助 | 默认允许创建自建空间/笔记本；组创建仍关闭；仍受 `spaces.max_per_user` 与默认配额约束 | `allow_user_creation=true`；每用户空间数/默认配额 | 同左，按租户/组配额 | 超限 429/413；solo 保持零运维默认，不强制开组 |
| 空间成员/角色 | 单 owner；可选 viewer | space_owner/space_admin/editor/viewer；组映射 | 同左 + 审计强度更高 | 显式用户角色优先；组取 max；最后 owner 保护 |
| 存储配额 | 本地磁盘软提示/可关 | tenant→group→space→user；storage/documents/chunks/members/spaces | 同左，按租户配额 | 80/95% 告警；hard 阻断；与 token 配额分开 |
| 共享/可见性 | private 本地 | private 默认；internal 可发现元数据 | 同左 + 租户策略 | 读正文仍需成员/分享；链接分享 P2 |

### 2.10 运营模式、限速、支付与 i18n

| 能力 | solo | team | enterprise_internal | enterprise_external |
| --- | --- | --- | --- | --- |
| 运营模式 | 单机个人 | 企业内部多用户 | 企业内部大组织 | 对外出售 SaaS |
| 注册/多租户 | 关闭 | 关闭/单租户 | 多租户 | 公开注册/多租户 |
| 计费/支付 | 关闭 | 可关 | 内部配额 | 套餐/支付/发票/dunning |
| SSO | 关闭 | 可选 | 默认 | 默认/按套餐 |
| 连接器/MCP/公网 API | 关闭 | 可关 | 可选 | 按套餐 |
| 限速 | 宽松 | 租户/用户/token | 租户/用户/token | 按套餐/合同 |
| i18n | zh/en | zh/en | zh/en | zh/en + 扩展语言/RTL |

启动校验：enterprise_external 必须 billing+multi_tenant+public_api；solo 不得开 billing；审计关闭仅显式风险接受。

### 2.11 桌面 APP、solo+PG 与笔记

| 能力 | solo | team | enterprise | 说明 |
| --- | --- | --- | --- | --- |
| 交付形态 | Tauri APP（installer/portable）、CLI、浏览器 | 服务 + Web/CLI | 服务/容器 + Web/CLI | 桌面壳只属个人版；企业不装壳 |
| 存储后端 | SQLite 默认；可选 PostgreSQL | PostgreSQL | PostgreSQL | solo+PG 仍单用户/loopback/allow-all；team/ent 禁止 SQLite |
| Web 托管 | Rust 二进制托管 web/dist，同源 | 同左或外部 nginx | 外部 nginx/网关 | Tauri/浏览器/portable 单 sidecar |
| 笔记 | 私有笔记（我的笔记）；可自建空间/笔记本，组创建关闭 | 每人私有笔记空间；可发布到团队空间 | 同左；按空间角色 | 笔记复用 Wiki Page/Revision；桌面 Markdown vault 未实现；UI 统一 |
| 首启/更新 | 向导 SQLite/PG、凭据管理、签名更新 | 配置/迁移 | 配置/迁移/HA | 便携模式密钥保护 |

## 3. 基础设施替换表

| 组件 | solo | team | enterprise | 端口契约 | 不可替换的语义 |
| --- | --- | --- | --- | --- | --- |
| 向量库 | 默认不部署；hybrid 时 SQLite + sqlite-vec，精确 KNN，单文件 | PostgreSQL + pgvector，HNSW | 分布式向量库，HNSW / IVF-PQ，按租户分片 | upsert、delete、search（带租户 / 空间 / ACL 过滤、TopK） | 过滤必须与检索同一次执行；缺失租户拒绝 |
| 关键词索引 | SQLite FTS5 | PostgreSQL 全文检索或 ES / OpenSearch | ES / OpenSearch 集群 | BM25 检索、过滤、分页 | 过滤在检索内完成；分数量纲不与其他通道直接比较 |
| event-entity 索引 | SQLite FTS5 trigram + `entities(normalized_name)` + `event_entities(entity_id, event_id)` | PostgreSQL `events.tsv` GIN + `lower(name)` btree / `pg_trgm` + `event_entities` 复合索引 | 分布式后端沿用同一逻辑索引，按租户/空间分片 | 种子与扩展 SQL 带租户、空间、可见性条件 | 先过滤再扩展；不同后端只换计划，不换查询语义 |
| 图库 | SQLite 邻接表（可选） | PostgreSQL 递归 CTE（可选） | 图数据库（Neo4j / Nebula 等） | 邻居扩展、路径查询、按租户过滤 | 不返回跨租户边；多跳结果必须可回溯到文档 |
| 关系库 | SQLite 单文件 | PostgreSQL | HA PostgreSQL | 事务、文档注册表、ACL、claim、审计索引；同一逻辑 schema 与 Store 抽象 | 文档注册表与别名不可丢；租户键非空；SQL Server 不提供适配器 |
| 对象存储 | 本地文件系统 | S3 / MinIO | S3 + 版本化 + WORM | 不可变写入、读取、签名深链 | 原件只读；快照不可被覆盖；深链需 ACL 校验 |
| 缓存 | 进程内 LRU + SQLite 结果缓存；hybrid 后加查询向量表 | Redis | Redis Cluster / 分层缓存 | get / put / invalidate，按命名空间隔离 | 键必须含租户、ACL 指纹、索引代次、`as_of` |
| 任务队列 | 进程内任务表 | PostgreSQL 任务表或 Redis 队列 | Kafka / RabbitMQ / SQS | 至少一次投递、幂等消费、死信 | 编译任务幂等；同一 `revision_id` 不重复建索引 |
| 模型运行时 | 默认不需要；hybrid / 本地重排时用 ONNX Runtime | ONNX 服务 / LLM 网关 | 模型服务集群 + 网关 | 嵌入、重排、生成、OCR / ASR 的统一调用 | 模型版本进入索引代次与 Answer trace |
| 判别模型 | 默认关闭；可选外部 API（如 Jev） | 可选 API 或自托管 | 证据重排服务，按租户配额 | 类型化问题 + 候选 → `choice` / `score` / `noul` + 校准置信度 | 只改排序与选择，不生成正文；失败可回退 BM25 / RRF / CE |
| 身份 | 本地主体 + 本地令牌 | OIDC | SAML / OIDC 联邦 + SCIM + 工作负载身份 | 认证、主体解析、组查询 | Agent 独立主体；不允许共享人类账号 |

### 3.1 solo 可选 hybrid 的向量存储选型：sqlite-vec 而非 LanceDB

| 维度 | sqlite-vec | LanceDB | 对本项目的影响 |
| --- | --- | --- | --- |
| 部署形态 | SQLite 扩展，单文件 | 独立嵌入式列式库 | solo 要求单文件、零服务，sqlite-vec 更简 |
| 事务一致性 | 与元数据同库同事务 | 独立写入路径 | 避免文档状态与向量双写不一致 |
| ACL 过滤 | 同一条 SQL 内完成 | 需应用层协调过滤 | 召回期过滤更可靠，少一次回表 |
| 备份 | 复制一个文件 | 单独目录与版本 | 备份与恢复步骤更少 |
| ANN 能力 | 以精确 KNN 为主 | 提供 ANN 索引 | solo 的 3 万切片不需要 ANN，精确检索更准 |
| 适用规模 | 万级切片 | 更大规模 | 超过 5 万切片时升级到 team 的 pgvector，而不是在 solo 硬撑 |
| 迁移成本 | 索引重建即可，`doc_id` 不变 | 同左 | 不锁定，选型可替换 |

**决策：solo 默认只用 SQLite FTS5 BM25；启用 hybrid 时，向量存储选 sqlite-vec，不选 LanceDB。** 默认路径不需要向量库；hybrid 的 3 万切片以内也不需要 ANN，单文件事务与 ACL 同库过滤的价值大于列式与 ANN。LanceDB 的第二套存储、双写一致性与备份复杂度，在这个规模不足以抵消。重新评估触发：启用 hybrid 后切片超过 5 万、查询 P95 超过 1.5 秒、或需要在 solo 内做跨模态向量检索。

### 3.2 其他选型决策

| 决策 | 选择 | 理由 | 被否方案 | 重新评估触发 |
| --- | --- | --- | --- | --- |
| solo 嵌入模型 | 默认不启用；hybrid 时用 bge-m3，低内存用 multilingual-e5 小尺寸 | 默认路径不需要嵌入；hybrid 才支付模型体积与计算成本 | 默认本地大模型；云嵌入 API | 用户启用 hybrid；目标语言变化或模型体积超出设备预算 |
| solo 关键词 | SQLite FTS5 | 与元数据同库、零额外进程 | Elasticsearch / Tantivy | 切片超过 5 万或需要复杂分析器 |
| solo LLM | 默认不启用 | 零外部依赖、零 API 成本；抽取式可引用 | 默认云 LLM；默认本地大模型 | 用户显式启用本地 LLM，且设备满足内存目标 |
| solo 判别模型 | 默认关闭；可选 Jev 对 BM25 Top 30 重排 | 无需 GPU 与向量库，先解决“召回到了但没排上去”；不引入默认外部依赖 | 默认开启外部 API；默认本地大模型 | 评测显示排序增益不足、调用成本或 500 ms 延迟不可接受、或离线要求 |
| solo 图谱 | 默认关闭，保留 1 跳邻接表 | 个人知识库的多跳需求低频，图库运维成本不成立 | 内置图数据库 | 出现稳定的多跳评测集且准确率不足 |
| solo 缓存 | 默认归一化查询哈希 + TTL 结果缓存；hybrid 后加 SQLite 查询向量精确余弦 | 无 Redis；默认不依赖嵌入，启用 hybrid 后再支付语义缓存成本 | 默认纯向量语义缓存 | 查询量上升到需要跨进程缓存 |
| team 向量 | pgvector | 与元数据同库，事务与 ACL 过滤简单 | 独立向量库 | 切片超过 200 万或 P95 超过目标 |
| enterprise 图 | 图数据库按需启用 | 多跳与社区摘要需要原生图能力 | 关系库递归查询 | 多跳深度超过 3 跳或图规模超出关系库预算 |

### 3.3 数据库选型

| 项 | 决策 |
| --- | --- |
| solo | SQLite 单文件；个人零运维；与 FTS5 同库，hybrid 时同库挂 sqlite-vec |
| team / enterprise | PostgreSQL；承载并发、多租户、JSONB、GIN / tsvector，可选 pgvector |
| 逻辑 schema | 两档完全一致：治理字段、版本、有效期、claim、审计字段、event/entity/event_entities 与 seed_meta 同一套定义；差异只在物理类型与索引实现 |
| Store 抽象 | 同一组 Store 端口；档位只改配置与适配器，领域逻辑不感知数据库类型 |
| 跨档迁移 | export / import 逻辑数据与源文件；索引重建；`doc_id` 与引用不变 |
| 明确排除 | SQL Server 不提供适配器；与全开源技术栈、现有运维与跨档迁移路径不一致 |

档位只改变配置与适配器，不改变逻辑 schema；任何“solo 少字段、企业多字段”的设计都视为缺陷。

## 4. 治理矩阵

| 治理项 | solo | team | enterprise | 失败模式 / 约束 |
| --- | --- | --- | --- | --- |
| 身份认证 | 本地主体或本地令牌；操作系统用户是边界 | OIDC 登录；人类账号 + 服务账号 | SAML / OIDC 联邦 + SCIM + 工作负载身份 | 认证失败是 401，不写 `degraded[]`；Agent 不得共享人类账号 |
| Principal 类型 | `human` 为主；Agent 用本地令牌 | `human` / `agent` / `service` | 三类齐全；Agent 有归属人与独立凭据 | 缺少 Agent 主体会导致审计无法追责 |
| 租户 | 固定 `t_local`；机器即物理边界 | 单租户；多空间隔离 | 多租户；所有键含 `tenant_id`；敏感租户可独立存储与密钥 | 查询缺 `tenant_id` 必须拒绝；禁止跨租户检索 |
| 空间 | 固定 `s_default`；可增加空间但无多用户 ACL | 空间级成员与组授权 | 空间 + 内置 Wiki ACL + 外部源 ACL + 策略引擎 | 空间是权限边界；`collection` 不是权限边界 |
| Wiki 写路径 | 最小 Wiki：space / page / revision、Markdown、只读历史、`If-Match` 冲突 409 | 内置 Wiki + 外部 Wiki 连接器导入 Page / Revision | 内置 Wiki + 外部 Wiki 导入；外部源 ACL 权威 | 不嵌入外部 Wiki 运行时；不做 CRDT / 评论 / 审批 |
| ACL 模型 | 显式 `allow_all` 策略对象；`acl` 非空 | 用户 / 组 allow list；空间默认 ACL | 内置 Wiki ACL + 外部源 ACL 镜像 + allow list + 拒绝优先；双源取交集 | `NULL` 不等于放行；未知 ACL 按拒绝处理 |
| ACL 执行点 | 召回 SQL / 通道内过滤；L4 二次校验 | 召回过滤 + L4 复核 | 向量 / 搜索预过滤 + L4 权威 ACL 复核 | 禁止先取回再删除；索引滞后时收紧而非放宽 |
| 双源 ACL | 配对文档取交集；未配对内置页面用 Page ACL | 配对文档取交集；未配对内置页面用 Page ACL | 内置 Wiki Revision 与原件快照强制交集；原件 ACL 不可用时拒绝 | 转写层权限宽于原件层是泄漏路径 |
| 时序有效性 | 字段写入；召回过滤；API 支持 `as_of`；无 UI | UI 展示有效期；`as_of` 查询；冲突提示 | 源生效日期、回溯修正、保留与法律保留 | `valid_to` 为空表示未知，不表示永久有效 |
| 版本 / 替代 | Page Revision 与 Document `version` 各自递增；旧修订 `superseded`；跨文档 `superseded_by` | 同左；界面展示 Page Revision 链与文档版本链 | 同左 + 双源指纹对账 + claim 冲突 | 覆盖删除会破坏审计与引用 |
| 删除 / 保留 | 删除 Page / Revision 或源文件后重建索引；本地备份可删 | Page / Revision 软删除 + 保留策略；索引清除 | Page / Revision 受治理删除、法律保留、按租户保留、加密擦除 | WORM 生效后不能直接物理删除 |
| 审计 | 本地追加日志，默认 30 天；无防篡改 | 集中日志，默认 90 天；查询文本可脱敏 | WORM，1–3 年按策略；SIEM 导出；哈希链 | 企业严格模式审计失败即拒绝回答；其余档位缓冲并上报 |
| 模型台账 | 本地配置文件记录模型文件版本与可选判别模型标识 | 集中登记嵌入、重排、判别、生成模型版本 | 版本审批 + 灰度 + 按租户记录；变更进入审计 | 只改配置不留痕会导致结果不可复现 |
| 加密 | 依赖磁盘加密；无应用级密钥 | TLS + 存储加密 + 密钥管理 | 按租户密钥、KMS / HSM、轮换、加密擦除 | 跨租户共用密钥会使隔离失效 |
| Agent 身份 | 本地令牌，权限等同本机用户 | 独立服务账号，按空间授权 | 工作负载身份，短时凭据，权限不超过归属人 | Agent 令牌不得长期有效或跨租户复用 |

## 5. 资源与 SLO

### 5.1 规模假设

| 指标 | solo | team | enterprise |
| --- | --- | --- | --- |
| 租户数 | 1 | 1 | ≤ 20（平台），可扩展 |
| 主体数 | 1–3 | 20–200 | 1,000–50,000 |
| 文档量 | ≤ 2,000（10³ 量级） | ≤ 100,000（10⁵ 量级） | ≤ 10,000,000（10⁷ 量级，平台）；单租户 ≤ 1,000,000 |
| 切片量 | ≤ 30,000 | ≤ 2,000,000 | ≤ 200,000,000（平台）；单租户 ≤ 20,000,000 |
| 并发查询 | 1–2 | 20–50 | 500–2,000 |
| 持续 QPS | 0.2 | 10 | 200–1,000 |
| 日摄入文档 | 50 | 1,000 | 50,000 |
| 附件容量 | ≤ 5 GB | ≤ 500 GB | ≤ 50 TB |
| 索引新鲜度 | 摄入后立即可查 | ≤ 15 分钟 | ≤ 5 分钟 |
| 可用性 | 单进程，无 SLA | 99.5% | 99.9% |
| RPO / RTO | 依赖源目录备份；手动恢复 | 15 分钟 / 1 小时 | 5 分钟 / 30 分钟 |

规模列是设计目标量级，不是实测结论。文档量对齐 README 的 10³ / 10⁵ / 10⁷ 量级；切片量按平均每文档 15–20 个切片估算，实际以分块器与内容类型为准。

### 5.2 延迟目标

| 阶段 | solo | team | enterprise | 说明 |
| --- | --- | --- | --- | --- |
| 认证与主体解析 | ≤ 10 ms | ≤ 20 ms | ≤ 30 ms | 本地 / OIDC / 联邦 |
| 查询改写与术语归一 | ≤ 50 ms | ≤ 150 ms | ≤ 150 ms | solo 规则改写；企业含术语服务 |
| 查询嵌入 | —（默认不启用）；hybrid ≤ 300 ms | ≤ 150 ms | ≤ 100 ms | CPU ONNX / 服务化 / 批处理 |
| 多路召回 | ≤ 150 ms（FTS5 BM25） | ≤ 400 ms | ≤ 500 ms | 不含重排 |
| event_entity 查询（可选） | ≤ 300 ms | ≤ 500 ms | ≤ 500 ms | 目标；受种子 / 扩展事件 / 深度 / 超时限制 |
| RRF 融合 | —（默认单通道）；hybrid ≤ 20 ms | ≤ 30 ms | ≤ 50 ms | 内核内计算 |
| TopN 重排 | 可选判别模型 ≤ 500 ms；未启用为 0 | ≤ 300 ms | ≤ 400 ms | solo Top 30；team / enterprise Top 50 |
| 生成首 token | 抽取式 ≤ 300 ms | ≤ 1,500 ms | ≤ 1,500 ms | LLM 流式 |
| 端到端 P95 | ≤ 0.8 s（默认）；≤ 1.2 s（启用判别模型） | ≤ 2.5 s | ≤ 3.0 s | 目标 / 假设，非实测 |
| ACL 过滤额外开销 | ≤ 10 ms | ≤ 50 ms | ≤ 100 ms | 预过滤 + L4 复核 |

### 5.3 存储估算

假设：嵌入维度 1024，float32 每向量 4 KB，int8 每向量 1 KB；元数据按每切片 1 KB 估算；对象存储按平均文件大小与快照倍数估算；总占用含 1.3 倍索引与元数据开销。以下为算术估算，不是实测。

| 数据 | 计算 | solo | team | enterprise |
| --- | --- | --- | --- | --- |
| 向量 float32 | 切片数 × 4 KB | 默认 0；hybrid 30k → 约 120 MB | 2M → 约 8 GB | 200M → 约 800 GB |
| 向量 int8 | 切片数 × 1 KB | 默认 0；hybrid 30k → 约 30 MB | 2M → 约 2 GB | 200M → 约 200 GB |
| 元数据 / 切片文本 | 切片数 × 1 KB | 约 30 MB | 约 2 GB | 约 200 GB |
| 对象存储 | 原件 + 快照 | 约 5 GB | 约 500 GB | 约 50 TB |
| 总占用（含 1.3 倍） | 上述合计 × 1.3 | 默认约 6.5 GB；hybrid 约 6.7 GB | 约 660 GB | 约 66 TB |

### 5.4 成本约束

| 成本项 | solo | team | enterprise |
| --- | --- | --- | --- |
| 外部 API | 默认 0；启用判别模型后按输入 token 计费（官网口径 $0.084 / 百万输入 token，输出免费）；本地 LLM 无 API 费用 | LLM / 判别模型可选；按空间配额封顶 | 按租户配额与预算封顶；超限降级而非透支 |
| event 抽取 | 默认 0；启用 LLM 后按 chunk 批量计费 | 按空间配额；超时重试计入 | 按租户配额；抽取成本单独归因 |
| 基础设施 | 复用现有笔记本 / 工作站 | 单区域服务 + 托管数据库；固定月度预算 | 多租户集群；按租户归因，存储分层 |
| 运维人力 | 0 | 兼职运维 | 专职 SRE / 平台团队 |
| 存储 | 本地磁盘 | 对象存储 + 备份 | 热 / 温 / 冷分层；快照与审计单独计量 |
| 出口流量 | 无 | 内部网络为主 | 跨区 / 跨租户导出单独计量 |
| 单位成本 | 不统计 | 每次回答 token 与存储成本 | 每 1,000 次回答成本、每租户成本，按周回看 |

### 5.5 升级信号

| 信号 | solo → team | team → enterprise | 动作 |
| --- | --- | --- | --- |
| 切片量 | > 50,000 | > 2,000,000（设计上限的 90% 预警） | 先评估分块与去重；确需扩容则升级 |
| 文档量 | > 3,000 | > 100,000 | 升级档位，不在原档位硬撑 |
| 并发查询 | > 3 | > 100 | 升级并重新压测 |
| 端到端 P95 | > 1.2 s（启用判别模型后） | > 3.0 s | 先查索引与重排预算；持续超限则升级 |
| 语义 / 跨语言召回 | hybrid 后评测缺口仍 > 2 个百分点 | 多语言成为主路径 | 先做 hybrid 对照；仍不足再升级 |
| 判别模型延迟 / 成本 | 单次 > 500 ms 或调用成本超预算 | 按租户预算 | 关闭或换本地重排，不因此升级档位 |
| 租户数 | 不适用 | > 1 | team 不允许多租户，必须 enterprise |
| ACL 来源数 | 不适用 | > 3 个源系统 | 引入策略引擎与同步管道 |
| 审计保留 | > 30 天 | > 1 年 | 升级到 WORM + SIEM |
| 索引新鲜度 | 需要分钟级 | 需要 5 分钟内 | 引入增量编译与队列 |
| 全量重建时间 | > 30 分钟 | > 4 小时 | 分片、增量重建或升级 |
| 降级率 | 持续 > 5% | 持续 > 2% | 先修组件；若容量导致则升级 |

升级信号必须联合判断：单看文档数会误判（大量小文件切片少），单看 QPS 会漏掉重建窗口与存储增长。达到任一硬阈值即进入评估，不自动升级。

## 6. 验收标准

### 6.1 跨档不变量

| 编号 | 验收项 | 验证方法 | 通过标准 |
| --- | --- | --- | --- |
| X1 | Answer 契约完整 | 对三档各抽取 100 条回答做 schema 校验 | 必填字段 100% 存在；`degraded` 始终为数组 |
| X2 | 文档身份稳定 | 重建索引、升级分块器、迁移档位后比较 `doc_id` 集合 | 集合完全一致；历史引用可解析 |
| X3 | 降级显式 | 逐项注入组件故障 | 每个故障都有对应 `code`；无静默回退 |
| X4 | 治理字段非空 | 全表扫描 Document / Chunk / Claim | `tenant_id`、`space_id`、`acl`、`valid_from`、`version` 无空值 |
| X5 | ACL 在召回阶段 | 用无权主体查询并检查通道日志与候选集 | 无权内容未进入候选集；引用与深链不泄露 |
| X6 | 评测口径一致 | 同一评测集在三档运行 | 指标定义与计算方式一致；差异可解释 |
| X7 | 判别模型不改变召回集 | 对比启用 / 关闭判别模型时的候选集成员 | 候选集成员不变，仅顺序与选择变化；召回漏项不被判别模型掩盖 |
| X8 | event 通道可选且隔离 | 关闭抽取跑 `bm25`；显式请求 `event_entity`；注入抽取失败 | 默认无 event 降级；未启用报 `event_entity_not_enabled`；失败报 `event_extraction_unavailable`，BM25 仍通过 |

### 6.2 solo 验收

| 编号 | 验收项 | 验证方法 | 通过标准 |
| --- | --- | --- | --- |
| S1 | 离线可用 | 断网、无 Docker、无外部服务的干净机器；导入 500 份 Markdown 与 20 份 PDF | 可启动、可检索、可回答；不发起外部网络请求 |
| S2 | 零外部依赖 | 检查进程、端口与模型加载 | 只有本进程与 SQLite 文件；无数据库 / 缓存 / 向量服务；默认不加载嵌入模型，不占用 GPU |
| S3 | 索引可重建 | 删除索引文件后从源目录重建 | `doc_id` 集合不变；引用可解析；重建完成 |
| S4 | 关键词 / 嵌入缺失降级 | 纯 solo 关闭 FTS5；hybrid 下移除本地嵌入模型 | 纯 solo 返回 `keyword_unavailable` 与空证据；hybrid 返回 `embedding_model_missing` 并回退关键词；均不崩溃 |
| S5 | 无 LLM 回答 | 不配置任何 LLM，提出事实问题 | `mode=extractive`；每条结论有引用；无编造段落 |
| S6 | 时序过滤 | 准备一份已过期文档与一份当前文档 | 默认只返回当前文档；`as_of` 过去时间返回当时有效版本 |
| S7 | 规模目标 | 3 万切片参考语料，单用户查询 | 默认 P95 ≤ 0.8 s；启用判别模型 P95 ≤ 1.2 s（目标）；超过 5 万切片出现升级提示 |
| S8 | 双源可选 | 一组配对文档、一组未配对文档；制造指纹不一致 | 未配对正常回答；不一致返回 `source_original_stale` |
| S9 | 本地审计 | 查询后检查本地日志 | 含请求 ID、主体、引用、降级码、延迟；默认保留 30 天 |
| S10 | 判别模型可选路径 | 启用 Jev，对 BM25 Top 30 重排；再关闭 / 断网 | 启用时 `trace.decision` 有记录，排序变化；不可用时 `decision_unavailable` 并回 BM25 顺序；候选集成员不变 |

### 6.3 team 验收

| 编号 | 验收项 | 验证方法 | 通过标准 |
| --- | --- | --- | --- |
| T1 | OIDC 与组 ACL | 50 个测试账号，分属不同组；逐组查询 | 有权组可召回；无权组零召回；引用与深链不泄露 |
| T2 | ACL 撤销生效 | 撤销某用户组权限后立即查询 | ≤ 60 s 生效（目标）；缓存键变化；旧缓存不复用 |
| T3 | 评测门禁 | 100 条评测集在 CI 运行 | Recall@10、NDCG@10、引用精确率有基线；下降 > 2 个百分点阻断 |
| T4 | 索引重建 | 重建并升级分块器 | `doc_id` 不变；`chunk_id` 可变；历史引用可解析 |
| T5 | 组件故障注入 | 分别停止向量库、判别模型、重排服务、LLM | 返回 `dense_unavailable` / `decision_unavailable` / `rerank_unavailable` / `llm_unavailable`；无 500 |
| T6 | 双源 ACL 交集 | 配对文档的转写层与原件层设置不同 ACL | 生效 ACL 为交集；原件缺失返回 `source_original_missing` |
| T7 | 备份恢复 | 从备份恢复 PostgreSQL 与对象存储 | RPO ≤ 15 分钟、RTO ≤ 1 小时（目标）；引用可解析 |
| T8 | 单租户边界 | 尝试构造跨空间未授权查询 | 零召回；审计记录拒绝事件 |
| T9 | 审计完整性 | 抽取 100 条回答对账 | 每次回答有审计记录，含主体、引用、降级、延迟 |

### 6.4 enterprise 验收

| 编号 | 验收项 | 验证方法 | 通过标准 |
| --- | --- | --- | --- |
| E1 | 租户隔离 | 用租户 A 的凭据构造租户 B 的文档 ID 与过滤条件 | 零召回；存储键与密钥按租户隔离；审计记录拒绝 |
| E2 | ACL 同步 | 在源系统撤销权限并观察 | ≤ 5 分钟生效（目标）；`acl_sync_stale` 时按旧 ACL + L4 复核，不放宽 |
| E3 | 双源覆盖 | 抽查内置 Wiki 页面 Revision、外部 Wiki 导入页面与原始文件 | 内置 Wiki Revision 不可变且只读历史可回放；外部文件 100% 有只读快照与 `pair_id`；外部 Wiki 保留 `source_url` / `source_revision`；指纹对账有队列；不一致告警 |
| E4 | 审计不可篡改 | 检查 WORM 记录、哈希链、SIEM 导出 | 100% 回答有记录；篡改检测失败；保留期符合策略 |
| E5 | 安全降级 fail-closed | 注入 ACL 同步失败、原件缺失、审计写入失败 | 分别返回 `acl_unknown` / `source_original_missing` / 拒绝回答；不放行 |
| E6 | 规模与分片 | 单租户 2,000 万切片、100 万文档参考集 | P95 ≤ 3.0 s（目标）；单分片故障返回 `vector_partial`，不整体失败 |
| E7 | 评测与灰度 | 每租户 ≥ 200 条评测集；灰度 10% 流量 | 指标下降 > 2 个百分点自动阻断；可切回旧索引代次 |
| E8 | 保留与删除 | 对受法律保留的文档发起删除 | 删除被阻止；审批与导出流程可追溯 |
| E9 | HA | 停掉一个检索节点 | P95 ≤ 1.5 倍基线；错误率 ≤ 0.1%（目标） |
| E10 | 判别模型降级 | 停止判别模型服务 | 返回 `decision_unavailable`，回 Cross-Encoder 顺序；判别模型不生成正文；P95 ≤ 1.5 倍基线（目标） |

### 6.5 降级注入矩阵

| 注入项 | 期望 `code` | 期望行为 |
| --- | --- | --- |
| 停止向量服务 | `dense_unavailable` | 关键词结果仍返回；Answer 标注 evidence 缺口 |
| solo 默认关闭向量 | `dense_not_enabled` | 仅当查询要求语义召回时上报；BM25 结果仍返回 |
| 向量查询超时 | `dense_timeout` | 返回已完成通道的结果；不阻塞总预算 |
| hybrid 下移除嵌入模型 | `embedding_model_missing` | 仅关键词；solo 不崩溃 |
| 删除索引文件 | `index_missing` | 提示重建；不返回空结果冒充“无匹配” |
| 停止关键词索引 | `keyword_unavailable` | 纯 solo 返回空证据；team / enterprise 稠密结果仍返回 |
| 关闭图谱 | `graph_not_enabled` | 多跳查询明确声明；普通查询不报 |
| 图库故障 | `graph_unavailable` | 退到 1 跳元数据；不伪造多跳 |
| 请求 event_entity 但未配置抽取 | `event_entity_not_enabled` | 回 BM25；仅在显式模式或 auto 需要时上报 |
| event 抽取批次失败 / 超时 | `event_extraction_unavailable` | BM25 与其他通道继续；event 候选缺省 |
| 关闭多模态 | `multimodal_not_enabled` | 文本层结果仍返回；图片 / 音频问题明确缺口 |
| 单个模态失败 | `multimodal_partial` | 返回可用模态结果；列出缺失模态 |
| 关闭判别模型（策略要求时） | `decision_not_enabled` | 回 BM25 / RRF 顺序；仅当策略要求证据重排时上报 |
| 停止判别模型服务 | `decision_unavailable` | 回 BM25 / RRF / CE 顺序；候选集不变；置信度上限 medium |
| 停止重排服务 | `rerank_unavailable` | RRF / BM25 顺序直出；置信度上限 medium |
| 停止 LLM | `llm_unavailable` | 抽取式回答；保留引用 |
| ACL 同步滞后 | `acl_sync_stale` | 使用旧 ACL + L4 复核；不放宽 |
| ACL 无法确定 | `acl_unknown` | 拒绝返回该文档；fail-closed |
| 原件缺失 | `source_original_missing` | 仍可给转写层答案，但不可核对；标注 |
| 原件指纹不一致 | `source_original_stale` | 转写层标记待复核；Answer 标注 |
| 审计后端故障 | `audit_degraded` | team 本地缓冲；enterprise 严格模式拒绝回答 |
| 缓存故障 | `cache_unavailable` | 绕过缓存；不改变证据集 |
| 上下文超预算 | `context_budget_exceeded` | 裁剪最低排名片段；标注证据可能不完整 |

## 7. 降级码表

| code | category | 含义 | 触发 | 响应要求 |
| --- | --- | --- | --- | --- |
| `dense_unavailable` | evidence | 稠密向量通道不可用 | 服务停止、连接失败、索引不可读 | 返回其他通道结果；标注语义召回缺失 |
| `dense_not_enabled` | evidence | 稠密向量通道未启用 | solo 默认关闭，查询要求语义召回 | 仅按需上报；BM25 结果仍返回 |
| `dense_timeout` | evidence | 稠密通道超时 | 超过单通道预算 | 返回部分结果；不阻塞总预算 |
| `vector_degraded_exact` | performance | ANN 退化为精确扫描 | ANN 索引不可用，精确扫描可用 | 召回不降、延迟上升；标注性能降级 |
| `vector_partial` | evidence | 部分向量分片不可用 | 单分片故障 | 返回可用分片结果；标注覆盖不完整 |
| `embedding_model_missing` | evidence | 嵌入模型缺失 | hybrid 已启用但模型文件不存在、加载失败 | 仅关键词；solo 不得崩溃 |
| `index_missing` | evidence | 索引不存在 | 索引文件被删除、首次未构建 | 提示重建；不得把空结果当“无匹配” |
| `index_stale` | evidence | 索引落后于源 | 编译积压、队列延迟 | 返回旧结果并标注新鲜度 |
| `keyword_unavailable` | evidence | 关键词通道不可用 | FTS / 搜索引擎故障 | hybrid 时仅稠密；纯 solo 返回空证据；专有名词查询置信度下降 |
| `graph_not_enabled` | evidence | 图谱通道未启用 | 档位默认关闭或配置关闭 | 仅当查询需要多跳时上报；不伪造关系 |
| `graph_unavailable` | evidence | 图谱通道故障 | 图库连接失败、查询超时 | 退到 1 跳元数据；标注多跳缺失 |
| `event_entity_not_enabled` | evidence | event-entity 通道未启用 | 抽取未配置、租户/空间关闭 | 仅显式请求或 auto 需要时上报；回 BM25 |
| `event_extraction_unavailable` | evidence | event 抽取不可用 | 抽取批次失败、超时、模型不可用 | event 候选缺省；BM25 与其他通道继续 |
| `seed_import_failed` | evidence | 种子内容导入失败 | `docs/` 缺失、frontmatter 非法、写入失败 | 输出 warning / `degraded[]`；不阻塞启动；保留已导入内容 |
| `summary_generation_failed` | quality | 静态摘要生成失败 | LLM/解析失败、超时 | 标记 failed；回退抽取式摘要；不阻塞摄入 |
| `structured_not_configured` | evidence | 未配置结构化数据源 | 无结构化表 / 无只读连接 | 明确告知；不猜数值 |
| `structured_unavailable` | evidence | 结构化查询失败 | 连接失败、权限不足、超时 | 不返回过期数值；可回快照并标注时效 |
| `multimodal_not_enabled` | evidence | 多模态未启用 | 档位关闭或格式不支持 | 文本层结果仍返回；明确缺口 |
| `multimodal_partial` | evidence | 部分模态失败 | 单模型 / 单模态故障 | 返回可用模态；列出缺失模态 |
| `multimodal_unavailable` | evidence | 多模态整体不可用 | OCR / ASR / VLM 全部失败 | 仅文本层；标注不可检索的媒体 |
| `extractor_unavailable` | evidence | 文本抽取器缺失 | 格式不支持、解析器加载失败 | 保留元数据检索；不伪造文本 |
| `source_original_missing` | evidence | 原件缺失 | 文件被删除、快照未建立、深链失效 | 仍可给转写层答案；标注不可核对 |
| `source_original_stale` | evidence | 转写层与原件指纹不一致 | 对账失败、源更新未重转写 | 标记待复核；相关回答标注 |
| `decision_not_enabled` | quality | 判别模型未启用 | 档位默认关闭或策略要求证据重排但未配置 | 仅当查询要求时上报；回 BM25 / RRF 顺序 |
| `decision_unavailable` | quality | 判别模型不可用 | API 故障、超时、配额耗尽、模型加载失败 | 回 BM25 / RRF / CE 顺序；候选集不变；置信度上限 medium |
| `rerank_not_enabled` | quality | 重排未启用 | 档位关闭或查询要求精排但未配置 | 仅当查询要求时上报；RRF 顺序 |
| `rerank_unavailable` | quality | 重排服务故障 | 模型加载失败、超时、容量不足 | RRF 顺序；置信度上限 medium |
| `llm_rerank_unavailable` | quality | LLM 择优不可用 | 预算超限、模型不可用 | 保留 Cross-Encoder 结果；标注 |
| `llm_unavailable` | quality | LLM 生成不可用 | 未配置、超时、配额耗尽 | 抽取式回答；保留引用 |
| `verification_unavailable` | quality | 忠实度校验不可用 | 校验模型 / 服务故障 | 置信度上限 medium；标注未校验 |
| `citation_alignment_failed` | quality | 引用无法对齐到来源 | 生成句子找不到具体片段 | 移除无依据句子或引用；记录数量 |
| `context_budget_exceeded` | evidence | 必要证据被挤出上下文 | token 预算不足 | 标注可能不完整；优先保留高排名证据 |
| `query_budget_exceeded` | evidence | 查询总预算耗尽 | 多通道累计超时 | 返回已完成部分；标注未完成通道 |
| `acl_unknown` | security | ACL 无法确定 | 源 ACL 缺失、同步失败、配对缺一侧 | 拒绝返回该文档；fail-closed |
| `acl_sync_stale` | security | ACL 缓存滞后于源 | 同步延迟、源不可达 | 旧 ACL + L4 复核；不放宽 |
| `temporal_unknown` | security | 时序字段缺失或不可信 | 遗留数据、源时间无效 | 按不可信处理；标注并进入回填队列 |
| `cache_unavailable` | performance | 缓存不可用 | Redis / SQLite 缓存故障 | 绕过缓存；不改变证据集 |
| `audit_degraded` | security | 审计写入降级 | 日志后端故障、磁盘满 | team 缓冲；enterprise 严格模式拒绝回答 |
| `term_service_unavailable` | quality | 术语服务不可用 | 术语服务故障 | 使用原词检索；标注归一缺失 |
