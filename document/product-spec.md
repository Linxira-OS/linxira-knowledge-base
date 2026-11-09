# Linxira 知识库统一规范 v1

> 状态：冻结（Accepted）；变更需 ADR + 迁移说明。
> 决策：[ADR 0014](adr/0014-product-conventions.md)
> 适用：仓库、CLI、桌面、Web、Docker、发布资产、文档与配置。
> 原则：**品牌只用于显示与仓库名；运行路径、安装目录、日志、临时目录、服务/keyring 标识一律用 `lkb`。**

## 1. 命名

| 对象 | 必须 | 禁止 / 说明 |
| --- | --- | --- |
| 产品展示名 | `Linxira Knowledge Base`；中文 `Linxira 知识库` | 只用于 UI/关于页/文档标题 |
| 仓库 | `Linxira-OS/linxira-knowledge-base` | 仅仓库名；不得复制到文件系统路径 |
| 二进制 / CLI | `lkb`（Windows `lkb.exe`） | 不得用 `linxira` 作为可执行名 |
| 包 / crate | `lkb` | 不得新增 `linxira-*` crate 作为运行时包 |
| 配置前缀 | `lkb.toml` | 不得用 `linxira.toml` |
| 环境变量前缀 | `LKB_`（如 `LKB_CONFIG_FILE`、`LKB_DATABASE_URL`） | 不得用 `LINXIRA_` |
| 数据库名 | `lkb` / 测试 `lkb_test`；SQLite 文件 `lkb.sqlite` | 不得用 `linxira` 作为 DB 名/文件名 |
| API | `/api/v1` | 旧 `/api/search`、`/api/ask`、HTML `/w/...` 仅兼容，不再扩展 |
| 品牌字体/图标 | 显示用；路径不含品牌 | 不得把品牌写进路径/服务标识 |

**关键禁令**：文件系统路径、安装目录、数据目录、日志目录、临时目录、OS 服务名、keyring service 标识、锁文件、Unix socket、Windows named pipe，**禁止**出现 `Linxira` / `linxira`。发行资产文件名按第 5 节是唯一例外（品牌名作为资产名，不是安装/运行路径）。

## 2. 文件系统路径（跨平台）

| 平台 | 配置 | 数据 | 日志 |
| --- | --- | --- | --- |
| Windows | `%APPDATA%\lkb\lkb.toml` | `%LOCALAPPDATA%\lkb\data` | `%LOCALAPPDATA%\lkb\logs` |
| Windows portable | `.\lkb-data\lkb.toml` | `.\lkb-data\data` | `.\lkb-data\logs` |
| Linux | `$XDG_CONFIG_HOME/lkb/lkb.toml`（fallback `~/.config/lkb/lkb.toml`） | `$XDG_DATA_HOME/lkb`（fallback `~/.local/share/lkb`） | `$XDG_STATE_HOME/lkb/logs`（fallback `~/.local/state/lkb/logs`） |
| macOS（暂不打包） | `~/Library/Application Support/lkb/lkb.toml` | `~/Library/Application Support/lkb` | `~/Library/Logs/lkb` |

规则：

- 端口：loopback 默认 `7331`；桌面 sidecar 用随机空闲端口，冲突回退随机；不得把端口写进路径。
- 配置/DB/密钥/日志不得放在仓库或安装目录；唯一例外是 portable：exe 同目录 `portable.flag`，配置/数据/日志在 `.\lkb-data`。
- 临时文件用系统临时目录下的 `lkb-*`；不要用 `Linxira-*`。
- keyring service 用 `lkb`；OS 服务名用 `lkb`（企业部署按产品线可用 `lkb-enterprise`，仍不含品牌）。

**示例**：`C:\Users\alice\AppData\Roaming\lkb\lkb.toml`、`C:\Users\alice\AppData\Local\lkb\data\lkb.sqlite`、`/home/alice/.local/state/lkb/logs/app.jsonl`。禁止把品牌作为 AppData / Documents 的路径段。

## 3. 版本身份

| 版本 | profile | 存储 | 说明 |
| --- | --- | --- | --- |
| Personal | solo | SQLite 默认；可选 PostgreSQL | 单用户、loopback、allow-all；不是“SQLite 版” |
| Enterprise | team / enterprise | PostgreSQL | 多用户/多租户；不是“PostgreSQL 版” |

- **同一二进制**支持 SQLite 与 PostgreSQL；不得发布“SQLite 版/PostgreSQL 版”两个包。
- `profile` 与 `storage.backend` 解耦：solo 允许 `sqlite|postgres`；team/enterprise 只 PG。
- `lkb --version` 输出同一 semver；Cargo、web、release tag 三者一致。
- 档位差异只在配置/适配器，不产生第二套代码或第二套 UI。

### 3.1 版本冻结：0.2.z

**当前版本固定为 `0.2.z`，只递增 `z`（patch），不升 minor/major。**

