# Agent 访问、并发与可观测性（设计）

> 状态：设计基线
> 关联决策：[ADR 0011](adr/0011-agent-access-and-observability.md)
> 关联文档：[architecture.md](architecture.md)、[gap-assessment.md](gap-assessment.md)、[roadmap.md](roadmap.md)
> 范围：结构化日志、并发/背压、Agent 鉴权、配套 CLI、滥用测试、指标与告警。本文只定义契约，不写实现。

## 1. 目标与边界

| 目标 | 说明 |
| --- | --- |
| 可诊断 | 每个请求可用 request_id/trace_id 还原：路由、状态、延迟、DB/队列耗时、模型、token、降级、并发 |
| 不泄漏 | 查询正文默认只记哈希/长度；正文、密钥、令牌、Authorization 永不进日志 |
| 抗突发 | 每 token/租户/空间有并发与 QPS 上限；队列满 503、超限 429；可取消、可超时 |
| Agent 可接入 | `lkb_svc_` 服务令牌、scope、tenant/space 绑定、轮换/吊销；设备码登录；CLI/MCP 共用内核 |
| 可测试 | 50/100 并发、超 QPS、吊销、跨空间、超长输入、SQLite/PG 死锁都有验收场景 |

不在本期：mTLS/OAuth2 client credentials 默认开启、WAF、外部 SIEM 托管、分布式追踪后端选型（P2 另定）。

## 2. 结构化日志契约

### 2.1 JSON 字段

运行日志一行一个 JSON 对象，字段固定；缺失用 `null`/空数组，不用自由文本。

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `timestamp` | RFC3339 | 是 | UTC |
| `level` | enum | 是 | `debug` / `info` / `warn` / `error` |
| `service` | string | 是 | `lkb-api` / `lkb-worker` / `lkb-cli` / `lkb-mcp` |
| `request_id` | string | 是 | 请求唯一 ID |
| `trace_id` | string | 否 | 跨服务追踪；无后端时与 request_id 同值 |
| `actor_id` | string | 否 | 主体 ID；匿名请求为空 |
| `actor_type` | enum | 否 | `human` / `agent` / `service` / `anonymous` |
| `token_id` | string | 否 | 令牌公开 ID/哈希前缀；**不是明文 token** |
| `tenant` | string | 是 | tenant_id |
| `space` | string | 否 | space_id；跨空间请求写第一个/`*` |
| `route` | string | 是 | 路由模板，如 `/api/v1/query` |
| `method` | string | 是 | HTTP method |
| `status` | integer | 是 | HTTP 状态码 |
| `latency_ms` | integer | 是 | 端到端 |
| `db_ms` | integer | 否 | DB 累计耗时 |
| `queue_ms` | integer | 否 | 排队耗时 |
| `model` | string | 否 | 公开 alias |
| `tokens_in` / `tokens_out` / `tokens_cached` | integer | 否 | usage |
| `cache_hit` | boolean | 否 | Answer/网关缓存命中 |
| `degraded_codes` | array | 是 | 空数组或码表 |
| `error_code` | string | 否 | 稳定错误码，如 `rate_limited` |
| `concurrency` | integer | 是 | 该主体/租户当时并发 |
| `ip_hash` | string | 否 | 加盐哈希；不存原始 IP |
| `user_agent` | string | 否 | 截断到 256 字符 |

附加字段：`query_hash`（SHA-256）、`query_length`、`top_k`、`result_count`、`auth_result`、`decision`（模型原语）、`index_generation`。不得加 `query_text`、`response_text`、`authorization`、`api_key`、`password`、`cookie`。

### 2.2 级别、采样与保留

| 项 | 契约 |
| --- | --- |
| info | 每个请求一条摘要；写操作/管理操作 100% |
| warn | degraded、慢查询、重试、队列接近满、权限拒绝 |
| error | 5xx、认证后端失败、DB 不可用；100% |
| debug | 开发/诊断；生产默认关闭或 1–5% 采样 |
| 采样 | 成功读请求可采样；错误、写、管理、审计相关不采样 |
| 轮转 | 按大小/日期轮转（如 100MB 或 7 天，部署可调）；不写多行堆栈到主日志 |
| 保留 | solo 30 天；team 90 天；enterprise 365 天（目标，按租户策略） |
| 分离 | 运行日志与 audit_event 分开：运行日志可采样/轮转；审计不可采样，字段与保留独立 |

## 3. 并发与背压

### 3.1 存储并发

| 后端 | 契约 |
| --- | --- |
| SQLite | WAL；`busy_timeout`（目标 5s）；单写多读；写事务短；读连接池；避免长事务阻塞写 |
| PostgreSQL | 连接池上限；`statement_timeout`；`idle_in_transaction_session_timeout`；慢查询日志；死锁重试（有限次 + 抖动） |
| 通用 | 请求取消传播到 DB；超时返回 503/504；慢查询阈值可配；不在请求路径做全量重建 |

