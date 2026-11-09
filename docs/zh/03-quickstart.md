---
title: 快速开始
slug: 03-quickstart
lang: zh
order: 3
summary: 从构建 lkb 到导入示例、检索、回答、评测与启动本地 Web 的最短可运行路径。
---

# 快速开始

本文命令分成 GNU 风格（Linux、macOS、Git Bash）与 Windows PowerShell 两套，选择与你终端匹配的一列即可。

## 1. 前提

- Rust stable，版本不低于 1.82，edition 2021。
- 可用的 C 编译器与链接器。SQLite 是 bundled 构建，仍然需要 C 工具链。
- Windows 上二选一：安装 MSVC Build Tools + Windows SDK，或使用 MinGW 的 GNU target。
- 运行前端需要 Node.js 与 npm（`web/` 使用 npm 与 Vite）。
- 首次构建需要联网拉取 crates；运行默认路径不联网。

本仓库验证过的组合是 `stable-x86_64-pc-windows-gnu` + MSYS2 gcc：`cargo check --all-targets`、`cargo build --release` 与 `cargo test` 全部通过（162 tests passed；PostgreSQL 集成测试因未设置 `LKB_TEST_PG_URL` 打印 SKIP）。默认 `stable-x86_64-pc-windows-msvc` 在验证机器上因 `link.exe` 解析到 Git 自带 coreutils 的 `link` 而失败；装好 MSVC 链接器后可直接用默认 toolchain。

## 2. 构建

GNU 风格：

```bash
cargo build --release
# Windows 上已验证的写法
cargo +stable-x86_64-pc-windows-gnu build --release
```

PowerShell：

```powershell
cargo build --release
cargo +stable-x86_64-pc-windows-gnu build --release
```

产物是单个可执行文件：GNU 风格为 `target/release/lkb`，Windows 为 `target\release\lkb.exe`。在 Windows 的 Git Bash 里，后续 GNU 示例中的 `./target/release/lkb` 要写成 `./target/release/lkb.exe`。发布二进制约 27 MB；`target/` 目录约 12 GB，属于构建缓存，不应随发布分发。

## 3. 检查配置

不写配置文件也能运行：不存在 `lkb.toml` 时使用默认值，即 `solo` + SQLite + `127.0.0.1:7331` + `auth.mode = "none"`。默认数据库是 `data/lkb.sqlite`。

GNU 风格：

```bash
./target/release/lkb config check
```

PowerShell：

```powershell
.\target\release\lkb.exe config check
```

`config check` 打印生效配置（数据库 URL 打码，密钥只显示 `set` / `missing` / `not configured`）。可选覆盖数据库路径：GNU 用 `export LKB_SQLITE_PATH=...`，PowerShell 用 `$env:LKB_SQLITE_PATH = "..."`。

## 4. 首次运行：内置种子

```bash
./target/release/lkb seed                 # 24 篇产品文档写入 space=product
./target/release/lkb seed --lang zh        # 只导入中文
./target/release/lkb seed --json           # 机器可读报告
```

- 默认空间来自 `[seed].space = "product"`；报告含 `imported/skipped/failed/unchanged/user_modified` 与每语言计数。
- 初次执行会创建 Page/Revision，并派生 Document/Chunk 与摘要作业；重复执行幂等。
- 用户编辑过的页面（作者不是 `system:seed`）不会被覆盖。
- solo 空库首次 `serve` 会自动 seed；失败只写 warning，不阻塞启动。`GET /health` 的 `seed` 字段显示 `seed_version`、`applied_at` 与页面/修订计数。

## 5. 导入

`import` 接受一个目录或一个 `.jsonl` 文件。目录递归扫描 `.md`、`.markdown`、`.txt`、`.html`、`.htm`；PDF 需要带 `pdf` feature 构建。隐藏文件、符号链接以及 `target`、`node_modules`、`data`、`blobs`、`.lkb` 目录会被跳过。

```bash
./target/release/lkb import examples --lang zh
./target/release/lkb import ./knowledge --space s_team --lang zh \
  --acl-json '{"mode":"allow_all","users":[],"groups":[],"version":1}'
```

输出含 `imported`、`skipped`、`failed` 与逐条原因；`--json` 输出完整报告。重复导入同一路径同一内容计入 `skipped`。team / enterprise 首次向不存在的空间导入时必须给 `--acl-json`。

## 6. 检索

```bash
./target/release/lkb search "doc_id 如何派生" --lang zh
./target/release/lkb search "doc_id 如何派生" --mode bm25_judge
./target/release/lkb search "Overview" --lang en --k 5 --json
```

`--lang` 在召回 SQL 的 WHERE 中过滤语言（`zh`/`en`，不传则不过滤）；`--mode` 可用 `bm25`、`bm25_judge`、`event`、`event_entity`、`hybrid`、`auto`；`--k` 默认 10；`--as-of` 用 RFC3339。结果逐条显示 `path:起-止`、分数与片段。

## 7. 回答

```bash
./target/release/lkb ask "没有 LLM 时回答如何生成，引用是什么" --lang zh
./target/release/lkb ask "产品概览" --limit 8 --json
```