| 项 | 必须 |
| --- | --- |
| 版本格式 | `0.2.z`；`0.2.0` 是首个公开版，当前 `main` 为 `0.2.2` |
| tag | `v0.2.z`（如 `v0.2.2`） |
| 一致性 | `Cargo.toml`、`Cargo.lock`、`web/package.json`、`web/package-lock.json`、`desktop/src-tauri/Cargo.toml`、`desktop/src-tauri/Cargo.lock`、`desktop/src-tauri/tauri.conf.json`、tag 必须一致 |
| release notes | 每个 patch 一段；不得复用上一段 |
| 升级边界 | `0.3.0` / `1.0` 需要新 ADR + 迁移说明，不得直接打 tag |
| 禁止 | 在 0.2 阶段升 minor/major、并行维护多个 minor、用日期/构建号替代 semver |

## 3.2 版本与发布检查

- 发布前检查：`Cargo.toml` = `Cargo.lock` = `web/package.json` = `web/package-lock.json` = `desktop/src-tauri/Cargo.toml` = `desktop/src-tauri/Cargo.lock` = `desktop/src-tauri/tauri.conf.json` = tag（去掉 `v`）。
- 发布资产命名使用 `v<version>`；不得出现 `v0.2`、`v0.2.0.1`、`latest` 作为唯一版本标识。
- `lkb --version` 与 release 页面一致；桌面/CLI/Docker 使用同一二进制。

## 4. UI

- `web/` SPA 是**唯一产品 UI**；服务端 HTML 仅兼容/诊断（旧 `/w/...` 页面）。
- 桌面 Tauri 加载本地 `lkb serve` 的 SPA；浏览器/portable 加载同一 `web/dist`；不维护第三套 UI。
- SPA 由 Rust 二进制托管：`LKB_WEB_DIR`、SPA fallback、API 同源；外部 nginx 只作为可选部署，不改变 UI。
- i18n：zh-CN/en；文案 key 外置；不硬编码；API 错误稳定 code + 本地化 message。

## 5. 打包与发行

| 资产 | 目标 | 命名 |
| --- | --- | --- |
| Windows installer | NSIS/MSI | `lkb-solo-v<version>-windows-x64-setup.exe` / `.msi` |
| Windows portable | zip | `lkb-solo-v<version>-windows-x64-portable.zip` |
| Linux | deb / AppImage / tar.gz | `lkb-solo-v<version>-linux-x86_64.deb` / `.AppImage` / `.tar.gz` |
| Web dist | 静态资源 | `lkb-web-v<version>` |
| Enterprise | Docker + compose | `linxira-knowledge-base-enterprise-v<version>`（品牌资产名例外） |

- **不发 macOS**（macOS 仅保留开发路径，不产出 dmg/安装包）。
- 不发 debug 包、DB 专用包、`node_modules`、`target/`、测试数据。
- tag `v0.2.z`；Cargo.toml、web/package.json、web/package-lock.json、tag 一致；发布附 `SHA256SUMS`、SBOM、来源证明。
- 安装解压后的目录名用 `lkb`/`lkb-enterprise`，不得用 `Linxira*`。

## 6. 配置与密钥

| 项 | 必须 | 禁止 |
| --- | --- | --- |
| `lkb.toml` | 只存非秘密：profile、bind、backend、路径、模型 endpoint/model 名 | 明文密码、API key、token |
| 密钥来源 | 环境变量 `LKB_*` 或 OS keyring | 写入仓库/日志/错误/README 示例 |
| 桌面 PG 密码 | Windows Credential Manager/DPAPI 优先；fallback 0600 文件 | 明文配置/注册表明文 |
| 日志/报错 | 打码/哈希；只显示变量是否设置 | 输出 Authorization、cookie、DSN 密码 |
| CLI | token 从 env/stdin/keyring | shell history、URL query |

## 7. 错误与联系

- 错误响应/CLI 输出必须含稳定 `code`（如 `unauthorized`、`forbidden`、`rate_limited`、`space_quota_exceeded`）+ 本地化 `message`。
- 反馈：GitHub Issues `https://github.com/Linxira-OS/linxira-knowledge-base/issues`；安全漏洞走私密 `https://github.com/Linxira-OS/linxira-knowledge-base/security/advisories/new`。
- 错误页/CLI `--help`/README 指向同一入口，不出现第二个支持邮箱/域名。

## 8. 文档分层

| 目录 | 内容 | 读者 |
| --- | --- | --- |
| `document/` | 架构、ADR、规范、路线图、评估 | 维护者/Agent |
| `docs/` | 产品使用文档（zh/en） | 用户 |
| `desktop/README` | 桌面安装/首启/诊断 | 桌面用户 |
| `deploy/` | Docker/compose/nginx/配置模板 | 部署者 |

- 产品名/路径/版本/发布以此规范为准；其他文档冲突时，以本规范 + ADR 0014 为最高优先级。
- 新产品文档必须链接本规范；不得复制路径表造成漂移。

## 9. 兼容与弃用

