# 私有笔记（所有层级的每个用户）

> 状态：0.2.2 现状说明（2026-11-09）
> 相关文档：[spaces-and-permissions.md](spaces-and-permissions.md)、[desktop-app-and-notes.md](desktop-app-and-notes.md)（设计基线）、[tier-matrix.md](tier-matrix.md)、[plugin-api.md](plugin-api.md)、[pre-release-overview.html](pre-release-overview.html)
> 说明：本文只描述已经实现的能力；桌面 vault、实时协同等仍是后续工作。

## 中文

### 核心模型

- **每个用户都有自己的私有笔记空间**：无论 solo、team 还是 enterprise，登录用户都可以拥有一个 `notes_<principal>` 空间。该空间 `visibility = private`、`owner_type = user`、`owner_id = 当前用户`，只有本人可见。
- **空间 = 笔记本，页面 = 笔记**：笔记不是另一套存储，而是私有空间中的 Wiki `Page / Revision`。因此历史版本、乐观锁、搜索与摘要等现有能力直接适用。
- **入口统一**：侧栏「我的笔记」（`/notes`）和顶部栏「新建笔记」面向所有层级；`/spaces` 继续管理共享空间与自建空间，不承担私有笔记入口。

### 私有笔记 API（后端契约）

- `POST /api/v1/notes/space`：幂等确保当前用户的私有空间，返回 `{space: {...}}`；重复调用返回同一个空间。
- 空间命名 `notes_<safe principal_id>`；非法字符使用稳定哈希后缀。`notes_` 前缀为保留命名。
- 私有空间的内容由页面 ACL 与空间成员资格共同保护；租户管理员也不能通过普通页面接口读取他人笔记内容（后端返回 404）。
- 尚未部署该端点时（404），前端 `/notes` 回退到当前选择的空间，并显示「笔记空间暂不可用，回退到当前空间」；顶部栏新建失败时同样回退并提示。

### 现在可以做什么

- 在 `/notes` 查看当前用户私有空间中的页面（标题、摘要、版本、更新时间），点击打开 `/w/...`。
- 顶部栏「新建笔记」会先确保私有空间，再进入 `/create/{notes_space}` 编辑；保存后页面即成为一条笔记。
- 每次保存生成新的 revision；可查看历史、比较版本，并通过 revision 乐观锁避免静默覆盖。
- 默认 SQLite 单文件存储；同一二进制也支持 PostgreSQL（solo + PG / team / enterprise）。
- 统一搜索、问答与静态摘要会索引私有空间中的页面；可见性仍由 ACL 约束，不会跨用户召回。
- **笔记空间生命周期保护**：笔记空间本身不可删除、归档或转移；后端对 DELETE、archive、transfer 返回 403 `notes_space_immutable`。要删除单条笔记，请删除对应页面（页面仍可删除）。

### 已实现（v0）

- **块编辑（v0）**：编辑器支持 Markdown / Blocks 视图切换；按空行、代码围栏与表格切分块，支持拖拽和上下移动、块类型工具栏、实时预览。Markdown 仍是事实源，保存仍写入现有 `Page / Revision`。
- **插件 v0**：提供 manifest、hooks、block renderer 与 command registry；插件随 Web 包编译、在进程内注册，不下载、不执行远程 JavaScript，也没有沙箱或插件市场。详见 [plugin-api.md](plugin-api.md)；发布说明见 [pre-release-overview.html](pre-release-overview.html)。

### 如何开始

1. 启动 `lkb serve` 并登录（solo 默认无认证、仅 loopback）。
2. 打开侧栏「我的笔记」，或点击顶部栏「新建笔记」。
3. 新建笔记时填写 slug、标题、正文并保存；之后从 `/notes`、页面树或搜索打开。
4. 如需共享内容，使用「空间」（`/spaces`）创建/加入共享空间，并按空间角色授权。

### 尚未实现（诚实说明）

- **实时协同/CRDT**：没有多人光标、协同编辑或 CRDT；块编辑器只提供本地实时预览。
- **桌面本地 Markdown vault**：没有文件系统 vault、文件监听、frontmatter 标签、`[[wikilink]]`、反向链接；桌面壳已存在，但笔记事实源仍是数据库中的 `Page / Revision`。
- **外部插件加载/插件市场**：插件 v0 只支持编译内置模块；没有远程 JavaScript 下载、沙箱执行或插件市场。
- **发布/共享私有笔记的一键流程**：目前通过共享空间成员资格协作；没有把私有笔记复制/发布到共享空间的一键操作，也没有页面级公开分享。

### 与共享空间的边界

