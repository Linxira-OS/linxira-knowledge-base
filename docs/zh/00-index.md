---
title: 产品文档索引
slug: 00-index
lang: zh
order: 0
summary: 分层 AI 知识库（lkb）产品文档的中文入口、阅读顺序与语言说明。
---

# 分层 AI 知识库产品文档

本套文档随软件分发，同时是 `lkb seed` 的编译期内置种子内容：首次启动后可以在 `space=product` 里直接阅读和检索它们。文档只描述当前构建真实具备的行为；未实现或未实测的能力统一写成边界，而不是写成计划。

## 0. 如何使用这套文档

- 第一次使用：按 [03 快速开始](03-quickstart.md) 完成构建、首次 seed、导入、检索与启动服务。
- 评估选型：先读 [01 产品概览](01-overview.md)，再读 [08 部署与运维](08-deployment.md)。
- 想接入 Agent：读 [05 检索与 AI 回答](05-search-ai.md)（统一 query 与 SSE）、[06 认证与安全](06-auth-security.md)（令牌与角色）、[10 架构与实现](10-architecture.md)（MCP、OpenAPI、网关）。
- 排查问题：直接看 [11 常见问题与排错](11-faq.md)。

## 1. 文档地图

| 序号 | 文档 | 内容 |
| --- | --- | --- |
| 01 | [产品概览](01-overview.md) | 定位、三档 profile、四种部署模式、能做与不能做 |
| 02 | [核心概念](02-concepts.md) | space、page、revision、chunk、doc_id、claim、Answer、degraded、validity，以及角色、配额、别名、账本、摘要与会话 |
| 03 | [快速开始](03-quickstart.md) | 构建、seed、导入、检索、回答、评测、启动服务、构建前端、MCP |
| 04 | [内置 Wiki](04-wiki.md) | 页面创建、编辑、历史与恢复、If-Match 并发、权限、语言 |
| 05 | [检索与 AI 回答](05-search-ai.md) | 五种检索模式、统一 query、SSE、静态摘要、引用与 `knowledge_as_of` |
| 06 | [认证与安全](06-auth-security.md) | JSON 登录、cookie/CSRF/Bearer、角色、限流、审计、TLS 反代、fail-closed |
| 07 | [配置参考](07-configuration.md) | `lkb.toml` 全字段、环境变量、模型网关、限额、i18n、支付与计费开关 |
| 08 | [部署与运维](08-deployment.md) | SQLite/PostgreSQL、迁移、备份、单二进制、前端静态部署、可观测性 |
| 09 | [外部知识源与连接器](09-connectors.md) | 飞书、SharePoint、OneDrive、Confluence、MediaWiki 的接入边界 |
| 10 | [架构与实现](10-architecture.md) | L1–L8、Store/Provider/Policy、网关/配额/计费、摘要/会话、数据流与模块 |
| 11 | [常见问题与排错](11-faq.md) | 常见错误、降级码、错误码速查与已知限制 |

## 2. 语言与版本

- 中文目录为 `docs/zh/`，英文目录为 `docs/en/`；两边 slug 一一对应，命令、字段与章节编号保持一致。
- `lkb seed` 编译期只嵌入这 24 个文件（每语言 12 篇），写入 `space=product` 的 Page/Revision，并派生可检索的 Document/Chunk。
- 英文版是面向英文读者的意译，不是逐字机翻；技术名词首次出现时给出英文。
- 文档描述仓库内的当前实现（`lkb` 0.2.0）。命令以 [03 快速开始](03-quickstart.md) 与 `lkb --help` 为准。

## 3. 阅读顺序

最小路径是 01 → 03 → 05。要交给团队时，再读 06、07、08；要接外部系统时读 09；要改代码或判断实现边界时读 10；遇到错误直接查 11。

## 4. 本版新增能力速览

| 能力 | 入口 | 状态 |
| --- | --- | --- |
| 内置种子知识库 | `lkb seed`、solo 空库首次 `serve` 自动 seed | 已实现，`space=product` |
| 角色、组、空间成员、自助空间、存储配额 | `lkb group/space ...`、`/api/v1/spaces|groups` | 已实现（SQLite 已验证） |
| 模型网关（公开 alias、路由、故障切换） | `/api/v1/gateway/chat`、`lkb models ...` | 已实现 |
| token 配额与用量计量 | `lkb usage/quota`、`/api/v1/admin/*` | 已实现 |
| 钱包/套餐/订阅/支付适配器 | `/api/v1/billing/*`、管理后台 Billing 页 | 本地账本已实现；支付仅 stripe/manual/mock |
| 静态摘要 | `lkb summary ...`、页面摘要接口 | 已实现，失败回退抽取式 |
| 统一 query / 会话历史 / SSE | `POST /api/v1/query`、`/query/stream` | 已实现 |
| 契约 REST 与 JSON 登录 | `/api/v1/auth/*`、`/api/v1/pages`（If-Match） | 已实现 |
| MCP 与 OpenAPI | `lkb mcp serve`、`/openapi.json` | 已实现（MCP 为只读+写页面的最小集合） |
| 部署模式、功能门禁、i18n | `[deployment]`、`[features]`、`[i18n]` | 模式与校验已实现；`billing`/`audit`/`connectors` 已实际门禁，其余 feature（signup/sso/multi_tenant/public_api/mcp/public_share）仅配置/展示 |
| 结构化日志、`/metrics`、限流 | `LKB_LOG_FORMAT`、`GET /metrics` | 已实现（单进程计数） |
| Web 前端（React SPA） | `web/`，构建为 `web/dist` | 已实现；不新增前端测试 |

## 5. 相关文档

- [01 产品概览](01-overview.md)
- [03 快速开始](03-quickstart.md)
- [11 常见问题与排错](11-faq.md)
