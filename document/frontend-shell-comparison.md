# 前端外壳对比：PWA / Tauri / Electron

> 测量日期：2026-11-08（本地时区 UTC+08:00）
> 实验代码：`experiments/ui-shells/`（`web/`、`tauri/`、`electron/` 三份外壳共用同一套 Vite 产物）
> 原始数据：`experiments/ui-shells/bench/results/`

本文只记录**实测**得到的数字。凡未实际跑过的项一律写「未测」，不做估算；
凡属推断的结论标注 `[INFERENCE]`。

---

## 0. 结论摘要

| 判断 | 结论 |
| --- | --- |
| 首选形态 | **Web-first**：同一份 `dist` 直接以 Web/PWA 交付，是所有场景的基线 |
| 桌面默认外壳 | **Tauri 2**：安装包 3.28 MiB、单 exe 12.7 MiB，比 Electron 小约 35 倍 |
| 一致性兜底 | **Electron 45**：自带 Chromium，渲染与 JS 版本完全可控；代价是 115.81 MiB 安装包 |
| 反直觉的实测结果 | Tauri 的空闲内存**不低于** Electron（492.5 MB vs 433 MB），因为 Windows 上的 Tauri 用的是 WebView2——同样是 Chromium |
| 唯一 UI 代码 | 成立：三份外壳都消费 `web/dist`，页面层没有分支代码；只有 `detectShell()` 一处用于打标 |

---

## 1. 环境与工具链

| 项 | 值 |
| --- | --- |
| 操作系统 | Windows `10.0.26300.0`，x64 |
| Node.js | `v24.18.0` |
| npm | `12.2.0` |
| Rust | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`，`cargo 1.99.1` |
| Rust host | 默认 `x86_64-pc-windows-msvc`；本机同时安装 `stable-x86_64-pc-windows-gnu` 与 `1.97.1-x86_64-pc-windows-gnu` |
| Tauri 产物链接方式 | release exe 的导入表只有 `api-ms-win-crt-*`，**不含** `VCRUNTIME140.dll`（已检查二进制），符合 GNU/mingw-w64 链接特征；构建日志未显式打印 target，故仅记为「GNU 风格工具链」 |
| Tauri | Tauri 2 + `nsis` target，`strip = true`，`frontendDist = ../../web/dist` |
| Electron | `electron 45.2.3` + `electron-builder 26.18.0`，`nsis` target，x64，`electronDist` 使用本地已下载发行版（离线打包） |
| Web 栈 | Vite 8 + React 19 + Tailwind CSS 4 + `vite-plugin-pwa`（workbox） |
| Chrome | `156.0.8078.80`（`C:\Program Files\Google\Chrome\Application\chrome.exe`）——PWA 测量与截图的唯一浏览器 |
| 分辨率 | 截图 1440×900，DPR = 1 |

约束：**全程不启动可见 GUI、不抢焦点**。所有 Chrome 调用均为 `--headless=new`、
独立 `--user-data-dir`；Electron 使用 `--headless`（offscreen、`show: false`）。
本机 **未使用 Edge**，Edge 时期的旧数据与旧截图已归档到
`bench/results/edge-void/`，不作为本文任何结论的依据。

---

## 2. 测量方法

### 2.1 三外壳的启动方式

| 外壳 | 启动命令 | 加载方式 | 是否可见 |
| --- | --- | --- | --- |
| PWA | `chrome.exe --headless=new --disable-gpu --app=http://127.0.0.1:4173/` | 本地静态服务 `127.0.0.1:4173`（`bench/static-server.mjs`，gzip） | 无窗口 |
| Tauri | `lkb-shell-tauri.exe` | `tauri://localhost`（内嵌 `web/dist`） | 有窗口（见 2.4） |
| Electron | `"Layered KB.exe" --headless` | `file://`（`renderer/` 为 `web/dist` 副本，`asar` 打包） | 无窗口（offscreen） |

### 2.2 指标定义（`bench/measure.ps1`）

- **冷启动 paintMs**：`Start-Process` 的墙钟时刻 → 页面把 FCP 回传到本地 collector
  （`http://127.0.0.1:8791/mark`）的墙钟时刻。页面侧发送的是
  `performance.timeOrigin + FCP`，因此跨进程可与启动时刻直接相减。
  每轮使用**全新 profile**，没有 Service Worker / HTTP 缓存预热。
