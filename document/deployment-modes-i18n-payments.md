# 部署/运营模式、限速、i18n 与支付（设计）

> 状态：设计基线
> 关联决策：[ADR 0012](adr/0012-deployment-modes-i18n-payments.md)
> 关联文档：[architecture.md](architecture.md)、[tier-matrix.md](tier-matrix.md)、[gap-assessment.md](gap-assessment.md)
> 范围：部署/运营模式、功能门禁、限速与最大请求、支付边界、i18n。本文只定义契约，不写实现。

## 1. 部署/运营模式

| 模式 | 定位 | 默认能力 | 默认关闭 |
| --- | --- | --- | --- |
| `solo` | 个人单机 | 本地空间、检索/问答、可选模型；宽松限速 | 注册、多租户、SSO、计费、公网 API、MCP、连接器 |
| `team` | 企业内部多用户 | 组织/空间、成员/角色、审计、配额；按租户/用户/token 限速 | 公开注册、商业计费、默认公网 API/MCP、连接器可关 |
| `enterprise_internal` | 企业内部大组织 | team + SSO/多租户/审计/WORM、内部配额 | 公开注册、商业计费、对外支付 |
| `enterprise_external` | 对外出售（SaaS） | 公开注册/租户开通、套餐、支付、发票、dunning、税务、合规 | 内部专用功能按套餐门禁 |

配置：`deployment.mode`、`deployment.commercial`、`features.*`。`profile` 仍是 solo/team/enterprise；`enterprise_internal` / `enterprise_external` 是 enterprise profile 的运营模式，domain 逻辑不按 mode 分支，只读 feature gate。

`features.*` 至少：`signup`、`billing`、`connectors`、`sso`、`multi_tenant`、`public_api`、`mcp`、`audit`、`rate_limit`、`public_share`。模式给默认值；管理员可覆盖，但启动时校验（如 `commercial=true` 必须 `billing=true`；`enterprise_external` 必须 `multi_tenant=true`）。

## 2. 功能门禁矩阵

`✔` 默认开；`—` 默认关；`△` 可配/按套餐。

| 功能 | solo | team | enterprise_internal | enterprise_external |
| --- | --- | --- | --- | --- |
| 注册 / 租户开通 | — | — | — | ✔ |
| 计费 / 支付 | — | — | — | ✔ |
| SSO / OIDC | — | △ | ✔ | ✔ |
| 多租户 | — | — | ✔ | ✔ |
| 连接器 | — | △ | △ | △ |
| MCP | — | △ | ✔ | ✔ |
| 公网 API | — | △ | ✔ | ✔ |
| 审计 | 本地 | ✔ | ✔（WORM） | ✔（WORM） |
| 限速 | ✔（宽松） | ✔ | ✔ | ✔（按套餐） |
| 公开分享 | — | △ | △ | △ |
| 商业条款/税务 | — | — | — | ✔ |

启动校验：`enterprise_external` 缺 `billing`/`multi_tenant`/`public_api` 拒绝启动；`solo` 打开 `billing` 拒绝；审计关闭仅允许 solo/team 显式风险接受。

## 3. 限速与最大请求

限流是流量，不是配额：token 配额管模型成本，storage 配额管空间内容，rate limit 管 QPS/并发/日请求。

| 维度 | 典型键 | 默认（solo / team / enterprise） |
| --- | --- | --- |
| instance | 进程/实例 | 宽松 / 中 / 高 |
| IP（未认证） | 加盐 IP | 低 / 中 / 中 |
| user | principal_id | 宽松 / 中 / 中 |
| token | token_id | 宽松 / 中 / 按套餐 |
| tenant | tenant_id | 单租户 / 中 / 按合同 |
| space | space_id | 宽松 / 中 / 中 |

指标：QPS、并发、每日请求数、token 预算；令牌桶/滑动窗口；超限 429 + `Retry-After`；队列满 503 `queue_full`。管理员可按 tenant/token 调整；限流事件写审计与指标。personal 默认宽松但仍有上限；enterprise_external 按套餐/合同。

## 4. 支付与计费

抽象 `PaymentProvider`：`create_checkout`、`verify_webhook`、`refund`、`sync_subscription`、`invoice`。国际 Stripe/PayPal；国内微信支付/支付宝；订阅或预付 credits；webhook 幂等（provider event id）；退款/拒付/对账；货币/税务/发票。

默认外接 Lago/Stripe Billing/OpenMeter 等计量计费系统；KB 只发 `usage_events` 与 entitlement 快照，不手写支付核心。租户生命周期：trial → active → past_due → suspended → cancelled；欠费停服前给宽限与通知；降级保留只读/导出；entitlement 快照定期同步，调用前读缓存。

支付核心（渠道签名、支付状态机、税务计算、发票合规）不在本系统手写；我方负责产品/套餐映射、entitlement 执行、usage 归因与审计。

## 5. i18n

- 首发 `zh-CN` / `en`；消息目录外置，key 稳定（如 `error.auth.token_expired`），禁止 UI 硬编码/字符串拼接。
- 解析顺序：请求 `Accept-Language` → 用户偏好 → 租户默认 → `en` fallback；`lang` 内容元数据与 UI locale 分开。
- 数字/日期/货币/时区用 Intl（或等价库）；复数/性别/长文本/语序用 ICU MessageFormat；禁止写死格式。
- API 错误返回稳定 `code` + 本地化 `message`；日志用英文；邮件/通知/发票本地化。
- 文档双语；内容 `lang` 已存在；预留 RTL 与 28 种语言扩展；翻译 key 不复制索引。

## 6. 阶段与估算

估算口径：1 人周 = 1 名熟悉 Rust/axum/sqlx 的工程师 5 个工作日；**估算，非承诺**。

| 阶段 | 范围 | 人周 |
| --- | --- | --- |
| P0 | deployment.mode/features、启动校验、限速维度与 429/503、i18n 框架 zh/en、错误 code | 5–8 |
| P1 | PaymentProvider 适配、套餐/试用/降级、entitlement、webhook 幂等、发票对接 | 8–12 |
| P2 | 多语言扩展、税务/经销商/RTL、dunning/对账 | 6–10 |
| **合计** |  | **19–30** |

## 7. 风险与待验证

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 模式/功能门禁分叉 | 域逻辑按 mode 分支 | 只读 feature gate；启动校验；测试矩阵 |
| 限流过严/过松 | 误拒/打满 | 分维度默认值；管理员调整；压测 |
| 支付合规 | 税务/退款/拒付 | 外接计费引擎；法务评审；对账 |
| 多语言遗漏 | 文案硬编码 | key 扫描；CI 检查；fallback |
| 货币/时区错误 | 账单争议 | Intl/整数 minor units；账单审计 |

## 8. 与现有文档的关系

- [architecture.md](architecture.md)：§2.6 运营模式/门禁、X 限流/i18n/支付边界。
- [tier-matrix.md](tier-matrix.md)：§2.10 模式/限速/支付/i18n 差异。
- [roadmap.md](roadmap.md)：P1/P2/P3/P4 交付。
- [gap-assessment.md](gap-assessment.md)：能力面与工期修订。
- [adr/0012-deployment-modes-i18n-payments.md](adr/0012-deployment-modes-i18n-payments.md)：方向决策。
