# Repository Guidelines

面向在本仓库工作的 AI 编码助手。仓库当前状态：P0 文档已定稿，P1（Rust 实现）已落地，并通过 `stable-x86_64-pc-windows-gnu` 的 `cargo check --all-targets` / `cargo build --release` / `cargo test`（152 tests passed；PostgreSQL 集成已通过 WSL PostgreSQL 18.6 实测、不再 SKIP，详见 `document/wsl-postgres.md`；未设置 `LKB_TEST_PG_URL` 时仍打印 SKIP）。默认 MSVC 工具链仍缺可用链接器。本文只给工作所需的指针，不重复 `document/architecture.md` 的论述。

## 1. Project Overview

- **项目**：`lkb`（Layered Knowledge Base），版本 `0.2.2`（main），Rust 2021，MSRV `1.82`。许可证为分层授权：核心 `AGPL-3.0-only`，产品面 `MIT OR Apache-2.0`，核心可选商业许可（`LICENSING.md`）。一句话定位：同一检索内核，通过 solo / team / enterprise 三档 profile 提供从个人单机到企业多租户的知识库能力（`Cargo.toml`、`README.md`）。
- **仓库身份**：GitHub 仓库 `Linxira-OS/linxira-knowledge-base`（public）；crate / 二进制 / CLI 名仍为 `lkb`；根 `Cargo.toml` 的 `license = "AGPL-3.0-only"`（整包分发）；默认分支 `main`；首次提交 2026-11-09（初始导入时为 MIT，v0.2.2 起分层）。
- **三档 profile**：
  - `solo`：默认 SQLite 单文件（同一二进制也允许 PostgreSQL），单进程，默认 `auth.mode = none` 且只绑 loopback，零外部服务依赖。
  - `team`：PostgreSQL 后端，会话/Bearer 认证，共享空间。
  - `enterprise`：PostgreSQL 多租户；当前代码行为与 `team` 相同，仅名称与配置不同（多租户靠独立配置/数据库或外部路由，HTTP 层不解析租户）。
- **当前阶段**：P1 solo 闭环（导入 → 检索 → 带引用回答 → CLI/本地 Web + 最小 Wiki + 认证 + 导入导出 + 内置种子知识库）已实现并通过 GNU 工具链验证；PostgreSQL 后端同时提供，并已通过 WSL PostgreSQL 18.6 实测（152 tests，PG 集成不再 SKIP；本机凭据在 `.lkb/pg.env`，已 gitignore；无 PG 环境时仍 SKIP）。所有“能构建/能通过”的结论仍以实际命令输出为准。
- **素材现状**：`examples/docs/` 有 20 篇 Markdown，`examples/questions.jsonl` 有 20 条问题（满足 roadmap 的 P1 评测素材数量要求）。

## 2. Architecture & Data Flow

**模块边界（`src/lib.rs` 声明，`src/main.rs` 只调用库入口）**