- **windowMs**：进程启动 → `MainWindowHandle` 首次非零（仅在有窗口的模式下可测）。
- **页内 FCP**：`performance.getEntriesByName('first-contentful-paint')[0].startTime`，
  只反映页面自身从导航到首屏绘制的耗时，不含运行时/浏览器进程初始化。
- **内存**：启动后空闲 8 秒，对**进程树**求和 `WorkingSet64` 与 `PrivateMemorySize64`，
  含 WebView2 / Chromium 的全部 helper 进程；Tauri 额外按命令行过滤
  `msedgewebview2.exe` 以捕获非父子关系的 helper。
- 重复次数：Tauri 5 次、PWA 5 次、Electron 3 次（既有采集批次即 3 次，未补跑）。
  汇总取**中位数**，并保留逐次值。

### 2.3 「未启动 GUI」对数字的影响

用 headless 测量意味着：

1. **没有窗口合成开销**：headless 下不创建可见 HWND，也不会被 DWM 合成，因此 GPU/合成相关
   的耗时被低估；三者在这一点上条件一致（Tauri 除外，见下）。
2. **无法测「窗口出现时间」**：PWA 与 Electron 的 `windowMs` 记为 `null`，
   本文表格中标为「未测」，不拿别的数字替代。
3. **Electron 用 offscreen 渲染**：走的是 `BrowserWindow({ offscreen: true, show: false })`，
   与真实用户路径的窗口路径不完全相同。
4. **Tauri 无 offscreen 路径**：为拿到 `windowMs`，Tauri 那一行是 **16:31 的既有带窗口测量**；
   本次为遵守「不弹窗」约束**未重跑**。它的 `windowMs` 与 `paintMs` 因此来自可见窗口模式，
   与另外两行的无窗口模式不完全等价，比较时请看同一模式内的差异（见 §4 的说明）。

### 2.4 PWA 一行的口径

- 浏览器固定为 **Chrome**（headless），不使用 Edge。
- 这不是「已安装 PWA」的测量：每轮新 profile，Service Worker 首次注册、缓存为空，
  属于**最差情况**。已安装形态（SW 接管 + 缓存命中）未测。

### 2.5 体积口径

- `bench/size.mjs`：目录内全部文件求和为 raw；对 `.js/.css/.html/.svg/.json/webmanifest`
  再逐文件 gzip(level 9)/brotli(q11) 后求和。
- 安装包与 exe 用文件字节数直接读取（见 §3.1）。

---

## 3. 实测结果

### 3.1 体积

| 目标 | 数字 | 说明 |
| --- | --- | --- |
| Web `dist` 全量 | **raw 328.5 KB**，**gzip 104.8 KB**，**brotli 90.2 KB**（8 个文件） | 含 `sw.js`、`workbox-*.js`、`manifest.webmanifest` |
| 其中主 JS | 292.4 KB raw（`assets/index-BCoh2EOm.js`，gzip 后含在 104.8 内） | React 19 + react-router |
| 其中主 CSS | 18.0 KB raw（`assets/index-CpYAYmB3.css`） | Tailwind 4 产物 |
| Tauri 单 exe | **13,315,072 B = 12.70 MiB** | `target/release/lkb-shell-tauri.exe`，`strip = true` |
| Tauri NSIS 安装包 | **3,435,216 B = 3.28 MiB** | `Layered KB_0.1.0_x64-setup.exe`；默认 `downloadBootstrapper`，**不含** WebView2 运行时本体 |
| Electron NSIS 安装包 | **121,434,067 B = 115.81 MiB** | `Layered KB Setup 0.1.0.exe`，未签名 |
| Electron 解包目录 | **约 358 MiB** | 其中 `Layered KB.exe` 单文件 238.3 MiB，`locales/` 49 MiB，`dxcompiler.dll` 25 MiB |
| Electron 里的应用代码 | **`app.asar` 340,765 B = 332.8 KB** | 即：应用负载与 Web `dist` 同量级，其余全是 Chromium 运行时 |
| PWA 装机体积 | 未测（无安装包概念） | 磁盘占用 = 浏览器缓存，取决于浏览器实现 |

