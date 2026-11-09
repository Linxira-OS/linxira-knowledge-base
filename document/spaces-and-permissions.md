# 空间自助、用户组与权限（设计）

> 状态：设计基线
> 关联决策：[ADR 0010](adr/0010-self-service-spaces.md)
> 关联文档：[architecture.md](architecture.md)、[tier-matrix.md](tier-matrix.md)、[gap-assessment.md](gap-assessment.md)
> 范围：租户角色、用户组、空间角色、空间自助创建、成员协作、存储配额与共享。本文只定义契约，不写实现。

## 1. 目标与边界

| 目标 | 说明 |
| --- | --- |
| 用户组可建空间 | tenant 级 group 拥有空间；group_admin 管理成员与空间设置 |
| 自助但可控 | `spaces.allow_user_creation` 控制；每用户空间数/存储/文档/切片有上限 |
| 角色分层 | 租户角色、组角色、空间角色分别建模，不混成一套 |
| 协作共享 | 成员邀请、组授权、显式用户角色；默认 private，internal 可发现但不可越权读 |
| 配额可解释 | tenant → group → space → user 四级；storage_bytes/documents/chunks/members/spaces |
| 可审计 | 空间/组/成员/配额/转移/归档全部写 audit |

不在本期：链接分享（P2）、评论/审批（P2）、跨租户共享、外部组同步（SCIM 另立项）。

## 2. 角色分层与权限矩阵

### 2.1 三层角色，不混用

| 层 | 角色 | 作用域 |
| --- | --- | --- |
| 实例/租户 | `owner` / `admin` / `user` / `agent` | 整个 tenant；决定能否建组、建空间、看审计 |
| 组 | `group_admin` / `group_member` | 一个 group；决定组空间与组成员 |
| 空间 | `space_owner` / `space_admin` / `editor` / `viewer` | 一个 space；决定页面、成员、配额、共享 |

租户角色**不自动等于**空间角色：tenant admin 管理空间元数据，但默认不读内容；需要 `admin_break_glass` 并写审计。`owner` 的 break-glass 同样写审计。

### 2.2 完整权限矩阵

符号：`✔` 允许；`△` 有条件/仅所属范围/需审计；`—` 禁止。

| 角色 | 创建空间 | 空间设置 | 成员管理 | 页面读 | 页面写 | 页面删/恢复 | 共享 | 配额设置 | 转移所有权 | 归档/删除 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| tenant owner | ✔ | ✔ | ✔ | ✔（审计） | ✔（审计） | ✔（审计） | ✔ | ✔ | ✔ | ✔ |
| tenant admin | ✔ | ✔ | ✔ | △（break-glass+审计） | △（break-glass+审计） | △（break-glass+审计） | ✔ | ✔（≤tenant cap） | ✔（≤tenant cap） | ✔（≤tenant cap） |
| tenant user | △（策略允许） | △（自己/管理的空间） | △（自己/管理的空间） | △（成员空间） | △（editor+） | △（自己/管理的空间） | △（自己/管理的空间） | △（≤父级 cap） | △（space_owner） | △（space_owner） |
| tenant agent | — | — | — | △（token scope+space role） | △（token scope+space role） | — | — | — | — | — |
| group_admin | △（组策略） | △（组空间） | ✔（组内） | △（组空间角色） | △（组空间角色） | △（组空间角色） | △（组空间） | △（≤group cap） | △（组空间+tenant admin） | △（组空间） |
| group_member | △（若策略允许） | — | — | △（组空间角色） | △（组空间角色） | — | — | — | — | — |
| space_owner | — | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | ✔（≤父级 cap） | ✔ | ✔ |
| space_admin | — | ✔ | ✔ | ✔ | ✔ | ✔ | ✔ | △（≤父级 cap） | — | — |
| editor | — | — | — | ✔ | ✔ | △（仅自己页面） | — | — | — | — |
| viewer | — | — | — | ✔ | — | — | — | — | — | — |

说明：`commenter` 留到 P1；本期 viewer 只能读，不能评论/写。space_admin 不能转移所有权或删除空间；space_owner 才能转移/归档/删除。

## 3. 用户组

### 3.1 组模型

| 表 | 字段 | 约束 |
| --- | --- | --- |
| `groups` | `group_id`、`tenant_id`、`name`、`description`、`created_by`、`status`、`created_at`、`updated_at` | `(tenant_id, name)` 唯一；status active/archived |
| `group_members` | `group_id`、`principal_id`、`role`（group_admin/group_member）、`added_by`、`created_at` | 唯一 `(group_id, principal_id)`；至少保留一个 group_admin |

