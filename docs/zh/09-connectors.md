---
title: 外部知识源与连接器
slug: 09-connectors
lang: zh
order: 9
summary: 飞书、SharePoint、OneDrive、Confluence、MediaWiki 等外部源的接入边界，只做引用与深链，不存网盘，ACL 以源为准。
---

# 外部知识源与连接器

连接器框架已经落地：外部系统仍是事实源，本系统只保存文本快照、定位符、深链与 ACL 镜像，不下载原件、不接管外部权限、不写回源系统。真实厂商适配器（飞书、SharePoint、OneDrive、Confluence、MediaWiki）目前是配置校验 stub，可用的三类源是 `filesystem_export`、`jsonl` 与 `http_json`。

## 1. 设计原则

1. **源系统是权威。** 内容、权限、版本都以源为准；本系统保存文本、定位符、哈希与 ACL 快照。
2. **只做引用与深链。** 不下载并长期保存网盘附件，不做双向同步，不在源系统写回。
3. **ACL 以源为准。** 同步时读取条目 ACL 并写入文档；读不到就按拒绝处理。
4. **不嵌入第三方运行时。** 连接器只通过 HTTP/文件交换数据，不把 Confluence、MediaWiki 或网盘服务嵌进进程。
5. **可重建、可幂等。** 同一 `external_id` 的同步会更新引用与文档；重复同步不产生重复文档。

## 2. 连接器类型与实现状态

`kind` 的完整取值（`SUPPORTED_KINDS`）：

| kind | 状态 | 数据来源 | 说明 |
| --- | --- | --- | --- |
| `filesystem_export` | **可用** | `config.dir` 下的 `manifest.json` | 读取导出清单；`config.manifest` 可改文件名 |
| `jsonl` | **可用** | `config.path` | 每行一个 JSON 对象 |
| `http_json` | **可用** | `config.base_url` | GET 返回数组或 `{"items":[...]}`；`config.api_key_env` 可选 |
| `feishu` | 配置校验 stub | — | `test`/`sync` 返回 `connector_not_configured` |
| `sharepoint` | 配置校验 stub | — | 同上 |
| `onedrive` | 配置校验 stub | — | 同上 |
| `confluence` | 配置校验 stub | — | 同上 |
| `mediawiki` | 配置校验 stub | — | 同上 |

**stub 的准确含义**：可以创建、修改、删除记录，也可以调用 `test`/`sync`，但不会发起任何厂商 OAuth 或 API 请求；`test` 返回 200 且 `ok:false`、`status:"not_configured"`、`code:"connector_not_configured"`，`sync` 任务以 `failed` 结束并记录同一错误码。真实飞书/SharePoint/OneDrive/Confluence/MediaWiki 接入尚未实现。

## 3. 配置字段

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `space_id` | 是 | 目标空间；必须是已存在的空间 |
| `kind` | 是 | 上表取值 |
| `name` | 是 | 租户内唯一（`UNIQUE(tenant_id, name)`） |
| `config` / `config_json` | 否 | 连接器配置对象；必须是合法 JSON |
| `secret_env` | 否 | 密钥所在环境变量名；API 不存密钥值。当前内置连接器不读取该字段，`http_json` 的认证读取 `config.api_key_env` |
| `status` | 否 | 默认 `active` |
| `connector_id` | 否 | 缺省生成 `conn_` + 10 位十六进制 |

各 kind 的 `config`：

- `filesystem_export`：`dir` 必填；`manifest` 默认 `manifest.json`。创建/修改时会读取清单做校验。
- `jsonl`：`path` 必填；逐行解析，空行跳过。
- `http_json`：`base_url` 必填；可选 `api_key_env`，非空时以 `Authorization: Bearer` 发送；请求超时 15 秒。

条目字段（清单、JSONL 行或 HTTP 响应中的每一项）：`external_id`（也接受 `id`/`path`/`url`）、`url`（深链）、`title`、可选 `revision`、`acl`、`content`、`mime`、`lang`。`content` 存在且非空时才会索引正文；否则只记录引用。

创建/查询的 JSON 响应会打码 `config` 中包含 `secret`/`token`/`api_key`/`password`/`authorization` 的键，并返回 `secret_configured` 表示 `secret_env` 指向的变量是否已设置；密钥值本身既不返回也不落库。`secret_env` 当前只用于存储与状态展示，实际认证仍以各连接器的 `config` 为准。

## 4. 同步、外部引用与深链

`POST /api/v1/connectors/{id}/sync` 返回 **202** 与 `{job_id, status:"queued"}`，任务在后台执行。每个条目独立处理，单条失败不影响其余条目。

