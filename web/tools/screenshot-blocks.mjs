// Block editor evidence: seeds a demo page, drives the production build in
// headless Chrome, captures Markdown/Blocks/callout/read-back screenshots and
// asserts content preservation, focus, overflow and network isolation.
//
// usage: node tools/screenshot-blocks.mjs [--base http://127.0.0.1:4173]
//        [--api http://127.0.0.1:7441] [--out screenshots]
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
const apiBase = readFlag('--api', 'http://127.0.0.1:7441')
const outDir = path.resolve(readFlag('--out', 'screenshots'))
const chrome = process.env.CHROME_PATH ?? 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe'
const port = Number(readFlag('--port', '9335'))
const slug = 'block-editor-demo'
const markdown = [
  '# Block editor demo',
  '',
  'A short paragraph with **bold**, *italic* and `inline code`.',
  '',
  '## Lists and table',
  '',
  '- First item',
  '- Second item',
  '',
  '1. Ordered one',
  '2. Ordered two',
  '',
  '| Column | Value |',
  '| --- | --- |',
  '| Rows | 2 |',
  '| Table | intact |',
  '',
  '## Code fence',
  '',
  '```ts',
  'const answer = 42',
  '',
  'console.log(answer)',
  '```',
  '',
  '> [!NOTE] Callout from a plugin',
  '> The block renderer API keeps this block as Markdown source.',
  '> Nested **Markdown** still renders.',
  '',
  '---',
  '',
  'End of the demo page.',
].join('\n')

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

async function seedPage() {
  const csrfResponse = await fetch(`${apiBase}/api/v1/auth/csrf`, { headers: { accept: 'application/json' } })
  const csrfBody = await csrfResponse.json()
  const cookieHeaders = csrfResponse.headers.getSetCookie?.() ?? []
  const cookie = cookieHeaders.map((value) => value.split(';', 1)[0]).join('; ')
  const read = await fetch(`${apiBase}/api/v1/pages/product/${slug}?lang=zh`, { headers: { accept: 'application/json' } })
  let ifMatch = '*'
  if (read.ok) {
    const page = await read.json()
    ifMatch = page.current_revision_id
  }
  const response = await fetch(`${apiBase}/api/v1/pages/product/${slug}`, {
    method: 'PUT',
    headers: {
      accept: 'application/json',
      'content-type': 'application/json',
      'if-match': ifMatch,
      'x-csrf-token': csrfBody.csrf_token,
      cookie,
    },
    body: JSON.stringify({ title: 'Block editor demo', markdown, lang: 'zh' }),
  })
  if (!response.ok) throw new Error(`seed failed: ${response.status} ${await response.text()}`)
}

const profile = path.join(os.tmpdir(), `lkb-blocks-shot-${Date.now()}`)
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
  for (let index = 0; index < 160; index += 1) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`)
      if (response.ok) return
    } catch {
      /* not ready */
    }
    await sleep(150)
  }
  throw new Error('Chrome devtools endpoint never came up')
}

function connect(url) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url)
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
    await sleep(180)
  }
}

function clickButton(text) {
  return `(() => {
    const button = Array.from(document.querySelectorAll('button')).find((candidate) => (candidate.textContent || '').trim() === ${JSON.stringify(text)})
    if (!button) return false
    button.click()
    return true
  })()`
}

const results = []
const failures = []
const requests = new Set()
const consoleErrors = []

function assert(name, condition, detail = '') {
  results.push({ name, ok: Boolean(condition), detail })
  if (!condition) failures.push(`${name}${detail ? `: ${detail}` : ''}`)
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`)
}

async function capture(socket, file) {
  const data = await send(socket, 'Page.captureScreenshot', {
    format: 'png',
    clip: { x: 0, y: 0, width: 1440, height: 900, scale: 1 },
    captureBeyondViewport: false,
  })
  const target = path.join(outDir, file)
  writeFileSync(target, Buffer.from(data.data, 'base64'))
  console.log(`wrote ${target}`)
}

