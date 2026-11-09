// Captures the production build in Chrome (headless, no visible window, no
// focus stealing) and writes 1440x900 PNGs plus per-shot assertions to stdout.
//
// usage: node tools/screenshot.mjs [--base http://127.0.0.1:4173] [--out screenshots] [--group main]
//
// Shots may override the viewport (`width`/`height`), preload state
// (`before`, e.g. a localStorage layout) and run a post-ready interaction
// (`after`, e.g. opening a drawer) before the capture.
//
// Chrome is driven over the DevTools protocol with a throwaway profile; the
// script never talks to Edge and never opens a window.
import { spawn } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'

const args = process.argv.slice(2)
const readFlag = (name, fallback) => {
  const index = args.indexOf(name)
  return index === -1 ? fallback : args[index + 1]
}

const base = readFlag('--base', 'http://127.0.0.1:4173')
const outDir = path.resolve(readFlag('--out', 'screenshots'))
const chrome = process.env.CHROME_PATH ?? 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe'
const port = Number(readFlag('--port', '9334'))
const group = readFlag('--group', 'main')
const width = 1440
const height = 900

// Shot groups: `main` runs against the solo instance (loopback, auth.mode=none);
// `login` runs against a session-mode instance where the visitor is anonymous,
// so the sign-in view is reachable instead of redirecting into the app.
const shotGroups = {
  main: [
  {
    file: 'search-dark.png',
    url: `${base}/search?q=${encodeURIComponent('检索模式')}`,
    theme: 'dark',
    ready: 'document.querySelectorAll("main a[href^=\'/w/\']").length > 0',
    label: 'search / dark',
  },
  {
    file: 'search-light.png',
    url: `${base}/search?q=${encodeURIComponent('检索模式')}`,
    theme: 'light',
    ready: 'document.querySelectorAll("main a[href^=\'/w/\']").length > 0',
    label: 'search / light',
  },
  {
    file: 'answer-dark.png',
    url: `${base}/search?q=${encodeURIComponent('没有 LLM 时回答如何生成')}`,
    theme: 'dark',
    ready: 'document.body.innerText.includes("回答")',
    label: 'streamed answer / dark',
    clickAsk: true,
  },
  {
    file: 'article-dark.png',
    url: `${base}/w/product/01-overview?lang=zh`,
    theme: 'dark',
    ready: 'document.querySelector(".prose-lkb") !== null',
    label: 'article / dark',
  },
  {
    file: 'article-light.png',
    url: `${base}/w/product/01-overview?lang=zh`,
    theme: 'light',
    ready: 'document.querySelector(".prose-lkb") !== null',
    label: 'article / light',
  },
  {
    file: 'article-summary-dark.png',
    url: `${base}/w/product/01-overview?lang=zh`,
    theme: 'dark',
    ready: 'document.body.innerText.includes("文章摘要")',
    label: 'article summary / dark',
  },
  {
    file: 'admin-models-dark.png',
    url: `${base}/admin/models`,
    theme: 'dark',
    ready: 'document.querySelectorAll("table").length > 0',
    label: 'admin models / dark',
  },
  {
    file: 'admin-models-light.png',
    url: `${base}/admin/models`,
    theme: 'light',
    ready: 'document.querySelectorAll("table").length > 0',
    label: 'admin models / light',
  },
  {
    file: 'admin-usage-dark.png',
    url: `${base}/admin/usage`,
    theme: 'dark',
    ready: 'document.body.innerText.length > 60',
    label: 'admin usage / dark',
  },
  {
    file: 'admin-audit-light.png',
    url: `${base}/admin/audit`,
    theme: 'light',
    ready: 'document.body.innerText.length > 60',
    label: 'admin audit / light',
  },
  {
    file: 'admin-overview-dark.png',
    url: `${base}/admin`,
    theme: 'dark',
    ready: 'document.body.innerText.includes("最近 24 小时") && !document.querySelector(".lkb-skeleton")',
    label: 'admin overview / dark',
  },
  {
    file: 'admin-overview-light.png',
    url: `${base}/admin`,
    theme: 'light',
    ready: 'document.body.innerText.includes("最近 24 小时") && !document.querySelector(".lkb-skeleton")',
    label: 'admin overview / light',
  },
  {
    file: 'admin-users-light.png',
    url: `${base}/admin/users`,
    theme: 'light',
    ready: 'document.body.innerText.includes("新建用户") && !document.querySelector(".lkb-skeleton")',
    label: 'admin users / light',
  },
  {
    file: 'admin-groups-light.png',
    url: `${base}/admin/groups`,
    theme: 'light',
    ready: '!document.querySelector(".lkb-skeleton") && document.body.innerText.length > 80',
    label: 'admin groups / light',
  },
  {
    file: 'admin-quotas-light.png',
    url: `${base}/admin/quotas`,
    theme: 'light',
    ready: '!document.querySelector(".lkb-skeleton") && document.body.innerText.length > 80',
    label: 'admin quotas / light',
  },
  {
    file: 'admin-audit-drawer-dark.png',
    url: `${base}/admin/audit`,
    theme: 'dark',
    ready: 'document.querySelectorAll("table tbody tr button").length > 0',
    after:
      "const button = Array.from(document.querySelectorAll('table tbody tr button')).find((el) => (el.textContent || '').trim() === '详情'); if (button) button.click();",
    label: 'admin audit drawer / dark',
  },
  {
    file: 'admin-billing-dark.png',
    url: `${base}/admin/billing`,
    theme: 'dark',
    ready: 'document.body.innerText.includes("计费功能未启用") || document.body.innerText.includes("订阅与权益")',
    label: 'admin billing / dark',
  },
  {
    file: 'admin-billing-light.png',
    url: `${base}/admin/billing`,
    theme: 'light',
    ready: 'document.body.innerText.includes("计费功能未启用") || document.body.innerText.includes("订阅与权益")',
    label: 'admin billing / light',
  },
  {
    file: 'editor-dark.png',
    url: `${base}/w/product/01-overview/edit?lang=zh`,
    theme: 'dark',
    ready: 'document.querySelector("textarea") !== null',
    label: 'editor / dark',
  },
  {
    file: 'history-light.png',
    url: `${base}/w/product/01-overview/history?lang=zh`,
    theme: 'light',
    ready: 'document.body.innerText.includes("版本历史")',
    label: 'history / light',
  },
  ],
  login: [
    {
      file: 'login-light.png',
      url: `${base}/login`,
      theme: 'light',
      ready: 'document.querySelector("input[type=password]") !== null',
      label: 'login / light',
    },
    {
      file: 'login-dark.png',
      url: `${base}/login`,
      theme: 'dark',
      ready: 'document.querySelector("input[type=password]") !== null',
      label: 'login / dark',
    },
  ],
  // Connector panel needs `features.connectors = true`, which the solo
  // profile disables by default: run this group against an instance started
  // with `[features] connectors = true` (see tools/check-layout.mjs notes).
  connectors: [
    {
      file: 'admin-connectors-dark.png',
      url: `${base}/admin/connectors`,
      theme: 'dark',
      ready: 'document.body.innerText.includes("新建连接器") && !document.querySelector(".lkb-skeleton")',
      label: 'admin connectors / dark',
    },
    {
      file: 'admin-connectors-light.png',
      url: `${base}/admin/connectors`,
      theme: 'light',
      ready: 'document.body.innerText.includes("新建连接器") && !document.querySelector(".lkb-skeleton")',
      label: 'admin connectors / light',
    },
    {
      file: 'admin-connectors-edit-dark.png',
      url: `${base}/admin/connectors`,
      theme: 'dark',
      ready: 'document.querySelectorAll("table tbody tr button").length > 0',
      after:
        "const row = document.querySelector('table tbody tr'); const button = row ? Array.from(row.querySelectorAll('button')).find((el) => (el.textContent || '').trim() === '编辑') : null; if (button) button.click();",
      label: 'admin connector edit dialog / dark',
    },
  ],
  // Layout evidence: default viewports used during the scrollbar audit, a
  // dragged three-column state, a collapsed right rail and the <1024px
  // breakpoint where the sidebar hides.
  layout: [
    {
      file: 'layout-default-1568.png',
      url: `${base}/admin/users`,
      theme: 'dark',
      width: 1568,
      height: 1040,
      before: "localStorage.removeItem('lkb-layout')",
      ready: 'document.body.innerText.includes("新建用户") && !document.querySelector(".lkb-skeleton")',
      label: 'layout default / 1568x1040 / admin users',
    },
    {
      file: 'layout-default-1280.png',
      url: `${base}/admin/models`,
      theme: 'light',
      width: 1280,
      height: 800,
      before: "localStorage.removeItem('lkb-layout')",
      ready: 'document.querySelectorAll("table").length > 0',
      label: 'layout default / 1280x800 / admin models',
    },
    {
      file: 'layout-dragged-1440.png',
      url: `${base}/search?q=${encodeURIComponent('检索模式')}`,
      theme: 'dark',
      before:
        "localStorage.setItem('lkb-layout', JSON.stringify({ left: 400, right: 520, leftCollapsed: false, rightCollapsed: false }))",
      ready: 'document.querySelectorAll("[role=separator]").length === 2',
      label: 'layout dragged (400/520) / dark',
    },
    {
      file: 'layout-right-collapsed.png',
      url: `${base}/admin/models`,
      theme: 'light',
      before:
        "localStorage.setItem('lkb-layout', JSON.stringify({ left: 240, right: 336, leftCollapsed: false, rightCollapsed: true }))",
      ready: 'document.querySelectorAll("[role=separator]").length === 1',
      label: 'layout right rail collapsed / light',
    },
    {
      file: 'layout-breakpoint-1000.png',
      url: `${base}/search?q=${encodeURIComponent('检索模式')}`,
      theme: 'dark',
      width: 1000,
      height: 800,
      before: "localStorage.removeItem('lkb-layout')",
      ready: 'document.querySelector("aside") === null && document.querySelector("main") !== null',
      label: 'layout < 1024px hides sidebar',
    },
  ],
}

