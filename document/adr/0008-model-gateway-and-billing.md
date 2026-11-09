# ADR 0008：公开模型别名、供应商池与计量/配额/计费方向

- **状态**：Accepted
- **日期**：2026-11-08

## 背景

当前模型配置只有 `lkb.toml` 的 `models.*` 与 `config check`（`src/config.rs:472-475`、`src/cli.rs:98-101`），没有公开模型别名、供应商池、路由、usage、quota、wallet 或 ledger。这个形态无法支撑三件事：

- 同一个逻辑模型接入多家 API（例如 DeepSeek 的多个兼容端点），对外仍是一个稳定模型名；
- 后台按用户/空间/租户看到 token 消耗、缓存命中与成本；
- 未来商业化时按配额阻断、按 1M token 计费、预付充值。

## 决策

**引入“公开模型别名（alias）→ 供应商池（provider pool）→ 路由（route）”的网关层；计量、配额、计费作为独立账本能力，默认关闭商业化开关。**

- 对外只暴露 `model_aliases.public_id`；provider、provider_model、base_url 与内部错误仅管理员可见。Answer 的 `model` 字段是 alias，不是 provider 模型名。
- `model_aliases` / `providers` / `provider_models` / `routes` 构成网关配置；路由按租户/空间/角色访问策略、健康状态、优先级与权重选择，失败重试/切池，响应映射回 alias。
- 所有调用写 `usage_events`；token 优先取 provider 返回的 usage，缺失时估算并标注 `token_source`。按 user/space/tenant 聚合。
- `quota_policies` 支持 tenant/space/user/role 四种 scope，hard/soft，block/alert；调用前原子预留，成功提交、失败释放；超限 429 + Retry-After。
- **Agent 平面**：不把任何 agent runtime 嵌进 Rust 内核；Zeta 作为一等 Agent 客户端（OpenAPI 3.1 / MCP），PI 作为可选第二客户端。Agent 的 provider 配置与 usage 通过网关/KB 统一，避免三套 token 账。
- **网关平面**：team/enterprise 用 LiteLLM Proxy（首选）/ Bifrost（备选）sidecar；solo 用 Rust 薄网关 + tokenizer，保持单二进制、零外部依赖。网关版本与许可证在选型时核实。
- **计费平面**：商业化默认外接 OpenMeter/Lago/Stripe Billing，KB 只发 usage 事件与 entitlement 快照，不手写钱包/发票/支付；`wallets` / `ledger_entries` / `plans` / `topups` / `invoices` 仅作自托管/离线适配器。商业开关默认关闭，关闭时仍记录 usage，不阻断调用。
- 缓存命中必须计入 usage 与节省统计；精确缓存键包含 tenant/space/alias/prompt hash/params/prompt_version/ACL fingerprint/index generation；语义缓存可选，且不得跨租户、不得绕过 ACL。
- 管理端通过 `/api/v1/admin/*` 与后台页面管理 providers、aliases、routes、pricing、quotas、usage、cache 与 billing engine 状态；`wallets` / `ledger` 仅在自托管适配器启用时出现。密钥只写不回显；owner/admin 才可操作。

## 备选方案与为何不选

| 方案 | 否决理由 |
| --- | --- |
| 继续只配置单个 `base_url/model` | 无法切池、无法按租户/空间限制、无法统计与计费；换供应商要改客户端 |
| 对外暴露 provider 模型名 | 客户端绑定底层供应商，切池与灰度会破坏兼容；内部拓扑泄漏 |
| 只做日志、不做 usage 表 | 无法按 user/space/tenant 聚合，无法做配额与对账；日志不可作为账本 |
| 配额用应用层计数、无预留 | 并发下会超卖；失败请求无法释放；无法解释拒绝原因 |
| 直接接支付渠道、默认开启计费 | 支付、退款、发票与合规未就绪；会阻断现有免费用户 |

## 后果

**正面**

- 对外模型名稳定，供应商可替换、可灰度、可故障切换。
- 用量、成本、缓存命中与配额可解释；未来商业化不需要重做调用路径。
- 账本双分录、幂等键与审计字段让充值/扣费/退款可对账。

**负面**

- 多一层网关配置与解析逻辑；provider 价格/能力漂移需要定期同步。
- usage token 计数在 provider 缺失 usage 时只能估算；估算与账单必须明确标注。
- 配额预留、账本与支付对账增加事务与运维复杂度。
- 密钥、日志脱敏、provider 错误映射成为新的安全边界。

## 关联文档

- [document/model-gateway-and-billing.md](../model-gateway-and-billing.md) — 数据模型、缓存、计量、配额、计费、接口与阶段
- [document/gap-assessment.md](../gap-assessment.md) — 当前能力缺口、记分卡与 P0/P1 估算
- [document/architecture.md](../architecture.md) — Answer 契约、治理与审计
- [document/adr/0007-event-entity-retrieval.md](0007-event-entity-retrieval.md) — 模型调用与 usage 的现有边界