try {
  await seedPage()
  await waitForDevtools()
  const target = await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: 'PUT' })).json()
  const socket = await connect(target.webSocketDebuggerUrl)
  await send(socket, 'Page.enable')
  await send(socket, 'Runtime.enable')
  await send(socket, 'Network.enable')
  await send(socket, 'Emulation.setDeviceMetricsOverride', {
    width: 1440,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  })
  socket.addEventListener('message', (event) => {
    const message = JSON.parse(event.data)
    if (message.method === 'Network.requestWillBeSent') requests.add(message.params.request.url)
    if (message.method === 'Runtime.exceptionThrown') {
      consoleErrors.push(message.params.exceptionDetails.text ?? 'exception')
    }
    if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
      consoleErrors.push(message.params.args.map((arg) => arg.value ?? arg.description).join(' '))
    }
  })

  await send(socket, 'Page.navigate', { url: `${base}/search` })
  await sleep(1200)
  await evaluate(
    socket,
    `(() => { localStorage.setItem('lkb-theme', 'light'); localStorage.setItem('lkb-locale', 'zh-CN'); localStorage.setItem('lkb-space', 'product'); return true })()`,
  )

  // 1. Default Markdown split view.
  await send(socket, 'Page.navigate', { url: `${base}/w/product/${slug}/edit?lang=zh` })
  assert('editor opens in Markdown view', await waitUntil(socket, 'document.querySelector("textarea") !== null'))
  await sleep(700)
  await capture(socket, 'block-editor-markdown-light.png')
  const markdownBefore = await evaluate(socket, 'document.querySelector("textarea").value')
  const modeButtons = await evaluate(
    socket,
    `Array.from(document.querySelectorAll('button[aria-pressed]')).map((button) => ({ label: (button.textContent || '').trim(), pressed: button.getAttribute('aria-pressed'), aria: button.getAttribute('aria-label') }))`,
  )
  assert('mode switch is exposed', modeButtons.length === 2 && modeButtons[0].label === 'Markdown' && modeButtons[1].label === '分块', JSON.stringify(modeButtons))

  // 2. Switch to Blocks: content preserved, fences/tables/callout visible.
  await evaluate(socket, clickButton('分块'))
  assert('blocks view renders', await waitUntil(socket, 'document.querySelectorAll("section[aria-label^=\\"第\\"]").length >= 8'))
  await sleep(600)
  const blockState = await evaluate(
    socket,
    `(() => {
      const sections = Array.from(document.querySelectorAll('section[aria-label^="第"]'))
      return {
        count: sections.length,
        values: sections.map((section) => section.querySelector('textarea').value),
        callouts: document.querySelectorAll('.lkb-callout').length,
        pluginBadge: Array.from(document.querySelectorAll('span')).some((node) => (node.textContent || '').includes('插件：')),
        unlabeledButtons: sections.flatMap((section) => Array.from(section.querySelectorAll('button'))).filter((button) => !button.getAttribute('aria-label')).length,
        labeledSelects: sections.every((section) => Boolean(section.querySelector('select')?.getAttribute('aria-label'))),
      }
    })()`,
  )
  assert('all blocks recovered from Markdown', blockState.values.join('\n\n') === markdownBefore, `${blockState.count} blocks`)
  assert('code fence stays one block', blockState.values.some((value) => value.startsWith('```ts') && value.includes('const answer = 42') && value.includes('console.log(answer)') && value.split('```').length === 3))
  assert('table stays one block', blockState.values.some((value) => value.startsWith('| Column | Value |')))
  assert('callout plugin renderer runs', blockState.callouts === 1, `${blockState.callouts}`)
  assert('plugin badge is visible', blockState.pluginBadge)
  assert('block toolbar buttons have aria-labels', blockState.unlabeledButtons === 0)
  assert('block type selects have aria-labels', blockState.labeledSelects)
  const overflow = await evaluate(
    socket,
    `(() => {
      const main = document.querySelector('main')
      const blocks = Array.from(document.querySelectorAll('section[aria-label^="第"]'))
      return {
        document: document.documentElement.scrollWidth - window.innerWidth,
        main: main ? main.scrollWidth - main.clientWidth : 0,
        blocks: blocks.map((block) => block.scrollWidth - block.clientWidth).filter((delta) => delta > 2),
      }
    })()`,
  )
  assert('no horizontal overflow', overflow.document <= 1 && overflow.main <= 1 && overflow.blocks.length === 0, JSON.stringify(overflow))
  await capture(socket, 'block-editor-blocks-light.png')

  // 3. Per-block add/delete/move and keyboard focus.
  const beforeOps = await evaluate(socket, `Array.from(document.querySelectorAll('section[aria-label^="第"] textarea')).map((node) => node.value)`)
  await evaluate(socket, `document.querySelector('button[aria-label="在第 1 个块下方新增"]').click()`)
  assert('add-below inserts a block', await waitUntil(socket, `document.querySelectorAll('section[aria-label^="第"]').length === ${beforeOps.length + 1}`))
  await sleep(250)
  const focusState = await evaluate(
    socket,
    `(() => {
      const sections = Array.from(document.querySelectorAll('section[aria-label^="第"]'))
      return {
        activeTag: document.activeElement?.tagName,
        inSecondBlock: sections[1]?.contains(document.activeElement) ?? false,
      }
    })()`,
  )
  assert('new block receives keyboard focus', focusState.activeTag === 'TEXTAREA' && focusState.inSecondBlock, JSON.stringify(focusState))
  await evaluate(socket, `document.querySelector('button[aria-label="删除第 2 个块"]').click()`)
  assert('delete removes the block', await waitUntil(socket, `document.querySelectorAll('section[aria-label^="第"]').length === ${beforeOps.length}`))
  await evaluate(socket, `document.querySelector('button[aria-label="下移第 1 个块"]').click()`)
  await sleep(250)
  const moved = await evaluate(socket, `Array.from(document.querySelectorAll('section[aria-label^="第"] textarea')).map((node) => node.value)`)
  assert('move down reorders Markdown blocks', moved[0] === beforeOps[1] && moved[1] === beforeOps[0])
  await evaluate(socket, `document.querySelector('button[aria-label="上移第 2 个块"]').click()`)
  await sleep(250)
  const restored = await evaluate(socket, `Array.from(document.querySelectorAll('section[aria-label^="第"] textarea')).map((node) => node.value)`)
  assert('move up restores order', restored.join('\n\n') === beforeOps.join('\n\n'))

  const dragResult = await evaluate(
    socket,
    `(() => {
      const sections = Array.from(document.querySelectorAll('section[aria-label^="第"]'))
      const handle = sections[0]?.querySelector('button[draggable="true"]')
      if (!handle || !sections[2]) return { dispatched: false }
      const data = new DataTransfer()
      handle.dispatchEvent(new DragEvent('dragstart', { bubbles: true, dataTransfer: data }))
      sections[2].dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: data }))
      sections[2].dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: data }))
      return { dispatched: true }
    })()`,
  )
  await sleep(350)
  const dragged = await evaluate(socket, `Array.from(document.querySelectorAll('section[aria-label^="第"] textarea')).map((node) => node.value)`)
  assert('drag handle reorders blocks', dragResult.dispatched && dragged[2] === beforeOps[0] && dragged[0] === beforeOps[1], JSON.stringify(dragged.slice(0, 3)))
  await evaluate(
    socket,
    `(() => {
      const sections = Array.from(document.querySelectorAll('section[aria-label^="第"]'))
      const handle = sections[2]?.querySelector('button[draggable="true"]')
      const data = new DataTransfer()
      handle.dispatchEvent(new DragEvent('dragstart', { bubbles: true, dataTransfer: data }))
      sections[0].dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: data }))
      sections[0].dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: data }))
      return true
    })()`,
  )
  await sleep(300)
  const draggedBack = await evaluate(socket, `Array.from(document.querySelectorAll('section[aria-label^="第"] textarea')).map((node) => node.value)`)
  assert('drag back restores order', draggedBack.join('\n\n') === beforeOps.join('\n\n'))

  // 4. Callout block, scrolled into view.
  await evaluate(socket, `document.querySelector('.lkb-callout')?.closest('section[aria-label^="第"]')?.scrollIntoView({ block: 'center' })`)
  await sleep(400)
  await capture(socket, 'block-editor-callout-light.png')
  const calloutBox = await evaluate(
    socket,
    `(() => { const node = document.querySelector('.lkb-callout'); if (!node) return null; const rect = node.getBoundingClientRect(); return { top: rect.top, height: rect.height, width: rect.width, viewport: { width: innerWidth, height: innerHeight } } })()`,
  )
  assert('callout is on screen in its screenshot', calloutBox !== null && calloutBox.top >= 0 && calloutBox.top + calloutBox.height <= calloutBox.viewport.height, JSON.stringify(calloutBox))

  // 5. Round-trip: edit in Blocks, save through the existing Wiki endpoint.
  const marker = 'Readback marker 2026-11-09.'
  await evaluate(
    socket,
    `(() => {
      const section = Array.from(document.querySelectorAll('section[aria-label^="第"]')).find((candidate) => candidate.querySelector('textarea').value.startsWith('A short paragraph'))
      if (!section) return false
      const textarea = section.querySelector('textarea')
      const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set
      setter.call(textarea, textarea.value + ' ' + ${JSON.stringify(marker)})
      textarea.dispatchEvent(new Event('input', { bubbles: true }))
      return true
    })()`,
  )
  await sleep(300)
  const dirtyVisible = await evaluate(socket, 'document.body.innerText.includes("有未保存的修改")')
  assert('unsaved-change badge appears after a block edit', dirtyVisible)
  await evaluate(socket, clickButton('Markdown'))
  await waitUntil(socket, 'document.querySelectorAll("textarea").length === 1')
  const textareaAfterSwitch = await evaluate(socket, 'document.querySelector("textarea").value')
  assert('Markdown view keeps block edits', textareaAfterSwitch.includes(marker))
  await evaluate(socket, clickButton('保存'))
  assert('save navigates to the reader', await waitUntil(socket, `location.pathname === '/w/product/${slug}'`, 30_000))
  assert('reader renders saved note', await waitUntil(socket, 'document.querySelector(".prose-lkb") !== null'))
  await sleep(500)
  const readback = await evaluate(socket, 'document.querySelector(".prose-lkb").innerText')
  assert('reader shows the edited marker', readback.includes(marker))
  await capture(socket, 'block-editor-readback-light.png')

  // 6. Reopen the editor and read the persisted Markdown source back.
  await send(socket, 'Page.navigate', { url: `${base}/w/product/${slug}/edit?lang=zh` })
  assert('editor reopens with the saved note', await waitUntil(socket, 'document.querySelector("textarea") !== null'))
  await sleep(500)
  const reopened = await evaluate(socket, 'document.querySelector("textarea").value')
  assert('saved Markdown round-trips through the API', reopened.includes(marker) && reopened.includes('> [!NOTE] Callout from a plugin'))

  // 7. Dark-mode blocks view.
  await evaluate(socket, `localStorage.setItem('lkb-theme', 'dark')`)
  await send(socket, 'Page.navigate', { url: `${base}/w/product/${slug}/edit?lang=zh` })
  await waitUntil(socket, 'document.querySelector("textarea") !== null')
  await evaluate(socket, clickButton('分块'))
  await waitUntil(socket, 'document.querySelectorAll("section[aria-label^=\\"第\\"]").length >= 8')
  await sleep(600)
  await evaluate(socket, `window.scrollTo(0, 0)`)
  await capture(socket, 'block-editor-blocks-dark.png')

  const external = [...requests].filter((url) => {
    try {
      const parsed = new URL(url)
      return parsed.protocol.startsWith('http') && parsed.host !== '127.0.0.1:4173'
    } catch {
      return false
    }
  })
  assert('no external network requests', external.length === 0, external.join(', '))
  assert('no console errors', consoleErrors.length === 0, consoleErrors.slice(0, 3).join(' | '))

  socket.close()
  console.log('\nresults:', JSON.stringify({ failures, screenshots: 5 }, null, 2))
} finally {
  child.kill()
  try {
    rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 })
  } catch {
    /* throwaway profile */
  }
}

if (failures.length > 0) process.exitCode = 1
