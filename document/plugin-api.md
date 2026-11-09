# Plugin API v0 / 插件 API v0

Status: **front-end registration skeleton**. Plugins are compiled into the web
bundle and registered in-process; the host never loads, evaluates or fetches
remote JavaScript. The executable reference implementation is
`web/src/plugins/registry.ts` with types in `web/src/plugins/types.ts`.

状态：**前端注册骨架**。插件随 Web 包一起编译、在进程内注册；宿主不会加载、执行或
拉取远程 JavaScript。可执行的参考实现见 `web/src/plugins/registry.ts`，类型定义见
`web/src/plugins/types.ts`。

---

## 1. API version / API 版本

The host advertises `PLUGIN_API_VERSION = "0.1.0"`. Plugin manifests declare a
semver range in `engines.api`; registration is rejected when the range is
malformed, uses unsupported syntax, or does not accept the host version.

宿主暴露 `PLUGIN_API_VERSION = "0.1.0"`。插件在 `engines.api` 中声明 semver 范围；
范围语法非法、包含不支持的写法或与宿主版本不兼容时，注册会被拒绝。

Supported range grammar in v0 / v0 支持的范围语法：

| Form | Example | Meaning |
| --- | --- | --- |
| `*` | `*` | any host API / 任意宿主版本 |
| caret / 脱字符 | `^0.1.0` | `>=0.1.0 <0.2.0` (for `0.x`, minor is breaking / `0.x` 中 minor 视为破坏性) |
| tilde / 波浪号 | `~0.1.0` | `>=0.1.0 <0.2.0` |
| comparators / 比较符 | `>=0.1.0 <0.2.0` | space-joined AND / 空格连接，全部满足 |
| exact / 精确 | `=0.1.0` or `0.1.0` | one version / 单一版本 |

`||`, prerelease selectors, npm aliases and workspace protocols are not
supported. `engines.app` is informational in v0.

不支持 `||`、预发布版本选择器、npm alias 与 workspace 协议。`engines.app` 在 v0 仅作
信息展示。

Semver policy / Semver 策略:

- The plugin API follows semver. While the major version is `0`, a minor bump may
  contain breaking changes; plugins must pin a range (`^0.1.0` is the expected
  default).
- Breaking changes bump the API minor (pre-1.0) or major (post-1.0) and keep a
  one-minor deprecation window where practical.
- Contributions are versioned by the host registry, not by each plugin's own
  version. Two plugins may share an API version but not an id.

- 插件 API 遵循 semver。主版本为 `0` 时，minor 提升可能包含破坏性变更；插件应声明
  范围（推荐 `^0.1.0`）。
- 破坏性变更在 1.0 前提升 minor，1.0 后提升 major，并尽量保留一个 minor 的弃用期。
- 贡献项由宿主注册表统一管理版本；插件自身的 `version` 与 API 版本相互独立。两个插件
  可以共享 API 版本，但 id 必须唯一。

---

## 2. Manifest / 清单

```json
{
  "id": "lkb.callout",
  "name": "Callout blocks",
  "version": "0.1.0",
  "description": "Obsidian-style callouts rendered through the v0 block renderer API.",
  "author": "LKB",
  "engines": { "api": "^0.1.0" },
  "permissions": ["block:render", "command:register"],
  "contributes": {
    "blockRenderers": [
      {
        "id": "lkb.callout.renderer",
        "name": "Callout",
        "description": "Renders `> [!NOTE]` style blocks.",
        "priority": 100
      }
    ],
    "commands": [
      {
        "id": "lkb.callout.insert",
        "title": "Insert callout",
        "slash": "callout"
      }
    ]
  }
}
```

Rules enforced by the registry / 注册表强制规则:

- `id` matches `[a-z0-9][a-z0-9._-]{1,63}` and is unique.
- `version` is semver `MAJOR.MINOR.PATCH` (prerelease/build suffix accepted but
  ignored for compatibility).
- `permissions` are unique and drawn from the permission table below.
- Every executable `blockRenderers[]` / `commands[]` entry appears in
  `contributes`, and every declared contribution has an implementation.
- Contribution ids are unique across the whole host, not only within a plugin.
- Permission gates: renderers need `block:render`; commands need
  `command:register`; note lifecycle hooks need `note:read` / `note:write`;
  settings hooks need `settings:read`.

- `id` 必须匹配 `[a-z0-9][a-z0-9._-]{1,63}` 且全局唯一。
- `version` 为 semver `MAJOR.MINOR.PATCH`（预发布/构建后缀可解析，但兼容性比较时忽略）。
- `permissions` 不可重复，且只能取自下方权限表。
- 每个可执行的 `blockRenderers[]` / `commands[]` 必须出现在 `contributes` 中；每个声明
  的贡献项也必须有实现。
