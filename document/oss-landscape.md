# 开源生态调研：自建还是集成

> 调研对象：10 个 Wiki/知识库产品、13 个 RAG/知识库系统、OWASP 会话与 CSRF 指南。
> 检索时间：2026-11-08（UTC）。仓库元数据（许可证、stars、最近提交、最新 release）来自 GitHub REST API；官方文档与仓库文件实际打开读取。
> 结论先行：**内核与最小 Wiki 自建；外部 Wiki/知识库只做进程外连接器；不把任何第三方 Wiki 或 RAG 平台作为运行时依赖。**

---

## 0. 结论摘要

1. **自建最小 Wiki，范围锁死。** 只做 space / page / revision 三件事加 Markdown 与只读历史；不做富文本协同、评论、审批、插件市场。内置 Wiki 主要服务 solo 的本地写作；team/enterprise 的日常编辑仍以 Confluence/MediaWiki 等源系统为准，平台侧只读同步（口径冲突见 §7.1）。
2. **不自建协同编辑器。** 页面编辑用 Markdown + 服务端渲染；并发用 revision 版本号做乐观锁（`If-Match`），冲突返回 409，不做 CRDT。
3. **外部系统只做连接器。** 优先 Confluence、MediaWiki、Outline、Notion 四类；它们有稳定导出格式或 API，且企业装机量大。连接器走进程外 HTTP/文件，不链接第三方代码。
4. **RAG 平台不集成。** RAGFlow/Dify/Onyx/AnythingLLM 都依赖 Python/Node 运行时（前三者还是多服务栈），与“Rust 单二进制、solo 零外部依赖”直接冲突；只借鉴解析、连接器、ACL 同步与批处理实现。
5. **许可证是硬约束。** AGPL（Wiki.js/Docmost/Khoj/AppFlowy）、BSL（Outline）、改版 Apache（Dify，禁止多租户服务）、带品牌条款（Open WebUI）都不适合嵌入分发；连接器通过 API 隔离通常可降低衍生作品风险，但仍需法务确认。

---

## 1. 方法与证据等级

| 等级 | 含义 | 本文用法 |
| --- | --- | --- |
| A | 官方文档或官方仓库文件已实际打开并读取 | 直接陈述，附 URL |
| B | GitHub REST API 元数据或仓库 LICENSE/README 已读取 | 用于许可证、活跃度、版本号 |
| C | 仅搜索结果、第三方页面，或官方站点不可达 | 明确标注“未逐页核实”，并进入 §11 清单 |

本次未能访问的站点，相关结论降级为 C：

- `dokuwiki.org`：连接被拒（ECONNREFUSED），DokuWiki 的存储、修订、API 细节无法逐页核实。
- `r2r-docs.sciphi.ai`：文档站返回 404，R2R 的认证与部署细节无法核实。
- `docs.anythingllm.com`：返回 403，改用其 GitHub 文档仓库源码。
- BookStack 部分 `/docs/admin/*` 路径返回 404，改用可访问的 OIDC 页与 API 文档。

不引用厂商营销页中的准确率、吞吐、成本数字；本文只记录可验证的机制与限制。

---

## 2. 总表

缩写：PG = PostgreSQL；ES = Elasticsearch；对象存储 = S3/MinIO/Azure Blob 等；本地账号 = 用户名密码；OIDC = OpenID Connect；SAML = SAML 2.0；“未核实”= 本次未能取得官方页面。

| 项目 | 类别 | 语言 | 许可证 | 认证 | 存储 | 导入导出 | 嵌入方式 | 结论 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Wiki.js | Wiki | TypeScript | AGPL-3.0 | 本地、LDAP/AD、SAML、OIDC/OAuth2、Entra ID | PG 17/18（3.x 强制） | REST `/_api`；Git 存储同步；站点级导出未核实 | REST API + Web UI，无嵌入组件 | 借鉴 |
| Outline | Wiki | TypeScript | BSL 1.1 | 无密码；Google/Microsoft/Slack/邮箱链接/passkey；OIDC；SAML 需许可 | PG 14+、Redis | 工作区/集合 JSON/MD/HTML；导入 Confluence/Notion/docx | RPC API + webhook，无插件 | 借鉴；连接器候选 |
| BookStack | Wiki | PHP | MIT | 本地、OIDC、SAML、LDAP；API Token | MySQL/MariaDB（未逐页核实） | API 导出 MD/HTML/PDF/纯文本/zip | REST + webhook + 主题系统 | 借鉴 |
| Docmost | Wiki | TypeScript | AGPL-3.0 + EE | 本地、OIDC、SAML；JWT 默认 30 天 | PG、Redis、本地/S3/Azure | 导出未核实 | Web UI；iframe 默认禁止 | 借鉴 |
| AFFiNE | Wiki/协作 | TypeScript | MIT + 后端单独许可 | 本地、Google/GitHub/OIDC | PG、Redis、blob | 导出/API 未核实 | Web/桌面 | 不采用 |
| AppFlowy | Wiki/协作 | Dart | AGPL-3.0 | 密码、OIDC/OAuth2（付费档）、SAML、SCIM | PG、Redis、对象存储、GoTrue | 导出未核实 | 客户端 + Cloud | 不采用 |
| MediaWiki | Wiki | PHP | GPL-2.0-or-later | 本地 + AuthManager/PluggableAuth（OIDC/LDAP 扩展）；API OAuth | MySQL/MariaDB、PG、SQLite | Special:Export XML（含历史）；Action API | API + 扩展/皮肤 | 集成（连接器） |
| DokuWiki | Wiki | PHP | GPL-2.0 | 本地用户文件 + 认证插件；ACL 文件 | 文件系统（无 DB） | 文件级复制；API 未核实 | 插件 + RPC（未核实） | 不采用 |
| XWiki | Wiki | Java | LGPL-2.1 | 本地、LDAP/OIDC 扩展；REST Basic/会话 | 关系库（具体后端未逐页核实） | REST `/rest`；XAR 未核实 | REST + Java 扩展 | 不采用 |
| Gollum | Wiki | Ruby | MIT | 无内置认证，靠 Rack/反向代理 | Git 仓库 | git clone | 反向代理 + git | 借鉴 |
| RAGFlow | RAG | Go/Python | Apache-2.0 | 本地、注册开关、API Key；OAuth 未逐页核实 | MySQL/OceanBase、ES/Infinity、MinIO、Kvrocks、NATS | REST + MCP；文件上传 | HTTP API + MCP | 借鉴 |
| Dify | RAG/平台 | Python | 改版 Apache-2.0 | 本地、OAuth；SSO 仅企业版；API Key | PG、Redis、向量库、对象存储 | API；应用 DSL | API + 插件 + 聊天嵌入 | 不采用 |
| AnythingLLM | RAG | JavaScript | MIT | 单用户实例密码；多用户账号/角色；未见 SSO（未核实） | SQLite + LanceDB（默认未逐页核实） | API；工作区导入未核实 | API + 可嵌入 widget | 借鉴 |
| Khoj | RAG | Python | AGPL-3.0 | 默认匿名；邮箱 magic link、Google OAuth | PG + pgvector | API；无页面导出 | API + 客户端 | 借鉴 |
| Open WebUI | RAG/UI | Svelte/Python | BSD-3 + 品牌条款 | 本地、OAuth/OIDC（单 Provider）、LDAP、可信头 | SQLite 默认/Postgres（未逐页核实） | API；知识文件上传 | OpenAI 兼容 API + Pipelines | 借鉴 |
| Onyx | RAG | Python | MIT + EE 目录 | 密码（可禁用）、Google/OIDC/SAML；CE 仅一个 SSO | PG、Vespa、Redis、对象存储 | 连接器 + API | API + MCP + 连接器 | 借鉴 |
| R2R | RAG | Python | MIT | 未核实（文档站 404） | PG + pgvector（未逐页核实） | REST/SDK | REST API/SDK | 不采用 |
| Verba | RAG | Python | BSD-3 | 无 | Weaviate | 仅摄入 | 无 | 不采用 |
| LightRAG | RAG | Python | MIT | 可选 API Key 或账号+Token；默认公开 | KV/向量/图；可选 PG/Neo4j/Milvus/OpenSearch | REST；文档摄入 | REST + WebUI | 借鉴 |
| GraphRAG | RAG | Python | MIT | 无（CLI） | 本地工件 | CLI 产出 | 库/CLI | 借鉴 |
| LlamaIndex | RAG 框架 | Python | MIT | 无（库） | 自带存储适配器 | 框架 | 库 | 借鉴 |
| Haystack | RAG 框架 | Python | Apache-2.0 | 无（库） | 自带存储适配器 | 框架 | 库 | 借鉴 |
| jev-rag | RAG | Python | MIT | 无（本机单用户） | SQLite + FTS5 | 本地文件 + SQLite 索引 | 本地 Web + Python 模块 | 借鉴（最接近） |

表内 23 行数据。每个项目的官方来源 URL 见对应小节末尾的“来源”行，本表不重复列 URL 以免不可读。

---

## 3. A 组：Wiki/知识库产品

### 3.1 Wiki.js