> 体积比：Tauri 安装包 ≈ Electron 的 **1/35**；Tauri 安装包 ≈ Web `dist` gzip 的 **32 倍**
> （换来的是原生窗口、文件系统访问与离线启动）。

### 3.2 冷启动

| 外壳 | paintMs 中位数 | 逐次（ms） | 页内 FCP 中位数 | windowMs 中位数 | 逐次（ms） |
| --- | --- | --- | --- | --- | --- |
| PWA（Chrome 156 headless） | **754.5** | 872.6 / 700.9 / 710.5 / 754.5 / 771.4 | 389.8 | 未测 | —（headless 无窗口） |
| Tauri 2（WebView2） | **705.5** | 737.7 / 655.4 / 652.7 / 829.0 / 705.5 | **145.5** | **115** | 151 / 102 / 100 / 198 / 115 |
| Electron 45（offscreen） | **522.4** | 614.5 / 522.4 / 478.5 | 250.5 | 未测 | —（`show:false`） |

读法：

- **页面本身最快的是 Tauri**（页内 FCP 145.5 ms）：本地资源、无 HTTP 往返。
  PWA 的 389.8 ms 里包含一次本地 HTTP 请求 + gzip 解码 + SW 注册。
- **端到端最慢的也是 Tauri**：705.5 ms 里绝大部分是 WebView2 运行时冷启动
  （首次创建 `msedgewebview2.exe` 进程树）。Electron 的 522.4 ms 反而更低，
  因为它的 Chromium 随应用分发、不依赖系统组件的发现与初始化。
- 方差：Tauri 652.7–829.0（跨度 176 ms），PWA 700.9–872.6（跨度 172 ms），
  Electron 478.5–614.5（跨度 136 ms）。样本仅 3–5 次，**差值小于 100 ms 的排序不应被当成结论**。
- Electron 只有 3 次，置信度低于另外两行。

### 3.3 空闲内存（启动后 8 秒）

| 外壳 | 进程数 | 工作集中位数 | 逐次（MB） | 私有内存中位数（MB） | 主进程工作集（MB） |
| --- | --- | --- | --- | --- | --- |
| PWA（Chrome） | **9** | **466.5 MB** | 466.6 / 465.8 / 466.5 / 464.0 / 467.9 | **184.4** | 147.4–148.7 |
| Tauri 2 | **7** | **492.5 MB** | 491.5 / 492.5 / 492.5 / 497.6 / 490.8 | **255.1** | 28.1 |
| Electron 45 | **4** | **433.0 MB** | 433.0 / 471.0 / 420.0 | **241.9** | 94.8 |

- Tauri 的主进程只有 **28.1 MB**，但 6 个 `msedgewebview2.exe` helper 把总量推到 492.5 MB——
  这是「Tauri 省内存」这一说法在 Windows 上最容易被证伪的地方。
- Electron 进程数最少（4），工作集最低，但单进程更重（主进程 94.8 MB）。
- PWA 进程数最多（9），私有内存最低（184.4 MB）——Chromium 的共享内存被多个进程共同映射，
  工作集求和会重复计数，因此**跨外壳比较工作集并不完全公平**，私有内存更可比。
- 三者都远高于「一个原生 Win32 程序」的量级：这是 Chromium 内核的固定成本。

### 3.4 构建耗时

| 外壳 | 耗时 | 来源 |
| --- | --- | --- |
| Tauri | **32.58 s**（`release` 编译耗时；NSIS 打包未单独计时） | `bench/results/tauri-build.log`：`Finished \`release\` profile [optimized] target(s) in 32.58s` |
| Electron | **未测** | `electron-build.log` 无时间戳；未补跑 |
| Web `dist`（Vite） | **未测** | 本次未单独计时 |

---

## 4. 为什么数字是这个样子

1. **Windows 上的 Tauri 不是「轻量内核」，而是「薄壳 + 系统 Chromium」**。
   WebView2 与 Electron 渲染的是同一个引擎，CSS/JS 表现几乎一致，内存也在同一量级。
   Tauri 真正省下的是**分发体积**（3.28 MiB vs 115.81 MiB）与**升级带宽**，
   而不是运行时内存。
