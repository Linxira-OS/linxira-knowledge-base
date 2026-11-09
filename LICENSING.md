# Licensing / 许可证

> Status: effective from `v0.2.2` on the `main` line. Supersedes the single-license
> README and `Cargo.toml` statements of `v0.2.1` and earlier.
> 状态：自 `v0.2.2` 起在 `main` 生效；取代 `v0.2.1` 及更早版本的单许可证表述。

This repository uses **layered licensing**: the commercially valuable kernel is
AGPL-3.0-only, while the surrounding product surface (CLI, config, web UI,
desktop shell, deployment, scripts, docs, tests) is `MIT OR Apache-2.0`.

本仓库采用**分层授权**：商业价值最高的内核为 AGPL-3.0-only；周边产品面（CLI、配置、
Web UI、桌面壳、部署、脚本、文档、测试）为 `MIT OR Apache-2.0`。

## English

### 1. Layer table

| Layer | License | Paths |
| --- | --- | --- |
| Core | **AGPL-3.0-only** (`LICENSE`) | `src/search.rs`, `src/answer.rs`, `src/governance.rs`, `src/access.rs`, `src/contract.rs`, `src/store/**`, `src/ingest/**`, `src/providers/**`, `src/gateway.rs`, `src/token_quota.rs`, `src/quota.rs`, `src/lib.rs`, `migrations/**` |
| Product surface | **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE`) | `src/cli.rs`, `src/paths.rs`, `src/config.rs`, `src/util.rs`, `src/error.rs`, `src/i18n.rs`, `src/logging.rs`, `src/metrics.rs`, `src/limits.rs`, `src/connectors.rs`, `src/payment.rs`, `src/billing.rs`, `src/summary.rs`, `src/conversation.rs`, `src/seed.rs`, `src/frontmatter.rs`, `src/mcp.rs`, `src/eval.rs`, `src/event.rs`, `src/audit.rs`, `src/openapi.rs`, `src/web.rs`, `src/main.rs`, `web/**`, `desktop/**`, `deploy/**`, `scripts/**`, `docs/**`, `document/**`, `tests/**` |

**Why this boundary.** Retrieval, answer generation, governance fields, ACL
enforcement, storage adapters, ingestion, the model gateway, and quota/ledger
enforcement are the parts that make the product commercially valuable and are
the parts a competitor could otherwise embed without contributing back. The
permissive layer is the integration surface: CLI, configuration, paths, i18n,
observability, the web/desktop shells, deployment automation, documentation, and
tests. Those can be reused on their own (tooling, UIs, deployment recipes)
without triggering AGPL obligations.

SPDX headers in source files (`// SPDX-License-Identifier: ...`, `-- SPDX-...`
in SQL migrations, `# SPDX-...` in scripts) are authoritative for the file they
appear in. If a header and this table ever disagree, the **stricter** license
(AGPL-3.0-only) applies.

### 2. What you may do with the AGPL core

AGPL-3.0-only permits, without contacting the author:

- internal use inside your organization, including commercial internal use;
- modification, private forks, and self-hosting;
- redistribution, provided the AGPL-3.0-only terms are met;
- network service operation, provided users interacting with it over a network
  are offered the **Corresponding Source** of the version you run (AGPL §13),
  including modifications, plus the license notices.

The core is **not** dual-licensed with MIT/Apache. There is no permissive
option for the core paths in the table above.

### 3. Commercial license (AGPL core only)

A separate commercial license from the copyright holder is required when you
want to avoid AGPL-3.0-only obligations for the core, including:

- embedding or linking the core into a closed-source product;
- OEM distribution, bundling into hardware/appliances, or white-labeling under
  another brand;
- operating a modified core as a closed-source SaaS without offering
  Corresponding Source;
- any distribution that cannot satisfy AGPL §4–§6 / §13 (source offer, notice
  preservation, installation information where applicable).

**How to ask:** open an issue at
<https://github.com/Linxira-OS/linxira-knowledge-base/issues> and apply the
`commercial-license` label, or contact the organization through
<https://github.com/Linxira-OS>. Commercial terms (scope, term, fees, support)
are negotiated with the author case by case. No commercial license is granted
by default. There is no published commercial price list and no separate sales
email; the issue tracker is the single entry point.

The `MIT OR Apache-2.0` layer can be extracted and used commercially on its own
under those licenses; no commercial license is needed for that layer. Extraction
means reusing the permissive files **without** the AGPL core: if you combine
them with the core, the resulting work is AGPL-3.0-only.

### 4. Whole-binary and container distribution

The `lkb` binary, the desktop bundle, and the container image
`ghcr.io/linxira-os/linxira-knowledge-base` combine the permissive layer with
the AGPL core. Because the combined work links the core, **the distributed
binary/container as a whole is licensed under AGPL-3.0-only** even though most
source files carry a permissive identifier. The Docker image therefore declares
`org.opencontainers.image.licenses="AGPL-3.0-only"`. If you redistribute a
build, include the Corresponding Source (or a written offer valid for three
years) and keep the license notices.

### 5. Version history

| Version | License |
| --- | --- |
| `v0.2.1` and earlier | MIT (see `CHANGELOG.md`; those release artifacts remain MIT and are **not** retroactively relicensed) |
| `v0.2.2` and later | Layered: AGPL-3.0-only core + `MIT OR Apache-2.0` product surface + commercial option for the core |

The switch applies to the source tree from `v0.2.2` onward. Copies of `v0.2.1`
archives, installers, and container tags keep the terms they were published
under.

### 6. Contributions

By opening a pull request you agree that your contribution is licensed under
the license that applies to the files you touch (AGPL-3.0-only for the core,
`MIT OR Apache-2.0` for the permissive layer). For core contributions you
additionally grant the copyright holder a perpetual, worldwide, non-exclusive,
royalty-free right to relicense your contribution under the commercial license
described in section 3. This is a lightweight in-bound CLA recorded by the PR
itself; no separate signed form or copyright assignment is required. DCO
sign-off (`git commit -s`) is still mandatory. See `CONTRIBUTING.md`.

### 7. Third-party components

Third-party crates and npm packages keep their own licenses; the generated SPDX
SBOM lists the locked dependency set. Do not add a dependency whose license is
incompatible with the layer that uses it. AGPL/BSL components must run
out-of-process, as described in `CONTRIBUTING.md`.

---

## 简体中文

### 1. 分层授权表

| 层 | 许可证 | 路径 |
| --- | --- | --- |
| 核心 | **AGPL-3.0-only**（`LICENSE`） | `src/search.rs`、`src/answer.rs`、`src/governance.rs`、`src/access.rs`、`src/contract.rs`、`src/store/**`、`src/ingest/**`、`src/providers/**`、`src/gateway.rs`、`src/token_quota.rs`、`src/quota.rs`、`src/lib.rs`、`migrations/**` |
| 产品面 | **MIT OR Apache-2.0**（`LICENSE-MIT`、`LICENSE-APACHE`） | `src/cli.rs`、`src/paths.rs`、`src/config.rs`、`src/util.rs`、`src/error.rs`、`src/i18n.rs`、`src/logging.rs`、`src/metrics.rs`、`src/limits.rs`、`src/connectors.rs`、`src/payment.rs`、`src/billing.rs`、`src/summary.rs`、`src/conversation.rs`、`src/seed.rs`、`src/frontmatter.rs`、`src/mcp.rs`、`src/eval.rs`、`src/event.rs`、`src/audit.rs`、`src/openapi.rs`、`src/web.rs`、`src/main.rs`、`web/**`、`desktop/**`、`deploy/**`、`scripts/**`、`docs/**`、`document/**`、`tests/**` |

**边界理由。** 检索、回答生成、治理字段、ACL 判定、存储适配器、摄入、模型网关、
配额/账本执行是产品的商业价值核心，也是竞争对手最可能直接嵌入而不回馈的部分。
宽松层是集成面：CLI、配置、路径、i18n、可观测性、Web/桌面外壳、部署自动化、文档与
测试。这些部分可以被单独提取复用（工具、UI、部署配方），不触发 AGPL 义务。

源文件中的 SPDX 头（Rust/TS 用 `// SPDX-License-Identifier: ...`，SQL 迁移用
`-- SPDX-...`，脚本用 `# SPDX-...`）对该文件有效。若文件头与本表冲突，以**更严格**
的许可证（AGPL-3.0-only）为准。

### 2. AGPL 核心允许的用法

无需联系作者，AGPL-3.0-only 允许：

- 组织内部使用，包括商业性内部使用；
- 修改、私有分支与自托管；
- 再分发，但须遵守 AGPL-3.0-only 条款；
- 运营网络服务，但须向通过网络与之交互的用户提供所运行版本的
  **Corresponding Source（对应源码）**（AGPL 第 13 条），包括修改与许可证声明。

核心**不是** MIT/Apache 双许可；上表中的核心路径没有宽松选项。

### 3. 商业许可（仅限 AGPL 核心）

当需要规避核心的 AGPL-3.0-only 义务时，必须向版权持有人申请商业许可，典型场景：

- 将核心嵌入或链接进闭源产品；
- OEM 分发、随硬件/一体机捆绑，或换牌（white-label）销售；
- 以闭源 SaaS 运营修改后的核心，且不提供 Corresponding Source；
- 任何无法满足 AGPL 第 4–6 条 / 第 13 条（源码提供、声明保留、适用时的安装信息）
  的分发方式。

**联系方式：** 在
<https://github.com/Linxira-OS/linxira-knowledge-base/issues> 新建 issue 并加
`commercial-license` 标签，或通过组织主页 <https://github.com/Linxira-OS> 联系。
商业条款（范围、期限、费用、支持）由作者逐案协商；默认不授予任何商业许可；仓库不
公布商业报价，也不设单独销售邮箱，Issue 是唯一入口。

`MIT OR Apache-2.0` 层可单独按该许可证商用，无需商业许可。这里的“单独”指**不链接
AGPL 核心**地复用宽松文件；一旦与核心组合，组合作品整体按 AGPL-3.0-only 分发。

### 4. 整个二进制与容器分发

`lkb` 二进制、桌面安装包与容器镜像
`ghcr.io/linxira-os/linxira-knowledge-base` 同时包含宽松层与 AGPL 核心。由于组合
作品链接了核心，**对外分发的二进制/容器整体按 AGPL-3.0-only 授权**，即使多数源文件
带宽松标识。因此 Docker 镜像声明
`org.opencontainers.image.licenses="AGPL-3.0-only"`。再分发构建产物时，须一并提供
Corresponding Source（或有效期三年的书面提供承诺），并保留许可证声明。

### 5. 版本历史

| 版本 | 许可证 |
| --- | --- |
| `v0.2.1` 及更早 | MIT（见 `CHANGELOG.md`；已发布资产保持 MIT，**不追溯**变更） |
| `v0.2.2` 及以后 | 分层：AGPL-3.0-only 核心 + `MIT OR Apache-2.0` 产品面 + 核心商业许可选项 |

切换只作用于 `v0.2.2` 起的源码树；`v0.2.1` 归档、安装包与容器 tag 继续适用其发布时
的许可证。

### 6. 贡献授权

提交 PR 即表示同意：你的贡献按所修改文件适用的许可证授权（核心为 AGPL-3.0-only，
宽松层为 `MIT OR Apache-2.0`）。对核心的贡献，你还额外授予版权持有人一项永久、全球、
非独占、免版税的权利，使其可按第 3 节的商业许可对你贡献的部分进行再许可。这是通过
PR 记录在案的轻量入站 CLA，无需单独签署文件，也不要求版权转让。DCO 签名
（`git commit -s`）仍然强制。详见 `CONTRIBUTING.md`。

### 7. 第三方组件

第三方 crate 与 npm 包保留各自许可证；生成的 SPDX SBOM 列出锁定依赖集合。不得引入
与所在层许可证不兼容的依赖。AGPL/BSL 组件必须进程外运行，见 `CONTRIBUTING.md`。
