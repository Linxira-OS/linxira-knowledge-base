# 部署目录（deploy/）

本目录是发布与部署资产的事实源：企业版 Docker 镜像、compose 栈、nginx 配置与两个档位的示例配置。

| 路径 | 用途 |
| --- | --- |
| `Dockerfile` | 多阶段构建：Rust 二进制 + React SPA + `debian:bookworm-slim` 运行时（非 root、`/data` 卷、`EXPOSE 7331`、HEALTHCHECK） |
| `Dockerfile.dockerignore` | 只影响该 Dockerfile 的构建上下文（排除 `target/`、`node_modules` 等） |
| `compose.yaml` | 企业栈：`postgres:16-alpine` + `lkb` + `nginx`（SPA + `/api` 反代），含 healthcheck 与 volumes |
| `nginx.conf` | 挂载到 `nginx` 容器的 `/etc/nginx/conf.d/default.conf`：SPA 静态托管 + `/api` 同源反代（SSE 不缓冲） |
| `lkb.enterprise.toml.example` | 企业部署示例配置（PostgreSQL、enterprise profile） |
| `.env.example` | compose 的变量示例（复制为 `.env`，已被 gitignore） |
| `solo/` | 个人版示例配置与归档内说明（`solo/lkb.toml.example`、`solo/README.md`） |
| `desktop/` | 桌面版便携包组装脚本：`desktop/portable-zip.sh`（Windows）、`desktop/portable-tar.sh`（Linux） |

发布产物、版本规则、校验与回滚见 [document/release-process.md](../document/release-process.md)。

## 1. 企业版快速开始（compose）

```bash
cp deploy/lkb.enterprise.toml.example deploy/lkb.enterprise.toml   # 按需修改
cp deploy/.env.example deploy/.env                                 # 设置 LKB_DB_PASSWORD
docker compose -f deploy/compose.yaml up -d
# 打开 http://localhost:8080/  （SPA；/api 由 nginx 反代到 lkb）
```

组成与端口：

- `db`：`postgres:16-alpine`，数据在卷 `pg_data`，`pg_isready` 健康检查。
- `lkb`：镜像 `ghcr.io/linxira-os/linxira-knowledge-base:${LKB_IMAGE_TAG}`，容器内 7331，**不发布到宿主机**；配置由 `configs` 挂载到 `/etc/lkb/lkb.toml`；数据卷 `lkb_data`（`/data`）。
- `nginx`：`nginx:stable-alpine`，宿主 `${LKB_HTTP_PORT:-8080}` → 容器 80；`../web/dist` 挂载为站点根（仓库或企业 bundle 内自带 `web/dist`）。

需要 TLS 时在 nginx 之前再加一层（负载均衡/外层反代），并把 `[auth] cookie_secure` 改为 `true`。

## 2. 镜像

```
ghcr.io/linxira-os/linxira-knowledge-base:<version>      # 例：0.2.1
ghcr.io/linxira-os/linxira-knowledge-base:<major.minor>  # 例：0.2
ghcr.io/linxira-os/linxira-knowledge-base:latest         # 仅正式 release 推送
```

- 平台：`linux/amd64`（arm64 见发布流程文档的“暂不发布”清单）。
- 镜像内同时包含 `lkb` 与构建好的 SPA（`/srv/web`）；Rust 服务本身不托管 `dist`，compose 里由 nginx 托管。
- 正式发布构建带 buildx provenance 与 SBOM attestation：

```bash
docker buildx imagetools inspect ghcr.io/linxira-os/linxira-knowledge-base:0.2.1
docker scout attest get --predicate-type=https://spdx.dev/Document ghcr.io/linxira-os/linxira-knowledge-base:0.2.1
```

- 只用镜像跑单机 SQLite（不经 compose 的评估用法）：

```bash
docker run -d --name lkb -p 127.0.0.1:7331:7331 \
  -v lkb-data:/data \
  ghcr.io/linxira-os/linxira-knowledge-base:0.2.1
# 绑定非 loopback 的地址需要 [auth] 从 none 改为 session（solo 校验会拒绝公网 + none）
```

## 3. 配置

- `deploy/lkb.enterprise.toml.example` 经 `configs` 注入为 `/etc/lkb/lkb.toml`；复制为 `deploy/lkb.enterprise.toml` 后修改（该文件名已加入 `.gitignore`）。
- compose 变量：`LKB_DB_PASSWORD`（必填）、`LKB_HTTP_PORT`、`LKB_IMAGE_TAG`、`LKB_LOG_LEVEL`；见 `.env.example`。
- `LKB_DATABASE_URL` 由 compose 按 `LKB_DB_PASSWORD` 拼接，指向服务名 `db`；密码请使用 URL 安全字符或做百分号转义。
- 同一镜像也能以 solo/SQLite 运行：`profile = "solo"`、`[storage] backend = "sqlite"`、`LKB_SQLITE_PATH=/data/lkb.sqlite`——**没有**第二个镜像或第二套构建。

