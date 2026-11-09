# Changelog

本文件记录 `lkb`（Layered Knowledge Base）的版本变更。版本号与 `Cargo.toml` / `web/package.json` / `desktop/src-tauri` 保持一致；格式参考 [Keep a Changelog](https://keepachangelog.com/)，日期为 ISO 8601。

## [0.2.1] - 2026-11-09

0.2.z 线发布：只递增 patch。本版把桌面壳、管理面与静态 SPA 托管补齐到可发布状态；**macOS 不在发布范围**。

### Added — 桌面应用（Windows / Linux）

- **Tauri 2 桌面壳**（`desktop/`）：首启向导（SQLite / PostgreSQL）→ 启动内置 sidecar `lkb serve`（随机回环端口）→ `/health` 通过后打开主窗口；单实例运行，关闭窗口或退出即停止 sidecar。
- **安装包与便携包**：Windows NSIS 安装包与 portable zip；Linux deb、AppImage 与 portable tar.gz。sidecar 与 CLI 归档复用**同一个** `lkb` 二进制构建（不是第二套代码），资源内嵌 `web/dist`。
- **路径规范（统一 `lkb` 目录，无品牌名路径）**：配置 `%APPDATA%\lkb\lkb.toml`（Linux `$XDG_CONFIG_HOME/lkb/lkb.toml`）；数据 `%LOCALAPPDATA%\lkb\data\lkb.sqlite`（Linux XDG data 目录）；日志 `%LOCALAPPDATA%\lkb\logs`（Linux `$XDG_STATE_HOME/lkb/logs`）。
- **CLI 默认 OS 路径**：`lkb` 的配置/数据/日志默认路径与桌面壳统一（`src/paths.rs`），未显式传 `--config` 时按 `<config_dir>/lkb.toml` 解析。

### Added — 管理 API 与 Admin UI

- **管理 API 补全**：`/api/v1/admin/overview`、`admin/users`（列表/角色/禁用/重置密码）、`admin/billing/plans` CRUD、`admin/providers/{id}/test`、`admin/audit`，OpenAPI 3.1 同步。
- **Admin UI**：概览面板、CRUD 表单、空态与确认流程、可拖拽三栏布局。

### Added — 静态 SPA 托管

- `lkb serve` 可托管静态 SPA（`server.web_dir` / `LKB_WEB_DIR`），桌面壳与自托管部署共用同一套 `web/dist`。

### 发布产物（Release）

- 个人用户版（solo CLI）：`lkb-solo-v0.2.1-x86_64-unknown-linux-gnu.tar.gz`、`lkb-solo-v0.2.1-x86_64-pc-windows-msvc.zip`。
- 桌面版（Windows / Linux）：`lkb-solo-v0.2.1-windows-x64-setup.exe`、`lkb-solo-v0.2.1-windows-x64-portable.zip`、`lkb-solo-v0.2.1-linux-amd64.deb`、`lkb-solo-v0.2.1-linux-amd64.AppImage`、`lkb-solo-v0.2.1-linux-amd64.tar.gz`；**macOS 不发布**。
- 企业部署版（enterprise）：`ghcr.io/linxira-os/linxira-knowledge-base:0.2.1` 与 `linxira-knowledge-base-enterprise-v0.2.1.tar.gz`；另有 `lkb-web-v0.2.1.tar.gz`、`SHA256SUMS` 与 SPDX SBOM。
- 同一二进制同时支持 SQLite 与 PostgreSQL（配置切换），不发布单独的 SQLite / PostgreSQL 包；所有安装包未签名（Windows 可能出现 SmartScreen 提示）。版本规则、tag 流程、校验与回滚见 [document/release-process.md](document/release-process.md)。

## [0.2.0] - 2026-11-09

首个公开发布版本（更早版本从未公开发布）。

### Added — 核心能力

- **三档 profile，单一内核**：`solo`（SQLite 单文件、单进程、默认 `auth.mode = none` 且仅绑定 loopback）、`team`（PostgreSQL、会话/Bearer 认证、共享空间）、`enterprise`（PostgreSQL，多租户通过独立配置/数据库或外部路由）。档位差异只在配置与适配器装配层，不对代码分叉（ADR 0001）。
- **Wiki**：页面/版本/回滚、编辑器与历史视图；`lkb seed` 内置 `docs/zh` + `docs/en` 双语种子知识库（幂等、不覆盖用户修改、`seed_meta` 版本管理）。
- **RAG 检索**：`bm25`（默认）、`bm25_judge`、`event` / `event_entity`、`hybrid`、`auto` 多模式；SQLite FTS5 BM25 与 PostgreSQL `tsvector + ts_rank_cd`；可选 judge 重排、embedding 与 event-entity 抽取；缺失通道显式写入 `degraded[]`。
- **统一入口与会话**：`POST /api/v1/query`（`search` / `answer` / `both`）与 SSE 流式（`/api/v1/query/stream`、`/api/v1/answer/stream`）；`conversations` / `messages` 会话历史、query 改写与外部会话绑定。
- **Answer 契约**：`answer` / `citations[]` / `confidence` / `conflicts[]` / `degraded[]` / `knowledge_as_of` / `mode`；生成式答案附公开 alias 的 `model` 与 `usage`（ADR 0005）。
- **模型网关**：public alias / provider / model / route 控制面，访问策略、priority/weight/health 路由与 failover/熔断、`usage_events` 计量、token 配额预留/提交/释放；旧 `[models.llm]` 自动物化为 `default` alias。
- **静态摘要**：`document_summaries` + 异步 `summary_jobs`（导入/Wiki/seed 时入队），键 `doc + revision + model + prompt` 幂等，失败有限重试并保留抽取式回退（ADR 0009）。
- **空间与权限**：`groups` / `group_members` / `space_members` 三层角色，空间 CRUD、成员、归档/恢复/转移；召回期与二次 ACL 过滤 fail-closed；全量审计事件（ADR 0010）。
- **存储配额**：tenant → group → space → user 四级配额与 `storage_usage`，写入前预留、成功后重算、失败释放；超限 413 / 429；`quota_overrides` 临时覆盖。
- **认证与 SSO**：HttpOnly 会话 cookie（CSRF / Origin 校验）、Bearer 令牌（哈希存储、scope、吊销）、登录限流；SSO `trusted_header` 模式（`trusted_proxies` IP/CIDR 对端判定、自动开通、`group_role_map`、`allowed_email_domains`）（ADR 0012 配套）。
- **连接器**：运行时 `filesystem_export` / `jsonl` / `http_json` 连接器 + `connectors` / `external_refs` / `connector_jobs` 骨架，同步幂等、外部 ACL 以源为准、`secret_env` 不回显。
- **计费**：plans / subscriptions / wallets / ledger（双分录、追加式、幂等键）/ topups / invoices / payment_events / entitlements；本地钱包充值、扣费、退款冲正；stripe Checkout + webhook 验签（HMAC-SHA256 + 时间戳容差）、manual/mock 提供者；套餐状态机与 entitlement 快照。
- **可观测性与运维**：结构化日志（`LKB_LOG_FORMAT=json|text`，请求正文/密钥不落日志）、`/metrics`（Prometheus 文本）、`/health`、i18n（zh-CN / en，Accept-Language 回退与稳定错误码）（ADR 0011）。
- **接口面**：CLI（import/export/search/ask/eval/seed/serve/user/token/page/group/space/auth/models/usage/quota/summary/billing/mcp）、MCP stdio JSON-RPC（`kb_*` 工具）、OpenAPI 3.1（`/openapi.json`）。
- **Web 前端**：React SPA（Wiki 阅读/编辑、统一检索与问答、空间/管理台），构建产物 `web/dist` 随仓库提交以便独立静态部署。
- **数据与治理**：治理字段 day-one（`tenant_id` / `space_id` / `acl_json` / `valid_from` / `valid_to` / `version` / `superseded_by`）（ADR 0002）；文件为事实源、索引可重建（ADR 0003）；`doc_id` 由规范化路径 + 内容哈希派生（ADR 0006）。

### 未实现 / 明确不做（0.2.0）

- OIDC / SAML / SCIM 未实现；SSO 仅 `trusted_header` 模式。`lkb auth login --device` 返回 `not_implemented`。
- 微信 / 支付宝支付未实现（`payment_not_configured`）；当前支付网关实现为 stripe 与 manual/mock。
- `feishu` / `sharepoint` / `onedrive` / `confluence` / `mediawiki` 连接器仅有配置骨架（`connector_not_configured`）。
- 向量库 / ANN 索引未内置：无 sqlite-vec / pgvector 集成；hybrid 在进程内对已入库向量做余弦 + RRF。
- `enterprise` 当前与 `team` 行为相同：HTTP 层多租户路由、WORM 审计与 SIEM 导出未实现（靠独立配置/数据库隔离）。
- 多实例共享限流与配额计数未实现（进程内计数）；启动迁移无 advisory lock，生产应先单实例迁移。
- 进程内不终止 TLS（必须放在 HTTPS 反向代理之后）；PostgreSQL 中文检索走显式 `keyword_exact_fallback`（非 BM25，写入 `degraded[]`）。
- 导入为增量、不建立移动/重命名别名；PDF 抽取为可选 feature（行号非页码）。

### Security

- 仓库内置 `SECURITY.md`：漏洞请通过 GitHub Security Advisories 私密上报。
- 历史与当前树均无真实凭据入库（PG 凭据只存在于本地 gitignored `.lkb/pg.env`；`document/pg.env.example` 为占位符）。

### 发布产物（Release）

- 发布资产清单（solo CLI / 桌面版 / enterprise）见 0.2.1 段与 [document/release-process.md](document/release-process.md)；桌面安装包在 0.2.1 首次提供（macOS 不发布）。