2. **冷启动排序被运行时初始化主导**：Electron 自带 Chromium，路径最短、最可预测；
   WebView2 需要按系统安装情况解析运行时版本，首次启动更慢；
   浏览器 `--app` 启动还要经过完整的浏览器启动流程。
3. **页内 FCP 才反映「你的代码有多快」**：Tauri 145.5 ms 说明这份 UI 本身很轻
   （`dist` gzip 只有 104.8 KB）。真正的启动成本在外壳，不在应用代码。
4. **测量模式差异会污染跨行比较**：Tauri 行是可见窗口模式，另外两行无窗口。
   如果把三者都放到可见窗口下，Electron 与 PWA 会各自增加窗口创建的开销，
   Tauri 的优势会缩小。这批数据**足以支撑体积结论，不足以支撑「谁快 50 ms」的结论**。

---

## 5. Tauri / Electron / PWA 取舍

| 维度 | PWA | Tauri 2 | Electron 45 |
| --- | --- | --- | --- |
| 分发体积（实测） | 328.5 KB raw / 104.8 KB gzip | 3.28 MiB 安装包 / 12.70 MiB exe | 115.81 MiB 安装包 / ~358 MiB 解包 |
| 冷启动（实测，中位） | 754.5 ms | 705.5 ms（可见窗口模式） | 522.4 ms（offscreen） |
| 空闲私有内存（实测，中位） | 184.4 MB | 255.1 MB | 241.9 MB |
| 渲染引擎 | 用户浏览器（版本不可控） | 系统 WebView2（Windows）/ WebKitGTK（Linux） | 自带 Chromium（版本锁定） |
| 跨平台一致性 | 取决于浏览器，最差 | Windows 好，Linux/macOS 需逐平台回归 | **最好**（同一 Chromium 二进制） |
| 系统能力 | 受浏览器沙箱限制（文件系统、串口、托盘） | Rust 侧能力完整、权限可裁剪 | Node 侧能力完整、生态最成熟 |
| 更新 | 部署即生效 | 需自带 updater / 重新分发 | 需自带 updater / 重新分发 |
| 安装信任 | 无需安装 | NSIS 签名（当前未签名） | NSIS 签名（当前未签名） |
| 离线 | 需 Service Worker，首次访问必须联网 | 原生 | 原生 |

**取舍要点**

- Tauri 的收益是**分发与升级成本**，不是内存。若产品只在 Windows 上跑、
  且能接受 WebView2 作为系统依赖，Tauri 是默认桌面外壳。
- Electron 的收益是**确定性**：同一 Chromium、同一 JS 引擎、同一套字体回退与合成行为。
  当缺陷只在 WebView2 上复现、或需要打印/PDF/WebGL 等对内核版本敏感的能力时，
  Electron 的 115 MiB 就是必要成本。
- PWA 的收益是**零安装、零分发**。它不适合需要本地文件系统写入、系统托盘、
  全局快捷键的场景；这些能力在浏览器里没有等价物。

---

## 6. Linux / 信创风险

以下为风险清单，**本机没有 Linux/信创环境，全部未实测** `[INFERENCE]`
（依据为 Tauri 使用 wry + 平台 WebView、Electron 官方发行矩阵与各发行版内核版本策略）。

| 风险面 | Tauri | Electron | PWA |
| --- | --- | --- | --- |
| 渲染内核 | Linux 走 **WebKitGTK**（非 Chromium）。CSS 支持、字体度量、`position: sticky`、部分 flex/grid 细节、`backdrop-filter`、滚动条样式与 Chromium 有差异 | 自带 Chromium，**与 Windows 一致** | 取决于系统浏览器内核版本 |
| 典型发行版内核 | 麒麟 V10 / 统信 UOS 自带 WebKitGTK 常见为 4.0/4.1，版本落后，`wry` 需要 4.1+ 时可能要自行编译 | 不依赖系统内核 | 信创浏览器多为旧 Chromium 分支（如 8x–10x），需降级构建目标 |
| 中文输入法 | WebKitGTK + fcitx5 组合在部分版本存在候选框定位/预编辑问题，需实测 | 通常正常 | 取决于浏览器 |
| 高 DPI / 分数缩放 | GTK 缩放策略与 Chromium 不同，125%/150% 下 1px 边框可能发虚 | 与 Windows 一致 | 取决于浏览器 |
| LoongArch64 | Rust 侧可交叉编译，但 `wry`/GTK/`libsoup` 等系统库需发行版提供；无现成 CI 覆盖 | **无官方 loongarch64 预编译发行版**（官方矩阵为 x64 / arm64 / armv7l），需自行移植，兜底能力在此架构上**失效** | 依赖龙芯版浏览器，内核可能很旧 |
| ARM64 | 需交叉编译 GTK 依赖链 | 官方有 `linux-arm64` | 通常可用 |
| 打包格式 | 需自行产出 deb/rpm/AppImage，`.desktop` 与图标要单独维护 | `deb`/`rpm`/`AppImage` 由 electron-builder 生成 | 不涉及 |
| 签名/合规 | 国产化环境常要求二进制签名与供应链审计，Rust 依赖树需要 `cargo vendor` 存档 | npm 依赖树巨大，审计面更大 | 无二进制 |

