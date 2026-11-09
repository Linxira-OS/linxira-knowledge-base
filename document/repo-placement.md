# 仓库归属与发布前合规（Repo Placement）

> 状态：决策记录（生效中）
> 适用范围：代码托管位置、品牌资产归属、发布前合规清单、许可证边界。
> 核查时间：2026-11-08。事实口径见 §0；未完成项在 §4 显式标注"未完成"，不当作已具备。
> **许可证更新（2026-11-09，v0.2.2）**：仓库已改为分层授权——核心 `AGPL-3.0-only`，
> 产品面 `MIT OR Apache-2.0`，核心可选商业许可，见 `LICENSING.md`。本文
> §0/§4 中的 `MIT` 记录是 2026-11-08 的历史快照，不描述当前许可证。

## 0. 核查到的事实

| 项 | 实测结果 |
| --- | --- |
| `git remote -v` | 空，没有任何 remote |
| `git log` | 无提交（`your current branch 'master' does not have any commits yet`） |
| 分支 | 本地 `master`，无 upstream、无其他分支 |
| `Cargo.toml` | `name = "lkb"`、`version = "0.2.0"`、`edition = "2021"`、`license` 字段为 MIT（当时） |
| 根目录合规文件 | 本轮之前无 `LICENSE`、`SECURITY.md`、`CONTRIBUTING.md`、`CODEOWNERS`，无 `.github/`；本轮已补 `LICENSE` |
| `brand/` | 当前不存在（品牌资产由独立工作流产出，本文只定义其归属） |
| 许可证红线 | `document/oss-landscape.md` §1 第 5 条与 §11 已定：AGPL / BSL / 改版 Apache / 品牌条款项目不得嵌入分发，连接器只走进程外 HTTP/文件 |

`Cargo.toml` 的 `license` 只是元数据，**不构成许可证文本**；`LICENSE` 文件才是对外授权凭据。当时两者一致（MIT）；v0.2.2 起根包为 `AGPL-3.0-only`。

## 1. 结论先行

1. **组织仓库是唯一 canonical（权威源）。** 代码、文档、`brand/` 品牌资产、发布标签、安全公告都只在组织仓维护。
2. **个人仓库只承担三类用途**：实验分支、未成形草稿、以及公开后的 fork。个人仓不接收 PR、不打 tag、不产出 release，也不承载唯一的品牌资产。
3. **品牌资产随主仓维护。** `brand/`（logo 源文件、色板、字体许可说明、使用规范）放在 canonical 仓的 `brand/` 目录，与代码同一套 review / branch protection / 历史。
4. **若组织已有独立的 `brand` 仓，只做镜像，并指回主仓。** 镜像仓接受的是单向同步（CI 推送），不直接编辑；文件头部与仓 README 都必须写明 canonical 路径。不允许出现两个可编辑源（双源），否则 logo 版本、许可说明与商标声明会分叉。
5. **本项目的对外仓库名用 `lkb`，组织名以实际组织为准。** 本文件不臆造公司主体；`LICENSE` 的版权行暂用 `The lkb Authors`，公开发布前由法人/组织确认替换为正式主体（见 §5）。

一句话：**个人账号是工作台，组织账号是发货仓库；品牌资产跟着发货仓库走。**

## 2. 决策矩阵

评分口径：**组织仓** 表示"组织仓明显更优"，**个人仓** 表示"个人仓更合适"，**持平** 表示差异可忽略。所有结论按"公开发布 + 有外部使用者"的目标状态评估，不按当前个人开发状态评估。

