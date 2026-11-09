# ADR 0012：部署/运营模式、限速、i18n 与支付边界

- **状态**：Accepted
- **日期**：2026-11-08

## 背景

同一内核要服务个人、企业内部和对外出售三种运营形态；三者的注册、租户、SSO、审计、API/MCP、连接器、限速与支付能力不同。当前 profile 只有 solo/team/enterprise，没有 `deployment.mode`、feature gate、统一限速或 i18n 框架。

## 决策

**运营模式与 profile 解耦：`deployment.mode` + `deployment.commercial` + `features.*` 控制门禁；domain 逻辑只读 feature gate，不按 mode 分支。**

- 模式：solo（个人单机，无计费/注册/多租户/SSO，宽松限速）；team（企业内部多用户，审计/配额，可关连接器/公网 API/MCP/计费）；enterprise_internal（企业内大组织，SSO/多租户/审计/WORM）；enterprise_external（对外出售，注册/租户/套餐/支付/发票/dunning/税务）。
- 功能门禁：signup、billing、connectors、sso、multi_tenant、public_api、mcp、audit、rate_limit、public_share；模式默认值 + 管理员覆盖 + 启动校验。
- 限速：instance/IP/user/token/tenant/space 六维；QPS/并发/日请求/token 预算；令牌桶/滑动窗口；429+Retry-After，队列满 503；与 token/storage 配额分开；管理员可调，写审计/指标。
- 支付：抽象 PaymentProvider；国际 Stripe/PayPal，国内微信/支付宝；订阅/预付 credits；webhook 幂等、退款/拒付/对账；默认外接 Lago/Stripe Billing/OpenMeter，不手写支付核心；entitlement 快照驱动租户生命周期。
- i18n：首发 zh-CN/en，消息目录外置、key 稳定；Accept-Language→用户→租户→en；Intl 数字/日期/货币/时区；ICU 复数/性别；API 稳定 code + 本地化 message；日志英文；文档双语；预留 RTL 与 28 种语言。
- 功能门禁与限速在启动/请求入口执行；支付/发票/税务由外部计费系统负责；KB 只做 usage、entitlement 执行与审计。

## 后果

**正面**：一套内核覆盖个人/内部/对外；功能开关可审计；限速分层；i18n 可扩展。
**负面**：模式/门禁测试矩阵增大；支付合规与多语言翻译成本；限流多实例一致性复杂。

## 关联文档

- [document/deployment-modes-i18n-payments.md](../deployment-modes-i18n-payments.md)
- [document/architecture.md](../architecture.md)
- [document/tier-matrix.md](../tier-matrix.md)
- [document/gap-assessment.md](../gap-assessment.md)
- [document/roadmap.md](../roadmap.md)
