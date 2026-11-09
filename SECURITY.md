# 安全政策（Security Policy）

## 支持的版本

| 版本 | 支持状态 |
| --- | --- |
| 0.2.x | ✅ 当前支持；仅最新 0.2.x 接收安全修复 |
| 0.1.x 及更早 | ❌ 从未公开发布，不提供支持 |

## 报告漏洞

- 请通过 **GitHub Security Advisories 私密上报**：
  <https://github.com/Linxira-OS/linxira-knowledge-base/security/advisories/new>
- **不要**在公开 issue、PR、讨论区或社交媒体中披露漏洞细节；也不要在任何公开渠道粘贴密钥、令牌或数据库连接串。
- 报告请尽量包含：受影响版本/commit、部署档位（solo / team / enterprise）、复现步骤、影响评估、PoC（如可提供）。
- 响应目标：**72 小时内**确认收到并给出初步评估（这可能是初步分类，不承诺修复时间表）；确认后与报告者协调披露与修复时间。
- 修复发布后会在 GitHub Security Advisory（以及本仓库 `CHANGELOG.md`）中公布；不公开报告者身份，除非报告者同意。

## 安全范围

以下方向属于本项目的安全边界，欢迎上报：

- **认证与会话**：会话 cookie（HttpOnly / SameSite / 非 loopback Secure）、CSRF / Origin 校验、Bearer 令牌（仅存哈希、scope、吊销）、登录限流。
- **SSO（trusted header 模式）**：`trusted_proxies`（IP/CIDR）对端判定、内部身份头剥离、自动开通与 `group_role_map` / `allowed_email_domains` 行为、`sso.*` 审计事件。
- **ACL 与权限**：租户/组/空间三层角色求值、召回期过滤与二次 ACL 过滤（fail-closed）、空间成员/组授权、审计完整性。
- **多租户边界**：`tenant_id` / `space_id` 隔离与配额；隔离被绕过（跨租户/跨空间读取）属安全漏洞。
- **计费**：钱包/账本双分录与幂等、配额扣费与退款冲正、支付 webhook 验签（stripe HMAC-SHA256 + 时间戳容差）。
- **连接器**：`secret_env` 不落盘回显、外部 ACL 以源为准（读不到按拒绝）、同步幂等与凭据处理。
- **数据出口**：日志与审计不落正文/密钥/Authorization；`config check` 对 DSN 与密钥打码。

以下**不属于安全漏洞**（属已知边界，欢迎以 issue/PR 改进）：

- 未实现能力按设计返回 `not_implemented` / `not_configured` / `degraded[]`；
- `document/build-policy.md` §8 列出的实现边界（导入非同步删除、Unicode 规范化口径、单进程计数等）。

## 部署边界（部署方责任）

- **solo**：单机单进程，默认 `auth.mode = none` 且只绑定 loopback。不要把 `auth = none` 的服务暴露到非回环网络（启动校验会拒绝该组合）。
- **team / enterprise**：应用自身不终止 TLS。必须部署在 HTTPS 反向代理/负载均衡之后，并将应用端口限制在私网。会话 cookie 的 Secure 属性依赖 HTTPS 生效。
- **PostgreSQL**：连接凭据只经环境变量（如 `LKB_DATABASE_URL`）注入；不要提交真实凭据（`document/pg.env.example` 仅为占位符）。建议使用最小权限数据库账号并限制网络可达面。
- **SSO trusted_header**：只有 TCP 对端 IP 命中 `trusted_proxies` 的请求才会被信任；必须由可信反代注入身份头，并保证应用端口无法被旁路直连（`X-Forwarded-For` 不参与信任判定）。`trusted_proxies` 为空时，所有带 SSO 头的请求返回 401。
- **多实例**：启动迁移没有 PostgreSQL advisory lock，生产环境应先单实例完成迁移再扩容；进程内限流/配额计数不跨实例共享。

以上边界不是免责声明：如果某条边界被绕过（例如未认证访问、ACL 越权、跨租户读取、webhook 验签绕过），仍然是安全漏洞，请按上文渠道上报。