默认没有 LLM，返回抽取式答案（每条结论来自命中片段并附 `path` 与 `doc_id`）。配置 `[models.llm]` 后会生成摘要；引用无法对齐或调用失败时回退抽取式并写 `degraded[]`。生成成功时 Answer 会多出 `model`（公开 alias）、`usage`，`cache_hit` 当前恒为 `false`。

## 8. 评测

评测集是 JSONL，一行一个问题：

```json
{"query": "doc_id 如何派生", "expected_paths": ["02-concepts.md"]}
{"query": "space 是什么", "expected_paths": ["02-concepts.md"]}
```

`expected_paths` 与 `expected_doc_ids` 至少给一个；路径相对导入根目录，比较时统一分隔符并忽略大小写。

```bash
./target/release/lkb eval examples/questions.jsonl --k 5
./target/release/lkb eval examples/questions.jsonl --k 5 --lang zh --mode event_entity
```

输出 `Recall@5`、`MRR` 与逐条结果；`--json` 输出完整报告。

## 9. 启动服务

```bash
./target/release/lkb serve
```

打开 `http://127.0.0.1:7331/`。solo 默认免认证，只监听回环地址；`auth.mode = "none"` 配非回环地址会在启动时被拒绝。`serve` 同时提供：

- HTML 页面：`/`、`/login`、`/w/{space}/{slug}` 及其编辑/历史页；
- 兼容 JSON：`/api/search`、`/api/ask`；
- 契约 REST：`/api/v1/search|answer|query`、`/api/v1/pages`、`/api/v1/spaces|groups`、`/api/v1/billing`、`/api/v1/admin`；
- 运维端点：`/health`、`/metrics`、`/openapi.json`。

JSON 登录最小流程（`curl`，跨平台）：

```bash
curl -s http://127.0.0.1:7331/api/v1/auth/csrf
curl -s -X POST http://127.0.0.1:7331/api/v1/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"change-me","csrf_token":"<csrf>"}'
curl -s http://127.0.0.1:7331/api/v1/auth/me
```

浏览器之外也可以直接用 Bearer：`curl -H 'authorization: Bearer lkb_...' http://127.0.0.1:7331/api/v1/search?q=doc_id`。

## 10. 前端 SPA

Rust 服务不托管 `web/dist`；SPA 需要单独运行或托管，并把 `/api` 反向代理到后端（保持同源，cookie 与 CSRF 才能工作）。

```bash
cd web
npm install
npm run dev        # 127.0.0.1:5173，/api 代理到 LKB_API_TARGET（默认 http://127.0.0.1:7331）
npm run build      # tsc --noEmit && vite build → web/dist
npm run preview    # 127.0.0.1:4173，同样带 /api 代理
```

`web/tools/` 提供四个检查脚本：`preview-proxy.mjs`（带代理的预览）、`check-deployment.mjs`（路由/内容检查）、`check-session-flow.mjs`（登录/CSRF/权限链路）、`screenshot.mjs`（headless 截图到 `web/screenshots/`）。前端没有单测或 E2E 测试。

## 11. team / 对外模式的工作流

最小 `lkb.toml`：

```toml
profile = "team"

[server]
bind = "0.0.0.0:7331"

[auth]
mode = "session"
cookie_secure = true

[storage]
backend = "postgres"
url_env = "LKB_DATABASE_URL"
tenant_id = "t1"
default_space = "s_default"

[deployment]
mode = "team"
```

```bash
export LKB_DATABASE_URL='postgres://user:pass@localhost/lkb'
LKB_USER_PASSWORD='change-me' ./target/release/lkb user add alice --role user
./target/release/lkb user role set alice admin
./target/release/lkb group create eng --description engineering
./target/release/lkb space create --name "Team Space" --visibility private
./target/release/lkb space list
./target/release/lkb space usage <SPACE_ID>
./target/release/lkb token create alice --kind service --scope write --expires-days 30
./target/release/lkb auth login --token lkb_...
./target/release/lkb auth status --json
./target/release/lkb features --json
./target/release/lkb models list
./target/release/lkb usage --json
./target/release/lkb quota --json
./target/release/lkb serve
```

注意：CLI 是**本机优先**的，它直接打开本地数据库；`auth login` 只把 token 写进本机配置文件（`LKB_CONFIG_FILE` 或平台默认路径，Unix 0600），不做远端校验；`lkb billing checkout` 在当前版本恒返回 `payment_provider_unavailable`。PostgreSQL 运行路径未验证，见 [11 常见问题与排错](11-faq.md)。

## 12. MCP 服务

```bash
./target/release/lkb mcp serve --token lkb_...
```

stdio JSON-RPC 2.0（`initialize`、`tools/list`、`tools/call`、`ping`）。工具集合：`kb_search`、`kb_answer`、`kb_get_page`、`kb_get_page_summary`、`kb_get_page_history`、`kb_list_spaces`（读）与 `kb_write_page`（需要 `write` scope）。管理类工具不暴露。

## 13. 下一步

- 检索模式、SSE 与摘要： [05 检索与 AI 回答](05-search-ai.md)
- 配置全部字段： [07 配置参考](07-configuration.md)
- 部署与静态前端： [08 部署与运维](08-deployment.md)
- 出错先查： [11 常见问题与排错](11-faq.md)

## 14. 相关文档

- [01 产品概览](01-overview.md)
- [02 核心概念](02-concepts.md)
- [04 内置 Wiki](04-wiki.md)