| 模块 | 边界 |
| --- | --- |
| `config` | 配置加载、环境变量覆盖、归一化、校验、脱敏描述；唯一允许出现 profile 差异的装配层之一 |
| `governance` | `Principal` / `TokenScope` / `Policy` / `Acl` / `GovernanceFields`；生成唯一召回可见性谓词并做进程内二次校验 |
| `store` | `Store` trait + `SqliteStore` / `PostgresStore`；记录类型与迁移执行 |
| `ingest` | `Source`（文件系统 / JSONL）、`import_path`、`export_jsonl`、`doc_id`、分块、HTML 抽取 |
| `seed` | 编译期嵌入 `docs/zh / docs/en`、frontmatter 校验、Page/Revision + Document/Chunk 写入、幂等/用户修改保护、`seed_meta` |
| `access` | 租户/组/空间三层有效角色求值、`allowed_space_ids`、显式成员/组派生/owner 优先级 |
| `audit` | `audit_events` 写入与查询，best-effort 不掩盖业务响应 |
| `quota` | 空间存储配额检查/预留/释放/重算，tenant→group→space→user 层级与 `quota_overrides` |
| `logging` | `LKB_LOG_FORMAT=json / text` 事件格式化、request_id/access 日志、CLI 事件、concurrency 计数 |
| `limits` | 进程内令牌桶/并发/每日计数，429+Retry-After、503 queue_full、输入长度/top_k 校验 |
| `metrics` | `/metrics` Prometheus 低基数计数器 |
| `i18n` | Accept-Language/配置 locale 回退、稳定消息键、数字/日期/货币/复数格式化 |
| `payment` | `PaymentProvider` 接口与 `PaymentConfig`；无网关实现，未配置/不可用返回稳定 code |
| `gateway` | alias/provider/model/route 解析、访问策略、failover/熔断、OpenAI 兼容调用、usage 事件、legacy `[models.llm]` 物化 |
| `token_quota` | token/request/cost 四 scope 预留/提交/释放，hard 429、soft 告警路径 |
| `summary` | 静态摘要 job、网关生成/抽取式回退、usage_event_id、document_summaries；后台 worker |
| `providers` | `LlmProvider` / `EmbeddingProvider` / `JudgeProvider`、`ProviderSet`、OpenAI 兼容实现、测试用 fake |
| `search` | 关键词召回、可选 dense 召回、RRF 融合、可选 judge 重排 |
| `answer` | 抽取式/生成式回答、引用对齐、冲突与置信度装配 |
| `contract` | 对外输出契约：`Answer` / `Citation` / `Conflict` / `Confidence` / `Degraded` |
| `eval` | Recall@k / MRR 评测 |
| `web` | axum 路由、会话/CSRF/Origin/限流、Wiki CRUD、`/api/search`、`/api/ask` |
| `cli` | clap 子命令与 principal 解析 |
| `paths` | OS 目录规范（config/data/log/portable）与配置文件/凭据文件解析 |
| `util` / `error` | 哈希、时间、转义等工具；`StoreError` 与 HTTP `AppError` |

**主数据流**

1. `main` → `cli::run_blocking()`（自建 tokio 多线程 runtime）→ `paths::resolve_config_file`：显式 `--config` > `LKB_CONFIG_FILE` > 当前目录 `lkb.toml`（存在时）> OS 默认 `<config_dir>/lkb.toml`（`src/paths.rs`）→ `Config::load`：文件存在则解析，否则默认值（SQLite 默认 `<data_dir>/lkb.sqlite`）→ `apply_env` → `normalize` → `validate`（`src/config.rs`）。
2. `store::open(config)`：SQLite 先建父目录；PostgreSQL 从 `storage.url_env` 指向的环境变量取 URL。`open` 内部立即 `migrate()`。随后 `bootstrap_space` 仅在 solo 创建 allow-all 默认空间（`src/store/mod.rs`）。
3. `import`：`import_path` → `FilesystemSource` / `JsonlSource` → 规范化路径 `p1` → `content_hash` → `doc_id` → 分块 → 可选 embedding → `upsert_document`。同路径同内容跳过；内容变化时旧记录置 `superseded`/`superseded_by`/`valid_to`（`src/ingest/mod.rs`）。
4. `wiki` 保存：HTTP `page_save` → 认证/CSRF/可见性检查 → 新 `revision` + `save_page` → `index_wiki_document` 写入 `wiki/{space}/{lang}/{slug}.md` 文档并同步索引（`src/web.rs`）。
5. `seed`：`lkb seed` / solo 空库 `serve` 自动 seed → 解析 `docs/zh|en` frontmatter → Page/Revision（`author=system:seed`、`source_url=seed://{lang}/{slug}`）→ Document/Chunk/FTS；用户 Revision 存在则跳过。
6. `search`：`search_keyword(limit*3)`（SQLite FTS5+bm25 / PG tsvector+ts_rank_cd，失败或零行时有显式 exact fallback）→ hybrid 时取 embedding 候选做进程内余弦 → RRF 融合（k=60）→ `Policy::can_read` 二次过滤 → 可选 judge 只重排前 30 条（不改成员）→ `sort_degraded`（`src/search.rs`）。
7. `answer`：先 `search`；取前 `min(5)` 条生成抽取式答案，配置 LLM 时用严格 JSON 提示词生成并校验引用下标；对齐失败降级回抽取式；`knowledge_as_of` 取被引用文档 `valid_from` 的最小值（`src/answer.rs`）。
8. CLI 与 HTTP 都调用上述同一组库函数，接口层不持有索引句柄、不自行过滤。

**关键不变量（改动时不得破坏）**

