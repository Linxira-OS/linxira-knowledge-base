---
title: 部署与运维
slug: 08-deployment
lang: zh
order: 8
summary: SQLite 单机与 PostgreSQL 部署、自动迁移、备份恢复、单二进制分发、资源目标与升级边界。
---

# 部署与运维

本文命令使用 GNU/Bash 风格（Linux、macOS、Git Bash）。Windows PowerShell 中把 `./target/release/lkb` 换成 `.\target\release\lkb.exe`，把 `export NAME=value` 换成 `$env:NAME = "value"`。

## 1. 部署形态与模式

| 形态 | 组成 | 适用 |
| --- | --- | --- |
| solo | 一个进程 + 一个 SQLite 文件 | 个人、离线、单机 |
| team / enterprise | 一个进程 + 一个 PostgreSQL 数据库 | 团队共享 |
| 多租户隔离 | 每个租户一套进程 + 数据库（或 schema） | 当前唯一的租户隔离方式 |

`deployment.mode` 必须与 `profile` 匹配（`solo` → SQLite；`team`/`enterprise_internal`/`enterprise_external` → PostgreSQL）。模式提供 `[features]` 默认值与启动校验；`billing`、`audit` 与 `connectors` 已实际门禁，其余 feature（`signup`、`sso`、`multi_tenant`、`public_api`、`mcp`、`public_share`）仅配置/展示；`rate_limit` 始终生效。`enterprise_external` 还要求 `signup`、`billing`、`multi_tenant`、`public_api` 全开。

系统不依赖容器编排、消息队列、缓存服务或向量服务；`lkb` 是唯一运行时产物。

## 2. SQLite 单机

默认数据库是 `data/lkb.sqlite`，可用 `LKB_SQLITE_PATH` 或 `storage.sqlite_path` 覆盖。启动时：

1. 创建数据库文件的父目录。
2. 打开 SQLite。
3. 执行内嵌迁移。
4. solo 档确保 allow-all 空间 `s_default` 存在。
5. 若 `seed.enabled`/`seed.auto` 为真且库为空，执行内置 seed（失败只写 warning）。

运维要点：

- 一个数据库文件对应一个租户；不要让多个进程并发写同一个文件。
- 备份前先停止 `serve` 与正在运行的导入。
- 数据库文件不要放进被导入的源目录；扫描本身已跳过 `data`、`blobs`、`.lkb`、`target`、`node_modules` 与隐藏目录。
- 文件系统权限就是数据权限：solo 的边界是操作系统用户。

## 3. PostgreSQL（未验证）

1. 准备数据库与账号。
2. 设置 `storage.backend = "postgres"`。
3. 把连接串放进 `storage.url_env` 指定的变量（默认 `LKB_DATABASE_URL`）。
4. 运行任意 `lkb` 子命令；打开连接时会自动执行迁移。

```bash
export LKB_DATABASE_URL='postgres://user:pass@localhost/lkb'
./target/release/lkb config check
./target/release/lkb serve
```

**已知限制**：未设置 `LKB_TEST_PG_URL` 时集成测试打印 `SKIP postgres integration test: LKB_TEST_PG_URL is not set; refusing to pretend it passed`；本机没有实际跑过 PostgreSQL 运行路径。此前 `usage_events` 的 22 占位符 / 21 绑定缺陷已修复为 22/22，并新增静态守卫测试扫描 `src/store/postgres.rs` 的 SQL 字面量与 `.bind()` 链（140 条通过）；但实机迁移与全链路仍需在可用 PG 上验证后再用于生产。

## 4. 迁移

- SQLite 迁移位于 `migrations/sqlite/`（0001–0009），PostgreSQL 位于 `migrations/postgres/`（0001–0009），逻辑 schema 相同。
- 迁移内嵌在二进制里，打开存储时自动执行；没有单独命令。
- 迁移是前向的，没有 down migration。升级前先备份；回滚需要恢复备份并使用旧二进制。
- 已应用的迁移文件不得修改；新增变更应追加新编号。

## 5. 备份与恢复

### SQLite

```bash
# 先停止所有写入
cp data/lkb.sqlite backup/lkb-$(date +%Y%m%d).sqlite
```

恢复时把文件放回原路径并保证权限正确。需要逻辑迁移时用导出：

```bash
./target/release/lkb export --out backup/lkb.jsonl
./target/release/lkb import backup/lkb.jsonl --space s_default
```

导出记录包含 `doc_id`、路径、内容、ACL、有效期、版本与 `superseded_by`；路径与内容不变时 `doc_id` 保持一致，引用可以继续解析。导出**不包含** Page/Revision 表，但 Wiki 页面编译出的文档会以 `wiki/...` 路径出现；页面版本历史本身需要数据库级备份。

### PostgreSQL

按 PostgreSQL 常规流程使用 `pg_dump`/`pg_restore`。鉴于上面的运行路径未验证，正式依赖前请先实测导出/导入与 `usage_events` 写入。

### 恢复检查

- 计数：`imported + skipped + failed` 与源条目对齐。
- 身份：抽样比较 `doc_id` 集合。
- 检索：用固定问题集跑 `lkb eval`，与基线比较 Recall@k 与 MRR。
- 权限：用无权主体查询，确认零召回。

## 6. 单二进制与种子

