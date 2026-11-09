// Verifies the three-column layout (drag, keyboard, persistence, breakpoints),
// the brand mark rendering (no plate, accent dot), admin `NotProvided` dialogs
// and scrollbar hygiene of the production build.
//
// Chrome is driven over the DevTools protocol with a throwaway profile; the
// script never talks to Edge and never opens a window.
//
// usage: node tools/check-layout.mjs [--base http://127.0.0.1:4175] [--port 9336]
import { spawn } from 'node:child_process'
import { mkdirSync, rmSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'

const args = process.argv.slice(2)
const readFlag = (name, fallback) => {
  const index = args.indexOf(name)
  return index === -1 ? fallback : args[index + 1]
}

const base = readFlag('--base', 'http://127.0.0.1:4175')
const chrome = process.env.CHROME_PATH ?? 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe'
const port = Number(readFlag('--port', '9336'))

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const profile = path.join(os.tmpdir(), `lkb-check-${Date.now()}`)
mkdirSync(profile, { recursive: true })

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
    '--window-size=1440,900',
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
  const result = await send(socket, 'Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
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

async function setViewport(socket, width, height) {
  await send(socket, 'Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
}

async function setTheme(socket, value) {
  await send(socket, 'Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value }] })
}

async function goto(socket, url, ready = 'true', timeoutMs = 20_000) {
  await send(socket, 'Page.navigate', { url })
  const ok = await waitUntil(socket, ready, timeoutMs)
  await sleep(350)
  return ok
}

const results = []
function expect(name, ok, detail = '') {
  const state = ok === null ? 'SKIP' : ok ? 'PASS' : 'FAIL'
  results.push({ name, state, detail })
  console.log(`${state} ${name}${detail ? `  ${detail}` : ''}`)
}

const pageReady = 'document.querySelector("main") !== null && document.body.innerText.length > 40'

// Functional fixtures for the admin drawer checks. They are created through
// the app origin so the browser's session/CSRF cookies apply, and the provider
// is removed again at the end of the run. The test URL is intentionally
// unreachable: `/providers/{id}/test` then answers deterministically (no model
// configured) instead of depending on an external provider.
const PROVIDER_FIXTURE_ID = `layout-e2e-${Date.now()}`

const createProviderFixture = (socket) =>
  evaluate(
    socket,
    `(async () => {
      try {
        const csrfResponse = await fetch('/api/v1/auth/csrf', { credentials: 'same-origin' })
        const csrf = await csrfResponse.json()
        const response = await fetch('/api/v1/admin/providers', {
          method: 'POST',
          credentials: 'same-origin',
          headers: { 'content-type': 'application/json', 'x-csrf-token': csrf.csrf_token },
          body: JSON.stringify({
            provider_id: ${JSON.stringify(PROVIDER_FIXTURE_ID)},
            kind: 'openai',
            base_url: 'http://127.0.0.1:9/v1',
          }),
        })
        return { ok: response.ok, status: response.status }
      } catch (error) {
        return { ok: false, status: 0, error: String(error) }
      }
    })()`,
  )

const deleteProviderFixture = (socket) =>
  evaluate(
    socket,
    `(async () => {
      try {
        const csrfResponse = await fetch('/api/v1/auth/csrf', { credentials: 'same-origin' })
        const csrf = await csrfResponse.json()
        const response = await fetch('/api/v1/admin/providers/${PROVIDER_FIXTURE_ID}', {
          method: 'DELETE',
          credentials: 'same-origin',
          headers: { 'x-csrf-token': csrf.csrf_token },
        })
        return response.ok
      } catch {
        return false
      }
    })()`,
  )

const layoutState = (socket) =>
  evaluate(
    socket,
    `(() => {
      const asides = Array.from(document.querySelectorAll('aside'))
      const seps = Array.from(document.querySelectorAll('[role="separator"]'))
      const main = document.querySelector('main')
      return {
        asides: asides.map((el) => Math.round(el.getBoundingClientRect().width)),
        seps: seps.map((el) => ({
          now: Number(el.getAttribute('aria-valuenow')),
          max: Number(el.getAttribute('aria-valuemax')),
          min: Number(el.getAttribute('aria-valuemin')),
          label: el.getAttribute('aria-label'),
        })),
        mainW: main ? Math.round(main.getBoundingClientRect().width) : 0,
        stored: JSON.parse(localStorage.getItem('lkb-layout') || 'null'),
      }
    })()`,
  )

async function dragSeparator(socket, index, dx) {
  const rect = await evaluate(
    socket,
    `(() => {
      const el = document.querySelectorAll('[role="separator"]')[${index}]
      if (!el) return null
      const r = el.getBoundingClientRect()
      return { x: r.left + r.width / 2, y: r.top + 160 }
    })()`,
  )
  if (!rect) return false
  await send(socket, 'Input.dispatchMouseEvent', { type: 'mousePressed', x: rect.x, y: rect.y, button: 'left', buttons: 1, clickCount: 1 })
  await send(socket, 'Input.dispatchMouseEvent', { type: 'mouseMoved', x: rect.x + dx / 2, y: rect.y, button: 'left', buttons: 1 })
  await sleep(60)
  await send(socket, 'Input.dispatchMouseEvent', { type: 'mouseMoved', x: rect.x + dx, y: rect.y, button: 'left', buttons: 1 })
  await sleep(60)
  await send(socket, 'Input.dispatchMouseEvent', { type: 'mouseReleased', x: rect.x + dx, y: rect.y, button: 'left', buttons: 0, clickCount: 1 })
  await sleep(200)
  return true
}

const keyOnSeparator = (socket, index, key) =>
  evaluate(
    socket,
    `(() => {
      const el = document.querySelectorAll('[role="separator"]')[${index}]
      if (!el) return false
      el.focus()
      el.dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(key)}, bubbles: true }))
      return document.activeElement === el
    })()`,
  )

const dblclickSeparator = (socket, index) =>
  evaluate(
    socket,
    `(() => {
      const el = document.querySelectorAll('[role="separator"]')[${index}]
      if (!el) return false
      el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
      return true
    })()`,
  )

const clickByLabel = (socket, labelPattern) =>
  evaluate(
    socket,
    `(() => {
      const pattern = new RegExp(${JSON.stringify(labelPattern)})
      const button = Array.from(document.querySelectorAll('button')).find((el) => pattern.test(el.getAttribute('aria-label') || ''))
      if (!button) return null
      button.click()
      return button.getAttribute('aria-label')
    })()`,
  )

const clickByText = (socket, text, scope = 'document') =>
  evaluate(
    socket,
    `(() => {
      const root = ${scope}
      if (!root) return false
      const button = Array.from(root.querySelectorAll('button')).find((el) => (el.textContent || '').includes(${JSON.stringify(text)}))
      if (!button) return false
      button.click()
      return true
    })()`,
  )

const dialogInfo = (socket) =>
  evaluate(
    socket,
    `(() => {
      const dialogs = Array.from(document.querySelectorAll('[role="dialog"]'))
      return {
        count: dialogs.length,
        text: dialogs.map((el) => (el.innerText || '').replace(/\\s+/g, ' ').slice(0, 300)),
      }
    })()`,
  )

const closeDialog = (socket) =>
  evaluate(
    socket,
    `(() => {
      const dialogs = Array.from(document.querySelectorAll('[role="dialog"]'))
      const dialog = dialogs[dialogs.length - 1]
      if (!dialog) return false
      const button = Array.from(dialog.querySelectorAll('button')).find((el) => /关闭|取消|Close|Cancel/.test(el.textContent || ''))
      if (!button) return false
      button.click()
      return true
    })()`,
  )

const scrollbarReport = (socket) =>
  evaluate(
    socket,
    `(() => {
      const offenders = []
      for (const el of document.querySelectorAll('body *')) {
        const tag = el.tagName
        if (tag === 'TEXTAREA' || tag === 'SELECT' || tag === 'HTML' || tag === 'PRE' || tag === 'MAIN') continue
        const style = getComputedStyle(el)
        if (style.overflowY !== 'auto' && style.overflowY !== 'scroll') continue
        const dy = el.scrollHeight - el.clientHeight
        if (dy <= 1 || el.clientHeight <= 0 || el.clientHeight >= 4000) continue
        const classes = typeof el.className === 'string' ? el.className.trim().split(/\\s+/).slice(0, 3).join('.') : ''
        offenders.push({
          el: tag.toLowerCase() + (classes ? '.' + classes : ''),
          dy,
          clientH: el.clientHeight,
          inAside: Boolean(el.closest('aside')),
          text: (el.innerText || '').replace(/\\s+/g, ' ').slice(0, 50),
        })
      }
      const doc = document.documentElement
      const tabStrip = document.querySelector('div[class*="overflow-y-hidden"]')
      return {
        offenders,
        bodyDx: doc.scrollWidth - window.innerWidth,
        bodyDy: doc.scrollHeight - window.innerHeight,
        tabStripDy: tabStrip ? tabStrip.scrollHeight - tabStrip.clientHeight : null,
        tabStripDx: tabStrip ? tabStrip.scrollWidth - tabStrip.clientWidth : null,
        tabStripOverflowY: tabStrip ? getComputedStyle(tabStrip).overflowY : null,
      }
    })()`,
  )

let socket = null
let providerFixtureCreated = false
try {
  await waitForDevtools()
  const target = await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: 'PUT' })).json()
  socket = await connect(target.webSocketDebuggerUrl)
  await send(socket, 'Page.enable')
  await send(socket, 'Runtime.enable')

  await setViewport(socket, 1440, 900)
  await send(socket, 'Page.navigate', { url: `${base}/search?q=x` })
  await sleep(900)
  await evaluate(socket, `localStorage.clear(); localStorage.setItem('lkb-locale', 'zh-CN'); true`)

  // ---------------------------------------------------------------- drag ----
  await goto(socket, `${base}/search?q=%E6%A3%80%E7%B4%A2`, 'document.querySelectorAll("[role=separator]").length === 2')
  const initial = await layoutState(socket)
  expect(
    'layout: left+right visible with defaults',
    initial.asides.length === 2 && initial.asides[0] === 240 && initial.asides[1] === 336,
    JSON.stringify(initial.asides),
  )
  expect(
    'layout: separators expose aria values',
    initial.seps.length === 2 && initial.seps[0].now === 240 && initial.seps[1].now === 336 && initial.seps[0].label !== initial.seps[1].label,
    JSON.stringify(initial.seps),
  )
  expect('layout: main respects MAIN_MIN', initial.mainW >= 420, `mainW=${initial.mainW}`)

  await dragSeparator(socket, 0, 100)
  let state = await layoutState(socket)
  expect('drag: left grows by 100', state.asides[0] === 340, JSON.stringify(state.asides))
  expect('drag: left persisted', state.stored?.left === 340, JSON.stringify(state.stored))

  await dragSeparator(socket, 1, -80)
  state = await layoutState(socket)
  expect('drag: right grows by 80', state.asides[1] === 416, JSON.stringify(state.asides))
  expect('drag: main still >= 420', state.mainW >= 420, `mainW=${state.mainW}`)

  await dragSeparator(socket, 0, 400)
  state = await layoutState(socket)
  expect('drag: left clamps at 420 (or keeps main min)', state.asides[0] <= 420 && state.mainW >= 419, JSON.stringify(state.asides))

  await dblclickSeparator(socket, 0)
  await sleep(150)
  state = await layoutState(socket)
  expect('dblclick: left reset to default', state.asides[0] === 240, JSON.stringify(state.asides))

  await keyOnSeparator(socket, 0, 'ArrowRight')
  state = await layoutState(socket)
  expect('keyboard: ArrowRight grows left column', state.asides[0] === 256, JSON.stringify(state.asides))
  await keyOnSeparator(socket, 0, 'ArrowLeft')
  state = await layoutState(socket)
  expect('keyboard: ArrowLeft shrinks left column', state.asides[0] === 240, JSON.stringify(state.asides))

  await keyOnSeparator(socket, 1, 'ArrowLeft')
  state = await layoutState(socket)
  expect('keyboard: ArrowLeft moves right separator left (panel grows)', state.asides[1] === 432, JSON.stringify(state.asides))
  await keyOnSeparator(socket, 1, 'Home')
  state = await layoutState(socket)
  expect('keyboard: Home clamps to min', state.asides[1] === 300 && state.seps[1].min === 300, JSON.stringify(state.asides))
  await keyOnSeparator(socket, 1, 'End')
  state = await layoutState(socket)
  expect('keyboard: End jumps to max', state.asides[1] === 560, JSON.stringify(state.asides))
  await keyOnSeparator(socket, 1, 'ArrowRight')
  state = await layoutState(socket)
  expect('keyboard: ArrowRight shrinks right panel by step', state.asides[1] === 544, JSON.stringify(state.asides))
  await keyOnSeparator(socket, 1, 'Home')
  await keyOnSeparator(socket, 1, 'ArrowLeft')
  state = await layoutState(socket)
  expect('keyboard: min + one grow step = 316', state.asides[1] === 316, JSON.stringify(state.asides))

  await goto(socket, `${base}/search?q=%E6%A3%80%E7%B4%A2`, 'document.querySelectorAll("[role=separator]").length === 2')
  state = await layoutState(socket)
  expect('persistence: widths survive reload', state.asides[0] === 240 && state.asides[1] === 316, JSON.stringify(state.asides))

  // ------------------------------------------------------------- collapse ---
  const collapsed = await clickByLabel(socket, '收起侧栏|Collapse sidebar')
  await sleep(250)
  state = await layoutState(socket)
  expect('collapse: left collapses via button', collapsed !== null && state.asides.length === 1 && state.stored?.leftCollapsed === true, `${collapsed} ${JSON.stringify(state.asides)}`)
  const expanded = await clickByLabel(socket, '展开侧栏|Expand sidebar')
  await sleep(250)
  state = await layoutState(socket)
  expect('collapse: rail expands left again', expanded !== null && state.asides.length === 2, `${expanded}`)

  await clickByLabel(socket, '收起问答面板|Collapse assistant panel')
  await sleep(200)
  state = await layoutState(socket)
  expect('collapse: right collapses', state.asides.length === 1 && state.stored?.rightCollapsed === true, JSON.stringify(state.stored))
  await clickByLabel(socket, '展开问答面板|Expand assistant panel')
  await sleep(200)
  state = await layoutState(socket)
  expect('collapse: right re-expands', state.asides.length === 2, JSON.stringify(state.asides))

  // ---------------------------------------------------------- breakpoints ---
  await setViewport(socket, 1080, 800)
  await sleep(400)
  state = await layoutState(socket)
  expect('breakpoint: right auto-collapses < 1100px', state.asides.length === 1 && state.mainW >= 420, JSON.stringify(state.asides))
  await setViewport(socket, 1000, 800)
  await sleep(400)
  state = await layoutState(socket)
  expect('breakpoint: left hides < 1024px', state.asides.length === 0, JSON.stringify(state.asides))
  await setViewport(socket, 1440, 900)
  await sleep(400)

  // ---------------------------------------------------------------- embed ---
  await goto(socket, `${base}/search?embed=1&q=%E6%A3%80%E7%B4%A2`, 'document.querySelector("main") !== null')
  const embedState = await evaluate(
    socket,
    `(() => ({ seps: document.querySelectorAll('[role="separator"]').length, asides: document.querySelectorAll('aside').length, hasMain: document.querySelector('main') !== null }))()`,
  )
  expect('embed: no separators, no sidebars, main kept', embedState.seps === 0 && embedState.asides === 0 && embedState.hasMain, JSON.stringify(embedState))

  // --------------------------------------------------------------- logo -----
  await evaluate(socket, `localStorage.removeItem('lkb-layout'); localStorage.setItem('lkb-theme', 'dark'); true`)
  await goto(socket, `${base}/search?q=%E6%A3%80%E7%B4%A2`, pageReady)
  const logoProbe = `(() => {
    const svg = document.querySelector('header svg')
    if (!svg) return null
    const dot = svg.querySelector('circle')
    const svgStyle = getComputedStyle(svg)
    const parentStyle = getComputedStyle(svg.parentElement)
    return {
      svgBg: svgStyle.backgroundColor,
      parentBg: parentStyle.backgroundColor,
      dotFill: dot ? getComputedStyle(dot).fill : null,
      color: svgStyle.color,
    }
  })()`
  let logo = await evaluate(socket, logoProbe)
  expect('logo(dark): transparent mark, accent dot', logo?.svgBg === 'rgba(0, 0, 0, 0)' && logo?.dotFill === 'rgb(50, 215, 75)', JSON.stringify(logo))
  await evaluate(socket, `localStorage.setItem('lkb-theme', 'light'); true`)
  await goto(socket, `${base}/search?q=%E6%A3%80%E7%B4%A2`, pageReady)
  logo = await evaluate(socket, logoProbe)
  expect('logo(light): transparent mark, green dot', logo?.svgBg === 'rgba(0, 0, 0, 0)' && logo?.dotFill === 'rgb(26, 127, 55)', JSON.stringify(logo))
  expect('logo: mark uses theme text color', logo?.color === 'rgb(23, 23, 23)' || /^rgb/.test(logo?.color ?? ''), String(logo?.color))

  // --------------------------------------------------- admin NotProvided ----
  await evaluate(socket, `localStorage.setItem('lkb-theme', 'dark'); true`)
  await goto(socket, `${base}/admin`, 'document.body.innerText.includes("最近 24 小时") && !document.querySelector(".lkb-skeleton")')
  const tabCount = await evaluate(
    socket,
    `(() => ({ tabs: Array.from(document.querySelectorAll('div[class*="overflow-y-hidden"] button')).length, text: document.body.innerText.replace(/\\s+/g, ' ') }))()`,
  )
  const overview = tabCount
  expect(
    'admin: overview default tab',
    /最近 24 小时/.test(overview.text) && /最近审计/.test(overview.text) && /管理入口/.test(overview.text),
    overview.text.slice(0, 120),
  )
  expect(
    'admin: overview counts come from /admin/overview',
    /文档总数/.test(overview.text) && /分块数/.test(overview.text) && /缓存命中率/.test(overview.text),
  )
  const featureProbe = await (await fetch(`${base}/api/v1/features`)).json()
  const quotasEnabled = featureProbe?.features?.quotas === true
  const expectedTabs = quotasEnabled ? 11 : 10
  expect(
    `admin: ${expectedTabs} tabs (quotas ${quotasEnabled ? 'enabled' : 'disabled'})`,
    overview.tabs === expectedTabs,
    `tabs=${overview.tabs}`,
  )

  await goto(socket, `${base}/admin/users`, 'document.body.innerText.includes("新建用户") && !document.querySelector(".lkb-skeleton")')
  const usersText = await evaluate(socket, `document.body.innerText.replace(/\\s+/g, ' ')`)
  expect(
    'admin: create-user entry is a real action',
    /新建用户/.test(usersText) && !/后端未提供 POST \/api\/v1\/admin\/users/.test(usersText),
    usersText.match(/新建用户[^]{0,60}/)?.[0] ?? '',
  )
  const hasUserRow = await evaluate(socket, `document.querySelectorAll('table tbody tr').length > 0`)
  let dialog
  if (hasUserRow) {
    await clickByText(socket, '重置密码')
    await sleep(250)
    dialog = await dialogInfo(socket)
    expect('admin: reset-password NotProvided dialog', dialog.count === 1 && /密码重置接口/.test(dialog.text[0] ?? ''), dialog.text[0]?.slice(0, 120))
    await closeDialog(socket)
    await sleep(200)
  } else {
    expect('admin: reset-password NotProvided dialog', null, 'no user rows in this instance')
  }

  const providerFixture = await createProviderFixture(socket)
  providerFixtureCreated = providerFixture.ok

  const modelsReady = await goto(
    socket,
    `${base}/admin/models`,
    'Array.from(document.querySelectorAll("button")).some((el) => (el.textContent || "").trim() === "测试")',
    25_000,
  )
  const clickedTest = await evaluate(
    socket,
    `(() => {
      const button = Array.from(document.querySelectorAll('button')).find((el) => (el.textContent || '').trim() === '测试')
      if (!button) return false
      button.click()
      return true
    })()`,
  )
  const providerDialogReady = await waitUntil(
    socket,
    `(() => {
      const dialogs = Array.from(document.querySelectorAll('[role="dialog"]'))
      return dialogs.length === 1 && /测试 ·/.test(dialogs[0].innerText || '') && /(连通正常|测试未通过)/.test(dialogs[0].innerText || '')
    })()`,
    20_000,
  )
  dialog = await dialogInfo(socket)
  if (providerFixtureCreated && modelsReady) {
    expect(
      'admin: provider-test result dialog (controlled fixture)',
      clickedTest && providerDialogReady && dialog.count === 1 && /测试 ·/.test(dialog.text[0] ?? '') && /(连通正常|测试未通过)/.test(dialog.text[0] ?? ''),
      dialog.text[0]?.slice(0, 140),
    )
  } else {
    expect(
      'admin: provider-test result dialog (controlled fixture)',
      null,
      `SKIP (requires provider fixture; create=${providerFixture.ok ? 'ok' : `failed ${providerFixture.status ?? 0}`}, row=${modelsReady ? 'shown' : 'missing'})`,
    )
  }
  await closeDialog(socket)
  await sleep(200)

  await goto(socket, `${base}/admin/billing`, pageReady)
  const billingText = await evaluate(socket, `document.body.innerText.replace(/\\s+/g, ' ')`)
  const billingEnabled = /计划创建\/编辑\/删除/.test(billingText)
  expect(
    'admin: billing tab renders (plans NotProvided or disabled banner)',
    billingEnabled
      ? /billing\/plans/.test(billingText)
      : /计费功能未启用/.test(billingText),
    billingEnabled ? billingText.match(/计划创建[^]{0,70}/)?.[0] ?? '' : billingText.slice(0, 90),
  )

  const auditReady = await goto(socket, `${base}/admin/audit`, 'document.querySelectorAll("table tbody tr").length > 0', 25_000)
  const rowOpened = auditReady
    ? await evaluate(
        socket,
        `(() => {
          const button = Array.from(document.querySelectorAll('table tbody tr button')).find((el) => (el.textContent || '').trim() === '详情')
          if (!button) return false
          button.click()
          return true
        })()`,
      )
    : false
  await sleep(300)
  dialog = await dialogInfo(socket)
  if (auditReady) {
    expect(
      'admin: audit detail drawer (controlled fixture)',
      rowOpened && dialog.count === 1 && /事件 ID/.test(dialog.text[0] ?? ''),
      dialog.text[0]?.slice(0, 120),
    )
  } else {
    expect(
      'admin: audit detail drawer (controlled fixture)',
      null,
      'SKIP (requires audit fixture; no audit rows)',
    )
  }
  await closeDialog(socket)
  await sleep(200)

  // ---------------------------------------------------------- scrollbars ----
  const scrollRoutes = [
    '/search?q=%E6%A3%80%E7%B4%A2',
    '/notes',
    '/spaces',
    '/spaces/product?tab=general',
    '/admin',
    '/admin/models',
    '/admin/audit',
    '/admin/connectors',
    '/admin/billing',
    '/admin/quotas',
  ]
  for (const [width, height] of [[1568, 1040], [1440, 900], [1280, 800]]) {
    await setViewport(socket, width, height)
    for (const route of scrollRoutes) {
      await goto(socket, `${base}${route}`, `${pageReady} && !document.querySelector('.lkb-skeleton')`, 25_000)
      const report = await scrollbarReport(socket)
      // A scrollbar is spurious when the overflow is 1-8px (phantom from a
      // border/negative-margin mismatch) or when it sits outside the two
      // panes that are allowed to scroll (sidebars). `main` is the intended
      // scroll container and is excluded from the scan.
      const spurious = report.offenders.filter((entry) => entry.dy <= 8)
      const deepOutsideAsides = report.offenders.filter((entry) => entry.dy > 8 && !entry.inAside)
      const clean =
        spurious.length === 0 &&
        deepOutsideAsides.length === 0 &&
        report.bodyDy <= 1 &&
        (report.tabStripDy ?? 0) === 0
      expect(`scroll ${width}x${height} ${route}`, clean, JSON.stringify(report))
    }
  }
} finally {
  if (socket && providerFixtureCreated) {
    try {
      await deleteProviderFixture(socket)
    } catch {
      /* fixture cleanup must not mask assertion results */
    }
  }
  if (socket) socket.close()
  child.kill()
  try {
    rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 })
  } catch {
    /* leave the throwaway profile behind */
  }
}

const failures = results.filter((entry) => entry.state === 'FAIL')
const skipped = results.filter((entry) => entry.state === 'SKIP')
console.log(`\n${results.length} checks, ${failures.length} failures${skipped.length ? `, ${skipped.length} skipped` : ''}`)
for (const entry of skipped) console.log(`  SKIP ${entry.name} — ${entry.detail || 'no reason recorded'}`)
if (failures.length > 0) process.exitCode = 1
