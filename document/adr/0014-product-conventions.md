# ADR 0014：产品统一规范与 0.2.z 版本冻结

- **状态**：Accepted
- **日期**：2026-11-09

## 背景

产品名、CLI、路径、版本、UI、打包与文档口径尚未统一：桌面文档曾把品牌写进 `%APPDATA%` 与笔记目录；默认 SQLite 路径仍相对仓库；发布工作流仍构建 macOS；旧 `/api/*` 与 HTML 页面没有兼容标注。继续按当前路径推进会同时造成安装迁移、发布资产混乱与文档漂移。

## 决策

**冻结 `document/product-spec.md` 为唯一产品规范；品牌只用于显示与仓库名，运行路径/安装目录/日志/临时/keyring 标识一律用 `lkb`。**

- 产品展示名 `Linxira Knowledge Base` / `Linxira 知识库`；仓库 `Linxira-OS/linxira-knowledge-base`；二进制/CLI/包/配置前缀 `lkb`；env `LKB_`；DB `lkb`/`lkb_test`。
- Windows 路径：配置 `%APPDATA%\lkb\lkb.toml`；数据 `%LOCALAPPDATA%\lkb\data`；日志 `%LOCALAPPDATA%\lkb\logs`；portable 用 `.\lkb-data`。Linux/macOS 按 XDG/Application Support 的 `lkb` 路径。
- 禁止在文件系统路径、安装目录、数据/日志/临时目录、OS 服务/keyring 标识中使用 `Linxira`/`linxira`；发行资产品牌名是唯一例外。
- Personal（solo，SQLite 默认，可选 PG）与 Enterprise（team/enterprise，PG）同一二进制；不得发 SQLite/PG 两个包。
- `web/` SPA 是唯一产品 UI；服务端 HTML 仅兼容；桌面加载同一 SPA。
- 打包：Windows NSIS/MSI + portable；Linux deb/AppImage/tar.gz；企业 Docker+compose；**不发 macOS**；资产命名与 tag 按规范。
- 配置只存非秘密；密钥 env/keyring；桌面 PG 密码 keyring 优先、fallback 0600；日志不输出密钥。
- 错误码稳定 + zh/en；联系入口 GitHub Issues/Security。
- 文档分层：`document/` 内部、`docs/` 产品、`desktop/README`、`deploy/`；冲突以本规范 + 本 ADR 为准。
- 兼容：`/api/v1` 只增不改；旧 `/api/search`、`/api/ask`、HTML `/w/...` 只兼容不扩展；破坏性变更需 ADR + 迁移。

**版本冻结：当前固定 `0.2.z`，只递增 z。** `0.2.0` 是首个公开版，下一版 `0.2.1`；tag `v0.2.z`；Cargo.toml/web/package.json/package-lock 一致；每个 patch 一段 release notes；`0.3.0`/`1.0` 需新 ADR + 迁移。

## 备选方案与为何不选

| 方案 | 否决理由 |
| --- | --- |
| 保留品牌路径 | 安装/迁移/日志/keyring 与仓库名耦合，改名成本高；规范明确禁止 |
| 发 SQLite/PG 两个包 | 双包/双测试/双文档；同一二进制已可切换 |
| 继续发 macOS | 无签名/公证/测试资源；本期明确不发 |
| 0.2 阶段升 minor/major | 版本与迁移未就绪；固定 patch 便于兼容与发布节奏 |

## 后果

**正面**：路径/版本/发布/文档有唯一口径；安装迁移可控；UI/二进制唯一。
**负面**：需修正桌面文档路径、默认数据目录、发布工作流与旧 API 标注；这些是已知实现缺口，需单独任务。

## 关联文档

- [document/product-spec.md](../product-spec.md) — 统一规范 v1
- [document/desktop-app-and-notes.md](../desktop-app-and-notes.md)
- [document/release-process.md](../release-process.md)
- [README.md](../../README.md)
- [AGENTS.md](../../AGENTS.md)
