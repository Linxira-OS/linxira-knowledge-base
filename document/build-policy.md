# 构建策略（Build Policy）

> 状态：生效中。本文件约束本仓库的构建、测试与缓存清理行为。
> 适用范围：所有在本仓库执行构建、测试、lint 或工具链操作的人与 Agent。
> 验证记录：首次源码交付时未执行 cargo 命令；后续用户批准后使用已安装的 `stable-x86_64-pc-windows-gnu` 完成 check/build/test。默认 MSVC 工具链因缺少可用 `link.exe`/`cl.exe` 未能链接，安装 MSVC Build Tools 需单独确认。

## 1. 总原则

1. **构建前必须获得用户明确确认。** 未确认前不得运行会触发编译、解析依赖、下载 crate 或联网的命令，包括但不限于：`cargo build`、`cargo check`、`cargo test`、`cargo run`、`cargo clippy`、`cargo fmt --check`、`cargo metadata`、`cargo fetch`，以及任何 `rustup` 安装/更新命令。
2. **用户确认时必须同时报告三件事**：
   - 工具链与链接器检测结果（rustc/cargo 版本、target、`link.exe`/`gcc`/`clang` 是否存在、MSVC / Windows SDK 是否可用）；
   - 当前 `target/` 的体积与文件数量；
   - 本次是否建议先执行 `cargo clean`，以及理由。
3. **构建失败时保留完整错误输出。** 不截断、不摘要替代原始 stderr；不得静默重试、不得用“可能已通过”描述结果。
4. **禁止未经确认清理 `~/.cargo`。** 包括 registry、git db、缓存与工具链。清理 `target/` 前必须先报告其体积并得到确认。

## 2. 增量与全量构建的选择

| 变更类型 | 建议策略 | 说明 |
| --- | --- | --- |
| 单个模块内的小改动 | 增量构建 | 直接 `cargo build`；避免无意义的全量重编译 |
| 依赖图变化（新增/升级 crate、feature 变化、`Cargo.toml` 改动） | 先报告 `target/` 体积，建议 `cargo clean` 后再构建 | 旧依赖产物可能污染定位；以用户确认后的清理为准 |
| 工具链变化（rustup 默认工具链、target、链接器） | 先报告并建议清理 | 工具链变化后增量缓存可能不可复用 |
| 跨多个 crate/模块的大规模重构 | 先报告并建议清理 | 减少增量缓存导致的“看似通过” |
| 只改文档、迁移 SQL、测试数据 | 可不清理 | 但仍需构建前确认 |

`cargo clean` 只清理仓库内 `target/`，不等于清理 `~/.cargo`。两者必须分别报告、分别确认。

## 3. 当前仓库的构建约束

- **单一二进制 + 库目标**：`src/lib.rs` 供集成测试复用，`src/main.rs` 仅调用库入口。
- **禁止 `build.rs`**：本仓库没有构建脚本；如需新增必须先获得确认。
- **禁止 sqlx 编译期宏**：不使用 `query!`、`query_as!`；所有 SQL 走运行时 API（`sqlx::query`、`query_as` + `bind`）。因此构建不需要 `DATABASE_URL`，也不会在编译期连接数据库。
- **SQLite bundled**：`sqlx` 的 `sqlite` feature 在 0.8 中链接 bundled libsqlite3；不启用 `sqlite-unbundled`。即便如此，bundled 构建仍需要本机 C 编译器/链接器（MSVC 或 MinGW）。
- **PostgreSQL**：`sqlx` 的 `postgres` feature；运行时通过 `LKB_DATABASE_URL`（或配置 `storage.url_env` 指定的变量）连接。
- **可选 PDF**：`pdf = ["dep:pdf-extract"]` 是可选 feature，默认关闭；开启会增加依赖图。PDF 文本抽取为纯 Rust 依赖，但需在构建确认时一并评估。
- **网络**：默认路径不联网。只有显式配置 LLM / embedding / judge provider 后才会发出模型请求；测试使用 fake provider，不联网。
- **迁移**：`migrations/sqlite` 与 `migrations/postgres` 在启动时按版本执行；SQLite FTS5 创建失败不会让核心 schema 失败，但检索会返回 `keyword_unavailable` 降级，而不是静默改用其他排序。

## 4. 构建前检查（在获得确认后执行）

建议按顺序记录输出，不执行任何修改：

```text
rustc --version
cargo --version
rustc -vV
rustup show
```

Windows 链接器/编译器探测：

```text
where link.exe
where cl.exe
where gcc
where clang
```

同时检查 MSVC Build Tools 与 Windows SDK 是否安装。若 `link.exe`、`cl.exe`、`gcc`、`clang` 均不存在，bundled SQLite 与 Rust 链接阶段都可能失败；这是环境问题，不应通过改代码绕开。

`target/` 体积（PowerShell）：

```text
Get-ChildItem -Recurse -File target -ErrorAction SilentlyContinue |
  Measure-Object -Property Length -Sum |
  Select-Object Count, Sum
```

或在 Git Bash：

```text
du -sh target
```

## 5. 构建与验证命令（仅在确认后）

```text
# 小改动、增量
cargo build --release

# 依赖图/工具链/大重构，且用户确认 cargo clean 后
cargo clean
cargo build --release

# 测试（同样需要确认；会触发编译）
cargo test

# 仅 PostgreSQL 集成测试需要设置测试库；未设置时测试打印 SKIP，不伪装通过
LKB_TEST_PG_URL=postgres://user:pass@localhost/lkb_test cargo test --test postgres_integration
```

发布前建议的冒烟命令：

```text
target/release/lkb config check
target/release/lkb import examples
target/release/lkb search "doc_id 如何派生"
target/release/lkb ask "没有 LLM 时回答如何生成，引用是什么"
target/release/lkb eval examples/questions.jsonl --k 5
```