- **定位**：Node.js 全功能 Wiki，页面 + 多站点 + 多认证 + 多存储模块；最新 release v3.2.0（2026-11-08），29.2k stars；官方文档部分页面仍标注“3.0 BETA”，稳定性需按 release notes 确认。
- **运维门槛被 3.x 抬高**：官方 requirements 明确要求 PostgreSQL 17/18 与 Node.js 26，且必须独立域名、不支持子目录安装；文档不再列 MySQL/SQLite。
- **认证**：官方列 8 项策略：本地、LDAP/AD、SAML 2.0、OIDC/OAuth2、Entra ID，以及 Discord/GitHub/Google 三个 OAuth Provider；策略启用后还要在每个 site 的 Login 页单独激活。
- **API 与令牌**：3.x 在 `/_api` 暴露 OpenAPI 3.0 REST；API Key **不绑定用户**，按所选组继承权限，不能执行需要归属到具体用户的写操作；这类操作要用 personal access token。
- **修订**：页面时间线支持比较、下载、恢复；恢复会新增一条 revision，旧 revision 保留；可按历史 revision 分叉出新页面。
- **来源**：[A] https://docs.js.wiki/admin/auth 、https://docs.js.wiki/dev/api 、https://docs.js.wiki/guide/history 、https://docs.js.wiki/setup/requirements ；[B] https://github.com/requarks/wiki

### 3.2 Outline

- **定位**：团队 Wiki/知识库，文档 + 集合（collection）+ 嵌套页面；最新 v1.12.0（2026-10-23），41.2k stars。
- **认证**：不存储密码，登录始终经外部 Provider、passkey 或邮箱链接/验证码；自托管 OIDC 有独立配置文档（discovery 或手工端点）。SAML、审计日志、结构化属性、Confluence 导入器列在商业许可功能中。
- **修订模型**：编辑期间至少每 5 分钟记录一版；多人同时编辑归到同一 revision；恢复旧版会再生成一版，属于非破坏操作；revision 重命名/永久删除仅云端与商业版。
- **导入导出**：集合与工作区可导出 Markdown/HTML/JSON；JSON 是跨实例迁移的推荐格式，但有约 1.5GB 上限，且作者信息丢失、集合权限不导入；导入支持 Confluence、Notion、docx、Markdown、JSON。
- **API**：RPC 风格（`POST /documents.info` 这类端点），主应用就建在同一 API 上；API Key 用 `Authorization: Bearer`，可设过期时间与按端点通配的 scope。
- **许可**：BSL 1.1，Additional Use Grant 禁止向第三方提供 “Document Service”；Change Date 2030-10-23，Change License Apache 2.0。
- **来源**：[A] https://docs.getoutline.com/s/guide/doc/authentication-Wr4sfjvmL1 、https://docs.getoutline.com/s/guide/doc/revision-history-AiL6p22Ssq 、https://docs.getoutline.com/s/guide/doc/export-data-Da6C7HqL8M 、https://docs.getoutline.com/s/guide/doc/api-1rEIXDfLF6 、https://docs.getoutline.com/s/hosting/doc/oidc-8CPBm6uC0I 、https://docs.getoutline.com/s/hosting/doc/business-enterprise-rv0715NxO3 ；[B] https://github.com/outline/outline/blob/main/LICENSE

### 3.3 BookStack

- **定位**：PHP/Laravel 的“书架-书-章-页”层级 Wiki，强调简单与可审计；最新 v26.09.2，19.1k stars，MIT。
- **认证**：OIDC 可作主认证，支持 discovery、`sub` 绑定、组到角色同步；限制是**只支持 RS256**、不支持 token 加密、不支持 issuer discovery，24.02 起使用 PKCE；另有 SAML 2.0 与 LDAP（文档导航可见，未逐页核实）。
- **API**：REST + JSON；认证头是 `Authorization: Token <token_id>:<token_secret>`，用户角色必须勾选 “Access System API”；浏览器已登录时会话也可直接调 API，便于调试。
- **导出**：API 对 page/chapter/book 都提供 HTML、PDF、纯文本、Markdown、zip 导出端点；另有 webhook 与“视觉/逻辑主题系统”做扩展。
- **修订**：页面 revisions 视图支持查看、比较、恢复（公开实例与产品文档可见，官方专页未逐页核实）。
- **风险**：PHP + MySQL 的运行时与单二进制目标不兼容；浏览器会话也能调用 API，集成时必须只走 Token，避免把会话凭据带进服务端调用。
- **来源**：[A] https://www.bookstackapp.com/docs/admin/oidc-auth 、https://demo.bookstackapp.com/api/docs ；[B] https://github.com/BookStackApp/BookStack

### 3.4 Docmost

- **定位**：Confluence/Notion 的开源替代，实时协作 + space + 权限 + 评论 + 页面历史；v0.97.0，22.3k stars。
- **技术栈**：TypeScript（NestJS 服务端 + React 前端）；PostgreSQL + Redis 必需；文件存储支持 local/S3/Azure；JWT 默认有效期 30 天。
- **认证**：本地账号、OIDC（含组同步，组必须预先在 Docmost 建好，大小写不敏感）、SAML（有 `SAML_DISABLE_REQUESTED_AUTHN_CONTEXT` 等服务端开关）。
- **搜索**：默认用 PostgreSQL 搜索；Typesense 是付费版的专用搜索引擎，支持拼写容错与多语言分析。
- **嵌入**：默认发 `X-Frame-Options: SAMEORIGIN`，公共分享页豁免；要嵌到内网门户必须显式开 `IFRAME_EMBED_ALLOWED` 并配 `IFRAME_ALLOWED_ORIGINS`。
- **风险**：公开文档没有给出站点级导出格式与稳定 API 契约，作为迁移源或集成目标都缺少可验证出口；AGPL + `ee/` 双许可。
- **来源**：[A] https://docmost.com/docs/user-guide/authentication/oidc 、https://docmost.com/docs/self-hosting/environment-variables ；[B] https://github.com/docmost/docmost

### 3.5 AFFiNE

- **定位**：文档 + 白板 + 数据库的一体化协作工作区，不是 Wiki-first 产品；v0.27.4，74.3k stars。
- **自托管组件**：AFFiNE server（Web/API/sync/collab）+ PostgreSQL + Redis + blob 存储 + 反向代理；官方推荐 Docker Compose，最低约 4 核 2GB。
- **认证**：邮箱密码 + Google/GitHub/OIDC，OAuth 在 Admin Panel 配置，回调固定为 `/oauth/callback`。
- **许可**：根 LICENSE 是 MIT，但 `packages/backend` 与 `packages/common/native` 另行许可；不能把整个仓库当作纯 MIT。
- **集成面**：官方自托管文档没有给出内容导出/批量 API 的稳定契约（未核实），且数据模型是 block 文档而非 page/revision，迁移到本系统需要重写解析。
- **结论**：产品能力强但边界不匹配，不作为 Wiki 内核或迁移源。
- **来源**：[A] https://docs.affine.pro/self-host-affine/ 、https://docs.affine.pro/self-host-affine/administer/oauth-2-0 ；[B] https://github.com/toeverything/AFFiNE/blob/canary/LICENSE

### 3.6 AppFlowy

- **定位**：local-first 的 Notion 替代（文档、数据库、看板），桌面客户端为核心；0.15.0，78.4k stars。
- **自托管**：AppFlowy Cloud + GoTrue 认证服务 + PostgreSQL/Redis/对象存储（OIDC 文档明确提到 `gotrue` 容器与回调 `/gotrue/callback`；其余组件未逐页核实）。
- **认证**：密码；OIDC/OAuth2 可在 Admin Console 运行时注册（无需重启），但**需要 Seed 及以上付费档**，Free Tier 隐藏该页面；另支持 SAML 与 SCIM 2.0。
- **许可**：主仓库 AGPL-3.0；自托管商业能力在 `AppFlowy-SelfHost-Commercial` 仓库，另有许可与付费档。
- **边界**：不是 Wiki 数据模型（页面/修订/空间）优先的产品，且关键 SSO 能力被付费墙包住，不适合作为 solo/team 的内置能力。
- **来源**：[A] https://github.com/AppFlowy-IO/AppFlowy-SelfHost-Commercial/blob/main/docs/OIDC.md ；[B] https://github.com/AppFlowy-IO/AppFlowy

### 3.7 MediaWiki

- **定位**：维基百科同源 Wiki，扩展生态大；PHP + 关系库，适合大规模公开/内部知识库。GitHub 仓库是镜像（5.2k stars），开发在 Gerrit；最近 push 2026-11-08。
- **认证架构**：1.27 起用 SessionManager + AuthManager 取代 AuthPlugin；支持多认证提供者并存、联邦登录、MFA、非 cookie 会话（OAuth、TLS 客户端证书）；OIDC/LDAP 通过 PluggableAuth 等扩展接入。
- **修订模型**：每次保存产生 revision；`prop=revisions` 可取 `ids/timestamp/user/comment/sha1/size/content/tags`，带内容时 `rvlimit` 上限 50，不带内容上限 500；支持按时间或 revid 区间枚举。
- **导出**：`Special:Export` 输出 MediaWiki XML，POST 参数可带全历史（`history=1`）或按 `offset/limit` 分页；可直接导入另一个 MediaWiki；REST API 提供页面、历史、搜索。
- **存储**：MariaDB/MySQL 10.3+/5.7+、PostgreSQL 10+、SQLite 3.31+；PHP 8.3+。
- **结论**：不作为内置 Wiki（PHP + 重运维），但作为企业迁移源价值最高之一：XML 带完整历史，格式稳定。
- **来源**：[A] https://www.mediawiki.org/wiki/Manual:SessionManager_and_AuthManager 、https://www.mediawiki.org/wiki/API:Revisions 、https://www.mediawiki.org/wiki/Manual:Parameters_to_Special:Export 、https://www.mediawiki.org/wiki/Manual:Installation_requirements ；[B] https://github.com/wikimedia/mediawiki

