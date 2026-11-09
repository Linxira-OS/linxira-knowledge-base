# Admin Console 设计：Cloud Control Plane

> 状态：Proposed / 设计冻结候选。本文只定义目标与执行约束，不改 `web/` 代码、README、许可证或版本号，也不启动 `lkb serve`。
> 视觉参考：`document/pre-release-overview.html` §05–06。该页的后台 mockup 是**目标形态**，其中 `Owner/Admin/Editor/Viewer` 列必须按本文 §5 加上 scope 与 `TARGET` 标记，不能当作当前后端角色。

## 0. 结论与范围

| 项 | 决策 |
| --- | --- |
| 风格名 | **Cloud Control Plane / 云控制台风格** |
| 定位 | 企业基础设施控制台，不是 SaaS 营销后台 |
| 核心变化 | 从「11 个平级 tab + 单列 Card / 表单」改为「服务导航 + 资源表格 + 详情抽屉 + IAM 策略矩阵」 |
| 不改 | 现有 `--color-*` token、表格基础、`AdminQueryGate` / 空态 / 骨架 / 确认流程的语义 |
| 不在本文 | 后端实现、数据库迁移、角色重命名、SSO/OIDC 实现、公开分享 API |
| 交付判据 | 1568 / 1440 / 1280 无页面级横向滚动；键盘可达；zh/en；角色门禁；loading / empty / error / confirm / NotProvided 全部有明确状态 |

现有基线（必须承认，不粉饰）：

- `web/src/pages/admin/AdminPage.tsx` 是 `max-w-[900px]` 单列，`ADMIN_TABS` 有 11 个平级 tab。
- 概览、用户、角色、组、空间、模型、用量、配额、计费、连接器、审计在同一层级。
- `GroupsPanel` 的建组表单在列表底部；编辑在表格行内；成员卡片在表格下方展开。
- `RolesPanel` 是两列只读表；没有 scope、继承或策略矩阵。
- `DetailDrawer` 已存在（460 px），`ConfirmDialog`、`FormDialog`、`AdminSkeleton`、toast 已存在；可复用结构，不重造。

## 1. 风格定义

### 1.1 Cloud Control Plane

Cloud Control Plane 是**管理基础设施资源**的控制台：资源、作用域、权限、状态和审计是第一等公民。页面首先回答：

1. 当前在哪个 scope（tenant / space / group）？
2. 有哪些资源，状态如何，谁有权操作？
3. 这个操作会影响谁，是否可撤销，是否写入审计？

它不回答「产品有多好」，也不承担获客。主操作是运营动作（创建用户、调整策略、删除连接器），不是「开始试用」。

### 1.2 参考对象

| 产品 | 借鉴的交互模型 | 不复制的东西 |
| --- | --- | --- |
| AWS Console | 服务导航、资源列表、区域/账号上下文 | 过密的菜单、历史遗留视觉 |
| Google Cloud | IAM policy、scope、继承与 effective permissions | 复杂的产品矩阵 |
| Azure Portal | blade / drawer 的分层详情 | 过多 blade 叠层 |
| Cloudflare | 高密度资源表、scope 选择、清晰的状态 | 营销式 dashboard 卡片 |
| Vercel | 克制的深色表面、紧凑排版 | 面向开发者的品牌语气照搬 |

结论：**参考信息架构与交互，不复制视觉**。

### 1.3 与营销后台的区别

| 维度 | 营销后台 | Cloud Control Plane |
| --- | --- | --- |
| 首屏 | Hero、价值主张、CTA | 资源概览、异常/降级、最近审计 |
| 主操作 | “Get started” / “Upgrade” | “Create user” / “Add member” / “Rotate key” |
| 内容组织 | 卡片堆叠、插画、大留白 | 表格、筛选、scope、行详情 |
| 成功标准 | 转化 | 可发现、可解释、可审计、可回滚 |
| 颜色 | 品牌渐变、装饰色 | 中性底 + 1 px 线 + 唯一绿色状态/主操作 |
| 权限 | 通常不展示 | IAM 矩阵、继承、effective preview |

### 1.4 设计原则

1. **Resource-first**：每个页面以一个资源集合为主角；表格不是 Card 的附属内容。
2. **Scope-explicit**：任何角色/权限都带 scope；禁止裸 `Owner`、`Admin`、`Editor`、`Viewer`。
3. **Explainable permission**：权限矩阵必须说明 direct / inherited / effective，而不是只给一个勾。
4. **Honest state**：没有 API 就 `NotProvided`；没有数据就空态；降级就显示 `degraded`；不造假指标。
5. **Dense but calm**：36 px 行高、12–13 px 主体、1 px 分隔；阴影只给抽屉/对话框。
6. **One accent**：绿色只用于主操作、active、success；危险操作用 `--color-danger`，不引入第二品牌色。
7. **No hero cards**：管理页不使用营销式大卡片、渐变、插画或装饰拓扑。

### 1.5 现状诊断（为什么不像云控制台）