- **无 profile 分支**：领域逻辑不得按 `solo`/`team`/`enterprise` 分支；差异只在配置与适配器/装配点（`config::normalize`/`validate`、`bootstrap_space`、Wiki ACL 默认值、导入的隐式 allow-all）。
- **治理字段 day-one**：文档与切片都写入 `tenant_id` / `space_id` / `acl_json` / `acl_principals` / `valid_from` / `valid_to` / `version` / `superseded_by`；切片复制文档字段。`lang` 同时存在于 documents / chunks / pages / revisions / events。
- **单一可见性谓词**：所有召回查询复用 `Policy::visibility_from(alias, dialect, start)`；`Policy::can_read` 仅做二次 fail-closed 校验（tenant/space/时间窗/ACL 任一不符或 ACL 解析失败即拒绝）。
- **doc_id 稳定**：`hex(sha256(规范化相对路径 + "\0" + content_hash))[..16]` = 32 位小写 hex；路径规范化 `p1`（`/` 分隔、拒绝绝对路径与 `..`、ASCII 小写、不做 NFC）。内容变化生成新 id，旧记录 `superseded_by` 指向新 id。不要另造哈希口径。
- **显式降级**：可选能力缺失/失败写入 `degraded[]`（含 expected/actual/impact/fallback），禁止静默换实现；身份与 ACL 失败 fail-closed。
- **Answer 契约**：`answer` / `citations[{doc_id,chunk_id,path,line_start,line_end,quote,source_url?}]` / `confidence` / `conflicts` / `degraded` / `knowledge_as_of` / `mode` 恒被序列化；`knowledge_as_of = null` 表示“不适用”，不是“新鲜”。生成式答案至少 1 个合法引用，否则回退抽取式。
- **fail-closed 读口**：`get_page_unfiltered`、`list_documents`、`list_revisions` 不做 ACL 过滤，只可用于越权覆盖检查/导出等明确场景，不得喂给召回或回答。

## 3. Key Directories

```text
src/                         内核；入口 main.rs/lib.rs
src/paths.rs                 OS 目录解析（config/data/log；env 覆盖与 portable）
src/store/                   Store trait（mod.rs）+ sqlite.rs + postgres.rs
src/ingest/                  mod.rs + chunker.rs + doc_id.rs + html.rs
migrations/sqlite/           0001_init.sql、0002_fts.sql、0003_events.sql、0004_lang_seed.sql、0005_roles_spaces_quota_audit.sql、0006_model_gateway.sql、0007_summaries_conversations.sql、0008_message_context.sql、0009_billing.sql、0010_connectors.sql、0011_sso.sql、0012_user_last_login.sql
migrations/postgres/         0001_init.sql、0002_search.sql、0003_events.sql、0004_lang_seed.sql、0005_roles_spaces_quota_audit.sql、0006_model_gateway.sql、0007_summaries_conversations.sql、0008_message_context.sql、0009_billing.sql、0010_connectors.sql、0011_sso.sql、0012_user_last_login.sql
tests/                       17 个集成测试文件（见 §8）
examples/docs/               20 篇示例语料
examples/questions.jsonl     20 条评测问题
document/                    内部文档（维护者/Agent）：architecture、tier-matrix、roadmap、
                             build-policy、knowledge-freshness、oss-landscape、sag-assessment、
                             product-spec、desktop-app-and-notes、adr/0001–0014、knowledge/model-ledger.yaml
docs/                        产品使用文档（zh/en 各 12 篇）；编译期作为 `lkb seed` 种子源
```

- 迁移 SQL 用 `include_str!` 内嵌并由手写 runner 执行（**不是** `sqlx::migrate!`）；新增迁移 = 新编号文件 + 在 `MIGRATIONS` 数组登记，已应用的迁移不得改写。
- 数据目录：默认 `<data_dir>/lkb.sqlite`，按 OS 规范解析（`src/paths.rs`：Windows `%LOCALAPPDATA%\lkb\data`、Linux `$XDG_DATA_HOME/lkb`、macOS `~/Library/Application Support/lkb`；`LKB_DATA_DIR` > portable（exe 旁 `portable.flag` 时 `.\lkb-data\data`）> OS 默认，`LKB_SQLITE_PATH` 仍可覆盖文件）；`data/`、`blobs/`、`.lkb/`、`target/` 与 `*.sqlite*` 已在 `.gitignore`。测试一律用 `tempfile::tempdir()`/显式路径，不得写用户目录。