const shots = shotGroups[group]
if (!shots) {
  console.error(`unknown shot group ${group}; expected one of ${Object.keys(shotGroups).join(', ')}`)
  process.exit(2)
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const profile = path.join(os.tmpdir(), `lkb-shot-${Date.now()}`)
mkdirSync(profile, { recursive: true })
mkdirSync(outDir, { recursive: true })

const child = spawn(
  chrome,
  [
    '--headless=new',
    '--disable-gpu',
    '--hide-scrollbars',
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-extensions',
    '--disable-background-networking',
    `--window-size=${width},${height}`,
    'about:blank',
  ],
  { stdio: 'ignore', windowsHide: true },
)

let messageId = 0
function send(socket, method, params = {}) {
  const id = ++messageId
  return new Promise((resolve, reject) => {
    const onMessage = (event) => {
      const message = JSON.parse(event.data)
      if (message.id !== id) return
      socket.removeEventListener('message', onMessage)
      if (message.error) reject(new Error(`${method}: ${JSON.stringify(message.error)}`))
      else resolve(message.result)
    }
    socket.addEventListener('message', onMessage)
    socket.send(JSON.stringify({ id, method, params }))
  })
}

async function waitForDevtools() {
  for (let i = 0; i < 120; i += 1) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`)
      if (response.ok) return
    } catch {
      /* not up yet */
    }
    await sleep(150)
  }
  throw new Error('Chrome devtools endpoint never came up')
}

function connect(wsUrl) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(wsUrl)
    socket.addEventListener('open', () => resolve(socket), { once: true })
    socket.addEventListener('error', (event) => reject(new Error(String(event?.message ?? 'ws error'))), { once: true })
  })
}

async function evaluate(socket, expression) {
  const result = await send(socket, 'Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
  })
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text ?? 'evaluate failed')
  return result.result.value
}

async function waitUntil(socket, expression, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    const ok = await evaluate(socket, `(() => { try { return Boolean(${expression}) } catch (error) { return false } })()`)
    if (ok) return true
    if (Date.now() > deadline) return false
    await sleep(200)
  }
}

const report = []
try {
  await waitForDevtools()
  const target = await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: 'PUT' })).json()
  const socket = await connect(target.webSocketDebuggerUrl)
  await send(socket, 'Page.enable')
  await send(socket, 'Runtime.enable')
  await send(socket, 'Emulation.setDeviceMetricsOverride', {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
  })

  // Prime the origin once so localStorage can be written per shot.
  await send(socket, 'Page.navigate', { url: `${base}/search` })
  await sleep(1200)

  for (const shot of shots) {
    const shotWidth = shot.width ?? width
    const shotHeight = shot.height ?? height
    await send(socket, 'Emulation.setDeviceMetricsOverride', {
      width: shotWidth,
      height: shotHeight,
      deviceScaleFactor: 1,
      mobile: false,
    })
    await send(socket, 'Emulation.setEmulatedMedia', {
      features: [{ name: 'prefers-color-scheme', value: shot.theme }],
    })
    await evaluate(
      socket,
      `(() => { try { localStorage.setItem('lkb-theme', ${JSON.stringify(shot.theme)}); localStorage.setItem('lkb-locale', 'zh-CN') } catch (error) { return false } return true })()`,
    )
    if (shot.before) {
      await evaluate(socket, `(() => { ${shot.before} ; return true })()`)
    }
    await send(socket, 'Page.navigate', { url: shot.url })
    const ready = await waitUntil(socket, shot.ready)
    if (shot.clickAsk) {
      await evaluate(
        socket,
        `(() => {
          const buttons = Array.from(document.querySelectorAll('button'))
          const target = buttons.find((button) => /AI|回答/.test(button.textContent || '') && !button.disabled)
          if (!target) return false
          target.click()
          return true
        })()`,
      )
      await sleep(3500)
    }
    if (shot.after) {
      await evaluate(socket, `(() => { ${shot.after} ; return true })()`)
      await sleep(500)
    }
    await sleep(650)
    const metrics = await evaluate(
      socket,
      `(() => {
        const main = document.querySelector('main')
        return {
          theme: document.documentElement.dataset.theme,
          lang: document.documentElement.lang,
          background: getComputedStyle(document.body).backgroundColor,
          title: document.title,
          headings: Array.from(document.querySelectorAll('h1, h2')).slice(0, 4).map((node) => node.textContent.trim()),
          text: (main ? main.innerText : document.body.innerText).replace(/\\s+/g, ' ').slice(0, 220),
          scrollHeight: main ? main.scrollHeight : document.body.scrollHeight,
        }
      })()`,
    )
    const shotData = await send(socket, 'Page.captureScreenshot', {
      format: 'png',
      clip: { x: 0, y: 0, width: shotWidth, height: shotHeight, scale: 1 },
      captureBeyondViewport: false,
    })
    const file = path.join(outDir, shot.file)
    writeFileSync(file, Buffer.from(shotData.data, 'base64'))
    report.push({ file: shot.file, label: shot.label, ready, ...metrics })
    console.log(`wrote ${file}${ready ? '' : '  [READY-CHECK FAILED]'}`)
    console.log(`  ${JSON.stringify(metrics)}`)
  }

  socket.close()
} finally {
  child.kill()
  // Chrome may still hold the profile for a moment after SIGTERM; a failed
  // cleanup in the OS temp dir must not fail the run.
  try {
    rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 })
  } catch {
    /* leave the throwaway profile behind */
  }
}

const failures = report.filter((entry) => !entry.ready)
console.log(`\n${report.length} screenshots, ${failures.length} readiness failures`)
if (failures.length > 0) process.exitCode = 1
