import http from 'node:http'
import { createReadStream, readFileSync, statSync } from 'node:fs'
import { gzipSync } from 'node:zlib'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(here, '../web/dist')
const port = Number(process.env.STATIC_PORT ?? 4173)

const types = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.json': 'application/json',
  '.webmanifest': 'application/manifest+json',
  '.ico': 'image/x-icon',
  '.png': 'image/png',
  '.woff2': 'font/woff2',
}

http
  .createServer((req, res) => {
    const rawPath = decodeURIComponent((req.url ?? '/').split('?')[0])
    let file = path.resolve(root, '.' + (rawPath === '/' ? '/index.html' : rawPath))
    if (!file.startsWith(root)) {
      res.statusCode = 403
      res.end('forbidden')
      return
    }
    try {
      if (statSync(file).isDirectory()) file = path.join(file, 'index.html')
    } catch {
      file = path.join(root, 'index.html')
    }
    const ext = path.extname(file)
    const encoding = String(req.headers['accept-encoding'] ?? '')
    const compressible = ['.html', '.js', '.css', '.svg', '.json', '.webmanifest'].includes(ext)
    res.setHeader('Content-Type', types[ext] ?? 'application/octet-stream')
    res.setHeader('Cache-Control', 'no-store')
    res.setHeader('Service-Worker-Allowed', '/')
    if (compressible && encoding.includes('gzip')) {
      const body = gzipSync(readFileSync(file), { level: 9 })
      res.setHeader('Content-Encoding', 'gzip')
      res.setHeader('Content-Length', body.length)
      res.end(body)
      return
    }
    createReadStream(file).pipe(res)
  })
  .listen(port, '127.0.0.1', () => {
    console.log(`serving ${root} on http://127.0.0.1:${port}`)
  })