### 3.2 限流与队列

| 维度 | 契约 |
| --- | --- |
| 每 token | 并发上限 + QPS/突发；scope 越小上限越低 |
| 每 tenant | 总并发/QPS；防单租户打满 |
| 每 space | 检索/写入并发；防热点空间 |
| 队列 | 有界队列；深度接近上限时 503 `queue_full`；超 QPS 429 `rate_limited` |
| 优先级 | 管理/审计/写入 > 普通读；Agent 批量读不得饿死人类请求 |
| 多实例 | team 单实例可用内存计数；enterprise 用 Redis/PG 共享计数（近似一致），关键拒绝再落审计 |
| 取消 | 客户端断开、超时、队列淘汰都释放信号量与预留 |

### 3.3 超时与重试

- 请求总预算、检索/模型/DB 分项预算；超时写 `timeout` + 慢查询日志。
- 幂等重试只用于只读/幂等键写；非幂等写不自动重试。
- 死锁重试最多 2–3 次 + 抖动；超过返回 503。
- Agent 异常输入：超长 query（默认上限 8KB？可配）、top_k 上限、结果条数上限、枚举型分页检测；超限 400 `invalid_query`/`invalid_top_k`。

## 4. Agent 鉴权

### 4.1 服务令牌

| 项 | 契约 |
| --- | --- |
| 格式 | `lkb_svc_` + 随机串；只在创建时显示一次 |
| 存储 | 数据库只存 SHA-256、`token_id`、scope、tenant/space 绑定、expiry/revoked |
| Scope | `kb.read` / `kb.write` / `kb.admin`；admin 隐含 read/write；不默认授予 Agent admin |
| 绑定 | tenant 必填；space 列表可选；越界 403 |
| 轮换 | 新建 + 旧 token 宽限期；吊销立即 401 |
| 传输 | `Authorization: Bearer`；禁止放 URL/日志 |

### 4.2 设备码 / 浏览器批准

`lkb auth login --device`：

1. CLI 请求设备码，打印 `verification_uri` + `user_code`；
2. 用户在浏览器登录并批准 scope/tenant/space；
3. CLI 轮询换取 token；成功写 OS keyring 或 `~/.config/lkb/config.toml`（0600）；
4. 不在 shell history 中出现 token；`--token` 用于 CI，读 `LKB_API_TOKEN` 或 stdin。

失败：过期/拒绝/scope 不足返回明确错误；设备码限速、单次使用、短 TTL。

### 4.3 失败与滥用

| 场景 | 响应 |
| --- | --- |
| 无 token/格式错 | 401 `unauthorized` |
| scope 不足/space 越界 | 403 `forbidden` |
| 超 QPS/并发 | 429 `rate_limited` + Retry-After |
| 队列满 | 503 `queue_full` + Retry-After |
| 连续失败 | IP/tenant 锁定窗口，写审计；锁定返回 429/401，不泄露账号存在性 |
| 异常查询 | 超长 query/top_k 400；频繁枚举/跨空间探测 403/429 + 审计告警 |
| 吊销/过期 | 立即 401；不等缓存过期 |

可选：mTLS 或 OAuth2 client credentials，默认关闭；开启时仍映射到同一 scope/tenant/space。

## 5. 配套 CLI

与现有 `lkb` 合并，不新增第二个二进制；与 MCP server 共用同一内核。

| 命令 | 作用 |
| --- | --- |
| `lkb auth login [--device / --token]` | 设备码或直接令牌登录；写 keyring/0600 配置 |
| `lkb auth status` | 显示当前主体、tenant、space、scope、过期时间 |
| `lkb auth logout` | 删除本地凭据并吊销（可选） |
| `lkb token create/list/revoke` | 服务令牌生命周期；只显示一次明文 |
| `lkb search <query> --json` | 检索；输出 Answer/Search JSON |
| `lkb ask <query> --json` | 问答；输出 Answer 契约 |
| `lkb page list/show ...` | Wiki 页面读取 |
| `lkb mcp serve` | 启动 MCP server，使用同一认证/内核 |

配置：`~/.config/lkb/config.toml`（0600），或 `LKB_BASE_URL` / `LKB_API_TOKEN` / `LKB_TENANT` / `LKB_SPACE` 等环境变量；优先级 CLI flag > env > config。退出码：

| 码 | 含义 |
| --- | --- |
| 0 | 成功 |
| 2 | 用法/参数错误 |
| 3 | 未认证/令牌过期 |
| 4 | 无权限/空间越界 |
| 5 | 限流/配额 |
| 6 | 未找到 |
| 7 | 降级但成功（`degraded[]` 非空） |
| 1 | 其他错误 |

## 6. 暴力/滥用模拟测试

> 目标命令/场景；实现后执行。工具可用 `curl`/`xargs` 或压测工具，需在测试环境执行。