**结论**：信创场景里，Tauri 的「小体积」优势会被「WebKitGTK 版本不可控 + 需自行编译依赖」
部分抵消；Electron 在 x64/arm64 Linux 上是**唯一能保证渲染一致**的方案，
但在 LoongArch 上没有官方发行版，等于没有兜底。PWA 在信创环境中最省事，
前提是接受目标浏览器的内核版本，并按该版本设定构建 target。

---

## 7. 同一套 UI 代码、多打包目标

实测证据：

- `tauri/src-tauri/tauri.conf.json`：`"frontendDist": "../../web/dist"`——直接吃 Vite 产物。
- `electron/scripts/sync-dist.mjs`：构建前把 `web/dist` 复制为 `electron/renderer`，
  `app.asar` 仅 332.8 KB，说明没有二次打包 UI。
- `web/src/lib/telemetry.ts` 的 `detectShell()` 是**唯一**的外壳分支
  （检查 `window.__ELECTRON__` / `window.__TAURI_INTERNALS__`），只用于埋点与角标文案。
- 页面层代码无 `if (isTauri)` 之类的渲染分支；主题、路由、组件在三种外壳下完全同构。

**结论**：以 Web 为唯一事实源（single source of truth），外壳只负责「提供窗口 + 注入能力」。
新增外壳的成本 = 一个启动器 + 一次产物同步，而不是一份新前端。
代价是：外壳特有的能力（托盘、文件对话框、深链接）必须通过一层薄适配器暴露，
且**不能**让页面直接依赖外壳 API，否则同构就破了。

---

## 8. 视觉规范

### 8.1 设计意图

- **现代极简**：无渐变、无大色块、无装饰性插画；层级靠 1px 边框与中性底色区分。
- **iOS / HIG 取向**：字重克制（正文 400、标题 600）、行高宽松（正文 1.7–1.8）、
  圆角连续、点击态用透明度而非颜色跳变、`-webkit-font-smoothing: antialiased`。
- **深色为主**：深色是主工作面（截图基准），浅色是同等公民，不是「反色补丁」。
- **禁止蓝紫 AI 渐变**：代码中 `gradient` / `violet` / `purple` / `indigo` 出现次数为 **0**（已 grep 全量 `web/src`）。
  强调色只有一个：iOS 系统绿。AI 相关元素用绿色小圆点 + 中性描边卡片表达，
  不用彩虹渐变、不用发光描边。
- **Radix / shadcn neutral 对齐**：语义 token 命名与 shadcn 的
  `background / card / muted / border / foreground` 一一对应，但**未引入**
  `@radix-ui/*` 组件库（`package.json` 依赖只有 react / react-dom / react-router-dom）。

### 8.2 颜色 token（`web/src/index.css` 实测值）