## 6. 已知构建风险

| 风险 | 影响 | 处理 |
| --- | --- | --- |
| Windows 上未找到 MSVC / Windows SDK 链接器 | `link.exe` 缺失，Rust 与 bundled SQLite 均无法链接 | 由用户决定是否安装 VS Build Tools；不得静默安装 |
| MinGW 与 MSVC target 混用 | 链接 ABI 不匹配 | 确认 `rustup show` 的默认 target 与 PATH 中工具链一致 |
| bundled SQLite 需要 C 编译器 | 仅安装 rustup 不足以构建 | 构建前检查 `cl.exe`/`gcc` |
| FTS5 / trigram 不可用 | `chunks_fts` 迁移失败；检索返回 `keyword_unavailable` | 迁移逻辑已降级处理；需在目标平台验证 bundled SQLite 版本 |
| PostgreSQL `simple` 无中文分词 | 中文 FTS 可能零命中 | 已实现显式 `keyword_exact_fallback`；不是 BM25，且会写入 `degraded[]` |
| 可选 PDF 依赖图变化 | 开启 `--features pdf` 后编译时间与依赖增加 | 先确认，再决定是否清理 `target/` |
| 首次构建下载依赖 | 需要网络与 crates.io 可达 | 构建前确认网络策略；不得把下载失败当作代码失败 |
| `target/` 体积过大 | 磁盘与增量构建时间上升 | 先报告体积，再决定是否 `cargo clean` |

## 7. 验证清单（交付后由主流程执行）

- [ ] `cargo build --release` 通过，完整保留输出。
- [ ] `cargo test` 中 SQLite 单元/集成测试通过。
- [ ] 未设置 `LKB_TEST_PG_URL` 时，PostgreSQL 测试明确打印 SKIP 原因并以成功结束；设置后真实连接测试库。
- [ ] `lkb config check` 输出打码后的生效配置，且不出现 API key 明文。
- [ ] solo 默认 `auth.mode = none` 且只绑定回环地址；非回环 + `auth = none` 启动被拒绝。
- [ ] `lkb import examples` 输出成功/跳过/失败计数与原因。
- [ ] `lkb search` 与 `lkb ask` 返回引用（doc_id、chunk_id、path、line_start、line_end、quote）。
- [ ] `lkb export` 后在新 SQLite 库 `lkb import`，doc_id 集合完全一致。
- [ ] `lkb serve` 后 `/health` 返回 backend 与 FTS 状态。

## 8. 已知实现边界（交付时点）

这些边界不是待办占位，而是当前明确未实现或按更窄口径实现的行为；使用前必须知道。

1. **Unicode 规范化**：`doc_id` 的路径规范化是 `p1`：`/` 分隔、拒绝 `..` 与绝对路径、ASCII 小写。未做 Unicode NFC 归一化（仓库没有 `unicode-normalization` 依赖）。同一路径在不同 Unicode 组合形式下可能得到不同 `doc_id`。
2. **导入是增量、非同步删除**：`lkb import` 不会删除源文件已移除的文档，也不会为移动/重命名建立 `doc_alias`。移动文件会得到新 `doc_id`，旧记录保留为 active 直到人工处理。索引重建请从空的数据库导入。导入建立的是 Document/Chunk/FTS；内置 Wiki 的 page/revision 通过 HTTP 编辑或 `POST /w/{space}/{slug}` 建立，Wiki 保存时同时写入 Document（`wiki/...` 逻辑路径）。`lkb export` 迁移的是 Document/Chunk 逻辑数据与 `doc_id`，不导出 wiki page/revision 行；迁移后 Wiki 页面需要重新建立，但现有 `doc_id` 与引用仍可解析。
3. **空间成员与组目录未建模**：`tenant_id` / `space_id` / `acl_json` 字段与召回过滤已存在，但没有 space membership 表、用户组同步或 OIDC/SCIM 连接器。会话/令牌主体默认获得该租户的全部 `space_id`，可见性由文档 ACL 决定；组 ACL 目前只有数据模型，没有用户组供给。
4. **单进程单租户配置**：enterprise 的多租户通过每租户独立配置/数据库或外部路由实现，当前 HTTP 层不从请求中解析租户。
5. **hybrid 不依赖向量库**：默认无向量库；启用 embedding provider 后在进程内对已入库向量做余弦，再由 RRF 融合。没有 sqlite-vec / pgvector / ANN 索引。
6. **Jev 走 OpenAI 兼容网关**：`provider = "jev"` 与其它 provider 一样调用 `/chat/completions`（judge/LLM）或 `/embeddings`；没有 Jev 原生类型化原语客户端。
7. **PDF 行号**：启用 `pdf` feature 后，PDF 文本抽取的行号来自抽取文本，不是页码；需要页码的引用应后续扩展 `offsets`。
8. **PostgreSQL 中文检索**：主路径是 `tsvector + websearch_to_tsquery`，没有中文分词；中文查询会走显式 `keyword_exact_fallback`（`strpos` 子串），该路径不是 BM25，并写入 `degraded[]`。
9. **迁移并发**：启动迁移没有 PostgreSQL advisory lock；多实例同时首次启动时，依赖 `IF NOT EXISTS` 与唯一约束，极端情况下可能有一个实例报迁移冲突。生产部署应先单实例迁移再扩容。
10. **TLS 终止不在进程内**：`lkb serve` 不实现 TLS。非 loopback 部署必须放在 HTTPS 反向代理/负载均衡之后；否则会话 cookie 的 `Secure` 属性会使浏览器不回传 cookie，密码也会以明文 HTTP 传输。
