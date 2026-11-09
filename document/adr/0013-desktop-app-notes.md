# ADR 0013：桌面 APP 与 Markdown 笔记模式

- **状态**：Accepted
- **日期**：2026-11-09

## 背景

个人用户需要“下载即用”的 APP，而不是先部署 Web/服务；同时希望个人版能当笔记软件用。团队/企业是另一种多用户形态，但每个人在团队内也需要私有笔记空间。现有内核只有 `lkb serve` 与 CLI，没有桌面壳、首启向导、solo+PostgreSQL、静态 Web 托管或笔记 vault。

## 决策

**个人版 = Tauri 2 桌面壳 + sidecar `lkb serve` + 内置 Web dist；笔记 = Markdown vault 作为事实源，编译为文件型 Document/Chunk；Wiki 仍用 DB Page/Revision；UI 统一为“页面”，存储不强行合并。**

- 桌面壳首选 Tauri 2（Windows WebView2，NSIS/MSI + portable zip）；Electron 仅兜底。单实例、托盘、关闭行为、崩溃重启、日志/诊断、签名更新。
- 首启向导选择 SQLite（默认）或 PostgreSQL；测试连接；配置写 `%APPDATA%\lkb\lkb.toml` 或 portable 同目录；密码存 Credential Manager/DPAPI，不落明文；切换 DB 需迁移/新库。
- `solo` 允许 SQLite 或 PostgreSQL（仍单用户、loopback、allow-all）；team/enterprise 仍 PG；不改变企业语义。
- Rust 二进制托管 `web/dist`（`LKB_WEB_DIR`，SPA fallback，API 同源）；Tauri/浏览器/portable 都只启动一个 sidecar。
- 笔记：`personal` 空间每用户私有；vault 默认 `%USERPROFILE%\Documents\lkb-notes`；文件树/frontmatter/`[[wikilink]]`；文件监听增量索引；原子写/冲突检测/回收站或本地快照；Obsidian 风格 UI；不做插件市场。
- 团队内每人有 personal 空间；笔记可发布/共享到团队空间（复制为 Wiki 页面，保留来源）；权限按空间角色。
- 发布：Windows installer + portable、Linux tar.gz、macOS 暂不发布、`lkb-web` dist、企业 Docker；不发 debug/DB 专用/node_modules/target。

## 备选方案与为何不选

| 方案 | 否决理由 |
| --- | --- |
| Electron 默认 | 包体/内存大；Tauri 与 Rust 内核更匹配 |
| 只发浏览器版 | 个人用户要“双击 exe”；离线/本地文件体验不足 |
| 笔记直接存 DB Page/Revision | 外部编辑器/文件事实源不通；与 Wiki revision 语义冲突 |
| 笔记单独一套索引 | 双份检索/权限/引用；应复用 Document/Chunk/FTS |
| solo 锁死 SQLite | 个人也可能有外部 PG；后端选择应与 profile 语义解耦 |

## 后果

**正面**：个人零运维、可笔记、可离线；同一二进制支持 SQLite/PG；桌面/浏览器/团队共享内核。
**负面**：桌面壳/签名/更新增加发布面；笔记与 Wiki 双存储需 UI 统一；solo+PG 需测试负例防止语义漂移。

## 关联文档

- [document/desktop-app-and-notes.md](../desktop-app-and-notes.md)
- [document/spaces-and-permissions.md](../spaces-and-permissions.md)
- [document/architecture.md](../architecture.md)
- [document/tier-matrix.md](../tier-matrix.md)
- [document/roadmap.md](../roadmap.md)
