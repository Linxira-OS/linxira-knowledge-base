## 变更摘要

<!-- 一段话说明改了什么、为什么；关联 issue 用 #N。破坏性变更必须在此明确标注。 -->

## 检查清单

- [ ] 本地门禁通过：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`（改动 `web/` 时另加 `npm ci && npm run build`）
- [ ] 文档已同步：`docs/zh` 与 `docs/en` 双端更新；架构级决策已新增 ADR（`document/adr/`）
- [ ] 无密钥：提交中不含令牌、密码、连接串、私钥或 `.lkb/pg.env` 内容
- [ ] 迁移：涉及 schema 时 `migrations/sqlite` 与 `migrations/postgres` 均已更新且逻辑 schema 一致
- [ ] 契约检查：`Answer` / `degraded[]` / API 错误码等对外契约未被静默破坏（破坏性变更已在摘要说明并写入 `CHANGELOG.md`）
- [ ] 提交已按 DCO 签名（`git commit -s`）
