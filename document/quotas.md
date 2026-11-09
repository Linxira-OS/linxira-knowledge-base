# 配额分层设计（solo / team / enterprise）

> 状态：设计基线（2026-11）  
> 关联：[tier-matrix.md](tier-matrix.md)、[spaces-and-permissions.md](spaces-and-permissions.md)、[model-gateway-and-billing.md](model-gateway-and-billing.md)  
> 范围：空间/存储配额与 token/请求/成本配额的分层语义、API 门禁、错误码与降级。本文中的具体数值都是**建议默认值**，不是写死的产品常量。

> Status: design baseline. Quotas are policy, not measurement; usage is always readable.
> All numeric defaults below are recommendations, not hardcoded constants.

## 1. 定位：quota、usage、rate limit、钱包是四件事

| 概念 | 回答的问题 | 数据来源 | 失败语义 |
| --- | --- | --- | --- |
| Usage（用量） | 已经用了多少？ | `storage_usage`、`usage_events`、quota counters | 只读；不阻断 |
| Quota（配额） | 允许用到多少？ | `quotas`、`token_quotas`、`quota_overrides` | `413` / `429` / `quota_exceeded` |
| Rate limit（限速） | 每秒能发多少？ | `[limits]`、token/tenant/space QPS | `429 rate_limited` |
| Wallet / entitlement（钱包/权益） | 是否已付费、套餐是否覆盖？ | billing wallet、invoice、subscription | `402 payment_required` |

- 配额不是限速：限速按时间窗口抑制突发流量，配额按存储量/周期总量限制消耗；两者可同时生效，错误码不同。
- 配额不是钱包：钱包不足走 `402 payment_required`（billing 域）；配额超限走 `413`/`429`（quota 域）。不要把 `quota_exceeded` 映射成 `402`。
- 配额页只展示 usage + effective quotas；写入配额由 `features.quotas` 门禁。关闭时用量仍可读。

> EN: quota = allowance; usage = measurement; rate limit = per-second burst control; wallet/entitlement = commercial balance. They compose but never share status codes.

## 2. 功能门禁 `features.quotas`

`Feature::Quotas` 是第 11 个功能开关，`GET /api/v1/features` 返回 `features.quotas`。

| 部署模式 | 默认 | 说明 |
| --- | --- | --- |
| solo | `false` | 默认不做硬配额；用量与配额只读可见 |
| team | `true` | 存储 + token/请求/成本配额可配置 |
| enterprise_internal | `true` | 增加 tenant/role 级与成本配额 |
| enterprise_external | `true` | 同 enterprise，另可叠加套餐/合同 |

显式 override 永远优先：solo 可显式设 `features.quotas = true`（高级用户），team/enterprise 也可关闭。门禁只在 **API 层**，配额引擎内部不判断部署模式。

被门禁的写/管理端点（关闭时返回 `403 feature_disabled`）：

| 方法 | 路径 | 作用 |
| --- | --- | --- |
| `PUT` | `/api/v1/spaces/{space_id}/quota` | 设置空间存储/文档/切片/成员配额 |
| `POST` | `/api/v1/admin/quotas` | 创建/更新 token 配额 |
| `DELETE` | `/api/v1/admin/quotas/{quota_id}` | 删除 token 配额 |
| `POST` | `/api/v1/admin/quota-overrides` | 创建带 reason/expires_at 的 override |

保持可读、不受门禁影响：

- `GET /api/v1/spaces/{space_id}/usage`：空间用量 + 生效配额快照（solo 也可看）。
- `GET /api/v1/admin/quotas`：token 配额与计数器列表。
- `GET /api/v1/features`：返回 `quotas` 布尔值，供 UI 隐藏入口。

> EN: the gate lives in the HTTP layer. Quota engine code stays mode-agnostic; reads remain available when writes are disabled.

## 3. solo：默认仅用量可见