| 现象 | 代码事实（当前） | 结果 |
| --- | --- | --- |
| 设置页式单页 | `AdminPage` 一个文件内 11 个 tab，内容容器 `max-w-900`，顶部 PageHeader + Tabs | 像产品设置页，不像可扩展的服务控制台；宽表被窄容器挤压 |
| 无全局服务导航 | 没有左侧 nav、breadcrumb、resource route；tab 即一级导航 | 用户不知道“我在哪个服务/哪个资源”；深链只能到 tab |
| 权限组像表单 | `GroupsPanel` = 创建表单 + 组卡片 + 内联添加成员；无组详情页、成员表 enrichment、策略矩阵 | “权限组”看起来像一张设置表单，而不是 IAM 资源 |
| 卡片堆叠、层级弱 | 各 panel 以 Card 组织，页面纵向堆叠；无统一 FilterBar / ResourceTable / Drawer 模式 | 资源扫描、对比、批量操作弱；视觉层级靠卡片而非信息架构 |
| 角色词汇未分层 | 后端已有 tenant `owner/admin/user`、space `*`、group `admin/member`；UI 没有 scope-qualified label | 一旦引入 Owner/Editor/Viewer，必然歧义；必须先统一词汇 |

## 2. 信息架构

### 2.1 Shell 线框

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Topbar 56px: brand | Org / Project / Env | global search (⌘K) | help | account │
├───────────────┬──────────────────────────────────────────────────────────────┤
│ Left nav      │ Breadcrumb: Admin / Identity / Groups / engineering-platform  │
│ 240px         │ PageHeader: title · status · count · primary action          │
│               │ Tabs (resource detail only): Members / Roles & policies / …   │
│ Overview      │ Toolbar: search · filters · columns · refresh                 │
│ Identity      │ ┌───────────────────────────────────────────┐  ┌───────────┐ │
│  Users        │ │ ResourceTable                             │  │ Detail    │ │
│  Groups       │ │ 36px rows · sticky header · row actions   │  │ drawer    │ │
│  Roles        │ │                                           │  │ 480px     │ │
│  SSO          │ └───────────────────────────────────────────┘  └───────────┘ │
│ Content       │ Pagination / result count                                     │
│ Platform      │                                                              │
│ Governance    │                                                              │
└───────────────┴──────────────────────────────────────────────────────────────┘
```

### 2.2 全局顶栏

| 区域 | 内容 | 数据来源 / 约束 |
| --- | --- | --- |
| 左 | 产品标识、`Admin` 上下文 | 现有 brand；不放大 |
| Org | tenant 名称或 `tenant_id` | 当前 `principal.tenant_id`；没有名称 API 时显示 id，不伪造 |
| Project | 可选 space scope | 当前没有跨空间 project API；P0 隐藏/禁用，不显示假项目 |
| Env | `deployment.mode`（solo / team / enterprise）或部署标签 | 来自配置；不要凭空显示 `prod`/`staging` |
| 中 | 全局搜索 / command palette，`⌘K` / `Ctrl+K` | P0 只搜导航命令；P1 users/groups/spaces；P2 documents；未接入的 section 显示 `NotProvided` |
| 右 | Help、账号、角色、登出 | 现有 session；角色用 §5 的 scope-qualified label |

顶栏固定 56 px，不随内容滚动；移动端保留 brand、搜索、账号，隐藏 Org/Project/Env 的次要项。

### 2.3 左侧服务导航

一级分组固定为五组，顺序不可随意增加：

```text
OVERVIEW
  Overview

IDENTITY
  Users
  Groups
  Roles
  SSO            [NotProvided]

CONTENT
  Spaces
  Documents      [NotProvided]
  Connectors
  Models

PLATFORM
  Usage
  Quotas
  Billing

GOVERNANCE
  Audit log