| 维度 | 个人仓 | 组织仓 | 判定 | 说明 |
| --- | --- | --- | --- | --- |
| 所有权 / IP | 归个人账号；贡献者默认与个人建立关系，权利链不清 | 归组织；贡献通过 DCO/CLA 进入组织资产 | **组织仓** | 企业采购与尽调看的是组织持有 IP，不是个人昵称 |
| 访问与离职交接 | 账号即单点；人走仓空，或需强制转移 | 组织成员/团队角色管理，离职即撤权，仓库不动 | **组织仓** | 仓库转移虽可行，但 history/issue 链接与外部引用会断 |
| 分支保护 / 必审 | 免费个人私有仓保护能力受限；无 CODEOWNERS 强制审 | 组织策略统一：必需 review、CODEOWNERS、必需状态检查、禁 force push | **组织仓** | 单人开发时也用，防止未来的自己绕过 |
| CI 与 secrets | 与个人其他项目混用额度与 secret 命名空间 | 组织级 runner、环境保护规则、环境 secret、OIDC 到云 | **组织仓** | 发布 job 需要 environment 审批，个人仓难做隔离 |
| 安全扫描 / Dependabot | 可开，但告警落到个人；无组织级汇总 | 组织级 Dependabot、code scanning、secret scanning 策略统一 | **组织仓** | 依赖告警要有人接，组织能指派安全负责人 |
| 发布签名与供应链 | 签名身份绑个人密钥/账号，信任半径小 | 组织级签名（cosign keyless + OIDC）、provenance、SBOM 挂 release | **组织仓** | 供应链证明要能追到组织身份 |
| 许可证合规（AGPL/BSL 连接器隔离） | 隔离靠自觉，无强制检查 | 可加 license allowlist CI（`cargo deny`）、连接器独立目录与独立许可 | **组织仓** | 见 §5；这条是硬约束，不能靠人记 |
| 社区 / issue / PR | issue 与 star 归个人，转仓后部分引用失效 | 组织 issue 模板、讨论区、安全公告、triage 权限 | **组织仓** | 安全公告必须由组织发布才有可信度 |
| 成本 | 个人免费额度，几乎为零 | 组织席位/私有仓/Actions 分钟数有支出（公开仓 Actions 免费） | **个人仓（略）** | 成本是唯一个人仓占优项，且金额可忽略 |
| 迁移成本 | 从个人迁到组织：改 remote、重配 secret、修 CI、通知贡献者 | 现在直接建组织仓，成本≈一次 `git init` 的增量 | **组织仓** | 当前无 commit 无 remote，是成本最低的时点 |

**加权结论**：除"成本"外全部指向组织仓；而成本差异在公开仓 Actions 免费的条件下接近零。因此 canonical 定在组织仓，个人仓降级为 fork/试验场。

## 3. 迁移路径（当前零提交、零 remote）

### 3.1 第 0 步：首次提交前先补 `.gitignore`

根 `.gitignore` 原先只忽略 `target/`、`data/`、`blobs/`、`.lkb/`、`*.sqlite*`、`__pycache__/`、`.venv/`。`experiments/ui-shells/.gitignore` 另行忽略了 `node_modules/`、`dist/`、`renderer/`、`release/`、`src-tauri/gen/`，所以 `node_modules` 此前已不在 `git status` 里（实测 `experiments/ui-shells/electron/node_modules` 约 437 MB，未入库）。但这套规则只覆盖 `ui-shells/` 一层，换目录或遇到 `profiles/`、`.vite/`、Edge 浏览器残留就会漏；本轮把同类规则提升到根 `.gitignore`：

```text
node_modules/
experiments/**/dist/
experiments/**/release/
experiments/**/target/
experiments/**/src-tauri/target/
experiments/**/.vite/
experiments/**/profiles/
experiments/**/edge-*
```

- `experiments/**/edge-*` 命中的是 Edge 浏览器作废产物（`bench/results/edge-void/` 约 161 MB）；`.vite/`、`profiles/` 同类处理。
- **保留** `experiments/ui-shells/bench/results/` 下的 `*.json`、`*.jsonl`、`*.log` 与 `chrome/*.png`：3 张 PNG 合计 423 KB，体积可控，作为实测证据随首次提交入库，无需裁剪。
- 复核结果：`git status --short -uall` 剩 242 个未跟踪文件、合计约 2.53 MB；`node_modules` / `release` / `target` / `dist` / `.vite` / `profiles` / `edge-*` 零命中。首次 `git add` 前仍人工复核一次 `git status`。

### 3.2 第 1 步：首次 commit

```bash
git add .gitignore AGENTS.md Cargo.toml Cargo.lock README.md LICENSE \
        src tests migrations examples docs document \
        experiments   # 确认 node_modules 已被忽略后再加
git status --short          # 人工复核：不应出现 node_modules、target、*.sqlite
git commit -m "chore: initial import (MIT)"
```