## 4. 运维

| 事项 | 说明 |
| --- | --- |
| 迁移 | 启动时自动执行内嵌迁移，前向、无 down 脚本；升级前先备份 |
| 备份 | `docker compose -f deploy/compose.yaml exec db pg_dump -U lkb lkb > lkb-$(date +%F).sql`；SQLite 直接备份 `/data/lkb.sqlite` |
| 升级 | 拉取新 tag → 改 `LKB_IMAGE_TAG` → `docker compose ... up -d`；观察日志与 `/health` |
| 回滚 | 换回旧 `LKB_IMAGE_TAG` + 恢复备份（迁移无降级脚本）；镜像 tag 与 digest 见 GitHub Release |
| 日志 | `LKB_LOG_FORMAT=json` 写 stderr：`docker compose -f deploy/compose.yaml logs -f lkb` |
| 健康 | `curl -fsS http://localhost:8080/health`（经 nginx 转 `/health`）；指标在容器内 `/metrics` |
| 单实例 | 限流与配额预留是进程内计数，迁移也没有 advisory lock：同一数据库先只跑一个实例 |
| SSO | 仅 `trusted_header` 模式；代理必须**覆盖**身份头并列入 `sso.trusted_proxies`，详见 `docs/zh/08-deployment.md` 第 7 节 |

## 5. 企业 bundle

发布资产 `linxira-knowledge-base-enterprise-v<version>.tar.gz` 解压后与仓库同构：

```
linxira-knowledge-base-enterprise-v0.2.1/
  deploy/{compose.yaml,nginx.conf,lkb.enterprise.toml.example,.env.example,README.md}
  web/dist/          # SPA（compose 里为 ../web/dist）
  LICENSE
  CHANGELOG.md
```

按第 1 节的命令即可启动；无需源码、无需 Rust 工具链。

## 6. 桌面安装包（Windows / Linux）

桌面版（Tauri 2 外壳 + 同一个 `lkb` sidecar）由 `release.yml` 的 `desktop-windows` / `desktop-linux` 作业产出，**macOS 不发布**：

| 资产 | 说明 |
| --- | --- |
| `lkb-solo-v<version>-windows-x64-setup.exe` | NSIS 安装包（用户级安装），安装后从开始菜单启动 |
| `lkb-solo-v<version>-windows-x64-portable.zip` | 便携包：解压 `lkb/` 后直接运行 `lkb-desktop.exe` |
| `lkb-solo-v<version>-linux-amd64.deb` | Debian/Ubuntu 包：`sudo dpkg -i` 或 `apt install ./<file>.deb` |
| `lkb-solo-v<version>-linux-amd64.AppImage` | 单文件 AppImage：`chmod +x` 后直接运行 |
| `lkb-solo-v<version>-linux-amd64.tar.gz` | 便携包（与便携 zip 同布局） |

- 便携包内容：`lkb/lkb-desktop[.exe]`、`lkb/lkb[.exe]`（sidecar）、`lkb/resources/web/**`（内嵌 SPA）、`LICENSE`、`README.md`；Windows 上可能另有 `WebView2Loader.dll`。
- 桌面壳按统一 `lkb` 路径存放配置与数据（Windows `%APPDATA%\lkb` / `%LOCALAPPDATA%\lkb`，Linux XDG），详见 [`desktop/README.md`](../desktop/README.md)。
- 本地构建等价物：`scripts/build-sidecar.{sh,ps1}` 生成 sidecar 与 `resources/web`；`deploy/desktop/portable-zip.sh` / `deploy/desktop/portable-tar.sh` 组装便携包。所有安装包未签名，Windows 可能出现 SmartScreen 提示。

## 7. 校验与反馈

```bash
sha256sum -c SHA256SUMS
gh attestation verify linxira-knowledge-base-enterprise-v0.2.1.tar.gz --repo Linxira-OS/linxira-knowledge-base
```

- 仓库：https://github.com/Linxira-OS/linxira-knowledge-base
- 问题反馈：https://github.com/Linxira-OS/linxira-knowledge-base/issues
- 安全漏洞（私密上报）：https://github.com/Linxira-OS/linxira-knowledge-base/security/advisories/new