- 默认**不启用**空间存储、文档、切片、成员、token、请求或成本的硬配额。
- `spaces.allow_user_creation` 在 solo 归一化后保持开启：用户可以创建自己的笔记空间/笔记本，组创建仍关闭。空间数量仍受 `spaces.max_per_user` 约束。
- `GET /spaces/{id}/usage` 与 `GET /admin/quotas` 可读；SPA 默认隐藏配额管理入口（依赖 `features.quotas=false`）。
- 高级用户可显式 `features.quotas=true`，再用管理令牌配置配额；这是 opt-in 的企业化能力，不改变默认零运维目标。
- `features.quotas=false` 只关闭**写入口**；配额引擎本身不读取部署模式，已有配额记录仍按数据执行。默认 solo 库没有配额行，因此等价于不启用硬配额；从 team 降级到 solo 且保留配额行时，如需停止强制应显式删除配额或设为 soft。
- 关闭配额管理入口时，已有配额记录不会被删除；重新开启后继续生效。

> EN: solo defaults to measurement-only. Notes spaces are self-service; quota management is an explicit advanced opt-in and prior rows survive a toggle.

## 4. team：存储配额 + 模型配额

### 4.1 存储维度与作用域

维度：`storage_bytes`、`documents`、`chunks`、`members`、`spaces`。

检查顺序（从大到小，全部生效）：

```
tenant → group（组拥有空间时）/ user（个人空间）→ space
```

- `space` 行由写入操作直接检查；`tenant/group/user` 按空间 owner 聚合。
- 写入前先 `reserve`（`reserved_bytes`），失败 `release`，提交后 `refresh` 重算精确值。
- 归档/软删除的空间在物理清理前继续计入；这是有意为之，避免“归档即绕过配额”。

### 4.2 hard / soft

| enforcement | 行为 |
| --- | --- |
| `hard` | 超限阻断：字节维度 `413`，计数维度 `429`；code 为 `space_quota_exceeded` / `tenant_quota_exceeded` |
| `soft` | 不阻断，仅告警；保留余量观测。80%/95% 阈值告警是设计目标，当前版本未实现独立告警任务 |

- hard/soft 只影响是否阻断；override 若存在且 `limit_value >= 0`，当前实现按 hard 生效。
- 成员/空间计数用 `429`（可重试、可调整），存储字节用 `413`（载荷过大）。
- 配额错误必须可解释：message 带 dimension、limit、used；审计记录 `quota.updated`、`quota.override`、`quota.model.updated`。

### 4.3 override

`POST /api/v1/admin/quota-overrides` 字段：`scope_type`、`scope_id`、`dimension`、`limit_value`、`reason`（必填）、`expires_at`（可选）。override 按作用域覆盖下限/上限，带 actor 与时间戳，过期后自动失效。所有 override 写审计 `quota.override`。

### 4.4 token / 请求 / 成本配额

- 作用域：`tenant`、`space`、`user`、`role`；周期：`day`、`month`、`total`。
- 维度：`limit_tokens`、`limit_requests`、`limit_cost_micros`。
- 调用模型网关前按估算值 `reserve`，调用后按实际值 `commit`，失败 `release`；硬限超限返回 `429 quota_exceeded` + `Retry-After: 60`。
- soft 超限写审计 `quota.soft_exceeded` 并继续调用。
- token 配额与空间存储配额相互独立：存储满了不阻止只读问答；token 配额满了不删除文档。

> EN: team = storage quotas at tenant/owner/space + token-request-cost quotas at tenant/space/user/role. Reservations prevent oversell inside one process; overrides always carry reason and expiry.

## 5. enterprise：更多层级与成本治理

在 team 基础上：

- **tenant 级**：合同总量（存储、文档、成员、token、请求、成本）。
- **role 级**：按 `role` 限定模型 token/请求/成本上限，例如外部协作者与内部员工不同。
- **成本维度**：`limit_cost_micros` 以微单位货币计；币种随 `currency`，用于跨模型价格差异治理。
- **多实例限制**：存储配额与 token 配额的预留/提交已落库，但 `quota::write_guard()` 是**进程内**互斥。多实例并发下两个实例可能同时通过硬限检查，造成短暂超卖。当前建议：
  - 单实例或单写实例部署优先；
  - 多实例场景用数据库 advisory lock / 原子条件更新替换进程内锁（P1）；
  - 监控 `quota_exceeded` 与 `reserved_*` 偏差，定期 `refresh` 对账。
