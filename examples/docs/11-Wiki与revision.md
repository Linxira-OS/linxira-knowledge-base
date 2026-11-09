# Wiki 与 revision

## 数据模型

Wiki 由 spaces、pages、revisions 三张表组成。page 以 `(tenant_id, space_id, slug)` 为主键，保存标题、当前 revision id、ACL、版本号与时间戳；revision 保存 Markdown、作者、内容哈希与版本，历史修订永久保留。

## 编辑语义

保存页面需要 write 权限、通过 CSRF 校验，并且空间在主体允许范围内。首次创建 version 为 1；之后每次保存生成新的 revision_id，version 递增，旧 revision 内容不变，history 页列出各版本的作者、时间与内容哈希。

## 与检索的关系

页面保存后会作为文档写入索引，逻辑路径固定为 `wiki/{space}/{slug}.md`，`source_uri` 为 `wiki://{space}/{slug}`。同一路径的新内容会走与文件导入相同的 upsert 流程，旧文档被标记 superseded。

因此 Wiki 页面既能被检索，也能被回答引用；权限沿用页面的 ACL：solo 为 allow-all，团队档默认只允许作者主体。

CLI 用 `lkb page list` 与 `lkb page show <space> <slug>` 只读查看，写入仍通过 Web 表单完成。