### 3.8 DokuWiki

- **定位**：无数据库的平面文件 Wiki，安装即用、备份即拷贝目录；GPL-2.0，4.7k stars，最新 release-2026-07-14c。
- **存储与修订**：页面以文本文件存放在 `data/pages`，旧版本在 attic 目录；ACL 与用户分别存 `conf/acl.auth.php`、`conf/users.auth.php`（搜索片段与第三方安装指南可见；官方站点本次不可达，未逐页核实）。
- **认证**：内置本地用户文件，LDAP/AD/OAuth 等靠插件；权限模型是页面/命名空间 ACL，不是租户。
- **API**：XML-RPC / JSON-RPC 由插件提供（未核实具体覆盖范围）。
- **风险**：官方文档站 `dokuwiki.org` 连接被拒，无法确认当前版本的 API、修订保留策略与安全公告；不建议在未完成核实前做连接器。
- **来源**：[C] https://www.dokuwiki.org/ （本次不可达）、https://www.dokuwiki.org/config:mediarevisions （仅搜索索引）；[B] https://github.com/dokuwiki/dokuwiki

### 3.9 XWiki

- **定位**：Java 企业 Wiki/应用平台，页面 + 空间 + 翻译 + 对象/类 + 附件，扩展市场成熟；18.4.8（2026-11-03），平台仓库 1.3k stars，LGPL-2.1。
- **数据模型**：页面属于空间，可有多个版本（major/minor）与独立历史的翻译页；附件与对象在所有翻译间共享；REST 表示里通过 `rel=history/space/pages` 等链接导航。
- **REST API**：入口 `/rest`，支持 XML 与 JSON（`?media=json` 或 `Accept: application/json`）；认证支持 HTTP Basic、XWiki 会话 cookie、自定义认证（OIDC/Trusted auth 扩展）。
- **CSRF**：14.10.8+/15.2+ 对 `text/plain`、`multipart/form-data`、`application/x-www-form-urlencoded` 的写请求要求 `XWiki-Form-Token`，缺失返回 403；token 在响应头返回。
- **代价**：JVM + Servlet 容器 + 关系库，部署与升级复杂度高；扩展能力强但二次开发是 Java 技术栈。
- **来源**：[A] https://www.xwiki.org/xwiki/bin/view/Documentation/UserGuide/Features/XWikiRESTfulAPI ；[B] https://github.com/xwiki/xwiki-platform

### 3.10 Gollum

- **定位**：基于 Git 的轻量 Wiki；一个 Wiki 就是一个特定结构的 Git 仓库，页面是文件，历史是 commit。
- **修订模型**：每次保存即一次 commit，可查看/回滚任意版本；与 GitHub/GitLab Wiki 格式兼容，可直接 clone 后在本地编辑。
- **认证**：**刻意不内置用户认证**；社区用 Rack 中间件（如 `gollum-auth`）或反向代理做 Basic/CAS/SSO；无租户、无细粒度 ACL。
- **集成面**：页面即文件，迁移成本最低（直接读工作树）；但运行形态是 Ruby + 反向代理，不适合嵌入 Rust 二进制。
- **活跃度**：v6.1.0 发布于 2024-12-23，最近 push 2025-11-24，已明显放缓。
- **结论**：借鉴“revision = commit、内容即文件”的极简模型；不集成。
- **来源**：[A] https://github.com/gollum/gollum （README）；[C] https://rubydoc.info/gems/gollum-auth/0.6.0 （“does not include user authentication (on purpose)”）

---

## 4. B 组：RAG/知识库系统

### 4.1 RAGFlow

- **定位**：深度文档理解型 RAG 引擎，强项是复杂版面解析、表格/扫描件与切片可视化；v1.0.0（2026-11-06），93.7k stars，Apache-2.0。
- **技术栈**：1.0 后端为 Go（仓库主语言），解析与模型侧仍有 Python；官方 Compose 依赖 MySQL/OceanBase、Elasticsearch 8.11 或 Infinity、MinIO/S3、Kvrocks（Redis 协议）、NATS。
- **认证**：邮箱密码 + 注册开关（`ENABLE_REGISTER`）；HTTP/MCP 请求用 API Key；OAuth 登录在旧版配置文档中出现（仅搜索片段，未逐页核实）。
- **接口**：完整 REST API + MCP；数据集、文档、切片、检索、对话都有端点；没有 page/revision 概念，文档版本能力未核实。
- **可借鉴**：解析器分级、切片可视化与人工校正、检索测试集；这些正是“企业迁移后质量校验”的缺口。
- **不可集成**：五六个有状态服务，solo 直接违背零外部依赖；许可虽宽松，但运行时无法塞进单二进制。
- **来源**：[A] https://ragflow.io/docs/configurations 、https://ragflow.io/docs/acquire_ragflow_api_key ；[B] https://github.com/infiniflow/ragflow

### 4.2 Dify

- **定位**：LLM 应用开发平台（Agent、Workflow、Chatbot、RAG 管线），不是知识库产品；1.19.0，160.6k stars。
- **技术栈**：Python/Flask API + Next.js 前端 + PostgreSQL + Redis + 可插拔向量库 + 对象存储 + 插件守护进程；Compose 里还有 sandbox、websocket 等服务。
- **许可**：改版 Apache 2.0：禁止未经书面授权运行多租户环境（一个 workspace 即一个租户）；不得移除前端 LOGO/版权；另有商业许可路径。
- **认证**：邮箱密码 + OAuth（GitHub/Google）；SSO（SAML/OIDC）只在 Dify Enterprise；API Key 按应用/工作区签发。
- **风险**：多租户限制与我们的 enterprise 档直接冲突；“平台”定位会诱导把检索、编排、鉴权都搬进去，破坏“一个内核”。
- **来源**：[A] https://github.com/langgenius/dify/blob/main/LICENSE 、https://docs.dify.ai/en/self-host/deploy/configuration/environments 、https://docs.dify.ai/en/self-host/deploy/overview ；[C] https://enterprise-docs.dify.ai/en/2.5.x/administer/sso/introduction

### 4.3 AnythingLLM

- **定位**：开箱即用的本地/私有 RAG 应用，workspace + 文档上传 + 聊天 + 可嵌入 widget；v1.19.0，68k stars，MIT。
- **运行模式**：单用户模式（可给整个实例设一个密码，任何持密码者都能改配置、看全部聊天）与多用户模式（Admin/Manager/Default 三角色，按 workspace 授权）；进入多用户后不可回退。
- **认证边界**：官方 security 文档只描述账号密码与角色，未见 SSO/OIDC/SAML 配置项；是否完全不存在未核实。API Key 供程序访问。
- **存储**：应用数据用 SQLite；向量库支持 Lance、Chroma、Qdrant、Milvus、pgvector、Pinecone、Weaviate 等，代码路径含 `lance`，默认值未逐页核实。
- **可借鉴**：“单用户/多用户”两种安全边界的产品化表达，以及嵌入式 widget 的分发方式；不适合作为内核。
- **来源**：[A] https://github.com/Mintplex-Labs/anythingllm-docs/blob/master/pages/features/security-and-access.mdx ；[B] https://github.com/Mintplex-Labs/anything-llm

### 4.4 Khoj

- **定位**：个人 AI 助手/第二大脑，索引本地文档、笔记、邮件与网页；2.0.0-beta.28，38k stars，AGPL-3.0。
- **运行**：默认 `--anonymous-mode`（无认证）；启用认证后只有两条路径：Resend 驱动的邮箱 magic link，或 Google OAuth（需 prod 镜像/包）。
- **认证缺口**：官方自托管文档没有 OIDC/SAML 路径；团队化部署只能靠前置代理或改代码。
- **存储**：Docker Compose 使用 `pgvector/pgvector:pg15` 作为数据库，另含 SearXNG、沙箱与 computer 容器。
- **边界**：单人知识库模型，无 space/revision/ACL；API 可借鉴，代码不可嵌入。
- **来源**：[A] https://docs.khoj.dev/advanced/authentication/ 、https://github.com/khoj-ai/khoj/blob/master/docker-compose.yml ；[B] https://github.com/khoj-ai/khoj

### 4.5 Open WebUI

- **定位**：自托管 LLM 聊天前端（OpenAI 兼容），带知识库、RBAC、工具与 Pipelines；v0.13.0，158k stars。
- **认证**：邮箱密码、OAuth/OIDC（**同一时间只支持一个 OIDC Provider**）、LDAP、可信头（反向代理认证）；角色与组同步可映射 IdP claims。
- **会话实现（可借鉴）**：OAuth 的 access/id token 加密后存数据库 `oauth_session` 表，浏览器只拿一个不透明的 `oauth_session_id` HttpOnly cookie；支持自动刷新、每用户最多 10 个并发会话、OIDC Back-Channel Logout（需要 Redis 才能立即吊销已签发 JWT）。
- **许可**：BSD-3 风格，但第 4 条禁止移除/替换 “Open WebUI” 品牌，除非终端用户 ≤50、获得书面许可或企业许可；不能白标分发。
- **边界**：聊天 UI 优先，知识库是附件集合，没有页面/修订/空间权限模型。
- **来源**：[A] https://docs.openwebui.com/features/authentication-access/auth/sso 、https://github.com/open-webui/open-webui/blob/main/LICENSE ；[B] https://github.com/open-webui/open-webui