- **审计与保留**：enterprise 不允许关闭 audit；配额变更、override、超限事件进入审计流。

> EN: enterprise adds tenant/role/cost scopes. Multi-instance quota enforcement is not fully serialized yet; document the limitation and prefer a single writer or a DB-level lock until fixed.

## 6. 建议默认值（示例，不是硬编码）

| 档位 | 空间存储 | 文档 | 切片 | 成员 | token | 请求 | 成本 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| solo | 不限（soft 提示可选） | 不限 | 不限 | n/a | 不限 | 不限 | 不限 |
| team | 10 GiB / 租户 | 100,000 / 租户 | 按文档派生 | 50 / 空间 | 10M / 月 | 按合同 | 可选 |
| enterprise | 按合同 | 按合同 | 按合同 | 按合同 | 按合同 | 按合同 | 按合同 |

数值只作为配置示例：实现通过 `SpacesConfig` 默认配额与 `quotas` 表下发，不在引擎中写常量。数值调整必须走配置/管理 API 并留下审计。

> EN: the table is a starting point for operators. Nothing is compiled in; every limit is data.

## 7. 与配额页、token 配额、钱包/权益的关系

```
features.quotas ── 控制写入口是否可用
        │
配额页 ──读──> usage（永远可读）
        │
        └──读──> effective quotas（tenant/owner/space 或 token quotas）
                  │
                  ├─ 存储/计数 → quota engine → 413/429
                  └─ token/请求/成本 → token_quota → 429
钱包/权益 ──独立──> 402 payment_required（购买/续费/权益不足）
```

- 配额页必须同时展示“已用/上限/是否 hard/soft/override 到期时间”，否则用户无法解释 `429`。
- token 配额页属于管理端；solo 默认隐藏，team/enterprise 显示。
- 钱包/权益不参与配额检查；配额超限不触发扣费，也不自动升级套餐。套餐变更只改配额数据，仍需写审计。
- rate limit 的 `429 rate_limited` 与 token 配额的 `429 quota_exceeded` 必须保持不同 code，客户端才能区分“稍后重试”与“扩容/调额”。

## 8. 降级与错误语义

| 场景 | HTTP | code | 行为 |
| --- | --- | --- | --- |
| 功能门禁关闭 | 403 | `feature_disabled` | 写/管理端点拒绝；读端点正常 |
| 存储字节超限 | 413 | `space_quota_exceeded` / `tenant_quota_exceeded` | 写入前拒绝，不产生部分数据 |
| 文档/切片/成员/空间计数超限 | 429 | `space_quota_exceeded` / `tenant_quota_exceeded` | 可调整配额后重试 |
| token/请求/成本超限 | 429 | `quota_exceeded` | 带 `Retry-After: 60`；已预留量回滚 |
| soft 超限 | 200 | — | 审计 `quota.soft_exceeded`，继续执行 |
| 配额后端不可用 | 500 | `quota_check_failed` | fail-closed：拒绝写入，避免绕过 |
| 钱包/权益不足 | 402 | `payment_required` | 进入购买/续费流程，不是配额错误 |

配额检查失败（数据库错误）默认拒绝写入，宁可失败也不超卖；读取/用量查询失败按存储错误处理，不影响既有内容。

## 9. 验收要点

1. solo 默认 `features.quotas=false`；`PUT /spaces/{id}/quota`、`POST/DELETE /admin/quotas`、`POST /admin/quota-overrides` 返回 `403 feature_disabled`。
2. solo 的 `GET /spaces/{id}/usage`、`GET /admin/quotas`、`GET /features` 可用，且 `features.quotas` 明确为 `false`。
3. team/enterprise 默认 `features.quotas=true`，同一组端点正常工作；显式 override 可覆盖任一方向。
4. solo 归一化后允许创建自己的笔记空间；`groups.allow_user_creation` 仍为 `false`；默认空间 `s_default` 可写页面。
5. 配额引擎不读取 `ProfileName` / `DeploymentMode`，门禁只在 HTTP 层。