```

规则：

- 一级分组标题是导航分段，不是可点击 tab。
- 当前项：`--color-accent-weak` 背景 + 2 px 绿色左条 + `--color-fg` 文本；图标继承 `currentColor`。
- `NotProvided` / `feature_disabled` 项保留可见，但在右侧显示 `NotProvided` 或 `Disabled` 小徽标；不要用空白页假装。
- 导航宽度：≥1440 为 240 px；1280–1439 为 208 px；<1100 改为 overlay drawer，内容全宽。
- 导航不承载资源操作；主操作在 PageHeader。

### 2.4 Admin 二级导航

本设计不再使用 11 个平级 tab。层级为：

1. 顶栏（全局上下文）。
2. 左侧一级分组（Overview / Identity / Content / Platform / Governance）。
3. 左侧二级资源页（Users / Groups / Roles / …）。
4. 资源详情内的 tabs（仅 Groups 等需要 Members / Roles & policies / Audit / Settings 的资源）。

即：**左侧导航就是 admin 二级导航**；详情 tabs 是第三级，不与全局 tab 混用。

### 2.5 Breadcrumb / PageHeader / ResourceTable / Drawer

| 组件 | 规格 |
| --- | --- |
| Breadcrumb | `Admin / Identity / Groups / <name>`；最后一项 `aria-current="page"`；资源名未加载时用 id，不阻塞渲染 |
| PageHeader | eyebrow（分组）、标题、状态/数量、描述、主操作（最多 1 个绿色）、次操作；按钮可换行 |
| ResourceTable | 36 px 行高；sticky header；首列可 sticky；行 hover；行点击打开 drawer；操作列 `stopPropagation`；支持选择、批量操作、分页 |
| 工具栏 | 搜索、状态/角色/scope filter、filter chips、列选择、刷新；小屏折叠为 `Filter` 按钮 + drawer |
| DetailDrawer | 480 px（现有 460 px 可先复用并逐步加宽）；右侧滑出；内部 tabs；Esc 关闭；遮罩点击关闭；焦点 trap + 返回触发元素 |
| 空态 | 说明为什么为空 + 下一步（创建/清除筛选）；不显示假数据 |
| 加载 | 复用 `AdminSkeleton`；表格用行骨架，不用全屏 spinner |
| 错误 | 复用 `ErrorState` + retry；403/feature_disabled 用 `AdminQueryGate` 的 banner |
| NotProvided | 复用 `NotProvided`；说明缺少的 API/配置入口，不提供假表单 |

### 2.6 危险操作确认

危险操作分两级：

| 级别 | 例子 | 交互 |
| --- | --- | --- |
| 普通确认 | 移除成员、禁用用户、删除连接器 | `ConfirmDialog`；说明影响；确认按钮 `danger` |
| 高影响确认 | 删除/归档 group、删除 space、租户级设置、转移所有权 | `DangerZone` + `ConfirmDialog` 输入资源名；名称完全匹配才启用；显示影响范围（成员数/空间数/最后 Owner） |

最后一名 `group_admin` 的保护优先于普通确认：当前 API 已有 `last_group_admin_protected`；UI 必须在按钮层禁用并解释，不能只等 409。

### 2.7 响应式、键盘与状态

- **1568 / 1440**：240 px nav，内容区 max 1440 px，左右 32 px padding。
- **1280**：208 px nav，24 px padding；表格可选列（MFA、Last active、Created）按优先级隐藏；表格在 `.table-wrap` 内横向滚动，**body 不出现横向滚动条**。
- **<1100**：nav overlay；drawer 宽度 `min(480px, 92vw)`；PageHeader 操作换行。
- 验收检查：`document.documentElement.scrollWidth <= window.innerWidth`；`body` 无横向滚动；“幽灵滚动条”视为失败。
- 键盘：跳过导航链接；所有导航/行操作可 Tab；drawer/dialog 焦点 trap、Esc 关闭、关闭后焦点回到触发元素；矩阵支持方向键/空格；focus ring 使用 `--color-accent` 2 px。
- i18n：所有标签走 `t()`；`zh-CN` / `en`；角色通过 §5 的 `roleLabel(scope, id)`；错误显示稳定 code + 本地化 message；禁止裸 snake_case 出现在普通 UI。
- 状态矩阵：loading / empty / error / forbidden / feature_disabled / NotProvided / degraded 都要有明确视觉，不允许“空白即正常”。

## 3. 路由与页面映射

### 3.1 新路由树

```text
/admin                                      Overview
/admin/identity/users                       Users
/admin/identity/groups                      Groups
/admin/identity/groups/:group_id            Group detail (?tab=members|policies|audit|settings)
/admin/identity/roles                       Roles（只读角色目录）
/admin/identity/sso                         NotProvided
/admin/content/spaces                       Spaces
/admin/content/documents                    NotProvided
/admin/content/connectors                   Connectors
/admin/content/models                       Models
/admin/platform/usage                       Usage
/admin/platform/quotas                      Quotas
/admin/platform/billing                     Billing
/admin/governance/audit                     Audit log
```

细节：

- `/admin/identity/groups/:group_id?tab=members` 是 group 详情；tab 默认 `members`。
- 其他资源的详情优先用 `?detail=<id>` 打开右侧 drawer，避免为每个列表过早增加专用路由；等资源详情稳定后再升级为 path segment。
- `/admin/identity/sso` 与 `/admin/content/documents` 仍渲染完整 shell、面包屑和 PageHeader，但内容是 `NotProvided`，不造假表单。
- `Roles` 是只读角色目录（scope + display + status + target 标记），不提供角色 CRUD；`/api/v1/admin/roles` 当前只读。
- `Documents` 没有 admin list API；链接到可用搜索/空间内容，而不是伪造全局文档表。

### 3.2 现有 11 tab → 新 IA

| 旧 tab | 新路径 | 复用 panel | 备注 |
| --- | --- | --- | --- |
| `overview` | `/admin` | `OverviewPanel` | 只显示真实 `/admin/overview` 数据；不做营销卡片 |
| `users` | `/admin/identity/users` | `UsersPanel` | P1 包进 ResourceTable / FilterBar |
| `roles` | `/admin/identity/roles` | `RolesPanel` | 改为 scope-qualified 角色目录；只读 |
| `groups` | `/admin/identity/groups` | `GroupsPanel` | P1 重构为列表 + 详情 drawer/route |
| `spaces` | `/admin/content/spaces` | `SpacesPanel` | P2 |
| `models` | `/admin/content/models` | `ModelsPanel` | P2 |
| `usage` | `/admin/platform/usage` | `UsagePanel` | P3 |
| `quotas` | `/admin/platform/quotas` | `QuotasPanel` | P3 |
| `billing` | `/admin/platform/billing` | `BillingPanel` | P3；feature gate |
| `connectors` | `/admin/content/connectors` | `ConnectorsPanel` | P2；feature gate |
| `audit` | `/admin/governance/audit` | `AuditPanel` | P3；feature gate |
| — | `/admin/identity/sso` | `NotProvided` | 无 admin API |
| — | `/admin/content/documents` | `NotProvided` | 无 admin list API |

现有 `NotProvidedCards` 在 11 个 tab 底部重复出现，本设计拆分为对应页面的 `NotProvided`：SSO、Documents；公开分享/租户设置保留在 Overview 的 `NotProvided` 区，不新增可点击空路由。

### 3.3 旧深链兼容

在 SPA 内做客户端 `replace` 重定向，保留 `?query` 与 `#hash`，不新增 301/服务端规则：

