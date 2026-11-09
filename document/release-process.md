# 发布流程（Release Process）

> 状态：生效中。本文是 `lkb`（Linxira Knowledge Base）版本、打包与发布的唯一流程定义。
> 关联：[`.github/workflows/release.yml`](../.github/workflows/release.yml)、[`deploy/README.md`](../deploy/README.md)、[`document/build-policy.md`](build-policy.md)。

## 1. 版本规则

- 版本号遵循 SemVer `MAJOR.MINOR.PATCH`；tag 形如 `v0.2.0`。
- **三处版本号必须完全一致**，否则 `prepare` 任务直接失败：`Cargo.toml`、`web/package.json`、`web/package-lock.json`（根 `version` 与 `packages[""]`）。
- `CHANGELOG.md` 必须存在 `## [<version>]` 段（含日期）；release notes 从该段提取并追加反馈渠道与资产清单，无匹配即报错，不允许空 body 发布。
- 已推送的 tag 不移动、不重写；需要修补就发新版本。当前首个公开版本为 `0.2.0`。

## 2. “版”是什么

**同一内核、同一二进制、三种 profile**（solo / team / enterprise，见 ADR 0001）。发布上的“个人用户版 / 企业部署版”指**打包与部署形态**，不是第二套代码：

| 名称 | 形态 | 边界 |
| --- | --- | --- |
| 个人用户版（solo） | 原生归档（tar.gz / zip），单文件 `lkb`，SQLite + 回环绑定 | 个人、离线、单机 |
| 企业部署版（enterprise） | GHCR 容器镜像 + compose/nginx 配置 bundle | PostgreSQL、反代、多用户 |

- **同一二进制同时支持 SQLite 与 PostgreSQL**，靠 `profile` 与 `[storage]` 配置切换；**不发布**“SQLite 版 / PostgreSQL 版”两个包。
- 桌面应用（Tauri 安装包 / portable zip）为 **P1，尚未实现**：`desktop/` 落地后新增 `desktop` job（Tauri build + NSIS/MSI + portable zip）并在此文档登记资产；在此之前**不发布**任何桌面安装包。

## 3. 资产矩阵

| 资产 | 形态 | 说明 |
| --- | --- | --- |
| `lkb-solo-v<version>-x86_64-unknown-linux-gnu.tar.gz` | solo | ubuntu-22.04 构建（glibc 2.35），兼容较旧发行版 |
| `lkb-solo-v<version>-x86_64-pc-windows-msvc.zip` | solo | windows-latest，MSVC |
| `lkb-solo-v<version>-x86_64-apple-darwin.tar.gz` | solo | `macos-15-intel`（Intel 官方标签；`macos-13`/`macos-14` 已退役） |
| `lkb-solo-v<version>-aarch64-apple-darwin.tar.gz` | solo | `macos-15`（Apple Silicon） |
| `lkb-web-v<version>.tar.gz` | 可选 | `web/dist` SPA，供静态托管（须把 `/api` 同源反代到 lkb） |
| `ghcr.io/linxira-os/linxira-knowledge-base:<version>` | enterprise | 另推 `:<major.minor>`；`:latest` 仅正式 release 推送 |
| `linxira-knowledge-base-enterprise-v<version>.tar.gz` | enterprise | compose + nginx + 示例配置 + `web/dist` |
| `SHA256SUMS` | 全部 | 归档与 SBOM 的 SHA-256 清单 |
| `linxira-knowledge-base-v<version>.spdx.json` | 全部 | SPDX 2.3 依赖 SBOM（Cargo.lock + package-lock.json） |
| 构建来源证明（attestation） | 全部 | `actions/attest-build-provenance`；镜像另有 buildx provenance + SBOM attestation |

归档内容（solo）：`lkb`（Windows 为 `lkb.exe`）、`LICENSE`、`README.md`、`CHANGELOG.md`、`deploy/solo/lkb.toml.example`、`deploy/solo/README.md`。

**明确不发布**：debug 构建、`target-*`、测试报告、`node_modules`、源码包（GitHub Release 页面自动附带 source zip/tar）。**暂不发布**：Linux arm64、Docker arm64、PDF feature（`--features pdf`）专用构建——列为实验，先在文档说明。

已知限制：Windows 与 macOS 归档均未签名（无 Authenticode / Developer ID 证书），Windows 可能触发 SmartScreen、macOS 需手动放行首次运行。

## 4. 触发方式与门禁

```text
push tag v*                    -> 正式发布（非 draft，推镜像并标 :latest）
workflow_dispatch(tag, dry_run) -> dry_run 默认 true
```