### 3.2 组的创建与成员

- 创建组：tenant admin，或 `groups.allow_user_creation=true` 时允许 user 创建；创建者自动成为 group_admin。
- group_admin 可加/移除 group_member，可指定另一个 group_admin；不能移除最后一个 group_admin。
- group 可拥有空间：`spaces.owner_type=group, owner_id=group_id`。
- 组不能跨 tenant；group_member 不得是其他 tenant 的 principal。

### 3.3 组角色 → 空间角色

| 情形 | 映射 |
| --- | --- |
| group 拥有空间 | group_admin 默认 `space_admin`；group_member 默认 `viewer`，空间 settings 可把默认改为 `editor` |
| 组被授权到他人空间 | group_admin/group_member 按 `space_members` 的 `role` 显式指定，不自动推导 |
| 显式用户角色存在 | **显式用户角色优先**，可高于或低于组派生角色；管理员可“钉死”某个用户 |
| 多个组 | 无显式用户角色时取最高组派生角色（max），rank：viewer < editor < space_admin < space_owner |
| 租户/组角色 | 不自动叠加为 space_owner；space_owner 必须显式授予或由 owner_type 继承 |

## 4. 空间自助

### 4.1 创建策略与字段

| 项 | 契约 |
| --- | --- |
| `spaces.allow_user_creation` | solo 默认 `true`（可显式关闭，组创建仍关闭）；team/enterprise 默认 `true`（可关） |
| 每用户最大空间数 | `spaces.max_per_user`；user/group 创建时校验 |
| 默认配额 | `spaces.default_storage_bytes` / `documents` / `chunks` / `members` |
| 创建者 | 自动成为 `space_owner`；group 创建时 group 为 owner，创建者 group_admin 获 `space_admin` |
| 可见性 | 默认 `private`；`internal` 可在租户目录发现元数据，但读仍需成员或显式共享 |
| 状态 | `active` / `archived` / `deleted`（软删除） |

`spaces` 字段（在现有 `tenant_id/space_id/name/visibility/default_acl_json/owner_principal_id/created_at` 上扩展）：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `owner_type` | enum | `user` / `group` |
| `owner_id` | string | user_id 或 group_id |
| `status` | enum | `active` / `archived` / `deleted` |
| `settings` | json | 默认成员角色、链接分享开关、配额默认值等 |
| `archived_at` / `deleted_at` | timestamp | 生命周期 |

迁移兼容：`owner_type=user` 时 `owner_id=owner_principal_id`；`owner_principal_id` 保留为兼容/派生字段，group-owned 空间可为空。

### 4.2 space_members

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `space_id` | string | 是 | 空间 |
| `principal_type` | enum | 是 | `user` / `group` / `agent` |
| `principal_id` | string | 是 | 主体 |
| `role` | enum | 是 | `space_owner` / `space_admin` / `editor` / `viewer` |
| `added_by` | string | 是 | 操作者 |
| `created_at` | timestamp | 是 | 加入时间 |

唯一键：`(space_id, principal_type, principal_id)`。

### 4.3 生命周期

- 转移所有权：仅 space_owner 或 tenant owner/admin；目标必须是同 tenant 的 user/group；转移后旧 owner 降为 space_admin（或按设置）。
- 最后一个 space_owner 保护：移除/降级最后一个 space_owner 返回 `last_space_owner_protected`。
- 归档：只读，成员仍可读；不再接受写入；可恢复。
- 软删除：写 tombstone；保留期后物理清理；恢复需 space_owner/tenant admin。
- 删除空间：仅 space_owner/tenant owner；必须无未完成写入；写审计。

### 4.4 每用户私有笔记空间（系统空间）

笔记空间是**系统能力**，不是自助空间：solo/team/enterprise 的每个 principal 都有且只有一个私有笔记空间。