### 4.6 Onyx（原 Danswer）

- **定位**：企业“上下文层”，连接 50+ 应用做索引与检索，强调权限继承与 Agent 检索；v5.1.0，33.3k stars。
- **架构**：FastAPI 后端 + Next.js 前端 + Vespa 向量/搜索 + PostgreSQL + Redis + Celery worker + 对象存储（Compose 内含 object-store，具体实现随版本变化）。
- **认证**：密码登录（可整体关闭）、Google/OIDC/SAML；Community Edition 同时只能启用一个 SSO Provider，Business/Enterprise 可多 Provider；支持 allowed email domains、限制开放注册、会话过期与 IdP 同步。
- **权限**：连接器拉取文档时同时拉取 metadata 与 permissions，检索按用户权限过滤；这是企业档最值得借鉴的部分。
- **许可**：`ee/` 之外是 MIT Expat，`ee/` 目录是 Onyx Enterprise License。
- **边界**：多服务 Python 栈，无法作为 solo 依赖；连接器与 SSO 管理界面可作产品参照。
- **来源**：[A] https://docs.onyx.app/deployment/authentication/sso_providers 、https://docs.onyx.app/security/architecture 、https://github.com/onyx-dot-app/onyx （README/LICENSE）；[B] https://github.com/onyx-dot-app/onyx

### 4.7 R2R

- **定位**：带 REST API 的 RAG 检索系统，强调 agentic retrieval 与 SDK；8.1k stars，MIT。
- **活跃度**：最后一次提交 2025-11-07，最新 release v3.6.5（2025-06-06）；截至 2026-11-08 已约一年无提交，文档站根路径 404。
- **认证**：无法从官方文档核实；仓库 README 也未给出可用的认证说明。
- **风险**：停滞项目不适合作为长期依赖；即使只借鉴 API 设计，也要先确认维护状态。
- **来源**：[B] https://github.com/SciPhi-AI/R2R ；[C] https://r2r-docs.sciphi.ai/ （404）

### 4.8 Verba

- **定位**：Weaviate 官方的 RAG 演示应用（The Golden RAGtriever），带 Web UI 与多种文档导入。
- **状态**：仓库已归档，README 明确“不再开发、不修 bug、不发安全补丁”，仅作参考；BSD-3。
- **能力边界**：无认证、无多租户、无页面/修订；只是“上传文档-切片-检索-问答”的样例。
- **结论**：不采用；可作为最小 RAG 演示的教学材料。
- **来源**：[A] https://github.com/weaviate/Verba （README 归档声明）；[B] https://github.com/weaviate/Verba

### 4.9 LightRAG

- **定位**：轻量 GraphRAG 实现，实体/关系抽取 + 图 + 向量混合检索；v1.5.7，40.6k stars，MIT。
- **存储**：KV、向量、图三类存储可组合；默认内存 + 本地文件（官方明确“仅适合小规模测试，不适合生产”），生产推荐 PostgreSQL，也支持 Neo4j、Milvus、OpenSearch、MongoDB。
- **服务与认证**：提供 REST API + WebUI；认证需显式配置 `LIGHTRAG_API_KEY`，或 `AUTH_ACCOUNTS` + `TOKEN_SECRET`；未配置时所有端点公开，Ollama 兼容的 `/api/*` 默认仍开放，需 `WHITELIST_PATHS` 收紧。
- **模型约束**：嵌入模型必须在索引前确定，换模型要重嵌入；官方说明目前没有重嵌入工具，部分后端还要求建表时固定向量维度。
- **可借鉴**：图检索的 L0/L1 降级路径与实体抽取管线；不可直接嵌入（Python + 外部图/向量库）。
- **来源**：[A] https://github.com/HKUDS/LightRAG （README）；[B] https://github.com/HKUDS/LightRAG

### 4.10 Microsoft GraphRAG

- **定位**：研究型 GraphRAG 管线：LLM 抽取实体/关系、构建社区摘要，用于全局性问题的检索增强；v3.3.0，36.6k stars，MIT。
- **状态**：README 明确“基本处于维护模式，不接受新 PR、不实现新功能”，只做 bug 修复与 CVE 依赖更新。
- **形态**：Python 库 + CLI，无服务端、无认证、无多租户；索引是离线批处理，成本高，官方反复提示先小规模试。
- **可借鉴**：社区摘要与全局查询的思路，可映射到 enterprise 的图谱 L2/L3；不作为运行时组件。
- **来源**：[A] https://github.com/microsoft/graphrag （README）；[B] https://github.com/microsoft/graphrag

### 4.11 LlamaIndex

- **定位**：Python/TypeScript 的 LLM 数据框架，提供 connectors、index、retriever、query engine 抽象；v0.14.25（2026-09-21），52.8k stars，MIT。
- **形态**：库，不是服务；没有内置认证、租户、HTTP 服务或页面模型，存储全部交给后端适配器。
- **可借鉴**：Reader/Index/Retriever 的分层与“ ingestion pipeline + 事件”抽象；我们已在 Rust 侧用端口/适配器表达，不需要引入 Python。
- **风险**：抽象层多、版本演进快，直接照搬概念会带来不必要的间接层。
- **来源**：[B] https://github.com/run-llama/llama_index

### 4.12 Haystack

- **定位**：deepset 的 Python 编排框架，Pipeline + Component 模型，生产集成多；v3.4.1，26.9k stars，Apache-2.0。
- **形态**：库/框架；开源版没有内置认证、多租户与服务端；企业能力在独立商业产品。
- **可借鉴**：显式 Pipeline 图与组件契约、评测/追踪的接入点；与我们的 L2–L5 分层可以一一映射。
- **边界**：Python 运行时与单二进制约束冲突，不集成。
- **来源**：[B] https://github.com/deepset-ai/haystack

### 4.13 jev-rag

- **定位**：本地优先的开源知识搜索，默认“无向量”的 BM25 + Jev 重排；MIT，18 stars，最近 push 2026-11-03，v0.4.0。
- **索引**：文档解析为 heading-aware passage，存 SQLite 的 documents/passages 表；FTS5 做 BM25，拉丁文本 NFKC + 小写，中文用重叠 bigram，查询词去重后取前 80 个以 OR 连接；文件哈希用于跳过未变文件。
- **检索链路**：BM25 Top 30 → Jev `noul` 相关性判断（每批 10 条、最多 8 并发）→ 阈值过滤 → Top 10 证据 → OpenRouter 生成并带引用；另有 agentic 多查询 + RRF、hybrid embedding + RRF、taxonomy 路由、Passage Gate、Line Search 等可选管线。
- **工程细节**：默认路径不需要 embedding 或向量库；本地 Web 只监听 `127.0.0.1:8765`，无认证、单用户；索引与文档目录可分离。
- **可借鉴**：这是我们架构的最接近参照——SQLite FTS5 + Jev 批处理 + CJK bigram + 哈希增量；差异是它没有 space/revision/ACL，也没有多租户。
- **风险**：单人项目、无认证、无迁移/治理模型，不能作为依赖，只能作为实现与评测参照。
- **来源**：[A] https://github.com/aifabrice/jev-rag/blob/main/docs/ARCHITECTURE.md 、https://github.com/aifabrice/jev-rag （README）；[B] https://github.com/aifabrice/jev-rag

---

## 5. C 组：认证与会话实践

### 5.1 OWASP Session Management Cheat Sheet（已核实）

- **Cookie 属性**：会话 cookie 必须 `Secure` + `HttpOnly`；`SameSite` 显式设为 `Strict`（首选）或 `Lax`，不能依赖浏览器默认值，不能无 `Secure` 用 `None`；推荐 `__Host-` 前缀（要求 `Secure`、无 `Domain`、`Path=/`）。
- **作用域**：不要设 `Domain`；`Path` 尽量窄；同一主机不要混跑不同安全级别的应用。
- **ID 性质**：至少 64 bit 熵（自建生成器建议 128 bit）；只用 CSPRNG；ID 本身不承载用户信息；服务端保存会话状态。
- **服务端存储**：高熵不防“存储被读”；建议存一次性校验器（identifier + secret verifier，服务端存 SHA-256），不要存原始 token。
- **过期**：必须同时有 idle timeout 与 absolute timeout，且服务端强制；高风险应用 idle 2–5 分钟，低风险 15–30 分钟；办公应用 absolute 4–8 小时；还可在会话中途轮换 ID。
- **登录与提权**：认证成功、权限变更、密码修改后必须轮换 session ID；旧 ID 立即失效。
- **登出**：必须服务端失效，同一逻辑会话的所有 ID（含轮换/并发产生的）一起吊销；仅删 cookie 不够。
- **日志**：记录会话创建/轮换/销毁、登录/登出、权限变更、超时；不要记录原始 session ID，记录加盐哈希。
- **禁止**：不要把 token 放 `localStorage`/`sessionStorage`；XSS 一次即可全取。
- 来源：https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html

### 5.2 OWASP CSRF Prevention Cheat Sheet（已核实）