发布二进制是一个文件：Windows 为 `target\release\lkb.exe`，其他平台为 `target/release/lkb`。已验证的 Windows 发布产物约 27 MB；`target/` 约 12 GB，属于构建缓存，不应随发布分发。部署只需要：二进制、可选的 `lkb.toml`、数据库 URL 与模型密钥的环境变量、可写的数据库目录或可达的 PostgreSQL。

`lkb seed` 把 24 篇随包文档写入 `space=product`，生成 Page/Revision 并派生 Document/Chunk。它幂等、不覆盖用户编辑过的页面，并把 `seed_version`、`source_manifest_hash`、`applied_at` 与计数写入 `seed_meta`；`GET /health` 会返回同一份 seed 元数据。seed 失败不会阻塞启动。

## 7. 前端部署

Rust 服务不托管 `web/dist`；SPA 是独立产物。

```bash
cd web
npm install
npm run build        # tsc --noEmit && vite build → web/dist
npm run preview      # 127.0.0.1:4173，带 /api 代理
```

部署规则：

- 用任意静态服务器托管 `web/dist`，并把 `/api` 反向代理到后端，且必须**同源**，否则 cookie 会话与 CSRF 会失效。
- 开发时 `npm run dev` 监听 `127.0.0.1:5173`，把 `/api` 代理到 `LKB_API_TARGET`（默认 `http://127.0.0.1:7331`）。
- `web/tools/preview-proxy.mjs`、`check-deployment.mjs`、`check-session-flow.mjs`、`screenshot.mjs` 是本地检查工具；截图输出到 `web/screenshots/`（14 张，覆盖深浅色下的搜索、回答、文章、编辑、历史、登录与管理页）。
- 前端没有单测/E2E；当前覆盖是这些 Node 检查脚本加后端测试集。

### 反代与 SSO Header

启用 `[sso].mode = "trusted_header"` 时，部署必须满足：

- 反向代理先完成认证，并在转发时**覆盖**（不是追加或透传）user/email/groups 三个头；客户端传入的同名头必须被丢弃。
- 应用只信任 TCP peer IP；把代理地址（IP/CIDR）加入 `sso.trusted_proxies`。非法条目会导致启动失败；列表为空时允许启动，但 `serve` 记录 WARN，所有携带 SSO 头的请求返回 401。
- 不要把应用端口直接暴露给客户端；绑定 loopback 或私网，并让防火墙只允许代理访问。应用会剥离客户端伪造的 `x-lkb-sso-authenticated`，但网络隔离仍是必需项。
- `X-Forwarded-For` 不参与 SSO 信任判断（只用于日志 IP 哈希与登录限流）；不要用它替代 peer 校验。
- 代理负责 TLS 与 `Host` 保留；失败码与排查见 [06 认证与安全](06-auth-security.md) 与 [11 常见问题与排错](11-faq.md)。

## 8. 可观测性

- `GET /health` 公开，返回 `{status, profile, backend, fts, seed}`；存储故障返回 503 `{"status":"unavailable","error":"storage_unavailable"}`。
- `GET /metrics` 公开，`text/plain; version=0.0.4`，无标签：`lkb_http_requests_total`、`lkb_http_5xx_total`、`lkb_rate_limited_total`、`lkb_queue_full_total`、`lkb_auth_failures_total`、`lkb_slow_queries_total`、`lkb_inflight_requests`。`lkb_slow_queries_total` 当前没有调用者，恒为 0。
- 日志写 **stderr**，格式为文本或 JSON（`LKB_LOG_FORMAT=json`）。JSON 字段包括 `request_id`、`trace_id`、`actor`、`token_id`（哈希前缀）、tenant/space、route/method/status、latency/db/queue、`degraded_codes`、`error_code`、`concurrency`、`ip_hash`、`user_agent`。查询文本不落日志，只记 `query_hash` 与 `query_length`。
- 审计与运行日志分离：审计保存原始 IP，运行日志保存 `ip_hash`。

## 9. 限额、配额与多实例注意

- 限流是进程内的（内存令牌桶与计数）：重启清零，多实例不共享。
- 存储配额预留由进程内互斥锁串行化，多写者同理。
- token 配额与计费账本存库，重启后一致；但 PostgreSQL 路径未验证。
- `[limits].max_queue` 已定义但未读取；503 来自并发耗尽。

## 10. 资源与规模

设计目标（非实测）：solo ≤ 2,000 文档 / 30,000 切片；team ≤ 100,000 / 2,000,000；enterprise ≤ 10,000,000 / 200,000,000。已验证规模是仓库语料加上 24 篇种子文档，SQLite 上跑过 113 个测试。solo 超过约 5 万切片或延迟持续上升时，应评估迁移到 team，而不是继续在 solo 加索引。

## 11. 升级与回滚

1. 备份数据库与配置。
2. 停止旧进程。
3. 替换二进制。
4. 启动并观察迁移日志与 `/health`（含 `seed` 块）。
5. 跑一次固定评测集。

回滚需要恢复备份并使用旧二进制，因为迁移不提供降级脚本。升级前先确认 `lkb.toml` 仍通过 `config check`；校验更严格时会直接启动失败，而不是带着错误配置运行。

## 12. 相关文档

- [06 认证与安全](06-auth-security.md)
- [07 配置参考](07-configuration.md)
- [11 常见问题与排错](11-faq.md)
