# 桌面应用与笔记模式（设计）

> 状态：设计基线
> 关联决策：[ADR 0013](adr/0013-desktop-app-notes.md)
> 关联文档：[architecture.md](architecture.md)、[spaces-and-permissions.md](spaces-and-permissions.md)、[gap-assessment.md](gap-assessment.md)
> 范围：个人版桌面壳、首启向导、solo+PostgreSQL、静态 Web 托管、Markdown 笔记模式、发布包与安全。本文只定义契约，不写实现。

## 1. 目标与形态

| 形态 | 说明 |
| --- | --- |
| 个人 APP | Windows 安装包或 portable zip；双击 exe 进入默认个人版；单机、单用户、零外部服务 |
| 个人 + 外部 PG | 个人版可选 SQLite（默认）或 PostgreSQL（localhost/外部服务器）；仍是单用户、loopback、allow-all |
| 团队/企业 | 独立部署形态；多用户、空间/组/角色、审计、配额；每个人在团队内仍有自己的 notes space |
| 笔记模式 | 个人版内置 Obsidian 风格 Markdown 笔记；企业内每人有私有 personal 空间，可发布到团队空间 |

## 2. 桌面壳选型

| 项 | 决策 |
| --- | --- |
| 首选 | Tauri 2（Windows 优先；WebView2；安装包小；Rust 后端与现有内核同栈） |
| 兜底 | Electron 仅在 Tauri 无法满足 Windows 兼容/签名/自动更新时评估，不作为默认 |
| 平台 | Windows 10/11 x64 首发；Linux 个人版 tar.gz；macOS dmg/tar.gz 可选后续 |
| 启动 | 双击 exe → Tauri 启动 sidecar `lkb serve` → 打开窗口加载本地 URL |
| 端口 | 默认 loopback 随机端口；可配置固定端口；端口冲突时回退随机并记录 |
| 单实例 | 第二次启动只激活已有窗口，不启动第二个 sidecar |
| 托盘/关闭 | 关闭窗口默认最小化到托盘（可配置退出）；托盘可显示状态/打开/退出 |
| 崩溃恢复 | sidecar 退出时壳重启一次；连续失败进入诊断页，提供日志路径/一键复制 |
| 日志 | 壳日志与 sidecar 日志分目录；不记录 token/密码；诊断包脱敏 |
| 安装包 | NSIS 或 MSI；portable zip 同目录写配置/数据；未签名 SmartScreen 需提示 |
| 更新 | Tauri updater；签名校验；稳定/预览通道；失败回滚保留旧版本 |

## 3. 首启向导

1. 选择存储：SQLite（默认，零配置）或 PostgreSQL（host/port/db/user/password/SSL）。
2. PostgreSQL 填连接信息 → “测试连接”（超时/SSL/TLS 错误可读）。
3. 写配置：`%APPDATA%\Linxira\lkb.toml`；portable 模式写 portable 同目录 `lkb.toml`。
4. 密码：Windows Credential Manager / DPAPI；配置文件只存引用/键名，不落明文。
5. 首次启动跑迁移；失败展示可操作的诊断（权限/端口/DB/SSL）。
6. 切换数据库：明确“迁移到新库”或“新建空库”；迁移需备份/导出；不静默覆盖。

配置优先级：CLI flag > 环境变量 > portable 配置 > `%APPDATA%` 配置 > 默认。

## 4. solo + PostgreSQL

当前校验是 solo 必须 SQLite、team 必须 PG。目标：

| profile | 允许后端 | 约束 |
| --- | --- | --- |
| solo | SQLite（默认）或 PostgreSQL | 单用户、`allow_all`、仅 loopback；PG 连接可 localhost/外部服务器；无 OIDC/多租户 |
| team | PostgreSQL | 多用户、空间/组/角色；不允许 SQLite |
| enterprise | PostgreSQL | 多租户/SSO/审计；不允许 SQLite |

配置/校验变更：

- `storage.backend` 在 solo 下允许 `sqlite|postgres`；PG 使用 `LKB_DATABASE_URL` 或 host/port/db/user/password/SSL。
- solo+PG 仍保持 `tenant_id=t_local`、单空间/allow-all、单用户；不得因 PG 变成 team 语义。
- 启动校验：team/enterprise + SQLite 拒绝；solo+PG 若绑定非 loopback 且未启用认证，拒绝（沿用 loopback 保护）。
- 测试：solo+SQLite 与 solo+PG 跑同一组用例；team+SQLite 负例；PG 迁移/备份/恢复。
- 文档/示例明确“后端选择”与“profile 语义”解耦，不把 PG 当成团队版开关。

## 5. Web 托管

- Rust 二进制直接托管 `web/dist`：`LKB_WEB_DIR` 指向静态目录；未知路径 SPA fallback 到 `index.html`；API 同源，无 CORS。
- Tauri、浏览器、portable 都只启动一个 sidecar 进程；桌面壳只加载本地 URL。
- 外部 nginx 模式保留：适用于企业多实例/静态资源 CDN；API 同源或明确 CORS；两种模式不改变内核。
- 静态资源缓存/压缩由二进制或前置代理处理；CSP 由壳/代理设置。

## 6. 笔记模式

### 6.1 入口与空间

- 侧栏“笔记”与“知识库/Wiki”并列；个人版默认进入笔记首页。
- 个人空间 `personal`：每用户私有，默认 ACL 仅 owner；团队内每人也有一份 personal 空间。
- 笔记可“发布/共享”到团队空间：复制为 Wiki Page/Revision，保留来源链接；权限按目标空间角色。