| 旧路径 | 新路径 |
| --- | --- |
| `/admin/overview` | `/admin` |
| `/admin/users` | `/admin/identity/users` |
| `/admin/groups` | `/admin/identity/groups` |
| `/admin/roles` | `/admin/identity/roles` |
| `/admin/spaces` | `/admin/content/spaces` |
| `/admin/connectors` | `/admin/content/connectors` |
| `/admin/models` | `/admin/content/models` |
| `/admin/usage` | `/admin/platform/usage` |
| `/admin/quotas` | `/admin/platform/quotas` |
| `/admin/billing` | `/admin/platform/billing` |
| `/admin/audit` | `/admin/governance/audit` |
| `/admin/sso` | `/admin/identity/sso` |
| `/admin/documents` | `/admin/content/documents` |
| `/admin/:unknown` | `/admin`（保持现有 fallback 行为） |

实现约束：

- 新嵌套路由必须排在 `/admin/:tab` 兼容路由之前；兼容路由只匹配旧 tab allowlist，不能把 `/admin/identity` 当成未知 tab。
- 重定向使用 `replace`，避免浏览器历史堆积。
- 外部书签、文档、监控里的旧路径必须继续可用。
- 旧 `?selected=` 等本地 UI 状态不在兼容契约内；只保证页面与列表可见。

### 3.4 路由状态契约

- 列表筛选：`?q=&status=&page=&per_page=`；刷新/分享可恢复。
- Group 详情：`?tab=members|policies|audit|settings`。
- Drawer：`?detail=<id>` 或 group 的 path segment；关闭 drawer 回到同一列表 query。
- 不在 URL 中存临时 toast、hover、focus；这些是本地状态。
- 未知 tab/资源 id：页面内 `NotProvided` 或 `ErrorState`，不静默改数据。

## 4. 权限组页面交互规格

### 4.1 组列表

| 列 | P1 可用数据 | 目标数据 |
| --- | --- | --- |
| Name | `Group.name` + `group_id` | 同左 |
| Description | `Group.description` | 同左 |
| Status | `Group.status` | 同左 |
| Members | **当前 `Group` 无 `member_count`** | 后端 `admin/groups` 返回 `member_count`；未实现前显示 `—`，不 N+1 伪造 |
| Created | `created_at` | 同左 |
| Actions | View / Edit / Delete | 行菜单；危险操作走确认 |

- 搜索：P1 客户端按 `name` / `group_id` 过滤（组数量小）；超过 200 组时再引入后端 `q`（当前 `GET /api/v1/admin/groups` 无此参数）。
- 状态筛选：`active` / `archived`；未知状态原样 + badge。
- 排序：默认 `updated_at desc`（若 API 不返回排序，保持 API 顺序）；不伪造“最近活跃”。
- 行点击打开详情；行内操作用 `stopPropagation`。
- 空态：无组显示“创建第一个组”；筛选为空显示“清除筛选”。

### 4.2 组详情 Tabs

`/admin/identity/groups/:group_id`

| Tab | 内容 | P1 状态 |
| --- | --- | --- |
| Members | 成员资源表、添加/移除/改角色 | 可用（当前 group_admin/member） |
| Roles & policies | 目标策略矩阵 + 当前角色说明 | **只读 preview**；无 group policy API |
| Audit | 组相关审计事件 | 若 `AuditFilter` 不支持 target，链接到全局 audit 并按 action 过滤 |
| Settings | 名称/描述/状态、危险区 | 可用（patch/delete） |

详情头部：组名、`group_id`、状态、成员数（若可用）、创建时间、最后更新；主操作 `Add member`，次操作 `Edit`，危险操作收进 `Settings`。

布局决策：组详情用整页 route，不用三栏或窄抽屉。Members 与 Roles & policies 分 tab；1280 宽下三栏会把策略矩阵压到 400 px 以下，只能横向滚动。

### 4.3 Members 表

当前 `GroupMember` 只有 `principal_id`、`role`、`added_by`、`created_at`。目标列与现状必须分开：

| 列 | 当前可实现 | 目标 / 后端缺口 |
| --- | --- | --- |
| Member | `principal_id` | 显示名 + email；需要 principal join |
| Type | 不可用 | human / service / agent；需要 principal type |
| Role | `group_admin` / `group_member` | 目标增加 group_owner/editor/viewer |
| Scope | 组本身 | group-owned spaces / 资源 scope；需要策略模型 |
| MFA | 不可用 | 需要身份源字段 |
| Last active | 不可用 | 需要会话/活动数据 |
| Added by / at | 可用 | 同左 |

P1 决策：先显示 `principal_id` + role + added_by + created_at；`Type/MFA/Last active` 列不渲染（或显示 `—` 并在列头 tooltip 标注“backend not provided”）。**不能用 mockup 的假姓名/邮箱/MFA 状态填充。** 等后端 enrichment 后再加列。

角色编辑：

