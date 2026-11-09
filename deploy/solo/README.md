# 个人用户版（solo）· 安装与使用

这是 `lkb`（Linxira Knowledge Base）单一二进制的个人版归档：**一个可执行文件 + 可选 `lkb.toml`**，默认 SQLite，单进程、无外部服务。内置的 HTML 界面直接由服务进程渲染，浏览器打开即可用。

归档内容：

```
lkb                # Linux/macOS 可执行文件（Windows 为 lkb.exe）
LICENSE
README.md
CHANGELOG.md
deploy/solo/lkb.toml.example
deploy/solo/README.md          # 本文件
```

## 1. 快速开始

```bash
# Linux x86_64（ubuntu-22.04 构建，glibc ≥ 2.35；Debian 12+/Ubuntu 22.04+ 可用）
tar -xzf lkb-solo-v0.2.0-x86_64-unknown-linux-gnu.tar.gz
cd lkb-solo-v0.2.0-x86_64-unknown-linux-gnu
./lkb serve                     # 打开 http://127.0.0.1:7331/
```

Windows 解压 `.zip` 后在 PowerShell 中运行 `.\lkb.exe serve`；macOS 解压 `.tar.gz` 后 `./lkb serve`（首次运行如提示出自 Gatekeeper，可在「系统设置 → 隐私与安全性」中允许）。macOS arm64 归档在 Apple Silicon 上原生运行；x86_64 归档同样可运行。

首次启动会在 `data/lkb.sqlite` 建库、执行内嵌迁移，并把内置的 24 篇中英文产品文档 seed 到 `space=product`（幂等，不覆盖你的修改）。默认仅绑定回环地址、`auth.mode = "none"`——这是 solo 的安全边界：只能本机访问。

常用命令：

```bash
./lkb config check                  # 打印生效配置（密钥打码）
./lkb import ~/notes --space s_default
./lkb search "复盘" --lang zh
./lkb ask "复盘结论是什么"
./lkb seed                          # 手动（重新）写入产品文档种子
```

## 2. 配置

不带配置文件时使用默认值即可。需要自定义时：

```bash
cp deploy/solo/lkb.toml.example lkb.toml
./lkb config check                  # 校验语法与取值
```

常用环境变量覆盖：`LKB_SQLITE_PATH`（默认 `data/lkb.sqlite`）、`LKB_BIND`、`LKB_AUTH_MODE`、`LKB_LOG_FORMAT=json`、`LKB_LOG_LEVEL`。完整列表见 `docs/zh/07-configuration.md`（仓库内）。

## 3. 可选：独立 Web SPA

发布资产里的 `lkb-web-v<版本>.tar.gz` 是 React SPA（`web/dist`）。Rust 服务**不**托管它，需要静态服务器，并把 `/api` 反向代理到 `http://127.0.0.1:7331`（必须同源，否则会话 cookie 与 CSRF 失效）。也可以直接使用内置 HTML 界面，无需这个包。

## 4. 同一二进制也支持 PostgreSQL

solo 与 team/enterprise 是同一份代码、同一个二进制，差异只在配置：

```toml
profile = "team"                    # 或 "enterprise"
[storage]
backend = "postgres"
url_env = "LKB_DATABASE_URL"
```

```bash
export LKB_DATABASE_URL='postgres://user:pass@localhost/lkb'
./lkb serve                         # 打开连接时自动执行内嵌迁移
```

因此**没有**“SQLite 版 / PostgreSQL 版”两个下载包；迁移是前向的，升级前先备份（`data/lkb.sqlite` 直接复制，或 `pg_dump`）。

## 5. 校验与反馈

```bash
sha256sum -c SHA256SUMS                     # 校验所有下载文件
gh attestation verify <文件> --repo Linxira-OS/linxira-knowledge-base   # 构建来源证明
```

- 仓库：https://github.com/Linxira-OS/linxira-knowledge-base
- 使用文档：https://github.com/Linxira-OS/linxira-knowledge-base/tree/main/docs
- 问题反馈：https://github.com/Linxira-OS/linxira-knowledge-base/issues
- 安全漏洞（私密上报）：https://github.com/Linxira-OS/linxira-knowledge-base/security/advisories/new
- 发布流程与升级/回滚：https://github.com/Linxira-OS/linxira-knowledge-base/blob/main/document/release-process.md

当前版本不包含桌面安装包（Tauri/NSIS/MSI 等），该能力见发布流程文档中的 P1 待办。
