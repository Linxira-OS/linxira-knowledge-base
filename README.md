# Layered Knowledge Base (`lkb`)

**English** | [简体中文](#简体中文)

[![CI](https://github.com/Linxira-OS/linxira-knowledge-base/actions/workflows/ci.yml/badge.svg)](https://github.com/Linxira-OS/linxira-knowledge-base/actions/workflows/ci.yml)
[![License: AGPL-3.0-only core](https://img.shields.io/badge/License-AGPL--3.0--only%20core-blue.svg)](LICENSE)
[![License: MIT OR Apache-2.0 rest](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0%20rest-green.svg)](LICENSING.md)
[![Commercial license available](https://img.shields.io/badge/License-Commercial%20available-orange.svg)](LICENSING.md#3-commercial-license-agpl-core-only)
[![Rust stable](https://img.shields.io/badge/rust-stable-blue.svg)](https://www.rust-lang.org/)

> Repository: `Linxira-OS/linxira-knowledge-base`; crate / CLI / binary name: `lkb`.
> Topics: `rag` · `knowledge-base` · `wiki` · `rust` · `self-hosted` · `mcp`

**Positioning.** One retrieval kernel, three profiles — `solo`, `team`, and
`enterprise` — covering everything from a single-user local knowledge base to a
multi-tenant enterprise deployment. Profiles change configuration and adapter
wiring, never code paths ([ADR 0001](document/adr/0001-one-core-three-profiles.md)).

## English

### Why layered

Both ends of the usual spectrum fail:

- **Enterprise architectures are too heavy for individuals.** External
  databases, separate vector services, SSO, tenant isolation, and audit chains
  push deployment cost above the value of a personal knowledge base.
- **Personal tools cannot carry enterprise governance.** Pure local file
  search has no tenant / space / ACL / validity / version fields; retrofitting
  them later means schema migrations, data backfills, split recall paths, and
  an inability to answer "why was this result returned".

The answer is **one kernel with three profiles**. Four cross-profile invariants
apply from day one: governance fields exist in the schema
([ADR 0002](document/adr/0002-governance-fields-day-one.md)); files are the
source of truth and indexes are rebuildable
([ADR 0003](document/adr/0003-files-are-source-of-truth.md)); optional
capabilities degrade explicitly ([ADR 0004](document/adr/0004-explicit-degradation.md));
humans and agents share one `Answer` contract
([ADR 0005](document/adr/0005-single-answer-contract.md)).

### Three profiles

| Dimension | solo | team | enterprise |
| --- | --- | --- | --- |
| Deployment | Single process, SQLite file or PostgreSQL + local directory, no external service required | Rust server + PostgreSQL (pgvector), containerized | Multi-tenant cluster + PostgreSQL, per-tenant isolation and quotas, data residency |
| Identity | Local user; loopback auth off by default, optional local session (cookie + CSRF) | OIDC (organization accounts), space members and roles | SSO (SAML / OIDC) + directory provisioning + service accounts |
| Retrieval | Default `auto` → `hybrid` when an embedding provider has indexed vectors or an extractor has events, else `bm25`; judge reranks Top 30 and can also run after hybrid RRF; hybrid is optional (local ONNX embeddings + sqlite-vec + RRF); extractive answers without an LLM | PostgreSQL full-text + pgvector hybrid, RRF fusion then rerank (cross-encoder / judge optional) | Parallel keyword / vector / graph / multimodal channels → RRF → judge + cross-encoder + optional LLM selection |
| Governance | Full governance fields; ACL resolves to allow-all; space, validity, and version written per record; recall filtering still runs; local append-only log | `space_id` isolation + space ACL filtered at recall; centralized logs | `tenant_id` hard isolation + recall-time ACL; WORM audit and SIEM export |
| Typical scale | 10³ documents, one user | 10⁵ documents, 10–10² users | 10⁷ documents, 10³+ users |

Scale figures are design targets (order of magnitude), not measured results.
See [document/tier-matrix.md](document/tier-matrix.md) for per-item differences.

### Key capabilities

- **One Rust binary** (axum + sqlx), no Python runtime; SQLite and PostgreSQL
  share one `Store` abstraction with matching migrations.
- **Wiki** with pages, revisions, history, and rollback; `lkb seed` embeds the
  bilingual product docs (12 pages per language) as an idempotent seed.
- **Retrieval modes**: `auto` (default) resolves to `hybrid` when an embedding
  provider has indexed vectors or an extractor has events, otherwise `bm25`;
  explicit modes are `bm25`, `bm25_judge`, `event` / `event_entity`, `hybrid`;
  judge reranks Top 30 only (never a recall cap) and runs after hybrid RRF by
  default. Missing channels are reported in `degraded[]` with stable codes.
- **Governance day one**: `tenant_id` / `space_id` / `acl_json` /
  `valid_from` / `valid_to` / `version` / `superseded_by` on every document and
  chunk; recall filters always come from `Policy::visibility`.
- **Answer contract** shared by CLI, REST, and MCP: `answer`, `citations[]`,
  `confidence`, `conflicts[]`, `degraded[]`, `knowledge_as_of`, `mode`.
- **Model gateway, quotas, billing**: public aliases only; provider endpoints
  stay admin-only; token quota reservation, wallet/ledger, plans, entitlements,
  and payment adapters.
- **Auth**: loopback `none`, HttpOnly session + CSRF, Bearer tokens for
  agents/CLI, trusted reverse-proxy header SSO (OIDC/SAML/SCIM not implemented).
- **Operations**: structured JSON logs, Prometheus `/metrics`, rate limits and
  backpressure, audit events, zh-CN/en i18n.
- **Web SPA + Tauri desktop shell** sharing the same `web/dist`; desktop
  installers ship for Windows and Linux (macOS is not released).

### Does retrieval need a model?

**No.** On SQLite, `retrieval.mode = "auto"` resolves to `hybrid` when an
embedding provider has indexed vectors or an extractor has events; otherwise it
stays on `bm25`. Answers are extractive when no LLM is configured.
The optional upgrades:

| Mode | Model dependency |
| --- | --- |
| `bm25` | None |
| `bm25_judge` | Judge / rerank model (reorders Top 30 only, not a recall cap) |
| `event` | Extractor model |
| `hybrid` | Embedding model for the dense channel; extractor and judge optional |
| `auto` | SQLite: embedding + vectors → `hybrid`; else extractor + events → `hybrid`; else `bm25`; PostgreSQL: always `hybrid` |

`auto` on SQLite checks a configured embedding provider's visible vectors first;
without vectors it writes `index_missing` and falls through to extractor/events,
then `bm25`. On PostgreSQL, `auto` always resolves to `hybrid`; without an
embedding provider the dense channel degrades and keyword retrieval still works.

Missing models degrade (`degraded[]`) and never block keyword retrieval. Full
details and recommended setups: [document/retrieval-and-models.md](document/retrieval-and-models.md).

### Quick start

Prerequisites: Rust stable (>= 1.82) plus a working C compiler/linker. Bundled
SQLite still needs a C toolchain; on Windows install MSVC Build Tools + Windows
SDK, or use MinGW. See [document/build-policy.md](document/build-policy.md).

```bash
# Windows: verified GNU toolchain path (MSVC works too once Build Tools are installed)
cargo +stable-x86_64-pc-windows-gnu build --release

# solo: SQLite, auth=none, bound to 127.0.0.1
target/release/lkb config check
target/release/lkb import examples
target/release/lkb seed
target/release/lkb search "Overview" --lang en
target/release/lkb ask "What is the Answer contract?"
target/release/lkb eval examples/questions.jsonl --k 5
target/release/lkb serve          # http://127.0.0.1:7331/
```

- Windows binary: `target\release\lkb.exe`; set variables with
  `$env:LKB_SQLITE_PATH = "..."`.
- Desktop: install `lkb-solo-v0.2.1-windows-x64-setup.exe` (NSIS) or the
  portable zip; on Linux use the `.deb`, `.AppImage`, or portable tarball.
  Packages are unsigned, so Windows SmartScreen may prompt.
- Default paths (override with `LKB_CONFIG_DIR` / `LKB_DATA_DIR` /
  `LKB_LOG_DIR`):

| Platform | Config | Data | Logs |
| --- | --- | --- | --- |
| Windows | `%APPDATA%\lkb\lkb.toml` | `%LOCALAPPDATA%\lkb\data` | `%LOCALAPPDATA%\lkb\logs` |
| Windows portable | `.\lkb-data\lkb.toml` | `.\lkb-data\data` | `.\lkb-data\logs` |
| Linux | `$XDG_CONFIG_HOME/lkb/lkb.toml` | `$XDG_DATA_HOME/lkb` | `$XDG_STATE_HOME/lkb/logs` |

Team / enterprise: set `profile = "team"`, `[storage] backend = "postgres"`,
export `LKB_DATABASE_URL`, and run `lkb serve`.

- **Security**: solo `auth.mode = "none"` only accepts loopback; secrets are read
  from environment variables and never written to config or logs; team /
  enterprise CLI uses Bearer tokens stored as SHA-256 hashes; desktop PostgreSQL
  credentials live in the OS keychain.
- **Full walkthrough**: [docs/en/03-quickstart.md](docs/en/03-quickstart.md);
  configuration reference: [docs/en/07-configuration.md](docs/en/07-configuration.md);
  example file: [deploy/solo/lkb.toml.example](deploy/solo/lkb.toml.example).

### Documentation

- [docs/en](docs/en/00-index.md) · [docs/zh](docs/zh/00-index.md) — product
  guides, also shipped as the compiled-in seed content
- [document/architecture.md](document/architecture.md) — kernel layers,
  boundaries, data flow
- [document/tier-matrix.md](document/tier-matrix.md) — per-profile capability
  matrix
- [document/implementation-status.md](document/implementation-status.md) —
  implementation details, migrations, verification record
- [document/retrieval-and-models.md](document/retrieval-and-models.md) —
  retrieval modes and model dependencies
- [document/product-spec.md](document/product-spec.md) — naming, paths,
  versioning, UI, packaging, and license freeze
- [document/knowledge-freshness.md](document/knowledge-freshness.md) ·
  [document/oss-landscape.md](document/oss-landscape.md) ·
  [document/sag-assessment.md](document/sag-assessment.md)
- [document/model-gateway-and-billing.md](document/model-gateway-and-billing.md) ·
  [document/summaries-and-conversations.md](document/summaries-and-conversations.md)
- [document/spaces-and-permissions.md](document/spaces-and-permissions.md) ·
  [document/agent-access-and-observability.md](document/agent-access-and-observability.md)
- [document/deployment-modes-i18n-payments.md](document/deployment-modes-i18n-payments.md) ·
  [document/desktop-app-and-notes.md](document/desktop-app-and-notes.md)
- [document/seed-content.md](document/seed-content.md) ·
  [document/build-policy.md](document/build-policy.md)
- [document/roadmap.md](document/roadmap.md) · [document/release-process.md](document/release-process.md)
- [document/adr/](document/adr/) — architecture decision records
- [deploy/README.md](deploy/README.md) · [desktop/README.md](desktop/README.md)

### Version and releases

- **Latest release: v0.2.1** (`main` is preparing v0.2.2).
- Releases are tag-driven; `Cargo.toml`, `web/package.json`, the desktop
  manifests, and the tag must match. See [document/release-process.md](document/release-process.md).
- v0.2.2 adds solo+PostgreSQL, embedding-aware `auto`, and an optional
  post-hybrid judge (`retrieval.judge_after_hybrid`), plus the layered license,
  this bilingual README, and the retrieval/model documentation.
- Release assets: solo archives (Linux x86_64 / Windows x64), desktop installers
  (Windows NSIS/portable, Linux deb/AppImage/tarball), and the enterprise image
  `ghcr.io/linxira-os/linxira-knowledge-base` with compose/nginx bundle,
  `SHA256SUMS`, SPDX SBOM, and build provenance. macOS is not released.

### License and commercial licensing

Licensing is **layered** from v0.2.2 ([LICENSING.md](LICENSING.md)):

| Layer | License | What it covers |
| --- | --- | --- |
| Core | **AGPL-3.0-only** | Retrieval, answers, governance, ACL, storage, ingestion, model gateway, quotas, migrations |
| Product surface | **MIT OR Apache-2.0** | CLI, config, i18n, logging, metrics, limits, connectors, billing, summaries, web UI, desktop shell, deploy, scripts, docs, tests |

- Internal use, modification, self-hosting, and redistribution of the core are
  allowed under AGPL-3.0-only. If you offer it as a network service, users must
  be offered the Corresponding Source (AGPL §13).
- The distributed binary/container links the core, so **the combined artifact is
  AGPL-3.0-only**; the permissive layer can be extracted and used separately
  (without the core) under MIT or Apache-2.0.
- **Commercial license** for the core is required for closed-source embedding,
  OEM, white-labeling, closed-source SaaS, or otherwise avoiding AGPL source
  obligations. Contact the author via
  [GitHub Issues](https://github.com/Linxira-OS/linxira-knowledge-base/issues)
  with the `commercial-license` label (or the [organization page](https://github.com/Linxira-OS));
  terms are negotiated case by case. There is no published sales email.
- **v0.2.1 and earlier were MIT**; already-published artifacts keep their
  original terms and are not retroactively relicensed.

### Contributing

Documentation precedes code; decisions that affect cross-profile behavior need
an ADR. PRs are dual-licensed under the layer they touch (AGPL-3.0-only for the
core, MIT OR Apache-2.0 elsewhere), and core contributions additionally allow
the author to offer commercial licenses. DCO sign-off is required. See
[CONTRIBUTING.md](CONTRIBUTING.md).

---

## 简体中文

### 定位

**同一检索内核，三档 profile（配置档位）**——`solo` / `team` / `enterprise`，
覆盖从个人单机到企业多租户的 AI 知识库能力。档位差异只体现为配置与适配器装配，
不 fork 代码（[ADR 0001](document/adr/0001-one-core-three-profiles.md)）。

### 为什么分层

问题陈述是两头都不合适：

- **企业架构对个人过重。** 外部数据库、独立向量服务、SSO、多租户隔离、审计链路，
  把个人知识库的部署与运维成本推高到超过收益本身。
- **个人方案撑不起企业治理。** 纯本地文件检索没有 tenant / space / ACL / 有效期 /
  版本这些治理字段；等企业客户出现再补，代价是 schema 迁移、历史数据回填、召回
  路径分叉，以及无法回答「这条结果为什么被返回」。

因此采用 **一个内核 + 三档 profile**，并配套四个跨档约束：治理字段从第一天存在
（[ADR 0002](document/adr/0002-governance-fields-day-one.md)）、文件是事实源且索引
可重建（[ADR 0003](document/adr/0003-files-are-source-of-truth.md)）、可选能力不可用
时显式降级（[ADR 0004](document/adr/0004-explicit-degradation.md)）、人类与 Agent
共用同一 Answer 契约（[ADR 0005](document/adr/0005-single-answer-contract.md)）。

### 三档能力阶梯

| 维度 | solo（个人单机） | team（团队共享） | enterprise（企业多租户） |
| --- | --- | --- | --- |
| 部署形态 | 单机单进程，SQLite 单文件或 PostgreSQL + 本地目录，无需外部服务 | Rust 服务 + PostgreSQL（pgvector），容器化部署 | 多租户集群 + PostgreSQL，按租户隔离与配额，支持数据驻留 |
| 身份 | 本机用户；默认仅 loopback 免认证，可启用本地会话（cookie + CSRF） | OIDC（组织账号），空间成员与角色 | SSO（SAML / OIDC）+ 目录供给 + 服务账号 |
| 检索 | 默认 `auto`：embedding 已有索引向量或抽取器已有 events 时走 `hybrid`，否则 `bm25`；judge 仅重排 Top 30，也可在 hybrid RRF 后重排；hybrid 为可选升级（本地 ONNX 嵌入 + sqlite-vec + RRF）；无 LLM 时抽取式回答 | PostgreSQL 全文检索 + pgvector hybrid，RRF 融合后重排 | 多路并行（关键词 / 向量 / 图谱 / 多模态）→ RRF → judge + Cross-Encoder + 可选 LLM 择优 |
| 治理 | 治理字段完整；ACL 取 allow-all；space、有效期与版本按记录写入；召回过滤照常执行；本地追加日志 | `space_id` 隔离 + 空间级 ACL，召回阶段过滤；集中日志 | `tenant_id` 强隔离 + 召回阶段 ACL；WORM 审计与 SIEM 导出 |
| 典型规模 | 10³ 量级文档，单人使用 | 10⁵ 量级文档，10–10² 人 | 10⁷ 量级文档，10³ 人以上 |

规模列为设计目标量级，不是实测结论。逐项差异见
[document/tier-matrix.md](document/tier-matrix.md)。

### 关键能力

- **Rust 单一二进制**（axum + sqlx），不依赖 Python 运行时；SQLite 与 PostgreSQL
  共用同一 `Store` 抽象，迁移脚本各自配套。
- **Wiki**：页面 / 版本 / 历史 / 回滚；`lkb seed` 内置中英各 12 篇产品文档，
  幂等且不覆盖用户修改。
- **检索模式**：默认 `auto`：embedding 已有索引向量或抽取器已有 events 时解析为
  `hybrid`，否则 `bm25`；显式模式为 `bm25`、`bm25_judge`、`event` /
  `event_entity`、`hybrid`；judge 仅重排 Top 30（不是召回上限），并默认在
  hybrid RRF 后执行。缺失通道以稳定 code 写入 `degraded[]`。
- **治理字段 day-one**：所有文档与切片都写 `tenant_id` / `space_id` /
  `acl_json` / `valid_from` / `valid_to` / `version` / `superseded_by`；
  召回过滤统一由 `Policy::visibility` 生成。
- **Answer 契约**：CLI / REST / MCP 共用 `answer`、`citations[]`、`confidence`、
  `conflicts[]`、`degraded[]`、`knowledge_as_of`、`mode`。
- **模型网关 / 配额 / 计费**：对外只暴露 alias，provider 端点仅 admin 可见；
  调用前 token 预留，钱包 / 账本 / 套餐 / entitlement / 支付适配器齐备。
- **认证**：loopback `none`、HttpOnly 会话 + CSRF、Agent/CLI Bearer token、
  可信反代 Header SSO（OIDC/SAML/SCIM 未实现）。
- **运维**：结构化 JSON 日志、Prometheus `/metrics`、限流与背压、审计事件、
  zh-CN/en i18n。
- **Web SPA + Tauri 桌面壳**共用同一 `web/dist`；桌面安装包覆盖 Windows 与
  Linux（macOS 不发布）。

### 检索是否需要模型

**不必须。** SQLite 上，当 embedding provider 已有索引向量或抽取器已有 events 时，
默认 `retrieval.mode = "auto"` 解析为 `hybrid`；否则保持 `bm25`。未配置 LLM 时使用
抽取式回答。可选升级如下：

| 模式 | 模型依赖 |
| --- | --- |
| `bm25` | 无 |
| `bm25_judge` | 决策 / 重排模型（仅重排 Top 30，不是召回上限） |
| `event` | 抽取器模型 |
| `hybrid` | dense 通道需要 embedding 模型；extractor 与 judge 可选 |
| `auto` | SQLite：embedding + 已有向量 → `hybrid`；否则 extractor + events → `hybrid`；否则 `bm25`；PostgreSQL：固定 `hybrid` |

SQLite 的 `auto` 先检查已配置 embedding provider 的可见向量；没有向量时写
`index_missing`，继续检查 extractor/events，最后落到 `bm25`。PostgreSQL 的 `auto`
固定解析为 `hybrid`；缺 embedding 时 dense 通道降级，关键词检索仍可用。

模型缺失时写入 `degraded[]` 并回退关键词，不阻塞检索。完整说明与推荐配置见
[document/retrieval-and-models.md](document/retrieval-and-models.md)。

### 快速开始

前提：Rust stable（>= 1.82）与可用的 C 编译器/链接器。bundled SQLite 仍需要 C
工具链。Windows 需安装 MSVC Build Tools + Windows SDK，或使用 MinGW。构建前请遵守
[document/build-policy.md](document/build-policy.md)。

```bash
# Windows：已验证的 GNU 工具链路径（安装 MSVC Build Tools 后可用默认 toolchain）
cargo +stable-x86_64-pc-windows-gnu build --release

# solo：SQLite、auth=none、只绑定 127.0.0.1
target/release/lkb config check
target/release/lkb import examples
target/release/lkb seed
target/release/lkb search "产品概览" --lang zh
target/release/lkb ask "没有 LLM 时回答如何生成，引用是什么"
target/release/lkb eval examples/questions.jsonl --k 5
target/release/lkb serve          # http://127.0.0.1:7331/
```

- Windows 二进制为 `target\release\lkb.exe`；PowerShell 用
  `$env:LKB_SQLITE_PATH = "..."` 设置环境变量。
- 桌面端：Windows 安装 `lkb-solo-v0.2.1-windows-x64-setup.exe`（NSIS）或
  portable zip；Linux 使用 `.deb`、`.AppImage` 或 portable tar.gz。安装包未签名，
  Windows 可能出现 SmartScreen 提示。
- 默认路径（可用 `LKB_CONFIG_DIR` / `LKB_DATA_DIR` / `LKB_LOG_DIR` 覆盖）：

| 平台 | 配置 | 数据 | 日志 |
| --- | --- | --- | --- |
| Windows | `%APPDATA%\lkb\lkb.toml` | `%LOCALAPPDATA%\lkb\data` | `%LOCALAPPDATA%\lkb\logs` |
| Windows portable | `.\lkb-data\lkb.toml` | `.\lkb-data\data` | `.\lkb-data\logs` |
| Linux | `$XDG_CONFIG_HOME/lkb/lkb.toml` | `$XDG_DATA_HOME/lkb` | `$XDG_STATE_HOME/lkb/logs` |

team / enterprise：设置 `profile = "team"`、`[storage] backend = "postgres"`，
导出 `LKB_DATABASE_URL` 后运行 `lkb serve`。

- **安全要点**：solo 的 `auth.mode = "none"` 只接受回环地址；密钥只从环境变量
  读取，不写入配置或日志；team / enterprise 的 CLI 使用 Bearer token，库中只存
  SHA-256；桌面 PostgreSQL 凭据保存在 OS keychain。
- **完整教程**：[docs/zh/03-quickstart.md](docs/zh/03-quickstart.md)；配置参考：
  [docs/zh/07-configuration.md](docs/zh/07-configuration.md)；示例文件：
  [deploy/solo/lkb.toml.example](deploy/solo/lkb.toml.example)。

### 文档索引

- [docs/zh](docs/zh/00-index.md) · [docs/en](docs/en/00-index.md) — 产品使用文档，
  同时作为编译期内置种子源
- [document/architecture.md](document/architecture.md) — 内核分层、组件边界、数据流
- [document/tier-matrix.md](document/tier-matrix.md) — 三档逐项能力矩阵
- [document/implementation-status.md](document/implementation-status.md) — 实现细节、
  迁移与验证记录
- [document/retrieval-and-models.md](document/retrieval-and-models.md) — 检索模式与
  模型依赖
- [document/product-spec.md](document/product-spec.md) — 命名、路径、版本、UI、打包
  与许可证冻结
- [document/knowledge-freshness.md](document/knowledge-freshness.md) ·
  [document/oss-landscape.md](document/oss-landscape.md) ·
  [document/sag-assessment.md](document/sag-assessment.md)
- [document/model-gateway-and-billing.md](document/model-gateway-and-billing.md) ·
  [document/summaries-and-conversations.md](document/summaries-and-conversations.md)
- [document/spaces-and-permissions.md](document/spaces-and-permissions.md) ·
  [document/agent-access-and-observability.md](document/agent-access-and-observability.md)
- [document/deployment-modes-i18n-payments.md](document/deployment-modes-i18n-payments.md) ·
  [document/desktop-app-and-notes.md](document/desktop-app-and-notes.md)
- [document/seed-content.md](document/seed-content.md) ·
  [document/build-policy.md](document/build-policy.md)
- [document/roadmap.md](document/roadmap.md) · [document/release-process.md](document/release-process.md)
- [document/adr/](document/adr/) — 架构决策记录
- [deploy/README.md](deploy/README.md) · [desktop/README.md](desktop/README.md)

### 版本与发布

- **最新 release：v0.2.1**（`main` 正在准备 v0.2.2）。
- 发布由 tag 驱动；`Cargo.toml`、`web/package.json`、桌面清单与 tag 必须一致，详见
  [document/release-process.md](document/release-process.md)。
- v0.2.2 增加 solo+PostgreSQL、感知 embedding 的 `auto`、可选的 hybrid 后
  judge（`retrieval.judge_after_hybrid`），同时包含分层许可证、双语 README 与
  检索/模型文档。
- 发布资产：solo 归档（Linux x86_64 / Windows x64）、桌面安装包（Windows
  NSIS/portable、Linux deb/AppImage/tar.gz）、企业镜像
  `ghcr.io/linxira-os/linxira-knowledge-base` 及 compose/nginx bundle、
  `SHA256SUMS`、SPDX SBOM 与构建来源证明。macOS 不发布。

### 许可证与商业许可

自 v0.2.2 起采用**分层授权**（[LICENSING.md](LICENSING.md)）：

| 层 | 许可证 | 覆盖范围 |
| --- | --- | --- |
| 核心 | **AGPL-3.0-only** | 检索、回答、治理、ACL、存储、摄入、模型网关、配额、迁移 |
| 产品面 | **MIT OR Apache-2.0** | CLI、配置、i18n、日志、指标、限流、连接器、计费、摘要、Web UI、桌面壳、部署、脚本、文档、测试 |

- 核心允许内部使用、修改、自托管与再分发（须遵守 AGPL-3.0-only）；若以网络服务
  形式提供，须向用户提供 Corresponding Source（AGPL 第 13 条）。
- 对外分发的二进制/容器链接了核心，因此**组合产物整体按 AGPL-3.0-only 分发**；
  宽松层在不链接核心时可单独按 MIT 或 Apache-2.0 提取使用。
- 闭源嵌入、OEM、白标、闭源 SaaS 或规避 AGPL 源码义务，需向作者申请**核心商业
  许可**。通过 [GitHub Issues](https://github.com/Linxira-OS/linxira-knowledge-base/issues)
  加 `commercial-license` 标签联系（或[组织主页](https://github.com/Linxira-OS)），
  条款逐案协商；不设公开销售邮箱。
- **v0.2.1 及更早版本按 MIT 发布**；已发布资产保持原许可证，不追溯变更。

### 贡献约定

文档先于代码；影响跨档行为的决策必须落 ADR。提交 PR 即按所修改文件的层授权
（核心 AGPL-3.0-only，其余 MIT OR Apache-2.0），核心贡献额外允许作者提供商业许可；
需 DCO 签名。详见 [CONTRIBUTING.md](CONTRIBUTING.md)。