| 项 | 契约 |
| --- | --- |
| 端点 | `POST /api/v1/notes/space` 幂等 ensure；`GET /api/v1/notes/space` 查询（未创建返回 `404 not_found`） |
| `space_id` | `notes_<safe_principal>`；principal_id 含非 `[A-Za-z0-9_-]` 字符时截断并追加 principal_id 的稳定 sha256 后缀；`notes_` 前缀保留，普通建空间请求返回 `400` |
| owner | `owner_type=user`、`owner_id=owner_principal_id=<principal_id>` |
| 可见性/状态 | `visibility=private`、`status=active`；名称 `My notes` |
| ACL | 空间默认 ACL 与页面 ACL 均为 `allow_list(users=[principal_id])`；租户管理员默认读不到正文/历史，需要审计式 break-glass |
| 配额 | 绕开 `spaces.allow_user_creation` 与 `spaces.max_per_user`（系统空间）；页面写入仍走 tenant/owner/space 配额与审计 |
| 冲突 | 目标 `space_id` 已存在但 owner/类型不匹配时返回 `409 conflict`，不静默复用 |
| 修改 | 系统笔记空间拒绝 `PATCH /spaces/{id}`（防止移除 `settings.kind=notes` 后弱化页面 ACL） |
| 生命周期 | `DELETE` / archive / restore / transfer 对系统笔记空间返回 `403 notes_space_immutable`；系统笔记空间不可删除/归档/转移，删除单条笔记请删除页面 |
| 审计 | `POST` 写 `notes.space.ensure`（actor/tenant/space/result，detail 含 `created`） |

与共享 space 的边界：笔记空间只服务 owner，不提供成员/共享/发布语义；需要协作时使用普通空间，将页面复制/导入到共享空间。笔记内容仍是 Document/Page/Revision，复用现有配额、审计与检索管道。

## 5. 配额层级与存储计量

### 5.1 层级

`tenant → group → space → user(owned spaces/storage)`。每级可设上限，子级不得超过父级剩余额度；所有适用 hard 上限同时生效。

| 维度 | 说明 |
| --- | --- |
| `storage_bytes` | 源字节 + 索引文本字节 + 保留历史字节 |
| `documents` | 活跃文档数；superseded 历史不计入 documents |
| `chunks` | 活跃切片数；历史 revision 切片按保留策略计入 storage |
| `members` | 空间成员数（含组展开） |
| `spaces` | 用户/组拥有的空间数 |

### 5.2 存储计量口径

`storage_bytes = source_bytes + index_bytes + history_bytes`：

- `source_bytes`：当前活跃 revision 的原始源字节 + 原件快照字节；同一内容重复文档按逻辑字节分别计（不做内容去重抵扣）。
- `index_bytes`：chunks / events 的文本字节（不含向量与数据库页开销）；索引重建不重复计。
- `history_bytes`：按保留策略保留的 superseded revision 与旧 event；物理清理/压缩完成后回收。
- 归档空间计入 storage，直到物理清理；软删除空间在保留期内计入，purge 后回收。
- 删除页面/空间：写 tombstone 后进入待清理；清理完成才释放配额；释放前写入仍受配额限制。

### 5.3 原子预留与超限

- 写入前按估算增量在 `storage_usage` 原子增加 `reserved_bytes`/计数；失败回滚。
- 成功提交后 `reserved` 转 `used`；失败/超时释放。
- 并发用行锁/事务；SQLite 与 PostgreSQL 语义一致。
- 超限：storage → 413 `space_quota_exceeded`；请求数/成员/空间数 → 429 `space_quota_exceeded` 或 `tenant_quota_exceeded`；带 `Retry-After` 或明确的清理/升级提示。
- soft limit：80% / 95% 告警，不阻断；hard：阻断。
- 管理员 override：`quota_overrides` 带 actor、reason、expires_at，写审计。
- 与 token 配额分开：storage quota 管空间内容；`quota_policies` 管模型 token。两者不共用计数器。

### 5.4 数据模型

| 表 | 关键字段 | 说明 |
| --- | --- | --- |
| `space_quotas` | `quota_id`、`tenant_id`、`scope_type`（tenant/group/space/user）、`scope_id`、`storage_bytes`、`documents`、`chunks`、`members`、`spaces`、`enforcement`、`updated_by`、`updated_at` | 唯一 `(tenant_id, scope_type, scope_id)` |
| `storage_usage` | `scope_type`、`scope_id`、`tenant_id`、`space_id`、`source_bytes`、`index_bytes`、`history_bytes`、`total_bytes`、`reserved_bytes`、`updated_at` | 聚合与预留 |
| `quota_overrides` | `tenant_id`、`scope_type`、`scope_id`、`dimension`、`limit`、`actor`、`reason`、`expires_at`、`created_at` | 管理员临时覆盖 |

## 6. 共享与协作

- `visibility=private`：仅 `space_members` 可见；默认。
- `visibility=internal`：租户目录可发现 **名称/描述/owner**，但读正文仍需成员或显式共享；不得通过目录推断内容。
- 成员邀请（站内）：space_admin/space_owner 发起；被邀请人接受后写 `space_members`；权限变更即时生效。
- 组授权：`space_members.principal_type=group`，组内成员按组角色映射获得空间角色。
- 链接分享：P2；必须有 scope/expiry/可撤销；不得绕过 ACL。
- ACL 与空间成员：访问 = 空间成员资格 ∩ 文档 ACL；两者都通过才返回。跨空间引用要求双方都可读。
- 编辑并发：沿用 revision + `If-Match`/409；评论/审批不做（P2）。
- 删除成员：即时失效；缓存按 `acl_fingerprint`/成员版本失效。