- 贡献项 id 在**整个宿主**内唯一，而非仅插件内唯一。
- 权限门控：块渲染器需要 `block:render`；命令需要 `command:register`；笔记生命周期
  hooks 需要 `note:read` / `note:write`；设置 hooks 需要 `settings:read`。

A compiled-in module / 编译进包的模块:

```ts
import type { PluginModule } from './types'

export const examplePlugin: PluginModule = {
  manifest: { /* as above */ },
  blockRenderers: [
    {
      id: 'lkb.callout.renderer',
      name: 'Callout',
      priority: 100,
      match: (block) => block.source.startsWith('> [!'),
      render: (block, context) => ({ html: /* sanitised by the host */ '<div>…</div>' }),
    },
  ],
  commands: [
    {
      id: 'lkb.callout.insert',
      title: 'Insert callout',
      slash: 'callout',
      run: (invocation, context) => {
        const note = invocation.note ?? context.host.getNote()
        if (!note) return
        context.host.updateNote({ markdown: `${note.markdown.trimEnd()}\n\n> [!NOTE]\n> Text.\n` })
      },
    },
  ],
}
```

The registry exposes `register`, `unregister`, `getPlugin`, `listPlugins`,
`listBlockRenderers`, `listCommands`, `resolveBlockRenderer`, `activate`,
`deactivate`, `runCommand` and a `subscribe`/`getRevision` pair for React's
`useSyncExternalStore`.

注册表提供 `register`、`unregister`、`getPlugin`、`listPlugins`、
`listBlockRenderers`、`listCommands`、`resolveBlockRenderer`、`activate`、
`deactivate`、`runCommand`，以及供 React `useSyncExternalStore` 使用的
`subscribe` / `getRevision`。

---

## 3. Lifecycle / 生命周期

```
bundle load
  → registry.register(module)
      validate manifest + semver + permissions + ids
      index contributions
      status: registered
  → host.activate(id, host)
      build PluginContext
      call module.activate?(context)
      status: active | failed
  → note open / save / render / command / settings events
  → host.deactivate(id) or registry.unregister(id)
      module.deactivate?()
      remove contributions
```

- **Registration is atomic.** Validation runs before any mutation; a rejected
  plugin leaves the registry untouched and throws `PluginRegistrationError`
  with the list of problems.
- **Activation is isolated.** A throwing `activate` moves the plugin to
  `failed`, records the message, and never breaks other plugins or the editor.
- **Deactivation** is best-effort and idempotent; contributions are removed
  even if `deactivate` throws.
- v0 registers only compiled-in modules. There is no download, dynamic import,
  `eval`, `new Function` or remote manifest fetch.

- **注册是原子的**：先完整校验再写入；被拒绝的插件不会留下半注册状态，并抛出带有
  问题列表的 `PluginRegistrationError`。
- **激活相互隔离**：`activate` 抛错只会把该插件标记为 `failed` 并记录信息，不会影响
  其他插件或编辑器。
- **停用**为尽力而为且幂等；即使 `deactivate` 抛错也会移除贡献项。
- v0 只注册编译进包的模块，不下载、不动态 import、不使用 `eval` / `new Function`，
  也不拉取远程 manifest。

### v0 implementation status / v0 实现状态

| Hook | v0 status |
| --- | --- |
| `onBlockRender` | Executable path is `BlockRenderer.match/render`; the registry dispatches it from `resolveBlockRenderer`. The hook shape is stored for the future sandbox host. |
| `onNoteOpen` | Typed; host dispatch lands with the sandboxed lifecycle. |
| `onNoteSave` | Typed; host dispatch lands with the sandboxed lifecycle. |
| `onCommand` | Typed; commands can be registered and queried, slash-menu dispatch is not wired. |
| `onSettings` | Typed; the settings UI and persistence are not wired. |

| Hook | v0 状态 |
| --- | --- |
| `onBlockRender` | 可执行路径是 `BlockRenderer.match/render`，由 `resolveBlockRenderer` 分发；hook 类型为未来沙箱宿主保留。 |
| `onNoteOpen` | 已定义类型；随沙箱生命周期一起接入宿主分发。 |
| `onNoteSave` | 已定义类型；随沙箱生命周期一起接入宿主分发。 |
| `onCommand` | 已定义类型；命令可注册/查询，slash 菜单分发尚未接入。 |
| `onSettings` | 已定义类型；设置 UI 与持久化尚未接入。 |