- `/api/v1` 只增不改；删除字段/改语义需新版本 + ADR + 迁移。
- 旧 `/api/search`、`/api/ask`、HTML `/w/...` 为兼容面：保留但不新增功能；标注兼容期与替代接口。
- 破坏性变更必须提供迁移脚本/导出导入；不得静默改变 `doc_id`、Answer 字段或权限语义。
- 弃用先文档标注 + 日志/响应 warning，再按版本移除。

## 10. 验收清单

- [ ] 全仓路径/配置/日志/临时/keyring grep 不含 `Linxira`/`linxira`（发行资产名按第 5 节例外）。
- [ ] Windows 安装后配置/数据/日志路径符合第 2 节；portable 使用 `.\lkb-data`。
- [ ] `lkb --version` 与 tag/Cargo/web/package-lock 版本一致（0.2.z）；桌面/CLI/Docker 使用同一二进制。
- [ ] release 资产不含 macOS、debug、DB 专用包、`node_modules`、`target/`。
- [ ] `lkb.toml` 无明文密钥；日志/报错无 token/密码/DSN。
- [ ] `/api/v1` 契约版本化；旧接口有兼容标注。
- [ ] README/AGENTS 的规范入口链接有效；文档相对链接无悬空。

## 11. 与现有实现的已知冲突（截至规范冻结）

| 冲突 | 现状 | 处理 |
| --- | --- | --- |
| 桌面文档路径含品牌 | 已修正为 `%APPDATA%\lkb`、`Documents\lkb-notes`；安装器/代码尚未落地 | 以本规范为准，代码任务单独验收 |
| 默认 SQLite 路径 | `src/config.rs` 默认 `data/lkb.sqlite`（相对仓库/当前目录） | 目标改为 OS 数据目录；代码未改，列为实现缺口 |
| 发布含 macOS | Release 工作流仍构建 macOS x86_64/arm64；README 已改为“macOS 暂不发布” | 目标不发 macOS；工作流未改，列为发布缺口 |
| 桌面壳未落地 | 无 Tauri/sidecar 启动器 | 设计见 desktop 文档，代码未实现 |
| Agent CLI 设备码 | README 标注 `--device` 返回 not_implemented | 目标实现设备码；代码未改 |
| 旧 API/HTML | `/api/search`、`/api/ask`、`/w/...` 仍在 | 保留兼容期，不再扩展 |

> 本表只记录冲突，不在本规范中改代码；实现变更需单独任务与验收。

## 12. 许可证与商业许可冻结（v0.2.2 起）

分层授权是产品边界的一部分，随版本冻结；变更需新 ADR 并同步 `LICENSING.md`。

| 层 | 许可证 | 路径 | 规则 |
| --- | --- | --- | --- |
| 核心 | `AGPL-3.0-only` | `src/search.rs`、`src/answer.rs`、`src/governance.rs`、`src/access.rs`、`src/contract.rs`、`src/store/**`、`src/ingest/**`、`src/providers/**`、`src/gateway.rs`、`src/token_quota.rs`、`src/quota.rs`、`src/lib.rs`、`migrations/**` | 不得改为宽松许可；不得把核心文件移入宽松层 |
| 产品面 | `MIT OR Apache-2.0` | `src/cli.rs`、`src/paths.rs`、`src/config.rs`、`src/util.rs`、`src/error.rs`、`src/i18n.rs`、`src/logging.rs`、`src/metrics.rs`、`src/limits.rs`、`src/connectors.rs`、`src/payment.rs`、`src/billing.rs`、`src/summary.rs`、`src/conversation.rs`、`src/seed.rs`、`src/frontmatter.rs`、`src/mcp.rs`、`src/eval.rs`、`src/event.rs`、`src/audit.rs`、`src/openapi.rs`、`src/web.rs`、`src/main.rs`、`web/**`、`desktop/**`、`deploy/**`、`scripts/**`、`docs/**`、`document/**`、`tests/**` | 可独立提取商用；新增 UI/工具默认落此层 |

- **整包分发**：根 `Cargo.toml` 的 `license = "AGPL-3.0-only"`；对外分发的二进制与容器因链接核心，整体按 AGPL-3.0-only 分发，Docker label 固定 `org.opencontainers.image.licenses="AGPL-3.0-only"`。
- **SPDX 头**：源文件首行必须带所属层的 SPDX 标识；文件头与 `LICENSING.md` 冲突时以 AGPL 为准。
- **商业许可**：仅针对 AGPL 核心；闭源嵌入、OEM、白标、闭源 SaaS、规避源码义务需联系作者；唯一入口为 GitHub Issues 加 `commercial-license` 标签，不设销售邮箱。
- **版本历史**：`v0.2.1` 及更早为 MIT，已发布资产不追溯；`v0.2.2` 起分层。
- **贡献**：PR 即按文件层授权，核心贡献额外授予作者商业再许可权（轻量入站 CLA）；DCO 仍强制。
