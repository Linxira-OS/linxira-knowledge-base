# ADR 0010：空间自助、用户组与分层权限

- **状态**：Accepted
- **日期**：2026-11-08

## 背景

当前只有 `spaces` 表与单一 `owner_principal_id/visibility/default_acl_json`，没有 `space_members`、用户组或存储配额；所有登录用户拿到租户下全部空间（见 gap-assessment 与 `src/web.rs` 现状）。这无法支持“不同用户组自己创建空间、设置存储配额、管理成员、协作共享、编辑权限”，也无法在团队/企业档表达最小权限。

## 决策

**把权限拆成三层：租户角色、组角色、空间角色；空间自助创建受策略与配额约束；存储配额独立于模型 token 配额。**

- 租户角色：owner / admin / user / agent；组角色：group_admin / group_member；空间角色：space_owner / space_admin / editor / viewer（commenter 留 P1）。
- 组可拥有空间（`owner_type=group`）；group_admin 默认映射 space_admin，group_member 默认 viewer，空间 settings 可改默认 editor。显式用户空间角色优先于组派生；无显式用户角色时多组取 max。
- 新增 `groups` / `group_members` / `space_members`；`space_members` 唯一键 `(space_id, principal_type, principal_id)`。
- `spaces` 增加 `owner_type`、`owner_id`、`status`、`settings`；`allow_user_creation` 控制自助；solo 默认可创建自己的笔记空间/笔记本（组创建仍关闭），team/enterprise 默认开启空间与组。
- 配额层级 tenant → group → space → user；维度 storage_bytes/documents/chunks/members/spaces；hard/soft、80/95% 告警、超限 413/429；写入前原子预留、失败回滚；管理员 override 带 actor/reason/expires_at。
- 存储计量：source_bytes + index_bytes（chunks/events 文本）+ history_bytes；归档/软删除在物理清理前计入，清理后回收；与模型 token 配额完全分开。
- 共享：默认 private；internal 仅可发现元数据，读正文仍需成员/显式共享；ACL 与空间成员取交集；链接分享/评论/审批 P2。
- 审计：space/member/quota/ownership/group 事件全部写 actor/tenant/space/result。

## 备选方案与为何不选

| 方案 | 否决理由 |
| --- | --- |
| 只复用 `owner_principal_id`，不做成员表 | 无法表达多成员协作、组、角色与最后 owner 保护 |
| 租户角色直接映射空间角色 | 实例管理员默认读全部内容，违反最小权限；空间边界无法独立治理 |
| 只做组、不做显式用户角色 | 无法处理跨组协作/例外授权；组变更会意外扩大权限 |
| 存储配额与 token 配额共用计数器 | 维度与回收规则不同；空间删除/token 重试无法对账 |
| 链接分享先于成员/ACL | 无法撤销、无法审计，容易绕过权限 |

## 后果

**正面**

- 用户组可自助建空间，权限边界清晰；空间 owner 可管理成员与配额。
- 租户/组/空间三层不混用，管理员不会因实例角色自动读内容。
- 存储配额可解释、可回收；与模型计费分开。

**负面**

- 多三张表与角色求值逻辑；组/显式用户优先级需要测试矩阵。
- 存储计量与清理时点增加实现复杂度；归档/软删除仍占配额。
- 共享/链接分享需要额外审计与撤销机制。

## 关联文档

- [document/spaces-and-permissions.md](../spaces-and-permissions.md) — 权限矩阵、组映射、配额与 API
- [document/architecture.md](../architecture.md) — Space/成员/组/配额数据模型
- [document/gap-assessment.md](../gap-assessment.md) — 当前空间/成员缺口与工作量
- [document/tier-matrix.md](../tier-matrix.md) — 三档差异
- [document/roadmap.md](../roadmap.md) — P3/P4