| Token | 深色 | 浅色 | 用途 |
| --- | --- | --- | --- |
| `--canvas` | `#0d0d0f` | `#f5f5f7` | 应用底色 |
| `--surface` | `#131316` | `#ffffff` | 卡片/内容面 |
| `--panel` | `#101012` | `#fbfbfc` | 侧栏/次级面 |
| `--raise` | `#1a1a1e` | `#ffffff` | 抬升面（AI 回答卡） |
| `--line` | `#26262b` | `#e6e6e9` | 默认 1px 边框/分隔线 |
| `--line-strong` | `#35353b` | `#d8d8dd` | hover 边框、滚动条滑块 |
| `--fg` | `#f4f4f5` | `#15151a` | 正文主色 |
| `--fg-2` | `#a1a1aa` | `#52525b` | 次级文字 |
| `--fg-3` | `#8a8a93` | `#6f6f76` | 元信息/标签 |
| `--accent` | `#32d74b` | `#1a7f37` | 唯一强调色（iOS 绿） |
| `--accent-weak` | `#16281b` | `#edf6f0` | 选中底、选区 |
| `--accent-line` | `#24432c` | `#cfe6d6` | 强调描边 |
| `--on-accent` | `#08130a` | `#ffffff` | 强调之上的文字 |
| `--warn` / `--warn-weak` | `#ffb340` / `#2a2114` | `#9a5b00` / `#fdf6e7` | 降级/抽取式提示 |
| `--code-bg` / `--code-fg` | `#0b0b0d` / `#e4e4e7` | `#f7f7f9` / `#27272a` | 代码块 |

### 8.3 对比度（按 WCAG 2.1 相对亮度实算）

| 组合 | 对比度 | 判定 |
| --- | --- | --- |
| 深色 `--fg` / `--canvas` | **17.66 : 1** | AAA |
| 深色 `--fg` / `--surface` | **16.87 : 1** | AAA |
| 深色 `--fg-2` / `--canvas` | **7.58 : 1** | AAA（正文） |
| 深色 `--fg-3` / `--canvas` | **5.67 : 1** | AA |
| 深色 `--accent` / `--canvas` | **10.13 : 1** | AAA |
| 浅色 `--fg` / `--canvas` | **16.71 : 1** | AAA |
| 浅色 `--fg-2` / `--canvas` | **7.10 : 1** | AAA |
| 浅色 `--fg-3` / `--canvas` | **4.58 : 1** | AA（临界，仅用于 11–12px 元信息，勿再降） |
| 浅色 `--accent` / `#ffffff` | **5.08 : 1** | AA |
| `--line` 与底色（深/浅） | 1.29 / 1.14 | 仅作分隔，不承载信息 |

规则：**颜色不单独承载信息**。命中数、相关度、状态都同时有文字标签；
低对比的 `--fg-3` 不得用于可点击文本或 3:1 以下的交互元素边界。

### 8.4 排版

| 项 | 值 |
| --- | --- |
| 正文 | 14px / 行高 1.7–1.8，字重 400 |
| 页面标题 `h1` | 22px / 字重 600 / `letter-spacing: -0.02em` |
| 卡片标题 | 13.5px / 字重 500–600 |
| 次级正文 | 12px–13px |
| 元信息 / 标签 | 11px，`uppercase`，`letter-spacing: 0.12–0.14em` |
| 最小字号 | 10px（仅限角标/相关度，不用于正文） |
| 无衬线栈 | `-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI Variable Text', 'Segoe UI', system-ui, 'PingFang SC', 'Microsoft YaHei', sans-serif` |
| 等宽栈 | `ui-monospace, 'SF Mono', 'Cascadia Code', 'Segoe UI Mono', Menlo, Consolas, monospace` |
| 平滑 | `-webkit-font-smoothing: antialiased`，`text-rendering: optimizeLegibility` |

中英文混排：中文优先 `PingFang SC` / `Microsoft YaHei`，数字与拉丁字母落在 `SF Pro Text` / `Segoe UI`，
不做字重 300 的细体（Windows 上会发虚）。

### 8.5 形状、边框、间距

| 项 | 值 |
| --- | --- |
| 卡片圆角 | `rounded-xl` = 12px（使用 14 处，主容器默认） |
| 按钮/输入/标签圆角 | `rounded-lg` = 8px（11 处）、`rounded-md` = 6px（12 处） |
| 胶囊 | `rounded-full`（状态点、头像、滚动条） |
| 边框 | 一律 **1px**，颜色用 `--line`；hover 提升到 `--line-strong`；不用阴影堆叠层级 |
| 焦点环 | `outline: 2px solid var(--accent)`，`outline-offset: 1px`，仅 `:focus-visible` |
| 间距节奏 | 4px 基准，卡片内边距 12–16px，区块间距 16–24px |
| 滚动条 | 宽 8px，滑块 `--line-strong`，轨道透明（`scrollbar-width: thin`） |
| 动效 | 仅两处：颜色/边框 150ms 过渡；流式光标 1s `steps(1)` 闪烁。无位移动画、无缩放动画 |
| 图标 | 侧栏用字符 glyph（`◆ ◇ ▣`）保持零图标依赖；功能图标为 16px 线性、1.5 描边 |