- 私有笔记空间默认仅本人可见，`notes_` 前缀为保留命名；不要把它当作团队共享空间使用。
- 笔记空间本身不可删除、归档或转移（403 `notes_space_immutable`）；删除单条笔记请删除页面，共享空间的生命周期操作不适用于私有笔记空间。
- 共享发生在**空间**粒度：`viewer` 只读、`editor` 可写、`space_admin` 管理成员与配额、`space_owner` 拥有所有权；没有单页级独立授权。
- 空间配额、审计、组与多用户治理属于团队/企业能力；个人部署保持单用户、loopback、零运维。配额开关按部署层级配置，与私有笔记是否存在无关。

## English

### Core model

- **Every signed-in user has a private notes space**: in solo, team and enterprise alike, the backend ensures a `notes_<principal>` space with `visibility = private`, `owner_type = user` and `owner_id` set to the current user. Only that user can see it.
- **A space is a notebook, a page is a note**: notes reuse the Wiki `Page / Revision` model inside the private space, so history, optimistic locking, search and summaries apply directly.
- **One entry point**: the sidebar's “My notes” (`/notes`) and the top bar's “New note” work for every tier; `/spaces` remains the management surface for shared and self-created spaces.

### Private notes API (backend contract)

- `POST /api/v1/notes/space` idempotently ensures the current user's private space and returns `{space: {...}}`; repeated calls return the same space.
- Space ids are `notes_<safe principal_id>` with a stable hash suffix when characters are unsafe; the `notes_` prefix is reserved.
- Content is protected by page ACLs plus space membership; tenant admins cannot read another user's notes through the normal page API (the backend returns 404).
- When the endpoint is not deployed yet (404), `/notes` falls back to the currently selected space and shows “Notes space unavailable; falling back to the current space”. The top-bar action falls back the same way.

### What works today

- `/notes` lists pages in the current user's private space (title, summary, version, updated time); clicking a row opens `/w/...`.
- The top-bar “New note” action ensures the private space first, then opens `/create/{notes_space}`; saved pages are notes.
- Each save creates a new revision with history and optimistic locking.
- SQLite is the default store; the same binary also supports PostgreSQL (solo + PG, team, enterprise).
- Unified search, answers and static summaries index pages in the private space while ACLs still prevent cross-user recall.
- **Notes-space lifecycle protection**: the notes space itself cannot be deleted, archived or transferred; the backend returns 403 `notes_space_immutable` for DELETE, archive and transfer. To remove an individual note, delete its page (pages remain deletable).

### Implemented (v0)

- **Block editing (v0)**: the editor switches between Markdown and Blocks; blocks are split on blank lines, code fences and tables, with drag/up-down reordering, a block-type toolbar and live preview. Markdown remains the source of truth and saves through the existing `Page / Revision` store.
- **Plugin v0**: manifest, hooks, block renderer and command registry are available. Plugins are compiled into the web bundle and registered in-process; the host never downloads or executes remote JavaScript and provides no sandbox or marketplace. See [plugin-api.md](plugin-api.md); release notes are in [pre-release-overview.html](pre-release-overview.html).

### Getting started

1. Start `lkb serve` and sign in (solo defaults to no auth, loopback only).
2. Open “My notes” in the sidebar, or click “New note” in the top bar.
3. Fill in slug, title and body, then save; reopen notes from `/notes`, the page tree or search.
4. To share content, use “Spaces” (`/spaces`) to create/join a shared space and grant roles there.

### Not implemented yet (honest scope)

- **Real-time collaboration/CRDT**: no multiplayer cursors, co-editing or CRDT; the block editor provides local live preview only.
- **Desktop local Markdown vault**: no filesystem vault, file watcher, frontmatter tags, `[[wikilinks]]` or backlinks; the desktop shell exists, but notes remain database `Page / Revision` rows.
- **External plugin loading/marketplace**: plugin v0 only supports compiled-in modules; no remote JavaScript download, sandboxed execution or marketplace.
- **One-click publish/share of a private note**: collaboration happens through shared-space membership; there is no one-click copy/publish into a shared space and no per-page public sharing.

### Boundaries with shared spaces

- The private notes space is visible only to its owner and the `notes_` prefix is reserved; it is not a team space.
- The notes space itself cannot be deleted, archived or transferred (403 `notes_space_immutable`); delete an individual note by deleting its page. Shared-space lifecycle operations do not apply to the private notes space.
- Sharing is space-scoped: `viewer` is read-only, `editor` can write, `space_admin` manages members/quotas and `space_owner` owns the space. There is no independent per-page grant.
- Space quotas, audit, groups and multi-user governance are team/enterprise capabilities; personal deployments stay single-user, loopback-only and zero-ops. The quota flag is a deployment-tier setting and does not control whether private notes exist.
