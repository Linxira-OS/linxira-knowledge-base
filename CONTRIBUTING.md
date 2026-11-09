# 贡献指南（Contributing）

感谢参与。本文约定本仓库（`lkb`，MIT）的贡献流程与硬性规则；与 `document/build-policy.md`、`document/architecture.md` 及 `document/adr/` 冲突时，以那些文件为准。

## 1. 提交与 DCO

- 所有提交必须按 [DCO 1.1](https://developercertificate.org/) 签名：使用 `git commit -s`，提交信息包含 `Signed-off-by: Your Name <you@example.com>`（真实姓名/邮箱）。
- 不使用 CLA；无法签署 DCO 的贡献无法合并。
- PR / issue 中不得出现密钥、令牌、真实连接串或个人数据。
- 提交信息沿用仓库现状（Conventional 风格前缀，如 `feat:`、`fix:`、`docs:`、`chore:`）；一个 PR 聚焦一件事。

## 2. 文档先于代码

- 行为、CLI、API、配置或迁移变化时，先更新文档再（或与代码同时）提交：
  - 产品文档：`docs/zh` 与 `docs/en` **双端同步**（两者是 `lkb seed` 的编译期内置种子，单端修改会破坏种子布局契约）；
  - 设计与决策记录：`document/`；架构级决策必须新增 ADR（`document/adr/`，编号顺延，格式参照现有 0001–0012）。
- 文档必须描述真实行为：未验证的结论、未测量的数字（性能、覆盖率、规模）不得写入。

## 3. 硬性约束

- **禁止 profile 分支**：档位差异只允许出现在配置与适配器装配层（`src/config.rs`、provider/store 选择）；业务代码不得按 `profile` 分支（[ADR 0001](document/adr/0001-one-core-three-profiles.md)）。运行时差异用 feature gate / deployment mode 表达，且领域代码只读 gate。
- **治理字段 day-one**：Document/Chunk 必须写入 `tenant_id` / `space_id` / `acl_json` / `valid_from` / `valid_to` / `version` / `superseded_by`；召回 SQL 必须经 `Policy::visibility` 生成过滤（[ADR 0002](document/adr/0002-governance-fields-day-one.md)）；身份与 ACL 失败必须 fail-closed，绝不进入降级放行。
- **degraded / Answer 契约**：可选能力缺失或失败必须以稳定 code 写入 `degraded[]`，不得静默成功（[ADR 0004](document/adr/0004-explicit-degradation.md)）；`Answer` / `Citation` / `Degraded` 对外契约变更属破坏性变更，需在 PR 说明并写入 `CHANGELOG.md`（[ADR 0005](document/adr/0005-single-answer-contract.md)、[ADR 0009](document/adr/0009-summaries-and-conversations.md)）。
- **i18n（zh/en）**：新增用户可见文案需同时提供 `zh-CN` 与 `en` 消息键（`src/i18n.rs`）；API 错误返回稳定 code + 本地化 message；日志保持英文。
- **连接器许可证隔离**：AGPL / BSL 等与 MIT 不兼容的组件**只能进程外运行**（HTTP / 子进程 / 桥接，不链接进二进制）；引入新依赖前核对许可证（MIT 兼容性、`document/oss-landscape.md` 红线），`Cargo.toml` 的 `license = "MIT"` 不得变更。
- **平台与依赖约束**：不引入第二个关系库适配器（无 SQL Server 等）；不新增 `build.rs`；不使用 sqlx 编译期宏（`query!` / `query_as!`，构建不得依赖 `DATABASE_URL`）；schema 变更必须同时提供 `migrations/sqlite` 与 `migrations/postgres`，逻辑 schema 保持一致。

## 4. 测试与构建门禁

提交前至少运行（与 CI 一致）：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked

# 可选：PostgreSQL 集成测试（提供测试库时；未设置会打印 SKIP，不伪装通过）
set -a; source .lkb/pg.env; set +a; cargo test --locked

# 改动 web/ 时
cd web && npm ci && npm run build
```

- CI 在 ubuntu-latest 与 windows-latest 上运行 fmt / clippy / test，并在 ubuntu 上构建 web；PR 必须全绿。
- 不要通过 `#[allow(...)]`、删除测试或弱化断言来绕过门禁。
- 新增行为需要有可观测的测试；无法自动化的部分在 PR 中说明手工验证步骤与真实输出。

## 5. PR 流程

1. Fork 或基于 `main` 新建分支；提交按 DCO 签名。
2. 按 PR 模板逐项自检（测试 / 文档 / 无密钥 / 迁移 / 契约检查）。
3. CI 全绿 + CODEOWNERS（`@BOHUYESHAN-APB`）审阅后由维护者合并；审查意见未解决前请勿强推或重写历史。
4. 破坏性变更、行为口径变化、已声明“未实现”边界的变化：即使 CI 通过，也须等待维护者明确确认。
