# 贡献指南（Contributing）

感谢参与。本文约定本仓库（`lkb`）的贡献流程与硬性规则；许可证为分层授权（核心 AGPL-3.0-only，产品面 MIT OR Apache-2.0，商业许可可选），详见 [LICENSING.md](LICENSING.md)。与 `document/build-policy.md`、`document/architecture.md` 及 `document/adr/` 冲突时，以那些文件为准。

## 1. 提交、DCO 与贡献授权

- 所有提交必须按 [DCO 1.1](https://developercertificate.org/) 签名：使用 `git commit -s`，提交信息包含 `Signed-off-by: Your Name <you@example.com>`（真实姓名/邮箱）。
- 不要求签署单独的 CLA 文件，也不要求版权转让。**提交 PR 即表示同意**：贡献按所修改文件适用的许可证授权（核心路径 AGPL-3.0-only，宽松路径 `MIT OR Apache-2.0`）；对核心的贡献额外授予版权持有人永久、全球、非独占、免版税的权利，使其可按商业许可再许可该贡献（轻量入站 CLA，以 PR 记录为准）。
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
- **连接器与许可证隔离**：AGPL / BSL 等与宽松层不兼容的组件**只能进程外运行**（HTTP / 子进程 / 桥接，不链接进二进制）；引入新依赖前核对许可证（AGPL 核心与 MIT/Apache 宽松层的兼容性、`document/oss-landscape.md` 红线）。许可证边界见 `LICENSING.md`：不得把 AGPL 核心文件移入宽松层或反向移动；新增核心能力默认 AGPL-3.0-only，新增 CLI/UI/部署/工具默认 `MIT OR Apache-2.0`，变更边界需在 PR 说明并同步 `LICENSING.md`。
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