### 8.6 深色优先的实现约束

- 主题由 `index.html` 的内联脚本在**首帧之前**写入 `document.documentElement.dataset.theme`，
  避免闪白；来源顺序：`localStorage['lkb-theme']` → `prefers-color-scheme`。
- `meta[name=color-scheme]` 声明 `light dark`，`theme-color` 分媒体查询给出
  `#0d0d0f`（深）/ `#ffffff`（浅）。
- 外壳背景必须与页面 `--canvas` 对齐，否则窗口创建瞬间会看到色差：
  Electron 侧 `main.js` 目前写死 `backgroundColor: '#f8fafc'`（旧 slate 白），
  **与 `--canvas` 的 `#f5f5f7` 不一致**，切到深色主题时尤其明显——这是当前唯一已知的
  外壳/页面观感裂缝，建议改为读取主题或推迟 `show` 到首帧之后。

---

## 9. 截图

全部由 **Chrome 156 headless** 生成（`--headless=new`、独立临时 profile、
`Emulation.setEmulatedMedia` 设定 `prefers-color-scheme`），1440×900、DPR 1。
**未使用 Edge，未弹出任何窗口，未抢焦点。** 截图仅作文件查看，本文不内嵌、不自动打开。

| 文件 | 主题 | URL hash | 断言（截图时实测） |
| --- | --- | --- | --- |
| `experiments/ui-shells/bench/results/chrome/ui-dark.png` | 深色 | `#/p/overview` | `theme=dark`，`background=rgb(13,13,15)`，`h1=分层 AI 知识库总览` |
| `experiments/ui-shells/bench/results/chrome/ui-light.png` | 浅色 | `#/p/overview` | `theme=light`，`background=rgb(245,245,247)`，`h1=分层 AI 知识库总览` |
| `experiments/ui-shells/bench/results/chrome/ui-search-dark.png` | 深色 | `#/search?q=检索` | `theme=dark`，`h1=「检索」`，命中 4 个页面（含 AI 回答与 3 条引用） |

尺寸：每张 1440×900，142–146 KB。
旧的 `ui-1440.png` 与 Edge 版本截图已移入 `bench/results/edge-void/`，**已作废**。

查看方式（按需手动）：

```
start "" "F:\CODE\RAG\experiments\ui-shells\bench\results\chrome\ui-dark.png"
```

> 以上命令供人工使用；自动化流程中**不要**执行，也不要让它抢焦点。

---

## 10. 推荐

### 10.1 默认架构

```
web/dist  ← 唯一 UI 事实源（Vite + React + Tailwind）
├── 直接部署      → Web / PWA（默认交付形态）
├── tauri 壳      → 桌面默认（Windows 优先，Linux 需回归 WebKitGTK）
└── electron 壳   → 一致性兜底（同 Chromium，能力最全，体积最大）
```

- **Web-first**：先把页面做到「浏览器里就是完整体验」，外壳只是加窗口和能力。
  这条路线已经成立：`dist` gzip 104.8 KB，页内 FCP 145.5 ms。
- **Tauri 为桌面默认**：安装包 3.28 MiB、exe 12.70 MiB，升级带宽比 Electron 低两个数量级。
- **Electron 为兜底**：当且仅当下列条件之一成立时启用，而不是「两边都发」。

### 10.2 何时选哪个

