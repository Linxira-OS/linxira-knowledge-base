# Linxira 桌面版（Windows / Linux）

Tauri 2 桌面壳：首启向导（SQLite / PostgreSQL）→ 启动内置 sidecar `lkb serve`（随机回环端口）
→ 健康检查通过后打开主窗口 `http://127.0.0.1:<port>`，由 sidecar 托管 `web/dist` 并同源提供 API。
关闭最后一个窗口或退出应用会停止 sidecar；应用为单实例（第二次启动只聚焦已有窗口）。

macOS 不在支持范围内。

## 目录

```
desktop/
  src-tauri/
    Cargo.toml           # 独立 crate（自带 [workspace]，不参与仓库根构建）
    build.rs             # tauri_build::build()
    tauri.conf.json      # 基础配置；tauri.windows.conf.json / tauri.linux.conf.json 按平台合并
    capabilities/        # 仅 "setup" 窗口拥有 core:default 权限；主窗口不授予任何 Tauri 权限
    icons/               # 由 `tauri icon brand/icon.svg` 生成
    src/main.rs          # 窗口/命令/单实例/退出清理
    src/setup.rs         # 路径、lkb.toml、PG 凭据（keyring / pg.env）
    src/engine.rs        # sidecar 生命周期 + /health 等待 + 日志尾部
    binaries/            # 构建产物（lkb-<triple>[.exe]，不入库）
    resources/web/       # 构建产物（web/dist 副本，不入库）
  ui/                    # 首启设置页（frontendDist，纯静态 HTML/CSS/JS，withGlobalTauri）
scripts/build-sidecar.sh / .ps1   # 生成上面两个构建产物
deploy/desktop/portable-zip.sh    # 组装 Windows portable zip
```

## 构建

1. 生成 sidecar 与静态资源（会构建 `lkb` 与 `web/dist`）：

   ```bash
   scripts/build-sidecar.sh                      # Linux / Git-Bash / CI
   powershell -File scripts/build-sidecar.ps1    # Windows PowerShell
   ```

   常用参数：`--target <triple>`（跨/指定目标，默认 rustc host）、`--skip-web`、`--skip-rust`。
   输出：

   - `desktop/src-tauri/binaries/lkb-<triple>[.exe]`（Tauri `externalBin` 命名约定）
   - `desktop/src-tauri/resources/web/**`（打包后运行时位于 `<resource_dir>/resources/web`）

2. 打包（在 `desktop/src-tauri` 下）：

   ```bash
   npx @tauri-apps/cli@^2 build            # 或 cargo tauri build
   ```

   - Windows：NSIS + MSI → `desktop/src-tauri/target/release/bundle/{nsis,msi}/`
   - Linux：deb + AppImage → `desktop/src-tauri/target/release/bundle/{deb,appimage}/`
   - Windows portable zip：`deploy/desktop/portable-zip.sh <release-dir> <out-dir> <version>`
     （release-dir 为 `desktop/src-tauri/target[/<triple>]/release`；
     产出 `lkb-solo-v<version>-windows-x64-portable.zip`，解压目录为 `lkb/`）

本机没有 MSVC 链接器时用 GNU 工具链（CI 仍用 MSVC）：

```bash
RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu scripts/build-sidecar.sh --skip-web
RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu cargo build --release --locked   # desktop/src-tauri
```

开发运行：`npx @tauri-apps/cli@^2 dev`（先执行第 1 步生成两个产物；否则 externalBin/resources 缺失会构建失败）。

## 首启流程

1. 读取配置（统一 `lkb` 布局，见下一节）：
   - Windows：`%APPDATA%\lkb\lkb.toml`
   - Linux：`$XDG_CONFIG_HOME/lkb/lkb.toml`（默认 `~/.config/lkb/lkb.toml`）
2. 无配置 → 打开设置窗口：
   - **SQLite（默认）**：数据文件 `<数据目录>/lkb.sqlite`（Windows `%LOCALAPPDATA%\lkb\data\lkb.sqlite`，
     Linux `$XDG_DATA_HOME/lkb/lkb.sqlite`），`profile=solo`、`auth.mode=none`，仅监听回环地址。
   - **PostgreSQL**：`profile=team` + `auth.mode=session`（团队 / 多用户语义，设置页已提示）。
     solo+PostgreSQL 目前被内核校验拒绝（`solo 必须 sqlite`），后续 ADR 再定。
