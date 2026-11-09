---
title: 内置 Wiki
slug: 04-wiki
lang: zh
order: 4
summary: 内置 Wiki 的页面创建、编辑、版本历史、并发保存行为、权限与语言边界。
---

# 内置 Wiki

内置 Wiki 只做四件事：空间、页面、修订、按语言分列。它不做富文本协同、评论、审批或插件系统；页面内容保存后会编译成可检索文档，因此“写下来就能被搜到”是同一条链路。

## 1. 访问入口

两套入口指向同一份数据：服务端渲染的 HTML 页面，以及 SPA 使用的 JSON API。

| 入口 | 路径 | 角色/权限 |
| --- | --- | --- |
| HTML 页面列表 | `GET /` | 登录（solo 免认证） |
| HTML 阅读 | `GET /w/{space}/{slug}` | 空间 `viewer` |
| HTML 编辑表单 | `GET /w/{space}/{slug}/edit` | 空间 `editor`；空间必须 `active` |
| HTML 保存 | `POST /w/{space}/{slug}`（表单 `csrf_token`） | 空间 `editor`；**无 If-Match** |
| HTML 历史 | `GET /w/{space}/{slug}/history` | 空间 `viewer` |
| JSON 读取/保存/删除 | `GET|PUT|POST|DELETE /api/v1/pages/{space}/{slug}` | `editor`（删除需 `space_owner`） |
| JSON 历史/单版本 | `GET .../history?include_markdown=true`、`GET .../revisions/{revision_id}` | 读权限 |
| JSON 恢复 | `POST .../restore` | `space_owner` |
| 页面列表 | `GET /api/v1/pages?space&lang&q&limit&cursor` | 读权限 |
| 静态摘要 | `GET .../summary?lang=` | 空间 `viewer` |
| SPA 路由 | `/w/:space/:slug`、`/edit`、`/history`、`/create/:space` | 由前端 `RequireAuth` 控制 |

页面身份是 `(tenant, space, lang, slug)`：同一个 slug 在中英文下是两篇页面，引用按语言定位。

## 2. 创建页面

SPA 中打开 `/create/{space}?slug=<slug>&lang=<zh|en>`；或直接访问 `/w/{space}/new-page/edit`。slug 规则：非空、长度不超过 200 字节、不含 `/` 或 `\`、不能是 `.` 或 `..`。标题不能为空，JSON 保存时 `markdown` 也不能为空。

首次保存会创建 Page 与版本号为 1 的 Revision；`space=product` 中的内置文档由 `lkb seed` 以 `system:seed` 作者写入，用户编辑后种子不再覆盖该页。

## 3. 编辑与保存

保存时：版本号加一、写入新 Revision（不可变快照）、页面指向新 Revision、以 `wiki/{space}/{lang}/{slug}.md` 重新编译 Document/Chunk 并同步索引。索引或配额失败会返回错误；配额在保存前预留、保存后重算、失败释放。空间已归档或软删除时写入返回 409 `space_not_active`。

- **HTML 表单路径**：只有 `csrf_token`、`lang`、`title`、`markdown`；没有版本前置检查，后提交者覆盖先提交者（last-write-wins）。
- **JSON API**：`PUT`/`POST` 必须带 `If-Match`，值就是当前 `current_revision_id`。

## 4. 历史、单版本与恢复

- `GET .../history` 返回版本列表（版本号、作者、时间、`content_hash`、语言）；`include_markdown=true` 时同时返回每版 Markdown。
- `GET .../revisions/{revision_id}` 读取单个修订的完整正文。
- `DELETE /api/v1/pages/{space}/{slug}?lang=` 归档页面（写 `valid_to` 并同步隐藏索引文档），需要 `space_owner`。
- `POST .../restore` 恢复已归档页面，同样需要 `space_owner`。

历史是只读的：恢复操作产生新的可见状态，不改写旧 Revision。

## 5. 并发保存与 If-Match

JSON API 的写入协议：

1. 先 `GET /api/v1/pages/{space}/{slug}`，拿到 `current_revision_id`（形如 `rev_` + 32 位十六进制）与 `version`。
2. 提交 `PUT`（或 `POST`）时带上 `If-Match: <current_revision_id>`。
3. 缺少 `If-Match` → **400 `bad_request`**（"If-Match header is required"）；`If-Match: *` 显式跳过检查。
4. 值不匹配 → **409 `revision_conflict`**；客户端应重新读取后提交。

响应不会返回 `ETag` 头；版本标识通过正文里的 `current_revision_id` 传递。HTML 表单路径没有该检查，仍是最后写入生效；多人协作时优先使用 JSON API 或 SPA 编辑器。

## 6. 权限与可见性

- 读取需要空间角色至少 `viewer`，写入至少 `editor`；删除/归档/恢复需要 `space_owner`。
- 空间成员资格与文档 ACL 取交集：空间成员、文档 ACL 两层都通过才返回。
- 页面文档 ACL 当前统一为显式 `allow_all`，实际权限由空间角色决定（空间成员至少 `viewer` 才能读，`editor` 才能写）；导入的普通文档仍可带真实 ACL，两种门禁取交集。
- 空间 `archived` 后只读；软删除的空间不再出现在列表中。
- 读不到页面返回 404；页面存在但不在你的 ACL 内时，写入返回 403，避免通过存在性探测内容。

## 7. 语言与切换

内容语言 `lang` 是页面身份的一部分，并且是召回 SQL 的过滤条件（`pages.lang`、`documents.lang`、`chunks.lang`、`events.lang` 都有索引）。界面语言与内容语言分开：

- **界面语言**：SPA 右上角 LocaleToggle 在 `zh-CN` 与 `en` 之间切换，保存在 `localStorage['lkb-locale']`，同时决定 API 的 `Accept-Language`。
- **内容语言**：TopBar 的语言选择器保存在 `localStorage['lkb-content-lang']`，作为 `lang` 参数随检索/问答发送；Wiki 阅读/编辑/历史页用 `?lang=` 指定。
- 种子内容按 `docs/zh` 与 `docs/en` 分别导入，`slug` 对齐；打开英文文档用对应语言的页面或 `?lang=en`。
- CLI：`lkb page list --lang zh`、`lkb page show <space> <slug> --lang en`。

## 8. CLI 操作

```bash
./target/release/lkb page list --space product --lang zh
./target/release/lkb page show product 01-overview --lang zh
```

Windows PowerShell 将 `./target/release/lkb` 换成 `.\target\release\lkb.exe`。team / enterprise 需要 `--token` 或 `LKB_API_TOKEN`。

## 9. 相关文档

- [02 核心概念](02-concepts.md)
- [05 检索与 AI 回答](05-search-ai.md)
- [06 认证与安全](06-auth-security.md)