| 结果 | 行为 |
| --- | --- |
| 有 `content` | 写入/更新文档，路径为 `connectors/{kind}/{safe_external_id}.md`，`source_role=external`，`lang` 缺省 `zh`；分块、可选 embedding，并入队摘要 |
| 无 `content` | 只写 `external_refs`，统计为 `refs_only`；不产生可检索文档 |
| 每个条目 | upsert `external_refs`：`provider=kind`、`external_id`、`url`（深链）、`revision`、`acl_json`、`fetched_at`、`status`、`doc_id`（refs-only 时为空） |
| 分页 | 每页 100 条，最多 50 页（5,000 条） |
| 任务状态 | `completed`（无失败）、`partial`（部分失败但有成功）、`failed`（全部失败或无成功条目） |
| 统计 | `items`、`indexed`、`refs_only`、`failed`、`errors[]` |

同步是幂等的：相同 `external_id` 的文档按 `doc_id` 内容寻址更新，引用按 `(tenant, provider, external_id)` 更新，重复执行不会产生重复文档（`tests/connectors.rs` 覆盖）。

深链就是条目 `url`，保存在 `external_refs.url`；回答引用仍指向 `doc_id` 与行号，不依赖外部系统在线。

## 5. ACL 以源为准

- 条目带 `acl` 时按 `Acl::from_json` 解析为 ACL 快照，写入文档与引用。
- 条目缺少 `acl` 或解析失败时，使用**空 allow list**（拒绝），不会回退到 allow-all；`tests/connectors.rs` 明确断言“missing source ACL must deny”。
- 最终访问仍然是两层：空间成员角色（至少 `viewer`）∩ 文档 ACL；因此同步后还需要把用户/组加入目标空间。
- 没有 ACL 同步守护进程：源权限变化后需要重新 sync；不写回源系统。

## 6. HTTP API 与功能门禁

所有连接器接口要求 **admin scope**，并受 `features.connectors` 门禁；关闭时返回 403 `feature_disabled`。

| 方法与路径 | 作用 |
| --- | --- |
| `GET /api/v1/connectors` | 列出连接器（配置打码） |
| `POST /api/v1/connectors` | 创建；校验 kind、space、JSON、构造器 |
| `PATCH /api/v1/connectors/{id}` | 修改 name/space/status/secret_env/config |
| `DELETE /api/v1/connectors/{id}` | 删除记录 |
| `POST /api/v1/connectors/{id}/test` | 连通性测试；stub 返回 200 + `connector_not_configured` |
| `POST /api/v1/connectors/{id}/sync` | 入队并后台执行；202 |
| `GET /api/v1/connectors/{id}/jobs?limit=50` | 任务历史 |
| `GET /api/v1/external-refs?doc_id&provider&limit=200` | 外部引用与深链 |

`features.connectors` 默认值：solo 关闭，team/enterprise 开启；可在 `[features]` 显式覆盖。它不是“厂商适配器已就绪”的开关，只是连接器 API 的总门禁。

## 7. 当前边界

- 只有 `filesystem_export`、`jsonl`、`http_json` 真正读取数据；五个厂商 kind 是 stub，**没有真实飞书/SharePoint/OneDrive/Confluence/MediaWiki 接入**。
- `http_json` 只做一次 GET，不分页、不处理 OAuth 刷新、不下载附件；条目必须由外部导出脚本或网关整理成约定 JSON。
- 不在本系统保存二进制原件，不做增量 webhook，不写回源系统，不做 ACL 自动同步。
- 连接器产出的文档是 `Document/Chunk`（`source_role=external`），不创建 Page/Revision，也不进入内置 Wiki 历史。
- `citations[].source_url` 当前不填充；深链信息保存在 `external_refs.url` 与文档 `source_uri`。

## 8. 迁移检查清单

- [ ] 每个源空间有明确 owner、条目数与 ACL 来源。
- [ ] 导出包保留原始格式与 SHA-256，可重复校验。
- [ ] 条目必须带稳定 `external_id` 与 `url`；缺失 ACL 的条目按拒绝处理。
- [ ] 同步后统计 `items/indexed/refs_only/failed`，失败条目有 `external_id` 与错误码。
- [ ] 把用户/组加入目标空间（空间角色 ∩ 文档 ACL 才是最终可见性）。
- [ ] 用固定问题集跑 `lkb eval`，记录 Recall@k 与 MRR 基线。
- [ ] 重复 sync 不产生重复文档，`external_refs` 计数稳定。

## 9. 相关文档

- [02 核心概念](02-concepts.md)
- [06 认证与安全](06-auth-security.md)
- [08 部署与运维](08-deployment.md)
- [11 常见问题与排错](11-faq.md)