### 6.2 存储模型（决策）

| 对象 | 存储 | 说明 |
| --- | --- | --- |
| 笔记 | Markdown 文件 vault | 事实源；默认 `%USERPROFILE%\\Documents\\Linxira Notes` 或配置路径 |
| Wiki 页面 | DB Page/Revision | 现有内置 Wiki；保留 revision/乐观锁 |
| 索引 | Document/Chunk/FTS | 笔记编译为文件型 Document；复用 doc_id、chunk、FTS |
| UI | 统一“页面” | 笔记与 Wiki 在同一侧栏/搜索中展示，但不强行合并存储 |

不把笔记强行塞进 Page/Revision：文件是事实源，编辑器/外部工具可直接改；Wiki 仍走 DB revision。UI 层统一，存储层分开。

### 6.3 文件与编辑

- 文件树、frontmatter（`title` / `tags` / `aliases`）、`[[wikilink]]`；文件名/slug 规则稳定。
- 文件监听增量索引；新建/改名/删除同步 Document；外部编辑不丢。
- 原子写：临时文件 + rename；冲突检测 mtime/hash；冲突时保留两份并提示，不静默覆盖。
- 回收站/版本快照：本地 revisions 或可选 Git；默认本地快照，Git 可选。
- 反向链接、标签、最近笔记由索引生成；可选图谱是视图，不是第二事实源。

### 6.4 UI

- Obsidian 风格：左侧文件树、搜索/命令面板、编辑/预览、标签、反向链接、最近笔记、可选图谱。
- 深色/浅色、zh/en i18n；不引入插件市场；不做富文本协同/CRDT。
- 笔记状态（保存/冲突/索引中）可见；离线可用。

## 7. 发布包矩阵

| 包 | 内容 | 说明 |
| --- | --- | --- |
| Windows installer | Tauri NSIS/MSI + sidecar + web dist | 默认个人版；未签名 SmartScreen 提示 |
| Windows portable zip | exe + sidecar + web dist + 配置模板 | 解压即用；数据/配置同目录 |
| Linux 个人版 | tar.gz（Tauri/二进制 + web dist） | 可选后续 |
| macOS | dmg/tar.gz | 可选后续；签名/公证 |
| `lkb-web` dist | 静态 web 资源 | 浏览器/外部 nginx 使用 |
| 企业 Docker + compose | API/worker + PG + 对象存储 | 不含桌面壳 |
| 不发 | debug 包、DB 专用包、node_modules、target/、测试数据 | 避免泄漏/膨胀 |

同一二进制同时支持 SQLite/PG；桌面包内含 sidecar 与 web 资源，不要求用户装 Node/Python。

## 8. 安全

- sidecar 只绑 loopback；随机端口 + 本地令牌；窗口加载本地 URL；CSP 限制外部脚本。
- 外部链接用系统浏览器打开；不在 WebView 内导航任意站点。
- 更新包签名校验；失败不替换；保留旧版本回滚。
- portable 模式密钥保护：DPAPI/凭据管理器不可用时，至少 0600 配置文件 + 明确风险提示；不把密码写明文。
- 错误报告默认只含版本/日志尾部/request_id；用户确认后再上传；链接 GitHub Issues/Security。

## 9. 阶段与估算

估算口径：1 人周 = 1 名熟悉 Rust/Tauri/前端 的工程师 5 个工作日；**估算，非承诺**；分项与合计独立取整。

| 阶段 | 范围 | 人周 |
| --- | --- | --- |
| P0 | Tauri 壳 + sidecar 启停/单实例/托盘 + 首启 SQLite/PG + 测试连接 + 凭据存储 + 静态 web 托管 | 6–10 |
| P1 | 笔记入口/personal 空间/vault/文件树/编辑器/反向链接/索引 | 6–10 |
| P2 | 安装包/portable/自动更新/签名/团队发布共享/诊断 | 4–8 |
| **合计** |  | **16–28** |

## 10. 风险与待验证

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| WebView2 缺失/版本差异 | 窗口打不开 | 安装包检测/引导安装；Tauri 兜底 |
| 未签名 SmartScreen | 安装信任低 | 提示/文档；后续代码签名 |
| sidecar 端口/单实例 | 多开/冲突 | 随机端口 + 单实例锁；端口冲突回退 |
| solo+PG 语义混淆 | 个人版被误当团队版 | 保持 loopback/allow-all/单用户；测试负例 |
| 文件监听/冲突 | 丢笔记 | 原子写/哈希冲突/快照；不静默覆盖 |
| 笔记与 Wiki 双模型 | 用户困惑 | UI 统一“页面”，存储边界文档化 |
| 自动更新失败 | 无法启动 | 签名/回滚/保留旧版本 |

## 11. 与现有文档的关系

- [spaces-and-permissions.md](spaces-and-permissions.md)：personal 空间、ACL、发布到团队空间。
- [architecture.md](architecture.md)：Document/Chunk、静态 Web、solo 语义。
- [tier-matrix.md](tier-matrix.md)：桌面/笔记/solo+PG 差异。
- [roadmap.md](roadmap.md)：P0/P1/P2 交付。
- [gap-assessment.md](gap-assessment.md)：桌面/笔记能力面与工期。
- [adr/0013-desktop-app-notes.md](adr/0013-desktop-app-notes.md)：方向决策。