---

## 4. Hooks / 钩子

All hook timings are synchronous or awaited in order; a slow hook must not be
used for rendering-critical work. In the future sandbox each hook becomes a
`postMessage` request with a host-side timeout.

所有钩子按顺序同步调用或 await；不得在渲染关键路径中使用慢钩子。未来沙箱中每个
hook 会变成带宿主超时的 `postMessage` 请求。

| Hook | When | Contract |
| --- | --- | --- |
| `onNoteOpen(note, ctx)` | After the editor has loaded a page and its Markdown. | Read-only snapshot; may warm caches or validate syntax. Cannot block opening. |
| `onNoteSave(note, ctx)` | After the backend accepted a revision. | Snapshot contains the new revision id. Must not trigger another save. |
| `onBlockRender(block, ctx)` | Before a block preview is painted. | Return `BlockRenderResult` or `null`. HTML is sanitised again by the host. Prefer a `BlockRenderer` contribution instead. |
| `onCommand(command, ctx)` | When a command is dispatched. | Cannot veto; errors surface as a toast and are isolated. |
| `onSettings(settings, ctx)` | After settings are loaded/changed. | Read-only snapshot; use `host.updateSettings` for writes when granted. |

### Block renderer contract / 块渲染器契约

```ts
interface BlockRenderer {
  id: string
  name: string
  priority?: number                 // higher wins; ties keep registration order
  match(block: BlockSnapshot): boolean
  render(block: BlockSnapshot, context: BlockRendererContext): BlockRenderResult | null
}
```

- `match` and `render` must be pure and fast; renderers run on every keystroke.
- Multiple renderers may match; only the highest-priority successful result is
  used. A throw or `null` falls through to the next renderer, then to the normal
  `renderMarkdown` preview.
- `context.renderMarkdown` lets a renderer embed nested Markdown (callout body,
  quoted content). Its output is already sanitised.
- Returned `html` is re-sanitised by the host with DOMPurify's HTML profile
  before insertion. `className` is reduced to `[\w -]`.
- No renderer may read or write the DOM, storage or network. v0 has no network
  permission at all.

- `match` / `render` 必须纯且快；它们会在每次按键时运行。
- 允许多个渲染器命中，只使用优先级最高且成功的结果。抛错或返回 `null` 会继续尝试
  下一个渲染器，最终回退到普通 `renderMarkdown` 预览。
- `context.renderMarkdown` 用于嵌入嵌套 Markdown（callout 正文、引用内容），其输出
  已消毒。
- 返回的 `html` 在插入前会再次经过 DOMPurify HTML profile 消毒；`className` 会被
  限制为 `[\w -]`。
- 渲染器不得访问 DOM、存储或网络。v0 完全不授予网络权限。

Built-in example: the callout renderer matches `> [!NOTE]`, `> [!TIP]`,
`> [!WARNING]`, `> [!DANGER]`, `> [!INFO]`, `> [!SUCCESS]`, `> [!QUESTION]`,
`> [!QUOTE]` and `> [!EXAMPLE]`, renders the body through the host Markdown
renderer, and wraps it in `.lkb-callout` chrome. The same plugin contributes the
`callout` command.

内置示例：callout 渲染器匹配 `> [!NOTE]`、`> [!TIP]`、`> [!WARNING]`、
`> [!DANGER]`、`> [!INFO]`、`> [!SUCCESS]`、`> [!QUESTION]`、`> [!QUOTE]`、
`> [!EXAMPLE]`，通过宿主 Markdown 渲染器渲染正文，并包在 `.lkb-callout`
样式中；同一插件还贡献 `callout` 命令。

---

## 5. Commands and slash commands / 命令与 slash 命令

```ts
interface Command {
  id: string
  title: string
  description?: string
  slash?: string          // without the leading slash
  run(invocation: CommandInvocation, ctx: PluginContext): void | Promise<void>
}
```

- Slash names must be unique case-insensitively across the host; a future slash
  menu filters by title and slash trigger.
- `CommandInvocation` carries the active note snapshot, the current selection
  and the source of the triggering block. The host may pass `selection: ''`.
- `run` may read the note and request a patch through `PluginHost.updateNote`.
  It must not write storage or the network directly.
- A command result is `{ ok, error? }`; a throw is caught and reported through
  `host.toast`, leaving the editor state untouched.
- v0 has no global command palette and no automatic key binding. Registration,
  lookup and isolated execution exist; the UI dispatch is planned for the
  sandboxed host.