- P1 角色选项只有 `group_admin` / `group_member`；显示为“组 Admin”/“组成员”，不显示裸值。
- 最后一名 `group_admin` 的降级/移除必须禁用；当前 API 返回 `last_group_admin_protected`，UI 提前拦截并解释。
- 添加成员需要 `principal_id`（当前 API）；P1 不做“按 email 搜索用户”的假交互，除非已有用户搜索 API。
- 移除成员走 `ConfirmDialog`；说明该成员可能仍通过 space/tenant 角色拥有权限（当前无 group policy，不能承诺“移除即失去全部访问”）。

### 4.4 Roles & policies 矩阵

矩阵是**目标 group policy**，不是当前 API：

- 列：`组 Owner (target)`、`组 Admin`、`组 Editor (target)`、`组 Viewer (target)`。
- 行：`View group`、`Edit settings`、`Manage members`、`Manage policies`、`Delete group`、`Manage group-owned spaces`、`Edit content`、`Read content`、`View audit`。
- `group_admin` 当前可用；`group_owner/editor/viewer` 标记 `TARGET`，不可写入。
- 矩阵单元格用 `✓` / `—` + `aria-label`，不能只靠颜色；表头 sticky，首列 sticky。
- `scope` 选择器 P1 只有 `Group`；目标增加 `Group-owned spaces`。**不要显示虚构的 “Engineering” scope**（evaluation mockup 中的该值只是视觉示意）。
- `inheritance` 区显示 chips：`Tenant role` / `Group role` / `Space role` / `Direct ACL`；每个 chip 可展开来源说明。
- `effective permissions` 预览：只读；按目标模型合并；标注 `direct` / `inherited`；当前后端没有 group policy 时不显示“已生效”的假勾选，改用 `TARGET` 说明。
- 保存：P1 禁用；等 group policy 持久化 API 落地后再启用 diff + 二次确认 + 审计。

### 4.5 Scope 与继承

目标计算（写入设计，不写实现）：

```text
effective(principal, resource) = union(
  tenant_role_capabilities,
  group_role_capabilities,
  space_role_capabilities,
  direct_acl
)
```

约束：

- 显式成员角色优先于组派生角色；组角色取最大值（max），不做叠加越权。
- 最后一名 owner 保护；当前没有 `group_owner`，因此当前按 **最后一名 `group_admin`** 保护。
- 当前 ACL 是 allow list，不提供 deny；不要设计“拒绝优先”的 UI，除非后端新增 deny 语义。
- 继承链要在 UI 可解释：`tenant Owner → group Admin → space Viewer`；不能只显示最终结果。
- 目标角色 `group_owner/editor/viewer` 未实现前，矩阵只读；不要把它们放进成员角色下拉。

### 4.6 Owner / 最后管理员保护

| 场景 | 当前行为 | 目标行为 |
| --- | --- | --- |
| 降级/移除最后一名 `group_admin` | API `last_group_admin_protected`；UI 应禁用 | 保留 |
| 删除/归档 group | 当前 `group_admin` 可调用 DELETE；实际将 `status` 置为 `archived` | 目标 `group_owner` 才可删除/归档；迁移前不要显示“Owner only”假限制 |
| 转移所有权 | 未实现 | `group_owner` + typed confirmation + audit |
| 删除/禁用最后一名 tenant owner | 现有 tenant 规则 | 沿用；不在 group 页面伪造 |

文案必须 scope-qualified：`最后一名组 Admin`，不是裸 `Owner`。如果未来引入 `group_owner`，再显示“组 Owner”。

### 4.7 DangerZone

- 删除/归档组：输入完整组名；显示成员数（若可用）、关联空间数（若可用）。当前 DELETE 实际把 `status` 置为 `archived`，是否有恢复 API 待确认；文案不要承诺物理删除或可恢复。确认按钮在输入完全匹配前禁用。
- 移除最后 admin：按钮禁用 + 解释；不弹确认。
- 归档/恢复：普通确认；说明对搜索/ACL 的影响（若未知则写“影响范围待后端确认”，不猜）。
- 所有危险操作成功/失败都 toast + 刷新 query；失败保留输入，不关闭对话框。

### 4.8 Audit tab

- 当前 `AuditFilter` 字段只有 `tenant_id`、`actor_id`、`action`、`space_id`、`from`、`to`、`limit`，**没有 `target_type` / `target_id`**。
- P1：Audit tab 显示组相关 action 的全局审计（如 `group.update`、`group.member_add`），通过 `action` 过滤；或直接链接 `/admin/governance/audit?action=group.*`。
- 目标：后端增加 target 过滤后，按 `target_type=group,target_id=<id>` 精确查询。
- 审计详情复用 `AuditPanel` + `DetailDrawer`；显示 actor、action、result、request_id、时间；不显示 CoT/敏感内容。

## 5. 角色词汇统一（重点）

记法：`scope.role`（如 `tenant.owner`、`space.editor`、`group.admin`）只是前端 composite key；实际 API 值仍是表中的 snake_case（`owner`、`space_owner`、`group_admin` 等）。不要把 composite key 写回请求。

### 5.1 当前角色（implemented）