- 首个 commit 即带 `LICENSE`（MIT），避免"先公开、后补许可"造成的历史授权空窗。
- `Cargo.lock` 属于应用仓库，应提交；不要忽略。
- 此刻仍无 remote，本地提交不产生任何公开发布。

### 3.3 第 2 步：建组织仓

- 组织下建仓，名 `lkb`（与 crate 名一致）。
- 可见性：**private 或 public 由组织政策决定**。若组织政策不允许 public，则先 private，公开发布作为单独审批事件；公开前完成 §4 清单。
- 不要勾选"用 README/`.gitignore`/`LICENSE` 初始化"——本地已有完整树，避免无谓 merge。

```bash
git remote add origin git@github.com:<org>/lkb.git
git branch -M main            # 与组织默认分支名对齐；若组织用 master 则跳过
git push -u origin main
```

### 3.4 第 3 步：push 后立刻配置

| 项 | 配置内容 |
| --- | --- |
| 分支保护（`main`） | 禁止直接 push、禁止 force push、禁止删除；必需 PR + 至少 1 个 approval；必需状态检查（fmt/clippy/test）；建议 linear history |
| `CODEOWNERS` | `*` 归核心维护者；`document/`、`migrations/`、`brand/` 单独指定 owner |
| Actions | 最小权限 `permissions: contents: read`；发布 job 用 environment + 人工审批 |
| Secrets | 组织级/环境级 secret；只放 `LKB_*_API_KEY` 这类运行时凭据的 CI 测试值，禁止提交真实密钥 |
| Secret scanning | 开启 secret scanning + push protection |
| Dependabot | 开启 `cargo` 生态更新与安全告警；指派安全负责人 |
| 安全公告 | 开启 private vulnerability reporting（`SECURITY.md` 落地后） |

### 3.5 第 4 步：个人仓只做 fork

- 个人仓从组织仓 fork，用途限实验/草稿；不提 PR 到自己的 fork 作为主流程。
- 个人 fork 不配置发布 secret、不打 release tag、不写 `brand/` 的"最新版本"。

### 3.6 若将来必须转移仓库

GitHub 的仓库转移会保留：**issues、PR（含评论）、stars、watchers、fork 网络关系、commit 历史、release 与 tag**。不会保留/需要重做：

- 本地 remote URL（`git remote set-url origin ...`）；
- **Actions secrets 与 environment secrets 不随转移迁移**，必须在新位置重建；
- webhook、deploy key、GitHub App 安装、Pages 自定义域、branch protection 规则需逐项复核；
- 外部文档与包管理器里的旧 URL 会 301 到新地址，但镜像/README 里的硬编码 URL 要手动改。

结论不变：**在零提交时直接建组织仓，比任何时点转移都便宜。**

## 4. 发布前最小清单

状态以本轮核查为准。**"未完成"就是未完成**，不得在 README 或 release notes 里暗示已具备。