- 有服务端状态的应用用 **synchronizer token**：每会话唯一、秘密、不可预测；token 不放在 cookie 里，不放 URL/日志；通过响应体/表单隐藏字段或自定义头回传。
- 无状态或难以维护服务端状态时，用 **signed double-submit cookie**（HMAC 绑定会话），不要用 naive double-submit；cookie 注入可绕过 naive 版本。
- **Fetch Metadata**（`Sec-Fetch-Site`）可作为主信号，但必须保留 Origin 校验作为 fallback；跨站的非安全方法默认拒绝。
- **SameSite 只是纵深防御**，不能替代 token。
- 拒绝简单 content type：JSON API 要求 `application/json`，不要让 `text/plain`/表单编码绕过预检。
- 状态变更不得用 GET。
- 来源：https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html

### 5.3 OWASP REST Security Cheat Sheet（已核实）

- 只提供 HTTPS；凭据不得出现在 URL（query 里的 API Key 会进日志）。
- JWT 必须验签，禁止 `alg:none`；校验 `iss`、`aud`、`exp`（及可选 `nbf`）；登出/提前终止时用 `jti` denylist 让未过期 JWT 失效。
- API Key 不能作为敏感资源的唯一保护；限流返回 429；违规可吊销。
- 每个端点都要做访问控制；错误响应不泄露堆栈；记录审计日志。
- 来源：https://cheatsheetseries.owasp.org/cheatsheets/REST_Security_Cheat_Sheet.html

### 5.4 Outline 与 Wiki.js 的登录/会话文档（能查到的部分）

- **Outline**：官方明确“不存密码”，登录由 Provider/passkey/邮箱链接完成；工作区可绑定多个认证 Provider；OIDC 自托管有独立文档（discovery 或手工端点）；SAML、审计日志、Confluence 导入器需商业许可。API Key 可过期、可按端点 scope。**未找到**关于会话 cookie 名称、属性、idle/absolute timeout 的公开文档（未核实）。
- **Wiki.js**：官方列出 8 种认证策略与“每个 site 单独激活”的规则；API Key 按组授权、不绑定用户；personal access token 用于需要归属的写操作。**Security 管理页文档写的是 “Documentation coming soon”**，没有 cookie 属性、会话时长、CSRF 机制的公开说明（未核实）。
- **对本次决策的含义**：两个产品都把“UI 会话”和“API 令牌”分开，但都没有把会话安全写成可审计的契约。我们必须在自己的文档里把 cookie 属性、CSRF、轮换、吊销写成规范，而不是靠实现默认值。
- 来源：[A] https://docs.getoutline.com/s/guide/doc/authentication-Wr4sfjvmL1 、https://docs.getoutline.com/s/hosting/doc/oidc-8CPBm6uC0I 、https://docs.js.wiki/admin/auth 、https://docs.js.wiki/dev/api

---

## 6. 自建 vs 集成决策矩阵

评分 1–5，**所有列均按“越优越高”计分**：迁移成本列实际衡量“迁移经济性”（5 = 成本最低），部署复杂度列衡量“部署简易度”（5 = 最简单）。“合计”为等权求和，仅用于暴露取舍，不替代判断。

| 方案 | 可控性 | 迁移成本 | 认证安全 | 部署复杂度 | 许可风险 | 二次开发成本 | 与三档匹配 | 合计 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 全自建：Rust 内核 + 最小 Wiki | 5 | 3 | 4 | 5 | 5 | 4 | 5 | 31 |
| 集成 Wiki.js | 3 | 2 | 4 | 2 | 2 | 3 | 2 | 18 |
| 集成 Outline | 2 | 2 | 4 | 2 | 2 | 2 | 2 | 16 |
| 集成 BookStack / MediaWiki | 3 | 2 | 4 | 2 | 4 | 3 | 2 | 20 |
| 集成 RAG 平台（RAGFlow/Onyx/Dify） | 1 | 1 | 3 | 1 | 2 | 2 | 3 | 13 |
| 外挂 Wiki + 只做连接器 | 4 | 4 | 3 | 3 | 5 | 5 | 3 | 27 |

读法与结论：

- **可控性**：只有自建能保证 `doc_id`/`revision_id` 派生规则、ACL 过滤位置、`degraded[]` 语义不被第三方改写；RAG 平台会把这些下沉到自己的黑盒。
- **迁移成本**：集成不是“省掉迁移”，而是“迁移到别人的模型再映射回来”；BookStack/MediaWiki 的层级与我们的 space/document 模型不同，仍要写映射层。
- **认证安全**：成熟产品的 SSO 实现比自建更完整，但它们的会话细节不公开（§5.4），企业审计时无法举证；自建可以用 OWASP 条目逐条对照。
- **部署复杂度**：单二进制是硬约束。Wiki.js 3 要 PG 17/18 + Node 26，Outline 要 PG + Redis，RAGFlow 要五个以上服务，任何一个都破坏 solo 的“零外部依赖”。
- **许可风险**：AGPL 对“修改后通过网络提供服务”有源码义务；BSL 的 Document Service 限制与知识库产品正面冲突；Dify 禁止多租户；Open WebUI 禁止白标。连接器走 HTTP 边界，不链接代码，风险最低。
- **二次开发成本**：Rust 自建是“自己写但可控”；TS/Python 二次开发是“改别人的模型但受上游版本与许可约束”。
- **三档匹配**：solo 要求单进程 + loopback 免认证；team 要求 OIDC + 组 ACL；enterprise 要求多租户 + SCIM + 源 ACL。没有外部 Wiki 能同时覆盖三档；自建 + 连接器可以。

**判定**：主方案是全自建（31 分），连接器作为补充（27 分），不是二选一。外挂 Wiki 只作为迁移源与可选展示面，不进入运行时关键路径。

### 6.1 逐项目三档适配

“可”指技术上能承担该档的运行时或作为该档的参照；“否”指与硬约束（单二进制、零外部依赖、许可、模型边界）冲突。这里的判断只针对“作为本系统组件或迁移源”，不评价产品自身是否好用。

| 项目 | solo | team | enterprise | 主要理由 |
| --- | --- | --- | --- | --- |
| Wiki.js | 否 | 可（若接受 AGPL） | 否 | PG 17/18 + Node 26；AGPL；无多租户 |
| Outline | 否 | 可（OIDC；SAML 付费） | 否 | BSL；自托管面向单团队；不可嵌入 |
| BookStack | 否 | 可（PHP 运维） | 否 | PHP/MySQL；权限模型粗；无多租户 |
| Docmost | 否 | 可 | 谨慎 | AGPL+EE；导出/API 契约缺口 |
| AFFiNE | 否 | 否 | 否 | 产品边界是协作工作区，不是 Wiki 内核 |
| AppFlowy | 否 | 否 | 否 | 客户端优先；SSO 付费档；非 Wiki 模型 |
| MediaWiki | 否 | 否 | 可作迁移源 | XML 全历史；PHP 重运维；ACL 粗 |
| DokuWiki | 可（极简） | 否 | 否 | 无数据库但无认证/租户；文档不可达 |
| XWiki | 否 | 否 | 可作迁移源 | Java 平台；REST 完整；部署重 |
| Gollum | 可（git 工作流） | 否 | 否 | 无认证/ACL；文件即页面 |
| RAGFlow | 否 | 否 | 参考 | 多服务；解析能力强；无 Wiki 模型 |
| Dify | 否 | 否 | 否 | 改版 Apache 禁止多租户；平台定位 |
| AnythingLLM | 可（单用户） | 可（多用户，无 SSO） | 否 | SQLite/LanceDB；无租户；无修订 |
| Khoj | 可 | 谨慎 | 否 | 默认匿名；仅 magic link/Google |
| Open WebUI | 可（单机） | 可（OIDC） | 否 | 品牌条款；聊天 UI 非知识库 |
| Onyx | 否 | 否 | 参考 | 多服务；连接器/ACL 同步值得借鉴 |
| R2R | 否 | 否 | 否 | 约一年无提交；认证未核实 |
| Verba | 否 | 否 | 否 | 已归档 |
| LightRAG | 可（本地实验） | 否 | 参考 | 默认存储不适合生产；API 默认公开 |
| GraphRAG | 可（离线） | 否 | 参考 | 维护模式；无服务端 |
| LlamaIndex | 否 | 否 | 否 | Python 库；无认证/租户 |
| Haystack | 否 | 否 | 否 | Python 库；无认证/租户 |
| jev-rag | 参考 | 否 | 否 | 单用户无认证；最接近的检索实现 |

---

## 7. 推荐：内置 Wiki 的最小范围

### 7.1 边界

`architecture.md` §8 明确“不做通用 CMS 或协同编辑器”，`roadmap.md` P1 又要求“spaces / pages / revisions”。两者不矛盾，前提是把内置 Wiki 定义为：

- **是**：Markdown 页面的事实源；solo 的本地写作与发布面；enterprise 中 wiki 原生内容的“不可变版本快照”来源；检索与 chunk 的上游。
- **不是**：Confluence 替代品；富文本协同编辑器；评论/审批/工作流平台；插件市场；对外多租户 SaaS。

因此最小 Wiki 的能力上限是“单作者编辑 + 版本历史 + 只读回溯”，不做 OT/CRDT、不做实时多人光标、不做行内评论。

需要显式指出的口径冲突：`architecture.md` §8 写的是“wiki 编辑沿用现有工具”，`roadmap.md` P1 又要求内置最小 Wiki。本文的收敛是：内置 Wiki 只服务 solo（没有外部 Wiki 可用）和“源系统不可达时的本地编辑”；team/enterprise 的日常编辑仍在 Confluence/MediaWiki 等源系统，平台侧只读同步。如果产品决定 enterprise 也以内置 Wiki 为主，必须同时修订 §8、§5.9 的双源规则与 ACL 权威定义，不能靠实现默认值悄悄改变边界。