## 4. Development Commands

> **构建门禁仍然有效**：执行任何 `cargo`/`rustup`/清理命令前先获得用户明确确认。当前已实际执行 `cargo +stable-x86_64-pc-windows-gnu check --all-targets`、`build --release` 与 `test`（152 passed；PG 集成已实测、不再 SKIP）；下列命令中未实际执行的项目仍不得当作已通过。

**构建门禁（强制）**

- 任何 `cargo build` / `cargo check` / `cargo test` / `cargo run` / `cargo clippy` / `cargo fmt --check` / `cargo metadata` / `cargo fetch`，以及任何 `rustup` 安装或更新，**执行前必须先获得用户明确确认**（`document/build-policy.md` §1）。
- 请求确认时必须同时报告：① 工具链与链接器检测结果（rustc/cargo 版本、target、`link.exe`/`cl.exe`/`gcc`/`clang`、MSVC/Windows SDK）；② `target/` 的体积与文件数；③ 是否建议先 `cargo clean` 及理由。
- 依赖图/feature/`Cargo.toml` 变化、工具链变化、跨模块大重构，优先建议清理 `target/` 后再构建；只改文档/迁移 SQL/测试数据可不清理但仍需确认。清理 `~/.cargo` 必须单独确认，禁止静默执行。
- 构建失败保留完整 stderr，不截断、不静默重试、不用“可能已通过”描述。**未实际执行并看到输出，不得声称构建/测试通过。**

```text
# 工具链与链接器探测（未验证）
rustc --version
cargo --version
rustc -vV
rustup show
where link.exe
where cl.exe
where gcc
where clang

# target 体积（未验证；PowerShell）
Get-ChildItem -Recurse -File target -ErrorAction SilentlyContinue |
  Measure-Object -Property Length -Sum | Select-Object Count, Sum
# Git Bash 等价（未验证）
du -sh target

# 构建（未验证；需用户确认）
cargo build --release
cargo clean
cargo build --release

# 测试（未验证；需用户确认）
cargo test
set -a; source .lkb/pg.env; set +a; cargo test --test postgres_integration   # 本机 WSL PG 18.6 凭据（gitignored；无文件时 PG 测试 SKIP）

# lint / 格式（未验证；需用户确认；仓库无 clippy.toml / rustfmt 配置）
cargo clippy --all-targets
cargo fmt --check
```

**运行与冒烟（未验证；需用户确认后执行）**

```text
target/release/lkb config check   # 打印生效配置与解析后的 config/data/log 路径
target/release/lkb config init    # 在 OS 默认位置写入最小 solo lkb.toml（不覆盖已有文件）
target/release/lkb import examples
target/release/lkb search "doc_id 如何派生"
target/release/lkb ask "没有 LLM 时回答如何生成，引用是什么"
target/release/lkb eval examples/questions.jsonl --k 5
target/release/lkb serve          # 默认 http://127.0.0.1:7331/
```

Windows 二进制为 `target\release\lkb.exe`；PowerShell 设环境变量用 `$env:LKB_SQLITE_PATH = "..."`。`lkb` 是唯一运行时产物，不调用 Python/Node。

## 5. Code Conventions & Common Patterns