| 项 | 状态 | 最小要求 |
| --- | --- | --- |
| `LICENSE`（MIT） | **已完成（本轮）** | 根目录标准 MIT 文本；版权行 `Copyright (c) 2026 The lkb Authors`；公开发布前确认主体 |
| `Cargo.toml` 的 `license` 字段 | **已完成（已存在）** | 当时与 `LICENSE` 一致（MIT）；v0.2.2 起根包改为 `AGPL-3.0-only` |
| `SECURITY.md` | **未完成** | 受支持版本、私密上报渠道（GitHub private reporting 或安全邮箱）、响应时限、披露策略、范围（含 solo 本地部署的信任边界） |
| `CONTRIBUTING.md` | **未完成** | 构建前置确认（见 `document/build-policy.md`）、分支/PR 约定、DCO 签名要求、文档先行规则、ADR 流程 |
| `CODEOWNERS` | **未完成** | `*` 与 `document/`、`migrations/`、`brand/` 的 owner；与分支保护的"必需 CODEOWNERS review"配套 |
| CI：fmt / clippy / test | **未完成**（无 `.github/`） | `cargo fmt --check`；`cargo clippy --all-targets -- -D warnings`；`cargo test`；矩阵至少 Linux stable |
| CI 的 GNU 工具链说明 | **未完成** | 本仓库在 Windows 上默认 `stable-x86_64-pc-windows-msvc` 因 `link.exe` 解析到 Git coreutils 的 `link`（缺 `cl.exe`）无法链接；已验证路径是 `stable-x86_64-pc-windows-gnu` + MSYS2 gcc 16.1.0（见 README"当前状态"）。CI 固定 Linux `x86_64-unknown-linux-gnu`，Windows job 显式安装 GNU target 与 MinGW，不依赖 MSVC |
| 依赖许可证扫描 | **未完成** | `cargo deny check licenses advisories`（或 `cargo-about`）写入 CI；allowlist 至少 MIT / Apache-2.0 / BSD-3-Clause / ISC / Unicode-3.0，AGPL/BSL 一律拒绝进默认依赖图 |
| 依赖安全检查 | **未完成** | `cargo audit` 或 `cargo deny check advisories`；Dependabot 告警指派负责人 |
| Release 签名 | **未完成** | 二进制与校验和签名（cosign keyless 或 minisign）；release 附 `SHA256SUMS`；tag 保护 |
| SBOM | **未完成** | 每次 release 产出 CycloneDX（`cargo cyclonedx`）或 SPDX，并作为 release asset 附带 |
| 供应链证明 | **未完成** | GitHub artifact attestation / SLSA provenance（可选但建议，与签名同批做） |
| 商标与品牌使用说明 | **未完成** | `brand/README.md` 说明 logo 使用边界；商标归属写在组织主体名下（见 §7） |
| 安全扫描（code scanning） | **未完成** | CodeQL 或等价；至少对 `src/` 开启 |

`document/build-policy.md` 的"构建前必须确认"约束仍然有效：CI 里的 cargo 命令属于已批准流水线，本地执行仍需按该文件确认。

## 5. 许可证策略

1. **核心代码：AGPL-3.0-only（v0.2.2 起）。** 检索 / 回答 / 治理 / ACL / 存储 / 摄入 / 模型网关 / 配额为核心，产品面（CLI、配置、Web、桌面、部署、脚本、文档、测试）为 `MIT OR Apache-2.0`；闭源嵌入、OEM、白标、闭源 SaaS 需商业许可。`LICENSE` 与根 `Cargo.toml` 的 `license = "AGPL-3.0-only"` 必须始终一致，边界见 `LICENSING.md`。
2. **连接器（Confluence / MediaWiki / Outline / Notion 等）只做进程外集成。** 依据 `document/oss-landscape.md`：
   - 通过 HTTP API / 导出文件交互，**不链接、不 vendoring、不复制第三方源码**；
   - 不把第三方代码编进 `lkb` 单二进制；
   - ACL 拿不到时 `deny`，不用"全员可见"兜底。
3. **若某个连接器必须基于 AGPL/BSL 代码**（例如复用其 SDK 或嵌入其服务端模块）：
   - 放在**独立目录、独立 crate、独立许可证文件**（如 `connectors/<name>/LICENSE` + `NOTICE`），并作为**独立进程/独立发行物**，不进入默认 build；
   - AGPL 组件分发时随附对应源码与许可文本；BSL 组件核对 Additional Use Grant 与 Change Date，禁止把它当作可自由分发的依赖；
   - 默认构建与默认配置不启用该组件；
   - 该边界属于法务确认项，不因"只是调 API"自动成立。
4. **Dify（改版 Apache，禁多租户、禁去 logo）、Open WebUI（品牌条款）、Wiki.js / Docmost / Khoj / AppFlowy（AGPL）、Outline（BSL）** 均不得作为运行时依赖或嵌入组件；只借鉴设计。
5. **禁止把第三方代码"顺手"复制进 `src/`。** 任何外部代码进入本仓，必须在 PR 中标注来源、许可证，并更新 `NOTICE`（若有）。
6. 发布物（二进制、容器镜像）因链接核心，整体按 AGPL-3.0-only 分发并随附 Corresponding Source 义务；`MIT OR Apache-2.0` 层可单独提取。种子文档与 `docs/` 随仓库分层许可。