## 7. API / UI / MCP

| 接口 | 作用 | 权限 |
| --- | --- | --- |
| `GET /api/v1/spaces` | 我的空间/加入的空间 | `kb.read` |
| `POST /api/v1/spaces` | 自助创建 | `kb.write` + 策略 |
| `PATCH /api/v1/spaces/{id}` | 设置/可见性/归档 | `space_admin`/`space_owner` |
| `DELETE /api/v1/spaces/{id}` | 软删除 | `space_owner`/tenant owner |
| `GET/POST /api/v1/spaces/{id}/members` | 成员列表/邀请 | `space_admin`/`space_owner` |
| `PATCH/DELETE /api/v1/spaces/{id}/members/{principal}` | 改角色/移除 | `space_admin`/`space_owner` |
| `GET /api/v1/spaces/{id}/usage` | 配额/用量 | 成员；管理员可全量 |
| `POST /api/v1/spaces/{id}/transfer` | 转移所有权 | `space_owner` + tenant admin |
| `GET/POST /api/v1/groups` | 组列表/创建 | admin 或策略 |
| `GET/POST/PATCH/DELETE /api/v1/groups/{id}/members` | 组成员 | `group_admin` |
| MCP `kb_list_spaces` | 只读空间列表 | `kb.read` |
| MCP `kb_space_usage` | 只读空间用量 | `kb.read` |

管理走 REST；MCP 不提供空间管理写工具。UI：空间设置/成员/配额/用量/归档；用户端“我的空间/加入的空间”；`internal` 空间只展示元数据，不泄露内容。

## 8. 审计事件

| 事件 | 关键字段 |
| --- | --- |
| `space.created` / `space.updated` / `space.archived` / `space.deleted` | actor、tenant、space、result、request_id |
| `member.added` / `member.removed` / `member.role_changed` | actor、space、principal、from/to role |
| `quota.updated` / `quota.exceeded` | actor、scope、dimension、limit、used、result |
| `ownership.transferred` | actor、space、from owner、to owner |
| `group.created` / `group.updated` / `group.archived` / `group.member_added` / `group.member_removed` | actor、tenant、group、principal |

所有事件含 `tenant_id`、`actor_id`、`actor_type`、`request_id`、`result`、`at`；审计读取不默认返回正文。

## 9. 阶段与估算

估算口径：1 人周 = 1 名熟悉 Rust/axum/sqlx 的工程师 5 个工作日；**估算，非承诺**；分项与合计独立取整，可能有 1 人周差异。

| 阶段 | 范围 | 人周 |
| --- | --- | --- |
| P0 | groups/group_members、space_members、三层角色矩阵、创建策略、基础 storage quota、审计事件 | 6–10 |
| P1 | 自助配额/用量页、成员邀请、共享/可见性、归档/恢复、转移所有权 | 6–10 |
| P2 | 链接分享、评论、审批 | 4–8 |
| **合计** |  | **16–28** |

## 10. 风险与待验证

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 角色叠加歧义 | 越权/误拒 | 显式用户角色优先；多组取 max；矩阵测试 |
| 组角色映射过宽 | 组内成员获得超预期权限 | 默认 group_member=viewer；改 editor 需 space settings + 审计 |
| 配额并发 | 超卖/误阻断 | 原子预留 + 行锁；SQLite/PG 同语义测试 |
| 存储计量争议 | 用户不理解用量 | 分 source/index/history；归档/删除回收时点写清 |
| 共享链接 | 越权/无法撤销 | P2；scope/expiry/撤销；不绕过 ACL |
| 最后 owner/group_admin | 孤儿空间/组 | last-owner 保护；恢复流程需 tenant owner |

## 11. 与现有文档的关系

- [architecture.md](architecture.md)：Space 字段、成员/组/配额表、ACL 与空间成员交集。
- [tier-matrix.md](tier-matrix.md)：三档空间自助/组/配额/共享差异。
- [roadmap.md](roadmap.md)：P3/P4 交付与出口。
- [gap-assessment.md](gap-assessment.md)：新增能力面与工期修订。
- [adr/0010-self-service-spaces.md](adr/0010-self-service-spaces.md)：方向决策。