3. 保存后立即以新配置启动 sidecar 验证；失败时设置窗口保留并展示错误与日志尾部，可改配置重试。
4. 启动命令与环境（shell 负责组装）：

   ```
   lkb serve --config <lkb.toml>     # --config 即服务器配置路径（权威参数）
     LKB_CONFIG_FILE=<lkb.toml>      # 与 --config 保持一致（CLI 客户端也读该变量）
     LKB_BIND=127.0.0.1:<free port>
     LKB_PROFILE=solo|team
     LKB_WEB_DIR=<resource_dir>/resources/web
     LKB_DATABASE_URL=postgres://…   # 仅 PostgreSQL
   ```

   端口由 shell 绑定 `127.0.0.1:0` 取得；最多等待 60s 轮询 `/health` 返回 200（首次含迁移）。
5. 关闭窗口 → 应用退出 → 杀掉 sidecar（连同进程组）；sidecar 提前退出会把退出码写入错误信息。

## 路径、凭据与日志

统一 `lkb` 目录规范（路径中不出现品牌名）：

| 内容 | Windows | Linux | macOS（未打包，代码保持正确） |
| --- | --- | --- | --- |
| 配置（`lkb.toml`、`pg.env`） | `%APPDATA%\lkb` | `$XDG_CONFIG_HOME/lkb`（默认 `~/.config/lkb`） | `~/Library/Application Support/lkb` |
| 数据（`lkb.sqlite`、附件/blobs） | `%LOCALAPPDATA%\lkb\data` | `$XDG_DATA_HOME/lkb`（默认 `~/.local/share/lkb`） | `~/Library/Application Support/lkb` |
| 日志（`sidecar.log`） | `%LOCALAPPDATA%\lkb\logs` | `$XDG_STATE_HOME/lkb/logs`（默认 `~/.local/state/lkb/logs`） | `~/Library/Logs/lkb` |

- `lkb.toml` 由向导生成（也可手工编辑，重启生效）；除内核字段外含 shell 私有的 `[desktop]` 表
  （host/port/database/user/sslmode/secret），`lkb serve` 会忽略该表。
- PostgreSQL 密码**不写入 TOML**：
  - 优先写入系统凭据库（keyring：Windows 凭据管理器 / Linux Secret Service，
    service=`lkb-desktop`, user=`postgres`）；
  - 凭据库不可用时写入 `<配置目录>/pg.env`（`LKB_DATABASE_URL=…`，Unix 0600、
    Windows 尽力 `icacls` 收紧），设置页会明确提示。
- 日志：sidecar stdout/stderr → `<日志目录>/sidecar.log`（每次启动截断重写），
  内存保留最后 100 行用于错误提示。日志不打印数据库 URL（内核自身对 URL 做掩码）。

## 打包资源与运行时布局

| 内容 | 源码位置 | 打包位置（运行时） |
| --- | --- | --- |
| 壳 exe | `target/release/lkb-desktop(.exe)` | 安装目录根 |
| sidecar | `binaries/lkb-<triple>[.exe]` | 壳 exe 同目录 `lkb[.exe]` |
| Web SPA | `resources/web/**` | `<resource_dir>/resources/web/**` |
| 设置页 | `ui/**` | 嵌入壳 exe（`frontendDist`） |
| WebView2Loader.dll | 构建产物（仅 `windows-gnu`） | 壳 exe 同目录（MSVC 构建静态链接，无此文件） |

打包名与标识：crate/`productName`/可执行文件为 `lkb-desktop`（NSIS currentUser 安装目录
`%LOCALAPPDATA%\lkb-desktop`），bundle identifier 为 `com.lkb.desktop`；
窗口标题等品牌展示文案保留 “Linxira”，但所有文件系统路径只使用 `lkb`。

`resource_dir()`：Windows 为 exe 所在目录；Linux 为 exe 目录 / `${exe_dir}/../lib/<product>` /
AppImage 的 `usr/lib/...` / 安装版 `/usr/lib/<product>`（由 Tauri 解析）。

## CI 系统依赖

- Windows（`windows-latest`）：MSVC 工具链 + WebView2 Runtime（Win10/11 一般自带；
  NSIS 安装器默认 webview2 bootstrapper）。NSIS/WiX 由 tauri CLI 首次构建时自动下载。
- Linux（`ubuntu-22.04+`）：
  `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev patchelf libayatana-appindicator3-dev
  libssl-dev build-essential curl wget file pkg-config`
  （keyring 的 Secret Service 后端在运行期需要桌面会话的 D-Bus；无会话时自动回退 `pg.env`）。
- 打包还要求 Node 24（web 构建）与 Rust stable。

## 已知限制

- 未实现：托盘、sidecar 崩溃自动重启、自动更新与代码签名、portable（配置随包）模式。
- 主窗口不授予任何 Tauri 权限；外部链接交给系统浏览器打开，WebView 不会离开本地页面。
- Windows 安装包未签名时会出现 SmartScreen 提示（文档已提示）。