### 7.2 必须有的表

沿用 `architecture.md` §5 的治理信封，不新造第二套 ACL；Wiki 表只补充页面语义。

| 表 | 关键列 | 约束 |
| --- | --- | --- |
| `space` | `space_id`、`tenant_id`、`name`、`visibility`、`default_acl`、`owner_principal_id`、`retention_policy`、`created_at`、`archived_at` | 复用架构 §5.3；`default_acl` 非空；`space_id` 稳定 |
| `page` | `page_id`、`tenant_id`、`space_id`、`slug`、`title`、`current_revision_id`、`version`、`status`、`created_at`、`updated_at`、`deleted_at` | `(tenant_id, space_id, slug)` 唯一；`version` 单调递增；删除是 tombstone |
| `page_revision` | `revision_id`、`page_id`、`version`、`parent_revision_id`、`content_md`、`content_hash`、`raw_hash`、`author_principal_id`、`created_at`、`comment`、`source_system`、`source_uri`、`canonicalization_version` | 只追加，不更新；`(page_id, version)` 唯一；`content_hash` 为规范化文本 SHA-256，`raw_hash` 为 Markdown 原始字节 SHA-256 |
| `page_alias` | `alias_id`、`page_id`、`old_slug`、`created_at` | 仅做 slug 重定向（301）；旧链接不做语义合并 |
| `doc_alias` | `old_doc_id`、`new_doc_id`、`reason`、`created_at` | 架构 §5.8 已定义；页面重命名导致 `doc_id` 变化时，引用解析跟随此表 |
| `import_job` | `job_id`、`source_system`、`source_uri`、`started_at`、`finished_at`、`imported`、`skipped`、`failed`、`report_json` | 幂等键 `(source_system, source_uri, raw_hash)`；失败逐条记录 |
| `import_item` | `job_id`、`source_id`、`source_hash`、`target_page_id`、`target_revision_id`、`status`、`error` | 支持断点续跑与重放 |
| `principal` / `session` / `api_token` | 见 §10 | 认证与吊销；`session` 只存 token 哈希 |
| `audit_event` | `event_id`、`tenant_id`、`principal_id`、`action`、`target`、`at`、`request_id`、`result` | 本地追加日志；不承诺防篡改 |

`doc_id` 必须沿用架构 §5.8 与 [ADR 0006](adr/0006-doc-id-identity.md) 的内容敏感口径，不另造规则：

- `canonical_key = "wiki/" + space_id + "/" + page_slug + ".md"`（`p1`：`/` 分隔、拒绝 `..` 与绝对路径、ASCII 小写；不做 Unicode NFC）。
- `content_hash = sha256(Markdown 字节)` 小写 hex。
- `doc_id = lowercase_hex(sha256(canonical_key + "\0" + content_hash)[0..16])`，32 个 hex 字符、无前缀。
- `revision_id` 由 page revision 实现生成（`rev_` + 16 字节 hex），不参与 `doc_id`。

同一页面的同一内容跨导出/导入、跨档位保持同一 `doc_id`；内容变化生成新 `doc_id`，旧 revision 保留，`superseded_by` 指向新 ID，`as_of` 可解析旧 ID。重命名会改变 `canonical_key`，因此生成新 `doc_id`，旧 ID 通过 `doc_alias` 跟随，不能假设“重命名后身份自动不变”。

### 7.3 必须有的接口

浏览器走 cookie + CSRF，Agent/CLI 走 Bearer；两套路由物理分离（见 §10.1）。

| 方法与路径 | 语义 | 关键约束 |
| --- | --- | --- |
| `POST /v1/spaces`、`GET /v1/spaces` | 创建/列出空间 | `tenant_id` 从凭据注入，不接受请求体覆盖 |
| `POST /v1/spaces/{space_id}/pages` | 创建页面并生成 revision 1 | 同一 slug 冲突返回 409；写入 `source_system=wiki` |
| `GET /v1/pages/{page_id}` | 读当前页 | 支持 `?revision_id=` 读历史；ACL 在查询内过滤 |
| `PUT /v1/pages/{page_id}` | 编辑，生成新 revision | 必须带 `If-Match: <version>`；不匹配返回 409；不合并冲突 |
| `GET /v1/pages/{page_id}/revisions` | 历史列表 | 分页；默认按 version 倒序；不返回被永久删除的 revision |
| `POST /v1/pages/{page_id}/restore` | 恢复到指定 revision | 生成**新** revision，不删除中间历史 |
| `DELETE /v1/pages/{page_id}` | 软删除 | 写 tombstone，保留 revision 与索引清理任务 |
| `GET /v1/search`、`POST /v1/answer` | 统一检索/回答 | 与文件来源共用同一内核；`space_ids` 与 ACL 必填 |
| `POST /v1/import`、`GET /v1/import/{job_id}` | 迁移导入与进度 | 幂等；报告 `imported/skipped/failed` |
| `GET /v1/export?format=jsonl` 或 `dir` | 导出 | 同一内容下 `doc_id` 稳定；包含 revision 与治理字段 |
| `POST /v1/tokens`、`DELETE /v1/tokens/{token_id}` | Agent 令牌 | 只在创建时返回明文；列表只显示前缀 |
| `GET /v1/sessions`、`DELETE /v1/sessions/{session_id}` | 查看/吊销自己的会话 | 吊销立即生效；记录审计 |

CLI 只做同一组操作的薄封装，不拥有第二套业务逻辑。

写接口的档位策略：内核只实现一套；team/enterprise 默认只暴露读接口，日常编辑写接口需显式开启并记录审计；迁移导入走独立的 `import` 端点，不受该开关限制，但同样写审计与 `import_job`。

### 7.4 不变量

1. revision 只追加；编辑与恢复都产生新 revision；`version` 严格递增。
2. `doc_id` 在同一内容、同一 `canonical_key` 下跨导出/导入、跨档位稳定；内容变化生成新 `doc_id`，旧 revision 保留，`superseded_by` 指向新 ID；`revision_id` 不参与 `doc_id`。页面重命名按 §5.8 生成新 `doc_id`，必须写 `doc_alias`，引用解析跟随别名。
3. 新 revision 入库后触发该文档的 chunk 重建与索引更新；旧 revision 默认不参与检索，但可按 `revision_id` 显式读取。
4. 所有查询在存储层注入 `tenant_id`、`space_id`、ACL；禁止先取回再过滤。
5. 删除是 tombstone；保留策略决定物理清理，清理前必须可从导出恢复。
6. 并发编辑用版本号乐观锁，不做自动合并；冲突必须由人解决。
7. Wiki 写入与文件摄入共用 `canonicalization_version`、`chunker_version`、`extractor_version` 的记录方式，便于重建。

---

## 8. 连接器策略

### 8.1 值得做的源

判断标准：装机量、导出稳定性、历史可恢复、ACL 可获得、格式可规范化。四项里缺两项就降级。

| 优先级 | 源 | 接入方式 | 历史 | ACL 可得性 | 主要成本 |
| --- | --- | --- | --- | --- | --- |
| P0 | Confluence Cloud/DC | 空间导出 + REST API | XML 是否含完整历史需按版本验证；API 可逐版本取 | 空间/页面限制需另调权限 API | 宏与布局映射；Cloud/DC 差异 |
| P0 | MediaWiki | Special:Export XML + Action API | XML 可带全历史 | 默认只有命名空间级，页面级 ACL 需扩展 | wikitext 模板/魔术字解析 |
| P1 | Notion | Markdown+CSV / HTML 导出 + API | 导出为当前内容，不含版本历史（未逐页核实） | 导出不含权限 | 数据库关系、callout 变 HTML、路径长度 |
| P1 | Outline | 工作区/集合 JSON 导出 + RPC API | revision 可取，但导出 JSON 不含作者/权限 | 需 API 另取集合权限 | 连接器只调 API，BSL 风险主要在自托管/分发 |
| P2 | BookStack | REST 导出 MD/HTML/zip | 页面 revisions 可读（官方专页未核实） | 角色权限需另取 | 层级映射（shelf/book/chapter/page） |
| P2 | Wiki.js | `/_api` 或 Git 存储 | 页面历史可读 | 组权限需另取 | 3.x API 较新，站点级导出未核实 |
| 暂不做 | Docmost / DokuWiki / XWiki / Gollum | 导出契约未核实或过于文件化 | — | — | ROI 低，等真实客户需求 |

### 8.2 为什么是这四类

- **Confluence**：企业装机量大，空间导出格式官方稳定（HTML/XML/CSV/PDF），REST 能补历史与附件；主要风险是宏与富布局，必须在规范化阶段显式降级并报告，而不是“尽量还原”。
- **MediaWiki**：XML 导出是少数**自带完整历史**的格式，且可再次导入；对“企业知识库迁移”来说，历史保真比样式保真重要。页面级 ACL 默认不存在，必须提前告知客户“迁移后可见性按命名空间/空间重设”，不能假装能还原。
- **Notion**：导出是 Markdown+CSV，结构清晰，适合中台/产品团队；但数据库关系会退化成 CSV，权限不随导出，必须用 API 补或 fail-closed。
- **Outline**：JSON 导出适合程序化导入，API 是 RPC 风格且覆盖面广；它自身是 BSL，但连接器只调用 HTTP API，不链接代码，许可风险可控。
- **BookStack / Wiki.js**：格式简单、装机量中等，作为 P2 增量需求处理；不在第一版连接器里承诺。