- **错误处理分层**：领域/存储用 `thiserror` 的 `StoreError`（`Database`/`Serialization`/`FtsUnavailable`/`Conflict`/`Invalid`/`NotFound`）；HTTP 用 `AppError` 统一 `{error, message}` 与 4xx/5xx 映射；CLI/配置/导入边界用 `anyhow` + `Context`/`bail!`；未知存储错误不向客户端泄露细节。
- **异步**：tokio 多线程 runtime（CLI 自建）；axum 0.8；`Store`/`Source`/三组 provider 用 `#[async_trait]`。`std::sync::Mutex` 只在不跨 `.await` 的临界区使用（如登录限流）。
- **依赖注入**：核心函数签名使用 `&Arc<dyn Store>` 与 `&ProviderSet`；测试用 `ProviderSet::with_none()`。不要为测试在 `src` 里新增 fake `Store`，集成测试直接用真实 SQLite。
- **SQL**：只用运行时 API（`sqlx::query` / `query_as` + `bind` + `try_get`）。**禁止 `query!` / `query_as!` 编译期宏**；占位符由 `SqlDialect::placeholder` 生成（SQLite `?`、PG `${n}`）；可见性条件动态拼接后必须保持 bind 顺序。
- **不按 profile 分支**：需要档位差异时改配置默认值或适配器装配点；领域代码出现 `if profile == ...` 视为缺陷。
- **标识符**：一律用 `src/ingest/doc_id.rs` 的派生函数，不要就地拼接哈希；`chunk_id` 含 tenant，`revision_id` 由 tenant/space/slug/content_hash/version 派生。
- **治理字段**：新增可召回的表/记录必须带同一组字段，并接入 `Policy::visibility_from`；切片字段从文档复制，修改 ACL 需重新 upsert。
- **降级**：在 `contract.rs` 增加构造器并走 `sort_degraded`（Security → Evidence → Quality → Performance）；新增能力先定义“不可用/未启用”两种降级，不得静默回退。
- **HTML**：导入侧 `ingest::html` 只是保守文本抽取，**不是安全清洗**；展示侧用 `pulldown-cmark` + `ammonia::clean`；模板插值统一 `html_escape`。
- **时间/ACL 比较**：时间戳规范化为 RFC3339 UTC `Z` 秒精度，SQL 与进程内均按字符串比较；不要引入本地时区或毫秒。
- **注释与命名**：解释“为什么”，不要复述代码；对外 JSON 字段一律 `snake_case`。

## 6. Important Files

| 文件 | 用途 |
| --- | --- |
| `Cargo.toml` | 唯一清单：edition/MSRV、可选 `pdf` feature、依赖；无 `build.rs`、无 workspace |
| `src/lib.rs` | 模块声明与 `VERSION`；新增模块必须在此暴露 |
| `src/main.rs` | 二进制入口，仅调用 `cli::run_blocking()` |
| `src/config.rs` | 默认值、环境变量、normalize/validate、`describe()` 脱敏 |
| `src/paths.rs` | OS 目录规范（config/data/log、portable）与配置文件/凭据文件解析 |
| `src/contract.rs` | 输出契约与降级构造器；改动视为接口变更 |
| `src/governance.rs` | 可见性谓词、ACL、时间窗、二次校验 |
| `src/store/mod.rs` | `Store` trait 与记录类型；改接口要同时改两个后端 |
| `src/store/sqlite.rs` / `postgres.rs` | 两套并行 SQL 实现与迁移执行 |
| `src/ingest/mod.rs` | 导入/导出与 upsert 语义 |
| `src/search.rs` / `src/answer.rs` | 召回融合与回答装配 |
| `src/web.rs` | axum 路由与认证/CSRF/Wiki |
| `src/cli.rs` | 子命令与 principal/token 解析 |
| `src/seed.rs` | 内置种子知识库、frontmatter 校验、幂等与 `seed_meta` |
| `migrations/**` | 表结构与检索索引 |
| `tests/*.rs` | CLI、axum oneshot、SQLite、PostgreSQL 集成测试 |
| `document/build-policy.md` | 构建门禁与已知实现边界（改构建行为前必读） |
| `document/roadmap.md` | P1 出口标准（验收依据） |
| `document/product-spec.md` | 统一规范 v1：命名/路径/版本 0.2.z/UI/打包/配置；冲突时最高优先级 |
| `document/desktop-app-and-notes.md` | 桌面 APP、首启、solo+PG、笔记 vault 与发布包 |
| `document/architecture.md` / `adr/` | 设计决策；`document/tier-matrix.md` 为能力矩阵 |

## 7. Runtime & Tooling Preferences