- slash 名称在宿主内大小写不敏感且唯一；未来的 slash 菜单按标题与触发词过滤。
- `CommandInvocation` 携带当前笔记快照、当前选区与触发命令的块源文本；宿主可传空选区。
- `run` 可以读取笔记并通过 `PluginHost.updateNote` 请求补丁，不得直接写存储或网络。
- 命令结果为 `{ ok, error? }`；抛错会被捕获并通过 `host.toast` 报告，不改变编辑器状态。
- v0 没有全局命令面板与自动快捷键；已有注册、查询与隔离执行，UI 分发留给沙箱宿主。

---

## 6. Permissions and sandbox / 权限与沙箱

| Permission | Grants | v0 |
| --- | --- | --- |
| `note:read` | Read the active note snapshot (`getNote`, `onNoteOpen`). | Grantable |
| `note:write` | Request note patches (`updateNote`); `onNoteSave` also needs `note:read`. | Grantable |
| `block:render` | Register block renderers and `onBlockRender`. | Grantable |
| `command:register` | Register commands and receive `onCommand`. | Grantable |
| `settings:read` | Read the plugin's scoped settings. | Grantable |
| `settings:write` | Update the plugin's scoped settings. | Grantable |
| `network:fetch` | Outbound HTTP. | **Rejected in v0** |

Declaring `network:fetch` is a registration error in v0: the host cannot enforce
the corresponding sandbox yet, so it fails closed. Plugins cannot access
`localStorage`, cookies, IndexedDB, the DOM, the clipboard, workers or iframes
through the API. `PluginHost` is the entire capability surface.

v0 中声明 `network:fetch` 会直接导致注册失败：宿主尚无法执行对应沙箱，因此采用
fail-closed。插件不能通过 API 访问 `localStorage`、cookie、IndexedDB、DOM、剪贴板、
worker 或 iframe；`PluginHost` 就是全部能力面。

### Future sandbox / 未来沙箱

Remote or market-installed plugins will not run in the host page:

远程或商店安装的插件不会运行在宿主页面内：

- **Isolation**: each plugin runs in a dedicated cross-origin iframe or a
  module worker. The host communicates only through a versioned
  request/response `postMessage` protocol; no shared objects, no DOM handles.
- **CSP**: the host document keeps `script-src 'self'` (no `unsafe-eval`, no
  remote scripts), `object-src 'none'`, `base-uri 'none'`,
  `frame-ancestors 'self'`. The sandbox frame uses its own origin and a stricter
  CSP; network access stays disabled unless `network:fetch` is granted.
- **Distribution**: signed manifests plus an allowlist; the host pins the
  plugin id, version range and content hash. Signature verification happens
  before activation. Updates are explicit, never silent.
- **Data boundary**: the host sends only the current block/note snapshot needed
  for a call, and receives sanitised HTML or a declarative patch. No cookies
  cross the boundary.
- **Timeouts and budgets**: each hook call gets a deadline and a result-size
  cap; exceeding either marks the plugin failed and unloads the frame/worker.

- **隔离**：每个插件运行在独立的跨源 iframe 或 module worker 中；宿主只通过带版本的
  request/response `postMessage` 通信，不共享对象、不传递 DOM 句柄。
- **CSP**：宿主文档保持 `script-src 'self'`（无 `unsafe-eval`、无远程脚本）、
  `object-src 'none'`、`base-uri 'none'`、`frame-ancestors 'self'`；沙箱帧使用独立
  origin 与更严格 CSP；除非授予 `network:fetch`，否则网络保持关闭。
- **分发**：清单签名 + allowlist；宿主锁定插件 id、版本范围与内容哈希，激活前验签；
  更新必须显式确认，不允许静默升级。
- **数据边界**：宿主只发送一次调用所需的块/笔记快照，只接收已消毒 HTML 或声明式补丁；
  cookie 不跨越边界。
- **超时与预算**：每次 hook 调用有截止时间与结果大小上限；超限即标记插件失败并卸载
  对应 frame/worker。

---

## 7. Failure isolation / 失败隔离

- Manifest, semver, permission and id errors reject registration before any
  registry mutation.
- A throwing `match` or `render` is skipped; other renderers and the plain
  Markdown preview still work.
- Sanitisation is the last step before DOM insertion, so a renderer cannot
  inject scripts, event handlers or remote resources.
- A throwing command returns `{ ok: false }`; a throwing activation sets
  `status: 'failed'` with an error message. Registry listeners are also invoked
  through try/catch.
- `unregister` removes all contributions even if `deactivate` throws.
- v0 cannot pre-empt an infinite loop because plugins are in-process; that is a
  reason external code is not loaded. The sandboxed host adds worker termination
  and deadlines.

