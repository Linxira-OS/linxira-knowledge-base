# ADR 0011：Agent 访问、结构化日志与并发背压

- **状态**：Accepted
- **日期**：2026-11-08

## 背景

Agent 的查询模式与人类不同：并发高、可能枚举、可能尝试跨空间探测，也可能来自不可信的外部进程。当前只有基础 Bearer/会话认证、本地 tracing 与登录限流；没有服务令牌 scope 的完整执行、设备码登录、统一 CLI/MCP、查询级限流、结构化日志契约或 `/metrics`。

## 决策

**运行面独立建模：结构化日志 + 指标 + 限流/背压 + Agent 服务令牌/设备码 + 统一 CLI/MCP。**

- 日志一行一个 JSON，字段固定（request_id/trace_id/actor/token_id/tenant/space/route/status/latency/db/queue/model/tokens/cache/degraded/error/concurrency/ip_hash/user_agent）；query 只记哈希与长度，正文/密钥/Authorization 永不记录；运行日志与 audit 分离。
- 并发：SQLite WAL + busy_timeout + 短写事务；PG pool + statement_timeout；每 token/tenant/space 的并发/QPS/队列上限；429/503 + Retry-After；取消/超时/慢查询/死锁重试；多实例用 Redis/PG 共享计数。
- Agent 鉴权：`lkb_svc_` 服务令牌，scope `kb.read/write/admin`，tenant/space 绑定，过期/吊销/轮换，哈希存储，只显示一次；设备码浏览器批准；CI 用 `--token`/env；keyring/0600，不落 shell history。
- 异常防护：401/403/429、暴力锁定、IP/tenant 限流、超长 query/top_k 400、跨空间探测 403、审计告警。
- CLI 合并进现有 `lkb`：auth login/status/logout、token、search/ask/page、mcp serve；`--json`；MCP 与 REST 共用内核。
- 可观测：`/health` 与 `/metrics`（Prometheus）；慢查询、错误率、并发、队列、token、降级、限流指标；告警阈值。
- 可选 mTLS/OAuth2 client credentials，默认关闭。

## 后果

**正面**：Agent 可安全接入；突发可背压；日志可诊断且不泄漏；滥用可追溯。
**负面**：日志/指标/限流增加运维与测试面；多实例限流需要共享存储；设备码/CLI 需要额外安全评审。

## 关联文档

- [document/agent-access-and-observability.md](../agent-access-and-observability.md)
- [document/architecture.md](../architecture.md) — L1/L7/X
- [document/gap-assessment.md](../gap-assessment.md)
- [document/roadmap.md](../roadmap.md)