- `dry_run=true`：完整构建全部资产、上传 workflow artifacts、计算 `SHA256SUMS` 并生成 attestation；**不创建 release、不推镜像**。适合发布前验证。
- `dry_run=false`：要求 tag 已存在，行为与 tag push 相同（重跑用）。
- 发布门禁（`release.yml` 内，与 CI 独立重跑一次）：
  1. `prepare`：tag/版本一致性、CHANGELOG 段存在；
  2. `quality`：`cargo test --locked`、`docker compose -f deploy/compose.yaml config`、`nginx -t`（容器内）；
  3. `rust`/`web`：四个目标原生构建 + `lkb --version` 冒烟 + `npm ci && npm run build`；
  4. `package`/`assets`：归档清单校验（`scripts/check-archive.sh`）、SPDX SBOM 生成；
  5. `docker`：Dockerfile 构建；dry-run 额外加载镜像并验证容器 `/health`。

## 5. 发布操作

dry-run（任何时候可做，不需要 tag 存在）：

```bash
gh workflow run release.yml -f tag=v0.2.0 -f dry_run=true
gh run watch --exit-status
```

正式发布（tag push 是规范路径）：

```bash
# 前置检查：Cargo.toml == web/package.json == web/package-lock.json == 0.2.0
#          CHANGELOG.md 含 "## [0.2.0]"，且 dry-run 全绿
git checkout main && git pull --ff-only
git tag -a v0.2.0 -m "lkb v0.2.0"
git push origin v0.2.0
gh run watch --exit-status
```

tag 已存在时的重跑：`gh workflow run release.yml -f tag=v0.2.0 -f dry_run=false`。

## 6. 校验、签名与 SBOM

```bash
sha256sum -c SHA256SUMS
gh attestation verify lkb-solo-v0.2.0-x86_64-unknown-linux-gnu.tar.gz \
  --repo Linxira-OS/linxira-knowledge-base
```

- 归档与 `SHA256SUMS` 由 GitHub Artifact Attestations 证明来源（Sigstore，绑定 workflow/commit）；当前**没有** GPG/cosign 自签密钥。
- 镜像：`docker buildx imagetools inspect <image>` 查看 digest 与 attestation；`docker scout attest get --predicate-type=https://spdx.dev/Document <image>` 读取镜像 SBOM。
- `linxira-knowledge-base-v<version>.spdx.json` 是来源级 SBOM（锁定依赖清单），不是二进制扫描结果；许可证字段为 `NOASSERTION`。

## 7. 回滚

1. **撤下 release**：`gh release delete v0.2.0 --yes`（保留 tag），如需同时删远端 tag 加 `--cleanup-tag`，或手动 `git push origin :refs/tags/v0.2.0` + `git tag -d v0.2.0`。
2. **回退 latest**：`gh release edit v0.1.9 --latest`（`latest` 只影响 Release 页面与 API 的“最新”指向）。
3. **镜像**：GHCR 上的旧 tag 仍可拉取；如需撤下某个版本，在 package 设置里删除对应版本（公开可见性不变）。
4. **数据**：迁移前向、无 down 脚本。回滚必须恢复备份并使用旧二进制 / 旧镜像；`deploy/README.md` §4 给出备份命令。

## 8. 企业部署升级步骤

1. 备份：`docker compose -f deploy/compose.yaml exec db pg_dump -U lkb lkb > lkb-$(date +%F).sql`，并记录当前镜像 digest（`docker compose ... images --digests`）。
2. 修改 `deploy/.env` 的 `LKB_IMAGE_TAG=<新版本>`。
3. `docker compose -f deploy/compose.yaml pull && docker compose -f deploy/compose.yaml up -d`。
4. 观察启动迁移日志与 `curl -fsS http://localhost:8080/health`（含 `seed` 元数据）。
5. 用固定问题集跑一次 `lkb eval` 并与基线对比。
6. 回滚：把 `LKB_IMAGE_TAG` 换回旧版本、恢复备份；不要在同一数据库上混跑新旧多实例（迁移无 advisory lock，限流/配额为进程内计数）。

## 9. 相关链接

- 工作流：[`.github/workflows/release.yml`](../.github/workflows/release.yml)；质量门禁复用 [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) 的同一套检查。
- 部署资产与运维：[`deploy/README.md`](../deploy/README.md)；个人版说明（随归档分发）：[`deploy/solo/README.md`](../deploy/solo/README.md)。
- 构建策略（本地构建须先确认）：[`document/build-policy.md`](build-policy.md)。
- 反馈：Issues https://github.com/Linxira-OS/linxira-knowledge-base/issues ；安全漏洞私密上报 https://github.com/Linxira-OS/linxira-knowledge-base/security/advisories/new 。
