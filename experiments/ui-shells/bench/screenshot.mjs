// Renders a URL in headless Edge and writes a PNG, so the shared UI can be
// checked visually without a human at the screen.
// usage: node bench/screenshot.mjs <url> <out.png> [width] [height] [light|dark]
import { spawn } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import path from 'node:path'

const [url, out, widthArg, heightArg, themeArg] = process.argv.slice(2)
if (!url || !out) {
  console.error('usage: node screenshot.mjs <url> <out.png> [width] [height] [light|dark]')
  process.exit(1)
}
const width = Number(widthArg ?? 1440)
const height = Number(heightArg ?? 900)
const theme = themeArg === 'dark' ? 'dark' : 'light'
const edge = process.env.EDGE_PATH ?? 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe'
const port = 9333
const profile = path.resolve('bench/results/edge-shot-profile')
rmSync(profile, { recursive: true, force: true })
mkdirSync(profile, { recursive: true })
mkdirSync(path.dirname(out), { recursive: true })

const child = spawn(
  edge,
  [
    '--headless=new',
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    '--no-first-run',
    '--no-default-browser-check',
    `--window-size=${width},${height}`,
    'about:blank',
  ],
  { stdio: 'ignore' },
)

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

async function waitForDevtools() {
  for (let i = 0; i < 100; i++) {
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/version`)
      if (res.ok) return await res.json()
    } catch {
      /* not up yet */
    }
    await sleep(150)
  }
  throw new Error('devtools endpoint never came up')
}

function connect(wsUrl) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(wsUrl)
    socket.addEventListener('open', () => resolve(socket), { once: true })
    socket.addEventListener('error', (event) => reject(new Error(String(event?.message ?? 'ws error'))), { once: true })
  })
}

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

try {
  await waitForDevtools()
  const target = await (await fetch(`http://127.0.0.1:${port}/json/new?${encodeURIComponent(url)}`, { method: 'PUT' })).json()
  const socket = await connect(target.webSocketDebuggerUrl)
  await send(socket, 'Page.enable')
  await send(socket, 'Runtime.enable')
  await send(socket, 'Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
  await send(socket, 'Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-color-scheme', value: theme }],
  })
  await send(socket, 'Page.navigate', { url })
  await sleep(3000)

  const shot = await send(socket, 'Page.captureScreenshot', { format: 'png' })
  writeFileSync(out, Buffer.from(shot.data, 'base64'))

  const probe = await send(socket, 'Runtime.evaluate', {
    expression: `(async () => {
      const before = document.documentElement.dataset.theme;
      const button = document.querySelector('button[aria-label*="色模式"]');
      button && button.click();
      await new Promise((resolve) => setTimeout(resolve, 120));
      const after = document.documentElement.dataset.theme;
      const stored = (() => { try { return localStorage.getItem('lkb-theme') } catch { return null } })();
      const bodyColor = getComputedStyle(document.body).color;
      const bodyBg = getComputedStyle(document.body).backgroundColor;
      const text = document.body.innerText.replace(/\\s+/g, ' ').slice(0, 160);
      return JSON.stringify({ before, after, stored, bodyColor, bodyBg, text });
    })()`,
    returnByValue: true,
    awaitPromise: true,
  })
  console.log(`wrote ${out}`)
  console.log(probe.result.value)
  socket.close()
} finally {
  child.kill()
}