### 8.3 连接器契约

- 每个连接器实现同一组端口：`list_spaces`、`list_pages(since)`、`get_revision(page, rev)`、`get_acl(page)`、`get_attachment(id)`。
- 连接器只产出中间表示（IR）：`{source_system, source_id, space_path, title, markdown, html_raw, author, created_at, updated_at, version, acl, attachments[]}`。
- 规范化与页面/修订落库在内核完成，连接器不直接写 `page`/`page_revision`。
- ACL 拿不到时默认 `deny` 并生成 `acl_unknown` 报告；禁止用“全员可见”兜底。
- 每个连接器必须有 fixture 回放测试：固定导出包 → 固定 IR → 固定页面/切片数量。

---

## 9. 迁移路径

### 9.1 阶段

| 阶段 | 输入 | 动作 | 输出 | 验收 |
| --- | --- | --- | --- | --- |
| 0 盘点 | 源系统、版本、页面数、附件量、ACL 模型 | 确认导出能力与缺口；标记不可迁移项 | 迁移清单与风险表 | 每个空间有 owner、页数、ACL 来源 |
| 1 导出 | 源系统 | 全量导出并原样存档；记录导出参数与时间 | 不可变导出包 + SHA-256 | 导出包可重复校验 |
| 2 规范化 | 导出包 | HTML/wikitext→Markdown；宏/组件映射表；链接重写；编码与 CJK 归一 | 规范化 IR + 映射报告 | 解析失败条目 100% 有原因码 |
| 3 页面/修订 | IR | 源 page id → `page_id`；最新内容→revision N；按策略导入历史；作者映射 | `page`、`page_revision`、`page_alias`、`doc_alias` | revision 数与源一致（若导入历史） |
| 4 ACL | 源 API/导出 | 组映射；未知 ACL fail-closed；生成收紧/放宽 diff | ACL 报告 | 放宽项 0 或逐条人工批准 |
| 5 分块/索引 | 当前 revision | 固定 `chunker_version` 切块；写 `doc_id`/`revision_id`；建 FTS/向量索引 | 索引代次 | 索引可从页面全量重建 |
| 6 校验 | 全量 | 计数、哈希、链接、ACL、黄金查询 | 校验报告 | 见 §9.3 |
| 7 切换 | 校验通过 | 只读双跑 → 写冻结 → 切换 → 回滚快照 | 生产流量 | 回滚可在一次快照恢复内完成 |

### 9.2 每类源的导出格式/API

| 源 | 全量导出 | 增量/API | ACL 是否随导出 | 主要保真风险 |
| --- | --- | --- | --- | --- |
| Confluence Cloud | 空间导出 HTML/XML/CSV/PDF（官方） | REST `/wiki/rest/api/content`，`body.storage`、`version`、`ancestors`；附件 `/child/attachment` | 否，需权限 API | 宏、布局、内联评论、附件链接；XML 历史完整性待验证 |
| Confluence DC | 空间 XML 导出（官方） | DC REST `/rest/api/content`（无 `/wiki` 前缀） | 否 | 宏、用户/组映射 |
| Notion | Markdown+CSV / HTML zip（官方） | API blocks/children 分页 | 否 | 数据库关系、callout 转 HTML、权限 |
| MediaWiki | `Special:Export` XML，可带全历史（官方） | Action API `prop=revisions`，`rvprop=content,timestamp,user,comment`，`rvlimit<=500` | 仅命名空间级 | wikitext 模板/魔术字 |
| Outline | 工作区/集合 JSON/MD/HTML（官方） | RPC API | 否（JSON 不含权限，作者丢失） | 1.5GB 上限、附件 |
| BookStack | API 导出 MD/HTML/纯文本/zip（官方） | REST API | 角色权限，需另取 | shelf/book/chapter 层级 |
| Wiki.js | 站点级导出未核实 | `/_api`（3.x）/GraphQL（2.x）；Git 存储 | 组权限，需另取 | 渲染器差异 |
| XWiki | XAR 未核实 | REST `/rest`，页面与历史 | 权限 API | 翻译页、对象属性 |
| Docmost | 未核实 | 未核实 | 空间/组权限 | 风险最高，暂不作为源 |
| DokuWiki | 文件复制 `data/pages` + attic（未核实） | 未核实 | ACL 文件 | 语法插件 |
| Gollum | `git clone` | git | 无 | 无 |

### 9.3 校验口径

- **计数**：`imported + skipped + failed = source_pages`；失败条目必须有 `source_id` 与错误码。
- **内容**：当前 revision 的规范化文本哈希与源端计算结果一致率 100%；不一致进入人工队列。
- **历史**：若导入历史，`page_revision` 数量与源端 revision 数一致；时间戳、作者、comment 允许缺失但必须记录缺失原因。
- **链接**：内部链接解析到 `page_id` 的比例 ≥ 99%；未解析链接保留原文并列入报告，不静默删除。
- **ACL**：生成“收紧/放宽/未知”三类 diff；“放宽”必须人工批准；“未知”按拒绝处理。
- **检索**：用既有 20 篇/20 问黄金集跑 Recall@5 ≥ 0.8；引用回跳 100%；新增迁移语料另建 20 条抽样。
- **幂等**：同一导出包重复导入，第二次 `imported=0`、`skipped=N`、`failed=0`，`doc_id` 集合不变。
- **可重建**：删除索引后全量重建，`doc_id` 集合与迁移前一致，Recall@5 下降 ≤ 0.05。

### 9.4 迁移期不变量

- 源系统的 ACL 是权威；本系统只做缓存与交集，任何“平台侧放宽”都必须显式批准并审计。
- 迁移不做语义合并：页面移动/重命名走 `page_alias`（slug 重定向）与 `doc_alias`（身份跳转），不用相似度判断“同一份文档”。
- 导出包永远保留；页面内容可从导出包重放，索引永远可从页面重建。
- 迁移期间双写只允许发生在“页面已存在且 revision 冲突可解释”的场景，否则以源为准。

---

## 10. 认证结论

### 10.1 浏览器 cookie 会话与 Agent Bearer 并存的依据

两者服务不同的威胁模型，不能互相替代：

- **浏览器**：凭据是“环境权威”（ambient authority），cookie 会被浏览器自动附带，所以必须防 CSRF、防 XSS 窃取、限制作用域。OWASP 明确 cookie 是首选的会话交换机制，并要求 `HttpOnly`、`Secure`、`SameSite`、前缀与超时。
- **Agent/CLI**：没有浏览器源，不存在“自动附带”的会话；每次请求显式带 Bearer，天然没有 CSRF 面，但必须防泄露、限权、可吊销。OWASP REST 指南要求 HTTPS、令牌不入 URL、JWT 验 `iss/aud/exp`、提前终止用 `jti` denylist。
- **共存的关键是路由隔离，不是二选一**：UI 路由只认 cookie，API 路由只认 Bearer；同一请求同时出现两者时拒绝（400/401），避免 confused deputy 与“用 cookie 调 API”绕过 CSRF。
- **主体模型**：Agent 是一等 Principal（`principal_type=agent`），有 owner、独立 scope、独立令牌；不得共享人类账号（架构 §5.2）。

这不违背 solo 的“默认免认证”：solo 只监听 loopback 时免认证是显式配置；一旦绑定非 loopback 且未启用认证，进程拒绝启动（roadmap P1 验收 9）。启用认证后，人类走 cookie、Agent 走 Bearer，代码路径与 team/enterprise 相同。

### 10.2 Cookie 属性

生产（TLS）建议：

```http
Set-Cookie: __Host-lkb_sid=<opaque-128bit>; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=...
```

- 名称用 `__Host-` 前缀：强制 `Secure`、禁止 `Domain`、`Path=/`，可防子域伪造与降级。
- `HttpOnly` 必须；`Secure` 必须；`SameSite` 显式设置。首选 `Strict`；如果产品需要从外部链接直接进入深页，用 `Lax`，但不能省略。
- 不设 `Domain`；`Path=/` 是 `__Host-` 的要求。
- solo 在纯 HTTP loopback 上启用认证时，`Secure` cookie 的接受行为依赖浏览器（localhost 通常被视为可信源，但各浏览器不一致）；为避免环境差异，本地认证可退化为无前缀 cookie + `HttpOnly; SameSite=Strict`，并把该部署标记为“仅本机”；一旦对外监听，必须 TLS + `__Host-`。
- 不要在 cookie 里放角色、租户、权限等可被读取的状态；cookie 只放不透明 ID。
- 响应加 `Cache-Control: no-store`；登出可选 `Clear-Site-Data: "cache", "cookies", "storage"`。

### 10.3 CSRF

- **同步令牌**：登录后生成每会话 token，服务端存哈希；写请求必须带 `X-CSRF-Token`（JSON）或隐藏字段（表单）。token 不写 cookie、不进 URL、不进日志。
- **校验范围**：所有非安全方法（POST/PUT/PATCH/DELETE），包括导入、令牌创建、页面恢复；读请求不改状态，不需要。
- **纵深防御**：校验 `Origin`，有 `Sec-Fetch-Site` 时拒绝 `cross-site` 的非安全方法；无该头时回退到 Origin + token。
- **内容类型**：JSON API 只接受 `application/json`，拒绝 `text/plain`、`application/x-www-form-urlencoded`、`multipart/form-data`（文件上传单独端点并单独校验）。
- **SameSite**：作为第三层，不作为唯一防线。
- **失败语义**：缺 token 或不匹配返回 403；不要返回 401 混淆认证失败。
- **Bearer 路由**：不做 CSRF 校验，但必须拒绝 cookie；否则等同于把 cookie 会话暴露给 CSRF。

