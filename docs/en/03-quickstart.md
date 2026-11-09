---
title: Quickstart
slug: 03-quickstart
lang: en
order: 3
summary: The shortest working path from building lkb through importing samples, searching, asking, evaluating, and serving the local web UI.
---

# Quickstart

Commands are shown in GNU style (Linux, macOS, Git Bash) and Windows PowerShell; use the column that matches your terminal.

## 1. Prerequisites

- Rust stable 1.82 or newer, edition 2021.
- A working C compiler and linker. SQLite is built bundled and still requires a C toolchain.
- On Windows, either install MSVC Build Tools plus the Windows SDK, or use a MinGW GNU target.
- Node.js and npm for the frontend (`web/` uses npm and Vite).
- The first build needs network access for crates; the default runtime path does not.

The verified combination in this repository is `stable-x86_64-pc-windows-gnu` with MSYS2 gcc: `cargo check --all-targets`, `cargo build --release`, and `cargo test` all pass (152 tests passed; the PostgreSQL integration test prints SKIP because `LKB_TEST_PG_URL` is unset). On the verification machine the default `stable-x86_64-pc-windows-msvc` failed because `link.exe` resolved to the coreutils `link` shipped with Git. With a real MSVC linker installed, the default toolchain works.

## 2. Build

GNU style:

```bash
cargo build --release
# verified on Windows in this repository
cargo +stable-x86_64-pc-windows-gnu build --release
```

PowerShell:

```powershell
cargo build --release
cargo +stable-x86_64-pc-windows-gnu build --release
```

The output is a single executable: `target/release/lkb` on GNU-style platforms, `target\release\lkb.exe` on Windows. In Git Bash on Windows, write `./target/release/lkb.exe` wherever the GNU examples use `./target/release/lkb`. The release binary is about 27 MB; `target/` is about 12 GB of cache and must not be shipped.

## 3. Check Configuration

No configuration file is required. Without `lkb.toml`, defaults apply: `solo`, SQLite, `127.0.0.1:7331`, and `auth.mode = "none"`. The default database is `data/lkb.sqlite`.

GNU style:

```bash
./target/release/lkb config check
```

PowerShell:

```powershell
.\target\release\lkb.exe config check
```

`config check` prints the effective configuration (database URLs masked; keys shown only as `set`, `missing`, or `not configured`). Override the database path with `export LKB_SQLITE_PATH=...` on GNU or `$env:LKB_SQLITE_PATH = "..."` on PowerShell.

## 4. First Run: Built-in Seed

```bash
./target/release/lkb seed                 # 24 product documents into space=product
./target/release/lkb seed --lang zh        # Chinese only
./target/release/lkb seed --json           # machine-readable report
```

- The default space comes from `[seed].space = "product"`; the report has `imported/skipped/failed/unchanged/user_modified` plus per-language counts.
- The first run creates Page/Revision records and derives Document/Chunk plus summary jobs; repeated runs are idempotent.
- Pages edited by users (author is not `system:seed`) are never overwritten.
- An empty solo database auto-seeds on the first `serve`; failures only log warnings and never block startup. The `seed` field of `GET /health` reports `seed_version`, `applied_at`, and page/revision counts.

## 5. Import

`import` accepts a directory or a `.jsonl` file. Directories are scanned recursively for `.md`, `.markdown`, `.txt`, `.html`, and `.htm`; PDF requires a build with the `pdf` feature. Hidden files, symlinks, and the `target`, `node_modules`, `data`, `blobs`, `.lkb` directories are skipped.

```bash
./target/release/lkb import examples --lang zh
./target/release/lkb import ./knowledge --space s_team --lang zh \
  --acl-json '{"mode":"allow_all","users":[],"groups":[],"version":1}'
```

The report shows `imported`, `skipped`, `failed`, and a reason per entry; `--json` prints the full report. Re-importing the same path and content counts as `skipped`. team/enterprise must pass `--acl-json` when importing into a space that does not exist yet.

## 6. Search

```bash
./target/release/lkb search "doc_id 如何派生" --lang zh
./target/release/lkb search "doc_id 如何派生" --mode bm25_judge
./target/release/lkb search "Overview" --lang en --k 5 --json
```

`--lang` filters inside the recall SQL (`zh`/`en`; omitted means no filter); `--mode` accepts `bm25`, `bm25_judge`, `event`, `event_entity`, `hybrid`, and `auto`; `--k` defaults to 10; `--as-of` takes RFC3339. Each hit shows `path:start-end`, a score, and a snippet.

