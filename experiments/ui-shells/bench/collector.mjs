import http from 'node:http'
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const resultsDir = path.join(here, 'results')
mkdirSync(resultsDir, { recursive: true })

const port = Number(process.env.MARK_PORT ?? 8791)
const marksFile = path.join(resultsDir, 'marks.jsonl')

http
  .createServer((req, res) => {
    res.setHeader('Access-Control-Allow-Origin', '*')
    res.setHeader('Cache-Control', 'no-store')
    const url = new URL(req.url ?? '/', 'http://127.0.0.1')
    if (url.pathname === '/mark') {
      const record = {
        shell: url.searchParams.get('shell') ?? 'unknown',
        fcp: Number(url.searchParams.get('fcp') ?? 0),
        t: Number(url.searchParams.get('t') ?? 0),
        bytes: Number(url.searchParams.get('bytes') ?? 0),
        received: Date.now(),
      }
      appendFileSync(marksFile, JSON.stringify(record) + '\n')
      // Per-shell "latest" file: one atomic-ish overwrite, so the measurement
      // script never has to count lines while another process appends.
      writeFileSync(path.join(resultsDir, `latest-${record.shell}.json`), JSON.stringify(record))
      res.end('ok')
      return
    }
    res.statusCode = 404
    res.end('not found')
  })
  .listen(port, '127.0.0.1', () => {
    console.log(`mark collector listening on http://127.0.0.1:${port} -> ${marksFile}`)
  })