- manifest、semver、权限与 id 错误会在任何注册表写入前拒绝注册。
- `match` / `render` 抛错会被跳过；其他渲染器与普通 Markdown 预览仍可用。
- 消毒是插入 DOM 前的最后一步，渲染器无法注入脚本、事件处理器或远程资源。
- 命令抛错返回 `{ ok: false }`；`activate` 抛错会把插件置为 `status: 'failed'` 并记录
  错误；注册表监听器同样通过 try/catch 调用。
- `unregister` 即使 `deactivate` 抛错也会移除全部贡献。
- v0 无法抢占死循环，因为插件在进程内运行；这正是 v0 不加载外部代码的原因。沙箱宿主
  会增加 worker 终止与超时。

---

## 8. Licensing / 许可关系

- The plugin API implementation lives in `web/**`, which `LICENSING.md` places
  in the **MIT OR Apache-2.0** product surface. The types and registry may be
  reused under those terms.
- A plugin authored by a third party is a separate work; the author chooses its
  license. Because the API surface is permissive, a plugin does not need a
  commercial licence merely to register renderers or commands.
- If a plugin embeds, links or copies **AGPL-3.0-only core** code (for example
  importing Rust core modules or shipping a derivative of them), the resulting
  work is AGPL-3.0-only. A commercial licence from the copyright holder is then
  required to avoid AGPL obligations for the core.
- Distributing a plugin together with the `lkb` binary/container that links the
  AGPL core makes the **combined distribution** AGPL-3.0-only, even though the
  plugin and web UI themselves are permissive. Include the Corresponding Source
  and notices as required by AGPL §4–§6/§13.
- The commercial licence covers the AGPL core only. It does not relicense the
  MIT/Apache product surface, and it is not needed for that surface.
- Future out-of-process/sandboxed plugins remain separate works; AGPL or BSL
  components must run out-of-process, consistent with `CONTRIBUTING.md`.
- This section is an engineering summary, not legal advice. The authoritative
  terms are `LICENSING.md`, `LICENSE`, `LICENSE-MIT` and `LICENSE-APACHE`.

- 插件 API 实现位于 `web/**`，按 `LICENSING.md` 属于 **MIT OR Apache-2.0** 产品面；
  类型与注册表可按该许可复用。
- 第三方插件是独立作品，许可由作者决定。由于 API 面是宽松许可，插件仅注册渲染器或
  命令时不需要商业许可。
- 若插件嵌入、链接或复制 **AGPL-3.0-only 核心**代码（例如引入 Rust 核心模块或其
  衍生作品），组合作品即为 AGPL-3.0-only；此时若需规避核心的 AGPL 义务，必须取得
  版权持有人的商业许可。
- 若插件与链接 AGPL 核心的 `lkb` 二进制/容器一起分发，则**组合分发物**整体为
  AGPL-3.0-only，即使插件与 Web UI 本身是宽松许可；须按 AGPL 第 4–6 条/第 13 条
  提供 Corresponding Source 与声明。
- 商业许可只覆盖 AGPL 核心，不改变 MIT/Apache 产品面的许可，该产品面也无需商业许可。
- 未来进程外/沙箱插件仍是独立作品；AGPL 或 BSL 组件必须在进程外运行，与
  `CONTRIBUTING.md` 一致。
- 本节是工程摘要，不构成法律意见。以 `LICENSING.md`、`LICENSE`、`LICENSE-MIT` 与
  `LICENSE-APACHE` 为准。

---

## 9. v0 non-goals / v0 非目标

- No remote module loading, plugin marketplace, auto-update or signature
  verification (the future design is described above).
- No arbitrary JS execution, DOM access or network access.
- No persistent plugin settings store, no cross-device sync.
- No slash-command UI or keybinding dispatch yet.
- No collaborative editing or cross-block transactions; the block editor is a
  view over one Markdown string.
- No backend change: the API is front-end only, and saved pages remain plain
  Markdown through the existing Wiki/notes endpoints.

- 不加载远程模块，不做插件市场、自动更新或签名校验（未来设计见上）。
- 不执行任意 JS，不访问 DOM 或网络。
- 没有持久化插件设置存储，也没有跨设备同步。
- 暂无 slash 命令 UI 或快捷键分发。
- 没有协作编辑或跨块事务；块编辑器只是单个 Markdown 字符串的视图。
- 不修改后端：API 仅存在于前端，保存仍通过现有 Wiki/笔记接口写入纯 Markdown。