### 10.4 会话存储

```text
session(
  session_id_hash  TEXT PRIMARY KEY,  -- SHA-256(随机 verifier)，不存明文
  principal_id     TEXT NOT NULL,
  tenant_id        TEXT NOT NULL,
  created_at       INTEGER NOT NULL,
  last_seen_at     INTEGER NOT NULL,
  idle_expires_at  INTEGER NOT NULL,
  absolute_expires_at INTEGER NOT NULL,
  ua_hash          TEXT,
  ip_hash          TEXT,
  revoked_at       INTEGER
)
```

- token 生成：CSPRNG ≥128 bit；可用“公开 identifier + 秘密 verifier”拆分，服务端存 verifier 哈希，查询时常量时间比较。
- 轮换：登录成功、权限变更、密码修改时生成新 session，旧 session 立即 `revoked_at`；不要保留“旧 ID 仍可用”的宽限期。
- 超时：建议 idle 30 分钟、absolute 8 小时（team/enterprise）；solo 本机可放宽到 absolute 7 天，但必须是显式“记住此设备”，且只允许 loopback 或 TLS。
- 清理：后台任务按 `idle_expires_at`/`absolute_expires_at` 删除；删除前写审计。
- 会话固定：只接受服务端生成过的 ID；收到未知 ID 直接拒绝并记安全事件。
- 日志：记录 session 创建/轮换/吊销、登录/登出、失败原因；只记录 `session_id_hash` 的加盐哈希，不记录原值。

### 10.5 令牌吊销

```text
api_token(
  token_id      TEXT PRIMARY KEY,   -- 公开前缀，用于查找
  principal_id  TEXT NOT NULL,
  tenant_id     TEXT NOT NULL,
  scopes        TEXT NOT NULL,      -- JSON array
  created_at    INTEGER NOT NULL,
  expires_at    INTEGER NOT NULL,
  last_used_at  INTEGER,
  revoked_at    INTEGER,
  token_hash    BLOB NOT NULL       -- SHA-256(secret)
)
```

- 格式建议 `lkb_<token_id>_<secret>`；服务端按 `token_id` 查找，常量时间比较 `token_hash`；数据库泄露不等于令牌可用。
- 默认有效期 90 天；支持轮换与重叠期（新令牌生效后再吊销旧令牌）；`revoked_at` 非空立即拒绝。
- scope 最小化：`search:read`、`answer:read`、`wiki:write`、`import:run`、`admin`；默认不给 `admin`，不支持隐式通配。
- 主体禁用/删除时级联吊销其全部令牌与会话；审计记录 `token_id`，不记录 secret。
- 优先使用**不透明令牌**而非 JWT：吊销即时、无需 denylist。若必须用 JWT（联邦/跨服务），TTL 控制在分钟级，校验 `iss/aud/exp/nbf`，登出写 `jti` denylist。
- 认证失败返回 401 并带 `WWW-Authenticate: Bearer`；限流返回 429；不要把令牌放 query。

### 10.6 三档落地

| 档位 | 人类入口 | Agent 入口 | 会话/令牌策略 |
| --- | --- | --- | --- |
| solo | 默认 loopback 免认证；启用后本地账号 + cookie + CSRF | 本地签发 Bearer | 单 Principal；cookie absolute 可 7 天；令牌手动吊销 |
| team | OIDC（authorization code + PKCE）+ cookie 会话；组→space | 每个服务账号独立 Bearer | idle 30 分钟 / absolute 8 小时；OIDC RP-initiated logout；会话轮换 |
| enterprise | IdP 联邦 + SCIM + 组映射；可选 SAML | 工作负载身份 + 短 TTL Bearer | 会话过期与 IdP 同步；集中审计；令牌按主体/租户限权；支持批量吊销 |

OIDC 实现要点：校验 `state` 与 `nonce`；只接受配置的 issuer；回调地址固定；不在前端保存 access token；需要调用下游时由后端持有并刷新（参考 Open WebUI 的 server-side session 表做法，但不引入其代码）。

### 10.7 对现有产品的取舍

- **不学 Outline 的“无密码”作为唯一路径**：它依赖外部 IdP，solo 没有 IdP；我们保留本地账号，但 team 默认 OIDC。
- **学 Wiki.js 的“API Key 不绑定用户”要谨慎**：这会让审计无法归因；我们的 Agent 令牌必须绑定 `principal_id`，写操作可归因。
- **学 Open WebUI 的“浏览器只拿不透明会话 ID，token 存服务端”**：这是 cookie 与外部 IdP token 共存的正解。
- **不学 Docmost 的 30 天 JWT 默认值**：对 team/enterprise 过长；会话与令牌都应短 TTL + 可吊销。

---

## 11. 未核实清单

以下条目在本文中被引用，但本次未能取得官方页面或未能逐页核实；实现前必须复核。

1. **DokuWiki**：官方站点 `dokuwiki.org` 连接被拒。存储目录（`data/pages`、attic）、修订保留、ACL 文件、XML-RPC/JSON-RPC 覆盖面均为 C 级；未核实当前版本的安全公告。
2. **R2R**：`r2r-docs.sciphi.ai` 根路径与 self-hosting 路径 404；认证方式、默认存储、部署要求未核实；仅确认仓库最后提交 2025-11-07。
3. **AnythingLLM**：官方文档站返回 403；改用 GitHub 文档源码。SQLite + LanceDB 为默认组合、API Key 细节、工作区导入格式未逐页核实。
4. **BookStack**：`/docs/admin/requirements/`、`/docs/admin/api/` 返回 404；MySQL/MariaDB 为运行库、SAML/LDAP 细节、页面修订的官方专页未逐页核实（OIDC 页与 API 文档已核实）。
5. **Docmost**：站点级导出格式、公开 API 契约、SAML 完整配置未核实；仅确认 OIDC、环境变量、页面历史功能与 JWT 默认 30 天。
6. **AFFiNE**：内容导出/批量 API、后端目录的具体许可文本未核实；仅确认 LICENSE 的分割结构与 OAuth/OIDC 配置。
7. **AppFlowy**：PostgreSQL/Redis/MinIO 的官方部署文档未逐页核实；仅确认 OIDC 文档提到的 GoTrue 容器、付费档限制、SCIM/SAML 存在。
8. **XWiki**：XAR 导出、默认数据库（HSQLDB）与集群部署未逐页核实；REST、版本模型、CSRF 已核实。
9. **Wiki.js**：3.x 的站点级导出、存储模块清单、会话 cookie 属性与超时未核实；`/admin/storage` 与 `/admin/security` 均写 “Documentation coming soon”；Git 存储仅见 2.x 文档与搜索片段。
10. **Outline**：会话 cookie 名称/属性/超时未公开；OIDC 是否在所有自托管版本可用、SAML 许可边界以官方 “Business + Enterprise” 页为准，未做版本级验证。
11. **MediaWiki**：页面级 ACL 默认不存在这一点基于 AuthManager/权限模型常识与文档结构，未逐页核实扩展组合；XML 导入的限制未逐页核实。
12. **RAGFlow**：OAuth 登录的配置项与支持 Provider 仅见旧版文档搜索片段；文档版本能力未核实。
13. **Dify**：SSO 仅企业版来自企业文档与自托管文档的提示，未做 Community 版实测；许可证限制的完整条款只读了前 800 字节。
14. **Open WebUI**：默认数据库与 PostgreSQL 切换、知识库文件模型未逐页核实；SSO、会话表、许可证品牌条款已核实。
15. **LightRAG**：各存储后端的最低版本与迁移工具未逐页核实；API 认证开关与默认公开行为已核实。
16. **GraphRAG**：输出工件格式（parquet 等）与索引成本未逐页核实；维护模式声明已核实。
17. **jev-rag**：Seven pipelines 的完整参数、评测口径与 v0.4.0 的发布说明只读了 README/ARCHITECTURE；未做代码级复核。
18. **Confluence / Notion 迁移保真**：Confluence XML 是否包含完整历史、Notion 导出是否包含版本历史，均未逐页核实；两者的 ACL 都不随导出，必须另取或 fail-closed。
19. **Onyx**：对象存储的具体实现（Compose 内的 object-store）随版本变化，未逐页核实；SSO、架构、许可边界已核实。
20. **许可证**：所有 SPDX 标识来自 GitHub API 或仓库 LICENSE 文件；未做法律意见。AGPL/BSL/改版 Apache 的实际义务需法务确认。

---

## 12. 关键参考

- OWASP Session Management Cheat Sheet：https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html
- OWASP CSRF Prevention Cheat Sheet：https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html
- OWASP REST Security Cheat Sheet：https://cheatsheetseries.owasp.org/cheatsheets/REST_Security_Cheat_Sheet.html
- RFC 6750（Bearer Token Usage）：https://datatracker.ietf.org/doc/html/rfc6750
- 本仓库相关文档：[architecture.md](architecture.md) §5、§8；[roadmap.md](roadmap.md) P1；[tier-matrix.md](tier-matrix.md)
