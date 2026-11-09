# ADR 0009：静态摘要、统一入口与 KB 侧会话历史

- **状态**：Accepted
- **日期**：2026-11-08

## 背景

知识库需要两种“总结”和一种“入口”：

- 每篇文章在转写后应有静态摘要，用于文章头部与搜索结果 snippet；
- 查询时应有多轮改写、检索、证据判断、带引用回答，并支持 SSE；
- Wiki 顶部搜索栏与右侧问答面板应是同一入口，而不是两套状态。

同时，Zeta 作为一等 Agent 客户端会产生会话与 working trace。KB 必须保存用户可见历史与引用，但不能把 Zeta 的 chain-of-thought/全量 trace 复制进来。

## 决策

**静态摘要与动态回答分开建模；统一入口共享同一 query/conversation；KB 是用户可见会话历史的 canonical，Zeta 是 agent working trace。**

- 静态摘要：转写/导入/Wiki revision 保存后异步 job 生成；输入规范化正文，输出 tldr、3–5 bullets、keywords、entities、lang、source_revision；写 `document_summaries`；revision/model/prompt_version 变化即重生成；失败回退抽取式并标 `summary_generation_failed`。
- 动态回答：查询时多轮改写（最近 N 轮）、检索、证据判断、Answer + citations、SSE；缓存键含 `conversation_context_hash`；`summary_kind=dynamic`、`summary_source=llm|extractive`、`cache_hit`。
- 统一入口：`POST /api/v1/query`，`mode=search|answer|both`，默认 `both`；保留 `/api/v1/search`；MCP `kb_query`；UI 顶部搜索与右侧面板共享状态，`?q=&conversation=` 深链。
- 会话：`conversations` + `messages` 存用户/助手可见消息、引用、usage、degraded；`external_session_id` 映射 Zeta session；不存 chain-of-thought；KB 展示真相，Zeta agent 真相。
- 权限/隐私：按 tenant/space/owner 隔离；admin 审计可见元数据但不默认读内容；删除写 tombstone；不得跨租户。
- 静态摘要成本写 `usage_events`（`source=summary`），受配额；不阻塞摄入；动态回答走既有模型网关/配额。

## 备选方案与为何不选

| 方案 | 否决理由 |
| --- | --- |
| 把静态摘要当实时回答 | 过期风险；用户无法区分 revision；必须单独字段与 UI 标识 |
| 搜索与问答各自维护状态/历史 | 双份 query、双份引用、双份权限；无法深链恢复 |
| 把 Zeta 全量 trace 复制进 KB | 体积、隐私、CoT 泄漏；KB 只需要用户可见消息 |
| 摘要同步生成、阻塞摄入 | 导入延迟不可控；LLM 失败会拖垮摄入 |
| 只存 Zeta 历史、不存 KB 历史 | 换 Agent/离线时历史丢失；无法做权限与审计 |

## 后果

**正面**

- 静态摘要可用于列表/预览，动态回答有引用与流式体验，两者不混淆。
- 一个入口、一个 conversation、一套引用，搜索与问答不会状态分叉。
- KB 历史可独立于 Zeta 存在；Zeta 替换不影响用户可见记录。

**负面**

- 多一套 `document_summaries` / `summary_jobs` / conversations / messages 表与后台。
- 摘要与动态回答都消耗 token，需要配额与成本归因。
- 多轮改写与缓存键增加复杂度；会话历史有隐私与保留要求。

## 关联文档

- [document/summaries-and-conversations.md](../summaries-and-conversations.md) — 数据模型、API/MCP、阶段与风险
- [document/architecture.md](../architecture.md) — L2/L5/L6、Answer 字段
- [document/gap-assessment.md](../gap-assessment.md) — 能力面与工作量修订
- [document/roadmap.md](../roadmap.md) — P0/P1/P2
- [document/tier-matrix.md](../tier-matrix.md) — 三档差异