| Scope | API 值 | 中文展示 | English | 状态 | 说明 |
| --- | --- | --- | --- | --- | --- |
| tenant | `owner` | 租户 Owner | Tenant Owner | implemented | tenant admin gate |
| tenant | `admin` | 租户 Admin | Tenant Admin | implemented | tenant admin gate |
| tenant | `user` | 租户成员 | Tenant Member | implemented | **不是 Viewer / Editor** |
| space | `space_owner` | 空间 Owner | Space Owner | implemented | 空间最终权限 |
| space | `space_admin` | 空间 Admin | Space Admin | implemented | 成员/设置 |
| space | `editor` | 空间 Editor | Space Editor | implemented | 可写 |
| space | `viewer` | 空间 Viewer | Space Viewer | implemented | 只读 |
| group | `group_admin` | 组 Admin | Group Admin | implemented | 管理组/成员；最后一名受保护 |
| group | `group_member` | 组成员 | Group Member | implemented | **仅成员关系，不是 Viewer/策略角色** |

### 5.2 目标 group 角色（target，未实现）

| Scope | 目标 API 值 | 中文展示 | English | 状态 | 目标能力 |
| --- | --- | --- | --- | --- | --- |
| group | `group_owner` | 组 Owner | Group Owner | **target** | 删除/转移、全部组管理、最后 owner 保护 |
| group | `group_admin` | 组 Admin | Group Admin | implemented | 成员/设置/策略；目标不含删除/转移 |
| group | `group_editor` | 组 Editor | Group Editor | **target** | 编辑组内容/组空间内容；不管成员/策略 |
| group | `group_viewer` | 组 Viewer | Group Viewer | **target** | 只读组内容/审计摘要 |

`Owner/Admin/Editor/Viewer` 只能作为**带 scope 的展示名**或**矩阵目标 persona**；不能作为同一层级的裸角色集合。

### 5.3 Display 与 mapping 规则

1. 所有角色通过 `roleLabel(scope, id)` 输出；组件不直接拼接字符串。
2. `scope` 必填：`tenant` / `space` / `group`。
3. `tenant.user` 显示“租户成员”，不映射为 Viewer；`group_member` 显示“组成员”，不映射为 Viewer。
4. `space.editor` 与目标 `group.editor` 不互换；scope 不同，能力不同。
5. 目标 `group_owner/editor/viewer` 只在 `PolicyMatrix` 的 `TARGET` 列出现；成员角色下拉不提供。
6. 未知角色：显示原始值 + `Unknown` badge；记录一次前端诊断；不猜测能力。
7. 普通 UI 禁止裸 snake_case；诊断/审计详情可显示 raw value，但要有 label。
8. `group_admin` 当前可归档组（DELETE 实际是 `status=archived`）；目标模型计划收紧为 `group_owner` only。迁移前 UI 不得声称“Admin 不能归档/删除”。

### 5.4 禁止混用（review checklist）

- 组列表/成员表出现裸 `Owner`、`Admin`、`Editor`、`Viewer` → 拒绝。
- 把 `group_member` 显示成 Viewer → 拒绝。
- 把 `tenant.user` 显示成 Editor/Viewer → 拒绝。
- 把 `space.editor` 当成 `group.editor` → 拒绝。
- 目标角色没有 `TARGET` 标记就出现在可编辑控件 → 拒绝。
- 颜色-only 表示权限 → 拒绝；必须有文本/aria-label。

## 6. 组件清单与复用

### 6.1 现有可复用

| 组件 | 位置 | 用法 |
| --- | --- | --- |
| `AdminSkeleton` | `features/admin/kit.tsx` | 表格/详情加载骨架 |
| `AdminQueryGate` | `features/admin/shared.tsx` | pending / 403 / feature_disabled / error |
| `ConfirmDialog` | `kit.tsx` | 普通危险操作；扩展 typed confirmation |
| `DetailDrawer` | `kit.tsx` | 资源详情；目标 480 px，补 focus trap/Esc/返回焦点 |
| `FormDialog` | `kit.tsx` | 创建/编辑 |
| `useAdminToast` | `kit.tsx` | 成功/失败反馈 |
| `OffsetPager` | `shared.tsx` | 25/page 分页 |
| `NotProvided` | `components/ui` | 无 API 能力 |
| `TableWrap` / `Table` / `Badge` / `Button` / `Banner` / `EmptyState` / `ErrorState` / `Tabs` | `components/ui` | 基础视觉与状态 |

注意：现有 `ConfirmDialog` / `DetailDrawer` 只有遮罩点击关闭，没有显式 focus trap、Esc、返回焦点；P0/P1 以**增强**方式补齐，不改视觉。

### 6.2 新增组件

| 组件 | 职责 | 关键 props / 约束 |
| --- | --- | --- |
| `AdminShell` | 顶栏 + 左导航 + 内容布局 + 路由出口 | `navGroups`、`active`、`breadcrumb`；移动端 overlay |
| `Breadcrumb` | 层级导航 | `items: {label, to?}[]`；最后一项 `aria-current` |
| `PageHeader` | 标题、状态、数量、描述、操作 | `eyebrow`、`title`、`meta`、`primary`、`secondary`；最多 1 个绿色主操作 |
| `ResourceTable<T>` | 通用资源表 | `rows`、`columns`、`rowKey`、`loading`、`error`、`empty`、`selected`、`onRowClick`、`toolbar`、`footer`、`stickyFirst` |
| `FilterBar` | 搜索/筛选/chips/reset | `value`、`onChange`、`fields`、`resultCount`；URL 同步由页面负责 |
| `PolicyMatrix` | 能力 × 角色矩阵 | `rows`、`roles`、`value`、`readOnly`、`target`、`scope`、`onChange`、`sources`；首列/表头 sticky |
| `DangerZone` | 高风险操作区 | `title`、`description`、`confirmText`、`impact`、`onConfirm`；输入匹配才启用 |
| `RoleBadge` / `roleLabel` | 角色显示 | `scope` + `id`；未知值兜底；不返回裸 label |