## 6. 风险

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 组织政策禁止 public 仓 | 计划中的公开首发被阻断，或被迫先私有后迁移 | 建仓前确认组织可见性政策；私有先跑通 §4，公开发布作为独立审批 |
| CLA 与 DCO 选择 | CLA 需贡献者签权利转让，外部贡献意愿下降；无 CLA 则 IP 链弱 | 默认 **DCO**（`Signed-off-by`），不要求版权转让；企业客户尽调时再评估 CLA |
| 商标归属 | logo 若由个人账号唯一持有，组织无法主张商标，也无法约束他人使用 | 商标以组织/法人名义申请与登记；`brand/` 随 canonical 仓；个人仓不持有唯一源 |
| 个人账号作为 bus factor | 账号丢失/封禁/离职导致仓库、issue、release 一起消失 | canonical 在组织；关键维护者 ≥2；开启组织级 2FA 与恢复码托管 |
| 企业客户对 org 的信任 | 个人仓签发的 release 难以通过采购与安全评审 | 组织身份 + 签名 + SBOM + `SECURITY.md` + 审计日志 |
| 双源品牌资产 | logo 版本、色值、使用规范分叉，对外物料不一致 | 单一 canonical + 单向镜像；镜像仓禁止直接编辑 |
| 首次提交误带 `node_modules` | 仓库体积暴涨、第三方许可证文本混入、历史难以清理 | 根 `.gitignore` 已加固（§3.1）；提交前 `git status` 人工复核 |
| 连接器许可越界 | AGPL/BSL 传染到主仓，迫使整体换许可或下架 | §5 进程外隔离 + 独立目录/独立许可 + CI license allowlist |
| Windows 链接器环境 | 贡献者按 README 在 MSVC 下构建失败，误判项目不可构建 | CI 固定 GNU 工具链；README 与 `CONTRIBUTING.md` 写明 MSVC `link.exe` 冲突与替代路径 |

## 7. 品牌资产（logo）放哪

**明确答案：放 canonical 组织仓的 `brand/` 目录。**

| 位置 | 角色 | 规则 |
| --- | --- | --- |
| `<org>/lkb` 的 `brand/` | **唯一源** | logo 源文件（SVG 优先）、导出位图、色板、字体与字体许可说明、使用规范；改动走 PR + `brand/` CODEOWNER 审阅 |
| 组织已有的 `brand` 仓（若存在） | **只读镜像** | 由 CI 从主仓单向同步；仓 README 与每个文件头部注明"canonical 在 `<org>/lkb/brand/`，请到主仓提 PR"；禁止在此直接编辑 |
| 个人仓 / 个人 fork | **不承载** | 可放草稿，但不得作为对外引用的唯一地址；对外链接一律指向 canonical |

理由：品牌资产与代码共享同一套 review、保护规则、历史与责任主体；镜像仓只是分发便利，不是第二个事实源。若镜像与主仓冲突，以主仓为准并强制覆盖镜像。

`brand/` 目录当前尚未创建（见 §0）。创建时至少包含：`brand/README.md`（使用边界与商标声明）、`brand/logo/`（SVG + PNG 导出）、`brand/palette.md`（色值）、`brand/fonts/` 或字体许可说明。

## 8. 待办（按顺序）

1. 补 `.gitignore`（`node_modules/`、`dist/`、`release/`、`target/`、`.vite/`、`profiles/`、`edge-*`），复核 `git status`。**已完成（本轮，见 §3.1）**
2. 首次 commit（MIT，含 `LICENSE`）。**未完成**
3. 建组织仓 `lkb`，push，配置 §3.4 的保护与扫描。**未完成**
4. 补 `SECURITY.md`、`CONTRIBUTING.md`、`CODEOWNERS`、`.github/workflows/ci.yml`（fmt/clippy/test + GNU 工具链）。**未完成**
5. 依赖许可/安全扫描、release 签名、SBOM 接入发布流程。**未完成**
6. `brand/` 落地并接入镜像策略。**未完成**
7. 公开发布前由法人/组织确认 `LICENSE` 版权主体并替换 `The lkb Authors`。**未完成**