- **Rust stable ≥ 1.82**，edition 2021，Cargo。仓库无 `rust-toolchain.toml`、无 `.cargo/`；根 `Cargo.lock` 已生成（构建产物）。
- 日志：`LKB_LOG_FORMAT=json / text`，`LKB_LOG_LEVEL`；JSON 写 stderr；`LKB_SERVICE` 可覆盖 service。
- 部署模式/门禁只在 config、路由与装配层判断；领域逻辑只调用 `Config::feature_enabled`。`deployment.mode` 与 profile 必须匹配；solo 禁止 billing；enterprise_external 必须 signup+billing+multi_tenant+public_api。
- Agent CLI 本地凭据：`<config_dir>/config.toml`（`LKB_CONFIG_DIR` 可覆盖）；`LKB_TOKEN`/`LKB_API_TOKEN`；`LKB_CONFIG_FILE` 选择主配置 `lkb.toml`。
- **SQLite bundled**：`sqlx` 0.8 的 `sqlite` feature 链接 bundled libsqlite3；不得启用 `sqlite-unbundled`。bundled 仍需要本机 C 编译器/链接器。
- **PostgreSQL 可选**：`sqlx` 的 `postgres` feature；运行时通过 `LKB_DATABASE_URL`（或 `storage.url_env` 指定的变量）连接；关系库只有 SQLite / PostgreSQL，不提供 SQL Server。
- **无 Python/Node 运行时**，无 `build.rs`；`pdf = ["dep:pdf-extract"]` 默认关闭。
- **默认不联网**：只有显式配置 LLM/embedding/judge provider 才请求模型；测试使用 fake/none provider。
- **Windows 链接器风险**：缺 MSVC + Windows SDK（`link.exe`/`cl.exe`）或 MinGW/MSVC target 混用会导致链接失败；这是环境问题，不得通过改代码绕开，也不得静默安装工具链。
- 首次构建需联网拉取 crates；`[profile.release]` 只设 `lto = "thin"`。

## 8. Testing & QA

```text
cargo test                                   # 已执行（GNU，152 passed；仍需用户确认后重跑）
cargo test --test integration_sqlite         # 已执行
cargo test --test axum_oneshot               # 已执行
cargo test --test cli                        # 已执行
cargo test --test seed_lang                  # 已执行
cargo test --test event_entity               # 已执行
cargo test --test postgres_integration       # 已执行（WSL PG 18.6 实测通过；无 LKB_TEST_PG_URL 时 SKIP）
```

**测试布局**（17 个集成测试文件 + 各模块 `#[cfg(test)]` 单测，`cargo test` 合计 152 个）：

| 文件 | 覆盖 |
| --- | --- |
| `tests/integration_sqlite.rs` | 导入→搜索→回答→导出→重导入往返；内容变化产生新 doc_id 且旧记录 `superseded_by` 前向链接 |
| `tests/axum_oneshot.rs` | `/health`、`/`、Wiki 保存与读取、`/api/search`、`/api/ask`、Origin 不匹配 403 |
| `tests/cli.rs` | `config check` 脱敏输出；`user add` / `import` / `search` / `export` / token 创建与 Bearer 搜索 |
| `tests/postgres_integration.rs` | PG 导入 + 关键词搜索；已通过 WSL PostgreSQL 18.6 实测（见 `document/wsl-postgres.md`）；未设置 `LKB_TEST_PG_URL` 时打印 SKIP |
| `tests/seed_lang.rs` | 24 篇双语种子、幂等、用户修改保护、语言过滤、自动 seed 条件、seed 失败不阻塞、seed 入队摘要作业 |
| `tests/rbac_spaces.rs` | 组/显式角色优先级、组展开 members、成员隔离、last_space_owner、scope 401/403+audit、空间 CRUD、quota 413、session CSRF |
| `tests/limits_observability.rs` | 100 并发请求、token QPS 429+Retry-After、吊销 401、超长 query/top_k 400、配额并发不 oversell、JSON 日志字段/脱敏 |
| `tests/p0f_modes_i18n_payment.rs` | mode feature 矩阵/覆盖/启动校验、feature_disabled、payment_not_configured、locale 回退/API 本地化/CLI locale、features endpoint |
| `tests/gateway_p0c.rs` | fake HTTP provider 双路由 failover、alias 隐藏、provider usage、usage_events、hard token quota 429、alias unavailable、admin providers redaction/quota creation |
| `tests/admin_management.rs` | 用户创建/409/重置密码/软禁用/last_login、plans CRUD+plan_in_use、subscriptions、provider-model PATCH/DELETE+provider test、overview 聚合、audit 详情/过滤/cursor |
| `tests/sso_trusted_header.rs` | 可信 peer 自动建用户/组角色映射/会话/重复幂等、不可信 peer/缺 user/禁用/域名/auto_provision off/伪造内部头拒绝、审计、严格启动校验/空 proxies 警告与 401 |
| `tests/connectors.rs` | filesystem_export 同步/深链/ACL/幂等、http_json fake server、stub not_configured、external_refs/jobs API |
| `tests/pg_sql_guard.rs` | Postgres 字面量 SQL 与 `.bind()` 链静态守卫（140 条）+ 自检用例 |
| `tests/p2a_billing.rs` | 钱包/双分录/幂等/余额不足、并发不超扣、套餐状态机+entitlement、Stripe 签名验签/重放、HTTP 手工充值+webhook+退款重放、订阅 HTTP、外部引擎不本地扣费（含 judge）、网关 402 |
| `tests/p1a_openapi_mcp.rs` | OpenAPI 路径/schema 校验、MCP tools 列表与 kb_search/kb_list_spaces/scope 拒绝、MCP stdio 子进程 smoke、admin 列表/审计/usage 聚合、契约 search/answer/page JSON CRUD+If-Match 409、JSON 保存后立即可搜/更新旧内容消失/删除不可搜/恢复可搜、页面列表 cursor/过滤/越界 403、revision body、JSON 登录 CSRF/Origin/401/成功/me/logout、会话 undelete |
| `tests/summary_conversations.rs` | 上游 SSE 多 chunk 真流式、断开取消+usage cancelled、非流式回退 degraded、 fake LLM 摘要 JSON/usage source summary、有限重试后 status=failed+抽取式回退、job 幂等、query search/both、网关多轮改写、SSE 事件顺序、会话 cursor 分页/external_session 绑定、judge 网关 usage、conversation 历史/owner 403 |
| `tests/event_entity.rs` | event-entity 通道、ACL/租户/有效期过滤、抽取失败降级、hybrid RRF |