`CommandPalette`（全局搜索）可以由 `AdminShell` 内部实现，也可以独立组件；P0 先做导航命令，P1 加 users/groups/spaces。不要在没有后端搜索时伪造文档结果。

### 6.3 Token 与视觉约束

- 只用现有 `--color-canvas`、`--color-surface`、`--color-panel`、`--color-raise`、`--color-line`、`--color-line-strong`、`--color-fg`、`--color-fg-2`、`--color-fg-3`、`--color-accent`、`--color-accent-weak`、`--color-accent-line`、`--color-danger`、`--color-warn`。
- 不新增品牌色；绿色只给主操作/active/success；危险操作只给 `danger`。
- 字号：正文 13 px，表格 12 px，标签 10–11 px；行高 36 px；圆角 8–10 px；1 px 线。
- 过渡 150 ms；阴影只给 drawer/dialog；不使用渐变、插画、装饰拓扑。
- Card 只在确实需要分组时使用；资源页不要 Card 堆叠。

### 6.4 数据与状态

- 路由持有可分享状态：tab、filter、page、drawer 的资源 id；panel 用现有 `useQuery` 取数。不引入新的全局 admin store。
- mutation 成功后只 invalidate 相关 key（list / detail / audit）；不整页刷新，不伪造乐观策略写入。
- 权限判断以后端 principal + `/api/v1/features` + `/api/v1/admin/roles` 为准；前端只做 display mapping 与交互禁用，不在客户端复制授权规则。
- loading / empty / error / feature_disabled 由 `AdminQueryGate` 与 panel 统一处理；toast 只用于 mutation 结果。
- Overview 只展示 `/api/v1/admin/overview` 已有字段；没有数据源就不画图、不造“趋势”。

## 7. 分阶段实施

### P0 — Shell / Navigation / Route compatibility

范围：`AdminShell`、topbar、left nav、breadcrumb、PageHeader、嵌套路由、旧 tab 重定向、admin role gate、responsive nav、focus 基础设施。

验收：

- 旧 11 个 tab URL 全部客户端 `replace` 重定向到新路由（不是 HTTP 302）；query/hash 保留。
- `/admin/identity` 不会被 `/admin/:tab` 误判；未知 tab 仍回 `/admin`。
- 1568 / 1440 / 1280 无 body 横向滚动；1280 下 nav 208 px 或 overlay。
- 键盘：skip link、Tab 顺序、focus-visible、drawer/dialog Esc + 返回焦点。
- zh/en 文案不缺失；角色门禁无 403 闪烁。
- headless 截图：1568×900、1440×900、1280×800。

### P1 — Identity / Users / Groups / IAM

范围：Users 迁移到 ResourceTable；Groups 列表 + 详情；成员表；目标 PolicyMatrix 只读；Roles 目录；SSO NotProvided；最后 `group_admin` 保护；删除/归档组 typed confirm；audit 链接。

验收：

- 组列表搜索/状态过滤/分页；成员数无 API 时显示 `—`，不 N+1。
- 成员表只显示后端真实字段；Type/MFA/Last active 不造假。
- 角色下拉只有 `group_admin`/`group_member`；显示“组 Admin”/“组成员”。
- PolicyMatrix 明确 `TARGET`；无写入；无裸 Owner/Admin/Editor/Viewer。
- 最后 admin 降级/移除禁用；删除/归档组需要输入组名。
- 组详情 tabs 可深链；drawer/route 刷新可恢复。
- i18n、键盘、loading/empty/error/confirm、headless 截图。

### P2 — Content: Spaces / Documents / Connectors / Models

范围：Spaces、Connectors、Models 迁移 ResourceTable/FilterBar；Documents NotProvided；feature gate banner；详情 drawer。

验收：

- 每个列表有搜索/筛选/分页/空态/错误重试。
- Connectors 的 feature_disabled 用 `AdminQueryGate` 的 `admin.connectors.disabled`；不显示假连接器。
- Documents 明确 NotProvided + 说明（无 admin list API；可用搜索/空间内容）。
- 1280 表格内部滚动；无页面级横向滚动。

### P3 — Platform / Governance / Overview

范围：Usage、Quotas、Billing、Audit、Overview 迁移；真实指标；feature gate；审计详情 drawer；概览的降级/状态摘要。

验收：

- Overview 只显示真实数据；没有假图表/假趋势。
- Billing/Audit feature_disabled 有明确 banner 和替代路径。
- 配额/用量表格支持 scope 过滤与空态。
- 审计详情复用 `DetailDrawer`；不显示敏感 token/DSN。
- headless 截图 + 全阶段回归。

### 全阶段验收清单（每阶段都跑）

