# 召回期 ACL

## 过滤必须写在召回 SQL 里

每次关键词或向量召回都调用 `Policy::visibility(alias)`，把可见性条件拼进同一条 SQL，并在 LIMIT 之前生效。条件由四部分组成：

- `tenant_id` 等于主体租户，跨租户永远不匹配；
- `space_id` 属于主体允许的空间列表；
- `valid_from <= as_of`，且 `valid_to IS NULL OR valid_to > as_of`；
- `acl_principals` 命中 `|*|`、`|u:<principal_id>|` 或 `|g:<group>|` 之一。

solo 也不例外：allow-all 主体仍然生成显式条件，而不是省略 WHERE。这条路径在低档位就被执行。

## 为什么不能后置剔除

先取 TopN 再删无权限结果，会让无权限文档占用名额，既泄漏存在性，也让有效召回变少。过滤必须发生在数据库侧。

## 二次校验

召回后还有一次进程内 `Policy::can_read` 校验，用于兜住索引元数据过期。被它剔除的候选会写入 `degraded[]`，码为 `acl_second_check_dropped`，类别是 security。这是补充检查，不是主过滤。

ACL JSON 无法解析时，`can_read` 直接返回 false；导入阶段则把该文件记为 failed。权限失败不降级放行。