## 7. Ask

```bash
./target/release/lkb ask "没有 LLM 时回答如何生成，引用是什么" --lang zh
./target/release/lkb ask "Overview" --limit 8 --json
```

Without an LLM the answer is extractive (every statement comes from a hit with `path` and `doc_id`). With `[models.llm]` configured the system summarizes; failed citation alignment or a failed call falls back to extractive and records `degraded[]`. On success, Answer also carries `model` (the public alias) and `usage`; `cache_hit` is always `false` today.

## 8. Evaluate

The evaluation set is JSONL, one question per line:

```json
{"query": "doc_id 如何派生", "expected_paths": ["02-concepts.md"]}
{"query": "space 是什么", "expected_paths": ["02-concepts.md"]}
```

Provide at least one of `expected_paths` or `expected_doc_ids`; paths are relative to the import root, compared with normalized separators and without case.

```bash
./target/release/lkb eval examples/questions.jsonl --k 5
./target/release/lkb eval examples/questions.jsonl --k 5 --lang zh --mode event_entity
```

The output reports `Recall@5`, `MRR`, and per-question results; `--json` prints the full report.

## 9. Serve

```bash
./target/release/lkb serve
```

Open `http://127.0.0.1:7331/`. solo defaults to no authentication and loopback only; binding a non-loopback address with `auth.mode = "none"` is rejected at startup. `serve` exposes:

- HTML pages: `/`, `/login`, `/w/{space}/{slug}` plus edit/history pages;
- legacy JSON: `/api/search`, `/api/ask`;
- contract REST: `/api/v1/search|answer|query`, `/api/v1/pages`, `/api/v1/spaces|groups`, `/api/v1/billing`, `/api/v1/admin`;
- operations: `/health`, `/metrics`, `/openapi.json`.

Minimal JSON login flow (`curl`, cross-platform):

```bash
curl -s http://127.0.0.1:7331/api/v1/auth/csrf
curl -s -X POST http://127.0.0.1:7331/api/v1/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"change-me","csrf_token":"<csrf>"}'
curl -s http://127.0.0.1:7331/api/v1/auth/me
```

Outside the browser you can also use Bearer directly: `curl -H 'authorization: Bearer lkb_...' http://127.0.0.1:7331/api/v1/search?q=doc_id`.

## 10. Frontend SPA

The Rust server does not host `web/dist`; the SPA runs or is hosted separately with `/api` reverse proxied to the backend (same origin, so cookies and CSRF work).

```bash
cd web
npm install
npm run dev        # 127.0.0.1:5173, /api proxies to LKB_API_TARGET (default http://127.0.0.1:7331)
npm run build      # tsc --noEmit && vite build → web/dist
npm run preview    # 127.0.0.1:4173, also with the /api proxy
```

`web/tools/` provides four check scripts: `preview-proxy.mjs` (preview with proxy), `check-deployment.mjs` (route/content checks), `check-session-flow.mjs` (login/CSRF/permission flow), and `screenshot.mjs` (headless screenshots into `web/screenshots/`). The frontend has no unit or E2E tests.

## 11. team / External Workflow

Minimal `lkb.toml`:

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

Notes: the CLI is **local-first** and opens the local database directly; `auth login` only writes the token into the local config file (`LKB_CONFIG_FILE` or the platform default, 0600 on Unix) and performs no remote verification; `lkb billing checkout` always returns `payment_provider_unavailable` in this release. The PostgreSQL runtime path is unverified; see [11 FAQ and Troubleshooting](11-faq.md).

## 12. MCP Server

```bash
./target/release/lkb mcp serve --token lkb_...
```

stdio JSON-RPC 2.0 (`initialize`, `tools/list`, `tools/call`, `ping`). Tools: `kb_search`, `kb_answer`, `kb_get_page`, `kb_get_page_summary`, `kb_get_page_history`, `kb_list_spaces` (read), and `kb_write_page` (requires `write` scope). No admin tools are exposed.

## 13. Next Steps

- Retrieval modes, SSE, and summaries: [05 Search and AI Answers](05-search-ai.md)
- Every configuration field: [07 Configuration](07-configuration.md)
- Deployment and static frontend: [08 Deployment](08-deployment.md)
- Troubleshooting: [11 FAQ and Troubleshooting](11-faq.md)

## 14. Related Documents

- [01 Overview](01-overview.md)
- [02 Core Concepts](02-concepts.md)
- [04 Built-in Wiki](04-wiki.md)