1. **宽度**：1568 / 1440 / 1280；`scrollWidth <= innerWidth`；表格内部滚动；无幽灵滚动条。
2. **键盘**：Tab 顺序、focus ring、drawer/dialog trap + Esc + 返回焦点；行操作可键盘触发。
3. **i18n**：zh-CN / en；无硬编码；角色 label 走映射；错误 code 本地化。
4. **角色门禁**：tenant owner/admin；solo local owner；403 不闪屏；目标角色不可写入。
5. **状态**：loading / empty / error / forbidden / feature_disabled / NotProvided / confirm 全部可见。
6. **截图**：headless Chrome 1568/1440/1280；关键页至少 Hero（Overview）、列表、详情、危险确认、NotProvided。
7. **无造假**：没有后端字段的列/指标不显示或明确 `—`/`NotProvided`。

## 8. 后端缺口与 NotProvided

| 能力 | 现状 | UI 决策 | 需要的后端 |
| --- | --- | --- | --- |
| SSO 配置 | 只有 `[sso]` 配置 / trusted header；无 admin API | `/admin/identity/sso` → `NotProvided`，链接配置文档 | 读取/更新 SSO 配置的 admin API |
| 公开分享 | `features.public_share` 仅 flag；无分享路由 | Overview/相关页 `NotProvided`；不做空路由 | 分享链接、权限、撤销、审计 |
| 租户设置 | 无 admin API | Overview `NotProvided` 卡片 | tenant settings API |
| Admin documents list | 无全局文档列表 API | `/admin/content/documents` → `NotProvided`；引导搜索/空间内容 | 分页/过滤的 admin documents API |
| Group policy 持久化 | 只有 `group_admin`/`group_member`；无 group policy 表/API | PolicyMatrix 只读 + `TARGET`；不可保存 | group roles + policy CRUD + effective API |
| Group member enrichment | `GroupMember` 只有 principal_id/role/added_by/created_at | 只显示现有字段；Type/MFA/Last active 不渲染 | principal join（name/email/type）、MFA、last active |
| Group member count | `Group` 无 member_count | 列表显示 `—`；不做 N+1 | `admin/groups` 聚合 member_count |
| Group-scoped audit | `AuditFilter` 无 target_type/target_id | 链接全局 audit，按 action 过滤；不假装精确 | audit target 过滤 |
| Group owner / transfer | 无 `group_owner`；仅最后 `group_admin` 保护 | 显示“组 Admin”保护；不显示假 Owner | group_owner + transfer + last-owner protection |
| Project / environment picker | 无 project API；env 来自 deployment mode | P0 隐藏/禁用 Project；Env 只显示真实配置 | tenant/project 模型或明确不做 |
| OIDC/SAML/SCIM | 只有 trusted header | SSO `NotProvided` | 身份联邦 |
| Role CRUD | `/admin/roles` 只读 | 角色目录只读；不做假表单 | 角色持久化/自定义（如产品需要） |

原则：**NotProvided 不是失败页**，而是诚实的边界；必须说明“缺什么、现在能做什么、去哪里配置/反馈”。

## 9. 风险与缓解

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 旧 11 tab 深链断裂 | 书签/文档/监控失效 | 兼容 redirect 表 + 自动化路由测试；保留 query/hash |
| 角色词汇迁移 | UI 混用、权限误判 | `roleLabel(scope,id)`；scope 必填；目标角色 `TARGET`；P1 不改 API 值 |
| PolicyMatrix 与真实 API 不一致 | 用户以为能保存/已生效 | 只读 + `TARGET`；保存按钮不渲染；NotProvided 说明 |
| 成员表字段缺失 | 假姓名/假 MFA 破坏信任 | 只显示真实字段；缺口列隐藏；后端 enrichment 后再加 |
| 三栏 + 表格宽度 | 1280 出现幽灵滚动条 | `min-width:0`、表格 wrapper `overflow-x:auto`、可选列隐藏；验收脚本检查 scrollWidth |
| Drawer/Dialog 可访问性 | 键盘用户被困 | focus trap、Esc、返回焦点；P0 补基础设施 |
| i18n 键膨胀 | 中英不一致 | 角色/状态集中映射；每个 phase 检查 zh/en |
| NotProvided 过多 | 导航像空壳 | 保留必要项 + badge；Overview 汇总缺口；不新增空路由 |
| 目标角色被误当成已实现 | 对外承诺过度 | 文档/UI 双标注 `TARGET`；发布说明只写已实现 |
| 认证/feature gate 闪烁 | 先显示再 403 | 布局级 guard + `AdminQueryGate`；不先渲染假数据 |
| 概览变成营销页 | 失去控制台感 | 只用真实 overview 数据；不做 hero/趋势图/插画 |

## 10. 待确认

1. **Project 的真实语义**：是 space、子租户，还是未来产品概念？未定义前顶栏 Project 隐藏/禁用。
2. **目标 group 角色**：`group_owner/editor/viewer` 是否按本文能力表落地？当前 `group_admin` 可归档组（DELETE 实际是 `status=archived`），目标是否收紧为 owner-only？
3. **PolicyMatrix 的发布策略**：P1 只读 preview，还是等 group policy API 完成后再出现？
4. **成员表 enrichment**：MFA / last active 是否来自身份源？没有来源就永久不显示，还是显示 `NotProvided`？
5. **Group-scoped audit**：是否值得为 group 增加 target filter，还是全局 audit 的 action 过滤足够？
6. **Roles 页面**：只读目录是否足够；是否需要自定义角色（当前无 API）？

以上 6 项不阻塞 P0；P1 前需要确认 1–3。