- 测试数据来自仓库 `examples/`（通过 `env!("CARGO_MANIFEST_DIR")` 定位）；临时库放在 `tempfile::tempdir()`。
- **PG SKIP 语义**：`LKB_TEST_PG_URL` 未设置或为空白时，测试打印 `SKIP postgres integration test: ... refusing to pretend it passed` 后直接 `return`；Cargo 仍记为 passed。无 PG 环境时报告必须写“未验证/SKIP”，不得写成“PostgreSQL 测试通过”；本机已通过 WSL PostgreSQL 18.6 实测（152 tests，PG 集成不再 SKIP，见 `document/wsl-postgres.md`）。设置 URL 时也只覆盖导入+搜索，并非与 SQLite 同一组用例。
- 测试默认不触网；但代码中没有“无出站请求”的断言。

**P1 出口标准（`document/roadmap.md`，验收以此为准）**：Recall@5 ≥ 0.8（20 条问题）；20/20 引用可回跳到源文件行号且文本包含答案；断网可完成全流程；删索引重建后 doc_id 集合一致且 Recall 下降 ≤ 0.05；能力未启用/不可用必须在 `degraded[]` 声明；重复导入幂等；未认证 401、缺 CSRF 403、loopback 启动保护；模型 stub/timeout/retry 生效；Wiki revision +1 且旧版可读、新版可检索；导入导出计数 20/0/0；干净目录单二进制启动；设置 `LKB_TEST_PG_URL` 时与 SQLite 跑同一组用例。

**当前测试边界**：PG 集成仅覆盖导入 + 关键词搜索（已通过 WSL PostgreSQL 18.6 实测；未设置 `LKB_TEST_PG_URL` 时仍 SKIP）；PDF 摄入未测；`lkb eval` 的 CLI 报告仅通过手工冒烟（Recall@5=1.0、MRR=0.925 on examples）；登录限流/CSRF/Origin 有单测与 axum 用例，但没有覆盖全部认证分支。

## 9. Documentation Rules

- 文档分工：`document/` 是**内部文档**（架构、ADR、路线图、构建策略，面向维护者与 Agent）；`docs/` 是**产品使用文档**（zh/en 各 12 篇），同时作为 `lkb seed` 的编译期种子源。`examples/docs/` 是示例语料，不是产品文档。
- 文档先于代码：影响跨档行为、数据模型或接口契约的改动，先更新 `document/architecture.md`、`document/tier-matrix.md` 或新增 ADR，再改代码；文档与实现不一致视为缺陷。
- 已 Accepted 的 ADR 不静默改写；被取代时标注 Superseded 并链接新 ADR。
- 不编造 star 数、版本、性能、覆盖率等未测量数据；不确定写“未确认”，并给出来源路径。