| 场景 | 命令/方法 | 期望 |
| --- | --- | --- |
| 50/100 并发查询 | 同一 token 并发 `GET /api/v1/search` 50/100 次 | 无 5xx；队列满 503 或成功；P95 可解释 |
| 单 token 超 QPS | 超过配置 QPS 连续请求 | 429 `rate_limited` + Retry-After |
| 吊销立即生效 | 吊销 token 后立刻请求 | 401；缓存不绕过 |
| 跨空间探测 | 用 A 空间 token 请求 B 空间 | 403；不泄漏标题/计数 |
| 超长 query / top_k | query > 上限、top_k > 上限 | 400 `invalid_query` / `invalid_top_k` |
| 日志字段完整 | 抓一条请求日志，检查 2.1 必填字段 | 字段齐全；无 query_text/key/token |
| 审计可追溯 | 查询 `GET /api/v1/admin/audit` | actor/tenant/space/action/result/request_id 齐全 |
| SQLite 并发写 | 多写事务并发 + busy_timeout | 无死锁；超时返回明确错误；无数据损坏 |
| PG 并发/死锁 | 并发写 + statement_timeout | 死锁重试后成功或 503；无挂起 |
| 采样/保留 | 检查日志轮转与保留 | 错误不采样；过期清理；审计不采样 |

日志检查示例（实现后）：`jq -e 'has("request_id") and has("trace_id") and has("degraded_codes")' app.jsonl`；不得出现 `query_text`、`api_key`、`authorization`。压测可用 `xargs -P`/`curl` 或团队选定工具，报告需记录并发、QPS、错误码分布与 P95。

## 7. 指标与告警

| 指标 | 类型 | 说明 |
| --- | --- | --- |
| `lkb_http_requests_total` | counter | route/method/status |
| `lkb_http_request_duration_seconds` | histogram | P50/P95/P99 |
| `lkb_db_duration_seconds` | histogram | 按操作 |
| `lkb_queue_depth` | gauge | 有界队列深度 |
| `lkb_inflight_requests` | gauge | 当前并发 |
| `lkb_rate_limited_total` | counter | 429 |
| `lkb_auth_failures_total` | counter | 401/403 |
| `lkb_tokens_total` | counter | input/output/cached |
| `lkb_cache_hits_total` | counter | 缓存命中 |
| `lkb_degraded_total` | counter | 按 code |
| `lkb_slow_queries_total` | counter | 超阈值 |

`/health` 做 liveness/readiness（DB、迁移、队列）；`/metrics` 暴露 Prometheus 文本，不暴露 query/tenant 高基数标签。告警建议（目标，可调）：5xx >5% 5m；P95 > SLO 10m；队列 >80% 5m；429/401/403 突增；慢查询 >阈值；token 预算 >80%；DB 连接池接近上限。

审计与运行日志分离：审计 `audit_event` 不采样、不可变；运行日志可采样/轮转；不得用运行日志替代审计。

## 8. 阶段与估算

估算口径：1 人周 = 1 名熟悉 Rust/axum/sqlx 的工程师 5 个工作日；**估算，非承诺**；分项与合计独立取整。

| 阶段 | 范围 | 人周 |
| --- | --- | --- |
| P0 | JSON 日志契约/脱敏、request_id、/health、基础 /metrics、服务令牌 scope、CLI auth/search/ask --json、基础并发/QPS、滥用测试 | 8–12 |
| P1 | 设备码登录、令牌轮换、CLI mcp、慢查询/死锁重试、Redis/PG 多实例限流、仪表盘 | 6–9 |
| P2 | mTLS/OAuth2 client credentials、异常检测/自动阻断、告警与审计联动 | 4–6 |
| **合计** |  | **18–27** |

## 9. 风险与待验证

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 日志泄漏 | 查询/密钥进入日志 | 字段白名单；query 只哈希/长度；日志审查测试 |
| 限流多实例不一致 | 超卖/误拒 | Redis/PG 共享计数；关键操作审计；允许近似 |
| SQLite 写锁 | 并发写超时 | WAL + busy_timeout + 短事务；压测 |
| Agent 暴力探测 | 枚举/跨空间 | per-token/tenant/space 限流；异常检测；403 不泄露 |
| 设备码被钓鱼 | token 被冒领 | 短 TTL、单次、浏览器批准、scope 最小化 |
| CLI token 落盘 | 本机泄漏 | keyring/0600；不写 shell history；logout 清理 |
| 指标高基数 | Prometheus 压力 | 不把 query/tenant 放标签；聚合标签 |

## 10. 与现有文档的关系

- [architecture.md](architecture.md)：L1 服务令牌/CLI、L7 并发、X 日志/指标/审计。
- [gap-assessment.md](gap-assessment.md)：Agent 鉴权/CLI/限流/可观测能力面与工期。
- [roadmap.md](roadmap.md)：P1/P2/P3 交付与出口。
- [adr/0011-agent-access-and-observability.md](adr/0011-agent-access-and-observability.md)：方向决策。