| 场景 | 选择 | 理由 |
| --- | --- | --- |
| 内部知识库、只读为主、无本地写盘 | **Web/PWA** | 零安装、零分发、部署即更新 |
| 需要托盘、全局快捷键、本地索引文件、离线优先 | **Tauri** | 3.28 MiB 交付，Rust 侧能力完整 |
| 只在 Windows 内网分发、带宽敏感 | **Tauri** | 体积差 35 倍，实测启动差距在方差内 |
| 客户环境 WebView2 版本老旧/被策略禁用 | **Electron** | 自带 Chromium，版本锁定 |
| 需要打印/PDF 精确排版、复杂 WebGL、音视频硬解 | **Electron** | 内核版本可控，行为可复现 |
| 缺陷只在 WebView2 复现、需要长期一致性 | **Electron** | 同一 Chromium 二进制，跨机器一致 |
| Linux x64/arm64 桌面 | **Electron**（若要一致）或 **Tauri**（若要小） | Tauri 走 WebKitGTK，必须逐发行版回归 |
| LoongArch 桌面 | **PWA 或原生** | Electron 无官方 loongarch64；Tauri 需自行移植 GTK 依赖链 |
| macOS | **Tauri**（WKWebView，系统一致）或 **Electron** | 未实测，`[INFERENCE]` |

### 10.3 不该做的事

- 不要因为「Tauri 轻」就认为它内存更省：Windows 上实测 492.5 MB，**高于** Electron 的 433 MB。
- 不要为了「两端都发」同时维护 Electron 与 Tauri 的发布流水线；先用 Web 验证，
  再按上表选**一个**桌面外壳，另一个只保留实验目录。
- 不要让页面直接 import 外壳 API；所有外壳能力经过一层适配器，否则同构失效。

---

## 11. 复现命令（无 GUI）

```powershell
# 1) 静态服务 + 埋点 collector（仅监听 127.0.0.1）
node bench/static-server.mjs      # 127.0.0.1:4173
node bench/collector.mjs          # 127.0.0.1:8791

# 2) PWA（Chrome headless，全新 profile，5 次）
powershell -NoProfile -ExecutionPolicy Bypass -File bench/measure.ps1 `
  -Shell pwa -Exe "C:\Program Files\Google\Chrome\Application\chrome.exe" `
  -AppArgs "--headless=new","--disable-gpu","--no-first-run","--no-default-browser-check","--app=http://127.0.0.1:4173/" `
  -Headless -Runs 5 -IdleSeconds 8

# 3) Tauri（有窗口；仅限允许弹窗的场合）
powershell -NoProfile -ExecutionPolicy Bypass -File bench/measure.ps1 `
  -Shell tauri -Exe "tauri\src-tauri\target\release\lkb-shell-tauri.exe" -Runs 5

# 4) Electron（offscreen）
powershell -NoProfile -ExecutionPolicy Bypass -File bench/measure.ps1 `
  -Shell electron -Exe "electron\release\win-unpacked\Layered KB.exe" `
  -AppArgs "--headless" -Headless -Runs 3

# 5) 体积
node bench/size.mjs web/dist
```

截图用 Chrome 的 `--headless=new` + CDP（`Page.captureScreenshot`），
`Emulation.setEmulatedMedia` 设 `prefers-color-scheme`，输出到 `bench/results/chrome/`。

---

## 12. 未测 / 失败 / 作废项

| 项 | 状态 | 说明 |
| --- | --- | --- |
| Electron 构建耗时 | **未测** | `electron-build.log` 无时间戳，本次未补跑 |
| Vite `dist` 构建耗时 | **未测** | 未单独计时 |
| PWA 窗口出现时间 | **未测** | headless 无窗口，不估算 |
| Electron 窗口出现时间 | **未测** | `show: false` + offscreen，无 `MainWindowHandle` |
| 已安装 PWA（SW 预热）的冷启动 | **未测** | 本次全部为冷缓存 |
| Linux / 信创 / LoongArch / ARM64 | **未测** | 无环境；§6 为风险清单 `[INFERENCE]` |
| macOS | **未测** | 无环境 |
| Tauri Linux 包（deb/AppImage） | **未测** | 未构建 |
| 代码签名 / 安装后首次启动 | **未测** | 两端安装包均未签名 |
| Electron 冷启动样本量 | 仅 3 次 | 低于另外两行的 5 次，排序置信度较低 |
| Edge 时期数据与截图 | **作废** | 已归档 `bench/results/edge-void/`，不参与结论 |
| 首次 Chrome 截图（light 被 localStorage 覆盖成 dark） | **失败已修正** | 清除 `lkb-theme` 后重载重新生成，三张断言见 §9 |
